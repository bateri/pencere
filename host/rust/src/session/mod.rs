//! The host's side of one pane's conversation with one program (specification
//! §5 to §10), as a state machine with no I/O.
//!
//! A [`Session`] belongs to one pane. The terminal feeds it what it learns:
//! the bytes or the APC strings the program wrote ([`Session::feed`],
//! [`Session::apc_put`] and its siblings), what the view says
//! ([`Session::view_message`] for a page, [`Session::module_message`] for a view
//! the terminal draws itself), what the person answers, what the terminal
//! observes of the pane ([`Session::observe`]), and when a wait it chose has run
//! out ([`Session::timed_out`]). The session answers with [`Action`]s, in the
//! order they must happen: bytes for the program's input, a view to start, a
//! message for the view, a question for the person. Whatever a frame made the
//! session queue is done before the terminal answers anything that follows that
//! frame in the stream: the `welcome` goes into the pane's input before the reply
//! to the `CSI c` behind the `hello` (specification §5.2).
//!
//! The session does no rate limiting, because it has no clock: a terminal that
//! wants to bound how often a pane may ask (§14) drops frames before it hands
//! them over.
//!
//! Everything the session decides is in the specification: who may open a view
//! (a single-use token, §6), which `open` is refused and with what code (§6.4),
//! which files of a bundle are missing and whether what arrived is what was
//! asked for (§8), the six messages the host carries between the program and its
//! view and the bridge of a page (§9), and what ends a view and in what order
//! (§10). What it cannot decide, the terminal supplies: a source of unpredictable
//! bytes ([`Entropy`]), a place for verified files ([`Store`]), what it can see of
//! the pane ([`Observed`]), the clock behind every timeout, the web view, the
//! sandbox of the views it draws itself and the window.

mod native;
mod page;

use std::mem;
use std::sync::Arc;

use crate::bundle::{BundleLimits, Digest, Manifest, Received, Receiving, Refusal, Store};
use crate::frame::{
    BodyDecoder, DecodeError, Direction, Event, Ids, Message, Outgoing, Split, StreamDecoder,
    encode,
};
use crate::icon::{self, Icon};
use crate::message::{
    self, Bye, Dismiss, FEATURE_BUNDLE, FEATURE_DISMISS, Hello, HostInfo, OpenBody, Reason,
    SUPPORTED_VERSIONS, TokenProbe, ViewOffer,
};
use crate::sandbox::{self, Access, Consent, Grant};
use crate::text;
use crate::theme::Theme;
use crate::token::{Entropy, Token};

/// The most bytes of JSON a message between the program and its view may have: what fits one
/// message toward the program, since the host passes it on in both directions.
pub const MAX_VIEW_MESSAGE: usize = 1024 * 1024;

/// The most characters of a name the program supplied (its own, the view's) that the session
/// passes on to the terminal.
pub const MAX_LABEL_CHARS: usize = 100;

/// The most characters of the words a view the terminal draws itself failed with that go in
/// the `closed` (specification §8.8): room for an error and the line it is on.
pub const MAX_FAILURE_CHARS: usize = 500;

/// The most questions about links that a page may have open at once (specification §9.7); the
/// page's `link` messages beyond it are dropped.
pub const MAX_OPEN_LINKS: usize = 8;

/// The version of the bridge of a page (specification §5.5, §9.4) that this crate speaks: the
/// version a host offers `web` at.
pub const WEB_BRIDGE_VERSION: &str = "1.0";

/// The size of the view's container, in CSS pixels: the content area of the pane
/// (specification §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// What the host tells a page about itself (specification §9.5, §11.1): the person's
/// machine, not the program's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewContext {
    /// The terminal's look.
    pub theme: Theme,
    /// The pane's content area.
    pub size: Size,
    /// The person's language and region (BCP 47), when known.
    pub locale: Option<String>,
    /// The person's time zone (IANA), when known.
    pub time_zone: Option<String>,
    /// Whether the person asked the system for less motion.
    pub reduce_motion: bool,
}

/// A kind of view the host opens and the versions of its interface the host speaks, in the host's
/// order (specification §5.2, §5.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    /// The kind.
    pub kind: ViewKind,
    /// The versions of the kind's interface (`major.minor`) the host speaks.
    pub versions: Vec<String>,
}

impl Offer {
    /// An offer of `kind` at `versions`.
    pub fn new(kind: ViewKind, versions: &[&str]) -> Self {
        Self {
            kind,
            versions: versions.iter().map(|v| (*v).to_owned()).collect(),
        }
    }
}

/// What a session is configured with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The host's name and version: in the `welcome`.
    pub host: HostInfo,
    /// Whether the host takes bundles (lists `bundle` in its `features`). Every kind of view
    /// arrives as a bundle (specification §8.1), so a host that does not take them opens none.
    pub bundle: bool,
    /// The limits a manifest is held to; they cannot exceed the protocol's.
    pub limits: BundleLimits,
    /// What a page is told.
    pub view: ViewContext,
    /// Whether the host opens the links of a page (its `link` messages, specification §9.7).
    pub open_links: bool,
    /// The kinds of view the host will open, each with the versions of its interface it speaks
    /// (specification §5.2): what its `welcome` offers of what the program's `hello` listed.
    /// Only [`ViewKind::Web`] at [`WEB_BRIDGE_VERSION`] by default; a terminal that draws a view
    /// itself adds [`ViewKind::Wasm`] and [`ViewKind::Lua`] with the versions of the kit it
    /// has. Their order is the order of the `welcome` of a restart (§10.7), which answers no
    /// `hello`. A kind is opened only if it is listed here and the host takes bundles.
    pub views: Vec<Offer>,
    /// The most bytes the entry of a view the terminal draws itself (a component or a script)
    /// may have; an `open` over it is refused with `limit`. Cannot exceed what the bundle
    /// limits allow a file.
    pub native_entry_limit: u64,
}

impl Config {
    /// A configuration for a host that takes bundles and opens links, with the protocol's bundle
    /// limits, that opens web pages only.
    pub fn new(host: HostInfo, theme: Theme, size: Size) -> Self {
        Self {
            host,
            bundle: true,
            limits: BundleLimits::PROTOCOL,
            view: ViewContext {
                theme,
                size,
                locale: None,
                time_zone: None,
                reduce_motion: false,
            },
            open_links: true,
            views: vec![Offer::new(ViewKind::Web, &[WEB_BRIDGE_VERSION])],
            native_entry_limit: crate::bundle::MAX_FILE_SIZE,
        }
    }
}

/// What the terminal can see of the pane, for the checks of specification §6.2
/// and §6.5. A field the terminal cannot observe stays `None` (or `false`) and
/// the check is skipped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Observed {
    /// Shell integration reports that a prompt is being edited (OSC 133 between
    /// the end of the prompt and the start of the command): every frame is
    /// ignored.
    pub prompt_editing: bool,
    /// Whether the pane is on the alternate screen. `Some(false)` makes an
    /// `open` refused with `screen`.
    pub alternate_screen: Option<bool>,
    /// On a pane whose tty the host owns: whether the foreground process group
    /// is the shell's own. `Some(true)` makes a `hello` go unanswered.
    pub shell_in_foreground: Option<bool>,
    /// On the same panes: whether the tty is canonical or echoes, the state a
    /// program must have left (§4.9). `Some(true)` makes a `hello` go unanswered.
    pub tty_canonical_or_echo: Option<bool>,
}

/// Where a session stands (specification §10.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Nothing outstanding.
    Idle,
    /// A token is outstanding.
    Offered,
    /// An `open` was accepted; files are pending, or the person is being asked.
    Opening,
    /// The view runs.
    Open,
    /// The program has ended: nothing is written until a `hello` says there is
    /// a program again.
    Ended,
}

/// What a view is made of, and so who draws it (specification §5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ViewKind {
    /// A web page: HTML, scripts and styles in a web view of the terminal's, that reaches the
    /// host only through the bridge of specification §9.4.
    Web,
    /// A WebAssembly component the terminal runs in its own sandbox and draws itself
    /// (specification §8.8).
    Wasm,
    /// A script in the Lua language, which the terminal runs in the same sandbox (its interpreter
    /// is a component) and draws itself (specification §8.8).
    Lua,
}

impl ViewKind {
    /// The name the wire gives the kind (`hello.views`, `welcome.views`, `open.view`;
    /// specification §5.5).
    pub fn name(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Wasm => "wasm",
            Self::Lua => "lua",
        }
    }

    /// Whether the terminal draws a view of this kind itself, with no document and no web view:
    /// the kit views of specification §8.8.
    pub fn is_native(self) -> bool {
        matches!(self, Self::Wasm | Self::Lua)
    }

    /// The kind a wire name stands for. A name that is not in the table of kinds is nobody's:
    /// a program may list a kind this host has never heard of.
    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Web, Self::Wasm, Self::Lua]
            .into_iter()
            .find(|kind| kind.name() == name)
    }

    /// The media type the entry of a bundle has when the view is of this kind (specification
    /// §6.3: an entry that disagrees with `view` is a malformed `open`).
    fn entry_type(self) -> &'static str {
        match self {
            Self::Web => "text/html",
            Self::Wasm => crate::bundle::WASM_MEDIA_TYPE,
            Self::Lua => crate::bundle::LUA_MEDIA_TYPE,
        }
    }
}

/// A view to start.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewStart {
    /// What the view is made of.
    pub kind: ViewKind,
    /// The version of the kind's interface the view is written against: one of those the
    /// `welcome` listed for the kind, as the `open` named it.
    pub version: String,
    /// What the person allowed of what a page asked for. Nothing for a view the terminal draws
    /// itself.
    pub grant: Grant,
    /// The content security policy to enforce, built from `grant` only. Empty for a view the
    /// terminal draws itself ([`ViewKind::is_native`]), which has no document to apply it to.
    pub policy: String,
    /// The value of the `allow` attribute of the page's frame. Empty for a view the terminal
    /// draws itself.
    pub allow: String,
    /// What the program calls itself (`program` of its `open`).
    ///
    /// The program wrote it, and a terminal draws it in its own chrome, where a program must not
    /// be able to type or to forge a neighbour. The session has therefore cut it to
    /// [`MAX_LABEL_CHARS`] characters and left out control characters, the bidirectional
    /// overrides and the invisible format characters (the zero-width joiner and non-joiner and
    /// the left-to-right and right-to-left marks, which text needs, excepted); what is here is
    /// safe to draw as it stands. `None` when it says nothing or nothing is left.
    pub program: Option<String>,
    /// What the program calls the view (`title` of its `open`), cut and cleaned as
    /// [`program`](Self::program) is.
    pub title: Option<String>,
    /// The icons the program gives for itself (`icons` of its `open`), those the
    /// [`icon`] module takes, in the program's order. Empty when it gives none.
    /// [`icon::pick`] chooses one for the terminal's background and the size it
    /// draws at.
    ///
    /// Like the names, they are for the terminal's own chrome, beside
    /// [`title`](Self::title). [`Consent`] has none of them: a question the terminal asks in its
    /// own name must not carry a picture the program chose, which could pass for the terminal's
    /// own or for another application's.
    pub icons: Vec<Icon>,
    /// For a page: the JSON text of the first `context` message (specification §9.5), `{"context":
    /// {…}}`, which the terminal delivers to the page once the page has loaded. The crate cannot
    /// know when that is, so what changes the context between the start and then comes to the
    /// terminal as [`Action::ToView`] and, if it is dropped for the page not being there yet, the
    /// current text is [`Session::page_context`]. `None` for a view the terminal draws itself,
    /// which the kit tells.
    pub context: Option<String>,
}

/// What the session asks the terminal to do. Actions come in the order they must
/// be done.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Action {
    /// Write these bytes into the input side of the pane's tty: one message, one
    /// or more complete frames. The terminal writes them without blocking, keeps
    /// what the tty does not take and writes it later, and puts nothing inside a
    /// frame (specification §4.8).
    Write(Vec<u8>),
    /// Ask the person which of the origins and permissions the page asked for it may use,
    /// then answer with [`Session::consent`] and the question's [`id`](Consent::id). The page
    /// does not start before that. The question says which program asks
    /// ([`Consent::program`]).
    ///
    /// The question is asked until it is answered or until [`Action::WithdrawConsent`]
    /// takes it back.
    Consent(Consent),
    /// The question with this [`Consent::id`] is not to be asked any more: the open it
    /// was about ended before the person answered (the program closed or ended the
    /// view, the person closed it, the wait ran out). The terminal takes the question
    /// off the screen. An answer to it that is already on its way is ignored by
    /// [`Session::consent`], so the person's click can never settle the question of
    /// another view.
    WithdrawConsent(u32),
    /// Start the view: run it, for a page create the web view and enforce `policy`, and pass what
    /// a page sends to [`Session::view_message`] and what a view the terminal draws itself says
    /// to [`Session::module_message`].
    StartView(Box<ViewStart>),
    /// Deliver this JSON object to the view: a message of the program, which the host has not
    /// read, or a `context` for a page.
    ToView(String),
    /// A page asks to open this `http` or `https` link in the person's browser (specification
    /// §9.7). The terminal asks the person if it wants and answers with
    /// [`Session::link_decision`]; the page is never answered.
    OpenLink {
        /// Identifies the request in the answer.
        link: u32,
        /// The URL.
        url: String,
    },
    /// Tear the view down and stop serving its bundle. Nothing is waited for and the view is not
    /// told (specification §10.2): the questions about links that are still open are void.
    DestroyView,
}

/// Counts of what the session saw and dropped, for diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Diagnostics {
    /// Frames that were not well-formed.
    pub malformed: u64,
    /// Frames over the frame limit.
    pub too_long: u64,
    /// Chunks that did not continue their message.
    pub chunk_order: u64,
    /// Messages dropped for another that began.
    pub abandoned: u64,
    /// Messages over the message limit.
    pub message_too_long: u64,
    /// Messages ignored: unknown kind, wrong direction, invalid payload, wrong
    /// moment; and answers to a question that is no longer the one asked.
    pub ignored: u64,
    /// Messages of a page that the host handled itself (`context`, `close`, `link`,
    /// specification §9.4).
    pub read: u64,
    /// Messages the session could not write (too large for the program's side).
    pub unwritable: u64,
}

impl Diagnostics {
    fn record(&mut self, error: DecodeError) {
        let counter = match error {
            DecodeError::Malformed => &mut self.malformed,
            DecodeError::TooLong => &mut self.too_long,
            DecodeError::ChunkOrder => &mut self.chunk_order,
            DecodeError::Abandoned => &mut self.abandoned,
            DecodeError::MessageTooLong => &mut self.message_too_long,
        };
        *counter = counter.saturating_add(1);
    }
}

// ---- internal state --------------------------------------------------------

enum Stage {
    /// Waiting for the files named in `need`.
    Files(Receiving),
    /// Nothing is missing; the view has not started.
    Ready,
    /// The person is being asked the question with this id.
    Consent { id: u32 },
}

struct Opening {
    /// The id of the `open`, for the `re` of what answers it.
    open: u32,
    kind: ViewKind,
    version: String,
    bundle: Manifest,
    stage: Stage,
    /// What the program calls itself and its view, and its icons, as the terminal may draw them.
    program: Option<String>,
    title: Option<String>,
    icons: Vec<Icon>,
    /// What a page asks to reach; `None` for the other kinds, and for a page that asks nothing.
    access: Option<Access>,
}

struct Live {
    kind: ViewKind,
    bundle: Manifest,
    /// What the person allowed.
    grant: Grant,
    /// The `context` last told to a page, as JSON text.
    context: Option<String>,
    /// The questions about links that are open (specification §9.7).
    links: Vec<u32>,
}

/// What ended a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cause {
    /// The program ended: it is not told.
    Program,
    /// The person closed the view.
    User,
    /// The page asked to close.
    View,
    /// The open or the view failed, with this code.
    Failed(&'static str),
    /// The host is starting the view again.
    Restart,
    /// The program asked for the view to close and goes on running; carries the
    /// id of its `dismiss`, which the `closed` that tells it answers.
    Dismissed(u32),
}

enum Phase {
    Idle,
    Opening(Box<Opening>),
    Live(Box<Live>),
}

/// One pane's conversation with one program. See the [module](self) documentation.
pub struct Session<S: Store, E: Entropy> {
    config: Config,
    context: ViewContext,
    store: S,
    entropy: E,
    ids: Ids,
    /// The id of the last question about a link ([`Action::OpenLink`]).
    links: u32,
    /// The id of the last question put to the person ([`Consent::id`]).
    questions: u32,
    token: Option<Token>,
    /// The kinds of view, with their versions, that the `welcome` which issued the outstanding
    /// token offered: what an `open` with the token may ask for (specification §6.4).
    offered: Vec<ViewOffer>,
    /// The token the view that is opening or open was opened with. The
    /// `open` spends it, and it stays the view's identity until the view has
    /// closed: the program's `bye` is believed only with it (§10.3).
    view_token: Option<Token>,
    /// The outstanding token came with an unsolicited `welcome` (§10.7).
    restart_offer: bool,
    /// The words a view the terminal draws itself failed with, for the `closed` of the close
    /// that is running (specification §8.8).
    failure: Option<String>,
    ended: bool,
    observed: Observed,
    allow_opens: bool,
    phase: Phase,
    stream: StreamDecoder,
    body: BodyDecoder,
    actions: Vec<Action>,
    diagnostics: Diagnostics,
}

impl<S: Store, E: Entropy> Session<S, E> {
    /// A session for a pane whose program has not said anything yet.
    pub fn new(config: Config, store: S, entropy: E) -> Self {
        let context = config.view.clone();
        Self {
            config,
            context,
            store,
            entropy,
            ids: Ids::new(),
            links: 0,
            questions: 0,
            token: None,
            offered: Vec::new(),
            view_token: None,
            restart_offer: false,
            failure: None,
            ended: false,
            observed: Observed::default(),
            allow_opens: true,
            phase: Phase::Idle,
            stream: StreamDecoder::new(Direction::ToHost),
            body: BodyDecoder::new(Direction::ToHost),
            actions: Vec::new(),
            diagnostics: Diagnostics::default(),
        }
    }

    // ---- input: the program's output -------------------------------------

    /// Feeds raw bytes of the program's output. They may be cut anywhere.
    pub fn feed(&mut self, bytes: &[u8]) {
        let mut events = Vec::new();
        self.stream.feed(bytes, &mut events);
        self.events(events);
    }

    /// An APC string begins in the program's output (the terminal's parser found
    /// `ESC _`).
    pub fn apc_start(&mut self) {
        self.body.start();
    }

    /// A piece of the body of the APC string that began.
    pub fn apc_put(&mut self, data: &[u8]) {
        let mut events = Vec::new();
        self.body.put(data, &mut events);
        self.events(events);
    }

    /// The APC string ended; `cancelled` is whether the parser abandoned it.
    pub fn apc_end(&mut self, cancelled: bool) {
        let mut events = Vec::new();
        self.body.end(cancelled, &mut events);
        self.events(events);
    }

    fn events(&mut self, events: Vec<Event>) {
        for event in events {
            match event {
                Event::Message(message) => self.message(message),
                Event::Error(error) => self.diagnostics.record(error),
            }
        }
    }

    /// Handles a message a decoder delivered.
    pub fn message(&mut self, message: Message) {
        // While a prompt is edited the host ignores the protocol (§6.2).
        if self.observed.prompt_editing {
            self.ignored();
            return;
        }
        match message.kind.as_str() {
            "hello" => self.on_hello(&message),
            "open" => self.on_open(&message),
            "file" => self.on_file(&message),
            "msg" => self.on_msg(&message),
            "bye" => self.on_bye(&message),
            "dismiss" => self.on_dismiss(&message),
            // A kind this version does not know, or one that only the host
            // sends: ignored (§4.5).
            _ => self.ignored(),
        }
    }

    // ---- input: the terminal ---------------------------------------------

    /// Tells the session what the terminal can see of the pane.
    pub fn observe(&mut self, observed: Observed) {
        self.observed = observed;
    }

    /// Sets whether the host's own policy lets a program open a view; when it
    /// does not, an `open` is refused with `policy`.
    pub fn allow_opens(&mut self, allow: bool) {
        self.allow_opens = allow;
    }

    /// The program ended: the terminal saw the command finish, the foreground
    /// process group change or the stream end. The view is closed and nothing is
    /// written until a `hello` says a program is there again (§10.3).
    pub fn program_ended(&mut self) {
        self.ended = true;
        self.token = None;
        self.view_token = None;
        self.restart_offer = false;
        self.close(Cause::Program, None);
    }

    /// The person closes the view (§10.5). The view is gone at once, and the program is told
    /// `closed` with reason `user`.
    pub fn close_view(&mut self) {
        self.close(Cause::User, None);
    }

    /// Starts the view again after something that restores a screen without
    /// restoring messages (a reconnection, a hand-over to another host process;
    /// §10.7): the host writes an unsolicited `welcome` with a new token and
    /// `restart`, and the program that has a view open opens it again.
    pub fn restart(&mut self) {
        match self.phase {
            Phase::Idle => self.offer_restart(),
            Phase::Opening(_) | Phase::Live(_) => self.close(Cause::Restart, None),
        }
    }

    /// A wait the terminal chose has run out. What it ends depends on what is
    /// being waited for: the files of an open or the person (the open ends with
    /// `error`, `timeout`), the program's `open` after a restart (the token is
    /// forgotten).
    pub fn timed_out(&mut self) {
        match &self.phase {
            Phase::Opening(_) => self.fail("timeout"),
            Phase::Idle if self.restart_offer => {
                self.token = None;
                self.restart_offer = false;
            }
            Phase::Idle | Phase::Live(_) => {}
        }
    }

    /// The pane was resized (§7.2): a page is told if it runs.
    pub fn set_size(&mut self, size: Size) {
        self.context.size = size;
        self.context_changed();
    }

    /// The terminal's look changed: a page is told if it runs.
    pub fn set_theme(&mut self, theme: Theme) {
        self.context.theme = theme;
        self.context_changed();
    }

    /// The person's language and region (BCP 47) and time zone (IANA) changed, or became known:
    /// a page is told if it runs.
    pub fn set_locale(&mut self, locale: Option<String>, time_zone: Option<String>) {
        self.context.locale = locale;
        self.context.time_zone = time_zone;
        self.context_changed();
    }

    /// The person's setting for less motion changed: a page is told if it runs.
    pub fn set_reduce_motion(&mut self, reduce_motion: bool) {
        self.context.reduce_motion = reduce_motion;
        self.context_changed();
    }

    // ---- input: the person ------------------------------------------------

    /// The person's answer to the question [`Action::Consent`] asked, whose
    /// [`id`](Consent::id) is `id`. It is cut down to what the page asked for,
    /// whatever it says.
    ///
    /// An answer to a question that is no longer the one being asked, because the
    /// open it was about has ended ([`Action::WithdrawConsent`]) or the person has
    /// answered it already, changes nothing: the program can open another view at
    /// once, and an answer must not settle the question of a view the person has not
    /// been shown.
    pub fn consent(&mut self, id: u32, grant: Grant) {
        let asked = matches!(
            &self.phase,
            Phase::Opening(open) if matches!(&open.stage, Stage::Consent { id: asked } if *asked == id)
        );
        if !asked {
            self.ignored();
            return;
        }
        let Some(open) = self.take_opening() else {
            return;
        };
        let grant = match &open.access {
            Some(access) => grant.within(access),
            None => Grant::none(),
        };
        self.start(*open, grant);
    }

    // ---- output ----------------------------------------------------------

    /// Takes the actions the session has queued, in order.
    pub fn take_actions(&mut self) -> Vec<Action> {
        mem::take(&mut self.actions)
    }

    // ---- introspection ---------------------------------------------------

    /// Where the session stands.
    pub fn state(&self) -> State {
        match &self.phase {
            Phase::Opening(_) => State::Opening,
            Phase::Live(_) => State::Open,
            Phase::Idle if self.ended => State::Ended,
            Phase::Idle if self.token.is_some() => State::Offered,
            Phase::Idle => State::Idle,
        }
    }

    /// The manifest of the view that is opening or open.
    pub fn bundle(&self) -> Option<&Manifest> {
        match &self.phase {
            Phase::Opening(open) => Some(&open.bundle),
            Phase::Live(live) => Some(&live.bundle),
            Phase::Idle => None,
        }
    }

    /// The bytes and media type of the file at `path` of the bundle of the view
    /// that is open, which is all the host serves: nothing outside the manifest
    /// answers (§8.6).
    pub fn serve(&self, path: &str) -> Option<(String, Arc<[u8]>)> {
        let Phase::Live(live) = &self.phase else {
            return None;
        };
        let file = live.bundle.file(path)?;
        Some((file.media_type.clone(), self.store.get(&file.digest)?))
    }

    /// The store.
    pub fn store(&self) -> &S {
        &self.store
    }

    /// The store, for the host to trim.
    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }

    /// What the session has seen and dropped.
    pub fn diagnostics(&self) -> &Diagnostics {
        &self.diagnostics
    }

    // ---- the program's messages ------------------------------------------

    fn ignored(&mut self) {
        self.diagnostics.ignored = self.diagnostics.ignored.saturating_add(1);
    }

    /// Takes the phase out and leaves `Idle` behind, so a caller must put a phase
    /// back on every path. A handler that expects one phase uses `take_opening`,
    /// which leaves any other phase where it is: a message at the wrong moment
    /// must not end a view that is running.
    fn take(&mut self) -> Phase {
        mem::replace(&mut self.phase, Phase::Idle)
    }

    /// Takes the phase out if an open is under way, and leaves it alone if not.
    fn take_opening(&mut self) -> Option<Box<Opening>> {
        match self.take() {
            Phase::Opening(open) => Some(open),
            other => {
                self.phase = other;
                None
            }
        }
    }

    /// Whether the hardening checks of §6.2 let the host answer a program.
    fn answers(&self) -> bool {
        let seen = &self.observed;
        !seen.prompt_editing
            && seen.shell_in_foreground != Some(true)
            && seen.tty_canonical_or_echo != Some(true)
    }

    fn issue_token(&mut self, restart: bool, offered: Vec<ViewOffer>) -> Token {
        let token = Token::from_bytes(&self.entropy.token_bytes());
        self.token = Some(token.clone());
        self.offered = offered;
        self.restart_offer = restart;
        token
    }

    /// What the `welcome` says the host supports: closing a view at the program's
    /// request is part of every session, bundles are the configuration's.
    fn features(&self) -> &'static [&'static str] {
        if self.config.bundle {
            &[FEATURE_BUNDLE, FEATURE_DISMISS]
        } else {
            &[FEATURE_DISMISS]
        }
    }

    /// What the host offers of `kind`, if it will open it: the configuration lists it, and a view
    /// arrives as a bundle, so the host has to take bundles.
    fn offer_of(&self, kind: ViewKind) -> Option<&Offer> {
        if !self.config.bundle {
            return None;
        }
        self.config.views.iter().find(|offer| offer.kind == kind)
    }

    /// The `views` of the `welcome` that answers a `hello` (specification §5.2): for each entry
    /// of the `hello`, in order, the same kind with its versions cut down to those the host also
    /// offers, in the program's order. A kind the host will not open, a name that is not in the
    /// table of kinds, and a kind with no version in common are left out, and a kind listed
    /// twice counts once, at its first place: the host eliminates and never reorders or adds.
    fn offer_for(&self, listed: &[ViewOffer]) -> Vec<ViewOffer> {
        let mut offered: Vec<ViewOffer> = Vec::new();
        // Only the three kinds of the table are ever kept, so this stays as short as the table
        // however long the program's list is.
        let mut seen: Vec<ViewKind> = Vec::new();
        for entry in listed {
            let Some(kind) = ViewKind::from_name(&entry.kind) else {
                continue;
            };
            if seen.contains(&kind) {
                continue;
            }
            seen.push(kind);
            let Some(offer) = self.offer_of(kind) else {
                continue;
            };
            let mut versions: Vec<String> = Vec::new();
            for version in &entry.versions {
                if offer.versions.contains(version) && !versions.contains(version) {
                    versions.push(version.clone());
                }
            }
            if !versions.is_empty() {
                offered.push(ViewOffer {
                    kind: kind.name().to_owned(),
                    versions,
                });
            }
        }
        offered
    }

    fn on_hello(&mut self, message: &Message) {
        if !self.answers() {
            self.ignored();
            return;
        }
        let Some(hello) = crate::json::from_object::<Hello>(&message.payload) else {
            self.ignored();
            return;
        };
        // A program that lists no kind of view cannot be offered one: it is not a hello.
        if hello.views.is_empty() {
            self.ignored();
            return;
        }
        // A hello means a program is reading the tty.
        self.ended = false;
        let version = SUPPORTED_VERSIONS
            .iter()
            .rev()
            .copied()
            .find(|v| hello.versions.contains(v));
        let payload = match version {
            Some(version) => {
                let offered = self.offer_for(&hello.views);
                let token = self.issue_token(false, offered.clone());
                message::welcome_offer(
                    version,
                    token.as_str(),
                    &self.config.host,
                    self.features(),
                    &offered,
                    false,
                )
            }
            None => message::welcome_unsupported(&self.config.host),
        };
        self.send("welcome", Some(message.id), &payload);
    }

    fn offer_restart(&mut self) {
        if !self.answers() {
            return;
        }
        self.ended = false;
        // No `hello` asked: every kind the host opens with every version of it, in the host's
        // order. The program that restarts a view chooses among them as it did from the first
        // `welcome`.
        let mut offered: Vec<ViewOffer> = Vec::new();
        for offer in &self.config.views {
            if self.offer_of(offer.kind).is_none()
                || offered.iter().any(|known| known.kind == offer.kind.name())
            {
                continue;
            }
            let mut versions: Vec<String> = Vec::new();
            for version in &offer.versions {
                if !versions.contains(version) {
                    versions.push(version.clone());
                }
            }
            if !versions.is_empty() {
                offered.push(ViewOffer {
                    kind: offer.kind.name().to_owned(),
                    versions,
                });
            }
        }
        let token = self.issue_token(true, offered.clone());
        let version = SUPPORTED_VERSIONS.last().copied().unwrap_or(1);
        let payload = message::welcome_offer(
            version,
            token.as_str(),
            &self.config.host,
            self.features(),
            &offered,
            true,
        );
        self.send("welcome", None, &payload);
    }

    fn refuse(&mut self, open: u32, code: &str) {
        self.send(
            "closed",
            Some(open),
            &message::closed(Reason::Refused, Some(code), None),
        );
    }

    fn on_open(&mut self, message: &Message) {
        if self.ended {
            self.ignored();
            return;
        }
        // The token first, before anything else in the payload (§6.4).
        let presented = match message::probe_token(&message.payload) {
            TokenProbe::NotJson => {
                self.ignored();
                return;
            }
            TokenProbe::NoToken => None,
            TokenProbe::Token(token) => Some(token),
        };
        let matched = match (&self.token, &presented) {
            (Some(outstanding), Some(presented)) => outstanding.matches(presented),
            _ => false,
        };
        if !matched {
            // A wrong token leaves the outstanding one alone, so a hostile
            // stream cannot use up the token of the program that is running.
            // The refusal goes into the same input as the answer to a hello, and
            // a stream anyone can write to can send an `open` as easily as a
            // `hello`: where a hello would go unanswered, so does this.
            if self.answers() {
                self.refuse(message.id, "token");
            } else {
                self.ignored();
            }
            return;
        }
        // Spent by the first open that carries it, whatever comes of it.
        let spent = self.token.take();
        let offered = mem::take(&mut self.offered);
        self.restart_offer = false;
        if !matches!(self.phase, Phase::Idle) {
            self.refuse(message.id, "busy");
            return;
        }
        if self.observed.alternate_screen == Some(false) {
            self.refuse(message.id, "screen");
            return;
        }
        let Some(body) = crate::json::from_object::<OpenBody>(&message.payload) else {
            self.refuse(message.id, "manifest");
            return;
        };
        // The kind of view and its version are the program's to say and nothing the host infers
        // from the files (§6.3); a payload that does not say, or has no bundle, is malformed.
        let (Some(view), Some(version), Some(raw)) =
            (body.view.as_deref(), body.version.as_deref(), body.bundle)
        else {
            self.refuse(message.id, "manifest");
            return;
        };
        let bundle = match Manifest::check(raw, self.config.limits) {
            Ok(manifest) => manifest,
            Err(Refusal::Manifest) => {
                self.refuse(message.id, "manifest");
                return;
            }
            Err(Refusal::Limit) => {
                self.refuse(message.id, "limit");
                return;
            }
        };
        // A name that is not in the table of kinds is not one the `welcome` offered: it is
        // refused below, after the checks that do not need to know what it stands for.
        let kind = ViewKind::from_name(view);
        let entry = bundle.entry_file();
        if let Some(kind) = kind {
            // The bundle has to be of the kind the program says: the entry of a page is an HTML
            // document, and that of a view the terminal draws itself is its component or its
            // script.
            if entry.is_none_or(|entry| entry.media_type != kind.entry_type()) {
                self.refuse(message.id, "manifest");
                return;
            }
        }
        // What a page asks to reach is read for a page only.
        let access = match (kind, body.access) {
            (Some(ViewKind::Web), Some(raw)) => {
                let read = serde_json::from_str(raw.get())
                    .ok()
                    .and_then(|value| Access::from_value(&value).ok());
                let Some(access) = read else {
                    self.refuse(message.id, "manifest");
                    return;
                };
                Some(access)
            }
            _ => None,
        };
        if kind.is_some_and(ViewKind::is_native)
            && entry.is_some_and(|entry| entry.size > self.config.native_entry_limit)
        {
            self.refuse(message.id, "limit");
            return;
        }
        let kind = match kind {
            Some(kind)
                if offered
                    .iter()
                    .any(|o| o.kind == view && o.versions.iter().any(|v| v == version)) =>
            {
                kind
            }
            _ => {
                self.refuse(message.id, "unsupported");
                return;
            }
        };
        if !self.allow_opens {
            self.refuse(message.id, "policy");
            return;
        }
        let missing = bundle.missing(&self.store);
        let list: Vec<String> = missing.iter().map(ToString::to_string).collect();
        self.send("need", Some(message.id), &message::need(&list));
        let stage = if missing.is_empty() {
            Stage::Ready
        } else {
            Stage::Files(Receiving::new(&bundle, &missing))
        };
        let waiting = matches!(stage, Stage::Files(_));
        // The view is the one this token opened. A refused open, a busy one
        // above all, has left the token of the view that runs where it was.
        self.view_token = spent;
        self.phase = Phase::Opening(Box::new(Opening {
            open: message.id,
            kind,
            version: version.to_owned(),
            bundle,
            stage,
            program: body.program.as_deref().and_then(text::label),
            title: body.title.as_deref().and_then(text::label),
            icons: icon::read(body.icons.as_deref()),
            access,
        }));
        if !waiting {
            self.begin();
        }
    }

    fn on_file(&mut self, message: &Message) {
        let Some(digest) = message.digest().and_then(Digest::parse) else {
            self.ignored();
            return;
        };
        let Some(mut open) = self.take_opening() else {
            self.ignored();
            return;
        };
        let Stage::Files(receiving) = &mut open.stage else {
            self.phase = Phase::Opening(open);
            self.ignored();
            return;
        };
        let received = receiving.accept(&digest, &message.payload);
        let complete = receiving.is_complete();
        self.phase = Phase::Opening(open);
        match received {
            Received::Ignored => self.ignored(),
            Received::Failed => self.fail("bundle"),
            Received::Verified(file) => {
                self.store.insert(file);
                if complete {
                    self.files_complete();
                }
            }
        }
    }

    fn files_complete(&mut self) {
        let held = match &self.phase {
            Phase::Opening(open) => open.bundle.is_held(&self.store),
            _ => return,
        };
        // A store may drop an entry at any time (§8.5), including one just kept.
        if held {
            self.begin();
        } else {
            self.fail("bundle");
        }
    }

    /// The files are here (or none were missing): a view the terminal draws itself starts at
    /// once, and so does a page that asked for nothing to reach. A page that did is not started
    /// before the person has been asked (specification §8.3, §12.2).
    fn begin(&mut self) {
        let Some(mut open) = self.take_opening() else {
            return;
        };
        let Some(mut question) = open.access.as_ref().and_then(Access::consent) else {
            self.start(*open, Grant::none());
            return;
        };
        self.questions = self.questions.checked_add(1).unwrap_or(1);
        question.id = self.questions;
        question.program.clone_from(&open.program);
        question.title.clone_from(&open.title);
        open.stage = Stage::Consent { id: question.id };
        self.phase = Phase::Opening(open);
        self.actions.push(Action::Consent(question));
    }

    /// Starts the view of an open whose files are held and whose person has been asked, if there
    /// was a question, with `grant` as the answer.
    fn start(&mut self, open: Opening, grant: Grant) {
        let Opening {
            kind,
            version,
            bundle,
            program,
            title,
            icons,
            access,
            ..
        } = open;
        let web = kind == ViewKind::Web;
        let context = web.then(|| page::context_text(&self.context, &grant));
        let start = ViewStart {
            kind,
            version,
            grant: grant.clone(),
            policy: if web {
                sandbox::content_security_policy(access.as_ref(), &grant)
            } else {
                String::new()
            },
            allow: if web {
                grant.allow_attribute()
            } else {
                String::new()
            },
            program,
            title,
            icons,
            context: context.clone(),
        };
        self.phase = Phase::Live(Box::new(Live {
            kind,
            bundle,
            grant,
            context,
            links: Vec::new(),
        }));
        self.actions.push(Action::StartView(Box::new(start)));
    }

    /// A `msg` of the program: while a view is open it is for the view, and goes to it as it
    /// is, unread (specification §9.3). Whoever can write the stream can send this, so it is held
    /// to being an object (a view reads nothing else) and to the size one message may have; for a
    /// page, to not being named like one of the host's own messages (§9.4). Before the view runs,
    /// and after it has closed, it is dropped.
    fn on_msg(&mut self, message: &Message) {
        let web = match &self.phase {
            Phase::Live(live) => live.kind == ViewKind::Web,
            Phase::Idle | Phase::Opening(_) => {
                self.ignored();
                return;
            }
        };
        let payload = &message.payload;
        let passes = payload.len() <= MAX_VIEW_MESSAGE
            && if web {
                page::to_page(payload)
            } else {
                crate::json::is_object(payload)
            };
        if passes {
            self.actions.push(Action::ToView(payload.clone()));
        } else {
            self.ignored();
        }
    }

    /// The program ended, if it says so with the token that opened the view. The
    /// stream may be written by anyone and `bye` has no answer to read, so a
    /// `bye` that carries no token, another token, or arrives when no view is
    /// opening or open, changes nothing (§10.3, §14).
    fn on_bye(&mut self, message: &Message) {
        let Some(bye) = crate::json::from_object::<Bye>(&message.payload) else {
            self.ignored();
            return;
        };
        let believed = match (&self.view_token, &bye.token) {
            (Some(opened), Some(presented)) => opened.matches(presented),
            _ => false,
        };
        if believed {
            self.program_ended();
            self.answer_bye(message.id);
        } else {
            self.ignored();
        }
    }

    /// The `closed` that answers a `bye` (§10.3): the one frame written for a program that has
    /// said it ended, because it is still reading. It is the last thing the host writes, so
    /// everything the host wrote before it comes ahead of it in the tty's queue, and the
    /// program that reads up to it has read the lot. Without it, a frame written before the
    /// host learned of the `bye` is read as typed input by whatever reads the tty next.
    fn answer_bye(&mut self, bye: u32) {
        let id = self.ids.next_id();
        let message = Outgoing {
            kind: "closed",
            id,
            re: Some(bye),
            digest: None,
            payload: &message::closed(Reason::Program, None, None),
        };
        let mut bytes = Vec::new();
        if encode(Direction::ToProgram, &message, Split::Fill, &mut bytes).is_ok() {
            self.actions.push(Action::Write(bytes));
        } else {
            self.diagnostics.unwritable = self.diagnostics.unwritable.saturating_add(1);
        }
    }

    /// The program closes its view and keeps running, if it says so with the token
    /// that opened the view (§10.8). The token is the same proof a `bye` gives and
    /// is read the same way.
    fn on_dismiss(&mut self, message: &Message) {
        let Some(dismiss) = crate::json::from_object::<Dismiss>(&message.payload) else {
            self.ignored();
            return;
        };
        let believed = match (&self.view_token, &dismiss.token) {
            (Some(opened), Some(presented)) => opened.matches(presented),
            _ => false,
        };
        if believed && matches!(self.phase, Phase::Opening(_) | Phase::Live(_)) {
            self.close(Cause::Dismissed(message.id), Some(message.id));
        } else {
            self.ignored();
        }
    }

    // ---- writing ---------------------------------------------------------

    /// Writes a message to the program, unless it has ended (§4.8).
    fn send(&mut self, kind: &str, re: Option<u32>, payload: &str) -> bool {
        if self.ended {
            return false;
        }
        let id = self.ids.next_id();
        let message = Outgoing {
            kind,
            id,
            re,
            digest: None,
            payload,
        };
        let mut bytes = Vec::new();
        if encode(Direction::ToProgram, &message, Split::Fill, &mut bytes).is_ok() {
            self.actions.push(Action::Write(bytes));
            true
        } else {
            self.diagnostics.unwritable = self.diagnostics.unwritable.saturating_add(1);
            false
        }
    }

    /// Writes a message of the view to the program as a `msg`.
    fn send_json(&mut self, text: &str) -> bool {
        self.send("msg", None, &crate::json::escape(text))
    }

    // ---- closing ---------------------------------------------------------

    /// Ends the open that is opening, with `error` and `code`.
    fn fail(&mut self, code: &'static str) {
        let re = match &self.phase {
            Phase::Opening(open) => Some(open.open),
            _ => None,
        };
        self.close(Cause::Failed(code), re);
    }

    /// A view ended on its own failure: the component trapped, went over a limit the terminal
    /// holds it to, handed out something it cannot use, a script made an error, or a page's web
    /// view ended. The view is closed as for any other end, and the program is told `closed`
    /// with `reason` `error`, `code` `view` and, in `message`, `why` (specification §8.8): the
    /// words the view failed with, which a script's error has the line in. They are the view's
    /// own, so they are cut and cleaned of what a terminal would act on before they are passed
    /// on.
    pub fn view_failed(&mut self, why: &str) {
        // Only a view that is there has words to carry: a failure told late (the view is gone)
        // must not leave them for the next, unrelated close.
        if matches!(self.phase, Phase::Opening(_) | Phase::Live(_)) {
            self.failure = text::words(why, MAX_FAILURE_CHARS);
        }
        self.close(Cause::Failed("view"), None);
    }

    /// Whatever ends a view (§10.2), at once and for every kind: the view is torn down, then the
    /// program is told unless it is the one that ended, then the token is forgotten.
    fn close(&mut self, cause: Cause, re: Option<u32>) {
        match self.take() {
            Phase::Idle => {
                if cause == Cause::Program {
                    self.token = None;
                }
                if cause == Cause::Restart {
                    self.offer_restart();
                }
            }
            Phase::Opening(open) => {
                // The person may be looking at the question of this open: it goes
                // away with it, and an answer on its way is for nobody.
                if let Stage::Consent { id } = &open.stage {
                    self.actions.push(Action::WithdrawConsent(*id));
                }
                // No view has run: nothing to tear down.
                let re = re.or(Some(open.open));
                self.finish(cause, re, false);
            }
            Phase::Live(_) => self.finish(cause, re, true),
        }
    }

    /// Steps 1 to 3 of §10.2.
    ///
    /// Step 3 forgets the view's token and nothing else. A token the host issued
    /// since (the answer to a `hello` that came while the view was opening or open)
    /// belongs to a program that is reading the tty, and so does the end
    /// of the program: `program_ended` has already forgotten what it ended, and a
    /// `hello` after it has already said there is a program again.
    fn finish(&mut self, cause: Cause, re: Option<u32>, destroy: bool) {
        // The words of a view's failure are for the close of that failure and no other.
        let words = self.failure.take();
        self.phase = Phase::Idle;
        if destroy {
            self.actions.push(Action::DestroyView);
        }
        match cause {
            Cause::Program | Cause::Restart => {}
            Cause::Dismissed(dismiss) => {
                self.send(
                    "closed",
                    Some(dismiss),
                    &message::closed(Reason::Program, None, None),
                );
            }
            Cause::User => {
                self.send("closed", re, &message::closed(Reason::User, None, None));
            }
            Cause::View => {
                self.send("closed", re, &message::closed(Reason::View, None, None));
            }
            Cause::Failed(code) => {
                self.send(
                    "closed",
                    re,
                    &message::closed(Reason::Error, Some(code), words.as_deref()),
                );
            }
        }
        self.view_token = None;
        if cause == Cause::Restart {
            self.offer_restart();
        }
    }
}
