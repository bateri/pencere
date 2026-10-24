//! The programs an app started, for when a signal ends the app and nothing in the app handles it.
//!
//! A program runs in a group of its own, so that a pipeline is ended whole; the terminal's ⌃C and its
//! hangup go to the app's group only. While a view is up, the program library's handlers turn them
//! into the end of the app, which ends the programs. At other times (an app that is a command and
//! shows nothing, or has not shown its view yet) the default action ends the app and would leave
//! the programs running with nobody to read them. This handler asks their groups to end first, and then
//! ends the app as the signal would have.

use std::sync::Once;
use std::sync::atomic::{AtomicI32, Ordering};

/// The signals a terminal sends to end a program, which the app would otherwise be ended by.
const ENDING: [libc::c_int; 4] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT];

/// How many groups the handler knows. More programs at once than this are not passed the signal.
const SLOTS: usize = 64;

/// The groups of the programs that have not been seen to end (their leaders' numbers). A handler
/// can only read atomics, so the list is a fixed one.
static GROUPS: [AtomicI32; SLOTS] = [const { AtomicI32::new(0) }; SLOTS];

static INSTALLED: Once = Once::new();

/// A program was started in a group of its own.
pub fn add(pid: u32) {
    install();
    let Ok(pid) = i32::try_from(pid) else {
        return;
    };
    for slot in &GROUPS {
        if slot
            .compare_exchange(0, pid, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return;
        }
    }
}

/// A program was seen to end, or was ended.
pub fn remove(pid: u32) {
    let Ok(pid) = i32::try_from(pid) else {
        return;
    };
    for slot in &GROUPS {
        let _ = slot.compare_exchange(pid, 0, Ordering::SeqCst, Ordering::SeqCst);
    }
}

extern "C" fn on_ending(signal: libc::c_int) {
    for slot in &GROUPS {
        let pid = slot.load(Ordering::SeqCst);
        if pid > 0 {
            // The programs are asked to end as the app's end asks them (`SIGTERM`), whichever signal
            // ended the app: a program started in the background of a shell ignores `SIGINT`.
            // SAFETY: a signal to a group of processes that this one started.
            unsafe {
                libc::kill(-pid, libc::SIGTERM);
            }
        }
    }
    // SAFETY: the signal's own action, taken again, and the signal sent again: it is blocked until
    // this handler returns, and then it ends the app as it would have.
    unsafe {
        libc::signal(signal, libc::SIG_DFL);
        libc::raise(signal);
    }
}

/// Installs the handler, once, for the signals that are not ignored (an app started with one
/// ignored, as `nohup` does, keeps it ignored).
fn install() {
    INSTALLED.call_once(|| {
        for signal in ENDING {
            // SAFETY: reads the action in place, and replaces the default with a handler that
            // only reads atomics and sends signals.
            unsafe {
                let mut old: libc::sigaction = std::mem::zeroed();
                if libc::sigaction(signal, std::ptr::null(), &raw mut old) != 0
                    || old.sa_sigaction != libc::SIG_DFL
                {
                    continue;
                }
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = on_ending as *const () as libc::sighandler_t;
                libc::sigemptyset(&raw mut action.sa_mask);
                libc::sigaction(signal, &raw const action, std::ptr::null_mut());
            }
        }
    });
}
