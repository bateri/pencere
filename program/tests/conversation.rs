//! The program's side of the conversation, against a terminal played by the test on the other
//! end of a pty: the handshake, the open and the files, the calls and the subscriptions of the
//! view, the way out, and the state the tty is left in.
//!
//! `serve` runs on a thread of the test, on the slave end of the pty; the test is the terminal,
//! on the master end. Signals are real (`raise`): the handlers `serve` installs are the
//! process's for as long as it runs, which is why the tests run one at a time.

// A test fails by panicking, and the helpers here are the test's.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pencere::{
    Bundle, CloseResult, EndedBy, Error, File, Kind, NoViewReason, Options, Outcome, ReopenResult,
    Terminal, View, ViewControl, ViewError,
};
use pencere_wire::frame::{Direction, Event, Message as Frame, StreamDecoder};
use serde_json::{Value, json};

mod common;

use common::*;

/// Runs `serve` on a thread of the test.
fn serve_on(
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

fn finished(handle: JoinHandle<Result<Outcome, Error>>) -> Outcome {
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

// ---- the way in ---------------------------------------------------------------------

#[test]
fn a_program_goes_from_the_hello_to_the_interrupt_and_leaves_the_tty_as_it_found_it() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let topic_ended = Arc::new(AtomicBool::new(false));
    let ended = Arc::clone(&topic_ended);
    let handle = serve_on(pane.terminal(), move |options| {
        options
            .call("list", |_params, _call| {
                Ok(json!({ "containers": ["web"] }))
            })
            .call("stop", |params, _call| match params["id"].as_str() {
                Some(id) => Ok(json!({ "stopped": id })),
                None => Err(ViewError::with_message("bad-params", "which one?")),
            })
            .call("nothing", |_params, _call| Ok(()))
            .topic("ticks", move |_params, subscription| {
                let ended = Arc::clone(&ended);
                subscription.on_end(move || ended.store(true, Ordering::SeqCst));
                let feed = subscription.clone();
                std::thread::spawn(move || {
                    let mut n = 0;
                    while !feed.is_cancelled() {
                        n += 1;
                        feed.push(json!({ "n": n }));
                        feed.sleep(Duration::from_millis(20));
                    }
                });
                Ok(())
            })
    });

    // The hello and the fence, in this order, and the hello is the fixture's.
    let (hello, open) = pane.open_a_view(&web_views());
    assert_eq!(hello, 1);
    assert!(pane.text().contains("\x1b[c"), "the fence");
    let alternate = pane
        .text()
        .find("\x1b[?1049h")
        .expect("the alternate screen");
    assert!(alternate < pane.text().find("pencere;open").expect("an open"));
    let open_body: Value = serde_json::from_str(&open.payload).expect("JSON");
    assert_eq!(open_body["token"], TOKEN);
    assert_eq!(open_body["view"], "web");
    assert_eq!(open_body["version"], "1.0");
    assert_eq!(open_body["program"], "test");
    assert_eq!(open_body["bundle"]["entry"], "index.html");

    // The terminal lacks one file of the two: only that one is sent.
    let app = digest_of(b"let a = 1;");
    pane.send("need", Some(open.id), &json!({ "missing": [app] }));
    let file = pane.expect("file");
    assert_eq!(file.digest(), Some(app.as_str()));
    assert_eq!(
        pencere_wire::base64::decode(file.payload.as_bytes()).as_deref(),
        Some(&b"let a = 1;"[..])
    );

    // Calls: a result, an error with words, a result that is nothing, a name nobody serves.
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 1, "method": "list" } }),
    );
    let reply: Value = serde_json::from_str(&pane.expect("msg").payload).expect("JSON");
    assert_eq!(
        reply,
        json!({ "response": { "id": 1, "result": { "containers": ["web"] } } })
    );
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 2, "method": "stop", "params": { "id": "web" } } }),
    );
    let reply: Value = serde_json::from_str(&pane.expect("msg").payload).expect("JSON");
    assert_eq!(
        reply,
        json!({ "response": { "id": 2, "result": { "stopped": "web" } } })
    );
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 3, "method": "stop" } }),
    );
    let reply: Value = serde_json::from_str(&pane.expect("msg").payload).expect("JSON");
    assert_eq!(
        reply,
        json!({ "response": { "id": 3, "error": { "code": "bad-params", "message": "which one?" } } })
    );
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 4, "method": "nothing" } }),
    );
    let reply: Value = serde_json::from_str(&pane.expect("msg").payload).expect("JSON");
    assert_eq!(reply, json!({ "response": { "id": 4, "result": {} } }));
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 5, "method": "nobody" } }),
    );
    let reply: Value = serde_json::from_str(&pane.expect("msg").payload).expect("JSON");
    assert_eq!(reply["response"]["error"]["code"], "not-found");

    // A subscription pushes until the view cancels it; one nobody serves is answered at once.
    pane.send(
        "msg",
        None,
        &json!({ "subscribe": { "id": 6, "topic": "ticks" } }),
    );
    for _ in 0..2 {
        let push: Value = serde_json::from_str(&pane.expect("msg").payload).expect("JSON");
        assert_eq!(push["push"]["id"], 6, "{push}");
    }
    pane.send("msg", None, &json!({ "cancel": { "id": 6 } }));
    let until = Instant::now() + WITHIN;
    while !topic_ended.load(Ordering::SeqCst) {
        assert!(
            Instant::now() < until,
            "the cleanup of the topic did not run"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    pane.send(
        "msg",
        None,
        &json!({ "subscribe": { "id": 7, "topic": "nobody" } }),
    );
    // The pushes that were on their way when the view cancelled are not delivered after it.
    let mut last = None;
    while let Some(frame) = pane.next_within(Duration::from_millis(300)) {
        last = Some(serde_json::from_str::<Value>(&frame.payload).expect("JSON"));
        if last.as_ref().is_some_and(|m| m["response"]["id"] == 7) {
            break;
        }
    }
    let last = last.expect("the answer to the subscription nobody serves");
    assert_eq!(last["response"]["error"]["code"], "not-found", "{last}");

    // ⌃C: a `bye` with the token and the reason, then the answer to it is waited for.
    interrupt();
    let bye = pane.expect("bye");
    let body: Value = serde_json::from_str(&bye.payload).expect("JSON");
    assert_eq!(body, json!({ "token": TOKEN, "reason": "signal" }));
    pane.send("closed", Some(bye.id), &json!({ "reason": "program" }));
    let outcome = finished(handle);
    let Outcome::Closed(closed) = outcome else {
        panic!("closed: {outcome:?}");
    };
    assert_eq!(closed.by, EndedBy::Signal(libc::SIGINT));
    assert_eq!(closed.view.kind, Kind::Web);
    assert_eq!(closed.view.version, "1.0");
    assert_eq!(
        Outcome::Closed(closed).shell_status(),
        130,
        "a shell calls it as it calls ⌃C"
    );

    // The screen is the shell's again, in the modes the shell had, with nothing left to read.
    while pane.read_some(Duration::from_millis(100)) {}
    assert!(pane.text().ends_with("\x1b[?1049l"), "{:?}", pane.text());
    modes_are_back(&pane);
    assert_eq!(queued_input(&pane), 0);
}

#[test]
fn a_bye_that_is_not_answered_ends_the_wait_and_throws_away_what_is_unread() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let handle = serve_on(pane.terminal(), |options| {
        options.bye_timeout(Duration::from_millis(150))
    });
    let (_, open) = pane.open_a_view(&web_views());
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    interrupt();
    let _bye = pane.expect("bye");
    // The terminal never answers; what it wrote before it knew is in the queue of the tty.
    pane.write_raw(b"left over");
    let started = Instant::now();
    let outcome = finished(handle);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(matches!(
        outcome,
        Outcome::Closed(pencere::Closed {
            by: EndedBy::Signal(_),
            ..
        })
    ));
    assert_eq!(
        queued_input(&pane),
        0,
        "what was not read is not left for the shell"
    );
    modes_are_back(&pane);
}

#[test]
fn a_terminal_that_is_not_reading_cannot_keep_the_program_from_leaving() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let handle = serve_on(pane.terminal(), |options| {
        options
            .bye_timeout(Duration::from_millis(200))
            .topic("flood", |_params, subscription| {
                let feed = subscription.clone();
                std::thread::spawn(move || {
                    let line = "x".repeat(500);
                    while feed.push(json!({ "line": line })) {}
                });
                Ok(())
            })
    });
    let (_, open) = pane.open_a_view(&web_views());
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    pane.send(
        "msg",
        None,
        &json!({ "subscribe": { "id": 1, "topic": "flood" } }),
    );
    // The terminal reads nothing for a while: the tty fills and the pushes wait.
    std::thread::sleep(Duration::from_millis(500));
    let started = Instant::now();
    interrupt();
    let outcome = finished(handle);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "left after {:?}",
        started.elapsed()
    );
    assert!(matches!(outcome, Outcome::Closed(_)), "{outcome:?}");
    modes_are_back(&pane);
    // Everything the program managed to say is a whole frame: the stream is not cut inside one.
    let mut events = Vec::new();
    let mut decoder = StreamDecoder::new(Direction::ToHost);
    while pane.read_some(Duration::from_millis(100)) {}
    decoder.feed(&pane.seen, &mut events);
    assert!(
        events.iter().all(|e| matches!(e, Event::Message(_))),
        "a frame was cut"
    );
}

// ---- no view ----------------------------------------------------------------------------

fn no_view_of(outcome: Outcome) -> pencere::NoView {
    match outcome {
        Outcome::NoView(no_view) => no_view,
        other => panic!("no view: {other:?}"),
    }
}

#[test]
fn the_reply_to_the_fence_before_any_welcome_is_no_host_and_a_late_welcome_is_ignored() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let why = Arc::new(Mutex::new(None));
    let kept = Arc::clone(&why);
    let handle = serve_on(pane.terminal(), move |options| {
        options.fallback(move |no_view| {
            *kept.lock().expect("lock") = Some(no_view.reason);
        })
    });
    let hello = pane.expect("hello");
    pane.write_raw(REPLY);
    // A welcome that comes after the reply is not believed (§5.3).
    pane.welcome(hello.id, TOKEN, &web_views());
    let no_view = no_view_of(finished(handle));
    assert_eq!(no_view.reason, NoViewReason::NoHost);
    assert_eq!(*why.lock().expect("lock"), Some(NoViewReason::NoHost));
    assert!(!pane.text().contains("pencere;open"), "{:?}", pane.text());
    // It never took the screen: there was nothing to put on it.
    assert!(!pane.text().contains("\x1b[?1049h"));
    modes_are_back(&pane);
}

#[test]
fn a_late_reply_to_the_fence_is_read_and_does_not_reach_the_shell() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let handle = serve_on(pane.terminal(), |options| {
        options
            .answer_timeout(Duration::from_millis(100))
            .late_reply_grace(Duration::from_secs(2))
    });
    let _hello = pane.expect("hello");
    // Later than the program waited, earlier than it stops reading.
    std::thread::sleep(Duration::from_millis(300));
    pane.write_raw(REPLY);
    let started = Instant::now();
    let no_view = no_view_of(finished(handle));
    assert_eq!(no_view.reason, NoViewReason::NoHost);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "the program waited the whole grace: {:?}",
        started.elapsed()
    );
    assert_eq!(queued_input(&pane), 0, "the late reply was read");
    modes_are_back(&pane);
}

#[test]
fn a_terminal_that_answers_nothing_is_no_host_after_the_time_to_answer() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let handle = serve_on(pane.terminal(), |options| {
        options
            .answer_timeout(Duration::from_millis(100))
            .late_reply_grace(Duration::from_millis(100))
    });
    let _hello = pane.expect("hello");
    let no_view = no_view_of(finished(handle));
    assert_eq!(no_view.reason, NoViewReason::NoHost);
    modes_are_back(&pane);
}

#[test]
fn a_host_with_nothing_to_open_is_a_fallback_at_once_and_its_reply_is_still_read() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let handle = serve_on(pane.terminal(), |options| options);
    let hello = pane.expect("hello");
    pane.welcome(hello.id, TOKEN, &json!([]));
    // The terminal's own answer to the fence follows the welcome.
    pane.write_raw(REPLY);
    let started = Instant::now();
    let no_view = no_view_of(finished(handle));
    assert_eq!(no_view.reason, NoViewReason::NoCommonView);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(!pane.text().contains("pencere;open"));
    assert_eq!(queued_input(&pane), 0);
    modes_are_back(&pane);
}

#[test]
fn a_host_that_shares_no_version_is_no_common_version() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let handle = serve_on(pane.terminal(), |options| options);
    let hello = pane.expect("hello");
    pane.send(
        "welcome",
        Some(hello.id),
        &json!({ "supported": [2], "host": { "name": "t", "version": "0" } }),
    );
    pane.write_raw(REPLY);
    let no_view = no_view_of(finished(handle));
    assert_eq!(no_view.reason, NoViewReason::NoCommonVersion);
}

#[test]
fn a_refused_open_is_no_view_with_the_code() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let handle = serve_on(pane.terminal(), |options| options);
    let (_, open) = pane.open_a_view(&web_views());
    pane.send(
        "closed",
        Some(open.id),
        &json!({ "reason": "refused", "code": "screen" }),
    );
    let no_view = no_view_of(finished(handle));
    assert_eq!(no_view.reason, NoViewReason::Refused);
    assert_eq!(no_view.code.as_deref(), Some("screen"));
    // The terminal ended it: there is no `bye` to say, and the screen is given back.
    while pane.read_some(Duration::from_millis(100)) {}
    assert!(!pane.text().contains("pencere;bye"));
    assert!(pane.text().ends_with("\x1b[?1049l"));
    modes_are_back(&pane);
}

#[test]
fn an_error_before_the_view_ran_is_no_view_and_one_after_it_is_a_view_that_failed() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let handle = serve_on(pane.terminal(), |options| options);
    let (_, open) = pane.open_a_view(&web_views());
    pane.send(
        "closed",
        Some(open.id),
        &json!({ "reason": "error", "code": "bundle", "message": "a file did not verify" }),
    );
    let no_view = no_view_of(finished(handle));
    assert_eq!(no_view.reason, NoViewReason::Failed);
    assert_eq!(no_view.message.as_deref(), Some("a file did not verify"));

    // A view the terminal draws itself that failed ran.
    let mut pane = Pane::new();
    let handle = serve_on(pane.terminal(), |options| options);
    let (_, open) = pane.open_a_view(&web_views());
    pane.send(
        "closed",
        Some(open.id),
        &json!({ "reason": "error", "code": "view", "message": "script:12: boom" }),
    );
    let Outcome::Closed(closed) = finished(handle) else {
        panic!("closed");
    };
    assert_eq!(closed.by, EndedBy::Error);
    assert_eq!(closed.message.as_deref(), Some("script:12: boom"));
}

#[test]
fn the_person_closing_the_view_is_a_view_that_closed() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let handle = serve_on(pane.terminal(), |options| options);
    let (_, open) = pane.open_a_view(&web_views());
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    pane.send("closed", None, &json!({ "reason": "user" }));
    let Outcome::Closed(closed) = finished(handle) else {
        panic!("closed");
    };
    assert_eq!(closed.by, EndedBy::User);
    assert_eq!(Outcome::Closed(closed).shell_status(), 0);
    modes_are_back(&pane);
}

// ---- other kinds, other versions --------------------------------------------------------

#[test]
fn the_program_opens_the_first_of_its_kinds_the_terminal_lists_at_a_version_both_speak() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let handle = std::thread::spawn({
        let terminal = pane.terminal();
        move || {
            let script = Bundle::single("view.lua", &b"return {}\n"[..]).expect("a bundle");
            let component = Bundle::single("view.wasm", &b"\0asm\x01\0\0\0"[..]).expect("a bundle");
            pencere::serve(
                Options::new()
                    .view(View::new(Kind::Wasm, &["0.5", "0.4"], component))
                    .view(View::new(Kind::Lua, &["0.5"], script))
                    .view(View::web(page()))
                    .terminal(terminal),
            )
        }
    });
    let hello = pane.expect("hello");
    let body: Value = serde_json::from_str(&hello.payload).expect("JSON");
    assert_eq!(
        body["views"],
        json!([
            { "kind": "wasm", "versions": ["0.5", "0.4"] },
            { "kind": "lua", "versions": ["0.5"] },
            { "kind": "web", "versions": ["1.0"] },
        ])
    );
    // The terminal draws scripts and pages and has an older component interface: the program's
    // first kind has a version in common (0.4), and it is the one opened.
    pane.welcome(
        hello.id,
        TOKEN,
        &json!([
            { "kind": "web", "versions": ["1.0"] },
            { "kind": "lua", "versions": ["0.5"] },
            { "kind": "wasm", "versions": ["0.4"] },
        ]),
    );
    let open = pane.expect("open");
    let body: Value = serde_json::from_str(&open.payload).expect("JSON");
    assert_eq!(
        (body["view"].as_str(), body["version"].as_str()),
        (Some("wasm"), Some("0.4"))
    );
    assert_eq!(body["bundle"]["files"][0]["type"], "application/wasm");
    pane.send("closed", None, &json!({ "reason": "user" }));
    let Outcome::Closed(closed) = finished(handle) else {
        panic!("closed");
    };
    assert_eq!(closed.view.kind, Kind::Wasm);
    assert_eq!(closed.view.version, "0.4");
}

#[test]
fn a_terminal_that_asks_again_after_a_break_is_given_the_view_again() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let calls = Arc::new(AtomicU32::new(0));
    let seen = Arc::clone(&calls);
    let handle = serve_on(pane.terminal(), move |options| {
        options.call("count", move |_params, _call| {
            Ok(json!({ "n": seen.fetch_add(1, Ordering::SeqCst) + 1 }))
        })
    });
    let (_, first) = pane.open_a_view(&web_views());
    pane.send("need", Some(first.id), &json!({ "missing": [] }));

    // The terminal came up again: an unsolicited welcome (no `re`) with a new token, and the
    // program opens with it. The calls the old view had out are over with it.
    pane.send(
        "welcome",
        None,
        &json!({
            "version": 1,
            "token": TOKEN_2,
            "host": { "name": "test", "version": "0" },
            "features": ["bundle", "dismiss"],
            "views": web_views(),
            "restart": true,
        }),
    );
    let second = pane.expect("open");
    let body: Value = serde_json::from_str(&second.payload).expect("JSON");
    assert_eq!(body["token"], TOKEN_2);
    pane.send("need", Some(second.id), &json!({ "missing": [] }));
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 1, "method": "count" } }),
    );
    let reply: Value = serde_json::from_str(&pane.expect("msg").payload).expect("JSON");
    assert_eq!(reply["response"]["result"]["n"], 1);

    // A `bye` carries the token of the view that runs.
    interrupt();
    let bye = pane.expect("bye");
    let body: Value = serde_json::from_str(&bye.payload).expect("JSON");
    assert_eq!(body["token"], TOKEN_2);
    pane.send("closed", Some(bye.id), &json!({ "reason": "program" }));
    finished(handle);
}

// ---- handlers ---------------------------------------------------------------------------

#[test]
fn a_call_that_takes_a_while_reports_its_progress_and_stops_when_it_is_cancelled() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let stopped = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stopped);
    let handle = serve_on(pane.terminal(), move |options| {
        options.call("slow", move |_params, call| {
            for step in 1..=100 {
                if call.sleep(Duration::from_millis(20)) {
                    flag.store(true, Ordering::SeqCst);
                    return Ok(json!(null));
                }
                call.progress(f64::from(step), Some(100.0), Some("step"));
            }
            Ok(json!({ "done": true }))
        })
    });
    let (_, open) = pane.open_a_view(&web_views());
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 1, "method": "slow" } }),
    );
    let progress: Value = serde_json::from_str(&pane.expect("msg").payload).expect("JSON");
    assert_eq!(progress["progress"]["id"], 1);
    assert_eq!(progress["progress"]["total"], 100.0);
    assert_eq!(progress["progress"]["note"], "step");
    pane.send("msg", None, &json!({ "cancel": { "id": 1 } }));
    let until = Instant::now() + WITHIN;
    while !stopped.load(Ordering::SeqCst) {
        assert!(Instant::now() < until, "the handler was not told");
        std::thread::sleep(Duration::from_millis(5));
    }
    // What the handler returned after the cancel is not an answer.
    std::thread::sleep(Duration::from_millis(100));
    while let Some(frame) = pane.next_within(Duration::from_millis(100)) {
        let message: Value = serde_json::from_str(&frame.payload).expect("JSON");
        assert!(message.get("response").is_none(), "{message}");
    }
    pane.send("closed", None, &json!({ "reason": "user" }));
    finished(handle);
}

#[test]
fn a_handler_that_panics_is_an_internal_error_of_its_call_and_nothing_more() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let handle = serve_on(pane.terminal(), |options| {
        options
            .call("boom", |_params, _call| -> Result<Value, ViewError> {
                panic!("the container is gone")
            })
            .call("fine", |_params, _call| Ok(json!({ "ok": true })))
    });
    let (_, open) = pane.open_a_view(&web_views());
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 1, "method": "boom" } }),
    );
    let reply: Value = serde_json::from_str(&pane.expect("msg").payload).expect("JSON");
    assert_eq!(reply["response"]["error"]["code"], "internal");
    assert_eq!(
        reply["response"]["error"]["message"],
        "the container is gone"
    );
    // The view goes on, and the screen was not written on.
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 2, "method": "fine" } }),
    );
    let reply: Value = serde_json::from_str(&pane.expect("msg").payload).expect("JSON");
    assert_eq!(reply["response"]["result"], json!({ "ok": true }));
    assert!(!pane.text().contains("panicked"), "{:?}", pane.text());
    pane.send("closed", None, &json!({ "reason": "user" }));
    finished(handle);
}

#[test]
fn a_result_over_the_limit_of_a_message_is_an_error_not_a_hang() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let handle = serve_on(pane.terminal(), |options| {
        options.call("big", |_params, _call| {
            Ok(json!({ "text": "x".repeat(2 << 20) }))
        })
    });
    let (_, open) = pane.open_a_view(&web_views());
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 1, "method": "big" } }),
    );
    let reply: Value = serde_json::from_str(&pane.expect("msg").payload).expect("JSON");
    assert_eq!(reply["response"]["error"]["code"], "message-too-long");
    pane.send("closed", None, &json!({ "reason": "user" }));
    finished(handle);
}

#[test]
fn the_limit_of_a_message_is_counted_before_the_wire_escapes_it_in_both_directions() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let handle = serve_on(pane.terminal(), |options| {
        options.call("echo", |params, _call| Ok(params))
    });
    let (_, open) = pane.open_a_view(&web_views());
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    // 200 000 characters of two bytes are 400 000 as the view says them and 1 200 000 as the wire
    // carries them: under the limit of a message, and over it as the wire has it. The call is
    // read, and the answer, which is as long, is written.
    let text = "\u{e9}".repeat(200_000);
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 1, "method": "echo", "params": { "text": text } } }),
    );
    let reply: Value = serde_json::from_str(&pane.expect("msg").payload).expect("JSON");
    assert_eq!(reply["response"]["result"]["text"], text.as_str());
    // One that is over the limit as the view said it is not read at all.
    let too_long = "x".repeat(1_100_000);
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 2, "method": "echo", "params": { "text": too_long } } }),
    );
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 3, "method": "echo" } }),
    );
    let reply: Value = serde_json::from_str(&pane.expect("msg").payload).expect("JSON");
    assert_eq!(reply["response"]["id"], 3, "{reply}");
    pane.send("closed", None, &json!({ "reason": "user" }));
    finished(handle);
}

// ---- when the program itself fails ------------------------------------------------------

#[test]
fn a_panic_of_the_program_gives_the_terminal_back() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let handle = std::thread::spawn({
        let terminal = pane.terminal();
        move || {
            // The files of the view cannot be made: the panic happens on the thread of the
            // loop, with the terminal taken.
            let view =
                View::per_version(Kind::Web, &["1.0"], |_version| panic!("the files are gone"));
            pencere::serve(Options::new().view(view).terminal(terminal))
        }
    });
    let hello = pane.expect("hello");
    pane.welcome(hello.id, TOKEN, &web_views());
    let until = Instant::now() + WITHIN;
    while !handle.is_finished() {
        assert!(Instant::now() < until, "serve did not unwind");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(handle.join().is_err(), "the panic went on");
    modes_are_back(&pane);
}

#[test]
fn two_serves_do_not_share_one_terminal() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let handle = serve_on(pane.terminal(), |options| options);
    let _hello = pane.expect("hello");
    let other = Pane::new();
    let refused = pencere::serve(
        Options::new()
            .view(View::web(page()))
            .terminal(other.terminal()),
    );
    assert!(matches!(refused, Err(Error::Busy)), "{refused:?}");
    pane.write_raw(REPLY);
    finished(handle);
}

#[test]
fn what_a_program_offers_is_checked_before_the_terminal_is_touched() {
    let _one = one_at_a_time();
    let pane = Pane::new();
    for (options, why) in [
        (Options::new(), "at least one"),
        (
            Options::new()
                .view(View::web(page()))
                .view(View::web(page())),
            "twice",
        ),
        (
            Options::new().view(View::new(Kind::Wasm, &["1"], page())),
            "not a version",
        ),
        (
            Options::new().view(View::new(Kind::Wasm, &[], page())),
            "versions",
        ),
    ] {
        let error = pencere::serve(options.terminal(pane.terminal())).expect_err(why);
        assert!(error.to_string().contains(why), "{error} ({why})");
    }
    // A page is not a component.
    let error = pencere::serve(
        Options::new()
            .view(View::new(Kind::Wasm, &["1.0"], page()))
            .terminal(pane.terminal()),
    )
    .expect_err("a page is not a component");
    assert!(matches!(error, Error::Bundle(_)), "{error}");
    modes_are_back(&pane);
}

#[test]
fn where_there_is_no_tty_there_is_no_view() {
    let _one = one_at_a_time();
    let mut ends = [0 as libc::c_int; 2];
    // SAFETY: a pipe for the test.
    assert_eq!(unsafe { libc::pipe(ends.as_mut_ptr()) }, 0);
    let ran = Arc::new(AtomicBool::new(false));
    let kept = Arc::clone(&ran);
    let outcome = pencere::serve(
        Options::new()
            .view(View::web(page()))
            .terminal(Terminal::from_descriptors(ends[0], ends[1]))
            .fallback(move |no_view| {
                assert_eq!(no_view.reason, NoViewReason::NoTty);
                kept.store(true, Ordering::SeqCst);
            }),
    )
    .expect("served");
    assert!(matches!(outcome, Outcome::NoView(_)));
    assert!(ran.load(Ordering::SeqCst), "the fallback ran");
    // SAFETY: closes the pipe of the test.
    unsafe {
        libc::close(ends[0]);
        libc::close(ends[1]);
    }
}

// ---- closing the view at the program's request (§10.8) ----------------------------------------

/// Runs `call` on a thread of the test, so that the test can be the terminal while it waits.
fn asking<T: Send + 'static>(call: impl FnOnce() -> T + Send + 'static) -> JoinHandle<T> {
    std::thread::spawn(call)
}

fn joined<T>(handle: JoinHandle<T>) -> T {
    let until = Instant::now() + WITHIN;
    while !handle.is_finished() {
        assert!(Instant::now() < until, "the call does not return");
        std::thread::sleep(Duration::from_millis(5));
    }
    handle.join().expect("the call does not panic")
}

/// A view is open and the terminal has taken it (`need` with nothing missing).
fn shown(pane: &mut Pane) -> Frame {
    let (_, open) = pane.open_a_view(&web_views());
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    open
}

#[test]
fn the_view_the_program_closes_is_closed_by_the_program_and_the_terminal_is_given_back() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let handle = serve_on(pane.terminal(), move |options| options.control(given));
    shown(&mut pane);
    let closer = {
        let control = control.clone();
        asking(move || control.close())
    };
    // The `dismiss` has the token the view was opened with (§10.8).
    let dismiss = pane.expect("dismiss");
    let body: Value = serde_json::from_str(&dismiss.payload).expect("JSON");
    assert_eq!(body, json!({ "token": TOKEN }));
    pane.send("closed", Some(dismiss.id), &json!({ "reason": "program" }));
    assert_eq!(joined(closer), CloseResult::Closed);
    // The program that closed its view is told once the terminal is its own again: it may print,
    // or run a shell, as soon as it is.
    modes_are_back(&pane);
    let Outcome::Closed(closed) = finished(handle) else {
        panic!("a view that closed");
    };
    assert_eq!(closed.by, EndedBy::Program);
    // The terminal forgot the view: there is no `bye` for it.
    while pane.read_some(Duration::from_millis(100)) {}
    assert!(!pane.text().contains("pencere;bye"), "{:?}", pane.text());
    assert!(pane.text().ends_with("\x1b[?1049l"), "{:?}", pane.text());
    modes_are_back(&pane);
    // Once `serve` has ended there is nothing to close.
    assert_eq!(control.close(), CloseResult::Unavailable);
}

#[test]
fn a_reopened_view_is_asked_for_with_a_new_hello_and_a_new_token_and_without_the_fence() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let calls = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&calls);
    let handle = serve_on(pane.terminal(), move |options| {
        options.control(given).call("count", move |_params, _call| {
            Ok(json!({ "n": counted.fetch_add(1, Ordering::SeqCst) + 1 }))
        })
    });
    shown(&mut pane);
    let reopener = {
        let control = control.clone();
        asking(move || control.reopen(None))
    };
    let dismiss = pane.expect("dismiss");
    pane.send("closed", Some(dismiss.id), &json!({ "reason": "program" }));
    // The pane is idle with the alternate screen as the program left it; the program asks again.
    let hello = pane.expect("hello");
    assert_eq!(
        pane.text().matches("\x1b[c").count(),
        1,
        "the fence is for the first hello only"
    );
    pane.welcome(hello.id, TOKEN_2, &web_views());
    let open = pane.expect("open");
    let body: Value = serde_json::from_str(&open.payload).expect("JSON");
    assert_eq!(body["token"], TOKEN_2);
    assert_eq!(
        pane.text().matches("\x1b[?1049h").count(),
        1,
        "the alternate screen stays on between the two views"
    );
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    assert_eq!(
        joined(reopener).expect("a reopening"),
        ReopenResult::Reopened
    );
    // The new view is served as the first was.
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": 1, "method": "count" } }),
    );
    let reply: Value = serde_json::from_str(&pane.expect("msg").payload).expect("JSON");
    assert_eq!(
        reply,
        json!({ "response": { "id": 1, "result": { "n": 1 } } })
    );
    // ⌃C ends the second view, with the second token.
    interrupt();
    let bye = pane.expect("bye");
    let body: Value = serde_json::from_str(&bye.payload).expect("JSON");
    assert_eq!(body, json!({ "token": TOKEN_2, "reason": "signal" }));
    pane.send("closed", Some(bye.id), &json!({ "reason": "program" }));
    let Outcome::Closed(closed) = finished(handle) else {
        panic!("a view that closed");
    };
    assert_eq!(closed.by, EndedBy::Signal(libc::SIGINT));
    modes_are_back(&pane);
}

#[test]
fn a_reopening_may_bring_other_files_and_other_kinds() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let handle = serve_on(pane.terminal(), move |options| options.control(given));
    shown(&mut pane);
    let newer = Bundle::new(
        "index.html",
        [File::new("index.html", &b"<p>newer</p>"[..])],
    )
    .expect("a bundle");
    let reopener = {
        let control = control.clone();
        asking(move || control.reopen(Some(vec![View::web(newer)])))
    };
    let dismiss = pane.expect("dismiss");
    pane.send("closed", Some(dismiss.id), &json!({ "reason": "program" }));
    let hello = pane.expect("hello");
    pane.welcome(hello.id, TOKEN_2, &web_views());
    let open = pane.expect("open");
    let body: Value = serde_json::from_str(&open.payload).expect("JSON");
    // The manifest is the newer files', and only them.
    assert_eq!(body["bundle"]["files"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        body["bundle"]["files"][0]["sha256"],
        digest_of(b"<p>newer</p>")
    );
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    assert_eq!(
        joined(reopener).expect("a reopening"),
        ReopenResult::Reopened
    );
    interrupt();
    let bye = pane.expect("bye");
    pane.send("closed", Some(bye.id), &json!({ "reason": "program" }));
    finished(handle);
}

#[test]
fn a_terminal_without_dismiss_keeps_the_view_and_is_told_nothing() {
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
            "features": ["bundle"],
            "views": web_views(),
        }),
    );
    let open = pane.expect("open");
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    assert_eq!(control.close(), CloseResult::Unsupported);
    assert_eq!(
        control.reopen(None).expect("a reopening"),
        ReopenResult::Unsupported
    );
    assert!(
        pane.next_within(Duration::from_millis(200)).is_none(),
        "nothing is written to a terminal that would ignore it: {:?}",
        pane.text()
    );
    interrupt();
    let bye = pane.expect("bye");
    pane.send("closed", Some(bye.id), &json!({ "reason": "program" }));
    finished(handle);
}

#[test]
fn there_is_no_view_to_close_before_the_terminal_has_one_and_while_one_is_being_closed() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    // Not tied to a `serve` yet.
    assert_eq!(control.close(), CloseResult::Unavailable);
    let given = control.clone();
    let handle = serve_on(pane.terminal(), move |options| options.control(given));
    // Before the terminal has answered the `hello`.
    let hello = pane.expect("hello");
    assert_eq!(control.close(), CloseResult::Unavailable);
    pane.welcome(hello.id, TOKEN, &web_views());
    let open = pane.expect("open");
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    let closer = {
        let control = control.clone();
        asking(move || control.close())
    };
    let dismiss = pane.expect("dismiss");
    // A second ask while the first is under way.
    assert_eq!(control.close(), CloseResult::Unavailable);
    pane.send("closed", Some(dismiss.id), &json!({ "reason": "program" }));
    assert_eq!(joined(closer), CloseResult::Closed);
    finished(handle);
}

#[test]
fn a_terminal_that_does_not_answer_the_dismiss_is_given_up_on_with_a_bye() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let handle = serve_on(pane.terminal(), move |options| {
        options
            .control(given)
            .reopen_timeout(Duration::from_millis(200))
            .bye_timeout(Duration::from_millis(150))
    });
    shown(&mut pane);
    let reopener = {
        let control = control.clone();
        asking(move || control.reopen(None))
    };
    let _dismiss = pane.expect("dismiss");
    // The terminal says nothing: the view it may still show gets a `bye`.
    let bye = pane.expect("bye");
    let body: Value = serde_json::from_str(&bye.payload).expect("JSON");
    assert_eq!(body, json!({ "token": TOKEN, "reason": "error" }));
    match joined(reopener).expect("a reopening") {
        ReopenResult::NoView(no_view) => assert_eq!(no_view.reason, NoViewReason::Timeout),
        other => panic!("no view: {other:?}"),
    }
    assert!(matches!(finished(handle), Outcome::NoView(_)));
    modes_are_back(&pane);
}

#[test]
fn a_reopening_that_the_terminal_does_not_answer_ends_serve_as_a_first_open_that_is_not() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let seen = Arc::new(Mutex::new(None));
    let kept = Arc::clone(&seen);
    let handle = serve_on(pane.terminal(), move |options| {
        options
            .control(given)
            .reopen_timeout(Duration::from_millis(200))
            .fallback(move |no_view| {
                *kept.lock().expect("lock") = Some(no_view.reason);
            })
    });
    shown(&mut pane);
    let reopener = {
        let control = control.clone();
        asking(move || control.reopen(None))
    };
    let dismiss = pane.expect("dismiss");
    pane.send("closed", Some(dismiss.id), &json!({ "reason": "program" }));
    let _hello = pane.expect("hello");
    // Nobody answers the new hello.
    match joined(reopener).expect("a reopening") {
        ReopenResult::NoView(no_view) => assert_eq!(no_view.reason, NoViewReason::Timeout),
        other => panic!("no view: {other:?}"),
    }
    assert_eq!(no_view_of(finished(handle)).reason, NoViewReason::Timeout);
    assert_eq!(*seen.lock().expect("lock"), Some(NoViewReason::Timeout));
    modes_are_back(&pane);
}

#[test]
fn the_need_of_the_old_open_that_crosses_a_dismiss_does_not_stop_the_closing() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let handle = serve_on(pane.terminal(), move |options| options.control(given));
    // The view is opened and the program closes it before the terminal has asked for its files.
    let (_, open) = pane.open_a_view(&web_views());
    let closer = {
        let control = control.clone();
        asking(move || control.close())
    };
    let dismiss = pane.expect("dismiss");
    // The `need` of the old open was on its way: it is not about a view the program still has.
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    pane.send("closed", Some(dismiss.id), &json!({ "reason": "program" }));
    assert_eq!(joined(closer), CloseResult::Closed);
    let Outcome::Closed(closed) = finished(handle) else {
        panic!("a view that closed");
    };
    assert_eq!(closed.by, EndedBy::Program);
    modes_are_back(&pane);
}

#[test]
fn a_terminal_that_asks_for_the_view_again_while_it_is_being_closed_ends_the_closing() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let handle = serve_on(pane.terminal(), move |options| options.control(given));
    shown(&mut pane);
    let closer = {
        let control = control.clone();
        asking(move || control.close())
    };
    let _dismiss = pane.expect("dismiss");
    // The terminal came up again (§10.7) and asks for the view: that is the view now, and the
    // closing of the old one has nothing left to close.
    pane.send(
        "welcome",
        None,
        &json!({
            "version": 1,
            "token": TOKEN_2,
            "host": { "name": "test", "version": "0" },
            "features": ["bundle", "dismiss"],
            "views": web_views(),
            "restart": true,
        }),
    );
    assert_eq!(joined(closer), CloseResult::Unavailable);
    let open = pane.expect("open");
    let body: Value = serde_json::from_str(&open.payload).expect("JSON");
    assert_eq!(body["token"], TOKEN_2);
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    interrupt();
    let bye = pane.expect("bye");
    pane.send("closed", Some(bye.id), &json!({ "reason": "program" }));
    finished(handle);
}

#[test]
fn a_terminal_that_asks_again_while_the_program_asks_for_another_view_is_given_the_new_view() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let handle = serve_on(pane.terminal(), move |options| options.control(given));
    shown(&mut pane);
    let reopener = {
        let control = control.clone();
        asking(move || control.reopen(None))
    };
    let dismiss = pane.expect("dismiss");
    pane.send("closed", Some(dismiss.id), &json!({ "reason": "program" }));
    let _hello = pane.expect("hello");
    // The terminal never answers the `hello`: it comes up again and asks for the view itself.
    pane.send(
        "welcome",
        None,
        &json!({
            "version": 1,
            "token": TOKEN_2,
            "host": { "name": "test", "version": "0" },
            "features": ["bundle", "dismiss"],
            "views": web_views(),
            "restart": true,
        }),
    );
    let open = pane.expect("open");
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    interrupt();
    let bye = pane.expect("bye");
    pane.send("closed", Some(bye.id), &json!({ "reason": "program" }));
    finished(handle);
    // The reopening was overtaken: it is told there was nothing to do.
    assert_eq!(
        joined(reopener).expect("a reopening"),
        ReopenResult::Unavailable
    );
}

#[test]
fn a_terminal_that_takes_the_new_open_and_says_nothing_more_is_given_up_on() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let handle = serve_on(pane.terminal(), move |options| {
        options
            .control(given)
            .reopen_timeout(Duration::from_millis(200))
            .bye_timeout(Duration::from_millis(150))
    });
    shown(&mut pane);
    let reopener = {
        let control = control.clone();
        asking(move || control.reopen(None))
    };
    let dismiss = pane.expect("dismiss");
    pane.send("closed", Some(dismiss.id), &json!({ "reason": "program" }));
    let hello = pane.expect("hello");
    pane.welcome(hello.id, TOKEN_2, &web_views());
    let _open = pane.expect("open");
    // Neither a `need` nor a `closed`: the program leaves, with a `bye` for the view it asked for.
    let bye = pane.expect("bye");
    let body: Value = serde_json::from_str(&bye.payload).expect("JSON");
    assert_eq!(body, json!({ "token": TOKEN_2, "reason": "error" }));
    match joined(reopener).expect("a reopening") {
        ReopenResult::NoView(no_view) => assert_eq!(no_view.reason, NoViewReason::Timeout),
        other => panic!("no view: {other:?}"),
    }
    assert!(matches!(finished(handle), Outcome::NoView(_)));
    modes_are_back(&pane);
}
