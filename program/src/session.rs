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
use std::io;
use std::panic;
use std::process::{Command, ExitStatus};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::time::Instant;

use pencere_wire::base64;
use pencere_wire::bundle::Digest;
use pencere_wire::frame::Message as Frame;

use crate::bundle::{Bundle, BundleError};
use crate::control::{self, CloseResult, HandOffResult, OnReturn, ReopenResult, Request};
use crate::exchange::{Exchange, Toward, WORK_ROOM, WRITE_ROOM};
use crate::files::{self, Files};
use crate::foreground::{self, Job};
use crate::handlers::{self, Outbox, Work};
use crate::link::{Item, Reader, Writer};
use crate::payload::{self, ByeReason, ClosedReason, FEATURE_DISMISS, Offer, OpenFields, Welcome};
use crate::terminal::{
    self, DEVICE_ATTRIBUTES, ENTER_ALTERNATE, FAREWELL, Pipe, Read, TakeError, Taken, Terminal,
};
use crate::view::Source;
use crate::{Closed, EndedBy, Error, Kind, NoView, NoViewReason, Options, Outcome, Shown, View};

thread_local! {
    /// This thread is the event loop of a `serve`: a panic on it gives the terminal back before
    /// anything is printed.
    static ON_THE_LOOP: Cell<bool> = const { Cell::new(false) };
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
    /// The terminal is lent to a command (§10.8): the program is a background job of the tty until
    /// the command ends, and touches the tty for nothing, since a read of it would stop the program.
    Lent,
    /// Done: `result` says how.
    Over,
}

/// What comes of leaving.
enum Then {
    /// A signal ended the program's use of the view.
    Signal(libc::c_int),
    /// The program cannot go on.
    Failure(Error),
    /// The terminal stopped answering while the program replaced its view: no view was shown.
    NoView(NoViewReason),
}

/// What the program asked of its view that the terminal has yet to answer (§10.8).
enum Reply {
    Reopen(SyncSender<ReopenResult>),
    Close(SyncSender<CloseResult>),
    HandOff(Box<HandingOff>),
}

/// Why a request of the program's was not acted on.
#[derive(Clone, Copy)]
enum Refusal {
    /// The terminal does not list `dismiss`.
    Unsupported,
    /// There is no view to act on.
    Unavailable,
    /// The terminal is lent to a command already.
    Busy,
}

impl Reply {
    /// Answers a request that was not acted on, or that came to nothing.
    fn refuse(self, why: Refusal) {
        match self {
            Self::Reopen(reply) => {
                let _ = reply.send(match why {
                    Refusal::Unsupported => ReopenResult::Unsupported,
                    Refusal::Unavailable | Refusal::Busy => ReopenResult::Unavailable,
                });
            }
            Self::Close(reply) => {
                let _ = reply.send(match why {
                    Refusal::Unsupported => CloseResult::Unsupported,
                    Refusal::Unavailable | Refusal::Busy => CloseResult::Unavailable,
                });
            }
            Self::HandOff(handing) => {
                for reopen in handing.reopens {
                    let _ = reopen.send(ReopenResult::Unavailable);
                }
                let _ = handing.reply.send(match why {
                    Refusal::Unsupported => HandOffResult::Unsupported,
                    Refusal::Unavailable => HandOffResult::Unavailable,
                    Refusal::Busy => HandOffResult::Busy,
                });
            }
        }
    }
}

/// A hand-off of the terminal under way (§10.8): the command, then the command that has the
/// terminal, then how it ended.
struct HandingOff {
    reply: SyncSender<HandOffResult>,
    /// The command, until it is given the terminal.
    command: Option<Command>,
    /// The command that has the terminal.
    job: Option<Job>,
    /// How the command ended, or why it could not be started, once it has.
    status: Option<io::Result<ExitStatus>>,
    /// The signal that asked the program to end while the command had the terminal.
    signal: Option<libc::c_int>,
    /// The reopenings asked while the command had the terminal: the view that comes back is theirs.
    reopens: Vec<SyncSender<ReopenResult>>,
    /// What the program does with how the command ended, before the view is asked for again.
    then: Option<OnReturn>,
}

impl HandingOff {
    fn new(command: Command, reply: SyncSender<HandOffResult>, then: Option<OnReturn>) -> Self {
        Self {
            reply,
            command: Some(command),
            job: None,
            status: None,
            signal: None,
            reopens: Vec::new(),
            then,
        }
    }

    /// The terminal took the open of the view shown again.
    fn returned(self) {
        for reopen in self.reopens {
            let _ = reopen.send(ReopenResult::Reopened);
        }
        let status = self
            .status
            .unwrap_or_else(|| Err(io::Error::other("the command did not run")));
        let _ = self.reply.send(HandOffResult::Returned(status));
    }

    /// `serve` ends with `result` before the view is shown again.
    fn settle(self, result: &Result<Outcome, Error>) {
        let no_view = match result {
            Ok(Outcome::NoView(no_view)) => Some(no_view.clone()),
            _ => None,
        };
        for reopen in self.reopens {
            let _ = reopen.send(match &no_view {
                Some(no_view) => ReopenResult::NoView(no_view.clone()),
                None => ReopenResult::Unavailable,
            });
        }
        let _ = self.reply.send(match (no_view, self.status) {
            (Some(no_view), status) => HandOffResult::NoView { status, no_view },
            // The view was asked for again and ended as it opened, or the program failed: `serve`
            // says which. The command had the terminal all the same.
            (None, Some(status)) => HandOffResult::Returned(status),
            (None, None) => HandOffResult::Unavailable,
        });
    }
}

/// Where a reopening or a closing is.
#[derive(PartialEq, Eq)]
enum Stage {
    /// The `dismiss` is out; its `closed` has not come.
    Dismissing,
    /// The view is gone and a new `hello` is out.
    Asking,
    /// The new view has been asked for and the terminal has not taken the open.
    Opening,
    /// A hand-off's: the view is gone and the command has the terminal.
    Lent,
}

/// A reopening or a closing under way: the program asked to close the view, and has an answer
/// to give when it is done.
struct Change {
    reply: Reply,
    stage: Stage,
    /// The id of the `dismiss`, whose `closed` ends the first stage.
    dismiss: Option<u32>,
    /// The kinds of view the new `hello` offers instead of the ones so far.
    views: Option<Vec<View>>,
    /// When the terminal is given up on for the stage it is in.
    until: Instant,
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
            if handlers::on_a_handler_thread() {
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

/// The terminal's stream toward the view: a message of the six goes only while one is shown.
struct Stream<'w> {
    writer: &'w mut Writer,
    showing: bool,
}

impl Toward for Stream<'_> {
    fn tell(&mut self, payload: &str) -> bool {
        self.showing && self.writer.message("msg", None, None, payload).is_ok()
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
    /// The terminal lists `dismiss` (§10.8): the program may close its view.
    dismiss: bool,
    /// The kinds of view a reopening offers instead of the options' ones.
    reopened: Option<Vec<View>>,
    /// The program is closing its view or showing another (§10.8).
    change: Option<Change>,
    /// The answer to a `close` that is given when the terminal has been given back, not before:
    /// the program that asked prints, or runs a shell, as soon as it has it.
    closed_reply: Option<SyncSender<CloseResult>>,
    /// The view is up on the terminal's side: what the view says is served.
    live: bool,
    /// What the view asks of the handlers, and what they answer.
    exchange: Exchange,
    /// What the view has attached, and the reads and writes of files that are out.
    files: Files,
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
    let outbox = Outbox::new(sender, Some(Arc::clone(&pipe)));
    let _bound = options
        .control
        .as_ref()
        .map(|control| control.bind(outbox.clone()));
    let mut session = Session {
        options,
        terminal,
        taken,
        pipe: Arc::clone(&pipe),
        reader: Reader::new(),
        writer: Writer::new(terminal.output()),
        work,
        outbox: outbox.clone(),
        phase: Phase::Over,
        fence_answered: false,
        alternate: false,
        token: None,
        shown: None,
        bundle: None,
        open_id: 0,
        accepted: false,
        heard: false,
        dismiss: false,
        reopened: None,
        change: None,
        closed_reply: None,
        live: false,
        exchange: Exchange::new(outbox),
        files: Files::default(),
        bye_sent: false,
        finished: false,
        result: None,
    };
    let result = session.converse();
    session.finish();
    // The terminal is the program's again: now the program that closed its view is told so.
    if let Some(reply) = session.closed_reply.take() {
        let _ = reply.send(CloseResult::Closed);
    }
    result
}

impl Session<'_> {
    /// The kinds of view the program offers: the options', or the ones a reopening brought.
    fn views(&self) -> &[View] {
        self.reopened.as_deref().unwrap_or(&self.options.views)
    }

    /// The offers of the hello and the choice among a terminal's: the kinds the program shows,
    /// with their versions, in the program's order.
    fn offers(&self) -> Vec<(Kind, &[String])> {
        self.views()
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
            // While the terminal is lent the program is a background job of the tty, which a read
            // of it would stop: only the pipe is waited on.
            let input = if matches!(self.phase, Phase::Lent) {
                -1
            } else {
                self.terminal.input()
            };
            let ready = match terminal::wait(input, output, self.pipe.read_end(), timeout) {
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
            // The terminal that does not answer the `dismiss`, or does not take the new open, is
            // not waited for for ever.
            Phase::Showing => self
                .change
                .as_ref()
                .filter(|change| matches!(change.stage, Stage::Dismissing | Stage::Opening))
                .map(|change| change.until),
            Phase::Lent | Phase::Over => None,
        }
    }

    fn deadlines(&mut self) {
        let now = Instant::now();
        match &self.phase {
            Phase::Asking { until, .. } if now >= *until => {
                // The terminal answered before: one that is silent now is late, not absent.
                let reason = if self.change.is_some() {
                    NoViewReason::Timeout
                } else {
                    NoViewReason::NoHost
                };
                self.no_view(reason);
            }
            Phase::Leaving { until, .. } if now >= *until => self.left(),
            Phase::Draining { until, .. } if now >= *until => self.drained(),
            Phase::Showing
                if self.change.as_ref().is_some_and(|change| {
                    matches!(change.stage, Stage::Dismissing | Stage::Opening)
                        && now >= change.until
                }) =>
            {
                // The terminal has the view the program asked to close and does not say so, or
                // does not take the open of the new one: the program ends its wait as it does for
                // a signal, with a `bye` for the view the terminal may still show.
                self.leave(Then::NoView(NoViewReason::Timeout), ByeReason::Error);
            }
            _ => {}
        }
    }

    // ---- signals -----------------------------------------------------------------

    fn signals(&mut self) {
        if let Some(signal) = terminal::take_signal() {
            self.interrupted(signal);
        }
        // A stop while the terminal is lent is not the person's ⌃Z, which goes to the command, and
        // a program that is not the tty's foreground cannot give it back.
        if terminal::take_stop() && !matches!(self.phase, Phase::Over | Phase::Lent) {
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
            Phase::Lent => self.interrupted_while_lent(signal),
        }
    }

    /// A signal while the terminal is lent: the command has the terminal and is waited for, and the
    /// program ends its use of the view when it has ended. A hangup is passed on to the command, as
    /// a shell passes its own on to its jobs: the terminal it has is gone too.
    fn interrupted_while_lent(&mut self, signal: libc::c_int) {
        let Some(handing) = self.hand_off_mut() else {
            return;
        };
        handing.signal.get_or_insert(signal);
        if signal == libc::SIGHUP
            && let Some(job) = &handing.job
        {
            job.signal(libc::SIGHUP);
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
            Then::NoView(reason) => Ok(Outcome::NoView(NoView {
                reason,
                code: None,
                message: None,
            })),
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
        self.exchange.end_view();
        self.files.forget();
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
        self.settle_change(&result);
        self.phase = Phase::Over;
        if self.result.is_none() {
            self.result = Some(result);
        }
    }

    /// The program's reopening or closing that is under way ends with `result`: a reopening that
    /// came to no view says why, anything else leaves the program with nothing to wait for.
    fn settle_change(&mut self, result: &Result<Outcome, Error>) {
        let Some(change) = self.change.take() else {
            return;
        };
        match change.reply {
            Reply::Reopen(reply) => {
                let _ = reply.send(match result {
                    Ok(Outcome::NoView(no_view)) => ReopenResult::NoView(no_view.clone()),
                    _ => ReopenResult::Unavailable,
                });
            }
            Reply::Close(reply) => {
                let _ = reply.send(CloseResult::Unavailable);
            }
            Reply::HandOff(handing) => handing.settle(result),
        }
    }

    /// No view will be shown. The terminal's answer to `CSI c` is read first if it has not come,
    /// so that it does not reach the shell as typed text (§5.3).
    fn no_view(&mut self, reason: NoViewReason) {
        self.no_view_with(reason, None);
    }

    /// As [`Self::no_view`], with words for the person.
    fn no_view_with(&mut self, reason: NoViewReason, message: Option<String>) {
        let no_view = NoView {
            reason,
            code: None,
            message,
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
        self.taken.hung_up();
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
            // Nothing is read or written while the terminal is lent; were it to go then, the
            // command, which has it, is told as of a hangup and waited for.
            Phase::Lent => {
                self.phase = Phase::Lent;
                self.interrupted_while_lent(libc::SIGHUP);
            }
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
            // A reopening asks without the fence: this reply is the first view's, late.
            Phase::Asking { .. } if self.change.is_some() => {}
            Phase::Asking { .. } => self.no_view(NoViewReason::NoHost),
            Phase::Draining { .. } => self.drained(),
            _ => {}
        }
    }

    fn on_message(&mut self, message: &Frame) {
        match &self.phase {
            Phase::Asking { hello, .. } => {
                if message.kind != "welcome" {
                    return;
                }
                if message.re() == Some(*hello) {
                    self.on_answer(message);
                } else if message.re().is_none() && self.change.is_some() {
                    // The terminal came up again while the program was asking for another view:
                    // it asks for the view, and the answer to the `hello` is not waited for.
                    if let Some(Welcome::Offer(offer)) = payload::welcome(&message.payload)
                        && offer.restart
                    {
                        self.restart_with(offer);
                    }
                }
            }
            Phase::Showing => match message.kind.as_str() {
                "welcome" => self.on_welcome(message),
                "need" if self.files.answers_attach(message.re()) => {
                    self.files.on_need(message, &mut self.writer);
                }
                "need" => self.on_need(message),
                "closed" => self.on_closed(message),
                "msg" => self.on_msg(message),
                "data" => self.files.on_data(message),
                "wrote" => self.files.on_wrote(message),
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
            Phase::Draining { .. } | Phase::Lent | Phase::Over => {}
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
                // does not wait (§5.3). When the terminal opens a kind of the program's in other
                // versions, it says which and the program says so in words (§5.2).
                None => {
                    let message = payload::no_version_sentence(&self.offers(), &offer.opens);
                    self.no_view_with(NoViewReason::NoCommonView, message);
                }
                Some(choice) => {
                    self.dismiss = offer
                        .features
                        .iter()
                        .any(|feature| feature == FEATURE_DISMISS);
                    self.open(offer.token, choice);
                }
            },
        }
    }

    /// The view to open, if the terminal will open one of the program's kinds (§5.3).
    fn choose(&self, offer: &Offer) -> Option<(usize, String)> {
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
        self.restart_with(offer);
    }

    /// The terminal asks for the view again (§10.7): the program opens with the token it gave.
    fn restart_with(&mut self, offer: Offer) {
        let Some(choice) = self.choose(&offer) else {
            return;
        };
        // A closing or a reopening that was under way is over with the view it was about; a hand-off
        // whose command has ended goes on with the view the terminal asks for.
        if !self.returning() {
            self.drop_change();
        }
        // What the old view had asked is over with it, and its token is the terminal's to have
        // forgotten: until the new open is out there is no view, and a `bye` with the dead token
        // would be nobody's.
        self.forget_view();
        self.dismiss = offer
            .features
            .iter()
            .any(|feature| feature == FEATURE_DISMISS);
        self.open(offer.token, choice);
    }

    /// The program's reopening or closing is over without having come to anything: the caller
    /// is told there was nothing to do.
    fn drop_change(&mut self) {
        if let Some(change) = self.change.take() {
            change.reply.refuse(Refusal::Unavailable);
        }
    }

    /// The hand-off under way, if one is.
    fn hand_off_mut(&mut self) -> Option<&mut HandingOff> {
        match &mut self.change {
            Some(Change {
                reply: Reply::HandOff(handing),
                ..
            }) => Some(handing),
            _ => None,
        }
    }

    /// A hand-off is under way whose command has not had the terminal back: a reopening asked now
    /// is the view that comes back.
    fn lending(&self) -> bool {
        self.change.as_ref().is_some_and(|change| {
            matches!(change.reply, Reply::HandOff(_))
                && matches!(change.stage, Stage::Dismissing | Stage::Lent)
        })
    }

    /// A hand-off is under way whose command has ended: the view is being shown again.
    fn returning(&self) -> bool {
        self.change
            .as_ref()
            .is_some_and(|change| match &change.reply {
                Reply::HandOff(handing) => handing.status.is_some(),
                _ => false,
            })
    }

    /// Asks for the view with the token of a `welcome` (§6.3): the kind and the version chosen.
    fn open(&mut self, token: String, (at, version): (usize, String)) {
        let options = self.options;
        let Some(spec) = self.views().get(at) else {
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
        let open = payload::open(
            &OpenFields {
                token: &token,
                kind,
                version: &version,
                program: options.program.as_deref(),
                title: options.title.as_deref(),
                icons: &options.icons,
                access: spec.access.as_ref(),
            },
            &bundle,
        );
        self.phase = Phase::Showing;
        if let Some(change) = &mut self.change {
            change.stage = Stage::Opening;
            change.until = Instant::now() + options.reopen_timeout;
        }
        if !self.alternate {
            self.alternate = true;
            terminal::note_alternate(true);
            self.writer.raw(ENTER_ALTERNATE);
        }
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
        // The open the terminal answers is the old view's, and a `dismiss` for that view is out:
        // nothing is sent to a view that is being closed, and no reopening has taken its open.
        if self
            .change
            .as_ref()
            .is_some_and(|change| change.stage == Stage::Dismissing)
        {
            return;
        }
        let Some(missing) = payload::need(&message.payload) else {
            return;
        };
        self.accepted = true;
        // The terminal took the open of the view the program asked for in place of the old one.
        match self.change.take().map(|change| change.reply) {
            Some(Reply::Reopen(reply)) => {
                let _ = reply.send(ReopenResult::Reopened);
            }
            Some(Reply::HandOff(handing)) => handing.returned(),
            Some(Reply::Close(_)) | None => {}
        }
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
        let mut stream = Stream {
            writer: &mut self.writer,
            showing: matches!(self.phase, Phase::Showing),
        };
        if self
            .exchange
            .on_message(self.options, &message.payload, &mut stream)
        {
            self.heard = true;
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
            Work::Control(request) => self.on_control(request),
            Work::Returned(status) => self.returned(status),
            work => {
                let mut stream = Stream {
                    writer: &mut self.writer,
                    showing: matches!(self.phase, Phase::Showing),
                };
                self.exchange.on_work(work, &mut stream);
            }
        }
    }

    // ---- closing the view at the program's request (§10.8) --------------------------

    /// A view runs, its open taken and nothing being done to it, at a version that has files
    /// beside its bundle (§5.5).
    fn has_files(&self) -> bool {
        matches!(self.phase, Phase::Showing)
            && self.live
            && self.accepted
            && self.change.is_none()
            && self
                .shown
                .as_ref()
                .is_some_and(|shown| files::has_files(shown.kind, &shown.version))
    }

    /// The program asks to close its view, or to show another in its place, or for a file.
    fn on_control(&mut self, request: Request) {
        // While a command has the terminal, or is about to, a reopening is the view that comes back:
        // with the views it brings, and answered as that view comes back.
        let request = match request {
            Request::Reopen { views, reply } if self.lending() => {
                if let Some(change) = &mut self.change {
                    if views.is_some() {
                        change.views = views;
                    }
                    if let Reply::HandOff(handing) = &mut change.reply {
                        handing.reopens.push(reply);
                    }
                }
                return;
            }
            request @ (Request::Attach { .. }
            | Request::Detach { .. }
            | Request::Read { .. }
            | Request::Write { .. }) => {
                let has = self.has_files();
                self.files.on_request(request, has, &mut self.writer);
                return;
            }
            request => request,
        };
        let lent = self
            .change
            .as_ref()
            .is_some_and(|change| matches!(change.reply, Reply::HandOff(_)));
        let (reply, views) = match request {
            Request::Reopen { views, reply } => (Reply::Reopen(reply), views),
            Request::Close { reply } => (Reply::Close(reply), None),
            Request::HandOff {
                command,
                reply,
                then,
            } => (
                Reply::HandOff(Box::new(HandingOff::new(command, reply, then))),
                None,
            ),
            // Taken above.
            Request::Attach { .. }
            | Request::Detach { .. }
            | Request::Read { .. }
            | Request::Write { .. } => return,
        };
        // There is a view to close only while one is opening or open and nothing else is being
        // done to it; and only a terminal that lists `dismiss` closes it (any other ignores the
        // kind, and the view would stay).
        let unavailable =
            !matches!(self.phase, Phase::Showing) || self.token.is_none() || self.change.is_some();
        if unavailable || !self.dismiss {
            reply.refuse(if lent {
                Refusal::Busy
            } else if unavailable {
                Refusal::Unavailable
            } else {
                Refusal::Unsupported
            });
            return;
        }
        let Some(token) = self.token.clone() else {
            return;
        };
        // What the view had asked is over with it; the terminal writes nothing it says from here.
        self.end_view();
        self.live = false;
        let sent = self
            .writer
            .message("dismiss", None, None, &payload::dismiss(&token));
        match sent {
            Ok(id) => {
                self.change = Some(Change {
                    reply,
                    stage: Stage::Dismissing,
                    dismiss: Some(id),
                    views,
                    until: Instant::now() + self.options.reopen_timeout,
                });
            }
            Err(_) => {
                reply.refuse(Refusal::Unavailable);
                self.fail(Error::Io(io::Error::other("the dismiss cannot be written")));
            }
        }
    }

    /// The `closed` that answers the `dismiss` has come: the terminal has torn the view down and
    /// forgotten its token, so there is no `bye` to send.
    fn dismissed(&mut self) {
        let Some(mut change) = self.change.take() else {
            return;
        };
        let shown = self.shown.clone();
        self.forget_view();
        match change.reply {
            Reply::Close(reply) => {
                let outcome = match shown {
                    Some(view) => Outcome::Closed(Closed {
                        by: EndedBy::Program,
                        view,
                        code: None,
                        message: None,
                    }),
                    None => Outcome::NoView(NoView {
                        reason: NoViewReason::NoHost,
                        code: None,
                        message: None,
                    }),
                };
                self.closed_reply = Some(reply);
                self.over(Ok(outcome));
            }
            Reply::HandOff(handing) => {
                change.reply = Reply::HandOff(handing);
                change.stage = Stage::Lent;
                self.change = Some(change);
                self.lend();
            }
            Reply::Reopen(reply) => {
                if let Some(views) = change.views.take() {
                    self.reopened = Some(views);
                }
                // The pane is idle and the program wants a view: a new token takes a `hello`,
                // then an `open`, as for the first. The terminal is known and the fence is for
                // telling "no host" from "no answer yet", so there is none.
                let hello = payload::hello(&self.offers());
                let until = Instant::now() + self.options.reopen_timeout;
                match self.writer.message("hello", None, None, &hello) {
                    Ok(id) => {
                        self.phase = Phase::Asking { hello: id, until };
                        self.change = Some(Change {
                            reply: Reply::Reopen(reply),
                            stage: Stage::Asking,
                            dismiss: None,
                            views: None,
                            until,
                        });
                    }
                    Err(_) => {
                        let _ = reply.send(ReopenResult::Unavailable);
                        self.fail(Error::Io(io::Error::other("the hello cannot be written")));
                    }
                }
            }
        }
    }

    // ---- lending the terminal to a command (§10.8) -----------------------------------

    /// The view the program closed for a hand-off is gone: the terminal is lent to the command,
    /// as it is given back at the end (the screen, the modes), and the command is started as its
    /// foreground job. A thread waits for it and wakes the loop when it has ended.
    fn lend(&mut self) {
        // What is waiting to be written goes out on the view's screen, before it is left; what a
        // terminal that is not reading has not taken by then is dropped, since nothing may be
        // written while the command has the terminal.
        let _ = self.writer.flush_within(FAREWELL);
        self.writer.forget_everything();
        self.taken.lend();
        self.alternate = false;
        self.phase = Phase::Lent;
        let tty = self.terminal.input();
        let Some(command) = self
            .hand_off_mut()
            .and_then(|handing| handing.command.take())
        else {
            return;
        };
        let job = match foreground::start(command, tty) {
            Ok(job) => job,
            Err(why) => {
                // The view is shown again, and the hand-off says why the command did not run.
                self.returned(Err(why));
                return;
            }
        };
        if let Some(handing) = self.hand_off_mut() {
            handing.job = Some(job.clone());
        }
        let outbox = self.outbox.clone();
        let waited = job.clone();
        let waiting = terminal::spawn_quiet(move || {
            outbox.send(Work::Returned(waited.wait()));
        });
        if waiting.is_err() {
            // No thread to wait on: the loop waits, having nothing to do while the command has the
            // terminal but hear the signals, which it then hears after.
            let status = job.wait();
            self.returned(status);
        }
    }

    /// The command the terminal was lent to has ended: the foreground and the modes are taken back
    /// and the view is asked for again, as a reopening asks (the views a reopening brought in the
    /// meantime, if one did). A signal that came while the command had the terminal ends the
    /// program's use of the view instead.
    fn returned(&mut self, status: io::Result<ExitStatus>) {
        if !matches!(self.phase, Phase::Lent) {
            return;
        }
        let Some(handing) = self.hand_off_mut() else {
            return;
        };
        if let Some(job) = &handing.job {
            job.take_back();
        }
        // The program keeps what it needs of how the command ended before the view is asked for
        // again, so that the first call of the view shown again finds it.
        control::tell_then(handing.then.take(), &status);
        handing.status = Some(status);
        let signal = handing.signal;
        self.taken.take_back();
        if let Some(signal) = signal {
            self.over(Ok(Outcome::NoView(NoView {
                reason: NoViewReason::Interrupted(signal),
                code: None,
                message: None,
            })));
            return;
        }
        if let Some(views) = self.change.as_mut().and_then(|change| change.views.take()) {
            self.reopened = Some(views);
        }
        // A new token takes a `hello`, without the fence: the terminal is known (§10.8).
        let hello = payload::hello(&self.offers());
        let until = Instant::now() + self.options.reopen_timeout;
        match self.writer.message("hello", None, None, &hello) {
            Ok(id) => {
                self.phase = Phase::Asking { hello: id, until };
                if let Some(change) = &mut self.change {
                    change.stage = Stage::Asking;
                    change.until = until;
                }
            }
            Err(_) => self.fail(Error::Io(io::Error::other("the hello cannot be written"))),
        }
    }

    // ---- the end of the view --------------------------------------------------------

    /// What `closed` means for the program that asked for the view (§13).
    fn on_closed(&mut self, message: &Frame) {
        // The answer to the `dismiss` the program sent (§10.8).
        if self.change.as_ref().is_some_and(|change| {
            change.stage == Stage::Dismissing
                && change.dismiss.is_some()
                && message.re() == change.dismiss
        }) {
            self.dismissed();
            return;
        }
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
