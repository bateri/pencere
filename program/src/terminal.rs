//! The terminal a program shows a view on: the modes it is in while the view is up, the
//! alternate screen, the signals that end the program or stop it, and the way back on every
//! path there is.
//!
//! What the protocol asks of a program (specification §4.9, §6.5, §10.3, §10.4): before it
//! speaks, the tty is non-canonical with echo off and **signals on** (so that ⌃C reaches the
//! program as SIGINT); when it is done, on every exit path it can run code on, the modes it
//! changed come back, the alternate screen is left, and what the terminal queued for the program
//! and it did not read is thrown away.
//!
//! **The input is read without ever blocking.** A signal character (⌃C) flushes the tty's input
//! queue (Appendix B), so a read that `poll` promised bytes for can find the queue emptied under
//! it; with the usual `VMIN` of 1 such a read waits for a byte that may never come, and the
//! signal that should end the program is no use to a program stuck in it. The tty is given
//! `VMIN` 0 instead — a read returns what there is, even nothing.
//!
//! **Signals reach the program through a pipe.** A handler does three things: it records the
//! signal, it writes a byte into the pipe the event loop polls, and it returns. Nothing else is
//! safe in a handler, and nothing else is needed: the loop decides what the signal means. The
//! handlers are installed without `SA_RESTART`, so a call a signal interrupts comes back.
//!
//! **Everything that gives the terminal back is one function** (`restore`), and it is safe to
//! call twice: the normal end, the panic hook and the `atexit` handler all call it, and the
//! first to run does the work.

use std::cell::UnsafeCell;
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::{Duration, Instant};

/// Switches to the alternate screen.
pub(crate) const ENTER_ALTERNATE: &[u8] = b"\x1b[?1049h";
/// Leaves the alternate screen.
pub(crate) const LEAVE_ALTERNATE: &[u8] = b"\x1b[?1049l";
/// The primary device attributes query: the fence after a `hello` (specification §5.2).
pub(crate) const DEVICE_ATTRIBUTES: &[u8] = b"\x1b[c";

/// How long what a program says while it leaves may wait for a terminal that is not reading.
pub(crate) const FAREWELL: Duration = Duration::from_millis(400);

/// How much is written at once when a write must not wait: a tty that is ready for writing has
/// room for this much, so the write finds the room it was promised.
const PIECE: usize = 256;

/// The signals that end the program, and the one that stops it.
const ENDING: [libc::c_int; 4] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT];
const STOPPING: libc::c_int = libc::SIGTSTP;

/// The terminal a program shows its view on.
///
/// The program writes its frames to the output and reads the terminal's answers from the input;
/// both have to be the tty of the pane. By default they are the program's standard input and
/// output. A program whose standard streams carry data (a filter in a pipeline) names the
/// terminal it talks on, [`Terminal::controlling`] (specification §13).
#[derive(Debug)]
pub struct Terminal {
    input: RawFd,
    output: RawFd,
    /// The descriptor opened for [`Terminal::controlling`], closed with the terminal.
    _owned: Option<OwnedFd>,
}

impl Terminal {
    /// The program's standard input and output.
    #[must_use]
    pub fn standard() -> Self {
        Self {
            input: libc::STDIN_FILENO,
            output: libc::STDOUT_FILENO,
            _owned: None,
        }
    }

    /// The controlling terminal of the process (`/dev/tty`), whatever the standard streams are:
    /// for a program in a pipeline, whose standard input and output are its data.
    ///
    /// # Errors
    ///
    /// When the process has no controlling terminal.
    pub fn controlling() -> io::Result<Self> {
        let fd = open_tty(c"/dev/tty")?;
        // macOS cannot `poll` /dev/tty (it answers POLLNVAL, which the loop would read as a hang-up),
        // but it can poll the device that /dev/tty stands for: that is opened, by its own name, when
        // it can be found. The two are the same terminal, with the same modes.
        let fd = match device_name() {
            Some(name) => open_tty(&name).unwrap_or(fd),
            None => fd,
        };
        Ok(Self {
            input: fd.as_raw_fd(),
            output: fd.as_raw_fd(),
            _owned: Some(fd),
        })
    }

    /// A terminal that reads `input` and writes `output`, two descriptors the caller keeps open
    /// for as long as the terminal is served.
    #[must_use]
    pub fn from_descriptors(input: RawFd, output: RawFd) -> Self {
        Self {
            input,
            output,
            _owned: None,
        }
    }

    pub(crate) fn input(&self) -> RawFd {
        self.input
    }

    pub(crate) fn output(&self) -> RawFd {
        self.output
    }

    /// Whether both ends are a terminal.
    pub(crate) fn is_tty(&self) -> bool {
        // SAFETY: isatty takes a descriptor and touches nothing else.
        unsafe { libc::isatty(self.input) == 1 && libc::isatty(self.output) == 1 }
    }
}

/// Opens a terminal to read and write, without making it the process's controlling terminal.
fn open_tty(path: &std::ffi::CStr) -> io::Result<OwnedFd> {
    // SAFETY: opens a path we name; the descriptor is ours and nothing else holds it.
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `open` returned a descriptor that nothing else owns.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// The device the process's controlling terminal is (`/dev/ttys004`), by the name the kernel gives
/// it: what `/dev/tty` stands for, which macOS can open but not `poll`.
#[cfg(target_os = "macos")]
fn device_name() -> Option<std::ffi::CString> {
    // SAFETY: `proc_bsdinfo` is plain data; the call fills it, for this process, or fails.
    let device = unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let size = libc::c_int::try_from(std::mem::size_of::<libc::proc_bsdinfo>()).ok()?;
        let got = libc::proc_pidinfo(
            libc::getpid(),
            libc::PROC_PIDTBSDINFO,
            0,
            (&raw mut info).cast(),
            size,
        );
        if got != size || info.e_tdev == u32::MAX {
            return None;
        }
        info.e_tdev
    };
    // SAFETY: `devname` gives a name in a buffer of its own, which is copied at once; it is called
    // here and nowhere else.
    let name = unsafe {
        let name = libc::devname(libc::dev_t::try_from(device).ok()?, libc::S_IFCHR);
        if name.is_null() {
            return None;
        }
        std::ffi::CStr::from_ptr(name).to_bytes().to_vec()
    };
    // A device without a name is shown as `#C:` and numbers: that is no path.
    if name.is_empty() || name.starts_with(b"#") {
        return None;
    }
    let mut path = b"/dev/".to_vec();
    path.extend_from_slice(&name);
    std::ffi::CString::new(path).ok()
}

/// Other systems poll /dev/tty as it is.
#[cfg(not(target_os = "macos"))]
fn device_name() -> Option<std::ffi::CString> {
    None
}

// ---- what the handlers and the hooks share --------------------------------------------

/// A termios in a static: written once before `ARMED` is published and only read after.
struct Slot(UnsafeCell<MaybeUninit<libc::termios>>);

// SAFETY: written only by the one thread that holds `IN_USE`, before `ARMED` is stored with
// release ordering, and read after an acquire load of `ARMED`.
unsafe impl Sync for Slot {}

static SAVED: Slot = Slot(UnsafeCell::new(MaybeUninit::uninit()));
static RAW: Slot = Slot(UnsafeCell::new(MaybeUninit::uninit()));
static ARMED: AtomicBool = AtomicBool::new(false);
static IN_USE: AtomicBool = AtomicBool::new(false);
static ALTERNATE: AtomicBool = AtomicBool::new(false);
static INPUT_FD: AtomicI32 = AtomicI32::new(-1);
static OUTPUT_FD: AtomicI32 = AtomicI32::new(-1);
static WAKE_FD: AtomicI32 = AtomicI32::new(-1);
static SIGNALLED: AtomicI32 = AtomicI32::new(0);
static STOPPED: AtomicBool = AtomicBool::new(false);

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
fn errno_location() -> *mut libc::c_int {
    // SAFETY: returns the address of this thread's errno.
    unsafe { libc::__error() }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn errno_location() -> *mut libc::c_int {
    // SAFETY: returns the address of this thread's errno.
    unsafe { libc::__errno_location() }
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "linux",
    target_os = "android"
)))]
fn errno_location() -> *mut libc::c_int {
    std::ptr::null_mut()
}

extern "C" fn on_signal(signal: libc::c_int) {
    let location = errno_location();
    // SAFETY: a thread's errno is its own; null is left alone.
    let saved = unsafe { location.as_ref().copied() };
    if signal == STOPPING {
        STOPPED.store(true, Ordering::SeqCst);
    } else {
        let _ = SIGNALLED.compare_exchange(0, signal, Ordering::SeqCst, Ordering::SeqCst);
    }
    wake_the_loop();
    if let Some(saved) = saved {
        // SAFETY: restores the value `write` may have changed, in this thread's own errno.
        unsafe { *location = saved };
    }
}

/// Writes a byte into the pipe the loop polls: async-signal-safe.
fn wake_the_loop() {
    let fd = WAKE_FD.load(Ordering::SeqCst);
    if fd >= 0 {
        // SAFETY: write is async-signal-safe; a full pipe or a closed one is no harm, the loop
        // is awake already or gone.
        unsafe {
            libc::write(fd, [1u8].as_ptr().cast(), 1);
        }
    }
}

/// The signal that ended the program, once.
pub(crate) fn take_signal() -> Option<libc::c_int> {
    match SIGNALLED.swap(0, Ordering::SeqCst) {
        0 => None,
        signal => Some(signal),
    }
}

/// Whether the person asked to stop the program (⌃Z), once.
pub(crate) fn take_stop() -> bool {
    STOPPED.swap(false, Ordering::SeqCst)
}

/// Takes the terminal for a command the program lends it to while no `serve` has it: whether it
/// was free. Until [`release`], a `serve` is told the terminal is taken (`Error::Busy`).
pub(crate) fn claim() -> bool {
    IN_USE
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

/// The terminal [`claim`] took is free again.
pub(crate) fn release() {
    IN_USE.store(false, Ordering::SeqCst);
}

/// Writes `bytes` to `fd` without ever waiting for room longer than `within`, a few bytes at a
/// time: the way a program that has been asked to end says its last words. A terminal that is
/// not reading (the pane's reader waits for its view to catch up) must not keep it from leaving.
pub(crate) fn write_within(fd: RawFd, mut bytes: &[u8], within: Duration) -> io::Result<()> {
    let until = Instant::now() + within;
    while !bytes.is_empty() {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        if !wait_for_room(fd, left)? {
            continue;
        }
        let wrote = write_piece(fd, bytes)?;
        bytes = bytes.get(wrote..).unwrap_or_default();
    }
    Ok(())
}

/// Waits up to `within` (and no more than a moment, so that a signal is seen) for the tty to
/// have room to write: whether it has.
pub(crate) fn wait_for_room(fd: RawFd, within: Duration) -> io::Result<bool> {
    let mut poll = libc::pollfd {
        fd,
        events: libc::POLLOUT,
        revents: 0,
    };
    let wait = i32::try_from(within.as_millis().min(100)).unwrap_or(100);
    // SAFETY: one pollfd, owned here.
    let ready = unsafe { libc::poll(&mut poll, 1, wait) };
    if ready < 0 {
        let error = io::Error::last_os_error();
        return if error.kind() == io::ErrorKind::Interrupted {
            Ok(false)
        } else {
            Err(error)
        };
    }
    Ok(ready > 0)
}

/// Writes the first few bytes of `bytes` to `fd`: how many were taken, 0 when the tty took none
/// (it was busy, or a signal interrupted the call).
pub(crate) fn write_piece(fd: RawFd, bytes: &[u8]) -> io::Result<usize> {
    let piece = bytes.len().min(PIECE);
    // SAFETY: writes from a slice we own to a descriptor the caller keeps open.
    let wrote = unsafe { libc::write(fd, bytes.as_ptr().cast(), piece) };
    if wrote >= 0 {
        return Ok(wrote as usize);
    }
    let error = io::Error::last_os_error();
    if matches!(
        error.kind(),
        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
    ) {
        Ok(0)
    } else {
        Err(error)
    }
}

/// Throws away what the terminal has queued for this program and it has not read (`tcflush`
/// with `TCIFLUSH`): so that what the host wrote and the program did not read is not left for
/// whatever reads the tty next.
pub(crate) fn discard_input(fd: RawFd) {
    // SAFETY: tcflush takes a descriptor and a queue selector and touches nothing else.
    unsafe {
        libc::tcflush(fd, libc::TCIFLUSH);
    }
}

fn termios_of(which: &Slot) -> libc::termios {
    // SAFETY: only called while `ARMED` is true, or by the `Taken` whose `take` wrote the slots
    // before it was made: after the slot was written (see `Slot`).
    unsafe { (*which.0.get()).assume_init() }
}

fn set_modes(fd: RawFd, termios: &libc::termios) -> io::Result<()> {
    // SAFETY: tcsetattr reads a termios of ours.
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, termios) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Gives the terminal back as it was found: the screen first, then the modes. The first call
/// does it; a second does nothing, so that the end of the program, the hook of a panic and the
/// handler of `exit` can all call it.
pub(crate) fn restore() {
    if !ARMED.swap(false, Ordering::SeqCst) {
        return;
    }
    let output = OUTPUT_FD.load(Ordering::SeqCst);
    if ALTERNATE.swap(false, Ordering::SeqCst) {
        let _ = write_within(output, LEAVE_ALTERNATE, FAREWELL);
    }
    let _ = set_modes(INPUT_FD.load(Ordering::SeqCst), &termios_of(&SAVED));
}

extern "C" fn restore_at_exit() {
    restore();
}

/// Notes that the alternate screen is on, so that every path back leaves it.
pub(crate) fn note_alternate(on: bool) {
    ALTERNATE.store(on, Ordering::SeqCst);
}

// ---- the pipe -------------------------------------------------------------------------

/// A pipe that wakes the event loop from a signal handler or from another thread.
#[derive(Debug)]
pub(crate) struct Pipe {
    read: OwnedFd,
    write: OwnedFd,
}

impl Pipe {
    pub(crate) fn new() -> io::Result<Self> {
        let mut ends = [0 as libc::c_int; 2];
        // SAFETY: `pipe` fills two descriptors we then own.
        if unsafe { libc::pipe(ends.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: two fresh descriptors that nothing else owns.
        let (read, write) =
            unsafe { (OwnedFd::from_raw_fd(ends[0]), OwnedFd::from_raw_fd(ends[1])) };
        for fd in [&read, &write] {
            use std::os::fd::AsRawFd;
            let raw = fd.as_raw_fd();
            // SAFETY: fcntl on a descriptor we own.
            unsafe {
                libc::fcntl(raw, libc::F_SETFD, libc::FD_CLOEXEC);
                let flags = libc::fcntl(raw, libc::F_GETFL);
                libc::fcntl(raw, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }
        Ok(Self { read, write })
    }

    pub(crate) fn read_end(&self) -> RawFd {
        std::os::fd::AsRawFd::as_raw_fd(&self.read)
    }

    pub(crate) fn write_end(&self) -> RawFd {
        std::os::fd::AsRawFd::as_raw_fd(&self.write)
    }

    /// Wakes whoever polls the other end. Never blocks, and a full pipe is as good as a byte.
    pub(crate) fn wake(&self) {
        // SAFETY: writes one byte to a descriptor we own.
        unsafe {
            libc::write(self.write_end(), [1u8].as_ptr().cast(), 1);
        }
    }

    /// Empties the pipe.
    pub(crate) fn drain(&self) {
        let mut buffer = [0u8; 64];
        loop {
            // SAFETY: reads into a buffer we own, from a non-blocking descriptor we own.
            let got =
                unsafe { libc::read(self.read_end(), buffer.as_mut_ptr().cast(), buffer.len()) };
            if got <= 0 {
                return;
            }
        }
    }
}

// ---- taking the terminal --------------------------------------------------------------

/// Why the terminal could not be taken.
#[derive(Debug)]
pub(crate) enum TakeError {
    /// Another `serve` has it.
    Busy,
    /// The modes cannot be read or set: the descriptor is not a terminal that can be taken.
    Failed,
}

/// The terminal while the program uses it: in the modes the protocol asks, with the handlers of
/// the signals installed. Dropping it gives it back.
pub(crate) struct Taken {
    input: RawFd,
    previous: Vec<(libc::c_int, libc::sigaction)>,
    given_back: bool,
    /// The terminal went away: its `SIGHUP` may still be on its way.
    hung_up: bool,
}

impl Taken {
    /// Puts the terminal in the modes the protocol asks (§4.9) and listens for the signals.
    /// `wake` is the pipe the loop polls.
    pub(crate) fn take(terminal: &Terminal, wake: &Pipe) -> Result<Self, TakeError> {
        if IN_USE.swap(true, Ordering::SeqCst) {
            return Err(TakeError::Busy);
        }
        let mut taken = Self {
            input: terminal.input(),
            previous: Vec::new(),
            given_back: false,
            hung_up: false,
        };
        // From here a failure drops `taken`, which undoes what was done.
        let mut saved: libc::termios;
        // SAFETY: tcgetattr/tcsetattr read and write a termios of ours.
        unsafe {
            saved = std::mem::zeroed();
            if libc::tcgetattr(terminal.input(), &mut saved) != 0 {
                return Err(TakeError::Failed);
            }
        }
        let mut raw = saved;
        raw.c_lflag &= !(libc::ICANON | libc::ECHO);
        // ISIG stays as it was asked: ⌃C is a SIGINT for the foreground group.
        raw.c_lflag |= libc::ISIG;
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 0;
        set_modes(terminal.input(), &raw).map_err(|_| TakeError::Failed)?;
        // SAFETY: nobody reads the slots before `ARMED` is stored below.
        unsafe {
            *SAVED.0.get() = MaybeUninit::new(saved);
            *RAW.0.get() = MaybeUninit::new(raw);
        }
        INPUT_FD.store(terminal.input(), Ordering::SeqCst);
        OUTPUT_FD.store(terminal.output(), Ordering::SeqCst);
        ALTERNATE.store(false, Ordering::SeqCst);
        SIGNALLED.store(0, Ordering::SeqCst);
        STOPPED.store(false, Ordering::SeqCst);
        WAKE_FD.store(wake.write_end(), Ordering::SeqCst);
        ARMED.store(true, Ordering::SeqCst);

        static AT_EXIT: Once = Once::new();
        AT_EXIT.call_once(|| {
            // SAFETY: registers a function that takes no arguments and that is safe to run at
            // exit (it does nothing when the terminal is not taken).
            unsafe {
                libc::atexit(restore_at_exit);
            }
        });

        taken.install_handlers();
        Ok(taken)
    }

    fn install_handlers(&mut self) {
        for signal in ENDING.into_iter().chain([STOPPING]) {
            // SAFETY: a sigaction of ours with a handler that only touches atomics and a pipe.
            unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = on_signal as *const () as libc::sighandler_t;
                // No `SA_RESTART`: a call a signal interrupts comes back, and the loop looks at
                // what the signal was.
                action.sa_flags = 0;
                libc::sigemptyset(&mut action.sa_mask);
                for other in ENDING.into_iter().chain([STOPPING]) {
                    libc::sigaddset(&mut action.sa_mask, other);
                }
                let mut old: libc::sigaction = std::mem::zeroed();
                if libc::sigaction(signal, &action, &mut old) == 0 {
                    self.previous.push((signal, old));
                }
            }
        }
    }

    fn restore_handlers(&mut self) {
        for (signal, old) in self.previous.drain(..) {
            // SAFETY: puts back the action that `sigaction` returned.
            unsafe {
                libc::sigaction(signal, &old, std::ptr::null_mut());
            }
        }
    }

    /// The terminal is gone: the program learned it from the tty, and the kernel's `SIGHUP` for
    /// the same hangup may come a moment later. The view has ended for it already, so when the
    /// terminal is given back that signal is ignored rather than left to end the program before
    /// it has done what it does after its view (a draft of what was not saved).
    pub(crate) fn hung_up(&mut self) {
        self.hung_up = true;
    }

    /// Ignores `SIGHUP` from here on.
    fn ignore_hangup() {
        // SAFETY: an action of no handler, for a signal of the process's own.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = libc::SIG_IGN;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(libc::SIGHUP, &action, std::ptr::null_mut());
        }
    }

    /// Gives the terminal back — what the terminal queued and was not read is thrown away if
    /// `discard`, the alternate screen is left, the modes are put back — and only then stops
    /// listening for the signals: a parent that forwards the ⌃C the tty sent (`npm run`) sends a
    /// second one a moment later, and without a handler its default action would end the
    /// process between the writes above. After a hangup `SIGHUP` stays ignored ([`Self::hung_up`]).
    pub(crate) fn give_back(&mut self, discard: bool) {
        if self.given_back {
            return;
        }
        self.given_back = true;
        if discard {
            discard_input(self.input);
        }
        crate::files::discard_parts();
        restore();
        WAKE_FD.store(-1, Ordering::SeqCst);
        // Ignored before the old actions are put back, so that there is no moment between them
        // when the hangup's signal would find its default action.
        if self.hung_up {
            Self::ignore_hangup();
            self.previous.retain(|(signal, _)| *signal != libc::SIGHUP);
        }
        self.restore_handlers();
        IN_USE.store(false, Ordering::SeqCst);
    }

    /// Lends the terminal to another program (a shell, an editor): the screen and the modes as they
    /// were found, as when it is given back. The handlers of the signals stay, so that the program
    /// hears a hangup while the other one has the terminal; and while it is lent nothing gives it
    /// back again (`restore` does nothing), since the program is then a background job of the tty,
    /// which a change of its modes would stop. [`Self::take_back`] ends the loan.
    pub(crate) fn lend(&mut self) {
        let output = OUTPUT_FD.load(Ordering::SeqCst);
        if ALTERNATE.swap(false, Ordering::SeqCst) {
            let _ = write_within(output, LEAVE_ALTERNATE, FAREWELL);
        }
        let _ = set_modes(self.input, &termios_of(&SAVED));
        ARMED.store(false, Ordering::SeqCst);
    }

    /// Takes the terminal back from the program it was lent to: the modes of the protocol again,
    /// and every path out gives them back as before. The alternate screen comes with the next view.
    pub(crate) fn take_back(&mut self) {
        crate::foreground::without_stops(|| {
            let _ = set_modes(self.input, &termios_of(&RAW));
        });
        ARMED.store(true, Ordering::SeqCst);
    }

    /// The person asked to stop the program: the terminal is given back, the program stops, and
    /// when it is continued the terminal is taken again as it was.
    pub(crate) fn suspend(&mut self) {
        let output = OUTPUT_FD.load(Ordering::SeqCst);
        let alternate = ALTERNATE.load(Ordering::SeqCst);
        if alternate {
            let _ = write_within(output, LEAVE_ALTERNATE, FAREWELL);
        }
        let _ = set_modes(self.input, &termios_of(&SAVED));
        // SAFETY: puts the default action of the signal back, stops this process with it, and
        // when the process is continued puts our handler back.
        unsafe {
            let mut default: libc::sigaction = std::mem::zeroed();
            default.sa_sigaction = libc::SIG_DFL;
            libc::sigemptyset(&mut default.sa_mask);
            let mut ours: libc::sigaction = std::mem::zeroed();
            libc::sigaction(STOPPING, &default, &mut ours);
            // The signal is blocked while its handler runs and this is not the handler, but
            // another thread may have it blocked: unblock it for this one.
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            libc::sigaddset(&mut set, STOPPING);
            libc::pthread_sigmask(libc::SIG_UNBLOCK, &set, std::ptr::null_mut());
            libc::raise(STOPPING);
            libc::sigaction(STOPPING, &ours, std::ptr::null_mut());
        }
        // Continued. What was typed while the program was stopped went to the shell.
        let _ = set_modes(self.input, &termios_of(&RAW));
        if alternate {
            let _ = write_within(output, ENTER_ALTERNATE, FAREWELL);
        }
    }
}

impl Drop for Taken {
    fn drop(&mut self) {
        self.give_back(false);
    }
}

// ---- the threads a program's handlers run on --------------------------------------------

/// Runs `body` on a thread that is not asked about the signals of the program: the event loop's
/// thread is the one the signals are for, and a thread that took one could not interrupt the
/// call the loop is waiting in.
pub(crate) fn spawn_quiet<F: FnOnce() + Send + 'static>(body: F) -> io::Result<()> {
    // SAFETY: blocks the signals in this thread so that the new one is born with them blocked,
    // and puts this thread's mask back.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        for signal in ENDING.into_iter().chain([STOPPING]) {
            libc::sigaddset(&mut set, signal);
        }
        let mut old: libc::sigset_t = std::mem::zeroed();
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, &mut old);
        let spawned = std::thread::Builder::new()
            .name("pencere-handler".into())
            .spawn(body);
        libc::pthread_sigmask(libc::SIG_SETMASK, &old, std::ptr::null_mut());
        spawned.map(drop)
    }
}

// ---- polling ---------------------------------------------------------------------------

/// What the descriptors of the loop have for it.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Ready {
    /// The terminal has bytes for the program.
    pub(crate) input: bool,
    /// The terminal has hung up.
    pub(crate) hangup: bool,
    /// Something woke the loop: a signal or another thread.
    pub(crate) wake: bool,
    /// The terminal has room for what the program writes.
    pub(crate) output: bool,
}

/// Waits for the input, the pipe and, when something is waiting to be written, the room.
/// `None` waits for ever. An interrupted wait is an empty one.
pub(crate) fn wait(
    input: RawFd,
    output: Option<RawFd>,
    wake: RawFd,
    timeout: Option<Duration>,
) -> io::Result<Ready> {
    let mut fds = [
        libc::pollfd {
            fd: input,
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: wake,
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: output.unwrap_or(-1),
            events: libc::POLLOUT,
            revents: 0,
        },
    ];
    let millis = match timeout {
        None => -1,
        // Rounded up: a wait that ends a hair early would spin until the deadline.
        Some(d) => i32::try_from(d.as_millis().saturating_add(1)).unwrap_or(i32::MAX),
    };
    // SAFETY: three pollfds, owned here.
    let got = unsafe { libc::poll(fds.as_mut_ptr(), 3, millis) };
    if got < 0 {
        let error = io::Error::last_os_error();
        return if error.kind() == io::ErrorKind::Interrupted {
            Ok(Ready::default())
        } else {
            Err(error)
        };
    }
    let events = |at: usize| fds.get(at).map_or(0, |fd| fd.revents);
    let hangup_bits = libc::POLLHUP | libc::POLLERR | libc::POLLNVAL;
    Ok(Ready {
        input: events(0) & (libc::POLLIN | hangup_bits) != 0,
        hangup: events(0) & hangup_bits != 0,
        wake: events(1) & libc::POLLIN != 0,
        output: events(2) & (libc::POLLOUT | hangup_bits) != 0,
    })
}

/// What a read of the terminal found.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Read {
    /// This many bytes.
    Bytes(usize),
    /// Nothing: a signal woke the read, or the bytes `poll` promised were flushed away before
    /// they could be read.
    Nothing,
    /// The terminal has hung up.
    Ended,
}

/// Reads what there is into `buffer` without waiting. `hangup` says that `poll` saw the terminal
/// hang up, which is how a read of nothing is told from the end of the input.
pub(crate) fn read(fd: RawFd, buffer: &mut [u8], hangup: bool) -> Read {
    // SAFETY: reads into a buffer we own. The terminal's `VMIN` is 0: this does not wait.
    let got = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
    if got > 0 {
        return Read::Bytes(got as usize);
    }
    if got < 0 {
        let error = io::Error::last_os_error();
        return match error.kind() {
            io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock => Read::Nothing,
            // A pty whose other end is gone: Linux says `EIO`.
            _ => Read::Ended,
        };
    }
    if hangup { Read::Ended } else { Read::Nothing }
}
