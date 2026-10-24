//! The host's side of one pane, around the `pencere` crate.
//!
//! The crate holds every rule: who may open a view, what is refused, which files
//! are missing, what the view is told, what ends it. What it does not do, this
//! does as a terminal does: it tells the session what the program wrote and what
//! it can see of the pane (the alternate screen, the tty's modes), carries out
//! what the session asks (write into the pane, start and destroy a view, ask the
//! person), and keeps the clock the crate does not have. The view is a script
//! (`view`), and the program's fate comes from the caller (`program_ended`).

use std::io::{self, Write};
use std::time::{Duration, Instant};

use pencere_host::bundle::MemoryStore;
use pencere_host::frame::{Direction, Event, Message, StreamDecoder};
use pencere_host::icon::{Icon, IconTheme};
use pencere_host::message::HostInfo;
use pencere_host::sandbox::Grant;
use pencere_host::session::{
    Action, Config, Observed, Offer, Session, Size, State, ViewKind, ViewStart,
};
use pencere_host::theme::{Rgb, Theme};
use serde_json::Value;

use crate::args::{Args, Ask, Then};
use crate::log;
use crate::pty::{self, Modes};
use crate::screen::{Screen, Seen};
use crate::store::{DirStore, Keep};
use crate::view::{Reaction, ScriptedView};

/// What a terminal answers to `CSI c`.
const DEVICE_ATTRIBUTES: &[u8] = b"\x1b[?62;4c";

/// The keystroke the person ends a program with.
const INTERRUPT: u8 = 0x03;

/// How long a terminal waits for the files and the start-up of an open before it
/// gives up on it. The crate has no clock: this is the host's.
const OPENING_PATIENCE: Duration = Duration::from_secs(5);

/// The most bytes waiting for the pane's input before replies to `CSI c` stop
/// being queued: a program that asks without ever reading would otherwise grow
/// the queue as long as it likes.
const QUEUE_LIMIT: usize = 1 << 20;

/// The most bytes of a payload the record reads. The crate reads a hostile stream
/// with care; a line of the record is not worth a tree of sixteen mebibytes.
const READ_LIMIT: usize = 64 * 1024;

/// The version of the interface of a view the terminal draws itself (a WebAssembly component or a
/// Lua script) that the stand-in speaks. It draws nothing: what a program is tried against is the
/// conversation.
const KIT_VERSION: &str = "1.0";

/// A pixel is not a cell, but the view's container is told in pixels.
const CELL_WIDTH: u32 = 8;
const CELL_HEIGHT: u32 = 16;

type Entropy = Box<dyn FnMut() -> [u8; 16]>;

/// The operating system's random bytes, which the token is made of, from a
/// descriptor opened before anything runs: a host that cannot get them has no
/// token to give and does not start. Reading a descriptor that is open does not
/// fail; if it ever did, stopping is better than a token that is not random.
fn secure_bytes(mut urandom: std::fs::File) -> Entropy {
    use std::io::Read;
    Box::new(move || {
        let mut bytes = [0u8; 16];
        if let Err(error) = urandom.read_exact(&mut bytes) {
            log::line(
                "host",
                format!("no secure random bytes for a token: {error}"),
            );
            std::process::exit(70);
        }
        bytes
    })
}

pub struct Host {
    session: Session<Keep, Entropy>,
    answering: bool,
    allow_network: bool,
    passthrough: bool,
    then: Then,
    /// The view whose finished script `then` waits for.
    on_view: u32,
    /// How many views have been started: each reopening of the program's is one.
    views: u32,
    /// What the page of each view sends.
    calls: Vec<Ask>,
    subscriptions: Vec<Ask>,
    links: Vec<String>,
    cancel_after: Option<u32>,
    view: Option<ScriptedView>,
    /// The view that runs is one the terminal draws itself, and its messages are the module's.
    native: bool,
    screen: Screen,
    on_alternate_screen: bool,
    /// The program has left the alternate screen: what it writes next is written on the screen
    /// it gave back, and the modes it leaves the tty in are the ones to look at.
    left_screen: bool,
    modes: Option<Modes>,
    /// What the program's output holds, read once more for the record.
    heard: StreamDecoder,
    /// What the session wrote into the pane's input, read for the record.
    said: StreamDecoder,
    /// Bytes for the pane's input that the tty has not taken yet. Everything
    /// the pane is typed goes through here, so that nothing lands inside a frame
    /// (specification §4.8).
    typed: Vec<u8>,
    kill: bool,
    hang_up: bool,
    /// The view has been started again once, as `--then restart` says.
    restarted: bool,
    waiting_since: Option<(State, Instant)>,
}

impl Host {
    pub fn new(args: &Args, size: pty::Size) -> io::Result<Self> {
        let urandom = std::fs::File::open("/dev/urandom")?;
        let store = match &args.store {
            Some(dir) => Keep::Directory(DirStore::open(dir)?),
            None => Keep::Memory(MemoryStore::new()),
        };
        let theme = Theme::new(
            Rgb(0x11, 0x13, 0x18),
            Rgb(0xd8, 0xdb, 0xe2),
            "Menlo, monospace",
        );
        let host = HostInfo {
            name: "pencere-fake-host".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        };
        let mut config = Config::new(
            host,
            theme,
            Size {
                width: u32::from(size.cols) * CELL_WIDTH,
                height: u32::from(size.rows) * CELL_HEIGHT,
            },
        );
        // A terminal that draws views itself opens them as bundles like pages; the stand-in
        // accepts the two kinds and plays their page as a script, with nothing drawn.
        config
            .views
            .push(Offer::new(ViewKind::Wasm, &[KIT_VERSION]));
        config.views.push(Offer::new(ViewKind::Lua, &[KIT_VERSION]));
        Ok(Self {
            session: Session::new(config, store, secure_bytes(urandom)),
            answering: args.answer,
            allow_network: args.allow_network,
            passthrough: args.passthrough,
            then: args.then,
            on_view: args.on_view,
            views: 0,
            calls: args.calls.clone(),
            subscriptions: args.subscriptions.clone(),
            links: args.links.clone(),
            cancel_after: args.cancel_after,
            view: None,
            native: false,
            screen: Screen::new(),
            on_alternate_screen: false,
            left_screen: false,
            modes: None,
            heard: StreamDecoder::new(Direction::ToHost),
            said: StreamDecoder::new(Direction::ToProgram),
            typed: Vec::new(),
            kill: false,
            hang_up: false,
            restarted: false,
            waiting_since: None,
        })
    }

    // ---- what the loop asks of the host ----------------------------------

    /// Bytes for the pane's input that are waiting for the tty.
    pub fn pending(&self) -> &[u8] {
        &self.typed
    }

    /// The tty took `count` of them.
    pub fn typed(&mut self, count: usize) {
        self.typed.drain(..count.min(self.typed.len()));
    }

    /// The person typed these into this terminal; they go to the pane as they
    /// are, after whatever is already waiting.
    pub fn person_typed(&mut self, bytes: &[u8]) {
        self.typed.extend_from_slice(bytes);
    }

    /// Whether the host wants the program killed, once.
    pub fn take_kill(&mut self) -> bool {
        std::mem::take(&mut self.kill)
    }

    /// Whether the host wants the terminal gone, once.
    pub fn take_hang_up(&mut self) -> bool {
        std::mem::take(&mut self.hang_up)
    }

    // ---- the program's output --------------------------------------------

    /// What the program wrote, with what the tty's modes were as it wrote it.
    pub fn program_output(&mut self, bytes: &[u8], modes: Option<Modes>) {
        self.modes = modes;
        if self.passthrough {
            let mut out = io::stdout().lock();
            let _ = out.write_all(bytes).and_then(|()| out.flush());
        }
        // The stream is cut where the terminal must act in step with it.
        let was_left = self.left_screen;
        let mut start = 0;
        for (at, &byte) in bytes.iter().enumerate() {
            let Some(seen) = self.screen.advance(byte) else {
                continue;
            };
            if let Some(part) = bytes.get(start..=at) {
                self.segment(part);
            }
            start = at + 1;
            match seen {
                Seen::Query => {
                    log::line("host", "answers CSI c");
                    if self.typed.len() < QUEUE_LIMIT {
                        self.typed.extend_from_slice(DEVICE_ATTRIBUTES);
                    }
                }
                Seen::Alternate(on) => {
                    self.on_alternate_screen = on;
                    self.left_screen = !on;
                    log::line(
                        "screen",
                        if on {
                            "alternate screen on"
                        } else {
                            "alternate screen off"
                        },
                    );
                }
            }
        }
        if let Some(rest) = bytes.get(start..) {
            self.segment(rest);
        }
        // What the program writes after it gave the screen back is written after it gave back
        // the tty's modes too, if it gives them back at all (§4.9): so this is where to look.
        let after = if start == 0 {
            was_left && !bytes.is_empty()
        } else {
            bytes.len() > start
        };
        if self.left_screen && after {
            match modes {
                Some(m) => log::line(
                    "host",
                    format!(
                        "sees the tty after the screen: canonical={} echo={}",
                        m.canonical, m.echo
                    ),
                ),
                None => log::line("host", "cannot read the tty's modes after the screen"),
            }
        }
    }

    /// A piece of the program's output, cut where the terminal must act in step with it. Each
    /// message in it is noted and then handed to the session, and what the session queued is
    /// done, before the next one is read: the record is in the order things happen, whatever
    /// the tty put in one read. A host that read the lot and then acted would write the
    /// program's `dismiss` ahead of the view's reaction to the `response` before it.
    fn segment(&mut self, part: &[u8]) {
        if part.is_empty() {
            return;
        }
        let mut events = Vec::new();
        self.heard.feed(part, &mut events);
        for event in events {
            self.noted(&event);
            let Event::Message(message) = event else {
                continue;
            };
            if !self.answering {
                continue;
            }
            // What the host can see of the pane, for the checks of §6.2 and §6.5:
            // there is no shell here to ask about, so only the modes and the screen.
            self.session.observe(Observed {
                prompt_editing: false,
                alternate_screen: Some(self.on_alternate_screen),
                shell_in_foreground: None,
                tty_canonical_or_echo: self.modes.map(|m| m.canonical || m.echo),
            });
            self.session.message(message);
            self.settle();
        }
    }

    /// The program's frames, for the record.
    fn noted(&self, event: &Event) {
        match event {
            Event::Error(error) => log::line("program", format!("dropped a frame ({error})")),
            Event::Message(message) => {
                log::line("program", describe_program(message));
                if message.kind == "hello" {
                    match self.modes {
                        Some(m) => log::line(
                            "host",
                            format!("sees the tty: canonical={} echo={}", m.canonical, m.echo),
                        ),
                        None => log::line("host", "cannot read the tty's modes"),
                    }
                    if !self.answering {
                        log::line("host", "does not host pencere: no answer");
                    }
                }
                if message.kind == "dismiss" {
                    log::line("host", "the program asks to close its view");
                }
            }
        }
    }

    // ---- what the session asks -------------------------------------------

    /// Carries out what the session queued, and what that queues, until nothing
    /// is left.
    fn settle(&mut self) {
        loop {
            let actions = self.session.take_actions();
            if actions.is_empty() {
                return;
            }
            for action in actions {
                self.act(action);
            }
        }
    }

    fn act(&mut self, action: Action) {
        match action {
            Action::Write(bytes) => {
                let mut events = Vec::new();
                self.said.feed(&bytes, &mut events);
                for event in &events {
                    if let Event::Message(message) = event {
                        log::line("host", describe_host(message));
                    }
                }
                self.typed.extend_from_slice(&bytes);
            }
            Action::Consent(consent) => {
                let grant = if self.allow_network {
                    consent.grant_all()
                } else {
                    Grant::none()
                };
                // The question names the program that asks, in the words the view
                // will start with.
                log::line(
                    "host",
                    format!(
                        "asked the person about origins={} permissions={} of program={} title={}: {}",
                        consent.domains.len(),
                        consent.permissions.len(),
                        said(&consent.program),
                        said(&consent.title),
                        if self.allow_network {
                            "allowed"
                        } else {
                            "refused"
                        },
                    ),
                );
                self.session.consent(consent.id, grant);
            }
            // The stand-in answers a question the moment it is asked, so there is none on
            // the screen to take back; a terminal that asks the person takes it away here.
            Action::WithdrawConsent(id) => {
                log::line("host", format!("took back question {id}"));
            }
            Action::StartView(start) => self.start_view(&start),
            Action::ToView(text) => {
                let reaction = self.view.as_mut().map(|view| view.receive(&text));
                if let Some(reaction) = reaction {
                    self.react(reaction);
                }
            }
            Action::OpenLink { link, url } => {
                log::line("host", format!("refuses to open {url} for the view"));
                self.session.link_decision(link, false);
            }
            Action::DestroyView => {
                log::line("view", "destroyed");
                self.view = None;
            }
            // An action a later version adds is one this host does not know.
            _ => {}
        }
    }

    fn start_view(&mut self, start: &ViewStart) {
        self.views += 1;
        self.native = start.kind.is_native();
        let manifest = self.session.bundle().cloned();
        if let Some(manifest) = manifest {
            log::line(
                "view",
                format!(
                    "started from the bundle: entry {}, {} files",
                    manifest.entry(),
                    manifest.files().len()
                ),
            );
            // What a web view would ask for, served from the store.
            for file in manifest.files() {
                match self.session.serve(&file.path) {
                    Some((media_type, bytes)) => log::line(
                        "view",
                        format!("fetched {} {media_type} {} bytes", file.path, bytes.len()),
                    ),
                    None => log::line("view", format!("could not fetch {}", file.path)),
                }
            }
        }
        log::line(
            "view",
            format!("kind={} version={}", start.kind.name(), start.version),
        );
        // What a terminal would put in the strip above the view: the program's own
        // words for the view and for itself, already cut and cleaned by the session.
        log::line(
            "view",
            format!(
                "title={} program={}",
                said(&start.title),
                said(&start.program)
            ),
        );
        // And the pictures it would draw beside them, as the session took them.
        log::line("view", format!("icons {}", drawn(&start.icons)));
        log::line("view", format!("policy {}", start.policy));
        let mut view = ScriptedView::new(
            self.calls.clone(),
            self.subscriptions.clone(),
            self.links.clone(),
            self.cancel_after,
        );
        // The page has loaded: it is told the context, and begins.
        let reaction = view.loaded(start.context.as_deref());
        self.view = Some(view);
        self.react(reaction);
    }

    fn react(&mut self, reaction: Reaction) {
        for message in &reaction.send {
            // The page of a web view talks through the bridge, the module of a view the terminal
            // draws itself through the session's other door; the messages are the same six.
            if self.native {
                self.session.module_message(message);
            } else {
                self.session.view_message(message);
            }
        }
        if reaction.done {
            self.calls_done();
        }
    }

    /// The script is done: what the command line said to do next.
    fn calls_done(&mut self) {
        // A program that shows its view again is waited for, whatever the action is.
        if self.views < self.on_view {
            log::line("view", "has nothing more to do");
            return;
        }
        match self.then {
            Then::Wait => log::line("view", "has nothing more to do"),
            Then::Interrupt => {
                log::line("host", "the person presses ^C");
                self.typed.push(INTERRUPT);
            }
            Then::Kill => {
                log::line("host", "kills the program with SIGKILL");
                self.kill = true;
            }
            Then::Hangup => {
                log::line("host", "hangs up the terminal");
                self.hang_up = true;
            }
            Then::Close => {
                log::line("host", "the person closes the view");
                self.session.close_view();
            }
            Then::PageClose => {
                let reaction = self.view.as_mut().map(ScriptedView::close);
                if let Some(reaction) = reaction {
                    self.react(reaction);
                }
            }
            Then::Restart if self.restarted => {
                log::line("host", "the person presses ^C");
                self.typed.push(INTERRUPT);
            }
            Then::Restart => {
                self.restarted = true;
                log::line("host", "starts the view again, as after a reconnection");
                self.session.restart();
            }
        }
    }

    // ---- the clock the crate does not have --------------------------------

    /// Gives up on what has been waited for too long.
    pub fn tick(&mut self, now: Instant) {
        let state = self.session.state();
        let patience = match state {
            State::Opening => OPENING_PATIENCE,
            _ => {
                self.waiting_since = None;
                return;
            }
        };
        match self.waiting_since {
            Some((waiting, since)) if waiting == state => {
                if now.duration_since(since) >= patience {
                    log::line("host", format!("gave up waiting while {state:?}"));
                    self.waiting_since = None;
                    self.session.timed_out();
                    self.settle();
                }
            }
            _ => self.waiting_since = Some((state, now)),
        }
    }

    /// The program is gone: the stream ended and the process with it. Whatever
    /// was open is closed, as §10.3 says, whether or not it said goodbye.
    pub fn program_ended(&mut self) {
        self.session.program_ended();
        self.settle();
    }
}

// ---- the record ------------------------------------------------------------

/// A name the session cleaned, quoted as the record shows it; `none` when the
/// program gave none.
fn said(text: &Option<String>) -> String {
    text.as_ref()
        .map_or("none".to_owned(), |t| format!("{t:?}"))
}

/// Icons the session took, as the record shows them (`png 32x32 dark`, comma
/// separated); `none` when it took none.
fn drawn(icons: &[Icon]) -> String {
    if icons.is_empty() {
        return "none".to_owned();
    }
    let one = |icon: &Icon| {
        let format = icon.format.media_type().trim_start_matches("image/");
        let theme = match icon.theme {
            None => "",
            Some(IconTheme::Light) => " light",
            Some(IconTheme::Dark) => " dark",
        };
        format!("{format} {}x{}{theme}", icon.width, icon.height)
    };
    icons.iter().map(one).collect::<Vec<_>>().join(",")
}

/// A payload as JSON, if it is small enough to be worth reading.
fn parsed(payload: &str) -> Value {
    if payload.len() > READ_LIMIT {
        return Value::Null;
    }
    serde_json::from_str(payload).unwrap_or(Value::Null)
}

/// The most characters of a name the program wrote that the record of its `open` shows.
const NAME_LIMIT: usize = 60;

/// What a message of the six is, in a few words: its member with the id inside it, and the method
/// or the topic it names, or whether a response is a result or an error.
fn describe_message(payload: &str) -> String {
    let message = parsed(payload);
    let member = message
        .as_object()
        .filter(|members| members.len() == 1)
        .and_then(|members| members.iter().next());
    let Some((name, body)) = member else {
        return format!("message of {} bytes", payload.len());
    };
    let id = body
        .get("id")
        .and_then(Value::as_u64)
        .map_or("?".to_owned(), |id| id.to_string());
    let detail = match name.as_str() {
        "call" => named(body, "method"),
        "subscribe" => named(body, "topic"),
        "response" if body.get("error").is_some() => " error".to_owned(),
        "response" => " result".to_owned(),
        _ => String::new(),
    };
    format!("{name}={id}{detail}")
}

/// ` name`, for a member of a message that names what it asks for.
fn named(body: &Value, key: &str) -> String {
    body.get(key)
        .and_then(Value::as_str)
        .map_or_else(String::new, |name| format!(" {name}"))
}

fn payload_str<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload.get(key).and_then(Value::as_str)
}

/// A message of the program, without the secrets in it (the token, the bytes of
/// files).
fn describe_program(message: &Message) -> String {
    let id = message.id;
    // Only the kinds that are read for what they say are parsed.
    let payload = match message.kind.as_str() {
        "open" | "bye" => parsed(&message.payload),
        _ => Value::Null,
    };
    match message.kind.as_str() {
        "hello" => format!("hello id={id}"),
        "open" => {
            let view = payload_str(&payload, "view").unwrap_or("?");
            let version = payload_str(&payload, "version").unwrap_or("?");
            // What the program calls its view is its own text: the record writes it out.
            let title = payload_str(&payload, "title").map_or_else(String::new, |title| {
                let cut: String = title.chars().take(NAME_LIMIT).collect();
                format!(" title={cut:?}")
            });
            let access = if payload.get("access").is_some() {
                " with access"
            } else {
                ""
            };
            let files = payload
                .pointer("/bundle/files")
                .and_then(Value::as_array)
                .map(Vec::len);
            match files {
                Some(files) => {
                    format!(
                        "open id={id} view={view} version={version}{title} bundle of {files} files{access}"
                    )
                }
                None => format!(
                    "open id={id} view={view} version={version}{title} without a bundle{access}"
                ),
            }
        }
        "file" => {
            let digest = message.digest().unwrap_or("?");
            format!("file id={id} {}", digest.get(..8).unwrap_or(digest))
        }
        "msg" => format!("msg id={id} {}", describe_message(&message.payload)),
        "bye" => format!(
            "bye id={id} reason={}",
            payload_str(&payload, "reason").unwrap_or("?")
        ),
        kind => format!("{kind} id={id}"),
    }
}

/// The kinds of view a `welcome` lists, with their versions: `web@1.0`.
fn offered(payload: &Value) -> String {
    let list: Vec<String> = payload
        .get("views")
        .and_then(Value::as_array)
        .map(|views| {
            views
                .iter()
                .map(|view| {
                    let versions: Vec<&str> = view
                        .get("versions")
                        .and_then(Value::as_array)
                        .map(|list| list.iter().filter_map(Value::as_str).collect())
                        .unwrap_or_default();
                    format!(
                        "{}@{}",
                        payload_str(view, "kind").unwrap_or("?"),
                        versions.join("|")
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    list.join(",")
}

/// A message the host wrote into the pane's input.
fn describe_host(message: &Message) -> String {
    let id = message.id;
    let re = message
        .re()
        .map(|re| format!(" re={re}"))
        .unwrap_or_default();
    let payload: Value = serde_json::from_str(&message.payload).unwrap_or(Value::Null);
    match message.kind.as_str() {
        "welcome" => {
            if payload.get("version").is_some() {
                let restart = if payload.get("restart") == Some(&Value::Bool(true)) {
                    " (restart)"
                } else {
                    ""
                };
                let version = payload.get("version").and_then(Value::as_u64).unwrap_or(0);
                let features: Vec<&str> = payload
                    .get("features")
                    .and_then(Value::as_array)
                    .map(|list| list.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                format!(
                    "welcome id={id}{re} version={version} with a token{restart} features={} views={}",
                    features.join(","),
                    offered(&payload)
                )
            } else {
                format!("welcome id={id}{re} (no common version)")
            }
        }
        "need" => {
            let missing = payload
                .get("missing")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            format!("need id={id}{re} missing={missing}")
        }
        "closed" => {
            let reason = payload_str(&payload, "reason").unwrap_or("?");
            match payload_str(&payload, "code") {
                Some(code) => format!("closed id={id}{re} reason={reason} code={code}"),
                None => format!("closed id={id}{re} reason={reason}"),
            }
        }
        "msg" => format!("msg id={id} {}", describe_message(&message.payload)),
        kind => format!("{kind} id={id}{re}"),
    }
}
