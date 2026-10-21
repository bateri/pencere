//! The pane: a pty with a program running on it, and the person's own
//! terminal when there is one.
//!
//! The only file that touches libc. The `pencere` crate does no I/O, so what a
//! terminal does with a process, a pty and a clock is here: start the program
//! with the pty as its controlling terminal (so that ^C becomes SIGINT for it,
//! as in any terminal), read its output and write its input without blocking,
//! and look at the modes of its tty.
#![allow(unsafe_code)]

use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::mem;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::ptr;

/// A pane's size in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

impl Size {
    pub const DEFAULT: Self = Self { cols: 80, rows: 24 };

    /// The size of the terminal this program runs in, if it runs in one.
    pub fn of_terminal() -> Option<Self> {
        let out = io::stdout();
        if !out.is_terminal() {
            return None;
        }
        // SAFETY: `winsize` is plain data and TIOCGWINSZ fills it or fails.
        let mut found: libc::winsize = unsafe { mem::zeroed() };
        let rc = unsafe { libc::ioctl(out.as_raw_fd(), libc::TIOCGWINSZ, &mut found) };
        (rc == 0 && found.ws_col > 0 && found.ws_row > 0).then_some(Self {
            cols: found.ws_col,
            rows: found.ws_row,
        })
    }
}

/// Two of the modes of the pane's tty (specification §4.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Modes {
    pub canonical: bool,
    pub echo: bool,
}

/// What reading the pane found.
#[derive(Debug, PartialEq, Eq)]
pub enum Output {
    Bytes(usize),
    Nothing,
    /// Nobody holds the other end any more.
    End,
}

fn check(rc: libc::c_int) -> io::Result<libc::c_int> {
    if rc < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(rc)
    }
}

/// The master end, non-blocking and not inherited, and the slave end.
fn open(size: Size) -> io::Result<(File, OwnedFd)> {
    let (mut master, mut slave) = (-1, -1);
    let mut winsize = libc::winsize {
        ws_row: size.rows,
        ws_col: size.cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: both descriptor out-pointers are valid, the name and the termios
    // are null, and the winsize outlives the call.
    check(unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut winsize,
        )
    })?;
    // SAFETY: openpty returned two fresh descriptors that nothing else owns.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    for fd in [&master, &slave] {
        // SAFETY: the descriptor is open and ours.
        check(unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) })?;
    }
    // SAFETY: as above.
    let flags = check(unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) })?;
    check(unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) })?;
    Ok((File::from(master), slave))
}

/// A program on a pty.
pub struct Pty {
    master: File,
    child: Child,
}

impl Pty {
    pub fn spawn(program: &[String], size: Size) -> io::Result<Self> {
        let (master, slave) = open(size)?;
        let (name, args) = program
            .split_first()
            .ok_or_else(|| io::Error::other("no program to run"))?;
        let mut command = Command::new(name);
        command
            .args(args)
            .stdin(Stdio::from(slave.try_clone()?))
            .stdout(Stdio::from(slave.try_clone()?))
            .stderr(Stdio::from(slave));
        // SAFETY: the closure runs in the forked child before exec and makes
        // two system calls, both async-signal-safe. Stdin is already the slave.
        unsafe {
            command.pre_exec(|| {
                check(libc::setsid())?;
                check(libc::ioctl(0, libc::TIOCSCTTY as _, 0))?;
                Ok(())
            });
        }
        let child = command.spawn()?;
        // The command holds the slave: while it did, no end of file could come.
        drop(command);
        Ok(Self { master, child })
    }

    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn master_fd(&self) -> RawFd {
        self.master.as_raw_fd()
    }

    /// What the program has written, without waiting.
    pub fn read(&self, buf: &mut [u8]) -> io::Result<Output> {
        match (&self.master).read(buf) {
            Ok(0) => Ok(Output::End),
            Ok(n) => Ok(Output::Bytes(n)),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(Output::Nothing)
            }
            // What a pty master reports once the program has gone.
            Err(e) if e.raw_os_error() == Some(libc::EIO) => Ok(Output::End),
            Err(e) => Err(e),
        }
    }

    /// Types `bytes` into the pane; returns how many the tty took (none, when
    /// it is full).
    pub fn write(&self, bytes: &[u8]) -> io::Result<usize> {
        match (&self.master).write(bytes) {
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(0)
            }
            other => other,
        }
    }

    /// The modes of the pane's tty; `None` where they cannot be read.
    pub fn modes(&self) -> Option<Modes> {
        // SAFETY: `termios` is plain data and tcgetattr fills it or fails.
        let mut termios: libc::termios = unsafe { mem::zeroed() };
        let rc = unsafe { libc::tcgetattr(self.master.as_raw_fd(), &mut termios) };
        (rc == 0).then_some(Modes {
            canonical: termios.c_lflag & libc::ICANON != 0,
            echo: termios.c_lflag & libc::ECHO != 0,
        })
    }

    /// SIGKILL: the program gets no chance to say goodbye.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
    }

    /// The program's end, if it has come.
    pub fn finished(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().ok().flatten()
    }
}

impl Drop for Pty {
    /// However the run ends, the program does not outlive it: an error on the way
    /// out must not leave a child holding a pty nobody reads.
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Sleeps until the pane has something to read, or (when `writable` asks) can
/// take more input, or the person typed something (`keys`), or `timeout_ms` is
/// up. Nothing depends on what it reports: the caller looks at everything after
/// it returns, so a descriptor poll does not handle well costs only a delay.
pub fn wait(pty: &Pty, writable: bool, keys: bool, timeout_ms: i32) {
    let mut polled = vec![libc::pollfd {
        fd: pty.master_fd(),
        events: libc::POLLIN | if writable { libc::POLLOUT } else { 0 },
        revents: 0,
    }];
    if keys {
        polled.push(libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        });
    }
    // SAFETY: `polled` is a valid array of the length given.
    let _ = unsafe {
        libc::poll(
            polled.as_mut_ptr(),
            polled.len() as libc::nfds_t,
            timeout_ms,
        )
    };
}

/// What the person's terminal has to say.
#[derive(Debug, PartialEq, Eq)]
pub enum Keys {
    Typed(usize),
    Nothing,
    /// The terminal hung up: nobody is there to type, and poll would report it
    /// again on every turn.
    Gone,
}

/// What the person has typed into this terminal, without waiting for more.
pub fn read_keys(buf: &mut [u8]) -> Keys {
    let mut polled = libc::pollfd {
        fd: libc::STDIN_FILENO,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: `polled` is one valid pollfd.
    if unsafe { libc::poll(&mut polled, 1, 0) } <= 0 {
        return Keys::Nothing;
    }
    if polled.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0
        && polled.revents & libc::POLLIN == 0
    {
        return Keys::Gone;
    }
    if polled.revents & libc::POLLIN == 0 {
        return Keys::Nothing;
    }
    // SAFETY: `buf` is valid for `buf.len()` bytes. The descriptor is read with
    // read(2) and not through std's buffered stdin, which would hold keys that
    // poll can no longer see.
    let count = unsafe { libc::read(libc::STDIN_FILENO, buf.as_mut_ptr().cast(), buf.len()) };
    match usize::try_from(count) {
        Ok(0) => Keys::Gone,
        Ok(count) => Keys::Typed(count),
        Err(_) => match io::Error::last_os_error().kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted => Keys::Nothing,
            _ => Keys::Gone,
        },
    }
}

/// The person's terminal taken as a terminal takes keys: every key reaches the
/// pane as bytes (^C as 0x03, which the pane's own tty turns into SIGINT), and
/// the settings come back when this is dropped. Output processing stays on, so
/// the lines written to stderr keep their line ends.
pub struct Raw {
    saved: libc::termios,
}

impl Raw {
    pub fn enter() -> io::Result<Self> {
        // SAFETY: `termios` is plain data; tcgetattr fills it or fails.
        let mut saved: libc::termios = unsafe { mem::zeroed() };
        check(unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut saved) })?;
        let mut raw = saved;
        raw.c_iflag &= !(libc::ICRNL | libc::INLCR | libc::IGNCR | libc::IXON);
        raw.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG | libc::IEXTEN);
        for (index, value) in [(libc::VMIN, 1), (libc::VTIME, 0)] {
            if let Some(slot) = raw.c_cc.get_mut(index) {
                *slot = value;
            }
        }
        // SAFETY: `raw` is a valid termios.
        check(unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw) })?;
        Ok(Self { saved })
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        // SAFETY: `saved` is what tcgetattr gave.
        let _ = unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.saved) };
    }
}
