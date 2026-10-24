//! The program's conversation with its terminal, from the first `hello` to the last byte it
//! writes: an event loop on the thread that called `serve`.
//!
//! The loop waits for the terminal's input, for the room to write, for the pipe that signals and
//! handlers wake it with, and for the next deadline of the phase it is in, and for nothing else:
//! a program with a view and nothing to say sleeps in `poll` until something happens.
//!
//! Where the conversation is (the `Phase`) says what a message means. The same `welcome` is the
//! answer to the `hello` while the program asks, and the request to start again (§10.7) while it
//! shows a view; the same `closed` is the end of a view, and the answer to the `bye` while the
//! program leaves.

use std::cell::Cell;
use std::collections::HashMap;
use std::io;
use std::panic;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, sync_channel};
use std::time::Instant;

use pencere_wire::base64;
use pencere_wire::bundle::Digest;
use pencere_wire::frame::Message as Frame;
use pencere_wire::messages::Message;
use serde_json::Value;

use crate::bundle::{Bundle, BundleError};
use crate::handlers::{
    Call, Feed, Flag, Outbox, Subscription, ViewError, Work, fits, payload_of, read_message,
    run_call, run_topic,
};
use crate::link::{Item, Reader, Writer};
use crate::payload::{self, ByeReason, ClosedReason, FEATURE_BUNDLE, Offer, OpenFields, Welcome};
use crate::terminal::{
    self, DEVICE_ATTRIBUTES, ENTER_ALTERNATE, FAREWELL, Pipe, Read, TakeError, Taken, Terminal,
};
use crate::view::Source;
use crate::{Closed, EndedBy, Error, Kind, NoView, NoViewReason, Options, Outcome, Shown};

/// How many calls of the view run at once. A view that asks for more is told `busy`.
const MAX_CALLS: usize = 64;

/// How many messages a handler can have waiting for the loop before it has to wait.
const WORK_ROOM: usize = 256;

/// How many bytes may wait to be written before the loop takes no more from the handlers.
const WRITE_ROOM: usize = 1 << 20;

thread_local! {
    /// This thread is the event loop of a `serve`: a panic on it gives the terminal back before
    /// anything is printed.
    static ON_THE_LOOP: Cell<bool> = const { Cell::new(false) };
    /// This thread runs a handler: its panic is the call's error, not the program's.
    static IN_A_HANDLER: Cell<bool> = const { Cell::new(false) };
}

/// Where the conversation is.
enum Phase {
    /// The `hello` is out; the terminal has until `until` to answer.
    Asking { hello: u32, until: Instant },
    /// A view is being opened or is open.
    Showing,
    /// The program has said `bye` (or has nothing to say it for) and waits for the answer to it,
    /// reading and throwing away whatever comes.
    Leaving {
        bye: Option<u32>,
        until: Instant,
        then: Then,
    },
    /// There is no view and the terminal's answer to `CSI c` has not come: it is read, so that
    /// it does not reach the shell as typed text.
    Draining { until: Instant, no_view: NoView },
    /// Done: `result` says how.
    Over,
}

/// What comes of leaving.
enum Then {
    /// A signal ended the program's use of the view.
    Signal(libc::c_int),
    /// The program cannot go on.
    Failure(Error),
}

/// Gives the terminal back if a panic ends the program in the middle of the conversation, and
/// puts the panic hook back.
struct PanicHook {
    previous: Arc<dyn Fn(&panic::PanicHookInfo<'_>) + Send + Sync>,
}

impl PanicHook {
    fn install() -> Self {
        let previous: Arc<dyn Fn(&panic::PanicHookInfo<'_>) + Send + Sync> =
            Arc::from(panic::take_hook());
        let chain = Arc::clone(&previous);
        panic::set_hook(Box::new(move |info| {
            if IN_A_HANDLER.with(Cell::get) {
                // The call is answered with the panic's words; nothing is printed on the view's
                // screen.
                return;
            }
            if ON_THE_LOOP.with(Cell::get) {
                // The default hook prints, and the screen is the view's until it is given back:
                // what it printed there would be gone with the screen.
                terminal::restore();
            }
            chain(info);
        }));
        ON_THE_LOOP.with(|on| on.set(true));
        Self { previous }
    }
}

impl Drop for PanicHook {
    fn drop(&mut self) {
        ON_THE_LOOP.with(|on| on.set(false));
        // `set_hook` is not allowed to a thread that is panicking; the hook stays, and does
        // nothing for a thread that is not the loop.
        if !std::thread::panicking() {
            let previous = Arc::clone(&self.previous);
            panic::set_hook(Box::new(move |info| previous(info)));
        }
    }
}

struct Session<'a> {
    options: &'a Options,
    terminal: &'a Terminal,
    taken: Taken,
    pipe: Arc<Pipe>,
    reader: Reader,
    writer: Writer,
    work: Receiver<Work>,
    outbox: Outbox,
    phase: Phase,
    /// The terminal's answer to `CSI c` has been read.
    fence_answered: bool,
    /// The alternate screen is on.
    alternate: bool,
    /// The token the open under way, or the view that runs, was made with: the view's identity.
    token: Option<String>,
    shown: Option<Shown>,
    bundle: Option<Arc<Bundle>>,
    open_id: u32,
    /// The terminal answered the open with a `need`: it took the open (§6.4).
    accepted: bool,
    /// The view has asked something of the program.
    heard: bool,
    /// The view is up on the terminal's side: what the view says is served.
    live: bool,
    calls: HashMap<u64, Arc<Flag>>,
    subscriptions: HashMap<u64, Arc<Feed>>,
    bye_sent: bool,
    finished: bool,
    result: Option<Result<Outcome, Error>>,
}

/// Shows the view and returns how it ended; the terminal is given back on every path.
pub(crate) fn run(options: &Options, terminal: &Terminal) -> Result<Outcome, Error> {
    let no_view = |reason| {
        Ok(Outcome::NoView(NoView {
            reason,
            code: None,
            message: None,
        }))
    };
    if !terminal.is_tty() {
        return no_view(NoViewReason::NoTty);
    }
    let pipe = Arc::new(Pipe::new().map_err(Error::Io)?);
    let taken = match Taken::take(terminal, &pipe) {
        Ok(taken) => taken,
        Err(TakeError::Busy) => return Err(Error::Busy),
        // A tty whose modes cannot be set is a tty nothing can be asked on.
        Err(TakeError::Failed) => return no_view(NoViewReason::NoTty),
    };
    let _hook = PanicHook::install();
    let (sender, work) = sync_channel(WORK_ROOM);
    let mut session = Session {
        options,
        terminal,
        taken,
        pipe: Arc::clone(&pipe),
        reader: Reader::new(),
        writer: Writer::new(terminal.output()),
        work,
        outbox: Outbox::new(sender, pipe),
        phase: Phase::Over,
        fence_answered: false,
        alternate: false,
        token: None,
        shown: None,
        bundle: None,
        open_id: 0,
        accepted: false,
        heard: false,
        live: false,
        calls: HashMap::new(),
        subscriptions: HashMap::new(),
        bye_sent: false,
        finished: false,
        result: None,
    };
    let result = session.converse();
    session.finish();
    result
}

impl Session<'_> {
    /// The offers of the hello and the choice among a terminal's: the kinds the program shows,
    /// with their versions, in the program's order.
    fn offers(&self) -> Vec<(Kind, &[String])> {
        self.options
            .views
            .iter()
            .map(|view| (view.kind, view.versions.as_slice()))
            .collect()
    }

    fn converse(&mut self) -> Result<Outcome, Error> {
        let hello = payload::hello(&self.offers());
        let Ok(id) = self.writer.message("hello", None, None, &hello) else {
            return Err(Error::Io(io::Error::other("the hello cannot be written")));
        };
        self.writer.raw(DEVICE_ATTRIBUTES);
        self.phase = Phase::Asking {
            hello: id,
            until: Instant::now() + self.options.answer_timeout,
        };
        self.event_loop();
        self.result
            .take()
            .unwrap_or_else(|| Err(Error::Io(io::Error::other("the conversation has no end"))))
    }

    fn event_loop(&mut self) {
        while !matches!(self.phase, Phase::Over) {
            self.signals();
            self.take_work();
            self.deadlines();
            if matches!(self.phase, Phase::Over) {
                break;
            }
            let timeout = self
                .next_deadline()
                .map(|at| at.saturating_duration_since(Instant::now()));
            let output = self.writer.pending().then(|| self.terminal.output());
            let ready = match terminal::wait(
                self.terminal.input(),
                output,
                self.pipe.read_end(),
                timeout,
            ) {
                Ok(ready) => ready,
                Err(why) => {
                    self.over(Err(Error::Io(why)));
                    break;
                }
            };
            if ready.wake {
                self.pipe.drain();
            }
            if ready.input {
                self.read_input(ready.hangup);
            }
            if ready.output && self.writer.pump().is_err() {
                self.terminal_gone();
            }
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        match &self.phase {
            Phase::Asking { until, .. }
            | Phase::Leaving { until, .. }
            | Phase::Draining { until, .. } => Some(*until),
            Phase::Showing | Phase::Over => None,
        }
    }

    fn deadlines(&mut self) {
        let now = Instant::now();
        match &self.phase {
            Phase::Asking { until, .. } if now >= *until => {
                self.no_view(NoViewReason::NoHost);
            }
            Phase::Leaving { until, .. } if now >= *until => self.left(),
            Phase::Draining { until, .. } if now >= *until => self.drained(),
            _ => {}
        }
    }

    // ---- signals -----------------------------------------------------------------

    fn signals(&mut self) {
        if let Some(signal) = terminal::take_signal() {
            self.interrupted(signal);
        }
        if terminal::take_stop() && !matches!(self.phase, Phase::Over) {
            // Whatever is half written is finished first: the terminal's modes and screen are
            // given back between two frames, never inside one.
            let _ = self.writer.flush_within(FAREWELL);
            self.taken.suspend();
        }
    }

    /// A signal asks the program to end (§10.4).
    fn interrupted(&mut self, signal: libc::c_int) {
        match self.phase {
            // The first signal is being served; a second one is a parent forwarding the ⌃C the
            // tty sent.
            Phase::Leaving { .. } | Phase::Over => {}
            Phase::Asking { .. } | Phase::Draining { .. } => {
                self.over(Ok(Outcome::NoView(NoView {
                    reason: NoViewReason::Interrupted(signal),
                    code: None,
                    message: None,
                })));
            }
            Phase::Showing => self.leave(Then::Signal(signal), ByeReason::Signal),
        }
    }

    /// The program ends its use of the view by its own choice: `bye` when a view is opening or
    /// open, then the input is read, and thrown away, until the answer to it (§10.3).
    fn leave(&mut self, then: Then, reason: ByeReason) {
        self.end_view();
        let Some(token) = self.token.clone() else {
            self.settle(then);
            return;
        };
        // What is queued and not yet written would reach the terminal after the `bye`, and open
        // or feed a view the program has just ended.
        self.writer.mute();
        let bye = payload::bye(&token, reason);
        let id = self.writer.last_words("bye", &bye).ok();
        self.bye_sent = id.is_some();
        self.phase = Phase::Leaving {
            bye: id,
            until: Instant::now() + self.options.bye_timeout,
            then,
        };
    }

    /// The wait for the answer to the `bye` is over.
    fn left(&mut self) {
        let Phase::Leaving { then, .. } = std::mem::replace(&mut self.phase, Phase::Over) else {
            return;
        };
        self.settle(then);
    }

    /// What `serve` returns for a program that has left.
    fn settle(&mut self, then: Then) {
        let result = match then {
            Then::Failure(error) => Err(error),
            Then::Signal(signal) => Ok(match (&self.shown, self.accepted) {
                (Some(view), true) => Outcome::Closed(Closed {
                    by: EndedBy::Signal(signal),
                    view: view.clone(),
                    code: None,
                    message: None,
                }),
                _ => Outcome::NoView(NoView {
                    reason: NoViewReason::Interrupted(signal),
                    code: None,
                    message: None,
                }),
            }),
        };
        self.over(result);
    }

    // ---- ending ------------------------------------------------------------------

    /// What the view had asked is over with it: calls are cancelled, subscriptions stop.
    fn end_view(&mut self) {
        for (_, call) in self.calls.drain() {
            call.raise();
        }
        for (_, feed) in self.subscriptions.drain() {
            feed.end();
        }
    }

    /// The terminal's view is gone, or the program no longer believes in it: nothing of it is
    /// served.
    fn forget_view(&mut self) {
        self.live = false;
        self.token = None;
        self.bundle = None;
        self.accepted = false;
        self.heard = false;
        self.end_view();
    }

    fn over(&mut self, result: Result<Outcome, Error>) {
        self.end_view();
        self.phase = Phase::Over;
        if self.result.is_none() {
            self.result = Some(result);
        }
    }

    /// No view will be shown. The terminal's answer to `CSI c` is read first if it has not come,
    /// so that it does not reach the shell as typed text (§5.3).
    fn no_view(&mut self, reason: NoViewReason) {
        let no_view = NoView {
            reason,
            code: None,
            message: None,
        };
        if self.fence_answered {
            self.over(Ok(Outcome::NoView(no_view)));
        } else {
            self.phase = Phase::Draining {
                until: Instant::now() + self.options.late_reply_grace,
                no_view,
            };
        }
    }

    fn drained(&mut self) {
        let Phase::Draining { no_view, .. } = std::mem::replace(&mut self.phase, Phase::Over)
        else {
            return;
        };
        self.over(Ok(Outcome::NoView(no_view)));
    }

    /// The terminal is gone: nobody reads what the program writes and nobody writes to it.
    fn terminal_gone(&mut self) {
        match std::mem::replace(&mut self.phase, Phase::Over) {
            Phase::Over => {}
            Phase::Asking { .. } | Phase::Draining { .. } => {
                self.over(Ok(Outcome::NoView(NoView {
                    reason: NoViewReason::NoHost,
                    code: None,
                    message: None,
                })));
            }
            Phase::Showing => {
                let result = match &self.shown {
                    Some(view) if self.accepted => Ok(Outcome::Closed(Closed {
                        by: EndedBy::Terminal,
                        view: view.clone(),
                        code: None,
                        message: None,
                    })),
                    _ => Ok(Outcome::NoView(NoView {
                        reason: NoViewReason::NoHost,
                        code: None,
                        message: None,
                    })),
                };
                self.over(result);
            }
            Phase::Leaving { then, .. } => self.settle(then),
        }
    }

    /// Done with the terminal: whatever is half written is finished, the terminal is given back
    /// and what it queued for the program and was not read is thrown away if the program said
    /// `bye` (§10.3).
    fn finish(&mut self) {
        self.finished = true;
        self.end_view();
        self.writer.mute();
        // A frame cut in two by the escape that leaves the screen would leave the rest of it on
        // the screen the person is given back.
        let _ = self.writer.flush_within(FAREWELL);
        self.writer.forget_everything();
        self.taken.give_back(self.bye_sent);
    }

    // ---- reading -----------------------------------------------------------------

    fn read_input(&mut self, hangup: bool) {
        let mut buffer = [0u8; 4096];
        match terminal::read(self.terminal.input(), &mut buffer, hangup) {
            Read::Nothing => {}
            Read::Ended => self.terminal_gone(),
            Read::Bytes(got) => {
                let bytes = buffer.get(..got).unwrap_or_default();
                for item in self.reader.feed(bytes) {
                    match item {
                        Item::Message(message) => self.on_message(&message),
                        Item::Reply => self.on_reply(),
                    }
                }
            }
        }
    }

    /// The terminal answered `CSI c`.
    fn on_reply(&mut self) {
        self.fence_answered = true;
        match self.phase {
            // The reply came before any `welcome`: there is no host, and a `welcome` that comes
            // later is ignored (§5.3).
            Phase::Asking { .. } => self.no_view(NoViewReason::NoHost),
            Phase::Draining { .. } => self.drained(),
            _ => {}
        }
    }

    fn on_message(&mut self, message: &Frame) {
        match &self.phase {
            Phase::Asking { hello, .. } => {
                if message.kind == "welcome" && message.re() == Some(*hello) {
                    self.on_answer(message);
                }
            }
            Phase::Showing => match message.kind.as_str() {
                "welcome" => self.on_welcome(message),
                "need" => self.on_need(message),
                "closed" => self.on_closed(message),
                "msg" => self.on_msg(message),
                // A kind this version does not know, or one only a program sends: ignored (§4.5).
                _ => {}
            },
            Phase::Leaving { bye, .. } => {
                // The answer to the `bye`. A `closed` with no `re` is the terminal's own word
                // that the view is gone (the person closed it as the `bye` went out): the `bye`
                // will not be answered, and nothing more is coming. What else comes is read and
                // thrown away.
                if message.kind == "closed"
                    && (message.re().is_none() || message.re() == *bye)
                    && payload::closed(&message.payload).is_some()
                {
                    self.left();
                }
            }
            Phase::Draining { .. } | Phase::Over => {}
        }
    }

    // ---- asking and opening --------------------------------------------------------

    /// What the terminal answered to the `hello` (§5.3).
    fn on_answer(&mut self, message: &Frame) {
        match payload::welcome(&message.payload) {
            // Not an answer: the wait goes on.
            None => {}
            Some(Welcome::Unsupported) => self.no_view(NoViewReason::NoCommonVersion),
            Some(Welcome::Offer(offer)) => match self.choose(&offer) {
                // A terminal with a token and nothing to open: the program knows at once and
                // does not wait (§5.3).
                None => self.no_view(NoViewReason::NoCommonView),
                Some(choice) => self.open(offer.token, choice),
            },
        }
    }

    /// The view to open, if the terminal will open one of the program's kinds (§5.3).
    fn choose(&self, offer: &Offer) -> Option<(usize, String)> {
        // Every kind of view arrives as a bundle: a terminal without the feature has nothing to
        // open (§8.1).
        if !offer.features.iter().any(|f| f == FEATURE_BUNDLE) {
            return None;
        }
        payload::choose(&self.offers(), &offer.views)
    }

    /// An unsolicited `welcome` while a view is shown: a terminal that came up after a break
    /// asks for the view again (§10.7).
    fn on_welcome(&mut self, message: &Frame) {
        if message.re().is_some() {
            return;
        }
        let Some(Welcome::Offer(offer)) = payload::welcome(&message.payload) else {
            return;
        };
        if !offer.restart {
            return;
        }
        let Some(choice) = self.choose(&offer) else {
            return;
        };
        // What the old view had asked is over with it, and its token is the terminal's to have
        // forgotten: until the new open is out there is no view, and a `bye` with the dead token
        // would be nobody's.
        self.forget_view();
        self.open(offer.token, choice);
    }

    /// Asks for the view with the token of a `welcome` (§6.3): the kind and the version chosen.
    fn open(&mut self, token: String, (at, version): (usize, String)) {
        let Some(spec) = self.options.views.get(at) else {
            return;
        };
        let kind = spec.kind;
        let bundle = match &spec.source {
            Source::Fixed(bundle) => Arc::clone(bundle),
            Source::PerVersion(files) => {
                match files(&version).and_then(|bundle| check_entry(kind, &bundle).map(|()| bundle))
                {
                    Ok(bundle) => Arc::new(bundle),
                    Err(why) => {
                        self.fail(Error::Bundle(why));
                        return;
                    }
                }
            }
        };
        self.phase = Phase::Showing;
        if !self.alternate {
            self.alternate = true;
            terminal::note_alternate(true);
            self.writer.raw(ENTER_ALTERNATE);
        }
        let open = payload::open(
            &OpenFields {
                token: &token,
                kind,
                version: &version,
                program: self.options.program.as_deref(),
                title: self.options.title.as_deref(),
                icons: &self.options.icons,
                access: spec.access.as_ref(),
            },
            &bundle,
        );
        let Ok(id) = self.writer.message("open", None, None, &open) else {
            self.fail(Error::Io(io::Error::other("the open cannot be written")));
            return;
        };
        self.open_id = id;
        self.bundle = Some(bundle);
        self.token = Some(token);
        self.shown = Some(Shown { kind, version });
        self.accepted = false;
        self.heard = false;
        self.live = true;
    }

    /// The program fails: it says so and waits for the answer like any program that ends.
    fn fail(&mut self, error: Error) {
        self.leave(Then::Failure(error), ByeReason::Error);
    }

    /// The terminal took the open and lacks some of the files: they are sent (§8.3).
    fn on_need(&mut self, message: &Frame) {
        let Some(bundle) = self.bundle.clone() else {
            return;
        };
        if self.accepted || message.re() != Some(self.open_id) {
            return;
        }
        let Some(missing) = payload::need(&message.payload) else {
            return;
        };
        self.accepted = true;
        let mut sent: Vec<&str> = Vec::new();
        for hex in &missing {
            if sent.contains(&hex.as_str()) {
                continue;
            }
            sent.push(hex);
            let Some(data) = Digest::parse(hex).and_then(|digest| bundle.read(&digest)) else {
                continue;
            };
            let body = base64::encode(data);
            if self.writer.message("file", None, Some(hex), &body).is_err() {
                self.fail(Error::Io(io::Error::other("a file cannot be written")));
                return;
            }
        }
    }

    // ---- what the view says (§9) ---------------------------------------------------

    fn on_msg(&mut self, message: &Frame) {
        if !self.live {
            return;
        }
        // The view is not trusted: what is not one of its three messages is left.
        let Some(request) = read_message(&message.payload) else {
            return;
        };
        match request {
            Message::Call(call) => {
                self.heard = true;
                self.on_call(call.id, call.method, call.params);
            }
            Message::Subscribe(subscribe) => {
                self.heard = true;
                self.on_subscribe(subscribe.id, subscribe.topic, subscribe.params);
            }
            Message::Cancel(cancel) => {
                self.heard = true;
                self.on_cancel(cancel.id);
            }
            Message::Response(_) | Message::Push(_) | Message::Progress(_) => {}
        }
    }

    /// Writes a message of the six toward the view; `false` when it was not (the view is gone, or
    /// it is over a message's limit).
    fn tell(&mut self, payload: &str) -> bool {
        if !matches!(self.phase, Phase::Showing) || !fits(payload) {
            return false;
        }
        self.writer.message("msg", None, None, payload).is_ok()
    }

    fn tell_failure(&mut self, id: u64, error: &ViewError) {
        let message = Message::failure(id, error.code(), error.message().unwrap_or(""));
        self.tell(&payload_of(&message));
    }

    fn on_call(&mut self, id: u64, method: String, params: Value) {
        // An id that is out is not used again; a view that does is not following §9.2.
        if self.calls.contains_key(&id) || self.subscriptions.contains_key(&id) {
            return;
        }
        let Some(handler) = self.options.calls.get(&method) else {
            self.tell_failure(
                id,
                &ViewError::with_message("not-found", "there is nothing to call by that name"),
            );
            return;
        };
        if self.calls.len() >= MAX_CALLS {
            self.tell_failure(
                id,
                &ViewError::with_message("busy", "too many calls are being answered"),
            );
            return;
        }
        let handler = Arc::clone(handler);
        let flag = Arc::new(Flag::new());
        self.calls.insert(id, Arc::clone(&flag));
        let call = Call::new(id, method, flag, self.outbox.clone());
        let started = terminal::spawn_quiet(move || {
            IN_A_HANDLER.with(|on| on.set(true));
            run_call(&*handler, params, &call);
        });
        if started.is_err() {
            self.calls.remove(&id);
            self.tell_failure(
                id,
                &ViewError::with_message("internal", "the call could not be started"),
            );
        }
    }

    fn on_subscribe(&mut self, id: u64, topic: String, params: Value) {
        if self.calls.contains_key(&id) || self.subscriptions.contains_key(&id) {
            return;
        }
        let Some(handler) = self.options.topics.get(&topic) else {
            self.tell_failure(
                id,
                &ViewError::with_message(
                    "not-found",
                    "there is nothing to subscribe to by that name",
                ),
            );
            return;
        };
        let handler = Arc::clone(handler);
        let feed = Arc::new(Feed::new(id, topic, self.outbox.clone()));
        self.subscriptions.insert(id, Arc::clone(&feed));
        let subscription = Subscription::new(feed);
        let started = terminal::spawn_quiet(move || {
            IN_A_HANDLER.with(|on| on.set(true));
            run_topic(&*handler, params, &subscription);
        });
        if started.is_err()
            && let Some(feed) = self.subscriptions.remove(&id)
        {
            feed.end();
            self.tell_failure(
                id,
                &ViewError::with_message("internal", "the subscription could not be started"),
            );
        }
    }

    /// The view gives up a call that is out, or ends a subscription; an id that is not out
    /// changes nothing (§9.2).
    fn on_cancel(&mut self, id: u64) {
        if let Some(call) = self.calls.remove(&id) {
            call.raise();
        } else if let Some(feed) = self.subscriptions.remove(&id) {
            feed.end();
        }
    }

    // ---- what the handlers have to say ---------------------------------------------

    /// Takes what the handlers handed over, as long as the terminal is keeping up.
    fn take_work(&mut self) {
        while self.writer.waiting() < WRITE_ROOM {
            let Ok(work) = self.work.try_recv() else {
                return;
            };
            self.on_work(work);
        }
    }

    fn on_work(&mut self, work: Work) {
        match work {
            Work::Answer { id, result } => {
                // Cancelled, or the view is over: the answer would be ignored.
                let Some(flag) = self.calls.remove(&id) else {
                    return;
                };
                if flag.is_raised() {
                    return;
                }
                match result {
                    Ok(value) => self.answer(id, value),
                    Err(error) => self.tell_failure(id, &error),
                }
            }
            Work::Progress { id, payload } => {
                if self.calls.get(&id).is_some_and(|flag| !flag.is_raised()) {
                    self.tell(&payload);
                }
            }
            Work::Push { id, payload } => {
                if self.subscriptions.contains_key(&id) {
                    self.tell(&payload);
                }
            }
            Work::Started { id, result } => {
                if let Err(error) = result
                    && let Some(feed) = self.subscriptions.remove(&id)
                {
                    feed.end();
                    self.tell_failure(id, &error);
                }
            }
        }
    }

    /// The response that ends a call with `result`: never `null`, nothing to say is `{}`.
    fn answer(&mut self, id: u64, result: Value) {
        let result = if result.is_null() {
            Value::Object(serde_json::Map::new())
        } else {
            result
        };
        let payload = payload_of(&Message::answer(id, result));
        if !fits(&payload) {
            self.tell_failure(
                id,
                &ViewError::with_message(
                    "message-too-long",
                    "the result is over the limit of a message",
                ),
            );
            return;
        }
        self.tell(&payload);
    }

    // ---- the end of the view --------------------------------------------------------

    /// What `closed` means for the program that asked for the view (§13).
    fn on_closed(&mut self, message: &Frame) {
        let Some(shown) = self.shown.clone() else {
            return;
        };
        if self.token.is_none() {
            return;
        }
        let Some(closed) = payload::closed(&message.payload) else {
            return;
        };
        // An answer to an open that is no longer the program's one is not about this view.
        if message.re().is_some_and(|re| re != self.open_id) {
            return;
        }
        let detail = |reason| NoView {
            reason,
            code: closed.code.clone(),
            message: closed.message.clone(),
        };
        let ended = |by| {
            Outcome::Closed(Closed {
                by,
                view: shown.clone(),
                code: closed.code.clone(),
                message: closed.message.clone(),
            })
        };
        let outcome = match closed.reason {
            ClosedReason::User => ended(EndedBy::User),
            ClosedReason::View => ended(EndedBy::View),
            ClosedReason::Program => ended(EndedBy::Program),
            ClosedReason::Refused => Outcome::NoView(detail(NoViewReason::Refused)),
            // A view the terminal draws itself that failed ran; before the view ran, an error is
            // as good as no host.
            ClosedReason::Error if closed.code.as_deref() == Some("view") || self.heard => {
                ended(EndedBy::Error)
            }
            ClosedReason::Error => Outcome::NoView(detail(NoViewReason::Failed)),
        };
        // The terminal has torn the view down and forgotten its token: there is no `bye` to send.
        self.forget_view();
        self.over(Ok(outcome));
    }
}

impl Drop for Session<'_> {
    /// The program is leaving without having finished: a panic. What it can still do for the
    /// terminal it does: say `bye` for the view that is up, and (the fields drop after this)
    /// give the terminal back and throw away what is queued.
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if let Some(token) = self.token.take() {
            self.writer.mute();
            if self
                .writer
                .last_words("bye", &payload::bye(&token, ByeReason::Error))
                .is_ok()
            {
                self.bye_sent = true;
            }
            let _ = self.writer.flush_within(FAREWELL);
        }
        self.taken.give_back(self.bye_sent);
    }
}

/// Whether the bundle's entry has the type the kind of view asks for (§6.3).
pub(crate) fn check_entry(kind: Kind, bundle: &Bundle) -> Result<(), BundleError> {
    match bundle.entry_type() {
        Some(found) if found == kind.entry_type() => Ok(()),
        found => Err(BundleError::mismatch(
            bundle.entry(),
            kind.name(),
            found.unwrap_or("none"),
            kind.entry_type(),
        )),
    }
}
