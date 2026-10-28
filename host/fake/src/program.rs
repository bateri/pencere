//! A program on a pty whose view is run for real: the program started as a terminal starts it, the
//! terminal's side of the conversation held by `pencere-host`'s session as the stand-in host holds
//! it ([`crate::pane`]), and the view it opens — a Lua script or a WebAssembly component — run in the
//! kit's sandbox under the drive's host with no screen (`pencere_drive::headless`), its page
//! speaking to the program through the session. A program and its page are tested together, as a
//! terminal shows them:
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut page = pencere_fake_host::program::open(&["target/debug/containers-rust", "--lua"])?
//!     .size(900.0, 500.0)
//!     .open()?;
//! assert!(page.says("Containers"));
//! page.press("down")?;
//! # Ok(())
//! # }
//! ```
//!
//! The host is a terminal's in every way but the screen: the program's `hello` is welcomed, its
//! `open` checked and its files verified by the session, its messages carried to the page and the
//! page's to it. Of them it reads the name of their one member and the number they belong to, and
//! nothing of what they say, to know which of the page's calls the program still owes an answer
//! (a `call` out, no `response` and no `cancel` of it yet), which the host with no screen waits for. A view the page asks to close is not closed by
//! it: a program's view is the program's to end. The program is killed when the page is let go of.

use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pencere_drive::headless::{Headless, Opening, Peer};
use pencere_host::bundle::MemoryStore;
use pencere_host::frame::Event;
use pencere_host::sandbox::Grant;
use pencere_host::session::{Action, Session, ViewKind};
use pencere_sandbox::Assets;

use crate::pane::{self, DEVICE_ATTRIBUTES, Entropy, Pane, Piece, Typing};
use crate::pty::{self, Modes, Output, Pty};

/// The most reads of the pane in one turn.
const READS: usize = 64;

/// The size of the pane the program is started in.
const PANE: pty::Size = pty::Size {
    cols: 120,
    rows: 40,
};

/// What went wrong before the page was up.
#[derive(Debug)]
pub enum Error {
    /// The program could not be started, or its terminal could not be set up.
    Start(std::io::Error),
    /// The program opened no view the terminal draws itself, and why.
    NoView(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Start(why) => write!(f, "the program could not be started: {why}"),
            Self::NoView(why) => write!(f, "the program opened no view of the kit: {why}"),
        }
    }
}

impl std::error::Error for Error {}

/// The files of a view's bundle, as the session verified them: what its scripts require and its
/// pictures are read from.
#[derive(Clone, Debug, Default)]
pub struct Bundle(Arc<HashMap<String, Served>>);

/// A file as the session serves it: its media type and its bytes.
type Served = (String, Arc<[u8]>);

impl Assets for Bundle {
    fn read(&self, path: &str) -> Option<Arc<[u8]>> {
        self.0.get(path).map(|(_, bytes)| Arc::clone(bytes))
    }

    fn media_type(&self, path: &str) -> Option<String> {
        self.0.get(path).map(|(media, _)| media.clone())
    }
}

/// A view the program opened: what it is made of, the path of its entry, and its files.
#[derive(Clone, Debug)]
pub struct Started {
    pub kind: ViewKind,
    pub entry: String,
    pub files: Bundle,
}

/// A program on a pty, its terminal's session, and what it said to its view: the program a page
/// under the host with no screen speaks to ([`Peer`]).
pub struct Program {
    pty: Pty,
    session: Session<MemoryStore, Entropy>,
    pane: Pane,
    alternate: bool,
    modes: Option<Modes>,
    /// Bytes for the program's input that the tty has not taken yet, with which the view said.
    typed: Typing,
    /// What the program said to its view that the page has not been told.
    said: Vec<String>,
    started: Option<Started>,
    /// Why the view is over, once it is.
    closed: Option<String>,
    ended: bool,
    /// The page's calls the program has not answered yet, by their numbers.
    owed: BTreeSet<u64>,
    buffer: Vec<u8>,
}

/// The name of the one member of a message and the number it belongs to: all a host reads of the
/// messages of a program and its view, to know which of the page's calls the program owes an
/// answer to.
fn heading(text: &str) -> Option<(String, Option<u64>)> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let members = value.as_object()?;
    if members.len() != 1 {
        return None;
    }
    let (name, body) = members.iter().next()?;
    Some((
        name.clone(),
        body.get("id").and_then(serde_json::Value::as_u64),
    ))
}

/// Starts `command` on a pty as a terminal would, waits for it to open a view the terminal draws
/// itself, and makes ready that view's page under the host with no screen, the program its peer:
/// the test sets the page up (its size, what measures it) and opens it.
///
/// # Errors
///
/// The program could not be started, or opened no view of the kit within a minute.
pub fn open(command: &[&str]) -> Result<Opening<Program>, Error> {
    let mut program = Program::start(command)?;
    let started = program.view(Duration::from_secs(60))?;
    Ok(match started.kind {
        ViewKind::Lua => Headless::bundle(started.files, &started.entry).peer(program),
        ViewKind::Wasm => {
            let bytes = started
                .files
                .read(&started.entry)
                .ok_or_else(|| Error::NoView(format!("its entry {} is missing", started.entry)))?;
            Headless::component(bytes.to_vec())
                .assets(started.files)
                .peer(program)
        }
        other => {
            return Err(Error::NoView(format!(
                "it opened a view of the kind {}",
                other.name()
            )));
        }
    })
}

impl Program {
    /// `command` started on a pty of its own, with a terminal's session around it.
    ///
    /// # Errors
    ///
    /// The program could not be started, or there are no random bytes for its token.
    pub fn start(command: &[&str]) -> Result<Self, Error> {
        let command: Vec<String> = command.iter().map(|part| (*part).to_owned()).collect();
        let urandom = File::open("/dev/urandom").map_err(Error::Start)?;
        let pty = Pty::spawn(&command, PANE).map_err(Error::Start)?;
        Ok(Self {
            pty,
            session: Session::new(
                pane::config(PANE),
                MemoryStore::new(),
                pane::entropy(urandom),
            ),
            pane: Pane::new(),
            alternate: false,
            modes: None,
            typed: Typing::default(),
            said: Vec::new(),
            started: None,
            closed: None,
            ended: false,
            owed: BTreeSet::new(),
            buffer: vec![0; 64 * 1024],
        })
    }

    /// Waits, at most `within`, for the program to open a view the terminal draws itself.
    ///
    /// # Errors
    ///
    /// The program ended, opened another kind of view, or opened none within the time.
    pub fn view(&mut self, within: Duration) -> Result<Started, Error> {
        let deadline = Instant::now() + within;
        loop {
            if let Some(started) = &self.started {
                return Ok(started.clone());
            }
            if let Some(why) = &self.closed {
                return Err(Error::NoView(why.clone()));
            }
            if Instant::now() >= deadline {
                return Err(Error::NoView(format!("none in {within:?}")));
            }
            self.turn(Duration::from_millis(20));
        }
    }

    /// One turn of the terminal: what waits for the program's input written, what it wrote read
    /// (after waiting at most `wait` for it) and done.
    fn turn(&mut self, wait: Duration) {
        if !self.typed.is_empty() {
            match self.pty.write(self.typed.bytes()) {
                Ok(taken) => self.typed.taken(taken),
                // The program is gone: nobody reads what is left.
                Err(_) => self.typed.clear(),
            }
        }
        let wait = i32::try_from(wait.as_millis()).unwrap_or(i32::MAX);
        if wait > 0 {
            pty::wait(&self.pty, !self.typed.is_empty(), false, wait);
        }
        let mut buffer = std::mem::take(&mut self.buffer);
        for _ in 0..READS {
            match self.pty.read(&mut buffer) {
                Ok(Output::Bytes(count)) => {
                    let modes = self.pty.modes();
                    if let Some(bytes) = buffer.get(..count) {
                        self.output(bytes, modes);
                    }
                }
                Ok(Output::Nothing) => break,
                Ok(Output::End) | Err(_) => {
                    self.end();
                    break;
                }
            }
        }
        self.buffer = buffer;
        if !self.ended && self.pty.finished().is_some() {
            self.end();
        }
    }

    /// What the program wrote, read as a terminal reads it.
    fn output(&mut self, bytes: &[u8], modes: Option<Modes>) {
        self.modes = modes;
        let mut pane = std::mem::take(&mut self.pane);
        pane.read(bytes, |piece| match piece {
            Piece::Frame(Event::Message(message)) => {
                self.session
                    .observe(pane::observed(self.alternate, self.modes));
                self.session.message(message);
                self.settle();
            }
            Piece::Frame(_) => {}
            Piece::Query => self.typed.type_in(DEVICE_ATTRIBUTES),
            Piece::Alternate(on) => self.alternate = on,
        });
        self.pane = pane;
    }

    /// The program is gone: its view with it.
    fn end(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        self.session.program_ended();
        self.settle();
        self.closed
            .get_or_insert_with(|| "the program ended".to_owned());
    }

    /// Does what the session asks, until it asks nothing.
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
            Action::WriteView(bytes) => {
                // What a view says is held only up to the bound (§4.8): past it the session closes
                // the view, and what was held of its words and not begun goes.
                if self.session.backlog(self.typed.said() + bytes.len()) {
                    self.typed.view_said(&bytes);
                } else {
                    self.typed.drop_said();
                }
            }
            Action::Write(bytes) => self.typed.type_in(&bytes),
            // Nobody is asked: what a page asks to reach is not given.
            Action::Consent(consent) => self.session.consent(consent.id, Grant::none()),
            Action::StartView(start) => {
                if !start.kind.is_native() {
                    self.closed = Some(format!(
                        "it opened a view of the kind {}",
                        start.kind.name()
                    ));
                    return;
                }
                let Some(manifest) = self.session.bundle().cloned() else {
                    self.closed = Some("its view has no bundle".to_owned());
                    return;
                };
                let files = manifest
                    .files()
                    .iter()
                    .filter_map(|file| {
                        self.session
                            .serve(&file.path)
                            .map(|served| (file.path.clone(), served))
                    })
                    .collect();
                self.owed.clear();
                self.started = Some(Started {
                    kind: start.kind,
                    entry: manifest.entry().to_owned(),
                    files: Bundle(Arc::new(files)),
                });
            }
            Action::ToView(text) => {
                if let Some((name, Some(id))) = heading(&text)
                    && name == "response"
                {
                    self.owed.remove(&id);
                }
                self.said.push(text);
            }
            // A link the program's view asks to open is the host's: this one opens none.
            Action::OpenLink { link, .. } => self.session.link_decision(link, false),
            Action::DestroyView => {
                if self.started.is_some() {
                    self.closed
                        .get_or_insert_with(|| "the program closed its view".to_owned());
                }
            }
            // Nothing is on a screen to take back; an action a later version adds is one this
            // host does not know.
            _ => {}
        }
    }
}

impl Program {
    /// How many of the page's calls the program has not answered yet.
    pub fn owed(&self) -> usize {
        self.owed.len()
    }

    /// Bytes for the program's input that the tty has not taken yet.
    pub fn unwritten(&self) -> usize {
        self.typed.len()
    }
}

impl Peer for Program {
    fn heard(&mut self, message: &str) {
        match heading(message) {
            Some((name, Some(id))) if name == "call" => {
                self.owed.insert(id);
            }
            // A call given up may never be answered (a subscription's number is none of them).
            Some((name, Some(id))) if name == "cancel" => {
                self.owed.remove(&id);
            }
            _ => {}
        }
        self.session.module_message(message);
        self.settle();
        self.turn(Duration::ZERO);
    }

    fn said(&mut self) -> Vec<String> {
        self.turn(Duration::ZERO);
        std::mem::take(&mut self.said)
    }

    fn busy(&mut self) -> bool {
        !self.owed.is_empty() && self.closed.is_none()
    }

    fn wait(&mut self, within: Duration) {
        let deadline = Instant::now() + within;
        while self.said.is_empty() && self.closed.is_none() && Instant::now() < deadline {
            self.turn(Duration::from_millis(10));
        }
    }

    fn closed(&mut self) -> Option<String> {
        self.closed.clone()
    }
}

impl Drop for Program {
    fn drop(&mut self) {
        if !self.ended {
            self.pty.kill();
        }
    }
}
