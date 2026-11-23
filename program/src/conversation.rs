//! `Conversation`: the program's handlers, served in the same process as the view, with no
//! terminal in between.
//!
//! `serve` carries the six messages over a terminal's stream; a conversation hands them to whoever
//! holds it — a host that runs the page in the same process, or the program's own tests — and
//! takes them back. Everything between is `serve`'s, by the same code: each call on a thread of
//! its own or of its lane, the subscriptions and their cleanups, the limit of a message, a panic as
//! the `internal` error of its call, the answers of a view that is gone dropped, and the pace of
//! the reader: what the handlers say waits for it, and a handler that pushes faster than it is read
//! waits as it would for a terminal.
//!
//! ```
//! use pencere::{Conversation, Options, Said};
//! use serde_json::json;
//! use std::time::Duration;
//!
//! let mut conversation = Conversation::new(
//!     Options::new().call("add", |params, _call| {
//!         Ok(json!(params["a"].as_i64().unwrap_or(0) + params["b"].as_i64().unwrap_or(0)))
//!     }),
//! );
//! conversation.send(r#"{"call":{"id":1,"method":"add","params":{"a":2,"b":3}}}"#);
//! let Some(Said::Message(answer)) = conversation.receive(Duration::from_secs(5)) else {
//!     panic!("an answer");
//! };
//! assert_eq!(answer, r#"{"response":{"id":1,"result":5}}"#);
//! ```

use std::collections::VecDeque;
use std::fmt;
use std::io;
use std::process::{Command, ExitStatus};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::control::{self, Bound, OnReturn, Request};
use crate::exchange::{Exchange, Toward, WORK_ROOM, WRITE_ROOM};
use crate::files::{FileError, name_of};
use crate::handlers::{Outbox, Work};
use crate::{CloseResult, HandOffResult, Options, ReopenResult, Take, View};

/// What the program says to the view in a [`Conversation`], in the order it says it.
#[derive(Debug)]
#[non_exhaustive]
pub enum Said {
    /// A message of the six toward the view — a `response`, a `push` or a `progress` (§9.2) — as
    /// the JSON text a terminal would be given: ASCII, any other character as an escape
    /// (`\u00e9`).
    Message(String),
    /// The program closed its view ([`crate::ViewControl::close`], which was told `Closed`):
    /// what the view had asked is over, and nothing more is heard or said.
    Closed,
    /// The program showed another view in its place ([`crate::ViewControl::reopen`], which was
    /// told `Reopened`): what the old view had asked is over, and what is sent from here on is
    /// the new view's. The kinds of view it brings, when it brings others.
    Reopened(Option<Vec<View>>),
    /// The program lent the terminal to a command ([`crate::ViewControl::hand_off`]), which a
    /// conversation does not run: what the view had asked is over, nothing is heard until the
    /// [`Lent`] says how the command ended (or is dropped), and what is sent from then on is the
    /// view's shown again. A reopening the program asks for meanwhile is that view, as it is for
    /// `serve`: when it brings other kinds of view, they are said ([`Said::Reopened`]) as the view
    /// comes back.
    HandedOff(Lent),
    /// The program attached files to the view (§8.8, [`crate::ViewControl::attach`]), which the
    /// program waits to hear held. Said in the order the program said it, so a file is held before
    /// the message that names it is heard.
    Attached(Attached),
    /// The program let go of the attached files of these names (`sha256:` and the digest): the
    /// holder forgets them.
    Detached(Vec<String>),
    /// The program asks for a piece of a file the person gave the view (§8.9,
    /// [`crate::ViewControl::read`]), which it waits to be given.
    Read(Reading),
    /// The program writes a piece of a file to a place the person chose in the Save panel (§8.10,
    /// [`crate::ViewControl::write`]), and waits to hear it was written. The pieces of a handle come
    /// in order.
    Write(Writing),
}

/// A piece of a file the person gave, which the program asks for in a [`Conversation`] by the file's
/// handle: the holder of the files reads it and says it with [`Reading::bytes`], or says why it
/// cannot with [`Reading::refused`] (`handle`, `range`, `busy`, `gone`, `io`: §8.9). Dropped without
/// either, the program is told that no view holds files ([`FileError::Unsupported`]). The program
/// asks for at most [`crate::MAX_PIECE`] bytes; fewer than it asked for is the end of the file.
#[derive(Debug)]
pub struct Reading {
    handle: String,
    offset: u64,
    length: u64,
    reply: Option<SyncSender<Result<Vec<u8>, FileError>>>,
}

impl Reading {
    /// The handle of the file, which the holder made when the person gave it.
    #[must_use]
    pub fn handle(&self) -> &str {
        &self.handle
    }

    /// Where in the file the piece starts.
    #[must_use]
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// How many bytes at most, and not 0: the holder says `range` of a 0 or one over
    /// [`crate::MAX_PIECE`].
    #[must_use]
    pub fn length(&self) -> u64 {
        self.length
    }

    /// Says the piece: the program has it.
    pub fn bytes(mut self, bytes: Vec<u8>) {
        self.answer(Ok(bytes));
    }

    /// Says why there is no piece, by the code (§8.9).
    pub fn refused(mut self, code: &str) {
        self.answer(Err(FileError::Refused(code.to_owned())));
    }

    fn answer(&mut self, result: Result<Vec<u8>, FileError>) {
        if let Some(reply) = self.reply.take() {
            let _ = reply.send(result);
        }
    }
}

impl Drop for Reading {
    fn drop(&mut self) {
        self.answer(Err(FileError::Unsupported));
    }
}

/// A piece of a file the program writes to the place the person chose in the Save panel, by its
/// handle: the holder writes it and says so with [`Writing::written`], or says why it cannot with
/// [`Writing::refused`] (`handle`, `order`, `size`, `space`, `io`: §8.10). The last piece has
/// [`Writing::end`]; the holder gives the file the person's name then, and the program waits for
/// that. Dropped without either, the program is told that no view holds files
/// ([`FileError::Unsupported`]).
#[derive(Debug)]
pub struct Writing {
    handle: String,
    offset: u64,
    bytes: Vec<u8>,
    end: bool,
    reply: Option<SyncSender<Result<(), FileError>>>,
}

impl Writing {
    /// The handle of the place, which the holder made when the person chose it.
    #[must_use]
    pub fn handle(&self) -> &str {
        &self.handle
    }

    /// Where in the file the piece goes: where the one before it ended.
    #[must_use]
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// The piece: at most [`crate::MAX_PIECE`] bytes (the holder says `size` of more).
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The piece is the file's last.
    #[must_use]
    pub fn end(&self) -> bool {
        self.end
    }

    /// Says the piece is written (and, for the last, that the file has the person's name).
    pub fn written(mut self) {
        self.answer(Ok(()));
    }

    /// Says why the piece is not written, by the code (§8.10).
    pub fn refused(mut self, code: &str) {
        self.answer(Err(FileError::Refused(code.to_owned())));
    }

    fn answer(&mut self, result: Result<(), FileError>) {
        if let Some(reply) = self.reply.take() {
            let _ = reply.send(result);
        }
    }
}

impl Drop for Writing {
    fn drop(&mut self) {
        self.answer(Err(FileError::Unsupported));
    }
}

/// The files the program attached to the view in a [`Conversation`], each by the name the view draws
/// it by (`sha256:` and its digest), and the way to say whether they are held. A conversation holds
/// no files: whoever draws the page does, as a terminal does; and says so, within the limits
/// of §8.8 ([`crate::MAX_ATTACHED_FILES`], [`crate::MAX_ATTACHED_FILE`] and
/// [`crate::MAX_ATTACHED_TOTAL`]), with [`Attached::held`], or [`Attached::refused`] with the
/// code (`limit`). Dropped without either, the program is told that no view holds files
/// ([`FileError::Unsupported`]).
#[derive(Debug)]
pub struct Attached {
    files: Vec<(String, Arc<[u8]>)>,
    reply: Option<SyncSender<Result<(), FileError>>>,
}

impl Attached {
    /// The files: the name of each and its bytes.
    #[must_use]
    pub fn files(&self) -> &[(String, Arc<[u8]>)] {
        &self.files
    }

    /// Says the files are held: the program's attach returns, and the view may draw them.
    pub fn held(mut self) {
        self.answer(Ok(()));
    }

    /// Says the files are not held, by the code of the refusal (`limit` when they are more than
    /// the view may have).
    pub fn refused(mut self, code: &str) {
        self.answer(Err(FileError::Refused(code.to_owned())));
    }

    fn answer(&mut self, result: Result<(), FileError>) {
        if let Some(reply) = self.reply.take() {
            let _ = reply.send(result);
        }
    }
}

impl Drop for Attached {
    fn drop(&mut self) {
        self.answer(Err(FileError::Unsupported));
    }
}

/// The terminal a program lent to a command in a [`Conversation`], where no command runs: the
/// command, and the way to say how it ended. Dropped without [`Lent::returned`], the program's
/// hand-off is told the command could not be started (`ErrorKind::Unsupported`).
#[derive(Debug)]
pub struct Lent {
    command: Box<Command>,
    /// How the command ended, for the conversation to tell the program.
    loan: Loan,
    told: bool,
}

/// How a command a conversation did not run ended, once the holder of its [`Lent`] has said.
type Loan = Arc<Mutex<Option<io::Result<ExitStatus>>>>;

impl Lent {
    /// The command the program asked to run.
    #[must_use]
    pub fn command(&self) -> &Command {
        &self.command
    }

    /// Says the command ended with `status`: the program's hand-off is told so, as when a
    /// terminal takes the open of the view shown again, and the view is heard again.
    pub fn returned(mut self, status: ExitStatus) {
        self.finish(Ok(status));
    }

    /// The command, for the holder that runs it ([`crate::Tty::lend`]); what is left is a command
    /// that does nothing, which [`Lent::command`] would then show.
    pub(crate) fn take_command(&mut self) -> Command {
        std::mem::replace(&mut *self.command, Command::new("true"))
    }

    /// Says how the command ended, or why it could not be run.
    pub(crate) fn ended(mut self, status: io::Result<ExitStatus>) {
        self.finish(status);
    }

    fn finish(&mut self, status: io::Result<ExitStatus>) {
        if !self.told {
            self.told = true;
            *self.loan.lock().unwrap_or_else(PoisonError::into_inner) = Some(status);
        }
    }
}

impl Drop for Lent {
    fn drop(&mut self) {
        self.finish(Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "a conversation runs no command",
        )));
    }
}

/// A program's handlers served in the same process as its view: [`Conversation::send`] gives them
/// what the view says, and [`Conversation::receive`] what the program says back. Made from the
/// same [`Options`] as `serve` — its calls, lanes, topics and control; its views, terminal,
/// fallback and times are a terminal's and are not used — and served by the same code.
///
/// What the handlers say waits until it is received: a conversation is driven by its reader, and
/// the requests of the program's [`crate::ViewControl`] are answered as they are received, as
/// [`Said::Closed`], [`Said::Reopened`] and [`Said::HandedOff`]. Dropping it ends what the view
/// had asked: calls are cancelled and subscriptions end.
pub struct Conversation {
    options: Options,
    exchange: Exchange,
    work: Receiver<Work>,
    /// What the program has said and was not received yet.
    said: VecDeque<Said>,
    /// The bytes of the messages among it.
    waiting: usize,
    /// The view is up: the program has not closed it.
    open: bool,
    /// The hand-off under way, while there is one.
    lent: Option<Lending>,
    /// The program's control is the conversation's until the view is closed, as it is `serve`'s
    /// until `serve` ends.
    bound: Option<Bound>,
}

impl fmt::Debug for Conversation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Conversation")
            .field("options", &self.options)
            .field("open", &self.open)
            .field("waiting", &self.said.len())
            .finish_non_exhaustive()
    }
}

/// A hand-off under way in a conversation: the program waits for the holder of the [`Lent`].
struct Lending {
    loan: Loan,
    reply: SyncSender<HandOffResult>,
    /// The reopenings asked meanwhile, answered as the view comes back.
    reopens: Vec<SyncSender<ReopenResult>>,
    /// The kinds of view the last of them brings.
    views: Option<Vec<View>>,
    /// What the program does with how the command ended, before the view is up again.
    then: Option<OnReturn>,
}

/// The conversation's queue, toward the view: a message of the six goes only while a view is up.
struct Queue<'q> {
    said: &'q mut VecDeque<Said>,
    waiting: &'q mut usize,
    up: bool,
}

impl Toward for Queue<'_> {
    fn tell(&mut self, payload: &str) -> bool {
        if !self.up {
            return false;
        }
        *self.waiting += payload.len();
        self.said.push_back(Said::Message(payload.to_owned()));
        true
    }
}

impl Conversation {
    /// A conversation with the handlers of `options`, and a view that is up.
    #[must_use]
    pub fn new(options: Options) -> Self {
        Self::with_pipe(options, None)
    }

    /// A conversation as [`Conversation::new`] makes it, whose handlers use `waker` when they have
    /// something to be received: a reader that waits on what the waker wakes (a [`crate::Tty`]'s
    /// wait) is told, and need not look for it.
    #[must_use]
    pub fn waking(options: Options, waker: &crate::Waker) -> Self {
        Self::with_pipe(options, Some(Arc::clone(&waker.0)))
    }

    fn with_pipe(options: Options, pipe: Option<Arc<crate::terminal::Pipe>>) -> Self {
        let (sender, work) = sync_channel(WORK_ROOM);
        let outbox = Outbox::new(sender, pipe);
        let bound = options
            .control
            .as_ref()
            .map(|control| control.bind(outbox.clone()));
        Self {
            options,
            exchange: Exchange::new(outbox),
            work,
            said: VecDeque::new(),
            waiting: 0,
            open: true,
            lent: None,
            bound,
        }
    }

    /// The view says `message`: the JSON text of a `call`, a `subscribe` or a `cancel` (§9.2),
    /// read as `serve` reads what a view sends. Whether it was one the program heard: what is not
    /// one of the three, is over the size or the depth of a message, or comes while there is no
    /// view (it was closed, or the terminal is lent) is left.
    pub fn send(&mut self, message: &str) -> bool {
        self.settle_lent();
        if !self.up() {
            return false;
        }
        let mut queue = Queue {
            said: &mut self.said,
            waiting: &mut self.waiting,
            up: true,
        };
        self.exchange.on_message(&self.options, message, &mut queue)
    }

    /// The terminal hands the program `take` (specification §10.9), as it would write a `take`:
    /// pushed to the view's subscriptions of [`TAKE_TOPIC`](crate::TAKE_TOPIC). Whether the program
    /// heard it: one of a kind it did not say it takes ([`Options::takes`]), one the wire would not
    /// carry ([`Take::is_valid`]), and one that comes while there is no view (it was closed, or the
    /// terminal is lent), is left.
    pub fn hand_over(&mut self, take: &Take) -> bool {
        self.settle_lent();
        if !self.up() || !take.is_valid() || !self.options.takes.contains(&take.kind()) {
            return false;
        }
        let mut queue = Queue {
            said: &mut self.said,
            waiting: &mut self.waiting,
            up: true,
        };
        self.exchange.on_take(take, &mut queue);
        true
    }

    /// What the program says next, if it has said something; does not wait.
    pub fn try_receive(&mut self) -> Option<Said> {
        self.next(None)
    }

    /// What the program says next, waiting up to `within` for it to say something.
    pub fn receive(&mut self, within: Duration) -> Option<Said> {
        self.next(Some(Instant::now() + within))
    }

    /// A view is up: the program has not closed it, and has not lent the terminal, or the
    /// command it lent it to has ended.
    fn up(&self) -> bool {
        self.open && self.lent.is_none()
    }

    /// A hand-off whose `Lent` has said how the command ended is over: the program is told, and the
    /// view is up again (with the kinds a reopening brought meanwhile, said as it comes back).
    fn settle_lent(&mut self) {
        let ended = self.lent.as_ref().and_then(|lending| {
            lending
                .loan
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
        });
        let Some(status) = ended else {
            return;
        };
        let Some(lending) = self.lent.take() else {
            return;
        };
        // Before the view is up again: what the view says next finds what the program kept.
        control::tell_then(lending.then, &status);
        for reopen in lending.reopens {
            let _ = reopen.send(ReopenResult::Reopened);
        }
        if lending.views.is_some() {
            self.said.push_back(Said::Reopened(lending.views));
        }
        let _ = lending.reply.send(HandOffResult::Returned(status));
    }

    fn next(&mut self, until: Option<Instant>) -> Option<Said> {
        loop {
            self.settle_lent();
            self.take_work();
            if let Some(said) = self.said.pop_front() {
                if let Said::Message(text) = &said {
                    self.waiting = self.waiting.saturating_sub(text.len());
                }
                return Some(said);
            }
            let left = until?.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            match self.work.recv_timeout(left) {
                Ok(work) => self.on_work(work),
                Err(_) => return None,
            }
        }
    }

    /// Takes what the handlers handed over, as long as the reader is keeping up.
    fn take_work(&mut self) {
        while self.waiting < WRITE_ROOM {
            let Ok(work) = self.work.try_recv() else {
                return;
            };
            self.on_work(work);
        }
    }

    fn on_work(&mut self, work: Work) {
        match work {
            Work::Control(request) => self.on_control(request),
            // No command runs in a conversation.
            Work::Returned(_) => {}
            work => {
                let up = self.up();
                let mut queue = Queue {
                    said: &mut self.said,
                    waiting: &mut self.waiting,
                    up,
                };
                self.exchange.on_work(work, &mut queue);
            }
        }
    }

    /// The program asks to close its view, to show another, or to lend the terminal: done at
    /// once, as by a terminal that lists `dismiss` and answers at once, and said.
    fn on_control(&mut self, request: Request) {
        self.settle_lent();
        match request {
            // While the terminal is lent, a reopening is the view that comes back.
            Request::Reopen { views, reply } if self.open && self.lent.is_some() => {
                if let Some(lending) = &mut self.lent {
                    if views.is_some() {
                        lending.views = views;
                    }
                    lending.reopens.push(reply);
                }
            }
            Request::Reopen { reply, .. } if !self.up() => {
                let _ = reply.send(ReopenResult::Unavailable);
            }
            // The files go to whoever draws the page, in the order the program said them: it holds
            // them or does not. A view that is not up has none.
            Request::Attach { files, reply } if self.up() => {
                let files = files
                    .into_iter()
                    .map(|(digest, bytes)| (name_of(&digest), bytes))
                    .collect();
                self.said.push_back(Said::Attached(Attached {
                    files,
                    reply: Some(reply),
                }));
            }
            // The person's files are the holder's too: it made the handles, and reads and writes
            // what they name. A view that is not up has none.
            Request::Read {
                handle,
                offset,
                length,
                reply,
            } if self.up() => {
                self.said.push_back(Said::Read(Reading {
                    handle,
                    offset,
                    length,
                    reply: Some(reply),
                }));
            }
            Request::Write {
                handle,
                offset,
                bytes,
                end,
                reply,
            } if self.up() => {
                self.said.push_back(Said::Write(Writing {
                    handle,
                    offset,
                    bytes,
                    end,
                    reply: Some(reply),
                }));
            }
            // With no view up the program has no files to read or write.
            Request::Attach { reply, .. } | Request::Write { reply, .. } => {
                let _ = reply.send(Err(FileError::Unsupported));
            }
            Request::Read { reply, .. } => {
                let _ = reply.send(Err(FileError::Unsupported));
            }
            Request::StageOffer { reply } => {
                let _ = reply.send(Err(FileError::Unsupported));
            }
            // Nothing is asked of a folder that nobody offered.
            Request::Stage { .. } | Request::Fetch { .. } | Request::Unstage { .. } => {}
            Request::Detach { digests } => {
                if self.open && !digests.is_empty() {
                    let names = digests.iter().map(name_of).collect();
                    self.said.push_back(Said::Detached(names));
                }
            }
            Request::Reopen { views, reply } => {
                self.exchange.end_view();
                let _ = reply.send(ReopenResult::Reopened);
                self.said.push_back(Said::Reopened(views));
            }
            Request::Close { reply } if !self.up() => {
                let _ = reply.send(CloseResult::Unavailable);
            }
            Request::Close { reply } => {
                self.exchange.end_view();
                self.open = false;
                self.bound = None;
                let _ = reply.send(CloseResult::Closed);
                self.said.push_back(Said::Closed);
            }
            Request::HandOff { reply, .. } if !self.up() => {
                let _ = reply.send(if self.open {
                    HandOffResult::Busy
                } else {
                    HandOffResult::Unavailable
                });
            }
            Request::HandOff {
                command,
                reply,
                then,
            } => {
                self.exchange.end_view();
                let loan: Loan = Arc::new(Mutex::new(None));
                self.lent = Some(Lending {
                    loan: Arc::clone(&loan),
                    reply,
                    reopens: Vec::new(),
                    views: None,
                    then,
                });
                self.said.push_back(Said::HandedOff(Lent {
                    command: Box::new(command),
                    loan,
                    told: false,
                }));
            }
        }
    }
}
