//! What the person's press on a link does (specification §9.7).
//!
//! A link the page asks for is shown on the bottom row first: that is the question put to the person.
//! A press or an Enter on the row is their yes, and what it does depends on where the program is:
//! on the person's own machine the address is opened in the browser the system opens it in, and
//! where the program runs on another machine (an ssh session) it is put on the terminal's
//! pasteboard, from where the person has it on their own. The same is done where the system's opener
//! cannot be started or fails.

use std::io;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// What sshd writes into the environment of a session: the connection, and the terminal of it.
const SESSION: [&str; 3] = ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"];

/// How long an opener that fails is waited for. One that runs longer is a browser that was started
/// (some openers wait for it) and what it exits with is not the address's.
const GRACE: Duration = Duration::from_secs(5);

/// Whether `name` is set to something in the environment `var` reads.
fn set(var: &impl Fn(&str) -> Option<String>, name: &str) -> bool {
    var(name).is_some_and(|value| !value.is_empty())
}

/// Whether the program runs in an ssh session: its person is on another machine, and a browser
/// started here would be on a machine they are not at.
pub(crate) fn remote(var: impl Fn(&str) -> Option<String>) -> bool {
    SESSION.iter().any(|name| set(&var, name))
}

/// Whether the system's opener can be asked to open an address here: on a Mac always, and elsewhere
/// where there is a display (an opener with none falls back to a browser in the terminal, which is
/// this program's).
pub(crate) fn can_open(var: impl Fn(&str) -> Option<String>) -> bool {
    cfg!(target_os = "macos")
        || ["DISPLAY", "WAYLAND_DISPLAY"]
            .iter()
            .any(|name| set(&var, name))
}

/// The system's opener.
pub(crate) fn opener() -> &'static str {
    if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

/// The addresses whose opener failed, for the loop to put on the pasteboard instead: the thread
/// that waits for an opener has no say over the terminal.
#[derive(Default)]
pub(crate) struct Failed(Mutex<Vec<String>>);

impl Failed {
    /// What failed since this was last asked.
    pub fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }

    fn add(&self, url: String) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(url);
    }
}

/// Asks `program` to open `url`: with the address as the one argument it has, no shell, and nothing
/// of the terminal (it is not to write to the screen or read the keys). It is waited for on a
/// thread of its own: an opener that exits with an error within [`GRACE`] puts the address in
/// `failed` and calls `wake` (the loop is woken). Where it cannot be started at all the error is
/// returned, and the caller has not opened anything.
///
/// # Errors
///
/// `program` is not there, or cannot be run.
pub(crate) fn open(
    program: &str,
    url: &str,
    failed: &Arc<Failed>,
    wake: impl FnOnce() + Send + 'static,
) -> io::Result<()> {
    let mut child = Command::new(program)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let (url, failed) = (url.to_owned(), Arc::clone(failed));
    let began = Instant::now();
    // A thread that cannot be made leaves the opener to itself: it was started.
    let _ = std::thread::Builder::new()
        .name("pencere-tui-opener".into())
        .spawn(move || {
            let well = child.wait().is_ok_and(|status| status.success());
            if !well && began.elapsed() < GRACE {
                failed.add(url);
                wake();
            }
        });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(have, _)| *have == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn a_session_of_ssh_is_where_the_person_is_not() {
        assert!(!remote(env(&[])));
        assert!(
            !remote(env(&[("SSH_CONNECTION", "")])),
            "set to nothing is not set"
        );
        for name in ["SSH_CONNECTION", "SSH_CLIENT", "SSH_TTY"] {
            assert!(
                remote(env(&[(name, "10.0.0.1 5000 10.0.0.2 22")])),
                "{name}"
            );
        }
        assert!(
            !remote(env(&[("SSH_AUTH_SOCK", "/tmp/agent")]),),
            "an agent is no session"
        );
    }

    #[test]
    fn an_opener_is_asked_for_where_the_system_has_one() {
        if cfg!(target_os = "macos") {
            assert!(can_open(env(&[])));
            assert_eq!(opener(), "open");
        } else {
            assert!(
                !can_open(env(&[])),
                "with no display it is a browser in the terminal"
            );
            assert!(can_open(env(&[("DISPLAY", ":0")])));
            assert!(can_open(env(&[("WAYLAND_DISPLAY", "wayland-0")])));
            assert_eq!(opener(), "xdg-open");
        }
    }

    /// A program of a script that exits as `code` says, in a directory of its own.
    fn script(name: &str, code: i32) -> (std::path::PathBuf, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("pencere-tui-opener-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let program = dir.join("opener");
        std::fs::write(
            &program,
            format!("#!/bin/sh\nprintf '%s' \"$1\" > \"$0.asked\"\nexit {code}\n"),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        (dir, program)
    }

    #[test]
    fn an_opener_that_fails_wakes_the_loop_with_the_address_and_one_that_does_not_does_not() {
        let url = "https://example.org/a?b=c&d=e";
        let (dir, program) = script("fails", 3);
        let failed = Arc::new(Failed::default());
        let (woken, wake) = std::sync::mpsc::channel();
        open(&program.to_string_lossy(), url, &failed, move || {
            let _ = woken.send(());
        })
        .unwrap();
        assert!(
            wake.recv_timeout(Duration::from_secs(10)).is_ok(),
            "the loop is woken"
        );
        assert_eq!(failed.take(), [url]);
        assert_eq!(failed.take(), Vec::<String>::new(), "and it is told once");
        // The address is the opener's one argument, as it was.
        assert_eq!(
            std::fs::read_to_string(dir.join("opener.asked")).unwrap(),
            url
        );
        std::fs::remove_dir_all(&dir).unwrap();

        let (dir, program) = script("works", 0);
        let (woken, wake) = std::sync::mpsc::channel();
        open(&program.to_string_lossy(), url, &failed, move || {
            let _ = woken.send(());
        })
        .unwrap();
        // The wake is dropped unused when the opener has been waited for.
        assert!(wake.recv_timeout(Duration::from_secs(10)).is_err());
        assert!(failed.take().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_opener_that_is_not_there_is_an_error_for_the_caller_to_copy_instead() {
        let failed = Arc::new(Failed::default());
        let result = open(
            "pencere-tui-no-such-opener",
            "https://example.org",
            &failed,
            || {},
        );
        assert!(result.is_err());
        assert!(failed.take().is_empty());
    }
}
