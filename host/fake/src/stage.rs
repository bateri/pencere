//! The host's folder, as the stand-in plays it (specification §8.11): a directory with a `marker`,
//! and the pipes in it that the bytes of a file cross through, instead of the `data` and `write`
//! messages. The stand-in runs on the program's machine, so the folder is a directory here and the
//! "way of its own to the program's machine" is a thread for each transfer.
//!
//! The session names the pipes and says when a transfer ends; what it asks (`StageFile`,
//! `FetchFile`, `Unstage`) is done here, and what came of each is handed back for the host to tell
//! the session. Three flaws can be asked of it, so that the program's ways of going on can be
//! tried: a `marker` that is not the `welcome`'s token, a way that is lost for a while (`route`)
//! after some bytes, and a pipe that stays silent.

#![allow(unsafe_code)]

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::{self, JoinHandle, sleep};
use std::time::{Duration, Instant};

use pencere_fake_host::log;
use pencere_host::session::{FetchError, StageDir, StageError};

/// The file in the folder that holds the token.
const MARKER: &str = "marker";

/// How long a pipe is waited for, to be opened by the program, before the transfer is `io`.
const OPENED_WITHIN: Duration = Duration::from_secs(5);

/// How long a pipe whose bytes stopped coming is waited for.
const STALLED_FOR: Duration = Duration::from_secs(2);

/// How long a pipe nobody reads is written to before the transfer is `io`.
const UNREAD_FOR: Duration = Duration::from_secs(10);

/// What a transfer's thread looks at between two reads of a pipe.
const LOOK_AGAIN: Duration = Duration::from_millis(5);

const PIECE: usize = 64 << 10;

/// The flaws the command line asks the folder to have.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Flaws {
    /// The `marker` holds another token than the `welcome`'s: a folder of another host, at the
    /// same path.
    pub wrong_marker: bool,
    /// The first transfer loses its way (`route`) after this many bytes.
    pub route_after: Option<u64>,
    /// No byte ever crosses a pipe.
    pub silent: bool,
}

/// What came of a transfer, for the session.
pub enum Finished {
    Staged {
        transfer: u32,
        result: Result<u64, StageError>,
    },
    Fetched {
        transfer: u32,
        result: Result<u64, FetchError>,
    },
}

struct Running {
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    pipe: PathBuf,
}

/// The folder and the transfers that use it.
pub struct Stage {
    dir: PathBuf,
    flaws: Flaws,
    /// The `route` the flaws ask for has not happened yet.
    route_left: Option<u64>,
    report: Sender<Finished>,
    finished: Receiver<Finished>,
    running: HashMap<u32, Running>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl Stage {
    /// Makes the folder at `dir` (and the ones above it, when they are not there), mode `0700`, with
    /// its `marker`; and says where it is.
    pub fn open(dir: &Path, flaws: Flaws) -> io::Result<(Self, StageDir)> {
        fs::create_dir_all(dir)?;
        let dir = fs::canonicalize(dir)?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        let mut random = [0u8; 32];
        File::open("/dev/urandom")?.read_exact(&mut random)?;
        let (token, other) = random.split_at(16);
        let token = hex(token);
        let held = if flaws.wrong_marker {
            hex(other)
        } else {
            token.clone()
        };
        fs::write(dir.join(MARKER), held)?;
        let offered = StageDir::new(&dir.to_string_lossy(), &token).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a folder that can be offered",
            )
        })?;
        let (report, finished) = channel();
        let stage = Self {
            dir,
            route_left: flaws.route_after,
            flaws,
            report,
            finished,
            running: HashMap::new(),
        };
        Ok((stage, offered))
    }

    /// Sends the bytes of `source` from `offset`, which is `size` long as the person gave it,
    /// through the pipe named `pipe`, which is made here. The program is told of the pipe when this
    /// returns `Ok`.
    pub fn stage_file(
        &mut self,
        transfer: u32,
        pipe: &str,
        source: PathBuf,
        size: u64,
        offset: u64,
    ) -> Result<(), StageError> {
        let path = self.dir.join(pipe);
        let mut file = File::open(&source).map_err(|_| StageError::Gone)?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|_| StageError::Io)?;
        make_pipe(&path).map_err(|_| StageError::Io)?;
        let cancel = Arc::new(AtomicBool::new(false));
        let route = self.route_left.take();
        let silent = self.flaws.silent;
        let report = self.report.clone();
        let fifo = path.clone();
        let stop = Arc::clone(&cancel);
        let thread = thread::spawn(move || {
            if let Some(result) = feed(&fifo, file, size - offset, route, silent, &stop) {
                let _ = report.send(Finished::Staged {
                    transfer,
                    result: result.map(|()| size),
                });
            }
        });
        self.running.insert(
            transfer,
            Running {
                cancel,
                thread: Some(thread),
                pipe: path,
            },
        );
        Ok(())
    }

    /// Takes the bytes the program writes into the pipe named `pipe`, which is made here, from
    /// `offset` to `size`, to `part`, which is given the name `target` when they are all in. The
    /// program is told of the pipe when this returns `Ok`.
    pub fn fetch_file(
        &mut self,
        transfer: u32,
        pipe: &str,
        place: (PathBuf, PathBuf),
        size: u64,
        offset: u64,
    ) -> Result<(), FetchError> {
        let (target, part_path) = place;
        let part = open_part(&part_path, offset)?;
        let path = self.dir.join(pipe);
        make_pipe(&path).map_err(|_| FetchError::Io)?;
        // The reading end is open before the program is told the pipe, so that it can open the
        // writing end at once.
        let reading = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
            .map_err(|_| {
                let _ = fs::remove_file(&path);
                FetchError::Io
            })?;
        let cancel = Arc::new(AtomicBool::new(false));
        let route = self.route_left.take();
        let silent = self.flaws.silent;
        let report = self.report.clone();
        let stop = Arc::clone(&cancel);
        let thread = thread::spawn(move || {
            let incoming = Incoming {
                size,
                offset,
                route,
                silent,
                target,
                part: part_path,
            };
            if let Some(result) = take(reading, part, &incoming, &stop) {
                let _ = report.send(Finished::Fetched { transfer, result });
            }
        });
        self.running.insert(
            transfer,
            Running {
                cancel,
                thread: Some(thread),
                pipe: path,
            },
        );
        Ok(())
    }

    /// The program took the transfer back: its thread stops, and the pipe goes.
    pub fn cancel(&mut self, transfer: u32) {
        if let Some(mut running) = self.running.remove(&transfer) {
            running.cancel.store(true, Ordering::SeqCst);
            if let Some(thread) = running.thread.take() {
                let _ = thread.join();
            }
            let _ = fs::remove_file(&running.pipe);
        }
    }

    /// What has ended since; each transfer's pipe is removed.
    pub fn take_finished(&mut self) -> Vec<Finished> {
        let mut done = Vec::new();
        while let Ok(finished) = self.finished.try_recv() {
            let transfer = match &finished {
                Finished::Staged { transfer, .. } | Finished::Fetched { transfer, .. } => *transfer,
            };
            // A transfer taken back already is not told.
            if let Some(mut running) = self.running.remove(&transfer) {
                if let Some(thread) = running.thread.take() {
                    let _ = thread.join();
                }
                let _ = fs::remove_file(&running.pipe);
                done.push(finished);
            }
        }
        done
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        for (_, mut running) in self.running.drain() {
            running.cancel.store(true, Ordering::SeqCst);
            if let Some(thread) = running.thread.take() {
                let _ = thread.join();
            }
            let _ = fs::remove_file(&running.pipe);
        }
        let _ = fs::remove_file(self.dir.join(MARKER));
        let _ = fs::remove_dir(&self.dir);
    }
}

fn make_pipe(path: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a NUL in a path"))?;
    // SAFETY: a NUL-terminated path that lives for the call.
    if unsafe { libc::mkfifo(name.as_ptr(), 0o600) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Waits up to `millis` for the descriptor to be ready.
fn ready(file: &File, for_write: bool, millis: i32) -> bool {
    let mut poll = libc::pollfd {
        fd: file.as_raw_fd(),
        events: if for_write {
            libc::POLLOUT
        } else {
            libc::POLLIN
        },
        revents: 0,
    };
    // SAFETY: one valid `pollfd` that lives for the call, and a count of one.
    unsafe { libc::poll(&raw mut poll, 1, millis) > 0 }
}

/// Writes `file` (`want` bytes of it) into the pipe once the program has opened it. `None` when the
/// transfer was taken back. With a `route`, the way is lost after that many bytes.
fn feed(
    pipe: &Path,
    mut file: File,
    want: u64,
    route: Option<u64>,
    silent: bool,
    cancel: &AtomicBool,
) -> Option<Result<(), StageError>> {
    let began = Instant::now();
    let mut writing = loop {
        if cancel.load(Ordering::SeqCst) {
            return None;
        }
        match fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(pipe)
        {
            Ok(writing) => break writing,
            // No one has opened the reading end yet.
            Err(why) if why.raw_os_error() == Some(libc::ENXIO) => {
                if began.elapsed() > OPENED_WITHIN {
                    return Some(Err(StageError::Io));
                }
                sleep(LOOK_AGAIN);
            }
            Err(_) => return Some(Err(StageError::Io)),
        }
    };
    if silent {
        // The pipe is open and nothing comes through it.
        while !cancel.load(Ordering::SeqCst) {
            sleep(LOOK_AGAIN);
        }
        return None;
    }
    let mut buf = vec![0; PIECE];
    let mut sent: u64 = 0;
    while sent < want {
        if cancel.load(Ordering::SeqCst) {
            return None;
        }
        let limit = route.map_or(want, |lost| lost.min(want));
        if sent >= limit {
            // The way is lost after `sent` bytes, which are in the pipe.
            return Some(Err(StageError::Route));
        }
        let step = usize::try_from(limit - sent).map_or(PIECE, |left| left.min(PIECE));
        let Ok(read) = file.read(buf.get_mut(..step).unwrap_or_default()) else {
            return Some(Err(StageError::Io));
        };
        if read == 0 {
            // The file is shorter than when it was given.
            return Some(Err(StageError::Gone));
        }
        match write_into(&mut writing, buf.get(..read).unwrap_or_default(), cancel) {
            Ok(true) => sent += read as u64,
            Ok(false) => return None,
            Err(_) => return Some(Err(StageError::Io)),
        }
    }
    Some(Ok(()))
}

/// Writes all of `bytes` into the pipe; false when the transfer was taken back first.
fn write_into(pipe: &mut File, mut bytes: &[u8], cancel: &AtomicBool) -> io::Result<bool> {
    let mut moved = Instant::now();
    while !bytes.is_empty() {
        if cancel.load(Ordering::SeqCst) {
            return Ok(false);
        }
        match pipe.write(bytes) {
            Ok(n) => {
                bytes = bytes.get(n..).unwrap_or_default();
                moved = Instant::now();
            }
            Err(why) if why.kind() == io::ErrorKind::WouldBlock => {
                if !ready(pipe, true, 20) && moved.elapsed() > UNREAD_FOR {
                    return Err(io::ErrorKind::TimedOut.into());
                }
            }
            Err(why) if why.kind() == io::ErrorKind::Interrupted => {}
            Err(why) => return Err(why),
        }
    }
    Ok(true)
}

/// The part of a place to save in, made anew for `offset` 0, and the one that holds `offset` bytes
/// otherwise, to be added to.
fn open_part(part: &Path, offset: u64) -> Result<File, FetchError> {
    if offset == 0 {
        // Made anew, and a link put where it goes is not written through.
        let _ = fs::remove_file(part);
        return fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(part)
            .map_err(|_| FetchError::Io);
    }
    let held = fs::OpenOptions::new()
        .append(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(part)
        .map_err(|_| FetchError::Size)?;
    if held.metadata().map_err(|_| FetchError::Io)?.len() == offset {
        Ok(held)
    } else {
        Err(FetchError::Size)
    }
}

/// What a fetch is to bring in.
struct Incoming {
    size: u64,
    offset: u64,
    route: Option<u64>,
    silent: bool,
    /// The person's name for the file, and the part beside it.
    target: PathBuf,
    part: PathBuf,
}

/// Reads the pipe into `part` until the file is `size` long, then gives it the name of the place.
/// `None` when the transfer was taken back.
fn take(
    mut pipe: File,
    mut part: File,
    incoming: &Incoming,
    cancel: &AtomicBool,
) -> Option<Result<u64, FetchError>> {
    let want = incoming.size - incoming.offset;
    let limit = incoming.route.map_or(want, |lost| lost.min(want));
    let mut buf = vec![0; PIECE];
    let mut got: u64 = 0;
    let began = Instant::now();
    let mut last = began;
    while got < want {
        if cancel.load(Ordering::SeqCst) {
            return None;
        }
        if incoming.silent {
            sleep(LOOK_AGAIN);
            continue;
        }
        if got >= limit {
            // The way is lost: what came stays, for the program to go on from.
            let _ = part.sync_all();
            return Some(Err(FetchError::Route {
                at: incoming.offset + got,
            }));
        }
        let step = usize::try_from(limit - got).map_or(PIECE, |left| left.min(PIECE));
        match pipe.read(buf.get_mut(..step).unwrap_or_default()) {
            Ok(0) => {
                // No writer yet, or the writer is gone: what is waited for is the program.
                let idle = if got == 0 {
                    began.elapsed() > OPENED_WITHIN
                } else {
                    last.elapsed() > STALLED_FOR
                };
                if idle {
                    return Some(Err(if got == 0 {
                        FetchError::Io
                    } else {
                        FetchError::Size
                    }));
                }
                sleep(LOOK_AGAIN);
            }
            Ok(n) => {
                if part.write_all(buf.get(..n).unwrap_or_default()).is_err() {
                    return Some(Err(FetchError::Io));
                }
                got += n as u64;
                last = Instant::now();
            }
            Err(why) if why.kind() == io::ErrorKind::WouldBlock => {
                ready(&pipe, false, 20);
            }
            Err(why) if why.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return Some(Err(FetchError::Io)),
        }
    }
    // More would come than the file has: `size`.
    sleep(Duration::from_millis(30));
    if let Ok(more) = pipe.read(&mut buf)
        && more > 0
    {
        return Some(Err(FetchError::Size));
    }
    if cancel.load(Ordering::SeqCst) {
        return None;
    }
    let renamed = part
        .sync_all()
        .and_then(|()| fs::rename(&incoming.part, &incoming.target));
    if renamed.is_ok() {
        log::line(
            "host",
            format!(
                "saved {} ({} bytes)",
                incoming
                    .target
                    .file_name()
                    .map_or_else(|| "?".into(), |name| name.to_string_lossy()),
                incoming.size
            ),
        );
    }
    Some(renamed.map(|()| incoming.size).map_err(|_| FetchError::Io))
}
