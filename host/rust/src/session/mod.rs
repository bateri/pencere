//! The host's side of one pane's conversation with one program (specification
//! §5 to §10), as a state machine with no I/O.
//!
//! A [`Session`] belongs to one pane. The terminal feeds it what it learns:
//! the bytes or the APC strings the program wrote ([`Session::feed`],
//! [`Session::apc_put`] and its siblings), what the view says
//! ([`Session::view_message`]), what the person answers, what the terminal
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
//! asked for (§8), the MCP client the host is to the program and the host the
//! view sees (§9), and what ends a view and in what order (§10). What it cannot
//! decide, the terminal supplies: a source of unpredictable bytes
//! ([`Entropy`]), a place for verified files ([`Store`]), what it can see of the
//! pane ([`Observed`]), the clock behind every timeout, the web view and the
//! window.

mod mcp;
mod view;

use std::mem;
use std::sync::Arc;

use serde_json::value::RawValue;

use crate::bundle::{BundleLimits, Digest, Manifest, Received, Receiving, Refusal, Store};
use crate::frame::{
    BodyDecoder, DecodeError, Direction, Event, Ids, Message, Outgoing, Split, StreamDecoder,
    encode,
};
use crate::jsonrpc::Id;
use crate::message::{
    self, Bye, Dismiss, FEATURE_BUNDLE, FEATURE_DISMISS, Hello, HostInfo, OpenBody, Reason,
    SUPPORTED_VERSIONS, TokenProbe, ToolCall,
};
use crate::sandbox::{Consent, Grant, UiMeta};
use crate::theme::Theme;
use crate::token::{Entropy, Token};

/// The most bytes of JSON a message from the view may have: what fits one
/// message toward the program, since the host passes it on.
pub const MAX_VIEW_MESSAGE: usize = 1024 * 1024;

/// The most requests of the view the host passes on and waits for at once.
pub const MAX_IN_FLIGHT: usize = 64;

/// The most pages of tools the host reads from the program.
const MAX_TOOL_PAGES: u32 = 16;

/// The most tools the host remembers.
const MAX_TOOLS: usize = 4096;

/// The most characters of a name the program supplied (the title of a tool, its
/// own name) that the session passes on to the terminal.
pub const MAX_LABEL_CHARS: usize = 100;

/// The size of the view's container, in CSS pixels: the content area of the pane
/// (specification §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// What the host tells a view about itself (specification §11.1): the person's
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
    /// The host's name for `userAgent`.
    pub user_agent: String,
}

/// What a session is configured with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The host's name and version: in the `welcome`, as the MCP client, and as
    /// the host of the view.
    pub host: HostInfo,
    /// Whether the host takes bundles (lists `bundle` in its `features`).
    pub bundle: bool,
    /// The limits a manifest is held to; they cannot exceed the protocol's.
    pub limits: BundleLimits,
    /// What the view is told.
    pub view: ViewContext,
    /// Whether the host opens links for the view (`openLinks`).
    pub open_links: bool,
    /// Whether the host takes the view's log messages (`logging`).
    pub logging: bool,
}

impl Config {
    /// A configuration for a host that takes bundles, opens links and takes
    /// logs, with the protocol's bundle limits; `user_agent` is the host's name
    /// and version.
    pub fn new(host: HostInfo, theme: Theme, size: Size) -> Self {
        let user_agent = format!("{}/{}", host.name, host.version);
        Self {
            host,
            bundle: true,
            limits: BundleLimits::PROTOCOL,
            view: ViewContext {
                theme,
                size,
                locale: None,
                time_zone: None,
                user_agent,
            },
            open_links: true,
            logging: true,
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
    /// An `open` was accepted; files or the MCP start-up are pending, or the
    /// person is being asked.
    Opening,
    /// The view runs.
    Open,
    /// The view is being torn down.
    Closing,
    /// The program has ended: nothing is written until a `hello` says there is
    /// a program again.
    Ended,
}

/// Where the document of a view comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewSource {
    /// The bundle of the `open`: [`Session::bundle`] says what it is and
    /// [`Session::serve`] serves its files.
    Bundle,
    /// The HTML the program's `resources/read` returned.
    Document(String),
}

/// A view to start.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewStart {
    /// Where the document is.
    pub source: ViewSource,
    /// The view's metadata.
    pub meta: UiMeta,
    /// What the person allowed.
    pub grant: Grant,
    /// The content security policy to enforce, built from `grant` only.
    pub policy: String,
    /// The value of the `allow` attribute of the view's frame.
    pub allow: String,
    /// The name of the tool the view shows the result of.
    pub tool: String,
    /// What the program calls that tool for people: its `title` in `tools/list`,
    /// else its `annotations.title`. `None` when it has neither, and the terminal
    /// falls back to `tool`.
    ///
    /// The program wrote it, and a terminal draws it in its own chrome, where a
    /// program must not be able to type or to forge a neighbour. The session has
    /// therefore cut it to [`MAX_LABEL_CHARS`] characters and left out control
    /// characters and the bidirectional overrides; what is here is safe to draw
    /// as it stands.
    pub title: Option<String>,
    /// What the program calls itself: `serverInfo.title` of its MCP `initialize`
    /// result, else `serverInfo.name`. `None` when it says neither. Cut and
    /// cleaned as [`title`](Self::title) is.
    pub program: Option<String>,
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
    /// Ask the person which of the declared domains and permissions the view may
    /// use, then answer with [`Session::consent`]. The view does not start before
    /// that. The question says which tool the view is for ([`Consent::tool`]).
    Consent(Consent),
    /// Start the view: create the web view, enforce `policy`, and pass what the
    /// view sends to [`Session::view_message`].
    StartView(Box<ViewStart>),
    /// Deliver this JSON-RPC message to the view.
    ToView(String),
    /// The view asks to open this `http` or `https` link in the person's browser.
    /// The terminal asks the person if it wants and answers with
    /// [`Session::link_decision`].
    OpenLink {
        /// Identifies the request in the answer.
        link: u32,
        /// The URL.
        url: String,
    },
    /// Tear the view down and stop serving its bundle. It has been told
    /// (`ui/resource-teardown`) and has answered or the wait has ended.
    DestroyView,
    /// The view logged this (`notifications/message`), as the JSON text of its
    /// parameters.
    ViewLog(String),
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
    /// moment.
    pub ignored: u64,
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

/// A tool of the program, as far as the host needs to know it.
struct ToolEntry {
    name: String,
    app: bool,
}

/// The tool a view is for, and what the terminal will be told about it and about
/// the program, both already cut and cleaned.
struct Found {
    tool: Box<RawValue>,
    uri: String,
    title: Option<String>,
    program: Option<String>,
}

enum Stage {
    /// Waiting for the files named in `need`.
    Files(Receiving),
    /// Nothing is missing; the MCP start-up has not begun.
    Ready,
    /// `initialize` is out.
    Initialize(Id),
    /// `tools/list` is out.
    Tools {
        request: Id,
        pages: u32,
        tools: Vec<ToolEntry>,
        found: Option<Found>,
    },
    /// `resources/read` of the view's document is out.
    Resource {
        request: Id,
        tools: Vec<ToolEntry>,
        found: Found,
    },
    /// The person is being asked.
    Consent {
        source: ViewSource,
        meta: UiMeta,
        tools: Vec<ToolEntry>,
        found: Found,
    },
}

struct Opening {
    /// The id of the `open`, for the `re` of what answers it.
    open: u32,
    tool: ToolCall,
    bundle: Option<Manifest>,
    stage: Stage,
    /// What the program calls itself, once its `initialize` has answered.
    program: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewState {
    AwaitInitialize,
    AwaitInitialized,
    Ready,
}

struct Forward {
    program: Id,
    view: Id,
}

struct Live {
    tool: ToolCall,
    found: Found,
    tools: Vec<ToolEntry>,
    bundle: Option<Manifest>,
    grant: Grant,
    state: ViewState,
    /// The view declared `experimental["pencere/pane"]`.
    pane_aware: bool,
    /// The mode the view was told.
    display_mode: &'static str,
    /// The call of `open.tool` that is out, if any.
    open_call: Option<Id>,
    forwards: Vec<Forward>,
    links: Vec<(u32, Id)>,
}

/// What ended a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cause {
    /// The program ended: it is not told.
    Program,
    /// The person closed the view.
    User,
    /// The view asked to close.
    View,
    /// The open or the view failed, with this code.
    Failed(&'static str),
    /// The host is starting the view again.
    Restart,
    /// The program asked for the view to close and goes on running; carries the
    /// id of its `dismiss`, which the `closed` that tells it answers.
    Dismissed(u32),
}

struct Closing {
    cause: Cause,
    /// The id of the `ui/resource-teardown` request, whose answer ends the wait.
    request: Id,
    /// The program ended while the view was being torn down.
    program_gone: bool,
}

enum Phase {
    Idle,
    Opening(Box<Opening>),
    Live(Box<Live>),
    Closing(Box<Closing>),
}

/// One pane's conversation with one program. See the [module](self) documentation.
pub struct Session<S: Store, E: Entropy> {
    config: Config,
    context: ViewContext,
    store: S,
    entropy: E,
    ids: Ids,
    /// The next JSON-RPC id toward the program.
    rpc: i64,
    /// The next JSON-RPC id toward the view.
    view_rpc: i64,
    links: u32,
    token: Option<Token>,
    /// The token the view that is opening, open or closing was opened with. The
    /// `open` spends it, and it stays the view's identity until the view has
    /// closed: the program's `bye` is believed only with it (§10.3).
    view_token: Option<Token>,
    /// The outstanding token came with an unsolicited `welcome` (§10.7).
    restart_offer: bool,
    /// A restart is to follow the close that is running.
    restart_after: bool,
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
            rpc: 1,
            view_rpc: 1,
            links: 0,
            token: None,
            view_token: None,
            restart_offer: false,
            restart_after: false,
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
        self.restart_after = false;
        self.close(Cause::Program, None);
    }

    /// The person closes the view (§10.5). The program is told `closed` with
    /// reason `user` once the view is gone.
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
            Phase::Closing(_) => self.restart_after = true,
            Phase::Opening(_) | Phase::Live(_) => self.close(Cause::Restart, None),
        }
    }

    /// A wait the terminal chose has run out. What it ends depends on what is
    /// being waited for: the files of an open, the MCP start-up or the person
    /// (the open ends with `error`, `timeout`), the view's answer to its
    /// teardown (the view is torn down without it), the program's `open` after a
    /// restart (the token is forgotten).
    pub fn timed_out(&mut self) {
        match &self.phase {
            Phase::Opening(_) => self.fail("timeout"),
            Phase::Closing(_) => self.finish_close(),
            Phase::Idle if self.restart_offer => {
                self.token = None;
                self.restart_offer = false;
            }
            Phase::Idle | Phase::Live(_) => {}
        }
    }

    /// The pane was resized (§7.2): the view is told if it runs.
    pub fn set_size(&mut self, size: Size) {
        self.context.size = size;
        self.context_changed(true, false);
    }

    /// The terminal's look changed: the view is told if it runs.
    pub fn set_theme(&mut self, theme: Theme) {
        self.context.theme = theme;
        self.context_changed(false, true);
    }

    // ---- input: the person ------------------------------------------------

    /// The person's answer to [`Action::Consent`]. It is cut down to what the
    /// view declared, whatever it says.
    pub fn consent(&mut self, grant: Grant) {
        let Some(open) = self.take_opening() else {
            return;
        };
        let Opening {
            open,
            tool,
            bundle,
            stage,
            program,
        } = *open;
        let Stage::Consent {
            source,
            meta,
            tools,
            found,
        } = stage
        else {
            self.phase = Phase::Opening(Box::new(Opening {
                open,
                tool,
                bundle,
                stage,
                program,
            }));
            return;
        };
        let grant = grant.within(&meta);
        self.start_view(tool, bundle, source, meta, grant, tools, found);
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
            Phase::Closing(_) => State::Closing,
            Phase::Idle if self.ended => State::Ended,
            Phase::Idle if self.token.is_some() => State::Offered,
            Phase::Idle => State::Idle,
        }
    }

    /// The manifest of the view that is opening or open, if it has one.
    pub fn bundle(&self) -> Option<&Manifest> {
        match &self.phase {
            Phase::Opening(open) => open.bundle.as_ref(),
            Phase::Live(live) => live.bundle.as_ref(),
            _ => None,
        }
    }

    /// The bytes and media type of the file at `path` of the bundle of the view
    /// that is open, which is all the host serves: nothing outside the manifest
    /// answers (§8.6).
    pub fn serve(&self, path: &str) -> Option<(String, Arc<[u8]>)> {
        let Phase::Live(live) = &self.phase else {
            return None;
        };
        let file = live.bundle.as_ref()?.file(path)?;
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
    /// `take_live` or `take_closing`, which leave any other phase where it is: a
    /// message at the wrong moment must not end a view that is running.
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

    /// Takes the phase out if a view is running, and leaves it alone if not.
    fn take_live(&mut self) -> Option<Box<Live>> {
        match self.take() {
            Phase::Live(live) => Some(live),
            other => {
                self.phase = other;
                None
            }
        }
    }

    /// Takes the phase out if a view is being torn down, and leaves it alone if not.
    fn take_closing(&mut self) -> Option<Box<Closing>> {
        match self.take() {
            Phase::Closing(closing) => Some(closing),
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

    fn issue_token(&mut self, restart: bool) -> Token {
        let token = Token::from_bytes(&self.entropy.token_bytes());
        self.token = Some(token.clone());
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

    fn on_hello(&mut self, message: &Message) {
        if !self.answers() {
            self.ignored();
            return;
        }
        let Some(hello) = crate::json::from_object::<Hello>(&message.payload) else {
            self.ignored();
            return;
        };
        // A hello means a program is reading the tty.
        self.ended = false;
        let version = SUPPORTED_VERSIONS
            .iter()
            .rev()
            .copied()
            .find(|v| hello.versions.contains(v));
        let payload = match version {
            Some(version) => {
                let token = self.issue_token(false);
                message::welcome_offer(
                    version,
                    token.as_str(),
                    &self.config.host,
                    self.features(),
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
        let token = self.issue_token(true);
        let version = SUPPORTED_VERSIONS.last().copied().unwrap_or(1);
        let payload = message::welcome_offer(
            version,
            token.as_str(),
            &self.config.host,
            self.features(),
            true,
        );
        self.send("welcome", None, &payload);
    }

    fn refuse(&mut self, open: u32, code: &str) {
        self.send(
            "closed",
            Some(open),
            &message::closed(Reason::Refused, Some(code)),
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
        if body.tool.name.is_empty() {
            self.refuse(message.id, "manifest");
            return;
        }
        let bundle = match body
            .bundle
            .map(|raw| Manifest::check(raw, self.config.limits))
        {
            None => None,
            Some(Ok(manifest)) => Some(manifest),
            Some(Err(Refusal::Manifest)) => {
                self.refuse(message.id, "manifest");
                return;
            }
            Some(Err(Refusal::Limit)) => {
                self.refuse(message.id, "limit");
                return;
            }
        };
        if bundle.is_some() && !self.config.bundle {
            self.refuse(message.id, "unsupported");
            return;
        }
        if !self.allow_opens {
            self.refuse(message.id, "policy");
            return;
        }
        let mut stage = Stage::Ready;
        if let Some(manifest) = &bundle {
            let missing = manifest.missing(&self.store);
            let list: Vec<String> = missing.iter().map(ToString::to_string).collect();
            self.send("need", Some(message.id), &message::need(&list));
            if !missing.is_empty() {
                stage = Stage::Files(Receiving::new(manifest, &missing));
            }
        }
        let waiting = matches!(stage, Stage::Files(_));
        // The view is the one this token opened. A refused open, a busy one
        // above all, has left the token of the view that runs where it was.
        self.view_token = spent;
        self.phase = Phase::Opening(Box::new(Opening {
            open: message.id,
            tool: body.tool,
            bundle,
            stage,
            program: None,
        }));
        if !waiting {
            self.begin_mcp();
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
            Phase::Opening(open) => open.bundle.as_ref().is_none_or(|m| m.is_held(&self.store)),
            _ => return,
        };
        // A store may drop an entry at any time (§8.5), including one just kept.
        if held {
            self.begin_mcp();
        } else {
            self.fail("bundle");
        }
    }

    /// The program ended, if it says so with the token that opened the view. The
    /// stream may be written by anyone and `bye` has no answer to read, so a
    /// `bye` that carries no token, another token, or arrives when no view is
    /// opening, open or closing, changes nothing (§10.3, §14).
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
        } else {
            self.ignored();
        }
    }

    /// The program closes its view and keeps running, if it says so with the token
    /// that opened the view (§10.8). The token is the same proof a `bye` gives and
    /// is read the same way; a view that is already closing is being closed, and
    /// the `closed` that ends it is on its way, so a `dismiss` then changes nothing.
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

    /// Writes a JSON-RPC message to the program as a `msg`.
    fn send_json(&mut self, text: &str) -> bool {
        self.send("msg", None, &crate::json::escape(text))
    }

    fn next_rpc(&mut self) -> Id {
        let id = Id::Int(self.rpc);
        self.rpc = self.rpc.saturating_add(1);
        id
    }

    fn next_view_rpc(&mut self) -> Id {
        let id = Id::Int(self.view_rpc);
        self.view_rpc = self.view_rpc.saturating_add(1);
        id
    }

    fn tell_view(&mut self, text: String) {
        self.actions.push(Action::ToView(text));
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

    /// Whatever ends a view (§10.2): the view is told and given a time to answer,
    /// then it is torn down, then the program is told unless it is the one that
    /// ended, then the token is forgotten.
    fn close(&mut self, cause: Cause, re: Option<u32>) {
        match self.take() {
            Phase::Idle => {
                if cause == Cause::Program {
                    self.token = None;
                }
                if cause == Cause::Restart {
                    self.offer_restart();
                }
                self.phase = Phase::Idle;
            }
            Phase::Opening(open) => {
                // No view has run: nothing to tell, nothing to tear down.
                let re = re.or(Some(open.open));
                self.finish(cause, re, false);
            }
            Phase::Live(live) => self.teardown(*live, cause),
            Phase::Closing(mut closing) => {
                if cause == Cause::Program {
                    closing.program_gone = true;
                }
                self.phase = Phase::Closing(closing);
            }
        }
    }

    fn teardown(&mut self, mut live: Live, cause: Cause) {
        // What the view had in flight fails; a call of the tool that was out is
        // cancelled.
        for forward in mem::take(&mut live.forwards) {
            let text = crate::jsonrpc::failure(
                &forward.view,
                crate::jsonrpc::DENIED,
                "the view is closing",
            );
            self.tell_view(text);
        }
        for (_, view) in mem::take(&mut live.links) {
            let text =
                crate::jsonrpc::failure(&view, crate::jsonrpc::DENIED, "the view is closing");
            self.tell_view(text);
        }
        if live.state != ViewState::Ready {
            // The view has not said it is initialized: the host may not send it
            // anything, so it is torn down without a word.
            self.finish(cause, None, true);
            return;
        }
        if live.open_call.take().is_some() {
            let text = crate::jsonrpc::notification(
                "ui/notifications/tool-cancelled",
                Some(serde_json::json!({ "reason": "the view is closing" })),
            );
            self.tell_view(text);
        }
        let request = self.next_view_rpc();
        let reason = match cause {
            Cause::Program => "program-ended",
            Cause::User => "user",
            Cause::View => "view",
            Cause::Failed(_) => "error",
            Cause::Restart => "restart",
            Cause::Dismissed(_) => "dismissed",
        };
        let text = crate::jsonrpc::request(
            &request,
            "ui/resource-teardown",
            Some(serde_json::json!({ "reason": reason })),
        );
        self.tell_view(text);
        self.phase = Phase::Closing(Box::new(Closing {
            cause,
            request,
            program_gone: cause == Cause::Program,
        }));
    }

    /// The view answered its teardown or the wait ended.
    fn finish_close(&mut self) {
        let Some(closing) = self.take_closing() else {
            return;
        };
        let cause = if closing.program_gone {
            Cause::Program
        } else {
            closing.cause
        };
        self.finish(cause, None, true);
    }

    /// Steps 2 to 4 of §10.2.
    ///
    /// Step 4 forgets the view's token and nothing else. A token the host issued
    /// since (the answer to a `hello` that came while the view was opening, open or
    /// closing) belongs to a program that is reading the tty, and so does the end
    /// of the program: `program_ended` has already forgotten what it ended, and a
    /// `hello` after it has already said there is a program again.
    fn finish(&mut self, cause: Cause, re: Option<u32>, destroy: bool) {
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
                    &message::closed(Reason::Program, None),
                );
            }
            Cause::User => {
                self.send("closed", re, &message::closed(Reason::User, None));
            }
            Cause::View => {
                self.send("closed", re, &message::closed(Reason::View, None));
            }
            Cause::Failed(code) => {
                self.send("closed", re, &message::closed(Reason::Error, Some(code)));
            }
        }
        self.view_token = None;
        let again = mem::take(&mut self.restart_after);
        if cause == Cause::Restart || again {
            self.offer_restart();
        }
    }
}
