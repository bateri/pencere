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

use pencere_fake_host::log;
use pencere_fake_host::pane::{self, DEVICE_ATTRIBUTES, Entropy, Pane, Piece, Typing};
use pencere_fake_host::pty::{self, Modes};
use pencere_fake_host::store::{DirStore, Keep};
use pencere_host::bundle::MemoryStore;
use pencere_host::frame::{Direction, Event, Message, StreamDecoder};
use pencere_host::icon::{Icon, IconTheme};
use pencere_host::sandbox::Grant;
use pencere_host::session::{Action, FetchError, Session, StageError, State, ViewStart};
use serde_json::Value;

use crate::args::{Args, Ask, Gestures, Then};
use crate::files::Desk;
use crate::stage::{Finished, Stage};
use crate::view::{Reaction, ScriptedView};

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
    /// How the program's output is read: cut where the host acts in step with it, its frames.
    pane: Pane,
    on_alternate_screen: bool,
    /// The program has left the alternate screen: what it writes next is written on the screen
    /// it gave back, and the modes it leaves the tty in are the ones to look at.
    left_screen: bool,
    modes: Option<Modes>,
    /// What the session wrote into the pane's input, read for the record.
    said: StreamDecoder,
    /// Bytes for the pane's input that the tty has not taken yet, with which a view said.
    typed: Typing,
    kill: bool,
    hang_up: bool,
    /// The view has been started again once, as `--then restart` says.
    restarted: bool,
    waiting_since: Option<(State, Instant)>,
    /// The opens the host refuses, counting from 1, and how many the program has asked for.
    refuse_opens: Vec<u32>,
    opens: u32,
    /// The host closes a view at the program's request.
    dismiss: bool,
    /// What the person gives each view as it starts, and the panels it asks for.
    gestures: Gestures,
    desk: Desk,
    /// How many reads of files the session has asked for.
    reads: u32,
    /// The folder the bytes of files cross through, when the command line asks for one (§8.11).
    stage: Option<Stage>,
}

impl Host {
    pub fn new(args: &Args, size: pty::Size) -> io::Result<Self> {
        let urandom = std::fs::File::open("/dev/urandom")?;
        let store = match &args.store {
            Some(dir) => Keep::Directory(DirStore::open(dir)?),
            None => Keep::Memory(MemoryStore::new()),
        };
        // A terminal that draws views itself opens them as bundles like pages; the stand-in
        // accepts the two kinds and plays their page as a script, with nothing drawn.
        let mut session = Session::new(pane::config(size), store, pane::entropy(urandom));
        session.offer_dismiss(args.dismiss);
        let stage = match &args.stage {
            Some(dir) => {
                let (stage, offered) = Stage::open(dir, args.stage_flaws.clone())?;
                log::line("host", format!("keeps a folder at {}", offered.dir()));
                session.offer_stage(Some(offered));
                Some(stage)
            }
            None => None,
        };
        Ok(Self {
            session,
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
            pane: Pane::new(),
            on_alternate_screen: false,
            left_screen: false,
            modes: None,
            said: StreamDecoder::new(Direction::ToProgram),
            typed: Typing::default(),
            kill: false,
            hang_up: false,
            restarted: false,
            waiting_since: None,
            refuse_opens: args.refuse_opens.clone(),
            opens: 0,
            dismiss: args.dismiss,
            gestures: args.gestures.clone(),
            desk: Desk::default(),
            reads: 0,
            stage,
        })
    }

    // ---- what the loop asks of the host ----------------------------------

    /// Bytes for the pane's input that are waiting for the tty.
    pub fn pending(&self) -> &[u8] {
        self.typed.bytes()
    }

    /// The tty took `count` of them.
    pub fn typed(&mut self, count: usize) {
        self.typed.taken(count);
    }

    /// The person typed these into this terminal; they go to the pane as they
    /// are, after whatever is already waiting.
    pub fn person_typed(&mut self, bytes: &[u8]) {
        self.typed.type_in(bytes);
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

    /// What the program wrote, with what the tty's modes were as it wrote it. Each message in it
    /// is noted and then handed to the session, and what the session queued is done, before the
    /// next one is read: the record is in the order things happen, whatever the tty put in one
    /// read. A host that read the lot and then acted would write the program's `dismiss` ahead of
    /// the view's reaction to the `response` before it.
    pub fn program_output(&mut self, bytes: &[u8], modes: Option<Modes>) {
        self.modes = modes;
        if self.passthrough {
            let mut out = io::stdout().lock();
            let _ = out.write_all(bytes).and_then(|()| out.flush());
        }
        let was_left = self.left_screen;
        let mut switched = false;
        let mut pane = std::mem::take(&mut self.pane);
        let written = pane.read(bytes, |piece| match piece {
            Piece::Frame(event) => self.frame(event),
            Piece::Query => {
                log::line("host", "answers CSI c");
                if self.typed.len() < QUEUE_LIMIT {
                    self.typed.type_in(DEVICE_ATTRIBUTES);
                }
            }
            Piece::Alternate(on) => {
                switched = true;
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
        });
        self.pane = pane;
        // What the program writes after it gave the screen back is written after it gave back
        // the tty's modes too, if it gives them back at all (§4.9): so this is where to look.
        let after = if switched {
            written
        } else {
            was_left && written
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

    /// A frame of the program's (or one that was dropped): noted, then handed to the session, and
    /// what the session queued done.
    fn frame(&mut self, event: Event) {
        self.noted(&event);
        let Event::Message(message) = event else {
            return;
        };
        if !self.answering {
            return;
        }
        self.session
            .observe(pane::observed(self.on_alternate_screen, self.modes));
        // An open the host is told to refuse is refused by its own policy, as a terminal whose
        // person said no would refuse it.
        let refused = message.kind == "open" && {
            self.opens += 1;
            self.refuse_opens.contains(&self.opens)
        };
        if refused {
            log::line("host", format!("refuses open {} by its policy", self.opens));
            self.session.allow_opens(false);
        }
        self.session.message(message);
        if refused {
            self.session.allow_opens(true);
        }
        self.settle();
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
                    if !self.dismiss {
                        log::line("host", "does not close views for the program: ignored");
                    }
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

    /// Puts what the session wrote into the pane's input (a view's words where `view`), and into
    /// the record.
    fn write(&mut self, bytes: &[u8], view: bool) {
        let mut events = Vec::new();
        self.said.feed(bytes, &mut events);
        for event in &events {
            if let Event::Message(message) = event {
                log::line("host", describe_host(message));
            }
        }
        if view {
            self.typed.view_said(bytes);
        } else {
            self.typed.type_in(bytes);
        }
    }

    fn act(&mut self, action: Action) {
        match action {
            Action::WriteView(bytes) => {
                // What a view says is held only up to the bound (§4.8): past it the session
                // closes the view, and what was held of its words and not begun goes.
                if self.session.backlog(self.typed.said() + bytes.len()) {
                    self.write(&bytes, true);
                } else {
                    self.typed.drop_said();
                }
            }
            Action::Write(bytes) => self.write(&bytes, false),
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
            Action::ReadFile {
                read,
                handle,
                offset,
                length,
            } => {
                self.reads += 1;
                if self.gestures.interrupt_at_read == Some(self.reads) {
                    log::line("host", "the person presses ^C");
                    self.typed.type_in(&[INTERRUPT]);
                }
                let length = usize::try_from(length).unwrap_or(usize::MAX);
                let result = self.desk.read(&handle, offset, length);
                self.session.file_read(read, result);
            }
            Action::WriteFile {
                write,
                handle,
                offset,
                bytes,
                end,
            } => {
                let result = self.desk.write(&handle, offset, &bytes, end);
                self.session.file_written(write, result);
            }
            Action::StageFile {
                stage,
                handle,
                offset,
                pipe,
            } => self.stage_file(stage, &handle, offset, &pipe),
            Action::FetchFile {
                fetch,
                handle,
                offset,
                size,
                pipe,
            } => self.fetch_file(fetch, &handle, offset, size, &pipe),
            Action::Unstage { transfer } => {
                log::line("host", format!("takes back the pipe of {transfer}"));
                if let Some(stage) = self.stage.as_mut() {
                    stage.cancel(transfer);
                }
            }
            Action::AbortWrite { handle } => self.desk.abort(&handle),
            Action::ForgetFiles(names) => {
                log::line("view", format!("forgets {} attached files", names.len()));
            }
            Action::Pick { panel, .. } => {
                log::line("host", "shows the Open panel");
                if self.gestures.picks.is_empty() {
                    log::line("person", "dismisses the Open panel");
                }
                for path in self.gestures.picks.clone() {
                    self.desk.give(&mut self.session, "picks", &path);
                }
                self.session.panel_closed(panel);
            }
            Action::Save { panel, name, .. } => {
                log::line("host", format!("shows the Save panel for {name:?}"));
                match self.gestures.save_in.clone() {
                    Some(dir) => self.desk.choose(&mut self.session, &dir, &name),
                    None => log::line("person", "dismisses the Save panel"),
                }
                self.session.panel_closed(panel);
            }
            // An action a later version adds is one this host does not know.
            _ => {}
        }
    }

    /// The session asks for the file of a handle to be sent through a pipe (§8.11).
    fn stage_file(&mut self, transfer: u32, handle: &str, offset: u64, pipe: &str) {
        let source = self.desk.source(handle);
        let started = match (source, self.stage.as_mut()) {
            (Ok((path, size)), Some(stage)) => {
                log::line(
                    "host",
                    format!("stages {} from {offset} through a pipe", name_of(&path)),
                );
                stage.stage_file(transfer, pipe, path, size, offset)
            }
            (Err(_), _) => Err(StageError::Gone),
            (_, None) => Err(StageError::Io),
        };
        match started {
            Ok(()) => {
                self.session.pipe_ready(transfer);
                self.person_may_interrupt();
            }
            Err(error) => self.session.staged(transfer, Err(error)),
        }
    }

    /// The person presses ^C once a pipe is up, when the command line says so.
    fn person_may_interrupt(&mut self) {
        if self.gestures.interrupt_at_pipe {
            log::line("host", "the person presses ^C");
            self.typed.type_in(&[INTERRUPT]);
        }
    }

    /// The session asks for a file to be taken in from a pipe, into the place of a handle (§8.11).
    fn fetch_file(&mut self, transfer: u32, handle: &str, offset: u64, size: u64, pipe: &str) {
        let started = match (self.desk.place_of(handle), self.stage.as_mut()) {
            (Some(place), Some(stage)) => {
                log::line(
                    "host",
                    format!("fetches {} from {offset} through a pipe", name_of(&place.0)),
                );
                stage.fetch_file(transfer, pipe, place, size, offset)
            }
            _ => Err(FetchError::Io),
        };
        match started {
            Ok(()) => {
                self.session.pipe_ready(transfer);
                self.person_may_interrupt();
            }
            Err(error) => self.session.fetched(transfer, Err(error)),
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
        // What the person does with files as the view starts: the handles it is given are the ones
        // the page's calls name as `$1`, `$2`, …
        self.desk.clear();
        for (how, path) in self.gestures.gives.clone() {
            self.desk.give(&mut self.session, how.word(), &path);
        }
        if self.gestures.open_panel {
            log::line("view", "asks for the Open panel");
            self.session.module_pick(self.gestures.picks.len() > 1);
            self.settle();
        }
        if let Some(name) = self.gestures.save.clone() {
            log::line("view", format!("asks for the Save panel for {name:?}"));
            self.session.module_save(&name, None);
            self.settle();
        }
        let mut calls = self.calls.clone();
        for call in &mut calls {
            if let Some(params) = call.params.as_mut() {
                self.desk.fill(params);
            }
        }
        let mut view = ScriptedView::new(
            calls,
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
        // A link is the host's, not a message to the program: a web page asks with its bridge's
        // `link`, a view the terminal draws itself with the `link` of a reply, and the session
        // holds both to the same rule and asks the person the same way.
        for url in &reaction.links {
            if self.native {
                self.session.module_link(url);
            } else {
                self.session
                    .view_message(&serde_json::json!({ "link": { "url": url } }).to_string());
            }
        }
        // A page's wish to be closed: a web page's is the bridge's `close`, which the session ends
        // the view for (the program told `closed` with reason `view`, §9.6). A view the terminal
        // draws itself has no message of the host's own (§10.6): its reply's `close` is the
        // terminal's to take or not, and a program's view is the program's to end — so it is not
        // taken, and nothing of it reaches the program, which would read it as a message of the
        // six.
        if reaction.close {
            if self.native {
                log::line(
                    "host",
                    "keeps the view: a view the terminal draws itself is its program's to end",
                );
            } else {
                self.session
                    .view_message(&serde_json::json!({ "close": {} }).to_string());
            }
        }
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
                self.typed.type_in(&[INTERRUPT]);
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
                self.typed.type_in(&[INTERRUPT]);
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
        // What the transfers through the folder's pipes came to.
        let finished = self.stage.as_mut().map(Stage::take_finished);
        for finished in finished.into_iter().flatten() {
            match finished {
                Finished::Staged { transfer, result } => self.session.staged(transfer, result),
                Finished::Fetched { transfer, result } => self.session.fetched(transfer, result),
            }
            self.settle();
        }
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

/// The last part of a path, as the record shows a file.
fn name_of(path: &std::path::Path) -> String {
    path.file_name().map_or_else(
        || "?".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
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
        "open" | "bye" | "attach" | "detach" | "read" | "stage" | "fetch" | "unstage" => {
            parsed(&message.payload)
        }
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
        "attach" | "detach" => format!(
            "{} id={id} files={}",
            message.kind,
            payload
                .get("files")
                .and_then(Value::as_array)
                .map_or(0, Vec::len)
        ),
        "read" => format!(
            "read id={id} offset={} length={}",
            payload.get("offset").and_then(Value::as_u64).unwrap_or(0),
            payload.get("length").and_then(Value::as_u64).unwrap_or(0)
        ),
        "stage" => format!(
            "stage id={id} offset={}",
            payload.get("offset").and_then(Value::as_u64).unwrap_or(0)
        ),
        "fetch" => format!(
            "fetch id={id} size={} offset={}",
            payload.get("size").and_then(Value::as_u64).unwrap_or(0),
            payload.get("offset").and_then(Value::as_u64).unwrap_or(0)
        ),
        "unstage" => format!(
            "unstage id={id} of={}",
            payload.get("id").and_then(Value::as_u64).unwrap_or(0)
        ),
        "write" => format!(
            "write id={id} offset={}{} {} bytes",
            message.offset().unwrap_or(0),
            if message.end() { " end" } else { "" },
            base64_length(&message.payload)
        ),
        kind => format!("{kind} id={id}"),
    }
}

/// How many bytes base64 text of this length holds.
fn base64_length(text: &str) -> usize {
    let padding = text.bytes().rev().take_while(|&b| b == b'=').count();
    (text.len() / 4 * 3).saturating_sub(padding)
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
            let refused: Vec<&str> = payload
                .get("refused")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(|r| payload_str(r, "code")).collect())
                .unwrap_or_default();
            if refused.is_empty() {
                format!("need id={id}{re} missing={missing}")
            } else {
                format!(
                    "need id={id}{re} missing={missing} refused={}",
                    refused.join(",")
                )
            }
        }
        "data" => match message.error_code() {
            Some(code) => format!("data id={id}{re} e={code}"),
            None => format!("data id={id}{re} {} bytes", base64_length(&message.payload)),
        },
        "staged" | "fetched" => {
            let kind = &message.kind;
            if payload.get("pipe").is_some() {
                format!("{kind} id={id}{re} pipe")
            } else if let Some(code) = payload_str(&payload, "code") {
                match payload.get("at").and_then(Value::as_u64) {
                    Some(at) => format!("{kind} id={id}{re} code={code} at={at}"),
                    None => format!("{kind} id={id}{re} code={code}"),
                }
            } else {
                let done = payload
                    .get("sent")
                    .or_else(|| payload.get("saved"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let word = if kind == "staged" { "sent" } else { "saved" };
                format!("{kind} id={id}{re} {word}={done}")
            }
        }
        "wrote" => match payload_str(&payload, "code") {
            Some(code) => format!("wrote id={id}{re} code={code}"),
            None => format!("wrote id={id}{re}"),
        },
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
