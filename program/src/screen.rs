//! The terminal as a screen the program draws on itself.
//!
//! Where no terminal hosts the program's view ([`NoViewReason::NoHost`](crate::NoViewReason) and
//! the others that leave it with none), a program that carries a host for its view can still show
//! it: [`Options::screen`](crate::Options::screen) hands the program's options and the terminal to
//! that host, which draws the view with the characters of the terminal. The terminal is taken as
//! [`Taken`] takes it for the protocol, and with the keys of the line discipline switched off
//! (flow control, suspend, quit, the next character literally), ⌃C left a signal; and the way back is
//! the same one: the end, a signal, a panic and `exit` all give it back, after writing what the
//! host asked to be written when it does.

use std::io;
use std::os::fd::RawFd;
use std::sync::Arc;
use std::time::Duration;

use crate::session::PanicHook;
use crate::terminal::{self, ENTER_ALTERNATE, Modes, Pipe, TakeError, Taken, Terminal};
use crate::{Error, Lent, foreground};

/// Whether the environment says the program runs in tmux or screen: the variables they set for what runs
/// in them, and the terminal's name, which an `ssh` carries over to the machine on the other side (where
/// their variables are not) and which both give their windows (`screen`, `screen-256color`,
/// `tmux-256color`). Neither passes what a host is asked in a string of its own (tmux and screen keep it
/// for the title of the window) or draws the pictures of the terminal behind it, so that the program
/// draws its own screen without asking.
#[must_use]
pub fn in_multiplexer() -> bool {
    multiplexed(|name| std::env::var(name).ok())
}

fn multiplexed(var: impl Fn(&str) -> Option<String>) -> bool {
    let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
    let named = |name: &str| {
        var(name).is_some_and(|value| {
            let value = value.to_ascii_lowercase();
            value.starts_with("screen") || value.starts_with("tmux")
        })
    };
    set("TMUX") || set("STY") || named("TERM") || var("TERM_PROGRAM").as_deref() == Some("tmux")
}

#[cfg(test)]
mod multiplexer_tests {
    use super::multiplexed;

    fn said(pairs: &'static [(&'static str, &'static str)]) -> bool {
        multiplexed(move |name| {
            pairs
                .iter()
                .find(|(have, _)| *have == name)
                .map(|(_, value)| (*value).to_owned())
        })
    }

    #[test]
    fn the_environment_of_tmux_and_screen_says_so() {
        assert!(said(&[("TMUX", "/tmp/tmux-501/default,1,0")]));
        assert!(said(&[("STY", "123.pts-0.host")]));
        assert!(said(&[("TERM", "screen-256color")]));
        assert!(said(&[("TERM", "tmux-256color")]));
        assert!(said(&[
            ("TERM", "xterm-256color"),
            ("TERM_PROGRAM", "tmux")
        ]));
        assert!(!said(&[("TERM", "xterm-256color")]));
        assert!(!said(&[
            ("TERM", "xterm-ghostty"),
            ("TMUX", ""),
            ("STY", "")
        ]));
        assert!(!said(&[]));
    }
}

/// How large the terminal's window is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    /// Columns.
    pub cols: u16,
    /// Rows.
    pub rows: u16,
    /// Pixels across, 0 where the terminal does not say.
    pub width: u16,
    /// Pixels down, 0 where the terminal does not say.
    pub height: u16,
}

/// What [`Tty::wait`] woke for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ready {
    /// The terminal has bytes to read.
    pub input: bool,
    /// A [`Waker`] was used, or a signal came (ask [`Tty::take_signal`] and [`Tty::take_resize`]).
    pub woken: bool,
    /// The terminal is gone.
    pub hangup: bool,
}

/// Wakes the [`Tty::wait`] of the screen it came from, from any thread.
#[derive(Clone, Debug)]
pub struct Waker(pub(crate) Arc<Pipe>);

impl Waker {
    /// Makes the wait return. Never blocks.
    pub fn wake(&self) {
        self.0.wake();
    }
}

/// The program's terminal, taken for a screen it draws on itself. Dropping it gives the terminal
/// back: what the terminal queued for the program and it did not read is thrown away.
pub struct Tty<'a> {
    terminal: &'a Terminal,
    // Dropped first: the terminal is given back before the pipe the signals wake is closed.
    taken: Taken,
    pipe: Arc<Pipe>,
    _hook: PanicHook,
}

impl std::fmt::Debug for Tty<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tty").finish_non_exhaustive()
    }
}

impl<'a> Tty<'a> {
    pub(crate) fn take(terminal: &'a Terminal) -> Result<Self, Error> {
        let pipe = Arc::new(Pipe::new()?);
        let mut taken = match Taken::take_in(terminal, &pipe, Modes::Screen) {
            Ok(taken) => taken,
            Err(TakeError::Busy) => return Err(Error::Busy),
            Err(TakeError::Failed) => {
                return Err(Error::Io(io::Error::other(
                    "the modes of the terminal cannot be set",
                )));
            }
        };
        taken.listen_for_resizes();
        Ok(Self {
            terminal,
            taken,
            pipe,
            _hook: PanicHook::install(),
        })
    }

    /// The size of the window, from what the terminal says: 80 by 24 where it says nothing.
    #[must_use]
    pub fn size(&self) -> Size {
        // SAFETY: `winsize` is plain data; the call fills it for a descriptor of ours, or fails.
        let found = unsafe {
            let mut window: libc::winsize = std::mem::zeroed();
            (libc::ioctl(self.terminal.output(), libc::TIOCGWINSZ, &mut window) == 0
                && window.ws_col > 0
                && window.ws_row > 0)
                .then_some(window)
        };
        match found {
            Some(window) => Size {
                cols: window.ws_col,
                rows: window.ws_row,
                width: window.ws_xpixel,
                height: window.ws_ypixel,
            },
            None => Size {
                cols: 80,
                rows: 24,
                width: 0,
                height: 0,
            },
        }
    }

    /// Switches to the alternate screen and writes `on` (the modes the host wants: the mouse, the
    /// paste brackets). `off` is written, before the screen is left, whenever the terminal is given
    /// back: at the end, on a signal, in a panic.
    ///
    /// # Errors
    ///
    /// When the terminal cannot be written to.
    pub fn enter(&self, on: &[u8], off: &[u8]) -> io::Result<()> {
        terminal::set_leave(off);
        terminal::note_alternate(true);
        let mut bytes = ENTER_ALTERNATE.to_vec();
        bytes.extend_from_slice(on);
        self.write_all(&bytes)
    }

    /// Lends the terminal to the command of `lent`, and waits until it has ended: the modes that
    /// were switched on are switched off (what [`Tty::enter`] and [`Tty::leaving`] gave), the screen
    /// is left, the terminal is given back as it was found and the command runs as its foreground
    /// job; then the terminal is taken again and the screen is entered again with `on`. The
    /// program's hand-off is told how the command ended, or why it could not be run. Nothing is
    /// drawn or read meanwhile, and what the screen showed is gone from the terminal: the caller
    /// draws it anew, switches on again what [`Tty::leaving`] had it take back, and asks for the
    /// size.
    ///
    /// # Errors
    ///
    /// When the terminal cannot be written to as the screen is entered again.
    pub fn lend(&mut self, mut lent: Lent, on: &[u8]) -> io::Result<()> {
        let _ = self.write_all(&terminal::leave_bytes());
        self.taken.lend();
        let tty = self.terminal.input();
        let outcome = foreground::start(lent.take_command(), tty).and_then(|job| {
            let status = job.wait();
            job.take_back();
            status
        });
        self.taken.take_back();
        terminal::note_alternate(true);
        let mut bytes = ENTER_ALTERNATE.to_vec();
        bytes.extend_from_slice(on);
        let entered = self.write_all(&bytes);
        lent.ended(outcome);
        entered
    }

    /// What is written, before the screen is left, whenever the terminal is given back, in place of
    /// the `off` given to [`Tty::enter`]: for a mode that was switched on after the screen was
    /// entered (one the terminal was asked about first).
    pub fn leaving(&self, off: &[u8]) {
        terminal::set_leave(off);
    }

    /// Waits until the terminal has bytes, a [`Waker`] is used or a signal comes, at most `within`
    /// (for ever where `None`). A signal that interrupts the wait ends it with nothing ready: ask
    /// [`Tty::take_signal`].
    ///
    /// # Errors
    ///
    /// When the wait fails in a way that is not a signal.
    pub fn wait(&self, within: Option<Duration>) -> io::Result<Ready> {
        let mut polled = [
            libc::pollfd {
                fd: self.terminal.input(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: self.pipe.read_end(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let milliseconds = within.map_or(-1, |within| {
            i32::try_from(within.as_millis().saturating_add(1)).unwrap_or(i32::MAX)
        });
        // SAFETY: two pollfds, owned here.
        let ready = unsafe { libc::poll(polled.as_mut_ptr(), 2, milliseconds) };
        if ready < 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::Interrupted {
                Ok(Ready {
                    woken: true,
                    ..Ready::default()
                })
            } else {
                Err(error)
            };
        }
        let [terminal, wake] = polled;
        let gone = libc::POLLHUP | libc::POLLERR | libc::POLLNVAL;
        if wake.revents & libc::POLLIN != 0 {
            self.pipe.drain();
        }
        Ok(Ready {
            input: terminal.revents & libc::POLLIN != 0,
            woken: wake.revents & libc::POLLIN != 0,
            hangup: terminal.revents & gone != 0 && terminal.revents & libc::POLLIN == 0,
        })
    }

    /// Reads what the terminal has: nothing is 0 bytes, never a wait.
    ///
    /// # Errors
    ///
    /// When the read fails in a way that is not a signal or a terminal with nothing to say.
    pub fn read(&self, buffer: &mut [u8]) -> io::Result<usize> {
        // SAFETY: reads into a buffer we own, from a descriptor the caller keeps open.
        let got = unsafe {
            libc::read(
                self.terminal.input(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            )
        };
        if got >= 0 {
            return Ok(got as usize);
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

    /// Writes all of `bytes`, waiting as long as the terminal does not take them: a signal that
    /// ends the program ends the wait with [`io::ErrorKind::Interrupted`].
    ///
    /// # Errors
    ///
    /// When the terminal cannot be written to, or a signal came.
    pub fn write_all(&self, mut bytes: &[u8]) -> io::Result<()> {
        let fd: RawFd = self.terminal.output();
        while !bytes.is_empty() {
            let piece = bytes.len().min(16 * 1024);
            // SAFETY: writes from a slice we own to a descriptor the caller keeps open.
            let wrote = unsafe { libc::write(fd, bytes.as_ptr().cast(), piece) };
            if wrote > 0 {
                bytes = bytes.get(wrote as usize..).unwrap_or_default();
                continue;
            }
            if wrote == 0 {
                continue;
            }
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => {
                    if terminal::signal_pending() {
                        return Err(error);
                    }
                }
                io::ErrorKind::WouldBlock => {
                    let _ = terminal::wait_for_room(fd, Duration::from_millis(100))?;
                }
                _ => return Err(error),
            }
        }
        Ok(())
    }

    /// What wakes [`Tty::wait`] from another thread.
    #[must_use]
    pub fn waker(&self) -> Waker {
        Waker(Arc::clone(&self.pipe))
    }

    /// The signal that ends the program (⌃C is `SIGINT`, 2; `SIGTERM`, `SIGHUP` and `SIGQUIT` too), once.
    #[must_use]
    pub fn take_signal(&self) -> Option<i32> {
        terminal::take_signal()
    }

    /// Whether the window changed size since this was last asked, once.
    #[must_use]
    pub fn take_resize(&self) -> bool {
        terminal::take_resize()
    }

    /// Whether the person stopped the program from outside (`SIGTSTP`), once: the keys of the tty
    /// that would are switched off, so this is another process's doing, and is the screen's to
    /// ignore or to answer.
    #[must_use]
    pub fn take_stop(&self) -> bool {
        terminal::take_stop()
    }
}

impl Drop for Tty<'_> {
    fn drop(&mut self) {
        self.taken.give_back(true);
    }
}
