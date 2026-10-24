//! `fs.watch`: the kernel tells the app that files under a directory changed, and the app is not
//! woken for anything else. On a Mac that is FSEvents, on Linux inotify; neither is polled.
//!
//! A change is a path and what became of it (`created`, `modified`, `removed` for a path that is
//! gone, `renamed` for one that arrived by a move, or `other` when the system says something this
//! does not name), and whether it is a directory. What the system reports at about the same time
//! comes as one list: a save is often several writes. The kinds are what the system saw, and a file
//! that was created and written in the same moment may be told as `created` alone; an app that
//! wants to know what is there now looks at the file.

use std::io;
use std::path::{Path, PathBuf};

/// One change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The path, absolute, with the links of the watched directory resolved.
    pub path: PathBuf,
    pub kind: &'static str,
    pub dir: bool,
}

/// A directory being watched: dropping it stops the watch.
pub struct Watcher {
    /// Dropping it stops the watch.
    _inner: imp::Watcher,
}

/// Starts watching `root` (the directory and everything under it when `recursive`); `tell` is
/// called, on a thread of its own, with the changes.
pub fn start(
    root: &Path,
    recursive: bool,
    tell: impl Fn(Vec<Change>) + Send + 'static,
) -> io::Result<Watcher> {
    let root = root.canonicalize()?;
    if !root.is_dir() {
        return Err(io::Error::other(format!(
            "{} is not a directory",
            root.display()
        )));
    }
    Ok(Watcher {
        _inner: imp::start(root, recursive, Box::new(tell))?,
    })
}

type Tell = Box<dyn Fn(Vec<Change>) + Send>;

// ---- macOS: FSEvents ------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod imp {
    #![allow(unsafe_code, non_upper_case_globals, clippy::too_many_arguments)]

    use std::cell::Cell;
    use std::ffi::{CStr, CString, c_char, c_void};
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
    use std::sync::mpsc::channel;
    use std::thread::JoinHandle;

    use super::{Change, Tell};

    type CFIndex = isize;

    #[repr(C)]
    struct CFArrayCallBacks {
        version: CFIndex,
        retain: *const c_void,
        release: *const c_void,
        copy_description: *const c_void,
        equal: *const c_void,
    }

    #[repr(C)]
    struct StreamContext {
        version: CFIndex,
        info: *mut c_void,
        retain: *const c_void,
        release: *const c_void,
        copy_description: *const c_void,
    }

    type Callback =
        extern "C" fn(*const c_void, *mut c_void, usize, *mut c_void, *const u32, *const u64);

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFAllocatorDefault: *const c_void;
        static kCFTypeArrayCallBacks: CFArrayCallBacks;
        static kCFRunLoopDefaultMode: *const c_void;
        fn CFStringCreateWithCString(
            allocator: *const c_void,
            text: *const c_char,
            encoding: u32,
        ) -> *const c_void;
        fn CFArrayCreate(
            allocator: *const c_void,
            values: *const *const c_void,
            count: CFIndex,
            callbacks: *const CFArrayCallBacks,
        ) -> *const c_void;
        fn CFRelease(object: *const c_void);
        fn CFRunLoopGetCurrent() -> *mut c_void;
        fn CFRunLoopRun();
        fn CFRunLoopStop(run_loop: *mut c_void);
        fn CFAbsoluteTimeGetCurrent() -> f64;
        fn CFRunLoopTimerCreate(
            allocator: *const c_void,
            fire_date: f64,
            interval: f64,
            flags: u64,
            order: CFIndex,
            callout: extern "C" fn(*mut c_void, *mut c_void),
            context: *const StreamContext,
        ) -> *mut c_void;
        fn CFRunLoopAddTimer(run_loop: *mut c_void, timer: *mut c_void, mode: *const c_void);
        fn CFRunLoopTimerInvalidate(timer: *mut c_void);
    }

    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn FSEventStreamCreate(
            allocator: *const c_void,
            callback: Callback,
            context: *const StreamContext,
            paths: *const c_void,
            since: u64,
            latency: f64,
            flags: u32,
        ) -> *mut c_void;
        fn FSEventStreamScheduleWithRunLoop(
            stream: *mut c_void,
            run_loop: *mut c_void,
            mode: *const c_void,
        );
        fn FSEventStreamStart(stream: *mut c_void) -> u8;
        fn FSEventStreamStop(stream: *mut c_void);
        fn FSEventStreamInvalidate(stream: *mut c_void);
        fn FSEventStreamRelease(stream: *mut c_void);
    }

    /// How long the watch waits to hear the fence before it gives up on telling its own beginning
    /// from what came before: it then reports everything, as the system does (seconds).
    const FENCE_WAIT: f64 = 3.0;
    const UTF8: u32 = 0x0800_0100;
    const SINCE_NOW: u64 = u64::MAX;
    // File-level events (the default is the directory that changed).
    const FILE_EVENTS: u32 = 0x10;
    const CREATED: u32 = 0x100;
    const META: u32 = 0x400;
    const RENAMED: u32 = 0x800;
    const MODIFIED: u32 = 0x1000;
    const IS_DIR: u32 = 0x2_0000;
    /// How long the system gathers changes before it tells: a save is several writes.
    const LATENCY: f64 = 0.05;

    pub struct Watcher {
        run_loop: Arc<AtomicPtr<c_void>>,
        stopped: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    /// What tells a stream's beginning from the past.
    ///
    /// The system hands a new stream the events it has not yet numbered, and it numbers an event
    /// after the program that caused it has gone on: a directory made just before the watch is
    /// reported as changed just after it, with an id above what `FSEventsGetCurrentEventId` said
    /// when the stream was made, and flushing the stream does not bring it out first. What does
    /// order them is the system's own queue: a change made in a private directory after the
    /// stream is up is numbered after everything made before, so the id of that one event (the
    /// floor) separates what came before the watch from what came after it. Nothing is polled and
    /// nothing waits for that event: the watch is begun when the change in the fence has been made,
    /// and what the system reports until the event is heard is the past.
    const FENCE_PREFIX: &str = "pencere-watch-fence-";

    struct Fence {
        directory: PathBuf,
        /// The directory as the system is told it.
        text: CString,
    }

    impl Fence {
        /// A private directory of the temporary kind, or none when one cannot be made. The ones
        /// that processes which have since died left behind are removed first.
        fn make() -> Option<Fence> {
            static MADE: AtomicU32 = AtomicU32::new(0);
            sweep(&std::env::temp_dir());
            let directory = std::env::temp_dir().join(format!(
                "{FENCE_PREFIX}{}-{}",
                std::process::id(),
                MADE.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&directory).ok()?;
            // The system names paths by what they really are.
            let made = directory.canonicalize().ok().and_then(|real| {
                let text = CString::new(real.to_string_lossy().as_bytes()).ok()?;
                Some(Fence {
                    directory: real,
                    text,
                })
            });
            if made.is_none() {
                let _ = std::fs::remove_dir(&directory);
            }
            made
        }

        /// A change in the directory, which the stream is to hear after everything that came
        /// before it.
        fn raise(&self) -> bool {
            std::fs::write(self.directory.join("up"), b"up").is_ok()
        }
    }

    /// Removes the fence directories in `temp` that belong to a process which no longer lives: a
    /// process that was killed has no time to remove its own.
    fn sweep(temp: &Path) {
        let Ok(entries) = std::fs::read_dir(temp) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(rest) = name.to_str().and_then(|n| n.strip_prefix(FENCE_PREFIX)) else {
                continue;
            };
            let Some(pid) = rest
                .split('-')
                .next()
                .and_then(|pid| pid.parse::<i32>().ok())
            else {
                continue;
            };
            if pid != std::process::id() as i32 && !lives(pid) {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }

    /// Whether a process with this id exists (one the user may not signal does).
    fn lives(pid: i32) -> bool {
        // SAFETY: signal 0 sends nothing; it asks whether the process could be signalled.
        let signalled = unsafe { libc::kill(pid, 0) };
        signalled == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }

    impl Drop for Fence {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.directory.join("up"));
            let _ = std::fs::remove_dir(&self.directory);
        }
    }

    /// One event as the system reports it: the path, its flags and its id.
    type Reported = (String, u32, u64);

    /// What `on_events` works with, on the thread of the watch.
    struct Gate {
        tell: Tell,
        /// Where the fence is, when there is one.
        fence: Option<PathBuf>,
        /// The id of the fence's event once it is heard: what was numbered up to it came before
        /// the watch.
        floor: Cell<Option<u64>>,
        /// Everything the system reports is told: there was no fence, or it was not heard in time.
        open: Cell<bool>,
    }

    impl Gate {
        fn new(tell: Tell, fence: Option<PathBuf>) -> Self {
            Self {
                tell,
                open: Cell::new(fence.is_none()),
                fence,
                floor: Cell::new(None),
            }
        }

        fn of_fence(&self, path: &str) -> bool {
            self.fence
                .as_deref()
                .is_some_and(|fence| Path::new(path).starts_with(fence))
        }

        /// What of a batch the watch tells of. Until the fence is heard the gate is shut and
        /// every event is the past; from then on what was numbered up to the fence is, and the
        /// events of the fence itself are nobody's.
        fn admit(&self, batch: Vec<Reported>) -> Vec<Reported> {
            if self.floor.get().is_none()
                && let Some(id) = batch
                    .iter()
                    .filter(|(path, ..)| self.of_fence(path))
                    .map(|(.., id)| *id)
                    .max()
            {
                self.floor.set(Some(id));
            }
            let floor = self.floor.get();
            let open = self.open.get();
            batch
                .into_iter()
                .filter(|(path, _, id)| {
                    !self.of_fence(path) && (open || floor.is_some_and(|floor| *id > floor))
                })
                .collect()
        }

        /// The fence was not heard in time: everything is told from now on.
        fn give_up_waiting(&self) {
            if self.floor.get().is_none() {
                self.open.set(true);
            }
        }
    }

    /// The wait for the fence is over (a one-shot timer of the run loop of the watch).
    extern "C" fn on_late(_timer: *mut c_void, info: *mut c_void) {
        // SAFETY: `info` is the `Gate` that `start` boxed and leaks until the thread ends, and it
        // is used on that thread alone.
        let gate = unsafe { &*(info as *const Gate) };
        gate.give_up_waiting();
    }

    extern "C" fn on_events(
        _stream: *const c_void,
        info: *mut c_void,
        count: usize,
        paths: *mut c_void,
        flags: *const u32,
        ids: *const u64,
    ) {
        // SAFETY: `info` is the `Gate` that `start` boxed and leaks until the thread ends, and it
        // is used on that thread alone; the system gives `count` paths (C strings, as the stream
        // was made without CF types), `count` flags and `count` ids.
        let gate = unsafe { &*(info as *const Gate) };
        let paths = paths as *const *const c_char;
        let mut reported = Vec::with_capacity(count);
        for index in 0..count {
            // SAFETY: `index < count`.
            let (path, flags, id) =
                unsafe { (*paths.add(index), *flags.add(index), *ids.add(index)) };
            if path.is_null() {
                continue;
            }
            // SAFETY: a C string the system owns for the call.
            let path = unsafe { CStr::from_ptr(path) }
                .to_string_lossy()
                .into_owned();
            reported.push((path, flags, id));
        }
        let mut changes = Vec::with_capacity(count);
        for (path, flags, _) in gate.admit(reported) {
            // The system gathers what happened to a path since the last time it told: a file made
            // and removed in that time has both flags. What the file is now settles which it is.
            let gone = std::fs::symlink_metadata(&path).is_err();
            let kind = if gone {
                "removed"
            } else if flags & CREATED != 0 {
                "created"
            } else if flags & RENAMED != 0 {
                "renamed"
            } else if flags & (MODIFIED | META) != 0 {
                "modified"
            } else {
                "other"
            };
            changes.push(Change {
                path: PathBuf::from(path),
                kind,
                dir: flags & IS_DIR != 0,
            });
        }
        if !changes.is_empty() {
            (gate.tell)(changes);
        }
    }

    pub fn start(root: PathBuf, recursive: bool, tell: Tell) -> io::Result<Watcher> {
        // FSEvents watches a tree; a watch that is not recursive keeps to the children of the
        // directory.
        let not_recursive = !recursive;
        let top = root.clone();
        let tell = Box::new(move |changes: Vec<Change>| {
            let kept: Vec<Change> = if not_recursive {
                changes
                    .into_iter()
                    .filter(|change| change.path.parent() == Some(top.as_path()))
                    .collect()
            } else {
                changes
            };
            if !kept.is_empty() {
                tell(kept);
            }
        }) as Tell;
        let run_loop = Arc::new(AtomicPtr::new(std::ptr::null_mut()));
        let (ready, started) = channel::<io::Result<()>>();
        let shared = Arc::clone(&run_loop);
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopped);
        let path = CString::new(root.to_string_lossy().as_bytes())
            .map_err(|_| io::Error::other("a path with a NUL in it"))?;
        let thread = std::thread::Builder::new()
            .name("pencere-watch".into())
            .spawn(move || {
                let fence = Fence::make();
                let gate = Gate::new(tell, fence.as_ref().map(|fence| fence.directory.clone()));
                // SAFETY: the calls are the framework's, with arguments it documents; the boxed
                // gate outlives the stream and the timer, which are released before the box is.
                unsafe {
                    let info = Box::into_raw(Box::new(gate));
                    let text = CFStringCreateWithCString(kCFAllocatorDefault, path.as_ptr(), UTF8);
                    let fence_cf = fence.as_ref().map(|fence| {
                        CFStringCreateWithCString(kCFAllocatorDefault, fence.text.as_ptr(), UTF8)
                    });
                    let values = [text, fence_cf.unwrap_or(std::ptr::null())];
                    let list = CFArrayCreate(
                        kCFAllocatorDefault,
                        values.as_ptr(),
                        if fence_cf.is_some() { 2 } else { 1 },
                        &raw const kCFTypeArrayCallBacks,
                    );
                    let context = StreamContext {
                        version: 0,
                        info: info.cast(),
                        retain: std::ptr::null(),
                        release: std::ptr::null(),
                        copy_description: std::ptr::null(),
                    };
                    let stream = FSEventStreamCreate(
                        kCFAllocatorDefault,
                        on_events,
                        &raw const context,
                        list,
                        SINCE_NOW,
                        LATENCY,
                        FILE_EVENTS,
                    );
                    CFRelease(list);
                    CFRelease(text);
                    if let Some(fence_cf) = fence_cf {
                        CFRelease(fence_cf);
                    }
                    if stream.is_null() {
                        let _ = ready.send(Err(io::Error::other(
                            "the system would not make a stream to watch the directory with",
                        )));
                        drop(Box::from_raw(info));
                        return;
                    }
                    let current = CFRunLoopGetCurrent();
                    FSEventStreamScheduleWithRunLoop(stream, current, kCFRunLoopDefaultMode);
                    if FSEventStreamStart(stream) == 0 {
                        let _ = ready.send(Err(io::Error::other(
                            "the system would not start watching the directory",
                        )));
                        FSEventStreamInvalidate(stream);
                        FSEventStreamRelease(stream);
                        drop(Box::from_raw(info));
                        return;
                    }
                    shared.store(current, Ordering::SeqCst);
                    // The stream is up: what is made in the fence from here on is heard after
                    // everything that came before, and the watch is begun. Without a fence, or
                    // when it cannot be raised, everything the system reports is told.
                    let mut late = std::ptr::null_mut();
                    if fence.as_ref().is_some_and(Fence::raise) {
                        // If the event is not heard in time the gate opens, once and for all.
                        let timer_context = StreamContext {
                            version: 0,
                            info: info.cast(),
                            retain: std::ptr::null(),
                            release: std::ptr::null(),
                            copy_description: std::ptr::null(),
                        };
                        late = CFRunLoopTimerCreate(
                            kCFAllocatorDefault,
                            CFAbsoluteTimeGetCurrent() + FENCE_WAIT,
                            0.0,
                            0,
                            0,
                            on_late,
                            &raw const timer_context,
                        );
                        if !late.is_null() {
                            CFRunLoopAddTimer(current, late, kCFRunLoopDefaultMode);
                        }
                    } else {
                        (*info).open.set(true);
                    }
                    let _ = ready.send(Ok(()));
                    // A stop that came before the run loop ran is asked for again by `drop`, until
                    // this thread is gone.
                    while !stop.load(Ordering::SeqCst) {
                        CFRunLoopRun();
                    }
                    if !late.is_null() {
                        CFRunLoopTimerInvalidate(late);
                        CFRelease(late);
                    }
                    FSEventStreamStop(stream);
                    FSEventStreamInvalidate(stream);
                    FSEventStreamRelease(stream);
                    drop(Box::from_raw(info));
                }
                drop(fence);
            })?;
        // The watch is begun, or the system would not begin it: the thread says which at once, and
        // does not wait for the fence to be heard.
        match started.recv() {
            Ok(Ok(())) => Ok(Watcher {
                run_loop,
                stopped,
                thread: Some(thread),
            }),
            Ok(Err(why)) => {
                let _ = thread.join();
                Err(why)
            }
            Err(_) => Err(io::Error::other("the watch did not start")),
        }
    }

    impl Drop for Watcher {
        fn drop(&mut self) {
            self.stopped.store(true, Ordering::SeqCst);
            let Some(thread) = self.thread.take() else {
                return;
            };
            // Asked again until the thread is gone: a stop that arrives before the run loop has
            // started does nothing.
            while !thread.is_finished() {
                let run_loop = self.run_loop.load(Ordering::SeqCst);
                if !run_loop.is_null() {
                    // SAFETY: the run loop of the watch's thread, which is alive until it ends.
                    unsafe { CFRunLoopStop(run_loop) };
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            let _ = thread.join();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn gate(fence: Option<&str>) -> Gate {
            Gate::new(Box::new(|_| {}), fence.map(PathBuf::from))
        }

        fn event(path: &str, id: u64) -> Reported {
            (path.to_owned(), 0, id)
        }

        fn paths(kept: Vec<Reported>) -> Vec<String> {
            kept.into_iter().map(|(path, ..)| path).collect()
        }

        #[test]
        fn what_the_system_reports_before_the_fence_is_heard_is_the_past() {
            let gate = gate(Some("/fence"));
            // A batch of what happened before the watch, delivered before the fence's own event.
            assert!(
                gate.admit(vec![event("/root/a", 5), event("/root/b", 6)])
                    .is_empty()
            );
            // The batch that holds the fence's event: what is numbered up to it is the past, what
            // is numbered after it is the watch's, and the fence is nobody's.
            let kept = gate.admit(vec![
                event("/fence/up", 10),
                event("/root/old", 9),
                event("/root/new", 11),
            ]);
            assert_eq!(paths(kept), ["/root/new"]);
            // Stragglers of the past that come later are still the past.
            assert!(gate.admit(vec![event("/root/late", 8)]).is_empty());
            assert_eq!(
                paths(gate.admit(vec![event("/root/next", 12)])),
                ["/root/next"]
            );
        }

        #[test]
        fn a_gate_that_waited_in_vain_opens_and_one_that_heard_the_fence_does_not() {
            let waited = gate(Some("/fence"));
            assert!(waited.admit(vec![event("/root/a", 3)]).is_empty());
            waited.give_up_waiting();
            assert_eq!(paths(waited.admit(vec![event("/root/a", 3)])), ["/root/a"]);
            // The fence's own events are nobody's, open or not.
            assert!(waited.admit(vec![event("/fence/up", 4)]).is_empty());
            let heard = gate(Some("/fence"));
            heard.admit(vec![event("/fence/up", 10)]);
            heard.give_up_waiting();
            assert!(heard.admit(vec![event("/root/past", 7)]).is_empty());
            // No fence to wait for: everything is told from the first.
            assert_eq!(
                paths(gate(None).admit(vec![event("/root/a", 1)])),
                ["/root/a"]
            );
        }

        #[test]
        fn the_fences_of_processes_that_died_are_swept_and_the_living_ones_stay() {
            let temp = std::env::temp_dir().join(format!("pencere-sweep-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&temp);
            std::fs::create_dir_all(&temp).expect("a directory");
            let mut child = std::process::Command::new("true").spawn().expect("a child");
            let dead = child.id();
            child.wait().expect("it ends");
            let name = |pid: u32, n: u32| format!("{FENCE_PREFIX}{pid}-{n}");
            for made in [
                name(dead, 0),
                name(std::process::id(), 3),
                "other".to_owned(),
            ] {
                std::fs::create_dir_all(temp.join(&made).join("below")).expect("a directory");
            }
            sweep(&temp);
            let mut left: Vec<String> = std::fs::read_dir(&temp)
                .expect("a directory")
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect();
            left.sort();
            let mut expected = vec![name(std::process::id(), 3), "other".to_owned()];
            expected.sort();
            assert_eq!(left, expected);
            let _ = std::fs::remove_dir_all(&temp);
        }
    }
}

// ---- Linux: inotify -------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod imp {
    #![allow(unsafe_code)]

    use std::collections::HashMap;
    use std::ffi::CString;
    use std::io;
    use std::os::fd::RawFd;
    use std::path::{Path, PathBuf};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use super::{Change, Tell};

    const MASK: u32 = libc::IN_CREATE
        | libc::IN_DELETE
        | libc::IN_MODIFY
        | libc::IN_CLOSE_WRITE
        | libc::IN_MOVED_FROM
        | libc::IN_MOVED_TO
        | libc::IN_ATTRIB
        | libc::IN_DELETE_SELF
        | libc::IN_MOVE_SELF;
    /// How long after a change the others are waited for: a save is several writes.
    const QUIET: Duration = Duration::from_millis(40);

    pub struct Watcher {
        stop: RawFd,
        thread: Option<JoinHandle<()>>,
    }

    struct Tree {
        inotify: RawFd,
        recursive: bool,
        watches: HashMap<i32, PathBuf>,
    }

    impl Tree {
        fn add(&mut self, directory: &Path) {
            let Ok(path) = CString::new(directory.to_string_lossy().as_bytes()) else {
                return;
            };
            // SAFETY: a path of ours, a descriptor of ours.
            let watch = unsafe { libc::inotify_add_watch(self.inotify, path.as_ptr(), MASK) };
            if watch < 0 {
                return;
            }
            self.watches.insert(watch, directory.to_path_buf());
            if self.recursive
                && let Ok(entries) = std::fs::read_dir(directory)
            {
                for entry in entries.flatten() {
                    if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                        self.add(&entry.path());
                    }
                }
            }
        }

        /// What one read of the descriptor says.
        fn read(&mut self, out: &mut Vec<Change>) {
            let mut buffer = [0u8; 16384];
            loop {
                // SAFETY: reads into a buffer of ours from a descriptor of ours.
                let got =
                    unsafe { libc::read(self.inotify, buffer.as_mut_ptr().cast(), buffer.len()) };
                if got <= 0 {
                    return;
                }
                let mut at = 0usize;
                let got = got as usize;
                let header = std::mem::size_of::<libc::inotify_event>();
                while at + header <= got {
                    let Some(head) = buffer.get(at..at + header) else {
                        break;
                    };
                    // SAFETY: `header` bytes of an inotify_event, read unaligned.
                    let event: libc::inotify_event =
                        unsafe { std::ptr::read_unaligned(head.as_ptr().cast()) };
                    let name_at = at + header;
                    let name_end = name_at + event.len as usize;
                    let name = buffer
                        .get(name_at..name_end.min(got))
                        .map(|bytes| {
                            let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
                            String::from_utf8_lossy(bytes.get(..end).unwrap_or_default())
                                .into_owned()
                        })
                        .unwrap_or_default();
                    at = name_end;
                    let Some(directory) = self.watches.get(&event.wd).cloned() else {
                        continue;
                    };
                    let path = if name.is_empty() {
                        directory.clone()
                    } else {
                        directory.join(&name)
                    };
                    let is_dir = event.mask & libc::IN_ISDIR != 0;
                    if event.mask & libc::IN_IGNORED != 0 {
                        self.watches.remove(&event.wd);
                        continue;
                    }
                    if is_dir && event.mask & (libc::IN_CREATE | libc::IN_MOVED_TO) != 0 {
                        self.add(&path);
                    }
                    // Gone is gone, whether it was removed or moved away; what arrived by a move
                    // is renamed.
                    let kind = if event.mask
                        & (libc::IN_DELETE | libc::IN_DELETE_SELF | libc::IN_MOVED_FROM)
                        != 0
                    {
                        "removed"
                    } else if event.mask & libc::IN_MOVED_TO != 0 {
                        "renamed"
                    } else if event.mask & libc::IN_CREATE != 0 {
                        "created"
                    } else if event.mask
                        & (libc::IN_MODIFY | libc::IN_CLOSE_WRITE | libc::IN_ATTRIB)
                        != 0
                    {
                        "modified"
                    } else {
                        "other"
                    };
                    out.push(Change {
                        path,
                        kind,
                        dir: is_dir,
                    });
                }
            }
        }
    }

    pub fn start(root: PathBuf, recursive: bool, tell: Tell) -> io::Result<Watcher> {
        // SAFETY: makes descriptors of ours.
        let inotify = unsafe { libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK) };
        if inotify < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut ends = [0 as libc::c_int; 2];
        // SAFETY: fills two descriptors.
        if unsafe { libc::pipe2(ends.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) } != 0 {
            let why = io::Error::last_os_error();
            // SAFETY: closes what was opened above.
            unsafe { libc::close(inotify) };
            return Err(why);
        }
        let (stop_read, stop_write) = (ends[0], ends[1]);
        let mut tree = Tree {
            inotify,
            recursive,
            watches: HashMap::new(),
        };
        tree.add(&root);
        if tree.watches.is_empty() {
            // SAFETY: closes what was opened above.
            unsafe {
                libc::close(inotify);
                libc::close(stop_read);
                libc::close(stop_write);
            }
            return Err(io::Error::other(
                "the system would not start watching the directory (is the limit of watches reached?)",
            ));
        }
        let thread = std::thread::Builder::new()
            .name("pencere-watch".into())
            .spawn(move || {
                let mut batch: Vec<Change> = Vec::new();
                let mut quiet_since: Option<Instant> = None;
                loop {
                    let mut fds = [
                        libc::pollfd {
                            fd: stop_read,
                            events: libc::POLLIN,
                            revents: 0,
                        },
                        libc::pollfd {
                            fd: tree.inotify,
                            events: libc::POLLIN,
                            revents: 0,
                        },
                    ];
                    let timeout = match quiet_since {
                        Some(since) => {
                            QUIET.saturating_sub(since.elapsed()).as_millis() as libc::c_int + 1
                        }
                        None => -1,
                    };
                    // SAFETY: polls descriptors of ours.
                    unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout) };
                    if fds[0].revents != 0 {
                        break;
                    }
                    if fds[1].revents != 0 {
                        tree.read(&mut batch);
                        quiet_since = Some(Instant::now());
                    }
                    if quiet_since.is_some_and(|since| since.elapsed() >= QUIET)
                        && !batch.is_empty()
                    {
                        tell(std::mem::take(&mut batch));
                        quiet_since = None;
                    }
                }
                // SAFETY: closes what `start` opened.
                unsafe {
                    libc::close(tree.inotify);
                    libc::close(stop_read);
                }
            })?;
        Ok(Watcher {
            stop: stop_write,
            thread: Some(thread),
        })
    }

    impl Drop for Watcher {
        fn drop(&mut self) {
            // SAFETY: writes a byte to, and closes, the descriptor `start` kept.
            unsafe {
                libc::write(self.stop, [1u8].as_ptr().cast(), 1);
            }
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            // SAFETY: closes the descriptor `start` kept.
            unsafe { libc::close(self.stop) };
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod imp {
    use std::io;
    use std::path::PathBuf;

    use super::Tell;

    pub struct Watcher;

    pub fn start(_root: PathBuf, _recursive: bool, _tell: Tell) -> io::Result<Watcher> {
        Err(io::Error::other(
            "watching files is not supported on this system",
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    use super::*;

    fn directory(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pencere-watch-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a directory");
        dir.canonicalize().expect("a canonical path")
    }

    /// The changes that come within `within`, all of them.
    fn gathered(changes: &std::sync::mpsc::Receiver<Vec<Change>>, within: Duration) -> Vec<Change> {
        let mut all = Vec::new();
        while let Ok(more) = changes.recv_timeout(within) {
            all.extend(more);
        }
        all
    }

    #[test]
    fn a_watch_tells_of_files_created_changed_and_removed_under_a_directory() {
        let root = directory("tree");
        // What was made before the watch began is not told, however close to it.
        fs::write(root.join("before.txt"), "zero").expect("a file");
        fs::create_dir(root.join("before-dir")).expect("a directory");
        fs::write(root.join("before-dir/inner.txt"), "zero").expect("a file below");
        let (tell, changes) = channel();
        let watcher = start(&root, true, move |found| {
            let _ = tell.send(found);
        })
        .expect("a watch");
        // Nothing has happened since: the watch says nothing.
        assert_eq!(gathered(&changes, Duration::from_millis(400)), vec![]);

        fs::write(root.join("a.txt"), "one").expect("a file");
        let found = gathered(&changes, Duration::from_millis(600));
        assert!(
            found.iter().any(|change| change.path == root.join("a.txt")),
            "{found:?}"
        );

        // A directory made later is watched too.
        fs::create_dir(root.join("sub")).expect("a directory");
        std::thread::sleep(Duration::from_millis(300));
        let _ = gathered(&changes, Duration::from_millis(300));
        fs::write(root.join("sub/b.txt"), "two").expect("a file below");
        let found = gathered(&changes, Duration::from_millis(600));
        assert!(
            found
                .iter()
                .any(|change| change.path == root.join("sub/b.txt")),
            "{found:?}"
        );

        fs::remove_file(root.join("a.txt")).expect("removed");
        let found = gathered(&changes, Duration::from_millis(600));
        assert!(
            found
                .iter()
                .any(|change| change.path == root.join("a.txt") && change.kind == "removed"),
            "{found:?}"
        );

        // Once stopped, it is quiet.
        drop(watcher);
        let _ = gathered(&changes, Duration::from_millis(100));
        fs::write(root.join("c.txt"), "three").expect("a file");
        assert!(gathered(&changes, Duration::from_millis(400)).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_watch_tells_of_what_happens_the_moment_it_has_started_and_not_of_what_came_before() {
        // Over and over, because the system numbers the past a little later than it was made.
        for round in 0..12 {
            let root = directory(&format!("prompt{round}"));
            fs::write(root.join("before.txt"), "zero").expect("a file");
            let (tell, changes) = channel();
            let watcher = start(&root, true, move |found| {
                let _ = tell.send(found);
            })
            .expect("a watch");
            fs::write(root.join("after.txt"), "one").expect("a file");
            let found = gathered(&changes, Duration::from_millis(300));
            assert!(
                found
                    .iter()
                    .any(|change| change.path == root.join("after.txt")),
                "round {round}: {found:?}"
            );
            assert!(
                found
                    .iter()
                    .all(|change| change.path != root.join("before.txt") && change.path != root),
                "round {round}: {found:?}"
            );
            drop(watcher);
            let _ = fs::remove_dir_all(&root);
        }
    }

    #[test]
    fn a_watch_that_is_not_recursive_keeps_to_the_directory() {
        let root = directory("flat");
        fs::create_dir(root.join("sub")).expect("a directory");
        let (tell, changes) = channel();
        let watcher = start(&root, false, move |found| {
            let _ = tell.send(found);
        })
        .expect("a watch");
        fs::write(root.join("sub/deep.txt"), "x").expect("a file below");
        assert!(
            gathered(&changes, Duration::from_millis(500))
                .iter()
                .all(|change| change.path.parent() == Some(root.as_path())),
        );
        fs::write(root.join("top.txt"), "x").expect("a file");
        let found = gathered(&changes, Duration::from_millis(600));
        assert!(
            found
                .iter()
                .any(|change| change.path == root.join("top.txt")),
            "{found:?}"
        );
        drop(watcher);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_watch_that_is_stopped_at_once_stops() {
        // The stop can come before the thread of the watch has begun to wait.
        let root = directory("quick");
        for _ in 0..10 {
            let watcher = start(&root, true, |_| {}).expect("a watch");
            drop(watcher);
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn what_is_not_a_directory_cannot_be_watched() {
        assert!(start(Path::new("/nonexistent-pencere-watch"), true, |_| {}).is_err());
        let file = std::env::temp_dir().join(format!("pencere-watch-file-{}", std::process::id()));
        fs::write(&file, "x").expect("a file");
        assert!(start(&file, true, |_| {}).is_err());
        let _ = fs::remove_file(&file);
    }
}
