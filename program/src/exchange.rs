//! What a view and the program's handlers say to each other (specification §9): the view's
//! calls, subscriptions and cancels, taken to the handlers, and their answers, progress and pushes,
//! taken back. The same for a view on a terminal (`serve`) and for one in the same process
//! ([`crate::Conversation`]); what carries the messages, and when the handlers' words are taken
//! (the pace of the reader), is the owner's.
//!
//! A request of the view runs on a thread of its own (or of its lane's), and what its handler says
//! comes back through the owner's channel as `Work`, which the owner hands to [`Exchange::on_work`]
//! in the order it came. Nothing here knows of a terminal.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::io;
use std::sync::Arc;

use pencere_wire::messages::Message;
use serde_json::Value;

use crate::Options;
use crate::handlers::{
    self, Ask, Call, CallFn, Feed, Flag, Lane, Outbox, Subscription, ViewError, Work, fits,
    payload_of, read_message, run_call, run_topic,
};

/// How many messages a handler can have waiting for the owner before it has to wait.
pub(crate) const WORK_ROOM: usize = 256;

/// How many bytes of messages toward the view may wait before the owner takes no more from the
/// handlers: what keeps a handler that pushes faster than the view is read from filling the memory.
pub(crate) const WRITE_ROOM: usize = 1 << 20;

/// How many calls of the view run at once. A view that asks for more is told `busy`.
const MAX_CALLS: usize = 64;

/// How many calls of the view one lane holds, the one that runs among them. A view that asks for
/// more is told `busy`.
const MAX_IN_A_LANE: usize = 256;

/// Where the messages of the six go toward the view: a terminal's stream, or a conversation's queue.
pub(crate) trait Toward {
    /// Sends `payload`, the text of a message of the six within the limit, toward the view;
    /// whether it went (the view may be gone).
    fn tell(&mut self, payload: &str) -> bool;
}

/// A call of the view that has not been answered.
struct Outstanding {
    /// Raised when the view cancels it or is over.
    flag: Arc<Flag>,
    /// The lane it runs in, if its method has one.
    lane: Option<Arc<str>>,
}

/// The requests of the view that are out, and the lanes the calls run in.
pub(crate) struct Exchange {
    outbox: Outbox,
    /// The number of the view whose requests are served: what an earlier view asked is over.
    view: u64,
    calls: HashMap<u64, Outstanding>,
    subscriptions: HashMap<u64, Arc<Feed>>,
    /// The lanes that have had calls, by name: they last as long as the exchange.
    lanes: HashMap<Arc<str>, Lane>,
}

impl Exchange {
    /// An exchange whose handlers hand what they say to `outbox`.
    pub(crate) fn new(outbox: Outbox) -> Self {
        Self {
            outbox,
            view: 1,
            calls: HashMap::new(),
            subscriptions: HashMap::new(),
            lanes: HashMap::new(),
        }
    }

    /// What the view sent: one of its three messages, if it is one; whether it was. The view is
    /// not trusted: what is not a `call`, a `subscribe` or a `cancel` within the limits is left.
    pub(crate) fn on_message(
        &mut self,
        options: &Options,
        payload: &str,
        toward: &mut dyn Toward,
    ) -> bool {
        let Some(request) = read_message(payload) else {
            return false;
        };
        match request {
            Message::Call(call) => self.on_call(options, call.id, call.method, call.params, toward),
            Message::Subscribe(subscribe) => self.on_subscribe(
                options,
                subscribe.id,
                subscribe.topic,
                subscribe.params,
                toward,
            ),
            Message::Cancel(cancel) => self.on_cancel(cancel.id),
            Message::Response(_) | Message::Push(_) | Message::Progress(_) => return false,
        }
        true
    }

    /// What the view had asked is over with it: calls are cancelled, subscriptions stop, and what
    /// their handlers still have to say is about a view that is gone. What the view shown next
    /// asks is served as the first view's was; the lanes go on.
    pub(crate) fn end_view(&mut self) {
        self.view += 1;
        for (_, call) in self.calls.drain() {
            call.flag.raise();
        }
        for (_, feed) in self.subscriptions.drain() {
            feed.end();
        }
    }

    fn tell(toward: &mut dyn Toward, payload: &str) -> bool {
        fits(payload) && toward.tell(payload)
    }

    fn tell_failure(toward: &mut dyn Toward, id: u64, error: &ViewError) {
        let message = Message::failure(id, error.code(), error.message().unwrap_or(""));
        Self::tell(toward, &payload_of(&message));
    }

    fn on_call(
        &mut self,
        options: &Options,
        id: u64,
        method: String,
        params: Value,
        toward: &mut dyn Toward,
    ) {
        // An id that is out is not used again; a view that does is not following §9.2.
        if self.calls.contains_key(&id) || self.subscriptions.contains_key(&id) {
            return;
        }
        let Some(entry) = options.calls.get(&method) else {
            Self::tell_failure(
                toward,
                id,
                &ViewError::with_message("not-found", "there is nothing to call by that name"),
            );
            return;
        };
        let lane = entry.lane.clone();
        let out = self.calls.values().filter(|call| call.lane == lane).count();
        let (room, why) = match lane {
            None => (MAX_CALLS, "too many calls are being answered"),
            Some(_) => (MAX_IN_A_LANE, "too many calls wait in their lane"),
        };
        if out >= room {
            Self::tell_failure(toward, id, &ViewError::with_message("busy", why));
            return;
        }
        let handler = Arc::clone(&entry.handler);
        let flag = Arc::new(Flag::new());
        self.calls.insert(
            id,
            Outstanding {
                flag: Arc::clone(&flag),
                lane: lane.clone(),
            },
        );
        let ask = Ask {
            view: self.view,
            id,
        };
        let call = Call::new(ask, method, flag, self.outbox.clone());
        let started = match lane {
            None => handlers::spawn_handler(move || run_call(&*handler, params, &call)),
            Some(lane) => self.queue(lane, handler, params, call),
        };
        if started.is_err() {
            self.calls.remove(&id);
            Self::tell_failure(
                toward,
                id,
                &ViewError::with_message("internal", "the call could not be started"),
            );
        }
    }

    /// Queues a call in its lane, which is started with its first call.
    fn queue(
        &mut self,
        lane: Arc<str>,
        handler: Arc<CallFn>,
        params: Value,
        call: Call,
    ) -> io::Result<()> {
        let started = match self.lanes.entry(lane) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(Lane::start()?),
        };
        if started.queue(handler, params, call) {
            Ok(())
        } else {
            Err(io::Error::other("the lane is gone"))
        }
    }

    fn on_subscribe(
        &mut self,
        options: &Options,
        id: u64,
        topic: String,
        params: Value,
        toward: &mut dyn Toward,
    ) {
        if self.calls.contains_key(&id) || self.subscriptions.contains_key(&id) {
            return;
        }
        let Some(handler) = options.topics.get(&topic) else {
            Self::tell_failure(
                toward,
                id,
                &ViewError::with_message(
                    "not-found",
                    "there is nothing to subscribe to by that name",
                ),
            );
            return;
        };
        let handler = Arc::clone(handler);
        let ask = Ask {
            view: self.view,
            id,
        };
        let feed = Arc::new(Feed::new(ask, topic, self.outbox.clone()));
        self.subscriptions.insert(id, Arc::clone(&feed));
        let subscription = Subscription::new(feed);
        let started = handlers::spawn_handler(move || run_topic(&*handler, params, &subscription));
        if started.is_err()
            && let Some(feed) = self.subscriptions.remove(&id)
        {
            feed.end();
            Self::tell_failure(
                toward,
                id,
                &ViewError::with_message("internal", "the subscription could not be started"),
            );
        }
    }

    /// The view gives up a call that is out, or ends a subscription; an id that is not out
    /// changes nothing (§9.2).
    fn on_cancel(&mut self, id: u64) {
        if let Some(call) = self.calls.remove(&id) {
            call.flag.raise();
        } else if let Some(feed) = self.subscriptions.remove(&id) {
            feed.end();
        }
    }

    /// What a handler handed over: an answer, how far a call has got, a push, or how the start of
    /// a subscription went. The program's requests of its view (`Work::Control`) and the end of a
    /// command the terminal was lent to are the owner's, and are not handed here.
    pub(crate) fn on_work(&mut self, work: Work, toward: &mut dyn Toward) {
        match work {
            // About a view that is gone: its requests are over with it.
            Work::Answer { ask, .. }
            | Work::Progress { ask, .. }
            | Work::Started { ask, .. }
            | Work::Push { ask, .. }
                if ask.view != self.view => {}
            Work::Answer {
                ask: Ask { id, .. },
                result,
            } => {
                // Cancelled, or the view is over: the answer would be ignored.
                let Some(call) = self.calls.remove(&id) else {
                    return;
                };
                if call.flag.is_raised() {
                    return;
                }
                match result {
                    Ok(value) => Self::answer(toward, id, value),
                    Err(error) => Self::tell_failure(toward, id, &error),
                }
            }
            Work::Progress {
                ask: Ask { id, .. },
                payload,
            } => {
                if self
                    .calls
                    .get(&id)
                    .is_some_and(|call| !call.flag.is_raised())
                {
                    Self::tell(toward, &payload);
                }
            }
            Work::Push {
                ask: Ask { id, .. },
                payload,
            } => {
                if self.subscriptions.contains_key(&id) {
                    Self::tell(toward, &payload);
                }
            }
            Work::Started {
                ask: Ask { id, .. },
                result,
            } => {
                if let Err(error) = result
                    && let Some(feed) = self.subscriptions.remove(&id)
                {
                    feed.end();
                    Self::tell_failure(toward, id, &error);
                }
            }
            Work::Control(_) | Work::Returned(_) => {}
        }
    }

    /// The response that ends a call with `result`: never `null`, nothing to say is `{}`.
    fn answer(toward: &mut dyn Toward, id: u64, result: Value) {
        let result = if result.is_null() {
            Value::Object(serde_json::Map::new())
        } else {
            result
        };
        let payload = payload_of(&Message::answer(id, result));
        if !fits(&payload) {
            Self::tell_failure(
                toward,
                id,
                &ViewError::with_message(
                    "message-too-long",
                    "the result is over the limit of a message",
                ),
            );
            return;
        }
        Self::tell(toward, &payload);
    }
}

impl Drop for Exchange {
    /// What is out is over: its handlers are told, and the subscriptions' cleanups run.
    fn drop(&mut self) {
        self.end_view();
    }
}
