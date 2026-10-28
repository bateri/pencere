//! `pencere`: show a view in the pane of the terminal your program runs in.
//!
//! A program that runs in a terminal that hosts pencere can ask it for a view — a web page, a
//! WebAssembly component or a Lua script the terminal draws itself — that takes the pane the way
//! `vim` takes it, and gives it back when it is done. This crate is the program's side of the
//! protocol: it asks the terminal what it can show, opens the view, sends the files the terminal
//! lacks, answers the calls of the view and feeds its subscriptions, and, on the way out, leaves
//! the terminal as it found it. No async runtime: the view's handlers run on threads of their
//! own and the crate's event loop is the thread that called [`serve`].
//!
//! ```no_run
//! use pencere::{Bundle, Options, Outcome, View, ViewError};
//! use serde_json::json;
//!
//! # fn main() -> Result<(), pencere::Error> {
//! let page = Bundle::from_dir("view", "index.html")?;
//! let outcome = pencere::serve(
//!     Options::new()
//!         .view(View::web(page))
//!         .program("containers")
//!         .title("Containers")
//!         .call("list", |_params, _call| Ok(json!({ "containers": [] })))
//!         .call("stop", |params, _call| match params["id"].as_str() {
//!             Some(id) => Ok(json!({ "stopped": id })),
//!             None => Err(ViewError::with_message("bad-params", "which container?")),
//!         })
//!         // Runs when no view is shown, after the terminal is given back.
//!         .fallback(|_why| println!("no containers to show")),
//! )?;
//! if let Outcome::Closed(closed) = outcome {
//!     println!("the view closed ({:?})", closed.by);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! What the crate does so that a program does not have to (specification §4.9 to §10.4):
//!
//! * the **handshake**: a `hello` and the fence `CSI c`, an answer within two seconds or no
//!   host, and a late answer to the fence read and thrown away so that it does not reach the
//!   shell as typed text;
//! * the **modes** of the tty: non-canonical, echo off, signals on, the input read without ever
//!   waiting; given back on every path out, a panic included;
//! * the **signals**: ⌃C, `SIGTERM`, `SIGHUP` and ⌃\ end the view with a `bye`, wait up to half a
//!   second for the terminal's `closed`, throw away what is left unread and give the terminal
//!   back; ⌃Z gives the terminal back, stops the program, and takes it again when it continues;
//!   once the terminal has hung up, `SIGHUP` stays ignored after `serve` returns, so that the
//!   kernel's signal for the same hangup does not end the program before what it does next;
//! * the **alternate screen**, entered before the `open` and left on the way out;
//! * the **writes**: one thread writes, a few bytes at a time while the tty has room, so a
//!   terminal that is not reading cannot keep the program from reading or from leaving.
//!
//! The program must not print while the view is up: the terminal is the view's.
//!
//! A program that goes on after its view, or reloads the view as its files change, closes it and
//! shows another with a [`ViewControl`] (§10.8); one that runs a shell or an editor in its pane
//! lends it the terminal with [`ViewControl::hand_off`], and its view comes back when that ends.
//!
//! The same handlers are served with no terminal by a [`Conversation`]: for the program's tests,
//! and for a host that runs the view in the same process.

#![deny(unsafe_code)]
#![warn(missing_docs)]

/// The README's example is compiled with the crate's other documentation.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeExamples;

mod app;
mod bundle;
mod control;
mod conversation;
mod exchange;
#[allow(unsafe_code)]
mod foreground;
mod handlers;
mod link;
mod payload;
mod session;
#[allow(unsafe_code)]
mod terminal;
mod view;

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

pub use app::{App, DEV_VARIABLE, build};
pub use bundle::{Bundle, BundleError, File};
pub use control::{CloseResult, HandOff, HandOffResult, ReopenResult, ViewControl};
pub use conversation::{Conversation, Lent, Said};
pub use handlers::{Call, Subscription, ViewError};
pub use terminal::Terminal;
pub use view::{Access, Icon, IconTheme, Kind, Permission, View, WEB_VERSION};

use handlers::{CallFn, Method, TopicFn};
use view::Source;

/// What the program does when no view is shown.
type Fallback = Box<dyn FnOnce(&NoView) + Send>;

/// What `serve` is asked.
pub struct Options {
    views: Vec<View>,
    app: Option<App>,
    program: Option<String>,
    title: Option<String>,
    icons: Vec<Icon>,
    calls: HashMap<String, Method>,
    topics: HashMap<String, Arc<TopicFn>>,
    fallback: Option<Fallback>,
    terminal: Option<Terminal>,
    control: Option<ViewControl>,
    answer_timeout: Duration,
    late_reply_grace: Duration,
    bye_timeout: Duration,
    reopen_timeout: Duration,
}

impl fmt::Debug for Options {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut calls: Vec<&String> = self.calls.keys().collect();
        calls.sort();
        let mut topics: Vec<&String> = self.topics.keys().collect();
        topics.sort();
        f.debug_struct("Options")
            .field("views", &self.views)
            .field("program", &self.program)
            .field("title", &self.title)
            .field("calls", &calls)
            .field("topics", &topics)
            .finish_non_exhaustive()
    }
}

impl Default for Options {
    fn default() -> Self {
        Self::new()
    }
}

impl Options {
    /// Options with no view yet: [`Options::view`] adds the kinds the program can show.
    #[must_use]
    pub fn new() -> Self {
        Self {
            views: Vec::new(),
            app: None,
            program: None,
            title: None,
            icons: Vec::new(),
            calls: HashMap::new(),
            topics: HashMap::new(),
            fallback: None,
            terminal: None,
            control: None,
            answer_timeout: Duration::from_secs(2),
            late_reply_grace: Duration::from_secs(1),
            bye_timeout: Duration::from_millis(500),
            reopen_timeout: Duration::from_secs(10),
        }
    }

    /// A kind of view the program can show. The `hello` lists them in the order they are added,
    /// and the first kind the terminal will open, at the first of its versions the terminal
    /// speaks, is the view (specification §5.3). At least one is needed.
    #[must_use]
    pub fn view(mut self, view: View) -> Self {
        self.views.push(view);
        self
    }

    /// The views of the app's files ([`App::views`]): its page of the kit, then its web page, after
    /// the views [`Options::view`] adds. Under `pencere dev`, where the app is the project's files
    /// on the disk, the view is shown again whenever one of them changes, without ending the
    /// program (with the [`ViewControl`] of [`Options::control`], or one of its own).
    #[must_use]
    pub fn app(mut self, app: App) -> Self {
        self.app = Some(app);
        self
    }

    /// What the program calls itself: the terminal names its pane with it (§7.3).
    #[must_use]
    pub fn program(mut self, name: impl Into<String>) -> Self {
        self.program = Some(name.into());
        self
    }

    /// What the program calls the view.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// An icon for the program (§7.3).
    #[must_use]
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icons.push(icon);
        self
    }

    /// Answers the calls of the view to `method`. The handler is given the params of the call
    /// (`null` when it has none) and returns the result, or a [`ViewError`] the view gets as the
    /// error of the call; a result that serialises to `null` is `{}`, since a result is never
    /// `null`. A method that has no handler is answered `not-found`.
    ///
    /// The handler runs on a thread of its own, so it may take its time; it should look at
    /// [`Call::is_cancelled`] now and then, and may report its progress with [`Call::progress`].
    /// The calls of the view run at the same time, so which of two that come together is answered
    /// first is not defined; [`Options::call_in`] puts the calls that must not overtake one another
    /// in a lane.
    #[must_use]
    pub fn call<T, F>(mut self, method: impl Into<String>, handler: F) -> Self
    where
        T: Serialize,
        F: Fn(Value, &Call) -> Result<T, ViewError> + Send + Sync + 'static,
    {
        self.calls.insert(
            method.into(),
            Method {
                handler: Self::handler(handler),
                lane: None,
            },
        );
        self
    }

    /// Answers the calls of the view to `method` as [`Options::call`] does, in the lane `lane`: the
    /// calls of one lane run one at a time, in the order the view sent them, each once the one
    /// before it has been answered, on a thread of the lane's. Calls of other lanes, and those of
    /// [`Options::call`], go on meanwhile. The lane is the program's word for what has to stay in
    /// order: the edits of a document whose versions count up, the changes a language server is
    /// told of.
    ///
    /// A call the view cancels before its turn does not run, and is not answered (§9.2: the view
    /// ignores an answer after its `cancel`). A handler that panics is an `internal` error of its
    /// call, and the lane goes on with the next.
    #[must_use]
    pub fn call_in<T, F>(
        mut self,
        lane: impl Into<String>,
        method: impl Into<String>,
        handler: F,
    ) -> Self
    where
        T: Serialize,
        F: Fn(Value, &Call) -> Result<T, ViewError> + Send + Sync + 'static,
    {
        self.calls.insert(
            method.into(),
            Method {
                handler: Self::handler(handler),
                lane: Some(lane.into().into()),
            },
        );
        self
    }

    /// A handler of the program's, as `serve` holds it: what it returns made JSON.
    fn handler<T, F>(handler: F) -> Arc<CallFn>
    where
        T: Serialize,
        F: Fn(Value, &Call) -> Result<T, ViewError> + Send + Sync + 'static,
    {
        Arc::new(move |params: Value, call: &Call| {
            let result = handler(params, call)?;
            serde_json::to_value(result).map_err(|why| {
                ViewError::with_message("internal", format!("the result is not JSON: {why}"))
            })
        })
    }

    /// Feeds the subscriptions of the view to `topic`. The handler is given the params of the
    /// subscription and the [`Subscription`] to push to, and returns once it has started feeding
    /// it: usually it starts a thread that calls [`Subscription::push`] until
    /// [`Subscription::is_cancelled`], and, if there is something to stop, registers it with
    /// [`Subscription::on_end`]. Returning an error ends the subscription with an error
    /// response. A topic that has no handler is answered `not-found`.
    #[must_use]
    pub fn topic<F>(mut self, topic: impl Into<String>, handler: F) -> Self
    where
        F: Fn(Value, &Subscription) -> Result<(), ViewError> + Send + Sync + 'static,
    {
        self.topics.insert(topic.into(), Arc::new(handler));
        self
    }

    /// What the program does when no view is shown: no tty, no terminal answered, it shares no
    /// version or no kind, or it refused or ended the open before the view ran. It runs after
    /// the terminal has been given back, and is told why. It does not run when a signal ended the
    /// wait.
    #[must_use]
    pub fn fallback(mut self, fallback: impl FnOnce(&NoView) + Send + 'static) -> Self {
        self.fallback = Some(Box::new(fallback));
        self
    }

    /// The terminal to show the view on, when it is not the program's standard input and output:
    /// a filter in a pipeline, whose standard streams carry its data, asks for
    /// [`Terminal::controlling`] (specification §13). A program must decide this explicitly (with
    /// a flag of its own) and not guess it from its standard streams.
    #[must_use]
    pub fn terminal(mut self, terminal: Terminal) -> Self {
        self.terminal = Some(terminal);
        self
    }

    /// How long to wait for the terminal's answer to the `hello` (§5.3). Two seconds by default.
    #[must_use]
    pub fn answer_timeout(mut self, timeout: Duration) -> Self {
        self.answer_timeout = timeout;
        self
    }

    /// After an answer that did not come in time: how long to go on reading, so that a reply to
    /// `CSI c` that is only late does not reach the shell as typed text (§5.3). One second by
    /// default.
    #[must_use]
    pub fn late_reply_grace(mut self, grace: Duration) -> Self {
        self.late_reply_grace = grace;
        self
    }

    /// After a `bye`: how long to go on reading, and throwing away what is read, for the `closed`
    /// that answers it, before the unread rest of the input is thrown away too and the terminal
    /// is given back (§10.3). Half a second by default.
    #[must_use]
    pub fn bye_timeout(mut self, timeout: Duration) -> Self {
        self.bye_timeout = timeout;
        self
    }

    /// The hand the program closes the view with, shows another in its place, or lends the
    /// terminal to a command with, while `serve` runs (§10.8).
    #[must_use]
    pub fn control(mut self, control: ViewControl) -> Self {
        self.control = Some(control);
        self
    }

    /// How long a reopening, a closing or a hand-off ([`ViewControl`]) waits on the terminal for
    /// each of its answers: the `closed` of the `dismiss`, the `welcome` of the new `hello`, and the
    /// `need` or `closed` of the new `open`. Ten seconds by default; a terminal that does not answer in time
    /// is given up on ([`NoViewReason::Timeout`]), with a `bye` for the view it may still show.
    #[must_use]
    pub fn reopen_timeout(mut self, timeout: Duration) -> Self {
        self.reopen_timeout = timeout;
        self
    }

    /// Checks what the program offers.
    fn check(&self) -> Result<(), Error> {
        Self::check_views(&self.views)
    }

    /// Checks the kinds of view a program offers, as listed in a `hello`.
    pub(crate) fn check_views(views: &[View]) -> Result<(), Error> {
        if views.is_empty() {
            return Err(Error::Options(
                "a program offers at least one kind of view".into(),
            ));
        }
        for (at, view) in views.iter().enumerate() {
            let name = format!("view {at} ({})", view.kind);
            if views.iter().take(at).any(|other| other.kind == view.kind) {
                return Err(Error::Options(format!("{name}: the kind is listed twice")));
            }
            if view.versions.is_empty() {
                return Err(Error::Options(format!(
                    "{name}: a view names the versions of its interface"
                )));
            }
            for (n, version) in view.versions.iter().enumerate() {
                if !view::is_version(version) {
                    return Err(Error::Options(format!(
                        "{name}: {version:?} is not a version (major.minor)"
                    )));
                }
                if view.versions.iter().take(n).any(|other| other == version) {
                    return Err(Error::Options(format!(
                        "{name}: the version {version} is listed twice"
                    )));
                }
            }
            if view.access.is_some() && view.kind != Kind::Web {
                return Err(Error::Options(format!(
                    "{name}: only a web view has an access to ask for"
                )));
            }
            if let Source::Fixed(bundle) = &view.source {
                session::check_entry(view.kind, bundle).map_err(Error::Bundle)?;
            }
        }
        Ok(())
    }
}

/// What can go wrong with `serve` itself, as opposed to there being no view to show.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// What the program offers is not something a program may offer.
    Options(String),
    /// The files of a view are not a bundle that suits its kind.
    Bundle(BundleError),
    /// Another `serve` has the terminal, or a command it was lent to with no `serve` running
    /// ([`ViewControl::hand_off`]): a process has one terminal to show a view on.
    Busy,
    /// The terminal failed in a way that is nobody's to give up on.
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Options(why) => f.write_str(why),
            Self::Bundle(why) => write!(f, "the files of a view: {why}"),
            Self::Busy => f.write_str("another view of this process has the terminal"),
            Self::Io(why) => write!(f, "the terminal: {why}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bundle(why) => Some(why),
            Self::Io(why) => Some(why),
            _ => None,
        }
    }
}

impl From<BundleError> for Error {
    fn from(why: BundleError) -> Self {
        Self::Bundle(why)
    }
}

impl From<io::Error> for Error {
    fn from(why: io::Error) -> Self {
        Self::Io(why)
    }
}

/// How `serve` ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A view ran and has closed. The terminal is given back.
    Closed(Closed),
    /// No view was shown. The terminal is as it was found, and the fallback ran.
    NoView(NoView),
}

impl Outcome {
    /// The status a shell would report for the program: 128 and the number of the signal if one
    /// ended the view, 0 otherwise.
    #[must_use]
    pub fn shell_status(&self) -> u8 {
        let signal = match self {
            Self::Closed(Closed {
                by: EndedBy::Signal(signal),
                ..
            })
            | Self::NoView(NoView {
                reason: NoViewReason::Interrupted(signal),
                ..
            }) => *signal,
            _ => return 0,
        };
        u8::try_from(128 + signal).unwrap_or(1)
    }
}

/// A view that ran and has closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closed {
    /// What ended it.
    pub by: EndedBy,
    /// The view that ran.
    pub view: Shown,
    /// With [`EndedBy::Error`]: the `code` of the terminal's `closed`.
    pub code: Option<String>,
    /// With [`EndedBy::Error`]: the words the terminal gave, which for a view it draws itself
    /// are its error and the line (§9.8).
    pub message: Option<String>,
}

/// What ended a view that ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EndedBy {
    /// The person closed it (§10.5).
    User,
    /// The view asked to be closed (§10.6).
    View,
    /// The terminal says the program closed it (§10.8).
    Program,
    /// The view failed after it ran, or the terminal ended it with an error.
    Error,
    /// A signal (its number) ended the program's use of the view: ⌃C is `SIGINT`, 2. The program
    /// said `bye` and the terminal is given back; what the program does next is its own.
    Signal(i32),
    /// The terminal went away.
    Terminal,
}

/// The kind of view that was shown, and the version of its interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// The kind.
    pub kind: Kind,
    /// The version of its interface.
    pub version: String,
}

/// No view was shown, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoView {
    /// Why.
    pub reason: NoViewReason,
    /// The `code` of the `closed` of a refusal or a failure (§6.4).
    pub code: Option<String>,
    /// The words of the `closed`, for people (§9.8). For [`NoViewReason::NoCommonView`], when the
    /// terminal opens a kind of the program's in other versions, the sentence that names both
    /// sides (§5.2); [`serve`] prints it before the fallback runs.
    pub message: Option<String>,
}

/// Why no view was shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NoViewReason {
    /// The input or the output is not a terminal, a filter has none, or its modes cannot be set:
    /// there is no pane to ask in (§13).
    NoTty,
    /// Nothing answered the `hello` in time, or the terminal answered `CSI c` first (§5.3).
    NoHost,
    /// A terminal answered and shares no protocol version with this library.
    NoCommonVersion,
    /// A terminal answered and will open none of the kinds the program offered, at a version
    /// both speak (§5.2).
    NoCommonView,
    /// The terminal refused the `open`. The `code` says why when it does.
    Refused,
    /// The terminal ended the open before the view ran (a bundle that did not verify, files that
    /// did not come in time).
    Failed,
    /// A reopening, a closing or a hand-off ([`ViewControl`]) waited for the terminal's answer to its
    /// `dismiss`, its new `hello` or its new `open` longer than [`Options::reopen_timeout`].
    Timeout,
    /// A signal (its number) ended the wait, or came while the terminal was lent to a command
    /// ([`ViewControl::hand_off`]), so that no view was shown again.
    Interrupted(i32),
}

/// Shows the view of the program: see the crate's description. Returns when the view has closed
/// or when there is no view to show (after the fallback ran), with the terminal as it was
/// found.
///
/// A signal that ends the program while the view is up (⌃C reaches the program as `SIGINT`,
/// §10.4) ends the view, and `serve` returns [`EndedBy::Signal`]: the program decides what comes
/// next, and [`Outcome::shell_status`] is what a shell would call it.
///
/// # Errors
///
/// When what the program offers is not what a program may offer ([`Error::Options`],
/// [`Error::Bundle`]), when another `serve` has the terminal ([`Error::Busy`]), or when the
/// terminal fails in a way that is not the end of the view ([`Error::Io`]).
pub fn serve(mut options: Options) -> Result<Outcome, Error> {
    let app = options.app.take();
    if let Some(app) = &app {
        options.views.extend(app.views()?);
    }
    options.check()?;
    // An app read from the disk is watched while the view is up, and shown again when it changes.
    let watching = match &app {
        Some(app) if app.source_dir().is_some() => {
            let control = options.control.get_or_insert_with(ViewControl::new).clone();
            Some(app::watch(app.clone(), control))
        }
        _ => None,
    };
    let terminal = options.terminal.take().unwrap_or_else(Terminal::standard);
    let fallback = options.fallback.take();
    let outcome = session::run(&options, &terminal);
    drop(watching);
    drop(terminal);
    let outcome = outcome?;
    if let Outcome::NoView(no_view) = &outcome
        && !matches!(no_view.reason, NoViewReason::Interrupted(_))
    {
        // A terminal that opens the program's kinds, in other versions: the person is told what
        // would serve (the terminal is the program's own again, so this is the program's to say).
        if no_view.reason == NoViewReason::NoCommonView
            && let Some(words) = &no_view.message
        {
            eprintln!("pencere: {words}");
        }
        if let Some(fallback) = fallback {
            fallback(no_view);
        }
    }
    Ok(outcome)
}
