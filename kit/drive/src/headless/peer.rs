//! The program's side of a page under the headless host: what hears what the page says to its
//! program and answers it.

use std::collections::HashMap;
use std::time::Duration;

use pencere_wire::messages::{Error as Failure, Message};
use serde_json::Value;

/// The program a page under the headless host speaks to: it hears what the page says (`call`,
/// `subscribe`, `cancel`) and says what the program says back (`response`, `push`, `progress`),
/// each one JSON object as written.
///
/// A peer in this process ([`Calls`]) answers at once, while it hears. A program in another process
/// answers when it has: [`Peer::busy`] says it still owes the page an answer, and the host gives it
/// [`Peer::wait`] before it takes what it said. **A peer that stands for a host and not for the
/// program** (one that carries the page's words to a real program) reads no more of them than a host
/// may — the name of the message's one member — and counts what it is owed by those names.
pub trait Peer {
    /// The page said `message` to its program.
    fn heard(&mut self, message: &str);
    /// What the program has said to the page since it was last asked, in order.
    fn said(&mut self) -> Vec<String>;
    /// Whether the program owes the page an answer it is still working on: the host waits for it
    /// ([`Peer::wait`]) before it calls the page settled. A peer that answers at once owes nothing.
    fn busy(&mut self) -> bool {
        false
    }
    /// Waits, at most `within`, for the program to say something it owes.
    fn wait(&mut self, _within: Duration) {}
    /// Why the page's view is over, once it is: the program closed it, or ended. A program in this
    /// process never closes it.
    fn closed(&mut self) -> Option<String> {
        None
    }
}

/// A call the page made, as its program heard it.
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
    /// The page's number for it.
    pub id: u64,
    pub method: String,
    /// What the method was given; `null` for nothing.
    pub params: Value,
    /// It was answered (a result or an error).
    pub answered: bool,
    /// The page gave it up before it was answered.
    pub cancelled: bool,
}

/// A topic the page asked to hear.
#[derive(Clone, Debug, PartialEq)]
pub struct Subscription {
    /// The page's number for it: what a push of it carries.
    pub id: u64,
    pub topic: String,
    pub params: Value,
    /// The page has not cancelled it.
    pub live: bool,
}

type Handler = Box<dyn FnMut(&Value) -> Result<Value, Failure>>;

/// A program made of a table of calls, in this process: each method the page may call has a
/// handler that answers it at once, and the topics it subscribes to are pushed to by the test
/// ([`Calls::push`]). **A call no handler is written for is not refused**: it waits, as a slow
/// program's would, until the test answers it ([`Calls::answer`]) — so that what the page shows
/// while it waits can be looked at. What the page asked is kept ([`Calls::calls`],
/// [`Calls::methods`], [`Calls::subscriptions`]).
#[derive(Default)]
pub struct Calls {
    handlers: HashMap<String, Handler>,
    calls: Vec<Heard>,
    subscriptions: Vec<Subscription>,
    /// What the page said that was not one of the messages a page says.
    noise: Vec<String>,
    outbox: Vec<String>,
}

impl Calls {
    /// A program that answers nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The program answers a call of `method` with what `handler` returns for its params: the
    /// result, or why it failed ([`Failure`]: a short code and words for the person).
    #[must_use]
    pub fn on(
        mut self,
        method: &str,
        handler: impl FnMut(&Value) -> Result<Value, Failure> + 'static,
    ) -> Self {
        self.handlers.insert(method.to_owned(), Box::new(handler));
        self
    }

    /// The program pushes `value` to every live subscription of `topic`: how many there were.
    pub fn push(&mut self, topic: &str, value: &Value) -> usize {
        let ids: Vec<u64> = self
            .subscriptions
            .iter()
            .filter(|subscription| subscription.live && subscription.topic == topic)
            .map(|subscription| subscription.id)
            .collect();
        for id in &ids {
            self.outbox.push(Message::push(*id, value.clone()).text());
        }
        ids.len()
    }

    /// The program answers the oldest call of `method` that is still waiting, with a result or why
    /// it failed: whether there was one.
    pub fn answer(&mut self, method: &str, answer: Result<Value, Failure>) -> bool {
        let Some(call) = self
            .calls
            .iter_mut()
            .find(|call| call.method == method && !call.answered && !call.cancelled)
        else {
            return false;
        };
        call.answered = true;
        let id = call.id;
        self.outbox.push(response(id, answer));
        true
    }

    /// The program says `message` (one JSON object, as written) to the page, as it is.
    pub fn say(&mut self, message: &str) {
        self.outbox.push(message.to_owned());
    }

    /// Every call the page made, in order.
    pub fn calls(&self) -> &[Heard] {
        &self.calls
    }

    /// The methods the page called, in order.
    pub fn methods(&self) -> Vec<&str> {
        self.calls.iter().map(|call| call.method.as_str()).collect()
    }

    /// The calls still waiting for an answer.
    pub fn waiting(&self) -> Vec<&Heard> {
        self.calls
            .iter()
            .filter(|call| !call.answered && !call.cancelled)
            .collect()
    }

    /// Every topic the page subscribed to, in order.
    pub fn subscriptions(&self) -> &[Subscription] {
        &self.subscriptions
    }

    /// What the page said that is not a message a page says (not JSON, not `call`, `subscribe` or
    /// `cancel`): a program reads it as noise.
    pub fn noise(&self) -> &[String] {
        &self.noise
    }
}

/// A response to the call `id`.
fn response(id: u64, answer: Result<Value, Failure>) -> String {
    match answer {
        Ok(result) => Message::answer(id, result).text(),
        Err(failure) => Message::failure(id, &failure.code, &failure.message).text(),
    }
}

impl Peer for Calls {
    fn heard(&mut self, message: &str) {
        match Message::parse(message) {
            Ok(Message::Call(call)) => {
                let answered = match self.handlers.get_mut(&call.method) {
                    Some(handler) => {
                        let answer = handler(&call.params);
                        self.outbox.push(response(call.id, answer));
                        true
                    }
                    None => false,
                };
                self.calls.push(Heard {
                    id: call.id,
                    method: call.method,
                    params: call.params,
                    answered,
                    cancelled: false,
                });
            }
            Ok(Message::Subscribe(subscribe)) => self.subscriptions.push(Subscription {
                id: subscribe.id,
                topic: subscribe.topic,
                params: subscribe.params,
                live: true,
            }),
            Ok(Message::Cancel(cancel)) => {
                for call in &mut self.calls {
                    if call.id == cancel.id && !call.answered {
                        call.cancelled = true;
                    }
                }
                for subscription in &mut self.subscriptions {
                    if subscription.id == cancel.id {
                        subscription.live = false;
                    }
                }
            }
            Ok(Message::Response(_) | Message::Push(_) | Message::Progress(_)) | Err(_) => {
                self.noise.push(message.to_owned());
            }
        }
    }

    fn said(&mut self) -> Vec<String> {
        std::mem::take(&mut self.outbox)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_call_with_a_handler_is_answered_at_once_and_one_without_waits_for_the_test() {
        let mut calls = Calls::new().on("list", |params| Ok(json!({ "asked": params })));
        calls.heard(r#"{"call":{"id":1,"method":"list","params":{"all":true}}}"#);
        calls.heard(r#"{"call":{"id":2,"method":"save"}}"#);
        assert_eq!(
            calls.said(),
            [r#"{"response":{"id":1,"result":{"asked":{"all":true}}}}"#]
        );
        assert_eq!(calls.methods(), ["list", "save"]);
        assert_eq!(calls.waiting().len(), 1);
        assert!(calls.answer(
            "save",
            Err(Failure {
                code: "busy".into(),
                message: "not now".into(),
            })
        ));
        assert_eq!(
            calls.said(),
            [r#"{"response":{"id":2,"error":{"code":"busy","message":"not now"}}}"#]
        );
        assert!(!calls.answer("save", Ok(json!({}))), "nothing waits");
    }

    #[test]
    fn a_push_reaches_the_live_subscriptions_of_its_topic_and_a_cancel_ends_one() {
        let mut calls = Calls::new();
        calls.heard(r#"{"subscribe":{"id":3,"topic":"stats"}}"#);
        calls.heard(r#"{"subscribe":{"id":4,"topic":"stats"}}"#);
        calls.heard(r#"{"subscribe":{"id":5,"topic":"other"}}"#);
        assert_eq!(calls.push("stats", &json!(1)), 2);
        assert_eq!(
            calls.said(),
            [
                r#"{"push":{"id":3,"value":1}}"#,
                r#"{"push":{"id":4,"value":1}}"#
            ]
        );
        calls.heard(r#"{"cancel":{"id":3}}"#);
        assert_eq!(calls.push("stats", &json!(2)), 1);
        calls.heard("not a message");
        assert_eq!(calls.noise(), ["not a message"]);
    }
}
