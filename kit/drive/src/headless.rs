//! A host with no screen: a page run as a terminal runs it — in the kit's sandbox, laid out with the
//! kit's editors, driven by this crate's [`Page`] — with a clock that moves only when the test moves
//! it, a person whose hands are methods, and eyes that read what the page shows as words.
//!
//! ```no_run
//! use pencere_drive::headless::Headless;
//! # fn main() -> Result<(), pencere_drive::headless::Error> {
//! let mut page = Headless::script(r#"return { render = function() return pencere.text("hello") end }"#)
//!     .size(400.0, 300.0)
//!     .open()?;
//! assert!(page.says("hello"));
//! page.press("cmd+f")?; // a key, as a person presses it
//! page.advance(1.5)?; // a second and a half later, every frame and wake on the way drawn and given
//! # Ok(())
//! # }
//! ```
//!
//! **Everything a method does is done when it returns.** The page is told, then driven until it has
//! nothing more to say ([`Headless::settle`]): the module has done everything it was told (the
//! sandbox says so: nothing is waited for by the clock), its images are decoded, its replies are done
//! as a host does them, its program has heard what it said and answered, and the frames that came of
//! it are drawn — all at the time the clock shows. Time moves only by [`Headless::advance`] and
//! [`Headless::idle`], which draw every frame and give every wake on the way as a host's frames
//! would: the kit's timers, the `found` of a search that settles and a language read a piece at a
//! time are as determined as the rest. So is what an editor undoes together, which goes by the same
//! clock: what is typed with no `advance` between is one group, and an `advance` past the editor's
//! pause ([`pencere_editor::UNDO_PAUSE`], a second) puts what is typed after in a group of its own.
//!
//! A page that goes wrong is an [`Error`], with why: a method of a page that ended returns
//! [`Error::Ended`]. The text is measured by a fixed measure ([`Fixed`]: every character the same
//! width) unless the test gives another. The program is a table of calls ([`Calls`]) unless the test
//! gives another [`Peer`] — the program's own handlers in this process (`InProcess`, with the
//! feature `program`), or a program in another process. Every point is the host's: with no strip
//! and no zoom, the page's own.

mod field;
mod files;
#[cfg(feature = "program")]
mod in_process;
mod peer;

use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use pencere_editor::Buffer;
use pencere_sandbox::{Answer, Assets, Fault, Images, ModuleLimits, Program, Runner, Script};
use pencere_view::wire::Kind;
use pencere_view::{
    Button, Cursor, DrawList, Element, Event, FieldKind, Fill, Font, Id, Input, Key, Keyboard,
    Laid, Measure, Menu, MenuEntry, Metrics, Mods, Paint, Phase, Point, Rect, Shortcut, Size,
};

use field::{Done, Held};
pub use files::Files;
#[cfg(feature = "program")]
pub use in_process::InProcess;
pub use peer::{Calls, Heard, Peer, Subscription};
pub use pencere_wire::messages::Error as Failure;
pub use serde_json::Value;

use crate::{FrameAt, Host, Outline, Page, Route, Setup};

/// A frame of a host that draws sixty a second: how far apart [`Headless::advance`] draws the
/// frames of a page that moves, and the soonest after one that the next is drawn.
pub const FRAME: f64 = 1.0 / 60.0;

/// The most rounds of telling and taking one settling goes through before it says the page never
/// came to rest.
const ROUNDS: usize = 1000;

/// The most frames [`Headless::idle`] draws before it says the page never stopped asking for them.
const IDLE_FRAMES: usize = 10_000;

/// Text measured with no fonts: every character `advance` pixels across and a line `line` pixels
/// high, whatever the font and the scale (the engine measures text in the pixels it lays it out in:
/// at a scale of 1, points). By default 7 and 16.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fixed {
    pub advance: f32,
    pub line: f32,
}

impl Default for Fixed {
    fn default() -> Self {
        Self {
            advance: 7.0,
            line: 16.0,
        }
    }
}

impl Measure for Fixed {
    fn measure(&mut self, _font: &Font, text: &str) -> Metrics {
        #[allow(clippy::cast_precision_loss)]
        let characters = text.chars().count() as f32;
        Metrics {
            width: characters * self.advance,
            height: self.line,
        }
    }
}

/// The headless host's clock: what the module, the editors and the scrolling read
/// ([`Program::now`]), where the test moves it.
struct Clock(Mutex<f64>);

impl Clock {
    fn now(&self) -> f64 {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set(&self, now: f64) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = now;
    }
}

impl Program for Clock {
    fn may_say(&self, _bytes: usize) -> bool {
        true
    }

    fn taken(&self, _bytes: usize) {}

    fn now(&self) -> Option<f64> {
        Some(Clock::now(self))
    }
}

/// What went wrong with a page under the headless host, in words a test's author can act on.
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// The page is gone, and why: it failed, or its module could not be made.
    Ended(Fault),
    /// The page's view is over, and why: its program closed it, or ended ([`Peer::closed`]).
    Closed(String),
    /// Something did not come within the patience the host was given ([`Opening::patience`]).
    Slow {
        what: &'static str,
        waited: Duration,
    },
    /// The page never came to rest: it went on changing at one time for this many rounds.
    Unsettled { rounds: usize },
    /// The page asked for frames or wakes without end, at a time that went on.
    Restless { frames: usize },
    /// Nothing of this name is on the page (its last frame).
    NoSuch(String),
    /// The element is on the page, but a press at its middle reaches something else or nothing: it
    /// is covered, or scrolled out of view.
    Unreachable(String),
    /// No editor and no field has the keyboard.
    NoEditor,
    /// No menu is open to pick from.
    NoMenu,
    /// The open menu has no entry of that place or title.
    NoEntry(String),
    /// A menu is open: it takes the keyboard until an entry is picked or it is closed with escape.
    MenuOpen,
    /// The page cannot be told this command now: it does not list it, or lists it as of no use.
    NoCommand(String),
    /// Not a key a person can press (`cmd+shift+k`, `f5`, `page_down`, `escape`, …).
    NoKey(String),
    /// The bundle has no file at the entry it was told to start from.
    NoFile(String),
    /// Not a time to move the clock by: a number of seconds, 0 or more.
    NoTime(f64),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ended(why) => write!(f, "the page ended: {why}"),
            Self::Closed(why) => write!(f, "the page's view is over: {why}"),
            Self::Slow { what, waited } => write!(f, "{what} within {waited:?}"),
            Self::Unsettled { rounds } => write!(
                f,
                "the page never came to rest: it changed again after each of {rounds} rounds at one time"
            ),
            Self::Restless { frames } => write!(
                f,
                "the page never stopped asking for frames or wakes: {frames} were drawn"
            ),
            Self::NoSuch(name) => write!(f, "nothing named {name:?} is on the page"),
            Self::Unreachable(name) => write!(
                f,
                "{name:?} is on the page, but a press at its middle does not reach it (it is covered or scrolled away)"
            ),
            Self::NoEditor => f.write_str("no editor and no field has the keyboard"),
            Self::NoMenu => f.write_str("no menu is open"),
            Self::NoEntry(entry) => write!(f, "the menu has no entry {entry}"),
            Self::MenuOpen => {
                f.write_str("a menu is open: pick an entry from it, or press escape to close it")
            }
            Self::NoCommand(name) => {
                write!(f, "the page cannot be told the command {name:?} now")
            }
            Self::NoKey(keys) => write!(f, "{keys:?} is not a key a person can press"),
            Self::NoFile(path) => write!(f, "the bundle has no file at {path:?}"),
            Self::NoTime(seconds) => write!(f, "{seconds} is not a time the clock can move by"),
        }
    }
}

impl std::error::Error for Error {}

/// Where the keyboard is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    /// The page's own keys.
    Page,
    /// The host's text field over the page that has this id.
    Field(Id),
    /// The page's editor of this id.
    Editor(Id),
}

/// What the page is made of.
enum Source {
    Script(Script),
    Component(Vec<u8>),
    /// The script at `entry` of a bundle.
    Bundle {
        files: Arc<dyn Assets>,
        entry: String,
    },
    /// A module the test makes: the page started with what the host sets up.
    Spawn(Box<dyn FnOnce(Setup, Wake) -> Page>),
}

/// What wakes the host, from the module's thread.
type Wake = Box<dyn Fn() + Send + Sync>;

/// A page about to be opened under the headless host: how large its view is, what measures its text,
/// who its program is. [`Opening::open`] opens it.
#[must_use = "a page is opened by `open`"]
pub struct Opening<P: Peer = Calls> {
    source: Source,
    size: Size,
    scale: f64,
    reduce_motion: bool,
    limits: ModuleLimits,
    assets: Option<Arc<dyn Assets>>,
    measure: Box<dyn Measure>,
    blink: f64,
    start: f64,
    patience: Duration,
    peer: P,
}

/// A page under a host with no screen. See the module's documentation.
pub struct Headless<P: Peer = Calls> {
    page: Page,
    clock: Arc<Clock>,
    peer: P,
    woken: Receiver<()>,
    /// The view, in points, and its pixels to a point.
    size: Size,
    scale: f64,
    reduce_motion: bool,
    patience: Duration,
    /// The last frame's list.
    list: DrawList,
    /// The view changed (it was opened, resized): a frame is due.
    dirty: bool,
    seen: Seen,
}

/// What the host did of what the page asked, and what it keeps of its own.
#[derive(Default)]
struct Seen {
    cursor: Option<Cursor>,
    pasteboard: Option<String>,
    links: Vec<String>,
    menu: Option<Menu>,
    closed: bool,
    /// The host's field that has the keyboard, where one has it.
    field: Option<Id>,
    /// A press the host took (a field's, or one that closed a menu): its moves and its release are
    /// the host's too, and never the page's.
    taken: bool,
    /// What the host's fields hold, and what is selected in them.
    fields: HashMap<Id, Held>,
    diagnostics: Vec<String>,
    ended: Option<Fault>,
    sent: Vec<String>,
    /// What the program said to the page.
    received: Vec<String>,
    replies: Vec<Answer>,
}

/// The headless host as a [`Host`]: what it does with what the page said.
struct Deliver<'a, P> {
    seen: &'a mut Seen,
    peer: &'a mut P,
}

impl<P: Peer> Host for Deliver<'_, P> {
    fn send(&mut self, message: String) {
        self.peer.heard(&message);
        self.seen.sent.push(message);
    }

    fn close(&mut self) {
        self.seen.closed = true;
    }

    fn cursor(&mut self, cursor: Cursor) {
        self.seen.cursor = Some(cursor);
    }

    fn fill(&mut self, field: Id, fill: Fill) {
        self.seen.fields.entry(field).or_default().fill(&fill);
    }

    fn keyboard(&mut self, to: Keyboard) {
        // An editor's is the drive's, and the drive has given it: a field of the host's that has it
        // is told apart from an editor when the keyboard is used.
        self.seen.field = match to {
            Keyboard::Field(id) => Some(id),
            Keyboard::Page => None,
        };
    }

    fn copy(&mut self, text: String) {
        self.seen.pasteboard = Some(text);
    }

    fn link(&mut self, url: String) {
        self.seen.links.push(url);
    }

    fn menu(&mut self, menu: Menu) {
        self.seen.menu = Some(menu);
    }

    fn diagnostic(&mut self, words: String) {
        self.seen.diagnostics.push(words);
    }

    fn ended(&mut self, why: Fault) {
        self.seen.ended = Some(why);
    }
}

impl Headless<Calls> {
    /// A page that is the Lua script `source` (at `page.lua` of a bundle of none but it).
    pub fn script(source: impl Into<String>) -> Opening<Calls> {
        Opening::new(Source::Script(Script {
            path: "page.lua".to_owned(),
            source: source.into().into_bytes(),
            bundle: None,
        }))
    }

    /// A page that is the script at `entry` of the bundle `files`: what it requires and the
    /// pictures it shows are read from it too.
    pub fn bundle(files: impl Assets, entry: &str) -> Opening<Calls> {
        Opening::new(Source::Bundle {
            files: Arc::new(files),
            entry: entry.to_owned(),
        })
    }

    /// The page of the pencere app in `dir`, as its program carries it ([`Files::app`]): its
    /// `src/view/page.lua`, with the rest of `src/view/` and `src/assets/`.
    pub fn app(dir: impl AsRef<Path>) -> Opening<Calls> {
        Self::bundle(Files::app(dir), "page.lua")
    }

    /// A page that is the WebAssembly component in `bytes`.
    pub fn component(bytes: Vec<u8>) -> Opening<Calls> {
        Opening::new(Source::Component(bytes))
    }

    /// A page that is a module the test makes (`make` runs on the module's thread): a page written
    /// in Rust, run as a module is ([`Page::spawn`]).
    pub fn spawn<R: Runner>(
        make: impl FnOnce() -> Result<R, Fault> + Send + 'static,
    ) -> Opening<Calls> {
        Opening::new(Source::Spawn(Box::new(move |setup, wake| {
            Page::spawn(make, setup, wake)
        })))
    }
}

impl Opening<Calls> {
    fn new(source: Source) -> Self {
        Self {
            source,
            size: Size::new(800.0, 600.0),
            scale: 1.0,
            reduce_motion: false,
            limits: ModuleLimits::default(),
            assets: None,
            measure: Box::new(Fixed::default()),
            blink: 0.5,
            start: 0.0,
            patience: Duration::from_secs(60),
            peer: Calls::new(),
        }
    }
}

impl<P: Peer> Opening<P> {
    /// The view is `width` × `height` points (800 × 600 unless said).
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = Size::new(width, height);
        self
    }

    /// Its pixels to a point (1 unless said).
    pub fn scale(mut self, scale: f64) -> Self {
        self.scale = scale;
        self
    }

    /// The person asked for less motion.
    pub fn reduce_motion(mut self, reduce: bool) -> Self {
        self.reduce_motion = reduce;
        self
    }

    /// What measures the page's text ([`Fixed`] unless said).
    pub fn measure(mut self, measure: impl Measure + 'static) -> Self {
        self.measure = Box::new(measure);
        self
    }

    /// What the page's module is held to.
    pub fn limits(mut self, limits: ModuleLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Where the page's pictures are read from, for a page whose bundle does not say.
    pub fn assets(mut self, assets: impl Assets) -> Self {
        self.assets = Some(Arc::new(assets));
        self
    }

    /// How long an editor's caret stays lit, then dark (0.5 seconds unless said; 0: it does not
    /// blink).
    pub fn blink(mut self, half_period: f64) -> Self {
        self.blink = half_period;
        self
    }

    /// The time on the clock when the page opens (0 unless said).
    pub fn at(mut self, time: f64) -> Self {
        self.start = time;
        self
    }

    /// How long the host waits for what takes real time — the module made and doing what it was
    /// told, an image decoded, a program in another process answering — before it says so
    /// ([`Error::Slow`]): a minute unless said.
    pub fn patience(mut self, patience: Duration) -> Self {
        self.patience = patience;
        self
    }

    /// The program the page speaks to.
    pub fn peer<Q: Peer>(self, peer: Q) -> Opening<Q> {
        Opening {
            source: self.source,
            size: self.size,
            scale: self.scale,
            reduce_motion: self.reduce_motion,
            limits: self.limits,
            assets: self.assets,
            measure: self.measure,
            blink: self.blink,
            start: self.start,
            patience: self.patience,
            peer,
        }
    }

    /// Opens the page and drives it until it is at rest: its first page drawn, what it asked of its
    /// program answered.
    ///
    /// # Errors
    ///
    /// The page ended before it was at rest ([`Error::Ended`]: a script's error is its file and its
    /// line), or did not come to rest ([`Error::Slow`], [`Error::Unsettled`]); the bundle has no
    /// file at the entry ([`Error::NoFile`]).
    pub fn open(self) -> Result<Headless<P>, Error> {
        let clock = Arc::new(Clock(Mutex::new(self.start)));
        let setup = Setup {
            limits: self.limits,
            program: Arc::clone(&clock) as Arc<dyn Program>,
            assets: self.assets,
            blink: self.blink,
            measure: self.measure,
            ..Setup::default()
        };
        let (told, woken) = channel();
        let told = Mutex::new(told);
        let wake = move || {
            let _ = told.lock().unwrap_or_else(PoisonError::into_inner).send(());
        };
        let mut page = match self.source {
            Source::Script(script) => Page::script(script, setup, wake),
            Source::Component(bytes) => Page::component(bytes, setup, wake),
            Source::Spawn(start) => start(setup, Box::new(wake)),
            Source::Bundle { files, entry } => {
                let source = files
                    .read(&entry)
                    .ok_or_else(|| Error::NoFile(entry.clone()))?;
                let script = Script {
                    path: entry,
                    source: source.to_vec(),
                    bundle: Some(files),
                };
                Page::script(script, setup, wake)
            }
        };
        page.record(true);
        let mut headless = Headless {
            page,
            clock,
            peer: self.peer,
            woken,
            size: self.size,
            scale: self.scale,
            reduce_motion: self.reduce_motion,
            patience: self.patience,
            list: DrawList::default(),
            dirty: false,
            seen: Seen::default(),
        };
        headless.peer.opened();
        // The line that stands for the page, which asks the module for its first.
        headless.frame();
        headless.settle()?;
        Ok(headless)
    }
}

impl<P: Peer> Headless<P> {
    // --- Settling and the clock ------------------------------------------------------------------

    /// Drives the page until it has nothing more to say at the time the clock shows: what the module
    /// was told done, its images decoded, its replies done, its program's answers told it, and the
    /// frames that came of it drawn. Every method that tells the page something settles it before
    /// it returns; a test that tells the page through [`Headless::page_mut`] settles it itself.
    ///
    /// # Errors
    ///
    /// The page ended ([`Error::Ended`]), something did not come within the host's patience
    /// ([`Error::Slow`]), or the page went on changing ([`Error::Unsettled`]).
    pub fn settle(&mut self) -> Result<(), Error> {
        self.take(true)
    }

    /// Settles the page, drawing the frames that come of it where `frames` (a view that is not seen
    /// draws none).
    fn take(&mut self, frames: bool) -> Result<(), Error> {
        let settling = Instant::now();
        for _ in 0..ROUNDS {
            if let Some(why) = &self.seen.ended {
                return Err(Error::Ended(why.clone()));
            }
            if !self.page.module().flush(self.patience) {
                return Err(Error::Slow {
                    what: "the page did not do what it was told",
                    waited: self.patience,
                });
            }
            // The images decoded and drawn at their sizes: what the drive takes of them (and the
            // wake it is given for them) is the drive's, so the host looks only at whether any is
            // still on its way.
            let began = Instant::now();
            while self.page.images().is_some_and(Images::busy) {
                if began.elapsed() > self.patience {
                    return Err(Error::Slow {
                        what: "the page's images were not decoded",
                        waited: self.patience,
                    });
                }
                let _ = self.woken.recv_timeout(Duration::from_millis(2));
            }
            let drained = self.page.drain();
            let framing = frames && (self.dirty || drained.redraw || self.page.unsettled());
            self.seen.replies.extend(drained.replies.iter().cloned());
            drained.deliver(&mut Deliver {
                seen: &mut self.seen,
                peer: &mut self.peer,
            });
            // A program that owes the page an answer is given the time it takes, all of the
            // settling's patience at the most: one that says nothing in all of it never answered.
            // Not while the page moves: what it waits for may be what the page goes on to say at a
            // later frame, which only a clock that moves draws.
            let waits = self.peer.busy() && !self.page.animating();
            if waits {
                let left = self.patience.saturating_sub(settling.elapsed());
                if left.is_zero() {
                    return Err(Error::Slow {
                        what: "the program did not answer the page",
                        waited: self.patience,
                    });
                }
                self.peer.wait(left);
            }
            let said = self.peer.said();
            let posted = !said.is_empty();
            for message in said {
                self.page.message(&message);
                self.seen.received.push(message);
            }
            if let Some(why) = &self.seen.ended {
                return Err(Error::Ended(why.clone()));
            }
            if let Some(why) = self.peer.closed() {
                return Err(Error::Closed(why));
            }
            if framing {
                self.frame();
            } else if !posted && !waits {
                // What the host was woken for is all taken.
                while self.woken.try_recv().is_ok() {}
                return Ok(());
            }
        }
        Err(Error::Unsettled { rounds: ROUNDS })
    }

    /// The time on the host's clock, seconds.
    pub fn now(&self) -> f64 {
        self.clock.now()
    }

    /// Moves the clock `seconds` on, as a host's time goes: every frame the page wants on the way is
    /// drawn at its time — a page that moves at sixty a second, one that asked to be drawn or woken
    /// at its time, a caret's blink — the page woken where its time came, and settled at each; and a
    /// frame at the end.
    ///
    /// # Errors
    ///
    /// Not a time ([`Error::NoTime`]), or as [`Headless::settle`].
    pub fn advance(&mut self, seconds: f64) -> Result<(), Error> {
        if !(seconds.is_finite() && seconds >= 0.0) {
            return Err(Error::NoTime(seconds));
        }
        let end = self.now() + seconds;
        while let Some(next) = self.next_frame(true).filter(|next| *next < end) {
            self.clock.set(next);
            self.frame();
            self.settle()?;
        }
        self.clock.set(end);
        self.frame();
        self.settle()
    }

    /// Moves the clock on until the page asks for nothing more — no wake, no time to be drawn at,
    /// nothing moving — drawing every frame on the way as [`Headless::advance`] does (a caret's
    /// blink and the scroll bars going are the host's, and do not keep it). How many frames that
    /// took.
    ///
    /// # Errors
    ///
    /// The page never stopped asking ([`Error::Restless`]), or as [`Headless::settle`].
    pub fn idle(&mut self) -> Result<usize, Error> {
        let mut frames = 0;
        while let Some(next) = self.next_frame(false) {
            frames += 1;
            if frames > IDLE_FRAMES {
                return Err(Error::Restless { frames });
            }
            self.clock.set(next);
            self.frame();
            self.settle()?;
        }
        Ok(frames)
    }

    /// Moves the clock to the next time the page asked for — to be woken or drawn at, or a frame on
    /// for a page that moves; a frame after now at the soonest — and draws that frame, the page
    /// woken where its time came: whether the page asked for one. (A caret's blink and the scroll
    /// bars going are the host's, and are not one.)
    ///
    /// # Errors
    ///
    /// As [`Headless::settle`].
    pub fn step(&mut self) -> Result<bool, Error> {
        let Some(next) = self.next_frame(false) else {
            return Ok(false);
        };
        self.clock.set(next);
        self.frame();
        self.settle()?;
        Ok(true)
    }

    /// The view is not seen for `seconds`: no frame is drawn, and the host wakes the page from a
    /// clock of its own as a host does while its view is out of sight — at the time the page asked
    /// for, a second after the last wake at the soonest ([`Page::wake_unseen`]); what the page says
    /// meanwhile is taken and done as ever. The view is seen again after it: the next frame is drawn
    /// by [`Headless::advance`] or [`Headless::step`].
    ///
    /// # Errors
    ///
    /// Not a time ([`Error::NoTime`]), or as [`Headless::settle`].
    pub fn hide(&mut self, seconds: f64) -> Result<(), Error> {
        if !(seconds.is_finite() && seconds >= 0.0) {
            return Err(Error::NoTime(seconds));
        }
        let end = self.now() + seconds;
        for _ in 0..IDLE_FRAMES {
            let now = self.now();
            // The wake, if its time has come; then what it brought, and the time the page asks for
            // now (asked again at the same time, which gives no second wake).
            self.page.wake_unseen(now);
            self.take(false)?;
            match self.page.wake_unseen(now) {
                Some(next) if next > now && next <= end => self.clock.set(next),
                _ => {
                    self.clock.set(end);
                    return self.take(false);
                }
            }
        }
        Err(Error::Restless {
            frames: IDLE_FRAMES,
        })
    }

    /// The page is asked for itself again, now, as a host asks a view shown anew: what it shows
    /// of what changed in it with no reply that asked for a frame (a count a timer keeps).
    ///
    /// # Errors
    ///
    /// As [`Headless::settle`].
    pub fn refresh(&mut self) -> Result<(), Error> {
        self.page.refresh();
        self.settle()
    }

    /// Waits, as long as `within` of the machine's own time, for the page to come to what `done`
    /// says of it — a frame drawn at a time a frame later each round, what its program says taken
    /// as it comes: for a program in another process ([`Peer::wait`]), which speaks on the clock of
    /// the machine and not on this host's.
    ///
    /// # Errors
    ///
    /// It did not come to it within the time ([`Error::Slow`]), or as [`Headless::settle`].
    pub fn wait_for(
        &mut self,
        within: Duration,
        mut done: impl FnMut(&Self) -> bool,
    ) -> Result<(), Error> {
        let began = Instant::now();
        loop {
            if done(self) {
                return Ok(());
            }
            if began.elapsed() > within {
                return Err(Error::Slow {
                    what: "the page did not come to what was waited for",
                    waited: within,
                });
            }
            self.peer.wait(Duration::from_millis(20));
            self.advance(FRAME)?;
        }
    }

    /// The earliest of the times the page itself asked for: to be woken at, or drawn at.
    pub fn asks_at(&self) -> Option<f64> {
        self.page.asks_at()
    }

    /// When the next frame is wanted, a frame after this one at the soonest: what the page asked
    /// for, what moves, and where `host` the host's own (the caret's blink, the bars).
    fn next_frame(&self, host: bool) -> Option<f64> {
        let after = self.now() + FRAME;
        let asked = if host {
            self.page.wake_at()
        } else {
            self.page.asks_at()
        };
        let next = if self.page.animating() {
            Some(asked.map_or(after, |asked| asked.min(after)))
        } else {
            asked
        };
        next.map(|next| next.max(after))
    }

    /// A frame of the page, now.
    fn frame(&mut self) {
        let at = FrameAt {
            size: (self.pixels(self.size.w), self.pixels(self.size.h)),
            scale: self.scale,
            now: self.now(),
            reduce_motion: self.reduce_motion,
        };
        self.list = self.page.frame(at).list;
        self.dirty = false;
        // The pictures drawn as a host draws them: their pixels asked for at the size they are
        // drawn at (an SVG is drawn at it), which the next settling waits for.
        if let Some(images) = self.page.images() {
            for item in &self.list.items {
                if let Paint::Image(picture) = item {
                    let _ = images.bitmap(&picture.src, picture.full());
                }
            }
        }
    }

    // --- The keyboard ------------------------------------------------------------------------------

    /// The person presses `key` with `mods`, and where it went: as a host does it, to the host's
    /// field that has the keyboard, else to the page's editor that has it (the page's command for
    /// the key first, the editor's own, then the text: the character typed as the system's handling
    /// of text would, `Copy` and `Cut` to the host's pasteboard, `Paste` from it), else to the page
    /// (its command for the key first). A key into a host's field is [`Route::NotEditor`].
    ///
    /// # Errors
    ///
    /// A menu is open ([`Error::MenuOpen`]; escape closes it), or as [`Headless::settle`].
    pub fn key(&mut self, key: Key, mods: Mods) -> Result<Route, Error> {
        if self.seen.menu.is_some() {
            if key == Key::Escape {
                self.seen.menu = None;
                return Ok(Route::NotEditor);
            }
            return Err(Error::MenuOpen);
        }
        if let Focus::Field(field) = self.focus() {
            self.field_key(field, key, mods);
            self.settle()?;
            return Ok(Route::NotEditor);
        }
        let route = self.page.key(key, mods);
        match &route {
            Route::Type => {
                if let Key::Character(typed) = key {
                    self.page.editors().type_text(&typed.to_string());
                }
            }
            Route::Copy(text) | Route::Cut(text) => self.seen.pasteboard = Some(text.clone()),
            Route::Paste => {
                if let Some(text) = self.seen.pasteboard.clone() {
                    self.page.editors().paste(&text);
                }
            }
            Route::NotEditor | Route::Command(_) | Route::Page | Route::Done => {}
        }
        self.settle()?;
        Ok(route)
    }

    /// The person presses the keys `keys` say, as a page writes a command's (`cmd+shift+k`, `f5`,
    /// `page_down`, `escape`, `a`): see [`Headless::key`].
    ///
    /// # Errors
    ///
    /// Not keys ([`Error::NoKey`]), or as [`Headless::key`].
    pub fn press(&mut self, keys: &str) -> Result<Route, Error> {
        let shortcut = Shortcut::parse(keys).ok_or_else(|| Error::NoKey(keys.to_owned()))?;
        self.key(shortcut.key, shortcut.mods)
    }

    /// The person types `text`, a key at a time ([`Headless::key`]): a line break is the enter key,
    /// a tab the tab key.
    ///
    /// # Errors
    ///
    /// As [`Headless::key`].
    pub fn type_text(&mut self, text: &str) -> Result<(), Error> {
        for typed in text.chars() {
            let key = match typed {
                '\n' => Key::Enter,
                '\t' => Key::Tab,
                other => Key::Character(other),
            };
            self.key(key, Mods::default())?;
        }
        Ok(())
    }

    /// What the system's handling of text gives the editor or the field that has the keyboard in
    /// one piece — what an input method commits: no key is pressed for it.
    ///
    /// # Errors
    ///
    /// Neither an editor nor a field has the keyboard ([`Error::NoEditor`]), or as
    /// [`Headless::settle`].
    pub fn insert_text(&mut self, text: &str) -> Result<(), Error> {
        match self.focus() {
            Focus::Field(field) => {
                let held = self.seen.fields.entry(field).or_default();
                held.insert(text);
                let held = held.text.clone();
                self.page.text(field, &held);
            }
            Focus::Editor(_) => self.page.editors().type_text(text),
            Focus::Page => return Err(Error::NoEditor),
        }
        self.settle()
    }

    /// The person pastes `text`: it is put on the host's pasteboard and ⌘V pressed.
    ///
    /// # Errors
    ///
    /// As [`Headless::key`].
    pub fn paste(&mut self, text: &str) -> Result<(), Error> {
        self.seen.pasteboard = Some(text.to_owned());
        self.key(
            Key::Character('v'),
            Mods {
                command: true,
                ..Mods::default()
            },
        )
        .map(drop)
    }

    /// An input method composes `text` in the editor that has the keyboard: the part of it selected
    /// (where it starts, how long, in units of UTF-16) and the range of the document it replaces
    /// (`None`: what it composed before, else the selection).
    ///
    /// # Errors
    ///
    /// No editor has the keyboard ([`Error::NoEditor`]), or as [`Headless::settle`].
    pub fn compose(
        &mut self,
        text: &str,
        selected: (usize, usize),
        replace: Option<(usize, usize)>,
    ) -> Result<(), Error> {
        if self.page.focused_editor().is_none() {
            return Err(Error::NoEditor);
        }
        self.page.editors().compose(text, selected, replace);
        self.settle()
    }

    /// The person puts the keyboard in the page's editor named `name` some way other than a click
    /// (the tab key, a screen reader): its caret stays where it was.
    ///
    /// # Errors
    ///
    /// No editor of that name ([`Error::NoSuch`]), or as [`Headless::settle`].
    pub fn focus_editor(&mut self, name: &str) -> Result<(), Error> {
        let id = Id::named(name);
        if self.page.editor(id).is_none() {
            return Err(Error::NoSuch(name.to_owned()));
        }
        if self.seen.field.take().is_some() {
            self.page.focus(None);
        }
        self.page.editors().focus(Some(id));
        self.settle()
    }

    /// The person selects from `anchor` to `head` (characters of the document) of the page's editor
    /// named `name`, which takes the keyboard: as a click there and a drag, without the pointer.
    ///
    /// # Errors
    ///
    /// No editor of that name ([`Error::NoSuch`]), or as [`Headless::settle`].
    pub fn select(&mut self, name: &str, anchor: usize, head: usize) -> Result<(), Error> {
        let id = Id::named(name);
        if self.page.editor(id).is_none() {
            return Err(Error::NoSuch(name.to_owned()));
        }
        if self.seen.field.take().is_some() {
            self.page.focus(None);
        }
        self.page
            .editors()
            .select(id, vec![pencere_view::edit::Range::new(anchor, head)]);
        self.settle()
    }

    /// The caret of the page's editor named `name` put at `at` (a character of the document), the
    /// editor taking the keyboard: [`Headless::select`] of nothing.
    ///
    /// # Errors
    ///
    /// As [`Headless::select`].
    pub fn caret_to(&mut self, name: &str, at: usize) -> Result<(), Error> {
        self.select(name, at, at)
    }

    /// The editor that has the keyboard does `command`, as one of its keys or an entry of the host's
    /// menus does (`EditCommand::ToggleFold`, `Undo`, a move by a word).
    ///
    /// # Errors
    ///
    /// No editor has the keyboard ([`Error::NoEditor`]), or as [`Headless::settle`].
    pub fn edit(&mut self, command: pencere_view::edit::EditCommand) -> Result<(), Error> {
        if self.page.focused_editor().is_none() {
            return Err(Error::NoEditor);
        }
        self.page.editors().command(command);
        self.settle()
    }

    /// The input method is done: what it composed stays, as typed.
    ///
    /// # Errors
    ///
    /// As [`Headless::settle`].
    pub fn unmark(&mut self) -> Result<(), Error> {
        self.page.editors().unmark();
        self.settle()
    }

    /// A key typed into the host's field `field`, as a field of a system takes it (a stand-in's,
    /// [`field`]): what the field has a use for where its caret is, it does, the page told what it
    /// holds when that changed; any other key is the page's (its command first), by the kit's rule
    /// ([`pencere_view::FieldState::takes`]).
    fn field_key(&mut self, field: Id, key: Key, mods: Mods) {
        let kind = self
            .page
            .fields()
            .iter()
            .find(|held| held.id == field)
            .map_or(FieldKind::Line, |held| held.kind);
        let held = self.seen.fields.entry(field).or_default();
        match held.key(kind, key, mods, &mut self.seen.pasteboard) {
            Done::Passed => self.page.input(Input::Key { key, mods }),
            Done::Moved => {}
            Done::Changed => {
                let text = held.text.clone();
                self.page.text(field, &text);
            }
        }
    }

    // --- The pointer -------------------------------------------------------------------------------

    /// The person clicks the middle of the element named `name`.
    ///
    /// # Errors
    ///
    /// No such element ([`Error::NoSuch`]), or one a click there does not reach
    /// ([`Error::Unreachable`]), or as [`Headless::settle`].
    pub fn click(&mut self, name: &str) -> Result<(), Error> {
        let at = self.reach(name)?;
        self.click_with(at, Button::Left, 1)
    }

    /// The person clicks the element named `name` twice, as a double click.
    ///
    /// # Errors
    ///
    /// As [`Headless::click`].
    pub fn double_click(&mut self, name: &str) -> Result<(), Error> {
        let at = self.reach(name)?;
        self.click_with(at, Button::Left, 1)?;
        self.click_with(at, Button::Left, 2)
    }

    /// The person clicks the element named `name` with the right button.
    ///
    /// # Errors
    ///
    /// As [`Headless::click`].
    pub fn right_click(&mut self, name: &str) -> Result<(), Error> {
        let at = self.reach(name)?;
        self.click_with(at, Button::Right, 1)
    }

    /// The person clicks `x`, `y` points from the top-left corner of the element named `name`.
    ///
    /// # Errors
    ///
    /// No such element ([`Error::NoSuch`]), or a click there does not reach it
    /// ([`Error::Unreachable`]), or as [`Headless::settle`].
    pub fn click_in(&mut self, name: &str, x: f32, y: f32) -> Result<(), Error> {
        let bounds = self
            .bounds(name)
            .ok_or_else(|| Error::NoSuch(name.to_owned()))?;
        let at = Point::new(bounds.x + x, bounds.y + y);
        if !self.reaches(name, at) {
            return Err(Error::Unreachable(name.to_owned()));
        }
        self.click_at(at)
    }

    /// The person clicks at `at`, the host's points.
    ///
    /// # Errors
    ///
    /// As [`Headless::settle`].
    pub fn click_at(&mut self, at: Point) -> Result<(), Error> {
        self.click_with(at, Button::Left, 1)
    }

    /// A press and its release at `at` with `button`, the `count`th of a run of clicks.
    fn click_with(&mut self, at: Point, button: Button, count: u32) -> Result<(), Error> {
        self.pointer(Input::Press {
            at,
            button,
            mods: Mods::default(),
            count,
        })?;
        self.pointer(Input::Release { at, button })
    }

    /// The pointer moves to the middle of the element named `name`.
    ///
    /// # Errors
    ///
    /// As [`Headless::click`].
    pub fn hover(&mut self, name: &str) -> Result<(), Error> {
        let at = self.reach(name)?;
        self.pointer(Input::Move(at))
    }

    /// The pointer moves to `at`, the host's points.
    ///
    /// # Errors
    ///
    /// As [`Headless::settle`].
    pub fn move_to(&mut self, at: Point) -> Result<(), Error> {
        self.pointer(Input::Move(at))
    }

    /// The pointer leaves the page.
    ///
    /// # Errors
    ///
    /// As [`Headless::settle`].
    pub fn leave(&mut self) -> Result<(), Error> {
        self.pointer(Input::Leave)
    }

    /// The person drags from the middle of the element named `from` to the middle of the one named
    /// `to`, the button held through moves on the way.
    ///
    /// # Errors
    ///
    /// As [`Headless::click`].
    pub fn drag(&mut self, from: &str, to: &str) -> Result<(), Error> {
        let start = self.reach(from)?;
        let end = self
            .bounds(to)
            .ok_or_else(|| Error::NoSuch(to.to_owned()))?;
        self.drag_between(start, middle(end))
    }

    /// The person drags from `from` to `to`, the host's points: pressed, moved there in steps, let
    /// go.
    ///
    /// # Errors
    ///
    /// As [`Headless::settle`].
    pub fn drag_between(&mut self, from: Point, to: Point) -> Result<(), Error> {
        const STEPS: u8 = 8;
        self.pointer(Input::Press {
            at: from,
            button: Button::Left,
            mods: Mods::default(),
            count: 1,
        })?;
        for step in 1..=STEPS {
            let part = f32::from(step) / f32::from(STEPS);
            self.pointer(Input::Move(Point::new(
                from.x + (to.x - from.x) * part,
                from.y + (to.y - from.y) * part,
            )))?;
        }
        self.pointer(Input::Release {
            at: to,
            button: Button::Left,
        })
    }

    /// The wheel turns over the middle of the element named `name`, by `dx` and `dy` points
    /// (positive: the content moves left and up).
    ///
    /// # Errors
    ///
    /// As [`Headless::click`].
    pub fn wheel(&mut self, name: &str, dx: f32, dy: f32) -> Result<(), Error> {
        let at = self.reach(name)?;
        self.wheel_at(at, dx, dy)
    }

    /// The wheel turns over `at`, the host's points.
    ///
    /// # Errors
    ///
    /// As [`Headless::settle`].
    pub fn wheel_at(&mut self, at: Point, dx: f32, dy: f32) -> Result<(), Error> {
        self.page.wheel(at, dx, dy, Phase::Wheel);
        self.settle()
    }

    /// The box named `name` scrolled down to `y` points, as the wheel over it takes it there.
    ///
    /// # Errors
    ///
    /// No such box, or one that does not scroll ([`Error::NoSuch`]), or as [`Headless::click`].
    pub fn scroll_to(&mut self, name: &str, y: f32) -> Result<(), Error> {
        let offset = self
            .page
            .laid()
            .and_then(|laid| laid.scroll_extent(Id::named(name)))
            .map(|extent| extent.offset.y)
            .ok_or_else(|| Error::NoSuch(name.to_owned()))?;
        let zoom = self.page.placement().zoom;
        self.wheel(name, 0.0, (y - offset) * zoom)
    }

    /// The person does `input` with the pointer or a key, as it is: a press in a host's field gives
    /// the field the keyboard and is the field's, to its release; a press elsewhere takes the
    /// keyboard back to the page; a press while a menu is open closes the menu, and is not the
    /// page's.
    ///
    /// # Errors
    ///
    /// As [`Headless::settle`].
    pub fn pointer(&mut self, input: Input) -> Result<(), Error> {
        if self.seen.taken {
            match input {
                Input::Release { .. } => {
                    self.seen.taken = false;
                    return self.settle();
                }
                Input::Move(_) | Input::Leave => return self.settle(),
                _ => self.seen.taken = false,
            }
        }
        if let Input::Press { at, .. } = input {
            if self.seen.menu.take().is_some() {
                self.seen.taken = true;
                return self.settle();
            }
            let field = self
                .page
                .fields()
                .iter()
                .find(|field| contains(field.rect, at))
                .map(|field| field.id);
            if let Some(field) = field {
                self.seen.taken = true;
                if self.focus() != Focus::Field(field) {
                    self.seen.field = Some(field);
                    self.page.focus(Some(field));
                }
                return self.settle();
            }
            if self.seen.field.take().is_some() {
                self.page.focus(None);
            }
        }
        self.page.input(input);
        self.settle()
    }

    /// The middle of the element named `name`, where a press there reaches it.
    fn reach(&self, name: &str) -> Result<Point, Error> {
        let bounds = self
            .bounds(name)
            .ok_or_else(|| Error::NoSuch(name.to_owned()))?;
        let at = middle(bounds);
        if self.reaches(name, at) {
            Ok(at)
        } else {
            Err(Error::Unreachable(name.to_owned()))
        }
    }

    /// Whether a press at `at` (the host's points) reaches the element named `name`: it is the
    /// topmost there, or holds it.
    fn reaches(&self, name: &str, at: Point) -> bool {
        let place = self.page.placement();
        self.page
            .laid()
            .is_some_and(|laid| laid.ids_at(place.to_page(at)).contains(&Id::named(name)))
    }

    // --- What else the host does -------------------------------------------------------------------

    /// The person asks for the page's command named `name` from the host's list of what the page can
    /// be told to do now.
    ///
    /// # Errors
    ///
    /// The page does not list it now, or lists it as of no use ([`Error::NoCommand`]), or as
    /// [`Headless::settle`].
    pub fn command(&mut self, name: &str) -> Result<(), Error> {
        let id = Id::named(name);
        let listed = self
            .page
            .outline()
            .commands
            .iter()
            .any(|command| command.id == id && command.enabled);
        if !listed {
            return Err(Error::NoCommand(name.to_owned()));
        }
        self.page.command(id);
        self.settle()
    }

    /// The person picks the `index`th entry (from 0, separators counted) of the menu the page asked
    /// for.
    ///
    /// # Errors
    ///
    /// No menu is open ([`Error::NoMenu`]), it has no entry there ([`Error::NoEntry`]), or as
    /// [`Headless::settle`].
    pub fn pick(&mut self, index: usize) -> Result<(), Error> {
        let menu = self.seen.menu.as_ref().ok_or(Error::NoMenu)?;
        if !matches!(
            menu.entries.get(index),
            Some(MenuEntry::Item { enabled: true, .. })
        ) {
            return Err(Error::NoEntry(index.to_string()));
        }
        self.seen.menu = None;
        self.page.pick(index);
        self.settle()
    }

    /// The person picks the entry titled `title` of the menu the page asked for.
    ///
    /// # Errors
    ///
    /// As [`Headless::pick`].
    pub fn pick_titled(&mut self, title: &str) -> Result<(), Error> {
        let menu = self.seen.menu.as_ref().ok_or(Error::NoMenu)?;
        let index = menu
            .entries
            .iter()
            .position(
                |entry| matches!(entry, MenuEntry::Item { title: named, .. } if named == title),
            )
            .ok_or_else(|| Error::NoEntry(format!("{title:?}")))?;
        self.pick(index)
    }

    /// The person clicks into the host's field named `name` and types `text` over what it held.
    ///
    /// # Errors
    ///
    /// No field of that name ([`Error::NoSuch`]), or as [`Headless::settle`].
    pub fn fill(&mut self, name: &str, text: &str) -> Result<(), Error> {
        let field = Id::named(name);
        if !self.page.fields().iter().any(|held| held.id == field) {
            return Err(Error::NoSuch(name.to_owned()));
        }
        if self.focus() != Focus::Field(field) {
            self.seen.field = Some(field);
            self.page.focus(Some(field));
        }
        self.seen.fields.insert(field, Held::holding(text));
        self.page.text(field, text);
        self.settle()
    }

    /// Files are dropped on the middle of the element named `name`: their paths.
    ///
    /// # Errors
    ///
    /// As [`Headless::click`].
    pub fn drop_files(&mut self, name: &str, paths: Vec<String>) -> Result<(), Error> {
        let at = self.reach(name)?;
        self.page.files(at, paths);
        self.settle()
    }

    /// The view is `width` × `height` points from now on.
    ///
    /// # Errors
    ///
    /// As [`Headless::settle`].
    pub fn resize(&mut self, width: f32, height: f32) -> Result<(), Error> {
        self.size = Size::new(width, height);
        self.dirty = true;
        self.settle()
    }

    /// The page is drawn `zoom` times as large as its own points ([`crate::Placement::zoom`]).
    ///
    /// # Errors
    ///
    /// As [`Headless::settle`].
    pub fn zoom(&mut self, zoom: f32) -> Result<(), Error> {
        self.page.set_zoom(zoom);
        self.settle()
    }

    /// Whether the keyboard is in the page, in the person's window: a caret is drawn only then.
    ///
    /// # Errors
    ///
    /// As [`Headless::settle`].
    pub fn set_active(&mut self, active: bool) -> Result<(), Error> {
        self.page.set_active(active);
        self.settle()
    }

    // --- The program -------------------------------------------------------------------------------

    /// The program says `message` (one JSON object, as written) to the page.
    ///
    /// # Errors
    ///
    /// As [`Headless::settle`].
    pub fn tell(&mut self, message: &str) -> Result<(), Error> {
        self.page.message(message);
        self.seen.received.push(message.to_owned());
        self.settle()
    }

    /// The program the page speaks to.
    pub fn peer(&self) -> &P {
        &self.peer
    }

    /// The program the page speaks to, to change; what it says is told the page at the next
    /// settling.
    pub fn peer_mut(&mut self) -> &mut P {
        &mut self.peer
    }

    /// The page is put away and its program kept, for a page opened anew with it
    /// ([`Opening::peer`]): as a terminal opens a view again for the program that showed it, when
    /// the program showed another in its place or had the terminal back.
    pub fn into_peer(self) -> P {
        self.peer
    }

    // --- What the person sees ----------------------------------------------------------------------

    /// The words drawn in the last frame, in the order they are drawn: what a person sees (the lines
    /// of an editor in view among them).
    pub fn words(&self) -> Vec<&str> {
        self.list
            .items
            .iter()
            .filter_map(|item| match item {
                Paint::Text(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect()
    }

    /// The words of the element named `name` and of everything in it, in the order of the page's
    /// tree (an editor's text is [`Headless::editor_text`]).
    pub fn words_in(&self, name: &str) -> Vec<String> {
        let Some(root) = self.page.root() else {
            return Vec::new();
        };
        let nodes = root.fold();
        let id = Id::named(name);
        let Some(at) = nodes.iter().position(|node| node.meta.id == Some(id)) else {
            return Vec::new();
        };
        let end = subtree_end(&nodes, at);
        nodes
            .into_iter()
            .take(end)
            .skip(at)
            .filter_map(|node| match node.kind {
                Kind::Text(words) => Some(words),
                _ => None,
            })
            .collect()
    }

    /// Whether a word drawn in the last frame has `text` in it.
    pub fn says(&self, text: &str) -> bool {
        self.words().iter().any(|word| word.contains(text))
    }

    /// The words of the page as the module last made it, in the order of its tree, whether they are
    /// drawn or not (scrolled away, cut off).
    pub fn tree_words(&self) -> Vec<String> {
        self.page
            .root()
            .map(|root| {
                root.fold()
                    .into_iter()
                    .filter_map(|node| match node.kind {
                        Kind::Text(words) => Some(words),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Where the element named `name` is, the host's points, as of the last frame.
    pub fn bounds(&self, name: &str) -> Option<Rect> {
        let place = self.page.placement();
        self.page
            .laid()?
            .bounds(Id::named(name))
            .map(|rect| place.to_host(rect))
    }

    /// A picture of the page as the last frame drew it: a PNG of the view's pixels, the words in
    /// `fonts` (the page laid out with the same, [`Opening::measure`], for its words to fill their
    /// boxes; [`crate::shot::Fonts::none`] draws each character as a box, the same on every
    /// machine), in a dark terminal's colours.
    ///
    /// # Errors
    ///
    /// A view of no pixels, or a picture that could not be written.
    #[cfg(feature = "shot")]
    pub fn shot(&self, fonts: &crate::shot::Fonts) -> Result<Vec<u8>, crate::shot::Error> {
        self.shot_in(fonts, crate::shot::Palette::Dark)
    }

    /// A picture of the page as [`Headless::shot`] makes it, in the colours of `palette`.
    ///
    /// # Errors
    ///
    /// As [`Headless::shot`].
    #[cfg(feature = "shot")]
    pub fn shot_in(
        &self,
        fonts: &crate::shot::Fonts,
        palette: crate::shot::Palette,
    ) -> Result<Vec<u8>, crate::shot::Error> {
        crate::shot::png(
            &self.list,
            (self.pixels(self.size.w), self.pixels(self.size.h)),
            fonts,
            self.page.images(),
            palette,
        )
    }

    /// `points` of the view in its pixels.
    fn pixels(&self, points: f32) -> u32 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let pixels = (f64::from(points) * self.scale).round().max(0.0) as u32;
        pixels
    }

    /// The last frame's draw list (pixels, from the top-left corner of the view).
    pub fn list(&self) -> &DrawList {
        &self.list
    }

    /// The page as it was last laid out (its own points).
    pub fn laid(&self) -> Option<&Laid> {
        self.page.laid()
    }

    /// The page as the module last made it.
    pub fn root(&self) -> Option<&Element> {
        self.page.root()
    }

    /// The document of the page's editor named `name`: its text, its selections, its version, its
    /// decorations.
    pub fn editor(&self, name: &str) -> Option<&Buffer> {
        self.page.editor(Id::named(name))
    }

    /// The text of the page's editor named `name`.
    pub fn editor_text(&self, name: &str) -> Option<String> {
        self.editor(name).map(Buffer::text)
    }

    /// Where the keyboard is.
    pub fn focus(&self) -> Focus {
        if let Some(editor) = self.page.focused_editor() {
            return Focus::Editor(editor);
        }
        match self.seen.field {
            Some(field) if self.page.fields().iter().any(|held| held.id == field) => {
                Focus::Field(field)
            }
            _ => Focus::Page,
        }
    }

    /// What the host's field named `name` holds: what the person typed, or what the page filled it
    /// with.
    pub fn field(&self, name: &str) -> Option<&str> {
        self.seen
            .fields
            .get(&Id::named(name))
            .map(|held| held.text.as_str())
    }

    /// What is selected in the host's field named `name`, characters from 0: the caret alone where
    /// the range is empty.
    pub fn field_selection(&self, name: &str) -> Option<std::ops::Range<usize>> {
        self.seen.fields.get(&Id::named(name)).map(Held::selection)
    }

    /// The page's fields, what a screen reader says of it, its tooltips and its commands.
    pub fn outline(&self) -> &Outline {
        self.page.outline()
    }

    /// The pointer's shape the page last asked for, answering the pointer.
    pub fn cursor(&self) -> Option<Cursor> {
        self.seen.cursor
    }

    /// What is on the host's pasteboard: what the page or an editor copied, or the test pasted.
    pub fn pasteboard(&self) -> Option<&str> {
        self.seen.pasteboard.as_deref()
    }

    /// The links the page asked to open, in order (each one the sandbox let through).
    pub fn links(&self) -> &[String] {
        &self.seen.links
    }

    /// The menu the page asked for, while it is open (its place in the host's points).
    pub fn menu(&self) -> Option<&Menu> {
        self.seen.menu.as_ref()
    }

    /// Whether the page asked to be closed.
    pub fn closed(&self) -> bool {
        self.seen.closed
    }

    /// The words for the page's author of the images that could not be shown.
    pub fn diagnostics(&self) -> &[String] {
        &self.seen.diagnostics
    }

    /// Why the page ended, once it has.
    pub fn ended(&self) -> Option<&Fault> {
        self.seen.ended.as_ref()
    }

    /// What the page said to its program, in order: JSON objects, as written.
    pub fn sent(&self) -> &[String] {
        &self.seen.sent
    }

    /// What the page's program said to it, in order: JSON objects, as written.
    pub fn received(&self) -> &[String] {
        &self.seen.received
    }

    /// Every reply of the page, in order, each with whether it answered the pointer — as the module
    /// put it out, before the host did any of it.
    pub fn replies(&self) -> &[Answer] {
        &self.seen.replies
    }

    /// Every event the page was told, in order: what the person did, the editors' events, the boxes
    /// scrolled, the commands.
    pub fn told(&self) -> &[Event] {
        self.page.told()
    }

    /// The drive of the page, as it is.
    pub fn page(&self) -> &Page {
        &self.page
    }

    /// The drive of the page, to do to it what this host has no method for; settle it after
    /// ([`Headless::settle`]).
    pub fn page_mut(&mut self) -> &mut Page {
        &mut self.page
    }
}

impl Headless<Calls> {
    /// The program pushes `value` to the page's subscriptions of `topic`: how many there were.
    ///
    /// # Errors
    ///
    /// As [`Headless::settle`].
    pub fn push(&mut self, topic: &str, value: &Value) -> Result<usize, Error> {
        let pushed = self.peer.push(topic, value);
        self.settle()?;
        Ok(pushed)
    }

    /// The program answers the oldest call of `method` that waits for an answer: whether one did.
    ///
    /// # Errors
    ///
    /// As [`Headless::settle`].
    pub fn answer(&mut self, method: &str, answer: Result<Value, Failure>) -> Result<bool, Error> {
        let answered = self.peer.answer(method, answer);
        self.settle()?;
        Ok(answered)
    }

    /// The methods the page called, in order.
    pub fn methods(&self) -> Vec<&str> {
        self.peer.methods()
    }
}

/// Where the tree of the node at `at` of `nodes` ends (the index after its last node): each node is
/// followed by its children, each by its own.
fn subtree_end(nodes: &[pencere_view::wire::Node], at: usize) -> usize {
    let mut end = at.saturating_add(1);
    let mut owed = nodes.get(at).map_or(0, |node| node.children);
    while owed > 0 {
        let Some(node) = nodes.get(end) else {
            break;
        };
        owed = owed - 1 + node.children;
        end += 1;
    }
    end
}

fn middle(rect: Rect) -> Point {
    Point::new(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0)
}

fn contains(rect: Rect, at: Point) -> bool {
    at.x >= rect.x && at.y >= rect.y && at.x < rect.x + rect.w && at.y < rect.y + rect.h
}
