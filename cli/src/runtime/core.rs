//! The loop: one thread, which runs all the Lua of an app, and waits for what the app waits for —
//! the timers, the pipes of the processes it started, and whatever the other threads hand it
//! (the calls of the view, the end of `serve`).
//!
//! Nothing here blocks except the `poll` at the top of a turn, and nothing holds a borrow of the
//! loop's state while Lua runs: a callback is free to start a timer or a process.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::PathBuf;
use std::process::Child as Process;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use mlua::{Function, Lua, MultiValue, Value};
use pencere::ViewControl;

use super::Handed;
use super::events::{Event, Events, Inbox};
use super::world::World;
use crate::files::Files;

/// Where the app's words go: `print` and the errors of the runner. With `--log` a file, because
/// the terminal is under the view while it is up; otherwise the app's output.
pub struct Log {
    file: RefCell<Option<std::fs::File>>,
}

impl Log {
    pub fn new(path: Option<PathBuf>) -> std::io::Result<Self> {
        let file = match path {
            Some(path) => Some(OpenOptions::new().create(true).append(true).open(path)?),
            None => None,
        };
        Ok(Self {
            file: RefCell::new(file),
        })
    }

    /// What `print` says.
    pub fn print(&self, text: &str) {
        match self.file.borrow_mut().as_mut() {
            Some(file) => {
                let _ = writeln!(file, "{text}");
            }
            None => println!("{text}"),
        }
    }

    /// What the runner says is wrong.
    pub fn error(&self, text: &str) {
        match self.file.borrow_mut().as_mut() {
            Some(file) => {
                let _ = writeln!(file, "{text}");
            }
            None => eprintln!("{text}"),
        }
    }
}

/// A view the app offers: what `serve` was asked, which `pencere dev` needs to make the view again
/// from the files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewSpec {
    /// A page of the kit's page API, drawn by the terminal, with the pictures in `assets`.
    Lua { page: String, assets: String },
    /// A web page: the files of a directory, whose entry is `index.html`.
    Web {
        dir: String,
        access: Option<pencere::Access>,
    },
}

/// The views of the `serve` that is running, in the order the app prefers them.
#[derive(Debug, Clone)]
pub struct ServePlan {
    pub views: Vec<ViewSpec>,
}

/// How an app ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
    /// The app finished; its status.
    Done(i32),
    /// The app raised an error that nothing caught.
    Failed(String),
    /// The files changed and `pencere dev` starts the app again.
    Restart,
}

struct Timer {
    id: u64,
    due: Instant,
    every: Option<Duration>,
    func: Function,
}

pub struct Stream {
    pub fd: OwnedFd,
    pub buffer: Vec<u8>,
    pub handler: Option<Function>,
    pub open: bool,
    /// Given as it is read, in pieces, and not as lines: output that is not text.
    pub raw: bool,
}

impl Stream {
    /// The stream is over, or is no longer read: what is left in the buffer (the last line, which
    /// has no end) goes to the handler, and nothing more is read.
    fn finish(&mut self, out: &mut Vec<Vec<u8>>) {
        self.open = false;
        let rest = std::mem::take(&mut self.buffer);
        if !rest.is_empty() && self.handler.is_some() {
            out.push(rest);
        }
    }
}

pub struct Child {
    pub id: u64,
    pub process: Process,
    pub stdout: Stream,
    pub stderr: Stream,
    pub on_exit: Option<Function>,
    /// How many bytes the two streams together may say before the program is ended (`max_output`).
    pub limit: Option<usize>,
    /// How many bytes the streams have said so far, line ends included.
    pub produced: usize,
    /// The program said more than `limit` and was ended for it.
    pub overflowed: bool,
    /// When the program was ended by force. Its streams are given a moment to say what they have and
    /// then no longer waited for: a program it started that went into a group of its own keeps a
    /// pipe open, and the app must not wait for that.
    pub killed_at: Option<Instant>,
}

impl Child {
    /// Sends a signal to the program and to everything it started: it runs in a group of its own,
    /// so that a shell with a pipeline in it, or a program with children, is ended whole and not
    /// only at its head.
    pub fn signal(&self, signal: libc::c_int) {
        signal_group(self.process.id(), signal);
    }
}

/// A signal to the group of processes whose leader is `pid`, which is a program this one started
/// and has not yet seen end (a group's number is not taken again while one of its members lives).
fn signal_group(pid: u32, signal: libc::c_int) {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return;
    };
    // SAFETY: a signal to a group of processes that this one started.
    unsafe {
        libc::kill(-pid, signal);
    }
}

/// A program that ended: what to tell the app, its exit code, the signal that ended it, and whether it
/// was ended for saying too much.
type Finished = (Option<Function>, Option<i32>, Option<i32>, bool);

/// How long the streams of a program that was ended by force are waited for.
const KILLED_GRACE: Duration = Duration::from_millis(250);

/// How much of one pipe is read in one turn. The loop then goes back to waiting and to the app, so
/// that a program that says more than the app can follow does not fill the memory with it.
const TURN_BYTES: usize = 256 * 1024;

/// The `serve` that is running: what its view's calls and topics are in Lua.
pub struct ServeRun {
    pub calls: HashMap<String, Function>,
    pub topics: HashMap<String, Function>,
    pub done: Function,
    /// The thread the library's `serve` runs on.
    pub thread: Option<JoinHandle<()>>,
}

pub struct Core {
    pub events: Events,
    inbox: Inbox,
    pub files: Files,
    pub args: Vec<String>,
    pub name: String,
    pub log: Log,
    pub control: ViewControl,
    pub plan: Arc<Mutex<Option<ServePlan>>>,
    /// `pencere dev` has asked for the app to start again: nothing of it runs from now on.
    pub restart: Arc<AtomicBool>,
    /// What the app finds around it: its variables, its directory, whether it has a terminal.
    pub world: Rc<World>,
    /// Where the app's view goes instead of the terminal, when it is handed over.
    pub hand: Option<Sender<Handed>>,
    me: Weak<Core>,
    timers: RefCell<Vec<Timer>>,
    next_id: Cell<u64>,
    pub children: RefCell<Vec<Child>>,
    /// The reading of standard input, while there is one.
    pub stdin: RefCell<Option<super::stdin::Stdin>>,
    pub serve: RefCell<Option<ServeRun>>,
    pub sub_ends: RefCell<HashMap<u64, Vec<Function>>>,
    /// The directories being watched, by number: what to call, and the watch itself.
    pub watches: RefCell<HashMap<u64, (Function, super::watch::Watcher)>>,
    /// The `task` function of the prelude.
    task: RefCell<Option<Function>>,
    exit: RefCell<Option<Exit>>,
    /// What a shell would call the app if it ended now: that of the last view, 128 and a signal
    /// number if a signal ended it.
    pub status: Cell<i32>,
    lua: Lua,
}

impl Core {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        lua: Lua,
        events: Events,
        inbox: Inbox,
        files: Files,
        args: Vec<String>,
        name: String,
        log: Log,
        control: ViewControl,
        plan: Arc<Mutex<Option<ServePlan>>>,
        restart: Arc<AtomicBool>,
        me: Weak<Core>,
        world: Rc<World>,
        hand: Option<Sender<Handed>>,
    ) -> Self {
        Self {
            events,
            inbox,
            files,
            args,
            name,
            log,
            control,
            plan,
            restart,
            world,
            hand,
            me,
            timers: RefCell::new(Vec::new()),
            next_id: Cell::new(1),
            children: RefCell::new(Vec::new()),
            stdin: RefCell::new(None),
            serve: RefCell::new(None),
            sub_ends: RefCell::new(HashMap::new()),
            watches: RefCell::new(HashMap::new()),
            task: RefCell::new(None),
            exit: RefCell::new(None),
            status: Cell::new(0),
            lua,
        }
    }

    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    /// A weak hold on this loop, for a handle that must not keep it alive.
    pub fn weak(&self) -> Weak<Core> {
        Weak::clone(&self.me)
    }

    /// This loop, shared: what Lua holds on to when it holds a handle.
    pub fn rc(&self) -> Rc<Core> {
        // The loop lives in an `Rc` from its making to its end.
        self.me
            .upgrade()
            .unwrap_or_else(|| unreachable!("the loop is in an Rc"))
    }

    pub fn id(&self) -> u64 {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        id
    }

    pub fn set_task(&self, task: Function) {
        *self.task.borrow_mut() = Some(task);
    }

    /// The app is over, unless it already ended some other way first.
    pub fn end(&self, exit: Exit) {
        let mut slot = self.exit.borrow_mut();
        if slot.is_none() {
            *slot = Some(exit);
        }
    }

    pub fn exit(&self) -> Option<Exit> {
        self.exit.borrow().clone()
    }

    // ---- running Lua -----------------------------------------------------------------

    /// Runs `func` as a task (a coroutine the runtime resumes when what it waits for has come),
    /// with `done` and `fail` for how it ends, if it has any.
    pub fn run_task(&self, func: Function, handlers: Value, args: Vec<Value>) {
        let Some(task) = self.task.borrow().clone() else {
            return;
        };
        let mut all = vec![Value::Function(func), handlers];
        all.extend(args);
        if let Err(why) = task.call::<()>(MultiValue::from_iter(all)) {
            self.log.error(&format!("pencere: {why}"));
        }
    }

    /// Runs a callback of the app, whose end nobody waits for.
    pub fn callback(&self, func: Function, args: Vec<Value>) {
        self.run_task(func, Value::Nil, args);
    }

    // ---- timers ----------------------------------------------------------------------

    pub fn add_timer(&self, after: Duration, every: Option<Duration>, func: Function) -> u64 {
        let id = self.id();
        self.timers.borrow_mut().push(Timer {
            id,
            due: Instant::now() + after,
            every,
            func,
        });
        id
    }

    pub fn cancel_timer(&self, id: u64) -> bool {
        let mut timers = self.timers.borrow_mut();
        let before = timers.len();
        timers.retain(|timer| timer.id != id);
        timers.len() != before
    }

    pub fn has_timer(&self, id: u64) -> bool {
        self.timers.borrow().iter().any(|timer| timer.id == id)
    }

    fn fire_timers(&self) {
        let now = Instant::now();
        let mut due = Vec::new();
        {
            let mut timers = self.timers.borrow_mut();
            timers.sort_by_key(|timer| timer.due);
            let mut index = 0;
            while let Some(timer) = timers.get_mut(index) {
                if timer.due > now {
                    break;
                }
                due.push(timer.func.clone());
                match timer.every {
                    // A timer that fell behind does not fire for each tick it missed.
                    Some(every) => {
                        timer.due += every;
                        if timer.due <= now {
                            timer.due = now + every;
                        }
                        index += 1;
                    }
                    None => {
                        timers.remove(index);
                    }
                }
            }
        }
        for func in due {
            self.callback(func, Vec::new());
            if self.exit().is_some() {
                return;
            }
        }
    }

    fn next_timer(&self) -> Option<Instant> {
        self.timers.borrow().iter().map(|timer| timer.due).min()
    }

    // ---- the turn --------------------------------------------------------------------

    /// Whether anything is left that could wake the loop.
    pub fn is_waiting(&self) -> bool {
        !self.timers.borrow().is_empty()
            || !self.children.borrow().is_empty()
            || self.stdin.borrow().is_some()
            || !self.watches.borrow().is_empty()
            || self.serve.borrow().is_some()
    }

    /// One turn: wait for something to happen (at most `wait`), and run what it is.
    pub fn turn(&self, wait: Duration) {
        let mut fds = vec![libc::pollfd {
            fd: self.inbox.fd(),
            events: libc::POLLIN,
            revents: 0,
        }];
        let mut watched: Vec<(u64, bool)> = Vec::new();
        let mut reaping = false;
        for child in self.children.borrow().iter() {
            for (stream, is_err) in [(&child.stdout, false), (&child.stderr, true)] {
                if stream.open {
                    fds.push(libc::pollfd {
                        fd: stream.fd.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    });
                    watched.push((child.id, is_err));
                }
            }
            if (!child.stdout.open && !child.stderr.open) || child.killed_at.is_some() {
                reaping = true;
            }
        }
        // Standard input is the last descriptor, after the processes' pipes.
        let reading = super::stdin::watched(self);
        if let Some(fd) = reading {
            fds.push(libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            });
        }
        let mut timeout = wait;
        if let Some(due) = self.next_timer() {
            timeout = timeout.min(due.saturating_duration_since(Instant::now()));
        }
        if reaping {
            // A process that closed its pipes is waited for by looking; there is nothing to poll.
            timeout = timeout.min(Duration::from_millis(15));
        }
        let millis = i32::try_from(timeout.as_millis() + 1).unwrap_or(i32::MAX);
        // SAFETY: polls descriptors that are ours and alive for the call.
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, millis) };
        if self.restart.load(Ordering::SeqCst) {
            self.end(Exit::Restart);
            return;
        }
        if ready < 0 {
            // A signal woke us (the library's handlers are installed without restart): look again.
            return;
        }
        for event in self.inbox.take() {
            self.handle(event);
            if self.exit().is_some() {
                return;
            }
        }
        for (pollfd, (id, is_err)) in fds.iter().skip(1).zip(&watched) {
            if pollfd.revents != 0 {
                self.read_child(*id, *is_err);
            }
        }
        // A hang-up (the writer is gone) is read as well: the read says the input has ended.
        if reading.is_some() && fds.last().is_some_and(|pollfd| pollfd.revents != 0) {
            super::stdin::read(self);
        }
        self.reap();
        if self.exit().is_none() {
            self.fire_timers();
        }
    }

    fn handle(&self, event: Event) {
        super::serve::handle(self, event);
    }

    // ---- processes -------------------------------------------------------------------

    fn read_child(&self, id: u64, is_err: bool) {
        // The handler and the lines to give it, for each stream that has some.
        let mut batches: Vec<(Function, Vec<Vec<u8>>)> = Vec::new();
        {
            let mut children = self.children.borrow_mut();
            let Some(child) = children.iter_mut().find(|child| child.id == id) else {
                return;
            };
            let signal_pid = child.process.id();
            let Child {
                stdout,
                stderr,
                limit,
                produced,
                overflowed,
                killed_at,
                ..
            } = child;
            let (stream, other) = if is_err {
                (stderr, stdout)
            } else {
                (stdout, stderr)
            };
            let mut lines: Vec<Vec<u8>> = Vec::new();
            let mut ended_for_size = false;
            let mut taken = 0usize;
            let mut chunk = [0u8; 16384];
            while taken < TURN_BYTES {
                // SAFETY: reads into a buffer of ours from a descriptor of ours.
                let got = unsafe {
                    libc::read(
                        stream.fd.as_raw_fd(),
                        chunk.as_mut_ptr().cast(),
                        chunk.len(),
                    )
                };
                if got > 0 {
                    let got = got as usize;
                    taken += got;
                    let mut piece = chunk.get(..got).unwrap_or_default();
                    // Counted as it is read, before anything is made of it (a line, an entry in a
                    // table): what is more than the program may say is not kept.
                    if let Some(limit) = *limit {
                        let room = limit.saturating_sub(*produced);
                        if piece.len() > room {
                            piece = piece.get(..room).unwrap_or_default();
                            ended_for_size = true;
                        }
                    }
                    *produced += piece.len();
                    stream.buffer.extend_from_slice(piece);
                    split_lines(stream, &mut lines);
                    if ended_for_size {
                        break;
                    }
                    continue;
                }
                if got == 0 {
                    stream.finish(&mut lines);
                } else {
                    let error = std::io::Error::last_os_error();
                    if error.kind() == std::io::ErrorKind::Interrupted {
                        continue;
                    }
                    if error.kind() != std::io::ErrorKind::WouldBlock {
                        stream.open = false;
                    }
                }
                break;
            }
            let mut others: Vec<Vec<u8>> = Vec::new();
            if ended_for_size {
                // More than it may say: the program is ended whole, and what it was in the middle of
                // saying is the end of what the app hears.
                *overflowed = true;
                stream.finish(&mut lines);
                other.finish(&mut others);
                signal_group(signal_pid, libc::SIGKILL);
                killed_at.get_or_insert_with(Instant::now);
            }
            if let (false, Some(handler)) = (lines.is_empty(), &stream.handler) {
                batches.push((handler.clone(), lines));
            }
            if let (false, Some(handler)) = (others.is_empty(), &other.handler) {
                batches.push((handler.clone(), others));
            }
        }
        for (handler, lines) in batches {
            for line in lines {
                let text = match self.lua.create_string(&line) {
                    Ok(text) => Value::String(text),
                    Err(_) => continue,
                };
                self.callback(handler.clone(), vec![text]);
                if self.exit().is_some() {
                    return;
                }
            }
        }
    }

    /// Finds the processes that have ended and whose output has been read to its end.
    fn reap(&self) {
        let mut finished: Vec<Finished> = Vec::new();
        {
            let mut children = self.children.borrow_mut();
            children.retain_mut(|child| {
                let waiting_for_streams = child.stdout.open || child.stderr.open;
                let given_up = child
                    .killed_at
                    .is_some_and(|at| at.elapsed() >= KILLED_GRACE);
                if waiting_for_streams && !given_up {
                    return true;
                }
                match child.process.try_wait() {
                    Ok(Some(status)) => {
                        use std::os::unix::process::ExitStatusExt as _;
                        super::ending::remove(child.process.id());
                        finished.push((
                            child.on_exit.take(),
                            status.code(),
                            status.signal(),
                            child.overflowed,
                        ));
                        false
                    }
                    Ok(None) => true,
                    Err(_) => {
                        super::ending::remove(child.process.id());
                        finished.push((child.on_exit.take(), None, None, child.overflowed));
                        false
                    }
                }
            });
        }
        for (on_exit, code, signal, overflowed) in finished {
            if let Some(on_exit) = on_exit {
                let number = |n: Option<i32>| n.map_or(Value::Nil, |n| Value::Number(f64::from(n)));
                self.callback(
                    on_exit,
                    vec![number(code), number(signal), Value::Boolean(overflowed)],
                );
                if self.exit().is_some() {
                    return;
                }
            }
        }
    }

    /// The app is over: the processes it started are asked to end, and ended if they do not.
    pub fn stop_children(&self) {
        let children = std::mem::take(&mut *self.children.borrow_mut());
        if children.is_empty() {
            return;
        }
        let mut waiting: Vec<Process> = Vec::new();
        for mut child in children {
            child.signal(libc::SIGTERM);
            if child.process.try_wait().ok().flatten().is_none() {
                waiting.push(child.process);
            } else {
                super::ending::remove(child.process.id());
            }
        }
        let until = Instant::now() + Duration::from_millis(400);
        while !waiting.is_empty() && Instant::now() < until {
            waiting.retain_mut(|process| {
                let running = process.try_wait().ok().flatten().is_none();
                if !running {
                    super::ending::remove(process.id());
                }
                running
            });
            std::thread::sleep(Duration::from_millis(10));
        }
        for mut process in waiting {
            signal_group(process.id(), libc::SIGKILL);
            let _ = process.wait();
            super::ending::remove(process.id());
        }
    }

    /// Lets go of everything Lua holds, so that the Lua state can be dropped: the functions the
    /// module is made of hold this loop, and this loop holds the Lua state they are in; `pencere
    /// dev` runs an app many times in one process, and each run must be gone when the next begins.
    pub fn clear(&self) {
        let globals = self.lua.globals();
        for name in ["print", "warn", "require"] {
            let _ = globals.raw_set(name, Value::Nil);
        }
        self.timers.borrow_mut().clear();
        // Stopped (the threads of the watches are waited for) before the functions they call go.
        let watches: Vec<_> = self.watches.borrow_mut().drain().collect();
        drop(watches);
        *self.serve.borrow_mut() = None;
        *self.stdin.borrow_mut() = None;
        self.sub_ends.borrow_mut().clear();
        *self.task.borrow_mut() = None;
        for child in self.children.borrow_mut().iter_mut() {
            child.stdout.handler = None;
            child.stderr.handler = None;
            child.on_exit = None;
        }
        // Collected twice: what the first collection finalizes is freed by the second.
        let _ = self.lua.gc_collect();
        let _ = self.lua.gc_collect();
    }
}

/// The lines in a stream's buffer, taken out of it.
fn split_lines(stream: &mut Stream, out: &mut Vec<Vec<u8>>) {
    if stream.handler.is_none() {
        // Nobody is told, and nothing is kept.
        stream.buffer.clear();
        return;
    }
    if stream.raw {
        let piece = std::mem::take(&mut stream.buffer);
        if !piece.is_empty() {
            out.push(piece);
        }
        return;
    }
    // A line that never ends is not kept for ever.
    const LONGEST: usize = 1 << 20;
    // Looked through once, and what was taken out is dropped from the buffer once: dropping it a line
    // at a time moves the rest of the buffer every time.
    let mut from = 0usize;
    while let Some(rest) = stream.buffer.get(from..) {
        let (end, next) = match rest.iter().position(|byte| *byte == b'\n') {
            Some(at) => (from + at, from + at + 1),
            None if rest.len() >= LONGEST => (stream.buffer.len(), stream.buffer.len()),
            None => break,
        };
        let mut line = stream.buffer.get(from..end).unwrap_or_default().to_vec();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        out.push(line);
        from = next;
    }
    stream.buffer.drain(..from);
}
