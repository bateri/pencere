//! A pty with the test on the master end, and what the tests of the program ask of it.

// A test fails by panicking, and the helpers here are the test's.
#![allow(
    dead_code,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::VecDeque;
use std::os::fd::RawFd;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pencere::{Bundle, Error, File, Options, Outcome, Terminal, View};
use pencere_wire::frame::{
    Direction, Event, Ids, Message as Frame, Outgoing, Split, StreamDecoder, encode,
};
use serde_json::{Value, json};

pub const TOKEN: &str = "q7Vh2sB0xJm9kLw4TzN1cA";
pub const TOKEN_2: &str = "Zk3mP8rD5uYb1Xe6HqS0vg";
pub const REPLY: &[u8] = b"\x1b[?62;4c";
pub const WITHIN: Duration = Duration::from_secs(5);

/// One test has the process's signals and the terminal at a time.
pub fn one_at_a_time() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn termios_of(fd: RawFd) -> libc::termios {
    // SAFETY: tcgetattr fills a termios of ours.
    unsafe {
        let mut termios: libc::termios = std::mem::zeroed();
        assert_eq!(libc::tcgetattr(fd, &mut termios), 0, "tcgetattr");
        termios
    }
}

/// A pty, with the test on the master end and the program to be given the slave.
pub struct Pane {
    pub master: RawFd,
    pub slave: RawFd,
    pub decoder: StreamDecoder,
    pub ids: Ids,
    pub frames: VecDeque<Frame>,
    /// Everything the program wrote, frames and the rest.
    pub seen: Vec<u8>,
}

impl Pane {
    pub fn new() -> Self {
        let (mut master, mut slave) = (-1, -1);
        // SAFETY: openpty fills two descriptors we then own.
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            },
            0
        );
        // SAFETY: the master is ours.
        unsafe {
            let flags = libc::fcntl(master, libc::F_GETFL);
            libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
        Self {
            master,
            slave,
            decoder: StreamDecoder::new(Direction::ToHost),
            ids: Ids::new(),
            frames: VecDeque::new(),
            seen: Vec::new(),
        }
    }

    pub fn terminal(&self) -> Terminal {
        Terminal::from_descriptors(self.slave, self.slave)
    }

    /// Reads what the program wrote, for up to `within`; whether there was something.
    pub fn read_some(&mut self, within: Duration) -> bool {
        let mut poll = libc::pollfd {
            fd: self.master,
            events: libc::POLLIN,
            revents: 0,
        };
        let millis = i32::try_from(within.as_millis()).unwrap_or(i32::MAX);
        // SAFETY: one pollfd.
        if unsafe { libc::poll(&mut poll, 1, millis) } <= 0 {
            return false;
        }
        let mut buffer = [0u8; 4096];
        // SAFETY: reads into a buffer of ours.
        let got = unsafe { libc::read(self.master, buffer.as_mut_ptr().cast(), buffer.len()) };
        if got <= 0 {
            return false;
        }
        let bytes = &buffer[..got as usize];
        self.seen.extend_from_slice(bytes);
        let mut events = Vec::new();
        self.decoder.feed(bytes, &mut events);
        for event in events {
            if let Event::Message(message) = event {
                self.frames.push_back(message);
            }
        }
        true
    }

    /// The next frame the program wrote, if one comes within `within`.
    pub fn next_within(&mut self, within: Duration) -> Option<Frame> {
        let until = Instant::now() + within;
        loop {
            if let Some(frame) = self.frames.pop_front() {
                return Some(frame);
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            self.read_some(left.min(Duration::from_millis(50)));
        }
    }

    /// The next frame, which has to be of `kind`.
    pub fn expect(&mut self, kind: &str) -> Frame {
        let frame = self
            .next_within(WITHIN)
            .unwrap_or_else(|| panic!("no {kind}; the program wrote {:?}", self.text()));
        assert_eq!(frame.kind, kind, "{frame:?}");
        frame
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.seen).into_owned()
    }

    /// Writes bytes into the pane's input, as the terminal does: a few at a time, waiting while
    /// the tty has no room.
    pub fn write_raw(&mut self, mut bytes: &[u8]) {
        let until = Instant::now() + WITHIN;
        while !bytes.is_empty() {
            let piece = &bytes[..bytes.len().min(256)];
            // SAFETY: writes from a slice of ours.
            let wrote = unsafe { libc::write(self.master, piece.as_ptr().cast(), piece.len()) };
            if wrote > 0 {
                bytes = &bytes[wrote as usize..];
            } else {
                assert!(Instant::now() < until, "the program does not read");
                // Waits for the room the tty makes as the program reads.
                let mut poll = libc::pollfd {
                    fd: self.master,
                    events: libc::POLLOUT,
                    revents: 0,
                };
                // SAFETY: one pollfd.
                unsafe { libc::poll(&mut poll, 1, 5) };
            }
        }
    }

    /// Writes a message of the terminal.
    pub fn send(&mut self, kind: &str, re: Option<u32>, payload: &Value) -> u32 {
        let id = self.ids.next_id();
        let payload = pencere_wire::json::to_wire(payload).expect("JSON");
        let mut bytes = Vec::new();
        encode(
            Direction::ToProgram,
            &Outgoing {
                kind,
                id,
                re,
                digest: None,
                payload: &payload,
            },
            Split::Fill,
            &mut bytes,
        )
        .expect("a frame");
        self.write_raw(&bytes);
        id
    }

    /// The `welcome` that answers the `hello` `re`.
    pub fn welcome(&mut self, re: u32, token: &str, views: &Value) {
        self.send(
            "welcome",
            Some(re),
            &json!({
                "version": 1,
                "token": token,
                "host": { "name": "test", "version": "0" },
                "features": ["dismiss"],
                "views": views,
            }),
        );
    }

    /// Takes the `hello`, answers it, and takes the `open`: the frame of the `open`, and the
    /// text of the escape that switches the screen is among what was seen.
    pub fn open_a_view(&mut self, views: &Value) -> (u32, Frame) {
        let hello = self.expect("hello");
        let modes = termios_of(self.master);
        assert_eq!(modes.c_lflag & libc::ICANON, 0, "canonical");
        assert_eq!(modes.c_lflag & libc::ECHO, 0, "echo");
        assert_ne!(modes.c_lflag & libc::ISIG, 0, "signals stay on");
        self.welcome(hello.id, TOKEN, views);
        let open = self.expect("open");
        (hello.id, open)
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        // SAFETY: closes what `new` opened.
        unsafe {
            libc::close(self.master);
            libc::close(self.slave);
        }
    }
}

pub fn page() -> Bundle {
    Bundle::new(
        "index.html",
        [
            File::new("index.html", &b"<p>hello</p>"[..]),
            File::new("app.js", &b"let a = 1;"[..]),
        ],
    )
    .expect("a bundle")
}

pub fn web_views() -> Value {
    json!([{ "kind": "web", "versions": ["1.0"] }])
}

pub fn modes_are_back(pane: &Pane) {
    let modes = termios_of(pane.slave);
    assert_ne!(modes.c_lflag & libc::ICANON, 0, "canonical");
    assert_ne!(modes.c_lflag & libc::ECHO, 0, "echo");
}

/// How many bytes the tty has queued for a reader that has not read them. In canonical mode
/// an unfinished line is not counted, so the tty is looked at in the modes of a program for a
/// moment.
pub fn queued_input(pane: &Pane) -> libc::c_int {
    let saved = termios_of(pane.slave);
    let mut raw = saved;
    raw.c_lflag &= !libc::ICANON;
    let mut queued: libc::c_int = 0;
    // SAFETY: tcsetattr reads a termios of ours; FIONREAD fills an int.
    unsafe {
        libc::tcsetattr(pane.slave, libc::TCSANOW, &raw);
        libc::ioctl(pane.slave, libc::FIONREAD, &mut queued);
        libc::tcsetattr(pane.slave, libc::TCSANOW, &saved);
    }
    queued
}

pub fn interrupt() {
    // SAFETY: the handlers of `serve` are installed; this is the ⌃C.
    unsafe {
        libc::raise(libc::SIGINT);
    }
}

pub fn digest_of(bytes: &[u8]) -> String {
    pencere_wire::bundle::Digest::of(bytes).to_string()
}

/// Runs `serve` on a thread of the test.
pub fn serve_on(
    terminal: Terminal,
    options: impl FnOnce(Options) -> Options + Send + 'static,
) -> JoinHandle<Result<Outcome, Error>> {
    std::thread::spawn(move || {
        let base = Options::new()
            .view(View::web(page()))
            .program("test")
            .title("Test");
        pencere::serve(options(base).terminal(terminal))
    })
}

/// How `serve` ended, once it has.
pub fn finished(handle: JoinHandle<Result<Outcome, Error>>) -> Outcome {
    let until = Instant::now() + WITHIN;
    while !handle.is_finished() {
        assert!(Instant::now() < until, "serve does not return");
        std::thread::sleep(Duration::from_millis(5));
    }
    handle
        .join()
        .expect("serve does not panic")
        .expect("serve is not refused")
}

/// Runs `call` on a thread of the test, so that the test can be the terminal while it waits.
pub fn asking<T: Send + 'static>(call: impl FnOnce() -> T + Send + 'static) -> JoinHandle<T> {
    std::thread::spawn(call)
}

/// What `asking`'s call came to, once it has.
pub fn joined<T>(handle: JoinHandle<T>) -> T {
    let until = Instant::now() + WITHIN;
    while !handle.is_finished() {
        assert!(Instant::now() < until, "the call does not return");
        std::thread::sleep(Duration::from_millis(5));
    }
    handle.join().expect("the call does not panic")
}

/// A view is open and the terminal has taken it (`need` with nothing missing).
pub fn shown(pane: &mut Pane) -> Frame {
    let (_, open) = pane.open_a_view(&web_views());
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    open
}

/// Waits until `condition` holds.
pub fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let until = Instant::now() + WITHIN;
    while !condition() {
        assert!(Instant::now() < until, "{what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
