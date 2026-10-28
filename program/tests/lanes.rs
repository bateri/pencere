//! Calls in order (`Options::call_in`): the calls of a lane run one at a time, in the order the
//! view sent them, and the calls of other lanes, and the free ones, do not wait for them.
//!
//! `serve` runs on a thread of the test and the test plays the terminal on the other end of a pty,
//! as the conversation's tests do.

// A test fails by panicking, and the helpers here are the test's.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pencere::{ReopenResult, ViewControl, ViewError};
use serde_json::{Value, json};

mod common;

use common::*;

fn call(pane: &mut Pane, id: u64, method: &str, params: &Value) {
    pane.send(
        "msg",
        None,
        &json!({ "call": { "id": id, "method": method, "params": params } }),
    );
}

/// The next message of the six the program wrote.
fn said(pane: &mut Pane) -> Value {
    serde_json::from_str(&pane.expect("msg").payload).expect("JSON")
}

/// A gate a handler waits at until the test opens it, whatever becomes of its call.
#[derive(Clone, Default)]
struct Gate(Arc<Mutex<Vec<String>>>);

impl Gate {
    fn wait(&self, name: &str) {
        while !self
            .0
            .lock()
            .expect("the gate")
            .iter()
            .any(|open| open == name)
        {
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn open(&self, name: &str) {
        self.0.lock().expect("the gate").push(name.to_owned());
    }
}

fn ended(
    pane: &mut Pane,
    handle: std::thread::JoinHandle<Result<pencere::Outcome, pencere::Error>>,
) {
    pane.send("closed", None, &json!({ "reason": "user" }));
    finished(handle);
}

#[test]
fn the_calls_of_a_lane_run_one_at_a_time_in_the_order_the_view_sent_them() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let order = Arc::new(Mutex::new(Vec::new()));
    let running = Arc::new(AtomicBool::new(false));
    let overlapped = Arc::new(AtomicBool::new(false));
    let (kept, busy, crossed) = (
        Arc::clone(&order),
        Arc::clone(&running),
        Arc::clone(&overlapped),
    );
    let handle = serve_on(pane.terminal(), move |options| {
        options.call_in("edits", "append", move |params, _call| {
            if busy.swap(true, Ordering::SeqCst) {
                crossed.store(true, Ordering::SeqCst);
            }
            let n = params["n"].as_u64().unwrap_or(0);
            // Calls that take different times, which would finish out of order if they ran
            // together.
            std::thread::sleep(Duration::from_millis((n * 7) % 5));
            kept.lock().expect("the order").push(n);
            busy.store(false, Ordering::SeqCst);
            Ok(json!({ "n": n }))
        })
    });
    shown(&mut pane);
    for n in 1..=100 {
        call(&mut pane, n, "append", &json!({ "n": n }));
    }
    for n in 1..=100 {
        assert_eq!(
            said(&mut pane),
            json!({ "response": { "id": n, "result": { "n": n } } })
        );
    }
    assert_eq!(
        *order.lock().expect("the order"),
        (1..=100).collect::<Vec<u64>>()
    );
    assert!(!overlapped.load(Ordering::SeqCst), "one at a time");
    ended(&mut pane, handle);
}

#[test]
fn two_lanes_and_the_free_calls_do_not_wait_for_one_another() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let gate = Gate::default();
    let held = gate.clone();
    let handle = serve_on(pane.terminal(), move |options| {
        options
            .call_in("slow", "hold", move |_params, _call| {
                held.wait("hold");
                Ok(json!("held"))
            })
            .call_in("quick", "quick", |_params, _call| Ok(json!("quick")))
            .call("free", |_params, _call| Ok(json!("free")))
    });
    shown(&mut pane);
    call(&mut pane, 1, "hold", &json!(null));
    call(&mut pane, 2, "quick", &json!(null));
    call(&mut pane, 3, "free", &json!(null));
    let mut answered = vec![said(&mut pane), said(&mut pane)];
    answered.sort_by_key(|message| message["response"]["id"].as_u64());
    assert_eq!(
        answered,
        vec![
            json!({ "response": { "id": 2, "result": "quick" } }),
            json!({ "response": { "id": 3, "result": "free" } }),
        ]
    );
    gate.open("hold");
    assert_eq!(
        said(&mut pane),
        json!({ "response": { "id": 1, "result": "held" } })
    );
    ended(&mut pane, handle);
}

#[test]
fn a_call_cancelled_before_its_turn_does_not_run_and_is_not_answered() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let gate = Gate::default();
    let held = gate.clone();
    let marks = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&marks);
    let handle = serve_on(pane.terminal(), move |options| {
        options
            .call_in("lane", "hold", move |_params, _call| {
                held.wait("hold");
                Ok(json!("held"))
            })
            .call_in("lane", "mark", move |_params, _call| {
                Ok(json!(counted.fetch_add(1, Ordering::SeqCst) + 1))
            })
            .call("ping", |_params, _call| Ok(json!("pong")))
    });
    shown(&mut pane);
    call(&mut pane, 1, "hold", &json!(null));
    call(&mut pane, 2, "mark", &json!(null));
    pane.send("msg", None, &json!({ "cancel": { "id": 2 } }));
    call(&mut pane, 3, "mark", &json!(null));
    // The program reads what the view says in order: once a call sent after the cancel is
    // answered, the cancel has been read, and the lane can be let go.
    call(&mut pane, 4, "ping", &json!(null));
    assert_eq!(
        said(&mut pane),
        json!({ "response": { "id": 4, "result": "pong" } })
    );
    gate.open("hold");
    assert_eq!(
        said(&mut pane),
        json!({ "response": { "id": 1, "result": "held" } })
    );
    assert_eq!(
        said(&mut pane),
        json!({ "response": { "id": 3, "result": 1 } }),
        "the cancelled call did not run, and the lane went on"
    );
    assert_eq!(marks.load(Ordering::SeqCst), 1);
    ended(&mut pane, handle);
}

#[test]
fn a_handler_of_a_lane_that_panics_is_an_internal_error_and_the_lane_goes_on() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let handle = serve_on(pane.terminal(), |options| {
        options
            .call_in(
                "lane",
                "boom",
                |_params, _call| -> Result<Value, ViewError> { panic!("the buffer is gone") },
            )
            .call_in("lane", "fine", |_params, _call| Ok(json!({ "ok": true })))
    });
    shown(&mut pane);
    call(&mut pane, 1, "boom", &json!(null));
    call(&mut pane, 2, "fine", &json!(null));
    let reply = said(&mut pane);
    assert_eq!(reply["response"]["id"], 1);
    assert_eq!(reply["response"]["error"]["code"], "internal");
    assert_eq!(reply["response"]["error"]["message"], "the buffer is gone");
    assert_eq!(
        said(&mut pane),
        json!({ "response": { "id": 2, "result": { "ok": true } } })
    );
    assert!(!pane.text().contains("panicked"), "{:?}", pane.text());
    ended(&mut pane, handle);
}

#[test]
fn a_lane_keeps_its_order_from_one_view_to_the_next() {
    let _one = one_at_a_time();
    let mut pane = Pane::new();
    let control = ViewControl::new();
    let given = control.clone();
    let gate = Gate::default();
    let held = gate.clone();
    let order = Arc::new(Mutex::new(Vec::new()));
    let (first, then) = (Arc::clone(&order), Arc::clone(&order));
    let running = Arc::new(AtomicBool::new(false));
    let started = Arc::clone(&running);
    let handle = serve_on(pane.terminal(), move |options| {
        options
            .control(given)
            // A handler that does not stop when its view ends.
            .call_in("edits", "hold", move |_params, _call| {
                started.store(true, Ordering::SeqCst);
                held.wait("hold");
                first.lock().expect("the order").push("hold");
                Ok(json!("held"))
            })
            .call_in("edits", "edit", move |_params, _call| {
                then.lock().expect("the order").push("edit");
                Ok(json!("edited"))
            })
    });
    shown(&mut pane);
    call(&mut pane, 1, "hold", &json!(null));
    wait_for("the old view's call to run", || {
        running.load(Ordering::SeqCst)
    });
    let reopener = {
        let control = control.clone();
        asking(move || control.reopen(None))
    };
    let dismiss = pane.expect("dismiss");
    pane.send("closed", Some(dismiss.id), &json!({ "reason": "program" }));
    let hello = pane.expect("hello");
    pane.welcome(hello.id, TOKEN_2, &web_views());
    let open = pane.expect("open");
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    assert_eq!(
        joined(reopener).expect("a reopening"),
        ReopenResult::Reopened
    );
    // The new view's edit waits for the old view's call, which is still running.
    call(&mut pane, 1, "edit", &json!(null));
    assert!(
        pane.next_within(Duration::from_millis(200)).is_none(),
        "{:?}",
        pane.text()
    );
    gate.open("hold");
    assert_eq!(
        said(&mut pane),
        json!({ "response": { "id": 1, "result": "edited" } })
    );
    assert_eq!(*order.lock().expect("the order"), vec!["hold", "edit"]);
    ended(&mut pane, handle);
}
