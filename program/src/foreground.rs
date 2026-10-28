//! A command given the terminal: run as the foreground job of the tty, the way a shell runs one,
//! waited for, and the foreground taken back when it ends.
//!
//! The command gets a process group of its own and the tty's foreground with it, so that ⌃C, ⌃Z
//! and the reads of the tty are its own and not the program's; the program is a background job
//! until the command has ended. It must not touch the tty in the meantime: a read would stop it
//! (`SIGTTIN`), and so would a change of the modes (`SIGTTOU`), which is why the calls that take the
//! terminal back are made with `SIGTTOU` blocked.

use std::io;
use std::os::fd::{FromRawFd as _, OwnedFd, RawFd};
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::process::{Command, ExitStatus};
use std::sync::{Arc, Mutex, PoisonError};

/// A command that has the terminal.
#[derive(Debug, Clone)]
pub(crate) struct Job {
    pid: libc::pid_t,
    /// The tty the command was given, or -1.
    tty: RawFd,
    /// The process group that had the tty's foreground before: the one it is given back to.
    before: libc::pid_t,
    /// The command's status has been taken: its number may be another process's from then on, and
    /// nothing is sent to it. Held while the status is taken and while a signal is sent.
    reaped: Arc<Mutex<bool>>,
}

/// Starts `command` in a process group of its own, with the foreground of `tty` (when `tty` is the
/// program's controlling terminal; on any other the command runs, in its group, without one).
pub(crate) fn start(mut command: Command, tty: RawFd) -> io::Result<Job> {
    // SAFETY: asks which process group has the tty's foreground; -1 when it is not the program's
    // controlling terminal.
    let before = unsafe { libc::tcgetpgrp(tty) };
    // SAFETY: runs in the child between fork and exec, where only calls that are safe there are
    // made. The child does not inherit the signal mask of the thread that started it: a thread that
    // runs a handler has the program's signals blocked, and a command born with them blocked would
    // not hear ⌃C. Then it makes its own process group and takes the foreground for it, with the
    // signal a background job gets for that ignored for the moment.
    unsafe {
        command.pre_exec(move || {
            let mut none: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut none);
            libc::sigprocmask(libc::SIG_SETMASK, &none, std::ptr::null_mut());
            libc::setpgid(0, 0);
            let previous = libc::signal(libc::SIGTTOU, libc::SIG_IGN);
            libc::tcsetpgrp(tty, libc::getpid());
            libc::signal(libc::SIGTTOU, previous);
            Ok(())
        });
    }
    // `spawn` returns once the child has run what is above and the program is running, or has
    // failed to be (a program that is not there).
    let child = match command.spawn() {
        Ok(child) => child,
        Err(why) => {
            // The child may have taken the foreground before its program failed to start: it is
            // given back, or the program would be a background job of a tty nobody has.
            give_foreground(tty, before);
            return Err(why);
        }
    };
    let pid = libc::pid_t::try_from(child.id()).map_err(io::Error::other)?;
    // Waited for by `Job::wait`, which also sees it stop; dropping the handle does not wait.
    drop(child);
    Ok(Job {
        pid,
        tty,
        before,
        reaped: Arc::new(Mutex::new(false)),
    })
}

impl Job {
    /// Waits for the command to end: how it did. One that is stopped (⌃Z reaches a program that is
    /// not a shell) is continued, since the program that gave it the terminal has nothing else to
    /// give it to; one that is stopped for reaching for a terminal it does not have the foreground
    /// of (the program's terminal is not its controlling one) is ended as on a hangup, and if it
    /// reaches again, killed, since it can never go on.
    pub(crate) fn wait(&self) -> io::Result<ExitStatus> {
        let mut reached = 0;
        loop {
            // Waits for the command to stop or end without taking its status, so that until the
            // status is taken below, under the lock, its number stays its own (a zombie's), and a
            // signal sent to it meanwhile reaches nothing else.
            // SAFETY: a siginfo_t of ours for the call to fill.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            // SAFETY: waits for a child of ours, and fills `info`.
            let got = unsafe {
                libc::waitid(
                    libc::P_PID,
                    libc::id_t::try_from(self.pid).map_err(io::Error::other)?,
                    &raw mut info,
                    libc::WEXITED | libc::WSTOPPED | libc::WNOWAIT,
                )
            };
            if got < 0 {
                let why = io::Error::last_os_error();
                if why.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(why);
            }
            let mut reaped = self.reaped.lock().unwrap_or_else(PoisonError::into_inner);
            let mut status = 0;
            // SAFETY: takes the status of a child of ours that has one to give.
            let taken = unsafe {
                libc::waitpid(self.pid, &raw mut status, libc::WUNTRACED | libc::WNOHANG)
            };
            if taken < 0 {
                let why = io::Error::last_os_error();
                if why.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(why);
            }
            if taken == 0 {
                continue;
            }
            if libc::WIFSTOPPED(status) {
                drop(reaped);
                match libc::WSTOPSIG(status) {
                    libc::SIGTTIN | libc::SIGTTOU => {
                        reached += 1;
                        if reached > 1 {
                            self.signal(libc::SIGKILL);
                        } else {
                            self.signal(libc::SIGHUP);
                        }
                    }
                    _ => {}
                }
                self.signal(libc::SIGCONT);
                continue;
            }
            *reaped = true;
            return Ok(ExitStatus::from_raw(status));
        }
    }

    /// Sends `signal` to the command's process group, while the command has not been reaped: a
    /// hangup the program heard is passed on, as a shell passes its own to its jobs.
    pub(crate) fn signal(&self, signal: libc::c_int) {
        let reaped = self.reaped.lock().unwrap_or_else(PoisonError::into_inner);
        if *reaped {
            return;
        }
        // SAFETY: a signal to the process group of a child of ours that has not been reaped, or to
        // the child itself if it has no group of its own.
        unsafe {
            if libc::kill(-self.pid, signal) != 0 {
                libc::kill(self.pid, signal);
            }
        }
        drop(reaped);
    }

    /// Gives the tty's foreground back to the process group that had it before the command.
    pub(crate) fn take_back(&self) {
        give_foreground(self.tty, self.before);
    }
}

/// Gives the foreground of `tty` to the process group `group`, when there was one.
fn give_foreground(tty: RawFd, group: libc::pid_t) {
    if group > 0 {
        without_stops(|| {
            // SAFETY: gives the foreground of a tty we hold to a process group of our session.
            unsafe { libc::tcsetpgrp(tty, group) };
        });
    }
}

/// Runs `body` with `SIGTTOU` blocked on this thread: a program that is a background job of its
/// tty for a moment changes the tty's foreground or modes without being stopped for it (a call
/// with the signal blocked is made as if it were ignored).
pub(crate) fn without_stops<T>(body: impl FnOnce() -> T) -> T {
    // SAFETY: blocks one signal for this thread and puts the thread's mask back after.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTTOU);
        let mut old: libc::sigset_t = std::mem::zeroed();
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, &mut old);
        let result = body();
        libc::pthread_sigmask(libc::SIG_SETMASK, &old, std::ptr::null_mut());
        result
    }
}

/// Runs `command` as the foreground job of the program's controlling terminal, when no view has
/// the terminal, and takes the foreground back when it ends.
pub(crate) fn run(command: Command) -> io::Result<ExitStatus> {
    // The controlling terminal by its own name: the standard streams may be a filter's data.
    // SAFETY: opens a path we name; the descriptor is ours and nothing else holds it.
    let fd = unsafe {
        libc::open(
            c"/dev/tty".as_ptr(),
            libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
        )
    };
    // SAFETY: a descriptor `open` gave us and that nothing else owns.
    let tty = (fd >= 0).then(|| unsafe { OwnedFd::from_raw_fd(fd) });
    let job = start(command, fd)?;
    let ended = job.wait();
    job.take_back();
    drop(tty);
    ended
}
