//! The terminal lent to a command and taken back (`ViewControl::hand_off`): the view closes, the
//! terminal is given back as at the end, the command runs, and the view is asked for again.
//!
//! Most tests run `serve` on a thread of the test against the terminal the test plays on the
//! other end of a pty, as the conversation's tests do; the pty is not the test's controlling
//! terminal there, so the command runs without its foreground. The last test runs the program as a
//! process of its own, with the pty for its controlling terminal, to see the command given the
//! foreground and the program take it back.

// A test fails by panicking, and the helpers here are the test's.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pencere::{
    Bundle, CloseResult, EndedBy, HandOffResult, Kind, NoViewReason, Options, Outcome,
    ReopenResult, View, ViewControl,
};
use serde_json::{Value, json};

mod common;

use common::*;

/// `sh -c script`.
fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.args(["-c", script]);
    command
}

/// A file of the test's own, that is not there yet.
fn marker(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("pencere-handoff-{}-{name}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

fn json_of(frame: &pencere_wire::frame::Message) -> Value {
    serde_json::from_str(&frame.payload).expect("JSON")
}

/// The `dismiss` the hand-off starts with, answered: the token is the view's.
fn dismissed(pane: &mut Pane, token: &str) {
    let dismiss = pane.expect("dismiss");
    assert_eq!(json_of(&dismiss), json!({ "token": token }));
    pane.send("closed", Some(dismiss.id), &json!({ "reason": "program" }));
}

/// The terminal is the command's: the screen and the modes are as they were found. (The command
/// has to take a moment for the modes to be seen.)
fn given_back(pane: &mut Pane) {
    wait_for("the screen to be given back", || {
        pane.read_some(Duration::from_millis(20));
        pane.text().contains("\x1b[?1049l")
    });
    wait_for("the modes to be given back", || {
        termios_of(pane.slave).c_lflag & libc::ICANON != 0
    });
    modes_are_back(pane);
}

/// The view asked for again when the command has ended, as a reopening asks: a new `hello`, with
/// the modes of the protocol and without the fence, and an `open` with the new token.
fn shown_again(pane: &mut Pane, views: &Value) -> pencere_wire::frame::Message {
    let hello = pane.expect("hello");
    assert_eq!(
        pane.text().matches("\x1b[c").count(),
        1,
        "the fence is for the first hello only"
    );
    let modes = termios_of(pane.master);
    assert_eq!(modes.c_lflag & (libc::ICANON | libc::ECHO), 0, "raw again");
    assert_ne!(modes.c_lflag & libc::ISIG, 0, "signals on");
    pane.welcome(hello.id, TOKEN_2, views);
    let open = pane.expect("open");
    assert_eq!(json_of(&open)["token"], TOKEN_2);
    open
}

/// ⌃C ends the view shown again, with its token.
fn interrupted(pane: &mut Pane, handle: std::thread::JoinHandle<Result<Outcome, pencere::Error>>) {
    interrupt();
    let bye = pane.expect("bye");
    assert_eq!(json_of(&bye)["token"], TOKEN_2);
    pane.send("closed", Some(bye.id), &json!({ "reason": "program" }));
    let Outcome::Closed(closed) = finished(handle) else {
        panic!("a view that closed");
    };
    assert_eq!(closed.by, EndedBy::Signal(libc::SIGINT));
    modes_are_back(pane);
}

fn code_of(result: HandOffResult) -> Option<i32> {
    match result {
        HandOffResult::Returned(Ok(status)) => status.code(),
        other => panic!("a command that ran and a view that came back: {other:?}"),
    }
}

#[test]
fn a_command_given_the_terminal_runs_and_the_view_comes_back_with_the_same_handlers() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let handle = serve_on(pane.terminal(), move |options| {
        options
            .control(given)
            .call("ping", |_params, _call| Ok(json!({ "pong": true })))
    });
    shown(&mut pane);
    let lending = {
        let control = control.clone();
        asking(move || control.hand_off(shell("sleep 0.2; exit 7")).wait())
    };
    dismissed(&mut pane, TOKEN);
    given_back(&mut pane);
    let open = shown_again(&mut pane, &web_views());
    assert_eq!(
        pane.text().matches("\x1b[?1049h").count(),
        2,
        "the view comes back on the alternate screen"
    );
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    assert_eq!(code_of(joined(lending)), Some(7));
    // The view shown again is served by the same handlers.
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 1, "method": "ping" } }),
    );
    assert_eq!(
        json_of(&pane.expect("msg")),
        json!({ "response": { "id": 1, "result": { "pong": true } } })
    );
    interrupted(&mut pane, handle);
}

#[test]
fn the_answer_of_the_handler_that_hands_off_goes_out_before_the_view_closes() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let lender = control.clone();
    let result: Arc<Mutex<Option<HandOffResult>>> = Arc::new(Mutex::new(None));
    let slot = Arc::clone(&result);
    let handle = serve_on(pane.terminal(), move |options| {
        options.control(given).call("shell", move |_params, _call| {
            let handoff = lender.hand_off(shell("sleep 0.2; exit 3"));
            let slot = Arc::clone(&slot);
            std::thread::spawn(move || {
                *slot.lock().expect("the slot") = Some(handoff.wait());
            });
            Ok(json!("scheduled"))
        })
    });
    shown(&mut pane);
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 1, "method": "shell" } }),
    );
    assert_eq!(
        json_of(&pane.expect("msg")),
        json!({ "response": { "id": 1, "result": "scheduled" } })
    );
    dismissed(&mut pane, TOKEN);
    given_back(&mut pane);
    let open = shown_again(&mut pane, &web_views());
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    wait_for("the hand-off to be over", || {
        result.lock().expect("the slot").is_some()
    });
    let done = result.lock().expect("the slot").take().expect("a result");
    assert_eq!(code_of(done), Some(3));
    interrupted(&mut pane, handle);
}

#[test]
fn a_handler_that_waits_for_its_own_hand_off_is_not_waited_for() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let lender = control.clone();
    let result: Arc<Mutex<Option<HandOffResult>>> = Arc::new(Mutex::new(None));
    let slot = Arc::clone(&result);
    let handle = serve_on(pane.terminal(), move |options| {
        options.control(given).call("shell", move |_params, _call| {
            let done = lender.hand_off(shell("sleep 0.2; exit 0")).wait();
            *slot.lock().expect("the slot") = Some(done);
            Ok(json!("back"))
        })
    });
    shown(&mut pane);
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 1, "method": "shell" } }),
    );
    // The hand-off starts at once: the handler's answer would come after it, and is the old view's.
    dismissed(&mut pane, TOKEN);
    given_back(&mut pane);
    let open = shown_again(&mut pane, &web_views());
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    wait_for("the hand-off to be over", || {
        result.lock().expect("the slot").is_some()
    });
    let done = result.lock().expect("the slot").take().expect("a result");
    assert_eq!(code_of(done), Some(0));
    assert!(
        pane.next_within(Duration::from_millis(200)).is_none(),
        "the answer of a call of the old view does not reach the new one: {:?}",
        pane.text()
    );
    interrupted(&mut pane, handle);
}

#[test]
fn a_command_that_cannot_be_started_is_said_so_and_the_view_comes_back() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let handle = serve_on(pane.terminal(), move |options| options.control(given));
    shown(&mut pane);
    let lending = {
        let control = control.clone();
        asking(move || {
            control
                .hand_off(Command::new("no-such-program-here"))
                .wait()
        })
    };
    dismissed(&mut pane, TOKEN);
    let open = shown_again(&mut pane, &web_views());
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    match joined(lending) {
        HandOffResult::Returned(Err(why)) => {
            assert_eq!(why.kind(), std::io::ErrorKind::NotFound, "{why}");
        }
        other => panic!("a command that could not be started: {other:?}"),
    }
    interrupted(&mut pane, handle);
}

#[test]
fn a_terminal_that_cannot_close_the_view_is_not_lent_and_the_command_does_not_run() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let handle = serve_on(pane.terminal(), move |options| options.control(given));
    let hello = pane.expect("hello");
    pane.send(
        "welcome",
        Some(hello.id),
        &json!({
            "version": 1,
            "token": TOKEN,
            "host": { "name": "test", "version": "0" },
            "features": [],
            "views": web_views(),
        }),
    );
    let open = pane.expect("open");
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    let ran = marker("unsupported");
    let script = format!("touch '{}'", ran.display());
    assert!(matches!(
        control.hand_off(shell(&script)).wait(),
        HandOffResult::Unsupported
    ));
    assert!(
        pane.next_within(Duration::from_millis(200)).is_none(),
        "nothing is written to a terminal that would ignore it: {:?}",
        pane.text()
    );
    assert!(!ran.exists(), "the command did not run");
    interrupt();
    let bye = pane.expect("bye");
    pane.send("closed", Some(bye.id), &json!({ "reason": "program" }));
    finished(handle);
}

#[test]
fn there_is_one_hand_off_at_a_time_and_none_before_the_view_is_up() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let handle = serve_on(pane.terminal(), move |options| options.control(given));
    // The control is `serve`'s from its `hello` on: there is no view yet to close.
    let hello = pane.expect("hello");
    assert!(matches!(
        control.hand_off(shell("exit 0")).wait(),
        HandOffResult::Unavailable
    ));
    pane.welcome(hello.id, TOKEN, &web_views());
    let open = pane.expect("open");
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    let started = marker("busy");
    let script = format!("touch '{}'; sleep 0.3", started.display());
    let lending = {
        let control = control.clone();
        asking(move || control.hand_off(shell(&script)).wait())
    };
    dismissed(&mut pane, TOKEN);
    given_back(&mut pane);
    wait_for("the command to start", || started.exists());
    // While the command has the terminal another is not given it, and there is no view to close.
    assert!(matches!(
        control.hand_off(shell("exit 0")).wait(),
        HandOffResult::Busy
    ));
    assert_eq!(control.close(), CloseResult::Unavailable);
    let open = shown_again(&mut pane, &web_views());
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    assert_eq!(code_of(joined(lending)), Some(0));
    let _ = std::fs::remove_file(&started);
    interrupted(&mut pane, handle);
}

#[test]
fn a_reopening_while_the_command_has_the_terminal_is_the_view_that_comes_back() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let handle = serve_on(pane.terminal(), move |options| options.control(given));
    shown(&mut pane);
    let started = marker("reopen");
    let script = format!("touch '{}'; sleep 0.3", started.display());
    let lending = {
        let control = control.clone();
        asking(move || control.hand_off(shell(&script)).wait())
    };
    dismissed(&mut pane, TOKEN);
    given_back(&mut pane);
    wait_for("the command to start", || started.exists());
    // The files of the view changed while the person was in the shell.
    let page = Bundle::single("page.lua", &b"return {}"[..]).expect("a bundle");
    let reopening = {
        let control = control.clone();
        asking(move || control.reopen(Some(vec![View::new(Kind::Lua, &["2.0"], page)])))
    };
    let lua = json!([{ "kind": "lua", "versions": ["2.0"] }]);
    let open = shown_again(&mut pane, &lua);
    let body = json_of(&open);
    assert_eq!(body["view"], "lua");
    assert_eq!(body["bundle"]["entry"], "page.lua");
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    assert_eq!(
        joined(reopening).expect("a reopening"),
        ReopenResult::Reopened
    );
    assert_eq!(code_of(joined(lending)), Some(0));
    let _ = std::fs::remove_file(&started);
    interrupted(&mut pane, handle);
}

#[test]
fn a_hangup_while_the_command_has_the_terminal_is_passed_on_and_ends_serve_once_it_has_ended() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let fell_back = Arc::new(Mutex::new(false));
    let fell = Arc::clone(&fell_back);
    let handle = serve_on(pane.terminal(), move |options| {
        options
            .control(given)
            .fallback(move |_why| *fell.lock().expect("the flag") = true)
    });
    shown(&mut pane);
    let started = marker("hangup");
    // A command that would take longer than the test waits, unless the hangup reaches it.
    let script = format!("touch '{}'; exec sleep 30", started.display());
    let lending = {
        let control = control.clone();
        asking(move || control.hand_off(shell(&script)).wait())
    };
    dismissed(&mut pane, TOKEN);
    given_back(&mut pane);
    wait_for("the command to start", || started.exists());
    // SAFETY: the handlers of `serve` are installed; this is the hangup of the terminal.
    unsafe {
        libc::raise(libc::SIGHUP);
    }
    match joined(lending) {
        HandOffResult::NoView {
            status: Some(Ok(status)),
            no_view,
        } => {
            assert_eq!(status.signal(), Some(libc::SIGHUP), "{status:?}");
            assert_eq!(no_view.reason, NoViewReason::Interrupted(libc::SIGHUP));
        }
        other => panic!("a command ended by the hangup, and no view: {other:?}"),
    }
    let Outcome::NoView(no_view) = finished(handle) else {
        panic!("no view shown again");
    };
    assert_eq!(no_view.reason, NoViewReason::Interrupted(libc::SIGHUP));
    assert!(
        !*fell_back.lock().expect("the flag"),
        "a signal ended it: no fallback"
    );
    assert_eq!(
        pane.text().matches("pencere;hello").count(),
        1,
        "the view is not asked for again: {:?}",
        pane.text()
    );
    modes_are_back(&pane);
    let _ = std::fs::remove_file(&started);
}

#[test]
fn with_no_view_up_the_command_runs_all_the_same_and_has_the_terminal() {
    let _one = one_at_a_time();
    // A control given to no `serve`, as in a fallback: the terminal is the program's.
    let control = ViewControl::new();
    let mut command = shell("sleep 0.3; exit 5");
    command.stdin(Stdio::null());
    let lending = control.hand_off(command);
    // A `serve` started while the command has the terminal is told it is taken, and so is another
    // hand-off.
    let pane = Pane::new();
    assert!(matches!(
        pencere::serve(
            Options::new()
                .view(View::web(page()))
                .terminal(pane.terminal())
        ),
        Err(pencere::Error::Busy)
    ));
    assert!(matches!(
        control.hand_off(shell("exit 0")).wait(),
        HandOffResult::Busy
    ));
    assert_eq!(code_of(lending.wait()), Some(5));
}

#[test]
fn a_command_that_stops_is_continued_and_one_that_reaches_for_a_terminal_it_has_not_is_ended() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let handle = serve_on(pane.terminal(), move |options| options.control(given));
    shown(&mut pane);
    // A stop, as ⌃Z gives a program that is not a shell: it is continued, and ends as it would
    // have.
    let lending = {
        let control = control.clone();
        asking(move || control.hand_off(shell("kill -STOP $$; exit 6")).wait())
    };
    dismissed(&mut pane, TOKEN);
    let open = shown_again(&mut pane, &web_views());
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    assert_eq!(code_of(joined(lending)), Some(6));
    // Stopped for reaching for a terminal it is not the foreground of (the pty is not the test's
    // controlling terminal): it could never go on, and is ended as on a hangup.
    let lending = {
        let control = control.clone();
        asking(move || control.hand_off(shell("kill -TTIN $$; exit 0")).wait())
    };
    let dismiss = pane.expect("dismiss");
    pane.send("closed", Some(dismiss.id), &json!({ "reason": "program" }));
    let hello = pane.expect("hello");
    pane.welcome(hello.id, TOKEN, &web_views());
    let open = pane.expect("open");
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    match joined(lending) {
        HandOffResult::Returned(Ok(status)) => {
            assert_eq!(status.signal(), Some(libc::SIGHUP), "{status:?}");
        }
        other => panic!("a command ended by the hangup: {other:?}"),
    }
    interrupt();
    let bye = pane.expect("bye");
    pane.send("closed", Some(bye.id), &json!({ "reason": "program" }));
    finished(handle);
}

// ---- the foreground ----------------------------------------------------------------------

const MARKER: &str = "PENCERE_HANDOFF_CHILD";

/// The program: only when the test started itself as one. Its view asks for a shell, which reads
/// a line from the terminal, and then for the status the shell ended with.
#[test]
fn handoff_child() {
    if std::env::var_os(MARKER).is_none() {
        return;
    }
    let control = ViewControl::new();
    let lender = control.clone();
    let status: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));
    let set = Arc::clone(&status);
    let options = Options::new()
        .view(View::web(page()))
        .program("handoff")
        .control(control)
        // A program that is not there: its child took the foreground before it failed to start.
        .call("missing", {
            let lender = lender.clone();
            move |_params, _call| {
                let handoff = lender.hand_off(Command::new("no-such-program-here"));
                std::thread::spawn(move || handoff.wait());
                Ok(json!("scheduled"))
            }
        })
        .call("shell", move |_params, _call| {
            let handoff = lender.hand_off(shell("read line; echo \"got $line\"; exit 4"));
            let set = Arc::clone(&set);
            std::thread::spawn(move || {
                if let HandOffResult::Returned(Ok(ended)) = handoff.wait() {
                    *set.lock().expect("the status") = ended.code();
                }
            });
            Ok(json!("scheduled"))
        })
        .call("status", move |_params, call| {
            // The hand-off is told it is over as the terminal takes the open, which the view's
            // first call may beat.
            loop {
                if let Some(code) = *status.lock().expect("the status") {
                    return Ok(json!({ "code": code }));
                }
                if call.sleep(Duration::from_millis(10)) {
                    return Ok(json!(null));
                }
            }
        });
    let outcome = pencere::serve(options).expect("served");
    println!("PENCERE-ENDED-{}", outcome.shell_status());
}

/// Kills the program if the test fails before it has ended.
struct Program(Child);

impl Drop for Program {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn the_command_has_the_foreground_of_the_terminal_and_the_program_takes_it_back() {
    let mut pane = Pane::new();
    let slave = pane.slave;
    let mut command = Command::new(std::env::current_exe().expect("this test's own file"));
    command
        .args([
            "--exact",
            "handoff_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(MARKER, "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    // SAFETY: runs in the child between fork and exec, with calls that are safe there: the pty
    // becomes its standard streams and its controlling terminal, and the child the leader of a
    // session whose process group has the foreground.
    unsafe {
        command.pre_exec(move || {
            libc::setsid();
            libc::ioctl(slave, libc::TIOCSCTTY as _, 0);
            for fd in 0..3 {
                libc::dup2(slave, fd);
            }
            if slave > 2 {
                libc::close(slave);
            }
            Ok(())
        });
    }
    let program = Program(command.spawn().expect("the program starts"));

    shown(&mut pane);
    // The foreground comes back to the program after a command that could not start: otherwise
    // its read of the answer to its new `hello` would stop it.
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 1, "method": "missing" } }),
    );
    assert_eq!(
        json_of(&pane.expect("msg")),
        json!({ "response": { "id": 1, "result": "scheduled" } })
    );
    dismissed(&mut pane, TOKEN);
    let hello = pane.expect("hello");
    pane.welcome(hello.id, TOKEN, &web_views());
    let open = pane.expect("open");
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 1, "method": "shell" } }),
    );
    assert_eq!(
        json_of(&pane.expect("msg")),
        json!({ "response": { "id": 1, "result": "scheduled" } })
    );
    dismissed(&mut pane, TOKEN);
    wait_for("the modes to be given back", || {
        termios_of(pane.master).c_lflag & libc::ICANON != 0
    });
    // The shell reads the terminal, which only the foreground job may: a background one would be
    // stopped by the read.
    pane.write_raw(b"hi\n");
    wait_for("the shell to read its line", || {
        pane.read_some(Duration::from_millis(20));
        pane.text().contains("got hi")
    });
    let open = shown_again(&mut pane, &web_views());
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 1, "method": "status" } }),
    );
    assert_eq!(
        json_of(&pane.expect("msg")),
        json!({ "response": { "id": 1, "result": { "code": 4 } } })
    );
    // ⌃C reaches the program again: the foreground is its own once more.
    pane.write_raw(&[0x03]);
    let bye = pane.expect("bye");
    pane.send("closed", Some(bye.id), &json!({ "reason": "program" }));
    wait_for("the program to end", || {
        pane.read_some(Duration::from_millis(20));
        pane.text().contains("PENCERE-ENDED-")
    });
    assert!(
        pane.text().contains("PENCERE-ENDED-130"),
        "{:?}",
        pane.text()
    );
    drop(program);
}
