//! Requests answered later (`Options::call_deferred`, `Options::topic_deferred`): a program with a
//! loop of its own is given the calls and the subscriptions of the view in the order the view
//! made them, and answers each when it can, none waiting for another. Each scenario runs through a
//! terminal and in the same process (`both_ways!`).

// A test fails by panicking, and the helpers here are the test's.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pencere::{Answer, Call, Subscription, ViewError};
use serde_json::{Value, json};

mod common;

use common::*;

fn call(id: u64, method: &str, params: &Value) -> Value {
    json!({ "call": { "id": id, "method": method, "params": params } })
}

/// What the program's loop is handed, in the order the view made it.
enum Handed {
    Call(u64, Answer),
    Subscribe(String, Subscription, Answer<()>),
}

/// The handed requests, sent to the program's loop.
fn hand(to: &Arc<Mutex<Sender<Handed>>>, handed: Handed) {
    let _ = to.lock().expect("the loop").send(handed);
}

fn the_calls_reach_the_programs_loop_in_the_order_the_view_sent_them_and_wait_for_no_answer<
    S: Side,
>() {
    let (to_loop, at_loop) = channel();
    let to_loop = Arc::new(Mutex::new(to_loop));
    let mut view = S::start(move |options| {
        options.call_deferred("edit", move |params: Value, _call: Call, answer: Answer| {
            hand(
                &to_loop,
                Handed::Call(params["n"].as_u64().unwrap_or(0), answer),
            );
        })
    });
    // As many as may be out at once (64), and no more: none is answered until the loop has them all.
    for n in 1..=60 {
        view.send(&call(n, "edit", &json!({ "n": n })));
    }
    // The loop takes every call before it answers any: no call waited for the one before it.
    let mut taken = Vec::new();
    for _ in 1..=60 {
        match at_loop.recv_timeout(Duration::from_secs(10)) {
            Ok(Handed::Call(n, answer)) => taken.push((n, answer)),
            _ => panic!("a call"),
        }
    }
    assert_eq!(
        taken.iter().map(|(n, _)| *n).collect::<Vec<u64>>(),
        (1..=60).collect::<Vec<u64>>()
    );
    for (n, answer) in taken.into_iter().rev() {
        answer.send(Ok(json!({ "n": n })));
    }
    for n in (1..=60).rev() {
        assert_eq!(
            view.said(),
            json!({ "response": { "id": n, "result": { "n": n } } })
        );
    }
    view.end();
}

fn an_answer_given_at_once_one_dropped_and_a_panic_each_end_their_call<S: Side>() {
    let mut view = S::start(|options| {
        options
            .call_deferred("now", |params: Value, _call: Call, answer: Answer| {
                answer.send(Ok(params));
            })
            .call_deferred("dropped", |_params: Value, _call: Call, _answer: Answer| {})
            .call_deferred("owed", |_params: Value, _call: Call, answer: Answer| {
                drop(answer.unanswered(ViewError::with_message("unavailable", "the loop ended")));
            })
            .call_deferred("panics", |_params: Value, _call: Call, _answer: Answer| {
                panic!("the loop is gone");
            })
            .call_deferred("fails", |_params: Value, _call: Call, answer: Answer| {
                answer.send::<Value>(Err(ViewError::with_message("bad-params", "no path")));
            })
    });
    view.send(&call(1, "now", &json!({ "at": 3 })));
    assert_eq!(
        view.said(),
        json!({ "response": { "id": 1, "result": { "at": 3 } } })
    );
    view.send(&call(2, "dropped", &json!(null)));
    let dropped = view.said();
    assert_eq!(dropped["response"]["id"], 2);
    assert_eq!(dropped["response"]["error"]["code"], "internal");
    view.send(&call(5, "owed", &json!(null)));
    let owed = view.said();
    assert_eq!(owed["response"]["error"]["code"], "unavailable");
    assert_eq!(owed["response"]["error"]["message"], "the loop ended");
    view.send(&call(3, "panics", &json!(null)));
    let panicked = view.said();
    assert_eq!(panicked["response"]["error"]["code"], "internal");
    assert_eq!(panicked["response"]["error"]["message"], "the loop is gone");
    view.send(&call(4, "fails", &json!(null)));
    assert_eq!(view.said()["response"]["error"]["code"], "bad-params");
    view.end();
}

fn a_subscription_is_handed_in_order_among_the_calls_and_says_whether_it_started<S: Side>() {
    let (to_loop, at_loop) = channel();
    let to_loop = Arc::new(Mutex::new(to_loop));
    let to_calls = Arc::clone(&to_loop);
    let mut view = S::start(move |options| {
        options
            .topic_deferred(
                "changes",
                move |params: Value, feed: Subscription, started: Answer<()>| {
                    hand(
                        &to_loop,
                        Handed::Subscribe(params["of"].to_string(), feed, started),
                    );
                },
            )
            .call_deferred(
                "save",
                move |_params: Value, _call: Call, answer: Answer| {
                    hand(&to_calls, Handed::Call(0, answer));
                },
            )
    });
    view.send(&json!({ "subscribe": { "id": 1, "topic": "changes", "params": { "of": "a" } } }));
    view.send(&call(2, "save", &json!(null)));
    view.send(&json!({ "subscribe": { "id": 3, "topic": "changes", "params": { "of": "b" } } }));
    let next = || at_loop.recv_timeout(Duration::from_secs(10));
    let Ok(Handed::Subscribe(first, feed, started)) = next() else {
        panic!("the subscription first");
    };
    assert_eq!(first, "\"a\"");
    let Ok(Handed::Call(_, saved)) = next() else {
        panic!("the call second");
    };
    let Ok(Handed::Subscribe(third, _, refused)) = next() else {
        panic!("the other subscription third");
    };
    assert_eq!(third, "\"b\"");
    started.send(Ok(()));
    assert!(feed.push(json!({ "n": 1 })));
    assert_eq!(
        view.said(),
        json!({ "push": { "id": 1, "value": { "n": 1 } } })
    );
    refused.send(Err(ViewError::with_message("not-found", "no such file")));
    let refusal = view.said();
    assert_eq!(refusal["response"]["id"], 3);
    assert_eq!(refusal["response"]["error"]["code"], "not-found");
    saved.send(Ok(json!("saved")));
    assert_eq!(
        view.said(),
        json!({ "response": { "id": 2, "result": "saved" } })
    );
    view.end();
}

crate::both_ways!(
    the_calls_reach_the_programs_loop_in_the_order_the_view_sent_them_and_wait_for_no_answer,
    an_answer_given_at_once_one_dropped_and_a_panic_each_end_their_call,
    a_subscription_is_handed_in_order_among_the_calls_and_says_whether_it_started,
);
