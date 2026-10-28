//! What the view asks of the program, and the handlers that answer it (specification §9).
//!
//! A handler runs on a thread of its own, so a call that takes a while neither keeps ⌃C from
//! ending the program nor keeps the other calls waiting. What it has to say — its answer, how far
//! it has got, the values of a subscription — goes to the event loop through a channel, and the
//! loop is the one thread that writes to the terminal. A deferred handler is the other way round:
//! it is given the request on the thread that reads the view, in the order the view sent it, hands
//! it to the program's own loop and returns, and the program answers when it can ([`Answer`]).

use std::cell::{Cell, RefCell};
use std::fmt;
use std::io;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, SyncSender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use pencere_wire::json;
use pencere_wire::messages::{MAX_BYTES, Message};
use serde_json::Value;

use crate::control::{self, Answering};
use crate::terminal::{self, Pipe};

/// An error a handler ends a call or a subscription with: the view gets `code` (a short name it
/// can act on), and `message` if there is one (words for the person).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewError {
    code: String,
    message: Option<String>,
}

impl ViewError {
    /// An error the view can act on by its `code`.
    #[must_use]
    pub fn new(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: None,
        }
    }

    /// An error with words for the person as well.
    #[must_use]
    pub fn with_message(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: Some(message.into()),
        }
    }

    /// The short name the view acts on.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// The words for the person, if there are some.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

impl fmt::Display for ViewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.message {
            Some(message) => write!(f, "{}: {message}", self.code),
            None => f.write_str(&self.code),
        }
    }
}

impl std::error::Error for ViewError {}

/// A flag that is raised once, that a thread can wait on.
#[derive(Debug, Default)]
pub(crate) struct Flag {
    set: AtomicBool,
    lock: Mutex<()>,
    raised: Condvar,
}

impl Flag {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn raise(&self) {
        self.set.store(true, Ordering::SeqCst);
        // Taking the lock is what makes a thread that is about to wait see the flag or be woken.
        let _guard = self.lock.lock();
        self.raised.notify_all();
    }

    pub(crate) fn is_raised(&self) -> bool {
        self.set.load(Ordering::SeqCst)
    }

    /// Waits up to `duration` for the flag: whether it was raised.
    pub(crate) fn wait(&self, duration: Duration) -> bool {
        let Ok(guard) = self.lock.lock() else {
            return self.is_raised();
        };
        if self.is_raised() {
            return true;
        }
        let _ = self.raised.wait_timeout(guard, duration);
        self.is_raised()
    }
}

/// A request of a view: the number of the view that made it, and the id the view gave it. A view
/// numbers its requests from 1, and the view shown after it does again, so what a handler has to
/// say about a request names the view as well: an answer that comes after its view is over is not
/// taken for the answer to the next view's request of the same id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ask {
    pub(crate) view: u64,
    pub(crate) id: u64,
}

/// What a handler has for the event loop.
pub(crate) enum Work {
    /// A call is answered.
    Answer {
        ask: Ask,
        result: Result<Value, ViewError>,
    },
    /// How far a call has got: the payload of the `msg`.
    Progress { ask: Ask, payload: String },
    /// The handler of a topic returned.
    Started {
        ask: Ask,
        result: Result<(), ViewError>,
    },
    /// A value of a subscription: the payload of the `msg`.
    Push { ask: Ask, payload: String },
    /// The program asks to close its view, to show another, or to lend the terminal (§10.8).
    Control(crate::control::Request),
    /// The command the terminal was lent to has ended: how, or why it could not be waited for.
    Returned(std::io::Result<std::process::ExitStatus>),
}

/// The way from a handler's thread to the event loop: a channel with room for a few messages, so
/// that a handler that says more than the terminal reads waits (see `Subscription::push`), and
/// the pipe that wakes the loop when the loop waits on one (a conversation waits on the channel).
#[derive(Clone)]
pub(crate) struct Outbox {
    sender: SyncSender<Work>,
    pipe: Option<Arc<Pipe>>,
}

impl Outbox {
    pub(crate) fn new(sender: SyncSender<Work>, pipe: Option<Arc<Pipe>>) -> Self {
        Self { sender, pipe }
    }

    /// Hands `work` to the loop; whether it was taken (the loop is gone when it was not).
    pub(crate) fn send(&self, work: Work) -> bool {
        let sent = self.sender.send(work).is_ok();
        if sent && let Some(pipe) = &self.pipe {
            pipe.wake();
        }
        sent
    }
}

/// The text of a message of the six as the wire carries it.
pub(crate) fn payload_of(message: &Message) -> String {
    json::to_frame(&message.text())
}

/// Whether a payload, as the wire carries it, is a message of the size one may have. The size is
/// that of the text before the escapes of the wire made it longer (§9.3): a view that writes
/// non-ASCII text is not held to less than a program that has to write ASCII.
pub(crate) fn fits(payload: &str) -> bool {
    json::unescaped_len(payload) <= MAX_BYTES
}

/// Reads what the view sent in a `msg`: one of the six, if it is one. The terminal writes it as it
/// came, escaped by the wire, so the size is counted the way [`fits`] counts it and not on the
/// text (which is [`Message::parse`]'s way, and up to six times too strict for non-ASCII text).
pub(crate) fn read_message(payload: &str) -> Option<Message> {
    if !fits(payload) || !json::is_object(payload) || json::too_deep(payload) {
        return None;
    }
    let message: Message = serde_json::from_str(payload).ok()?;
    message.has_usable_id().then_some(message)
}

/// A call of the view that a handler is answering. It can be cloned, so that the handler can
/// hand the call to the thread that answers it and wait there.
#[derive(Clone)]
pub struct Call {
    ask: Ask,
    method: String,
    flag: Arc<Flag>,
    outbox: Outbox,
}

impl fmt::Debug for Call {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Call")
            .field("id", &self.ask.id)
            .field("method", &self.method)
            .field("cancelled", &self.is_cancelled())
            .finish_non_exhaustive()
    }
}

impl Call {
    pub(crate) fn new(ask: Ask, method: String, flag: Arc<Flag>, outbox: Outbox) -> Self {
        Self {
            ask,
            method,
            flag,
            outbox,
        }
    }

    /// The method the view called.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Whether the view gave the call up with a `cancel` (§9.2), or the view is over. A handler
    /// that stops its work then need not answer: the answer would be ignored.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.flag.is_raised()
    }

    /// Waits up to `duration`, or until the call is cancelled: whether it was.
    pub fn sleep(&self, duration: Duration) -> bool {
        self.flag.wait(duration)
    }

    /// Tells the view how far the call has got (§9.2). Ignored once the call is cancelled or
    /// answered, and when a number is not finite.
    pub fn progress(&self, done: f64, total: Option<f64>, note: Option<&str>) {
        if self.is_cancelled() || !done.is_finite() || total.is_some_and(|t| !t.is_finite()) {
            return;
        }
        let message = Message::progress(self.ask.id, done, total, note.unwrap_or(""));
        let payload = payload_of(&message);
        if fits(&payload) {
            self.outbox.send(Work::Progress {
                ask: self.ask,
                payload,
            });
        }
    }
}

type Cleanup = Box<dyn FnOnce() + Send>;

/// The state of a subscription that the loop and the handler's threads share.
pub(crate) struct Feed {
    ask: Ask,
    topic: String,
    flag: Flag,
    outbox: Outbox,
    cleanups: Mutex<Vec<Cleanup>>,
}

impl Feed {
    pub(crate) fn new(ask: Ask, topic: String, outbox: Outbox) -> Self {
        Self {
            ask,
            topic,
            flag: Flag::new(),
            outbox,
            cleanups: Mutex::new(Vec::new()),
        }
    }

    /// The subscription is over: the view cancelled it, or the view is.
    pub(crate) fn end(&self) {
        self.flag.raise();
        let cleanups = self
            .cleanups
            .lock()
            .map(|mut list| std::mem::take(&mut *list))
            .unwrap_or_default();
        for cleanup in cleanups {
            // What stops a feed is not the view's business when it fails.
            let _ = catch_unwind(AssertUnwindSafe(cleanup));
        }
    }
}

/// A subscription of the view that a handler feeds. It can be cloned and sent to the threads
/// that produce the values.
#[derive(Clone)]
pub struct Subscription {
    feed: Arc<Feed>,
}

impl fmt::Debug for Subscription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Subscription")
            .field("id", &self.feed.ask.id)
            .field("topic", &self.feed.topic)
            .field("cancelled", &self.is_cancelled())
            .finish_non_exhaustive()
    }
}

impl Subscription {
    pub(crate) fn new(feed: Arc<Feed>) -> Self {
        Self { feed }
    }

    /// The topic the view subscribed to.
    #[must_use]
    pub fn topic(&self) -> &str {
        &self.feed.topic
    }

    /// Pushes a value to the view; `false` when it was not sent: the subscription is over, the
    /// view is, or the message would be over 1 MiB (§9.3). `Null` is pushed as `null`.
    ///
    /// It waits while the terminal is catching up with what was pushed before, which is what
    /// keeps a thread that produces faster than the terminal reads from filling the memory.
    pub fn push(&self, value: Value) -> bool {
        if self.is_cancelled() {
            return false;
        }
        let payload = payload_of(&Message::push(self.feed.ask.id, value));
        if !fits(&payload) {
            return false;
        }
        self.feed.outbox.send(Work::Push {
            ask: self.feed.ask,
            payload,
        })
    }

    /// Whether the view cancelled the subscription or is over.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.feed.flag.is_raised()
    }

    /// Waits up to `duration`, or until the subscription ends: whether it has. A thread that
    /// pushes once a second waits with this, and stops when it returns `true`.
    pub fn sleep(&self, duration: Duration) -> bool {
        self.feed.flag.wait(duration)
    }

    /// Runs `cleanup` once when the subscription ends — the view cancels it or is over — to stop
    /// whatever feeds it. At once if it has ended already. It runs on the event loop's thread,
    /// so it should return promptly (raise a flag, close a channel) and not wait for anything.
    pub fn on_end(&self, cleanup: impl FnOnce() + Send + 'static) {
        let cleanup: Cleanup = Box::new(cleanup);
        // `end` raises the flag before it takes the list, so a cleanup that finds the flag down
        // while it holds the lock is in the list `end` takes.
        let cleanup = match self.feed.cleanups.lock() {
            Ok(mut list) if !self.feed.flag.is_raised() => {
                list.push(cleanup);
                return;
            }
            _ => cleanup,
        };
        let _ = catch_unwind(AssertUnwindSafe(cleanup));
    }
}

/// The text of a panic a handler died of.
pub(crate) fn panic_text(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = panic.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = panic.downcast_ref::<String>() {
        text.clone()
    } else {
        "the handler panicked".to_owned()
    }
}

/// The handler of a call, as `serve` holds it.
pub(crate) type CallFn = dyn Fn(Value, &Call) -> Result<Value, ViewError> + Send + Sync;

/// The handler of a call that is answered later ([`crate::Options::call_deferred`]).
pub(crate) type DeferredCallFn = dyn Fn(Value, Call, Answer) + Send + Sync;

/// The handler of a topic whose start is said later ([`crate::Options::topic_deferred`]).
pub(crate) type DeferredTopicFn = dyn Fn(Value, Subscription, Answer<()>) + Send + Sync;

/// How a method's calls are answered: on a thread of their own (or of their lane's), or by a
/// handler given them in order that answers later.
pub(crate) enum CallHandler {
    Thread(Arc<CallFn>),
    Deferred(Arc<DeferredCallFn>),
}

/// A method the view may call: its handler, and the lane its calls run in, if they run in one.
pub(crate) struct Method {
    pub(crate) handler: CallHandler,
    pub(crate) lane: Option<Arc<str>>,
}

/// How a topic is fed: by a handler on a thread of its own, or by one given the subscription in
/// order that says later whether it started.
pub(crate) enum TopicHandler {
    Thread(Arc<TopicFn>),
    Deferred(Arc<DeferredTopicFn>),
}

/// The way to answer a request of the view later, from any thread: what a deferred handler is
/// given with the request ([`crate::Options::call_deferred`], [`crate::Options::topic_deferred`]).
/// A call's answer is its result (`Answer`, of a `Value`); a subscription's says whether it
/// started (`Answer<()>`). It answers once; dropped without an answer, the request is answered
/// with an `internal` error. An answer to a request the view gave up, or of a view that is over,
/// is not sent.
pub struct Answer<T = Value> {
    ask: Ask,
    outbox: Outbox,
    /// The start of a subscription, not the answer to a call.
    start: bool,
    sent: bool,
    of: PhantomData<fn(T)>,
}

impl<T> fmt::Debug for Answer<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Answer")
            .field("id", &self.ask.id)
            .field("start", &self.start)
            .field("sent", &self.sent)
            .finish_non_exhaustive()
    }
}

thread_local! {
    /// The request whose deferred handler runs on this thread now, and its answer where the
    /// handler gave one before it returned: the loop that would take the answer from the channel
    /// is this thread, which is not to wait for itself.
    static INLINE: RefCell<Option<(Ask, Option<Work>)>> = const { RefCell::new(None) };
}

impl<T> Answer<T> {
    fn new(ask: Ask, outbox: Outbox, start: bool) -> Self {
        Self {
            ask,
            outbox,
            start,
            sent: false,
            of: PhantomData,
        }
    }

    fn give(&mut self, result: Result<Value, ViewError>) {
        if std::mem::replace(&mut self.sent, true) {
            return;
        }
        let work = if self.start {
            Work::Started {
                ask: self.ask,
                result: result.map(|_| ()),
            }
        } else {
            Work::Answer {
                ask: self.ask,
                result,
            }
        };
        let ask = self.ask;
        let away = INLINE.with(|slot| match slot.borrow_mut().as_mut() {
            Some((running, held @ None)) if *running == ask => {
                *held = Some(work);
                None
            }
            _ => Some(work),
        });
        if let Some(work) = away {
            self.outbox.send(work);
        }
    }
}

impl Answer<Value> {
    /// A call's answer.
    pub(crate) fn of_call(ask: Ask, outbox: Outbox) -> Self {
        Self::new(ask, outbox, false)
    }

    /// Answers the call with `result`: what it returns made JSON (a result that serialises to
    /// `null` is `{}`), or the error the view gets.
    pub fn send<R: serde::Serialize>(mut self, result: Result<R, ViewError>) {
        let result = result.and_then(|value| {
            serde_json::to_value(value).map_err(|why| {
                ViewError::with_message("internal", format!("the result is not JSON: {why}"))
            })
        });
        self.give(result);
    }
}

impl Answer<()> {
    /// A subscription's start.
    pub(crate) fn of_start(ask: Ask, outbox: Outbox) -> Self {
        Self::new(ask, outbox, true)
    }

    /// Says whether the subscription started: an error ends it with an error response.
    pub fn send(mut self, result: Result<(), ViewError>) {
        self.give(result.map(|()| Value::Null));
    }
}

impl<T> Drop for Answer<T> {
    fn drop(&mut self) {
        if !self.sent {
            self.give(Err(ViewError::with_message(
                "internal",
                "the program did not answer",
            )));
        }
    }
}

/// Runs a deferred handler for `ask` on this thread: what it answered before it returned, or the
/// words of the panic it died of.
pub(crate) fn run_deferred(ask: Ask, body: impl FnOnce()) -> Result<Option<Work>, String> {
    let outer = INLINE.with(|slot| slot.borrow_mut().replace((ask, None)));
    // While it runs this thread runs a handler: a panic in it is the error of its request, and
    // not the program's, which would give the terminal back.
    let was = HANDLER_THREAD.with(|on| on.replace(true));
    let ran = catch_unwind(AssertUnwindSafe(body));
    HANDLER_THREAD.with(|on| on.set(was));
    let held = INLINE.with(|slot| std::mem::replace(&mut *slot.borrow_mut(), outer));
    match ran {
        Ok(()) => Ok(held.and_then(|(_, work)| work)),
        Err(panic) => Err(panic_text(&*panic)),
    }
}

thread_local! {
    /// This thread runs handlers: a panic on it is the error of a call, not the program's.
    static HANDLER_THREAD: Cell<bool> = const { Cell::new(false) };
}

/// Whether this thread runs handlers.
pub(crate) fn on_a_handler_thread() -> bool {
    HANDLER_THREAD.with(Cell::get)
}

/// Starts a thread that runs handlers: one that the program's signals are not delivered to.
pub(crate) fn spawn_handler(body: impl FnOnce() + Send + 'static) -> io::Result<()> {
    terminal::spawn_quiet(move || {
        HANDLER_THREAD.with(|on| on.set(true));
        body();
    })
}

/// A call that waits for its turn in a lane.
struct Turn {
    handler: Arc<CallFn>,
    params: Value,
    call: Call,
}

/// A lane of calls ([`crate::Options::call_in`]): a thread that runs them one at a time, in the
/// order they are queued. It lasts as long as what serves the view, not as long as one view, so a
/// call of the view shown next waits for a handler of the view before that is still running. It
/// ends when it is dropped, once the call it runs has been answered.
pub(crate) struct Lane {
    turns: Sender<Turn>,
}

impl Lane {
    pub(crate) fn start() -> io::Result<Self> {
        let (turns, waiting) = channel::<Turn>();
        spawn_handler(move || {
            for turn in waiting {
                // Cancelled while it waited (or its view is over): it does not run, and is not
                // answered (§9.2).
                if turn.call.is_cancelled() {
                    continue;
                }
                run_call(&*turn.handler, turn.params, &turn.call);
            }
        })?;
        Ok(Self { turns })
    }

    /// Queues a call; whether the lane took it.
    pub(crate) fn queue(&self, handler: Arc<CallFn>, params: Value, call: Call) -> bool {
        self.turns
            .send(Turn {
                handler,
                params,
                call,
            })
            .is_ok()
    }
}

/// The handler of a topic, as `serve` holds it.
pub(crate) type TopicFn = dyn Fn(Value, &Subscription) -> Result<(), ViewError> + Send + Sync;

/// Runs the handler of a call on this thread and hands the answer to the loop.
pub(crate) fn run_call(handler: &CallFn, params: Value, call: &Call) {
    let answering = Answering::start();
    let result = catch_unwind(AssertUnwindSafe(|| handler(params, call)))
        .unwrap_or_else(|panic| Err(ViewError::with_message("internal", panic_text(&*panic))));
    drop(answering);
    call.outbox.send(Work::Answer {
        ask: call.ask,
        result,
    });
    control::answered();
}

/// Runs the handler of a topic on this thread and hands its result to the loop.
pub(crate) fn run_topic(handler: &TopicFn, params: Value, subscription: &Subscription) {
    let answering = Answering::start();
    let result = catch_unwind(AssertUnwindSafe(|| handler(params, subscription)))
        .unwrap_or_else(|panic| Err(ViewError::with_message("internal", panic_text(&*panic))));
    drop(answering);
    subscription.feed.outbox.send(Work::Started {
        ask: subscription.feed.ask,
        result,
    });
    control::answered();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_size_of_a_message_is_counted_before_the_wire_escapes_it() {
        // 400 000 characters of two bytes: 800 000 bytes as the view said them, 2 400 000 as the
        // wire carries them.
        let text = "\u{e9}".repeat(400_000);
        let payload = payload_of(&Message::call(1, "echo", json!({ "text": text })));
        assert!(payload.len() > MAX_BYTES, "the wire's form is longer");
        assert!(fits(&payload));
        assert!(matches!(read_message(&payload), Some(Message::Call(_))));
        // The same text, longer than one message may be as the view said it.
        let too_long = "\u{e9}".repeat(600_000);
        let payload = payload_of(&Message::call(1, "echo", json!({ "text": too_long })));
        assert!(!fits(&payload));
        assert!(read_message(&payload).is_none());
    }

    #[test]
    fn what_the_view_sent_is_a_message_only_if_it_is_an_object_of_the_six_with_an_id() {
        assert!(read_message(r#"{"cancel":{"id":3}}"#).is_some());
        assert!(read_message(r#"{"cancel":{"id":0}}"#).is_none());
        assert!(read_message(r#"{"other":{"id":3}}"#).is_none());
        assert!(read_message(r#"[{"cancel":{"id":3}}]"#).is_none());
        assert!(read_message("not json").is_none());
    }
}
