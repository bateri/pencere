//! The six messages between the view and the program's handlers (specification §9), each scenario
//! run twice: through a terminal, `serve` on a pty with the test the terminal, and in the same
//! process, a `Conversation` with the test the view. The two are served by the same code, and these
//! say they behave alike. Then what only a conversation has: the requests of the program's control
//! as what it says, and the pace of its reader.

// A test fails by panicking, and the helpers here are the test's.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pencere::{
    CloseResult, Conversation, HandOffResult, Kind, Options, ReopenResult, Said, View, ViewControl,
    ViewError,
};
use serde_json::{Value, json};

mod common;

use common::*;

fn call(id: u64, method: &str, params: &Value) -> Value {
    json!({ "call": { "id": id, "method": method, "params": params } })
}

// ---- calls ------------------------------------------------------------------------------

fn a_call_is_answered_with_its_result_its_error_nothing_or_not_found<S: Side>() {
    let mut view = S::start(|options| {
        options
            .call("stop", |params, _call| match params["id"].as_str() {
                Some(id) => Ok(json!({ "stopped": id })),
                None => Err(ViewError::with_message("bad-params", "which one?")),
            })
            .call("nothing", |_params, _call| Ok(()))
    });
    view.send(&call(1, "stop", &json!({ "id": "web" })));
    assert_eq!(
        view.said(),
        json!({ "response": { "id": 1, "result": { "stopped": "web" } } })
    );
    view.send(&call(2, "stop", &json!(null)));
    assert_eq!(
        view.said(),
        json!({ "response": { "id": 2, "error": { "code": "bad-params", "message": "which one?" } } })
    );
    view.send(&call(3, "nothing", &json!(null)));
    assert_eq!(
        view.said(),
        json!({ "response": { "id": 3, "result": {} } })
    );
    view.send(&call(4, "nobody", &json!(null)));
    assert_eq!(view.said()["response"]["error"]["code"], "not-found");
    // What is not a message of the view is left, and the next one is answered.
    view.send(&json!({ "response": { "id": 5, "result": {} } }));
    view.send(&json!({ "call": { "id": 0, "method": "nothing" } }));
    view.send(&call(6, "nothing", &json!(null)));
    assert_eq!(
        view.said(),
        json!({ "response": { "id": 6, "result": {} } })
    );
    view.end();
}

fn a_call_that_takes_a_while_reports_its_progress_and_stops_when_it_is_cancelled<S: Side>() {
    let stopped = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stopped);
    let mut view = S::start(move |options| {
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
    view.send(&call(1, "slow", &json!(null)));
    let progress = view.said();
    assert_eq!(progress["progress"]["id"], 1);
    assert_eq!(progress["progress"]["total"], 100.0);
    assert_eq!(progress["progress"]["note"], "step");
    view.send(&json!({ "cancel": { "id": 1 } }));
    wait_for("the handler to be told", || stopped.load(Ordering::SeqCst));
    // What the handler returned after the cancel is not an answer; progress that was on its way
    // may still come.
    std::thread::sleep(Duration::from_millis(100));
    while !view.says_nothing_for(Duration::from_millis(100)) {
        let message = view.said();
        assert!(message.get("response").is_none(), "{message}");
    }
    view.end();
}

fn a_handler_that_panics_is_an_internal_error_of_its_call_and_nothing_more<S: Side>() {
    let mut view = S::start(|options| {
        options
            .call("boom", |_params, _call| -> Result<Value, ViewError> {
                panic!("the container is gone")
            })
            .call("fine", |_params, _call| Ok(json!({ "ok": true })))
    });
    view.send(&call(1, "boom", &json!(null)));
    let reply = view.said();
    assert_eq!(reply["response"]["error"]["code"], "internal");
    assert_eq!(
        reply["response"]["error"]["message"],
        "the container is gone"
    );
    view.send(&call(2, "fine", &json!(null)));
    assert_eq!(
        view.said(),
        json!({ "response": { "id": 2, "result": { "ok": true } } })
    );
    assert!(!view.screen().contains("panicked"), "{:?}", view.screen());
    view.end();
}

fn a_result_over_the_limit_of_a_message_is_an_error_not_a_hang<S: Side>() {
    let mut view = S::start(|options| {
        options.call("big", |_params, _call| {
            Ok(json!({ "text": "x".repeat(2 << 20) }))
        })
    });
    view.send(&call(1, "big", &json!(null)));
    assert_eq!(view.said()["response"]["error"]["code"], "message-too-long");
    view.end();
}

fn the_limit_of_a_message_is_counted_before_the_wire_escapes_it_in_both_directions<S: Side>() {
    let mut view = S::start(|options| options.call("echo", |params, _call| Ok(params)));
    // 200 000 characters of two bytes are 400 000 as the view says them and 1 200 000 as the wire
    // carries them: under the limit of a message, and over it as the wire has it. The call is
    // read, and the answer, which is as long, is said.
    let text = "\u{e9}".repeat(200_000);
    view.send(&call(1, "echo", &json!({ "text": text })));
    assert_eq!(view.said()["response"]["result"]["text"], text.as_str());
    // One that is over the limit as the view said it is not read at all.
    view.send(&call(2, "echo", &json!({ "text": "x".repeat(1_100_000) })));
    view.send(&call(3, "echo", &json!(null)));
    assert_eq!(view.said()["response"]["id"], 3);
    view.end();
}

fn the_answer_to_a_call_of_a_view_that_is_gone_is_not_the_next_views_answer_of_the_same_id<
    S: Side,
>() {
    // A handler that does not look at `is_cancelled`, held until the test lets it answer.
    let released: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let gate = Arc::clone(&released);
    let mut view = S::start(move |options| {
        options.call("hold", move |params, _call| {
            let who = params["who"].as_str().unwrap_or("").to_owned();
            while !gate.lock().expect("the gate").contains(&who) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(json!({ "from": who }))
        })
    });
    view.send(&call(1, "hold", &json!({ "who": "old" })));
    view.reopened();
    // The new view numbers its calls from 1 again, and the old view's call is still being
    // answered.
    view.send(&call(1, "hold", &json!({ "who": "new" })));
    released.lock().expect("the gate").push("old".into());
    std::thread::sleep(Duration::from_millis(100));
    released.lock().expect("the gate").push("new".into());
    assert_eq!(
        view.said(),
        json!({ "response": { "id": 1, "result": { "from": "new" } } })
    );
    view.end();
}

// ---- subscriptions ----------------------------------------------------------------------

fn a_subscription_pushes_until_it_is_cancelled_and_its_cleanup_runs<S: Side>() {
    let topic_ended = Arc::new(AtomicBool::new(false));
    let ended = Arc::clone(&topic_ended);
    let mut view = S::start(move |options| {
        options.topic("ticks", move |_params, subscription| {
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
    view.send(&json!({ "subscribe": { "id": 1, "topic": "ticks" } }));
    for n in 1..=2 {
        assert_eq!(
            view.said(),
            json!({ "push": { "id": 1, "value": { "n": n } } })
        );
    }
    view.send(&json!({ "cancel": { "id": 1 } }));
    wait_for("the cleanup of the topic", || {
        topic_ended.load(Ordering::SeqCst)
    });
    // One nobody serves is answered at once; the pushes that were on their way when the view
    // cancelled are not said after it.
    view.send(&json!({ "subscribe": { "id": 2, "topic": "nobody" } }));
    loop {
        let message = view.said();
        if message["response"]["id"] == 2 {
            assert_eq!(message["response"]["error"]["code"], "not-found");
            break;
        }
        assert_eq!(message["push"]["id"], 1, "{message}");
    }
    assert!(view.says_nothing_for(Duration::from_millis(100)));
    view.end();
}

fn a_topic_whose_handler_fails_ends_the_subscription_with_its_error<S: Side>() {
    let mut view = S::start(|options| {
        options.topic("broken", |_params, _subscription| {
            Err(ViewError::with_message("unavailable", "no daemon"))
        })
    });
    view.send(&json!({ "subscribe": { "id": 1, "topic": "broken" } }));
    assert_eq!(
        view.said(),
        json!({ "response": { "id": 1, "error": { "code": "unavailable", "message": "no daemon" } } })
    );
    view.end();
}

crate::both_ways!(
    a_call_is_answered_with_its_result_its_error_nothing_or_not_found,
    a_call_that_takes_a_while_reports_its_progress_and_stops_when_it_is_cancelled,
    a_handler_that_panics_is_an_internal_error_of_its_call_and_nothing_more,
    a_result_over_the_limit_of_a_message_is_an_error_not_a_hang,
    the_limit_of_a_message_is_counted_before_the_wire_escapes_it_in_both_directions,
    the_answer_to_a_call_of_a_view_that_is_gone_is_not_the_next_views_answer_of_the_same_id,
    a_subscription_pushes_until_it_is_cancelled_and_its_cleanup_runs,
    a_topic_whose_handler_fails_ends_the_subscription_with_its_error,
);

// ---- what only a conversation has ------------------------------------------------------------

#[test]
fn a_conversation_can_be_handed_to_another_thread() {
    fn sendable<T: Send>() {}
    sendable::<Conversation>();
}

#[test]
fn the_programs_closing_and_reopening_are_said_and_answered_at_once() {
    let control = ViewControl::new();
    let mut conversation = Conversation::new(
        Options::new()
            .control(control.clone())
            .call("ping", |_params, _call| Ok(json!("pong"))),
    );
    let page = pencere::Bundle::single("page.lua", &b"return {}"[..]).expect("a bundle");
    let reopener = {
        let control = control.clone();
        asking(move || control.reopen(Some(vec![View::new(Kind::Lua, &["2.0"], page)])))
    };
    match conversation.receive(WITHIN) {
        Some(Said::Reopened(Some(views))) => assert_eq!(views.len(), 1),
        other => panic!("the view shown again, with its kinds: {other:?}"),
    }
    assert_eq!(
        joined(reopener).expect("a reopening"),
        ReopenResult::Reopened
    );
    // The view shown again is heard.
    assert!(conversation.send(&call(1, "ping", &json!(null)).to_string()));
    assert!(matches!(
        conversation.receive(WITHIN),
        Some(Said::Message(_))
    ));
    let closer = {
        let control = control.clone();
        asking(move || control.close())
    };
    assert!(matches!(conversation.receive(WITHIN), Some(Said::Closed)));
    assert_eq!(joined(closer), CloseResult::Closed);
    // There is no view any more: nothing is heard, and there is nothing to close.
    assert!(!conversation.send(&call(2, "ping", &json!(null)).to_string()));
    assert!(conversation.receive(Duration::from_millis(100)).is_none());
    assert_eq!(control.close(), CloseResult::Unavailable);
}

#[test]
fn a_hand_off_is_said_after_the_answer_of_the_handler_that_asked_and_the_reader_ends_it() {
    let control = ViewControl::new();
    let lender = control.clone();
    let result: Arc<Mutex<Option<HandOffResult>>> = Arc::new(Mutex::new(None));
    let slot = Arc::clone(&result);
    let mut conversation = Conversation::new(
        Options::new()
            .control(control.clone())
            .call("shell", move |_params, _call| {
                let mut command = Command::new("zsh");
                command.arg("-l");
                let handoff = lender.hand_off(command);
                let slot = Arc::clone(&slot);
                std::thread::spawn(move || {
                    *slot.lock().expect("the slot") = Some(handoff.wait());
                });
                Ok(json!("starting"))
            })
            .call("ping", |_params, _call| Ok(json!("pong"))),
    );
    assert!(conversation.send(&call(1, "shell", &json!(null)).to_string()));
    match conversation.receive(WITHIN) {
        Some(Said::Message(text)) => assert_eq!(
            serde_json::from_str::<Value>(&text).expect("JSON"),
            json!({ "response": { "id": 1, "result": "starting" } })
        ),
        other => panic!("the answer first: {other:?}"),
    }
    let Some(Said::HandedOff(lent)) = conversation.receive(WITHIN) else {
        panic!("the hand-off");
    };
    assert_eq!(lent.command().get_program(), "zsh");
    // The terminal is lent: there is no view to hear, and one hand-off at a time.
    assert!(!conversation.send(&call(2, "ping", &json!(null)).to_string()));
    let second = {
        let control = control.clone();
        asking(move || control.hand_off(Command::new("true")).wait())
    };
    assert!(conversation.receive(Duration::from_millis(100)).is_none());
    assert!(matches!(joined(second), HandOffResult::Busy));
    lent.returned(ExitStatus::from_raw(7 << 8));
    // The view shown again is heard, and the program told how the command ended.
    assert!(conversation.send(&call(3, "ping", &json!(null)).to_string()));
    assert!(matches!(
        conversation.receive(WITHIN),
        Some(Said::Message(_))
    ));
    wait_for("the hand-off to be over", || {
        result.lock().expect("the slot").is_some()
    });
    match result.lock().expect("the slot").take() {
        Some(HandOffResult::Returned(Ok(status))) => assert_eq!(status.code(), Some(7)),
        other => panic!("the command's status: {other:?}"),
    }
    // A hand-off whose `Lent` is dropped was not run.
    let third = {
        let control = control.clone();
        asking(move || control.hand_off(Command::new("true")).wait())
    };
    let Some(Said::HandedOff(lent)) = conversation.receive(WITHIN) else {
        panic!("the hand-off");
    };
    drop(lent);
    assert!(conversation.try_receive().is_none());
    match joined(third) {
        HandOffResult::Returned(Err(why)) => {
            assert_eq!(why.kind(), std::io::ErrorKind::Unsupported);
        }
        other => panic!("a command that did not run: {other:?}"),
    }
}

#[test]
fn a_handler_that_says_more_than_is_read_waits_for_the_reader() {
    let pushed = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&pushed);
    let mut conversation =
        Conversation::new(Options::new().topic("flood", move |_params, subscription| {
            let feed = subscription.clone();
            let counted = Arc::clone(&counted);
            std::thread::spawn(move || {
                // 10 KiB a push, as fast as it is taken.
                let text = "x".repeat(10 * 1024);
                while feed.push(json!({ "text": text })) {
                    counted.fetch_add(1, Ordering::SeqCst);
                }
            });
            Ok(())
        }));
    assert!(conversation.send(&json!({ "subscribe": { "id": 1, "topic": "flood" } }).to_string()));
    // Nobody reads: the pushes stop once what waits for the reader is full.
    let settled = || {
        let mut before = pushed.load(Ordering::SeqCst);
        loop {
            std::thread::sleep(Duration::from_millis(100));
            let now = pushed.load(Ordering::SeqCst);
            if now == before {
                return now;
            }
            before = now;
        }
    };
    let unread = settled();
    assert!(
        unread > 0 && unread < 1000,
        "{unread} pushes with nobody reading"
    );
    // One message read lets the handlers' words be taken up to the room of the reader, and then
    // they wait again.
    assert!(matches!(conversation.try_receive(), Some(Said::Message(_))));
    let read_one = settled();
    assert!(read_one > unread, "{read_one} after {unread}");
    assert!(read_one < unread + 1000, "{read_one} after {unread}");
    drop(conversation);
}

#[test]
fn a_reopening_while_the_terminal_is_lent_is_the_view_that_comes_back() {
    let control = ViewControl::new();
    let mut conversation = Conversation::new(Options::new().control(control.clone()));
    let lending = {
        let control = control.clone();
        asking(move || control.hand_off(Command::new("vi")).wait())
    };
    let Some(Said::HandedOff(lent)) = conversation.receive(WITHIN) else {
        panic!("the hand-off");
    };
    // The files of the view change while the command has the terminal.
    let page = pencere::Bundle::single("page.lua", &b"return {}"[..]).expect("a bundle");
    let reopening = {
        let control = control.clone();
        asking(move || control.reopen(Some(vec![View::new(Kind::Lua, &["2.0"], page)])))
    };
    assert!(conversation.receive(Duration::from_millis(100)).is_none());
    lent.returned(ExitStatus::from_raw(0));
    match conversation.receive(WITHIN) {
        Some(Said::Reopened(Some(views))) => assert_eq!(views.len(), 1),
        other => panic!("the view that comes back, with its kinds: {other:?}"),
    }
    assert_eq!(
        joined(reopening).expect("a reopening"),
        ReopenResult::Reopened
    );
    match joined(lending) {
        HandOffResult::Returned(Ok(status)) => assert_eq!(status.code(), Some(0)),
        other => panic!("the command's status: {other:?}"),
    }
}
