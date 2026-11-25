//! `pencere host` asks the terminal it runs in which terminal it is, and says what the terminal
//! answered. The test plays the terminal on the master end of a pty, as the library's own tests do:
//! it reads the `hello`, writes the `welcome` the test chose (or none), and reads what the command
//! printed and how it ended.

// A test fails by panicking, and the pty is the test's.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    unsafe_code
)]

use std::fs::File;
use std::io::Read as _;
use std::os::fd::{FromRawFd as _, OwnedFd, RawFd};
use std::os::unix::process::CommandExt as _;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const PENCERE: &str = env!("CARGO_BIN_EXE_pencere");
const REPLY: &[u8] = b"\x1b[?62;4c";
const WITHIN: Duration = Duration::from_secs(10);

/// What the command did: its status, and what it printed to the standard output and the error.
struct Ran {
    status: ExitStatus,
    out: String,
    err: String,
}

/// The `welcome` of a terminal that answers a `hello` with `id`.
fn welcome(id: &str, host: &str) -> Vec<u8> {
    format!(
        "\x1b_pencere;welcome;id=1,re={id};{{\"version\":1,\"token\":\"q7Vh2sB0xJm9kLw4TzN1cA\",{host}\"views\":[]}}\x1b\\"
    )
    .into_bytes()
}

/// What a terminal writes to answer: the `welcome`, and its own answer to `CSI c` after it (§5.2).
fn answering(mut welcome: Vec<u8>) -> Vec<u8> {
    welcome.extend_from_slice(REPLY);
    welcome
}

/// The id a `hello` in `seen` has, once the `hello` is all there.
fn hello_id(seen: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(seen);
    let at = text.find("\x1b_pencere;hello;id=")? + "\x1b_pencere;hello;id=".len();
    let rest = text.get(at..)?;
    let end = rest.find(';')?;
    // The fence follows the `hello`: the terminal answers the one before the other.
    text.contains("\x1b[c").then(|| rest[..end].to_owned())
}

/// Runs `pencere host` on a pty whose master end `answer` plays: given the `hello`'s id, what the
/// terminal writes back (nothing: it is a terminal that does not know the protocol, and only
/// answers `CSI c`). With `piped`, the standard output is a pipe, as in `$(pencere host)`.
fn run(piped: bool, answer: impl Fn(&str) -> Vec<u8>) -> Ran {
    run_late(piped, answer, None)
}

/// As [`run`], and `late` is what the terminal writes a moment after `answer`: its answer to `CSI c`,
/// when it is slower than its `welcome`. The command must still be waiting for it when it comes.
fn run_late(piped: bool, answer: impl Fn(&str) -> Vec<u8>, late: Option<&[u8]>) -> Ran {
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
    let tty = |fd: RawFd| unsafe { OwnedFd::from_raw_fd(libc::dup(fd)) };
    let mut command = Command::new(PENCERE);
    command
        .arg("host")
        .env_remove("PENCERE_TUI")
        .stdin(Stdio::from(tty(slave)))
        .stderr(Stdio::piped());
    if piped {
        command.stdout(Stdio::piped());
    } else {
        command.stdout(Stdio::from(tty(slave)));
    }
    // SAFETY: only async-signal-safe calls between fork and exec: the pty is the command's
    // controlling terminal, as the pane of a terminal is.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            libc::ioctl(0, libc::TIOCSCTTY as _, 0);
            Ok(())
        });
    }
    let mut child = command.spawn().expect("the command starts");
    // SAFETY: the slave is ours; the child has its own copies.
    unsafe { libc::close(slave) };
    let mut pty = unsafe { File::from_raw_fd(master) };

    let mut seen = Vec::new();
    let mut answered = false;
    let until = Instant::now() + WITHIN;
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait") {
            break status;
        }
        assert!(Instant::now() < until, "the command does not end");
        let mut poll = libc::pollfd {
            fd: master,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one pollfd.
        if unsafe { libc::poll(&mut poll, 1, 20) } > 0 {
            let mut buffer = [0u8; 4096];
            match pty.read(&mut buffer) {
                Ok(0) | Err(_) => {}
                Ok(got) => seen.extend_from_slice(&buffer[..got]),
            }
        }
        if !answered && let Some(id) = hello_id(&seen) {
            answered = true;
            std::io::Write::write_all(&mut pty, &answer(&id)).expect("the terminal answers");
            if let Some(reply) = late {
                let settled = Instant::now() + Duration::from_millis(200);
                while Instant::now() < settled {
                    assert!(
                        child.try_wait().expect("wait").is_none(),
                        "the command ended before the terminal answered `CSI c`, which would reach the shell as typed text"
                    );
                    std::thread::sleep(Duration::from_millis(20));
                }
                std::io::Write::write_all(&mut pty, reply).expect("the terminal answers late");
            }
        }
    };
    let mut out = String::new();
    if piped {
        child
            .stdout
            .take()
            .expect("a pipe")
            .read_to_string(&mut out)
            .expect("the standard output");
    } else {
        // What the command wrote to the pane after the handshake.
        let mut buffer = [0u8; 4096];
        loop {
            let mut poll = libc::pollfd {
                fd: master,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: one pollfd.
            if unsafe { libc::poll(&mut poll, 1, 50) } <= 0 {
                break;
            }
            match pty.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(got) => seen.extend_from_slice(&buffer[..got]),
            }
        }
        out = String::from_utf8_lossy(&seen).into_owned();
    }
    let mut err = String::new();
    child
        .stderr
        .take()
        .expect("a pipe")
        .read_to_string(&mut err)
        .expect("the standard error");
    Ran { status, out, err }
}

#[test]
fn a_terminal_that_names_itself_is_printed_as_its_name_and_its_version() {
    let host = r#""host":{"name":"bateri","version":"0.9.1"},"#;
    let ran = run(false, |id| answering(welcome(id, host)));
    assert_eq!(ran.status.code(), Some(0), "{}", ran.err);
    assert!(ran.out.contains("bateri 0.9.1\r\n"), "{:?}", ran.out);
    // Nothing is shown: no view was asked for.
    assert!(!ran.out.contains("pencere;open"), "{:?}", ran.out);
    assert_eq!(ran.err, "");
}

#[test]
fn it_asks_the_terminal_of_the_person_when_the_standard_output_is_a_pipe() {
    let host = r#""host":{"name":"bateri","version":"0.9.1"},"#;
    let ran = run(true, |id| answering(welcome(id, host)));
    assert_eq!(ran.status.code(), Some(0), "{}", ran.err);
    assert_eq!(ran.out, "bateri 0.9.1\n");
}

#[test]
fn the_words_of_the_terminal_are_written_out_and_not_acted_on() {
    let host = r#""host":{"name":"ba\u001b[2Jteri","version":"1"},"#;
    let ran = run(true, |id| answering(welcome(id, host)));
    assert_eq!(ran.status.code(), Some(0), "{}", ran.err);
    assert_eq!(ran.out, "ba\\x1b[2Jteri 1\n");
}

#[test]
fn a_terminal_that_answers_without_naming_itself_is_no_host_that_can_be_told() {
    let ran = run(true, |id| answering(welcome(id, "")));
    assert_eq!(ran.status.code(), Some(1));
    assert_eq!(ran.out, "");
    assert_eq!(ran.err, "pencere: no pencere host in this terminal\n");
}

#[test]
fn a_terminal_that_does_not_know_the_protocol_has_no_pencere_host() {
    let ran = run(true, |_| REPLY.to_vec());
    assert_eq!(ran.status.code(), Some(1));
    assert_eq!(ran.out, "");
    assert_eq!(ran.err, "pencere: no pencere host in this terminal\n");
}

#[test]
fn a_host_that_shares_no_version_still_names_itself() {
    let ran = run(true, |id| {
        answering(
            format!(
                "\x1b_pencere;welcome;id=1,re={id};{{\"supported\":[2],\"host\":{{\"name\":\"bateri\",\"version\":\"9\"}}}}\x1b\\"
            )
            .into_bytes(),
        )
    });
    assert_eq!(ran.status.code(), Some(0), "{}", ran.err);
    assert_eq!(ran.out, "bateri 9\n");
}

#[test]
fn control_c_while_it_waits_ends_it_with_the_status_of_a_shell_for_it() {
    // The terminal answers nothing; the person types ⌃C, which the pty turns into SIGINT.
    let ran = run(true, |_| vec![0x03]);
    assert_eq!(ran.status.code(), Some(130), "{}", ran.err);
    assert_eq!(ran.out, "");
    assert_eq!(ran.err, "");
}

#[test]
fn a_terminal_slow_to_answer_csi_c_is_waited_for_so_that_the_answer_is_not_typed_at_the_prompt() {
    let host = r#""host":{"name":"bateri","version":"0.9.1"},"#;
    let ran = run_late(true, |id| welcome(id, host), Some(REPLY));
    assert_eq!(ran.status.code(), Some(0), "{}", ran.err);
    assert_eq!(ran.out, "bateri 0.9.1\n");
}
