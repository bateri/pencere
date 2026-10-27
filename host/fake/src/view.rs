//! The view, as a script: what a page in the web view of a real terminal does for the
//! host, played from the command line.
//!
//! The page is told the `context` first, as a page that has loaded is. Then it sends what
//! it was given: its subscriptions and its requests to open links, and, once every
//! subscription has been heard from, its calls, one after the other, each when the answer
//! to the one before has come. It logs what the program says back (a `response`, a `push`,
//! a `progress`) and cancels a call or a subscription when it was told to. It draws
//! nothing: what is being tried is the conversation, and the page of a real view is for a
//! person to look at in a terminal that hosts pencere.
//!
//! A subscription is heard from when the program has pushed once to it (or answered it
//! with an error, or the page has cancelled it), and the script is done when that is so of
//! every subscription and its calls are answered (or cancelled), so that what happens next
//! (`--then`) happens after the program has said what the script waits for and not before
//! it can. A topic that pushes nothing therefore holds the script, and the calls behind it.

use std::collections::VecDeque;

use serde_json::{Map, Value, json};

use crate::args::Ask;
use crate::log;

/// How much of a value the record shows, in characters. `PENCERE_FAKE_GIST=N` says another number: a
/// test of a program that wants to read what it answered (the order of a listing) asks for all of it.
const GIST_LIMIT: usize = 100;

fn gist_limit() -> usize {
    std::env::var("PENCERE_FAKE_GIST")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(GIST_LIMIT)
}

/// What the script does after a message.
#[derive(Debug, Default)]
pub struct Reaction {
    /// Messages for the host, as JSON text.
    pub send: Vec<String>,
    /// Links the page asks the host to open. A web page asks with its bridge's `link` message, a
    /// view the terminal draws itself with the `link` of its reply: the host is told either way,
    /// and a view's link is never one of the messages to the program.
    pub links: Vec<String>,
    /// The script is done (or had nothing to do).
    pub done: bool,
}

/// A request that is out: a call that has not been answered, or a subscription.
#[derive(Debug)]
struct Out {
    id: i64,
    /// Reports of progress for a call, pushes for a subscription.
    count: u32,
    /// A subscription: it has been heard from, or cancelled.
    heard: bool,
    /// The page gave it up: what comes for it is ignored.
    cancelled: bool,
}

impl Out {
    fn new(id: i64) -> Self {
        Self {
            id,
            count: 0,
            heard: false,
            cancelled: false,
        }
    }
}

pub struct ScriptedView {
    calls: VecDeque<Ask>,
    subscriptions: Vec<Ask>,
    links: Vec<String>,
    cancel_after: Option<u32>,
    next_id: i64,
    /// The call that is out.
    call: Option<Out>,
    /// The subscriptions that were sent.
    subscribed: Vec<Out>,
    /// The calls that were cancelled before they were answered.
    given_up: Vec<i64>,
    /// The first `context` has come: the script has begun.
    begun: bool,
    done: bool,
}

/// `value` as the record shows it: compact, and cut where it would fill the line.
fn gist(value: &Value) -> String {
    let text = value.to_string();
    let limit = gist_limit();
    if text.chars().count() <= limit {
        return text;
    }
    let head: String = text.chars().take(limit).collect();
    format!("{head}... ({} bytes)", text.len())
}

fn number(value: &Value, pointer: &str) -> String {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .map_or_else(|| "?".to_owned(), |n| n.to_string())
}

/// The message of an object with one member, and the member's name: the shape of every message
/// a page and the host exchange.
fn single(message: &Value) -> Option<(&String, &Value)> {
    let members = message.as_object()?;
    if members.len() != 1 {
        return None;
    }
    members.iter().next()
}

/// A request of the page: a `call` or a `subscribe`, with its params if it has some.
fn request(kind: &str, id: i64, key: &str, ask: &Ask) -> String {
    let mut members = Map::new();
    members.insert("id".to_owned(), json!(id));
    members.insert(key.to_owned(), json!(ask.name));
    if let Some(params) = &ask.params {
        members.insert("params".to_owned(), params.clone());
    }
    json!({ kind: members }).to_string()
}

impl ScriptedView {
    pub fn new(
        calls: Vec<Ask>,
        subscriptions: Vec<Ask>,
        links: Vec<String>,
        cancel_after: Option<u32>,
    ) -> Self {
        Self {
            calls: calls.into(),
            subscriptions,
            links,
            cancel_after,
            next_id: 1,
            call: None,
            subscribed: Vec::new(),
            given_up: Vec::new(),
            begun: false,
            done: false,
        }
    }

    /// The page has loaded and the host tells it its first `context`. A page that was not given
    /// one (no host does that) begins all the same.
    pub fn loaded(&mut self, context: Option<&str>) -> Reaction {
        match context {
            Some(text) => self.receive(text),
            None => self.begin(),
        }
    }

    /// The page asks to be closed.
    pub fn close(&mut self) -> Reaction {
        log::line("view", "-> close");
        Reaction {
            send: vec![json!({ "close": {} }).to_string()],
            ..Reaction::default()
        }
    }

    fn take_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// What the host sent the page: its `context`, and the program's `response`, `push` and
    /// `progress`.
    pub fn receive(&mut self, text: &str) -> Reaction {
        let Ok(message) = serde_json::from_str::<Value>(text) else {
            log::line("view", "<- a message that is not JSON");
            return Reaction::default();
        };
        let Some((name, body)) = single(&message) else {
            log::line("view", "<- a message that is not an object of one member");
            return Reaction::default();
        };
        match name.as_str() {
            "context" => self.context(body),
            "response" => self.response(body),
            "push" => self.push(body),
            "progress" => self.progress(body),
            other => {
                log::line("view", format!("<- {other} (not a message for a page)"));
                Reaction::default()
            }
        }
    }

    fn context(&mut self, body: &Value) -> Reaction {
        let mode = body.get("mode").and_then(Value::as_str).unwrap_or("?");
        let access = body.get("access").map_or("none".to_owned(), gist);
        log::line(
            "view",
            format!(
                "<- context mode={mode} size={}x{} reduceMotion={} access={access}",
                number(body, "/size/width"),
                number(body, "/size/height"),
                body.get("reduceMotion")
                    .and_then(Value::as_bool)
                    .map_or("?".to_owned(), |on| on.to_string()),
            ),
        );
        if self.begun {
            return Reaction::default();
        }
        self.begin()
    }

    /// Everything the page sends once it has the context: its subscriptions, its links, and,
    /// if it has no subscription to wait for, the first of its calls.
    fn begin(&mut self) -> Reaction {
        self.begun = true;
        let mut send = Vec::new();
        for ask in std::mem::take(&mut self.subscriptions) {
            let id = self.take_id();
            self.subscribed.push(Out::new(id));
            log::line("view", Self::sent("subscribe", id, &ask));
            send.push(request("subscribe", id, "topic", &ask));
        }
        let links = std::mem::take(&mut self.links);
        for url in &links {
            log::line("view", format!("-> link {url}"));
        }
        let first = self.advance();
        send.extend(first.send);
        Reaction {
            send,
            links,
            done: first.done,
        }
    }

    fn sent(what: &str, id: i64, ask: &Ask) -> String {
        match &ask.params {
            Some(params) => format!("-> {what} id={id} {} {}", ask.name, gist(params)),
            None => format!("-> {what} id={id} {}", ask.name),
        }
    }

    /// The next of the calls it was given, or the end, once nothing is being waited for: no call
    /// is out and every subscription has been heard from.
    fn advance(&mut self) -> Reaction {
        if self.call.is_some() || self.subscribed.iter().any(|sub| !sub.heard) {
            return Reaction::default();
        }
        let Some(ask) = self.calls.pop_front() else {
            // The end, once.
            let first = !self.done;
            self.done = true;
            return Reaction {
                done: first,
                ..Reaction::default()
            };
        };
        let id = self.take_id();
        self.call = Some(Out::new(id));
        log::line("view", Self::sent("call", id, &ask));
        Reaction {
            send: vec![request("call", id, "method", &ask)],
            ..Reaction::default()
        }
    }

    fn cancel(&self, id: i64) -> String {
        log::line("view", format!("-> cancel id={id}"));
        json!({ "cancel": { "id": id } }).to_string()
    }

    fn response(&mut self, body: &Value) -> Reaction {
        let Some(id) = body.get("id").and_then(Value::as_i64) else {
            return Reaction::default();
        };
        if let Some(at) = self.given_up.iter().position(|&known| known == id) {
            self.given_up.remove(at);
            log::line(
                "view",
                format!("<- response id={id} after its cancel: ignored"),
            );
            return Reaction::default();
        }
        let what = match (body.get("result"), body.get("error")) {
            (Some(result), None) => format!("result={}", gist(result)),
            (None, Some(error)) => {
                let code = error.get("code").and_then(Value::as_str).unwrap_or("?");
                match error.get("message") {
                    Some(message) => format!("error code={code} message={}", gist(message)),
                    None => format!("error code={code}"),
                }
            }
            _ => "malformed".to_owned(),
        };
        if self.call.as_ref().is_some_and(|call| call.id == id) {
            log::line("view", format!("<- response id={id} {what}"));
            self.call = None;
            return self.advance();
        }
        if let Some(sub) = self.subscribed.iter_mut().find(|sub| sub.id == id) {
            // The answer to a subscription is an error: the program will not push.
            sub.heard = true;
            log::line("view", format!("<- response id={id} {what}"));
            return self.advance();
        }
        log::line("view", format!("<- response id={id} to nothing it asked"));
        Reaction::default()
    }

    fn push(&mut self, body: &Value) -> Reaction {
        let Some(id) = body.get("id").and_then(Value::as_i64) else {
            return Reaction::default();
        };
        let cancel_after = self.cancel_after;
        let Some(sub) = self.subscribed.iter_mut().find(|sub| sub.id == id) else {
            log::line("view", format!("<- push id={id} for nothing it asked"));
            return Reaction::default();
        };
        if sub.cancelled {
            log::line("view", format!("<- push id={id} after its cancel: ignored"));
            return Reaction::default();
        }
        sub.count += 1;
        let count = sub.count;
        log::line(
            "view",
            format!(
                "<- push id={id} value={}",
                gist(body.get("value").unwrap_or(&Value::Null))
            ),
        );
        // Heard once, or cancelled after as many pushes as it was told to wait for.
        let wanted = cancel_after.unwrap_or(1);
        if count < wanted || sub.heard {
            return Reaction::default();
        }
        sub.heard = true;
        let mut send = Vec::new();
        if cancel_after.is_some() {
            sub.cancelled = true;
            send.push(self.cancel(id));
        }
        let rest = self.advance();
        send.extend(rest.send);
        Reaction {
            send,
            done: rest.done,
            ..Reaction::default()
        }
    }

    fn progress(&mut self, body: &Value) -> Reaction {
        let Some(id) = body.get("id").and_then(Value::as_i64) else {
            return Reaction::default();
        };
        if self.given_up.contains(&id) {
            log::line(
                "view",
                format!("<- progress id={id} after its cancel: ignored"),
            );
            return Reaction::default();
        }
        let Some(call) = self.call.as_mut().filter(|call| call.id == id) else {
            log::line("view", format!("<- progress id={id} for nothing it asked"));
            return Reaction::default();
        };
        call.count += 1;
        let count = call.count;
        let total = body
            .get("total")
            .and_then(Value::as_u64)
            .map_or("?".to_owned(), |n| n.to_string());
        let note = body
            .get("note")
            .map_or_else(String::new, |note| format!(" note={}", gist(note)));
        log::line(
            "view",
            format!(
                "<- progress id={id} done={} total={total}{note}",
                number(body, "/done")
            ),
        );
        if self.cancel_after != Some(count) {
            return Reaction::default();
        }
        // The page gives the call up, and goes on to the next one as it would if it had been
        // answered.
        self.call = None;
        self.given_up.push(id);
        let mut send = vec![self.cancel(id)];
        let next = self.advance();
        send.extend(next.send);
        Reaction {
            send,
            done: next.done,
            ..Reaction::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTEXT: &str =
        r#"{"context":{"mode":"dark","size":{"width":640,"height":384},"reduceMotion":false}}"#;

    fn ask(name: &str) -> Ask {
        Ask {
            name: name.to_owned(),
            params: None,
        }
    }

    /// What the page sent, as JSON.
    fn sent(reaction: &Reaction) -> Vec<Value> {
        reaction
            .send
            .iter()
            .map(|text| serde_json::from_str(text).unwrap_or(Value::Null))
            .collect()
    }

    #[test]
    fn the_calls_go_one_after_the_other_and_the_script_is_done_with_the_last_answer() {
        let mut view = ScriptedView::new(vec![ask("a"), ask("b")], Vec::new(), Vec::new(), None);
        let first = view.loaded(Some(CONTEXT));
        assert_eq!(sent(&first), [json!({"call": {"id": 1, "method": "a"}})]);
        assert!(!first.done);
        let second = view.receive(r#"{"response":{"id":1,"result":{}}}"#);
        assert_eq!(sent(&second), [json!({"call": {"id": 2, "method": "b"}})]);
        let last = view.receive(r#"{"response":{"id":2,"error":{"code":"no"}}}"#);
        assert!(last.send.is_empty() && last.done);
        // The end is told once.
        assert!(!view.receive(r#"{"response":{"id":2,"result":{}}}"#).done);
    }

    #[test]
    fn a_script_with_nothing_to_send_is_done_with_the_context() {
        let mut view = ScriptedView::new(Vec::new(), Vec::new(), Vec::new(), None);
        let reaction = view.loaded(Some(CONTEXT));
        assert!(reaction.send.is_empty() && reaction.done);
    }

    #[test]
    fn the_calls_wait_until_every_topic_has_pushed_once() {
        let mut view = ScriptedView::new(
            vec![ask("call")],
            vec![ask("one"), ask("two")],
            vec!["https://example.com/".to_owned()],
            None,
        );
        let begun = view.loaded(Some(CONTEXT));
        assert_eq!(
            sent(&begun),
            [
                json!({"subscribe": {"id": 1, "topic": "one"}}),
                json!({"subscribe": {"id": 2, "topic": "two"}}),
            ]
        );
        assert_eq!(begun.links, ["https://example.com/"]);
        assert!(
            view.receive(r#"{"push":{"id":2,"value":1}}"#)
                .send
                .is_empty()
        );
        let heard = view.receive(r#"{"push":{"id":1,"value":1}}"#);
        assert_eq!(sent(&heard), [json!({"call": {"id": 3, "method": "call"}})]);
        // A second push to a topic that was heard is logged and changes nothing.
        let more = view.receive(r#"{"push":{"id":1,"value":2}}"#);
        assert!(more.send.is_empty() && !more.done);
    }

    #[test]
    fn a_topic_that_is_answered_with_an_error_does_not_hold_the_script() {
        let mut view = ScriptedView::new(vec![ask("call")], vec![ask("one")], Vec::new(), None);
        view.loaded(Some(CONTEXT));
        let reaction = view.receive(r#"{"response":{"id":1,"error":{"code":"not-found"}}}"#);
        assert_eq!(
            sent(&reaction),
            [json!({"call": {"id": 2, "method": "call"}})]
        );
    }

    #[test]
    fn a_topic_is_cancelled_at_the_push_it_was_told_to_and_what_follows_is_ignored() {
        let mut view = ScriptedView::new(Vec::new(), vec![ask("one")], Vec::new(), Some(2));
        view.loaded(Some(CONTEXT));
        assert!(!view.receive(r#"{"push":{"id":1,"value":1}}"#).done);
        let second = view.receive(r#"{"push":{"id":1,"value":2}}"#);
        assert_eq!(sent(&second), [json!({"cancel": {"id": 1}})]);
        assert!(second.done);
        assert!(
            view.receive(r#"{"push":{"id":1,"value":3}}"#)
                .send
                .is_empty()
        );
    }

    #[test]
    fn a_call_is_cancelled_at_the_report_it_was_told_to_and_the_next_one_goes() {
        let mut view = ScriptedView::new(
            vec![ask("slow"), ask("next")],
            Vec::new(),
            Vec::new(),
            Some(1),
        );
        view.loaded(Some(CONTEXT));
        let reaction = view.receive(r#"{"progress":{"id":1,"done":1,"total":5}}"#);
        assert_eq!(
            sent(&reaction),
            [
                json!({"cancel": {"id": 1}}),
                json!({"call": {"id": 2, "method": "next"}})
            ]
        );
        // The answer that was already on its way is the page's no more.
        assert!(
            view.receive(r#"{"response":{"id":1,"result":{}}}"#)
                .send
                .is_empty()
        );
        assert!(view.receive(r#"{"response":{"id":2,"result":{}}}"#).done);
    }

    #[test]
    fn what_is_not_a_message_for_a_page_changes_nothing() {
        let mut view = ScriptedView::new(vec![ask("a")], Vec::new(), Vec::new(), None);
        view.loaded(Some(CONTEXT));
        for text in [
            "not json",
            "[1]",
            r#"{"response":{"id":1,"result":{}},"push":{"id":1,"value":1}}"#,
            r#"{"close":{}}"#,
            r#"{"response":{"id":9,"result":{}}}"#,
        ] {
            let reaction = view.receive(text);
            assert!(reaction.send.is_empty() && !reaction.done, "{text}");
        }
    }
}
