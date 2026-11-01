//! How `pencere dev` is left: with ⌃C, which it says it is ("waiting for a change, ⌃C to leave").
//! Leaving that way is the end a person asked for, not a failure, and `pencere dev` ends with 0
//! whether ⌃C came while the program ran, while Cargo built it or while a change was waited for;
//! a shell's `make dev` then says nothing of an error.
//!
//! While it is listened for, ⌃C is noted and nothing more: the program, Cargo or a wait sees it and
//! ends, and the loop reads the note. A handler, not `SIG_IGN`, so that a program started meanwhile,
//! which is started with the handlers reset, takes ⌃C as it would. ⌃\ is held the same way and not
//! taken for leaving: it is the program's.

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

/// ⌃C came while it was listened for.
static ASKED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_interrupt(_signal: libc::c_int) {
    ASKED.store(true, Ordering::SeqCst);
}

extern "C" fn on_quit(_signal: libc::c_int) {}

/// ⌃C and ⌃\ listened for until this is dropped; then they are as they were.
pub struct Listening {
    previous: Vec<(libc::c_int, libc::sigaction)>,
}

/// Listens for ⌃C (and holds ⌃\) until the returned value is dropped. A signal that is ignored (a
/// `pencere dev` started in the background, or under `nohup`) stays ignored.
#[allow(unsafe_code)]
pub fn listen() -> Listening {
    let mut previous = Vec::new();
    let handlers: [(libc::c_int, extern "C" fn(libc::c_int)); 2] =
        [(libc::SIGINT, on_interrupt), (libc::SIGQUIT, on_quit)];
    for (signal, handler) in handlers {
        // SAFETY: reads the action in place and replaces it with a handler that only stores to an
        // atomic (or does nothing); the action read is put back when `Listening` is dropped.
        unsafe {
            let mut old: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(signal, std::ptr::null(), &raw mut old) != 0
                || old.sa_sigaction == libc::SIG_IGN
            {
                continue;
            }
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = handler as *const () as libc::sighandler_t;
            libc::sigemptyset(&raw mut action.sa_mask);
            if libc::sigaction(signal, &raw const action, std::ptr::null_mut()) == 0 {
                previous.push((signal, old));
            }
        }
    }
    Listening { previous }
}

impl Drop for Listening {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        for (signal, old) in &self.previous {
            // SAFETY: the action that was in place before `listen`, put back.
            unsafe {
                libc::sigaction(*signal, old, std::ptr::null_mut());
            }
        }
    }
}

/// Whether ⌃C came while it was listened for.
pub fn asked() -> bool {
    ASKED.load(Ordering::SeqCst)
}

/// Waits for a change to the files, listening for ⌃C: the first change, and what came with it at
/// the same time is let go. None when the person left, or no change can come any more.
pub fn wait_for_change(changes: &Receiver<String>) -> Option<String> {
    let _listening = listen();
    let first = loop {
        if asked() {
            return None;
        }
        match changes.recv_timeout(Duration::from_millis(100)) {
            Ok(path) => break path,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return None,
        }
    };
    // A save is often two or three writes.
    std::thread::sleep(Duration::from_millis(120));
    while changes.try_recv().is_ok() {}
    Some(first)
}

/// What `pencere dev` ends with when its loop ended with `status`: 0 where the person left with ⌃C
/// (the program ended by it is 130), else the status.
pub fn ending(status: i32) -> ExitCode {
    if asked() || status == 130 {
        return ExitCode::SUCCESS;
    }
    ExitCode::from(u8::try_from(status & 0xff).unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_ended_by_control_c_is_left_with_nothing_wrong_and_another_end_is_said() {
        assert_eq!(ending(130), ExitCode::SUCCESS);
        assert_eq!(ending(3), ExitCode::from(3));
        assert_eq!(ending(143), ExitCode::from(143));
    }
}
