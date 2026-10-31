//! `ViewControl`: how the program that runs `serve` closes the view it shows, or shows another in
//! its place, without ending (specification §10.8).
//!
//! `serve` returns when the view has closed, which is what a program that is only a view wants. A
//! program that goes on after its view — a tool that shows a text interface for a while, or one
//! whose author reloads the view as the files change — makes a control, gives it to `serve` and
//! calls it from another thread while `serve` runs:
//!
//! ```no_run
//! use pencere::{Bundle, Options, View, ViewControl};
//!
//! # fn main() -> Result<(), pencere::Error> {
//! let control = ViewControl::new();
//! let page = Bundle::from_dir("view", "index.html")?;
//! let reloader = control.clone();
//! std::thread::spawn(move || {
//!     // ... later, when the files changed:
//!     if let Ok(page) = Bundle::from_dir("view", "index.html") {
//!         let _ = reloader.reopen(Some(vec![View::web(page)]));
//!     }
//! });
//! pencere::serve(Options::new().view(View::web(page)).control(control))?;
//! # Ok(())
//! # }
//! ```
//!
//! A program that runs another program in its pane — a shell the person asked for, `$EDITOR` on a
//! file — lends it the terminal with [`ViewControl::hand_off`]: the view closes, the command has the
//! terminal as if a shell had started it, and when it ends the view is shown again, `serve` going on
//! as if nothing had happened.
//!
//! The three need a terminal that lists `dismiss` in its `welcome`; with another the view is not
//! touched and the answer says so.

use std::cell::{Cell, RefCell};
use std::io;
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};

use pencere_wire::bundle::Digest;

use crate::files::{FileError, Transferring};
use crate::handlers::{Outbox, Work};
use crate::payload::StageOffer;
use crate::{Error, NoView, Options, View};

/// What `ViewControl::reopen` came to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReopenResult {
    /// The old view is closed and the terminal has taken the open of the new one.
    Reopened,
    /// The terminal does not list `dismiss`. Nothing was sent and the view is as it was.
    Unsupported,
    /// There is no view to close: `serve` has not shown one yet, or has ended, or the view is
    /// already being closed or replaced; or someone else closed it (the person, the view) while
    /// this was under way. Nothing was sent, or what was sent no longer matters.
    Unavailable,
    /// The old view closed and no new one could be opened: the terminal did not answer in time
    /// ([`crate::NoViewReason::Timeout`]), refused or ended the open before the view ran. `serve`
    /// ends the same way and runs its fallback, as it does when the first open fails.
    NoView(NoView),
}

/// What `ViewControl::close` came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CloseResult {
    /// The view is closed; `serve` returns [`crate::EndedBy::Program`], with the terminal given back
    /// first, so that the program can show its own interface.
    Closed,
    /// The terminal does not list `dismiss`. Nothing was sent and the view is as it was.
    Unsupported,
    /// As for [`ReopenResult::Unavailable`]: there is no view to close.
    Unavailable,
}

/// What `ViewControl::hand_off` came to: [`HandOff::wait`].
#[derive(Debug)]
#[non_exhaustive]
pub enum HandOffResult {
    /// The command had the terminal and has ended, and the terminal has taken the open of the view
    /// shown again; or no view was up, and the command had the terminal of a program that was not
    /// using it. How the command ended, or why it could not be started (the view is shown again
    /// then too).
    Returned(io::Result<ExitStatus>),
    /// The view could not be shown again: the terminal did not answer in time, refused or ended the
    /// open, a signal asked the program to end while the command had the terminal, or the terminal
    /// did not close the view in time and the command was never given it (`status` is `None`).
    /// `serve` ends with the same `no_view`, and runs its fallback unless a signal ended it, as it
    /// does when the first open fails.
    NoView {
        /// How the command ended, or why it could not be started; `None` when it did not run.
        status: Option<io::Result<ExitStatus>>,
        /// Why there is no view.
        no_view: NoView,
    },
    /// The terminal does not list `dismiss`. Nothing was sent, the command did not run and the
    /// view is as it was.
    Unsupported,
    /// There is no view to close: `serve` has not shown one yet, or the view is being closed or
    /// replaced; or another `serve`, which this control is not given to, has the terminal. Nothing
    /// was done.
    Unavailable,
    /// The terminal is lent to a command already. Nothing was done.
    Busy,
}

/// A hand-off of the terminal that is under way: [`ViewControl::hand_off`]. Dropping it does not
/// stop the hand-off; [`HandOff::wait`] waits for it to be over.
#[derive(Debug)]
pub struct HandOff {
    answer: Receiver<HandOffResult>,
    /// The number of the hand-off when it waits, on this thread, for the handler that asked for it
    /// to answer.
    parked: Option<u64>,
}

impl HandOff {
    /// A hand-off that came to `result` at once.
    fn over(result: HandOffResult) -> Self {
        let (reply, answer) = sync_channel(1);
        let _ = reply.send(result);
        Self {
            answer,
            parked: None,
        }
    }

    /// Waits until the hand-off is over: the command has ended and the view is shown again, or why
    /// it is not. Called by the handler that asked for the hand-off, before it has answered, it
    /// does not wait for that answer: the hand-off starts at once, and the answer is the old view's,
    /// lost with it.
    pub fn wait(self) -> HandOffResult {
        if let Some(ticket) = self.parked {
            let parked = PARKED.with(|slot| {
                let mut slot = slot.borrow_mut();
                if slot.as_ref().is_some_and(|parked| parked.ticket == ticket) {
                    slot.take()
                } else {
                    None
                }
            });
            if let Some(parked) = parked {
                parked.send();
            }
        }
        // A loop that ends before it answers drops the request, and with it the way to answer.
        self.answer.recv().unwrap_or(HandOffResult::Unavailable)
    }
}

/// What the loop is asked.
pub(crate) enum Request {
    Reopen {
        views: Option<Vec<View>>,
        reply: SyncSender<ReopenResult>,
    },
    Close {
        reply: SyncSender<CloseResult>,
    },
    HandOff {
        command: Command,
        reply: SyncSender<HandOffResult>,
        then: Option<OnReturn>,
    },
    /// Files for the view to draw (§8.8), answered when the terminal holds them.
    Attach {
        files: Vec<(Digest, Arc<[u8]>)>,
        reply: SyncSender<Result<(), FileError>>,
    },
    /// Attached files the view lets go of.
    Detach {
        digests: Vec<Digest>,
    },
    /// A piece of a file the person gave (§8.9).
    Read {
        handle: String,
        offset: u64,
        length: u64,
        reply: SyncSender<Result<Vec<u8>, FileError>>,
    },
    /// A piece of a file for a place to save in (§8.10).
    Write {
        handle: String,
        offset: u64,
        bytes: Vec<u8>,
        end: bool,
        reply: SyncSender<Result<(), FileError>>,
    },
    /// The folder the terminal offers on the program's machine, if it does and a view that has
    /// files runs (§8.11).
    StageOffer {
        reply: SyncSender<Result<Option<StageOffer>, FileError>>,
    },
    /// The file of a handle, through a pipe (§8.11); what comes of it is told on `events`.
    Stage {
        handle: String,
        offset: u64,
        events: Sender<Transferring>,
    },
    /// A file for the place of a handle, through a pipe (§8.11).
    Fetch {
        handle: String,
        size: u64,
        offset: u64,
        events: Sender<Transferring>,
    },
    /// A `stage` or a `fetch` the program takes back, by the id it was sent with.
    Unstage {
        id: u32,
    },
}

/// What the program does with how a command it lent the terminal to ended, before the view is
/// shown again ([`ViewControl::hand_off_then`]).
pub(crate) type OnReturn = Box<dyn FnOnce(&io::Result<ExitStatus>) + Send>;

/// What is left of `then`, once: the thread that takes it first has it.
fn taken(then: &Mutex<Option<OnReturn>>) -> Option<OnReturn> {
    then.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
}

/// Gives `then` how the command ended. A panic in it is the program's to say, not the loop's to die
/// of: it is caught, and the view is shown again all the same.
pub(crate) fn tell_then(then: Option<OnReturn>, ended: &io::Result<ExitStatus>) {
    if let Some(then) = then {
        let _ = crate::handlers::as_a_handler(|| then(ended));
    }
}

/// A hand-off asked for by the handler of a call or a subscription, held until its answer is out.
struct Parked {
    ticket: u64,
    outbox: Outbox,
    request: Request,
}

impl Parked {
    fn send(self) {
        // A loop that is gone drops the request, and the hand-off is told `Unavailable`.
        self.outbox.send(Work::Control(self.request));
    }
}

thread_local! {
    /// This thread runs the handler of a call or a subscription.
    static ANSWERING: Cell<bool> = const { Cell::new(false) };
    /// The hand-off the handler that runs on this thread asked for.
    static PARKED: RefCell<Option<Parked>> = const { RefCell::new(None) };
}

/// The numbers of the hand-offs.
static TICKETS: AtomicU64 = AtomicU64::new(1);

/// A command has the terminal of a program that had no view up.
static LENT: AtomicBool = AtomicBool::new(false);

/// While this lives, the thread runs a handler: a hand-off it asks for is held until its answer is
/// out ([`answered`]).
pub(crate) struct Answering(());

impl Answering {
    pub(crate) fn start() -> Self {
        ANSWERING.with(|on| on.set(true));
        Self(())
    }
}

impl Drop for Answering {
    fn drop(&mut self) {
        ANSWERING.with(|on| on.set(false));
    }
}

/// The handler that ran on this thread has handed its answer to the loop: the hand-off it asked for
/// follows it, so that the answer reaches the view before the view is closed.
pub(crate) fn answered() {
    if let Some(parked) = PARKED.with(|slot| slot.borrow_mut().take()) {
        parked.send();
    }
}

/// The program's hand on the view `serve` shows. Give it to [`Options::control`]; until `serve`
/// has a view, and once it has ended, `reopen`, `close` and `hand_off` answer `Unavailable` —
/// except that `hand_off` runs its command, with no view to close, when no `serve` runs at all
/// (before `serve` is called, after it returns, in its fallback). It can be cloned and sent to the
/// threads that call it.
#[derive(Clone, Default)]
pub struct ViewControl {
    bound: Arc<Mutex<Option<(u64, Outbox)>>>,
}

/// The numbers the loops a control is tied to are told apart by.
static GENERATIONS: AtomicU64 = AtomicU64::new(1);

impl std::fmt::Debug for ViewControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewControl").finish_non_exhaustive()
    }
}

/// The control is tied to the loop of a `serve` while this lives.
pub(crate) struct Bound {
    bound: Arc<Mutex<Option<(u64, Outbox)>>>,
    generation: u64,
}

impl Drop for Bound {
    fn drop(&mut self) {
        // Only its own: a later `serve` may have taken the control while this one wound up.
        if let Ok(mut slot) = self.bound.lock()
            && slot
                .as_ref()
                .is_some_and(|(held, _)| *held == self.generation)
        {
            *slot = None;
        }
    }
}

impl ViewControl {
    /// A control that is tied to no `serve` yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `serve` ties the control to the loop it runs. A control belongs to one `serve` at a time;
    /// one that takes it later is the one that has it.
    pub(crate) fn bind(&self, outbox: Outbox) -> Bound {
        let generation = GENERATIONS.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut slot) = self.bound.lock() {
            *slot = Some((generation, outbox));
        }
        Bound {
            bound: Arc::clone(&self.bound),
            generation,
        }
    }

    pub(crate) fn outbox(&self) -> Option<Outbox> {
        self.bound
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(|(_, outbox)| outbox.clone()))
    }

    /// Closes the view and opens one again: with the same files (the terminal keeps what it
    /// verified, so what did not change does not cross the stream a second time) when `next` is
    /// `None`, or with the kinds of view `next` lists, which are what the new `hello` offers from
    /// now on. Waits as long as the terminal takes to tear the old view down, which the terminal
    /// bounds and [`Options::reopen_timeout`] bounds from this side. What the old view had asked
    /// of the program is over with it, as for any view.
    ///
    /// # Errors
    ///
    /// When `next` is not what a program may offer ([`Error::Options`], [`Error::Bundle`]): the
    /// view is as it was.
    pub fn reopen(&self, next: Option<Vec<View>>) -> Result<ReopenResult, Error> {
        if let Some(views) = &next {
            Options::check_views(views)?;
        }
        let (reply, answer) = sync_channel(1);
        let Some(outbox) = self.outbox() else {
            return Ok(ReopenResult::Unavailable);
        };
        if !outbox.send(Work::Control(Request::Reopen { views: next, reply })) {
            return Ok(ReopenResult::Unavailable);
        }
        // A loop that ends before it answers drops the request, and with it the way to answer.
        Ok(answer.recv().unwrap_or(ReopenResult::Unavailable))
    }

    /// Closes the view and ends `serve` with [`crate::EndedBy::Program`], giving the terminal back
    /// first, so that the program can show its own interface.
    pub fn close(&self) -> CloseResult {
        let (reply, answer) = sync_channel(1);
        let Some(outbox) = self.outbox() else {
            return CloseResult::Unavailable;
        };
        if !outbox.send(Work::Control(Request::Close { reply })) {
            return CloseResult::Unavailable;
        }
        answer.recv().unwrap_or(CloseResult::Unavailable)
    }

    /// Lends the terminal to `command` — a shell, an editor — and takes it back when it ends, the
    /// way a shell runs a command: the view is closed and the terminal given back (the screen, the
    /// modes), the command runs as the terminal's foreground job, in a process group of its own,
    /// with ⌃C, ⌃Z and the tty its own, and when it ends the view is shown again with the same
    /// options, `serve` going on. What the old view had asked is over with it, as for any view; the
    /// new view starts afresh and subscribes again.
    ///
    /// What was on its way to the view goes out before it closes. Called on the thread of a
    /// handler, the hand-off waits for the handler to return, so that its answer goes out too: a
    /// handler asks for the hand-off, answers, and the view closes after the answer. A reopening
    /// asked while the command has the terminal ([`ViewControl::reopen`] with other views, as when
    /// the files of the view change) is the view that comes back.
    ///
    /// When no `serve` runs — before it is called, after it returns, in its fallback — the command
    /// runs in the foreground of the program's controlling terminal all the same, and there is no
    /// view to show again; a `serve` started meanwhile is told the terminal is taken
    /// ([`Error::Busy`]). While a `serve` is asking for its first view there is no view to close
    /// yet, and the answer is `Unavailable`. A signal that asks the program to end while the command has the terminal is acted on
    /// when the command has ended (a hangup is passed on to it, as a shell passes its own on to its
    /// jobs): `serve` then ends as it does for the signal, with no view shown again.
    ///
    /// The command's standard streams are what `command` says: by default the program's own, which
    /// are the terminal unless the program is a filter in a pipeline.
    pub fn hand_off(&self, command: Command) -> HandOff {
        self.lend(command, None)
    }

    /// Lends the terminal to `command` as [`ViewControl::hand_off`] does, and gives `then` how it
    /// ended (or why it could not be started) once it has, **before the view is shown again**: on
    /// the library's thread, as the terminal is taken back, so that what the program keeps of it
    /// is there for the first call of the view that comes back — a view that asks at once how the
    /// shell it lent the pane to ended finds the answer. `then` is not called where the command
    /// did not run (the hand-off is `Unsupported`, `Unavailable` or `Busy`, or the terminal did
    /// not close the view); it should be brief (keep a value, send it on a channel), and a panic
    /// in it is caught. The [`HandOff`] says how the hand-off went, as `hand_off`'s does.
    pub fn hand_off_then(
        &self,
        command: Command,
        then: impl FnOnce(&io::Result<ExitStatus>) + Send + 'static,
    ) -> HandOff {
        self.lend(command, Some(Box::new(then)))
    }

    fn lend(&self, command: Command, then: Option<OnReturn>) -> HandOff {
        let Some(outbox) = self.outbox() else {
            return Self::run_with_no_view(command, then);
        };
        let (reply, answer) = sync_channel(1);
        let request = Request::HandOff {
            command,
            reply,
            then,
        };
        if ANSWERING.with(Cell::get) {
            // The handler's answer is not out yet: the request follows it ([`answered`]).
            let ticket = TICKETS.fetch_add(1, Ordering::SeqCst);
            let busy = PARKED.with(|slot| {
                let mut slot = slot.borrow_mut();
                if slot.is_some() {
                    return true;
                }
                *slot = Some(Parked {
                    ticket,
                    outbox,
                    request,
                });
                false
            });
            if busy {
                return HandOff::over(HandOffResult::Busy);
            }
            return HandOff {
                answer,
                parked: Some(ticket),
            };
        }
        // A loop that is gone drops the request, and with it the way to answer.
        outbox.send(Work::Control(request));
        HandOff {
            answer,
            parked: None,
        }
    }

    /// A hand-off when this control is given to no `serve` that runs: the terminal is the
    /// program's, unless another `serve` has it.
    fn run_with_no_view(command: Command, then: Option<OnReturn>) -> HandOff {
        if LENT.swap(true, Ordering::SeqCst) {
            return HandOff::over(HandOffResult::Busy);
        }
        // The terminal is the command's until it ends: a `serve` started meanwhile is told so.
        if !crate::terminal::claim() {
            LENT.store(false, Ordering::SeqCst);
            return HandOff::over(HandOffResult::Unavailable);
        }
        let given_back = || {
            crate::terminal::release();
            LENT.store(false, Ordering::SeqCst);
        };
        let (reply, answer) = sync_channel(1);
        // Held where the thread that waits takes it, or where this one does when there is no
        // such thread: the command could not be started either way.
        let then = Arc::new(Mutex::new(then));
        let waited = Arc::clone(&then);
        let started = std::thread::Builder::new()
            .name("pencere-hand-off".into())
            .spawn(move || {
                let ended = crate::foreground::run(command);
                given_back();
                tell_then(taken(&waited), &ended);
                let _ = reply.send(HandOffResult::Returned(ended));
            });
        match started {
            Ok(_) => HandOff {
                answer,
                parked: None,
            },
            Err(why) => {
                given_back();
                let ended = Err(why);
                tell_then(taken(&then), &ended);
                HandOff::over(HandOffResult::Returned(ended))
            }
        }
    }
}
