//! The program's view, shown on the terminal it was started in.
//!
//! [`run`] is what a program's options hand a terminal to when no terminal hosts the program's view
//! ([`pencere::Options::screen`]): it takes the view the program carries (the first of the kind this
//! crate draws), runs its page in this process, speaks to the program's handlers as a terminal
//! would carry the six messages (`pencere::Conversation`), and draws the page's frames in the cells
//! of the terminal, writing only what changed.
//!
//! One thread does it all and waits where there is nothing to do: for the terminal's bytes, for
//! the page (its module and its images wake the wait), for the handlers (they wake it too), for a
//! signal, and for the time the page asked to be woken at.

use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pencere::{
    Bundle, Closed, Conversation, EndedBy, Kind, Options, Said, Shown, Size, Tty, View, Waker,
};
use pencere_drive::{FrameAt, Host, Page, Route, Setup};
use pencere_sandbox::{Assets, Fault, Program, Script};
use pencere_view::{
    Button, Cursor, FieldKind, Fill, Id, Input, Key, KeyConvention, Keyboard, Menu, Mods, Panel,
    Phase, Point, copy_key,
};

use crate::field::{TextField, draw_fields};
use crate::input::{Decoder, Event, KeyEvent, Mouse, MouseKind};
use crate::measure::Cells;
use crate::output::base64;
use crate::paint::{CanvasMode, ImageMode, Options as Paint, render};
use crate::probe::{self, Found, Guess};
use crate::screen::Screen;
use crate::theme::Theme;

/// The version of the kit's interface (the `lua` view's) this screen runs.
pub const KIT_VERSION: &str = "0.1";

/// The pixels of the page a column is across and a row is down: the page is laid out on cells of
/// this size, and `render` takes them back to cells.
const COLUMN: f32 = 8.0;
const ROW: f32 = 18.0;

/// The rows a turn of the wheel moves.
const WHEEL_ROWS: f32 = 3.0;

/// The longest between two frames of a page that moves, and the shortest between any two.
const MOVING_GAP: f64 = 1.0 / 30.0;
const LEAST_GAP: f64 = 0.004;

/// The longest the page is waited for to answer something the person did, before the next is done.
const ANSWER: Duration = Duration::from_millis(100);

/// How many frames in a row are drawn without a picture that is not there yet, a twentieth of a
/// second apart, before it is given up.
const PATIENCE_FRAMES: u32 = 100;

/// How long a lone escape waits to be a key and not the start of a sequence.
const ESCAPE_WAIT: f64 = 0.03;

/// How long a second press is the same click's.
const DOUBLE: Duration = Duration::from_millis(400);

/// The modes the screen is put in: no wrap at the edge, no cursor until a field has one, the mouse
/// (buttons, drags and motion, in the long form), pastes in brackets, and the focus.
const ON: &[u8] = b"\x1b[?7l\x1b[?25l\x1b[?1002h\x1b[?1003h\x1b[?1006h\x1b[?2004h\x1b[?1004h";
/// What puts them back.
const OFF: &[u8] =
    b"\x1b[?1004l\x1b[?2004l\x1b[?1006l\x1b[?1003l\x1b[?1002l\x1b[?25h\x1b[?7h\x1b[0m";
/// The Kitty keyboard protocol's flags taken back, which comes before the rest where it was asked for.
const KEYS_OFF: &[u8] = b"\x1b[<u";
/// The pointer's shape given back to the terminal's own (an empty `OSC 22`), before the screen is
/// left, where the page asked for one.
const POINTER_OFF: &[u8] = b"\x1b]22;\x1b\\";

/// What is written, before the screen is left, whenever the terminal is given back: the modes put
/// back, the keyboard's flags taken back where they were asked for and the pointer's shape where the
/// page chose one.
fn off(keys: bool, pointer: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    if keys {
        bytes.extend_from_slice(KEYS_OFF);
    }
    if pointer {
        bytes.extend_from_slice(POINTER_OFF);
    }
    bytes.extend_from_slice(OFF);
    bytes
}

/// The shape of the pointer that is the page's, by the names Kitty's pointer-shape protocol
/// (`OSC 22`) reads, which are CSS's. A terminal that does not know the protocol ignores it.
fn pointer_name(cursor: Cursor) -> &'static str {
    match cursor {
        Cursor::Arrow => "default",
        Cursor::Pointer => "pointer",
        Cursor::Text => "text",
        Cursor::ResizeHorizontal => "ew-resize",
        Cursor::ResizeVertical => "ns-resize",
        Cursor::NotAllowed => "not-allowed",
        Cursor::Grab => "grab",
        Cursor::Grabbing => "grabbing",
        Cursor::Crosshair => "crosshair",
    }
}

/// The files of the view's bundle, as the page reads them.
struct Files(Bundle);

impl Assets for Files {
    fn read(&self, path: &str) -> Option<Arc<[u8]>> {
        self.0.get(path).map(|(_, data)| Arc::from(data))
    }

    fn media_type(&self, path: &str) -> Option<String> {
        self.0
            .get(path)
            .map(|(media_type, _)| media_type.to_owned())
    }
}

/// What the page asks of its program's system: the time on the clock the frames run on, and the
/// keyboard of a terminal that is not a Mac's. The program has the room for whatever the page says
/// (the handlers are read as fast as they write).
struct Clock {
    started: Instant,
}

impl Program for Clock {
    fn may_say(&self, _bytes: usize) -> bool {
        true
    }

    fn taken(&self, _bytes: usize) {}

    fn now(&self) -> Option<f64> {
        Some(self.started.elapsed().as_secs_f64())
    }

    fn keyboard(&self) -> KeyConvention {
        KeyConvention::Linux
    }
}

/// Why the screen ends.
struct Stop {
    by: EndedBy,
    code: Option<String>,
    message: Option<String>,
}

impl Stop {
    fn by(by: EndedBy) -> Self {
        Self {
            by,
            code: None,
            message: None,
        }
    }

    /// The terminal could not be written to or read from: it is gone, or a signal ended the wait.
    fn io(tty: &Tty<'_>, error: &io::Error) -> Self {
        if error.kind() == io::ErrorKind::Interrupted
            && let Some(signal) = tty.take_signal()
        {
            return Self::by(EndedBy::Signal(signal));
        }
        Self::by(EndedBy::Terminal)
    }

    fn fault(fault: &Fault) -> Self {
        Self {
            by: EndedBy::Error,
            code: Some("view".to_owned()),
            message: Some(fault.to_string()),
        }
    }
}

type Flow = Result<(), Stop>;

/// What the page said in a reply, gathered to be done in the order a host does it.
#[derive(Default)]
struct Effects {
    sent: Vec<String>,
    close: bool,
    redraw: bool,
    fills: Vec<(Id, Fill)>,
    keyboard: Option<Keyboard>,
    copies: Vec<(String, bool)>,
    ended: Option<Fault>,
    notes: Vec<String>,
    /// The shape the pointer is to take, the last the page asked for.
    cursor: Option<Cursor>,
}

impl Host for Effects {
    fn send(&mut self, message: String) {
        self.sent.push(message);
    }

    fn close(&mut self) {
        self.close = true;
    }

    fn redraw(&mut self) {
        self.redraw = true;
    }

    fn cursor(&mut self, cursor: Cursor) {
        self.cursor = Some(cursor);
    }

    fn fill(&mut self, field: Id, fill: Fill) {
        self.fills.push((field, fill));
    }

    fn keyboard(&mut self, to: Keyboard) {
        self.keyboard = Some(to);
    }

    fn copy(&mut self, text: String, by_person: bool) {
        self.copies.push((text, by_person));
    }

    fn link(&mut self, url: String) {
        self.notes.push(format!(
            "the page asked to open {url}, which this screen does not do"
        ));
    }

    fn menu(&mut self, _menu: Menu) {
        self.notes
            .push("the page asked for a menu, which this screen does not show".to_owned());
    }

    fn panel(&mut self, _panel: Panel) {
        self.notes.push(
            "the page asked for files or a place to save in, which this screen does not give"
                .to_owned(),
        );
    }

    fn diagnostic(&mut self, words: String) {
        self.notes.push(words);
    }

    fn ended(&mut self, why: Fault) {
        self.ended = Some(why);
    }
}

/// The last press, for counting a double click.
struct Press {
    cell: (u16, u16),
    button: Button,
    at: Instant,
    count: u32,
}

/// Where the time of the start goes, said when the screen ends if `PENCERE_TUI_TRACE` is set: each
/// mark with the seconds since the screen was entered.
struct Trace {
    began: Instant,
    marks: Option<Vec<(f64, String)>>,
}

/// The frames of the start the trace says the size of.
const TRACED_FRAMES: usize = 6;

impl Trace {
    fn new() -> Self {
        Self {
            began: Instant::now(),
            marks: std::env::var_os("PENCERE_TUI_TRACE").map(|_| Vec::new()),
        }
    }

    /// Notes what has been done now. The words are made only where the trace is asked for.
    fn mark(&mut self, words: impl FnOnce() -> String) {
        let at = self.began.elapsed().as_secs_f64();
        if let Some(marks) = &mut self.marks {
            marks.push((at, words()));
        }
    }

    /// How many marks the frames have.
    fn frames(&self) -> usize {
        self.marks.as_ref().map_or(usize::MAX, |marks| {
            marks
                .iter()
                .filter(|(_, words)| words.starts_with("frame"))
                .count()
        })
    }
}

/// The screen: the terminal, the page and the program, and what is known of what is shown.
struct Live<'t> {
    tty: Tty<'t>,
    clock: Arc<Clock>,
    waker: Waker,
    shown: Shown,
    bundle: Bundle,
    page: Page,
    conversation: Conversation,
    decoder: Decoder,
    screen: Screen,
    found: Found,
    size: Size,
    /// The host's text fields, by the name the page gave them, and which has the keyboard.
    fields: HashMap<Id, TextField>,
    focus: Option<Id>,
    /// The field a press is dragging the selection of.
    dragging: Option<Id>,
    pasteboard: Option<String>,
    press: Option<Press>,
    /// What came before the loop (typed while the terminal was asked what it is).
    early: Vec<Event>,
    /// A frame is due whatever the page says: the host changed something it shows.
    dirty: bool,
    /// The last frame was drawn without a picture that is on its way, and how many frames in a row
    /// were: a picture that never comes is not waited for for ever.
    waiting: bool,
    waits: u32,
    last_frame: f64,
    /// When a lone escape is a key.
    escape_at: Option<f64>,
    /// What the page asked that this screen does not do, said when it ends.
    notes: Vec<String>,
    /// The Kitty keyboard protocol was switched on, and is taken back.
    keys: bool,
    /// The shape the pointer was last given by `OSC 22`; none while the terminal's own is as it
    /// was.
    pointer: Option<Cursor>,
    /// The shape the page last asked for, and where the pointer last was (in the page's points).
    told: Cursor,
    pointer_at: Option<Point>,
    trace: Trace,
}

/// Shows the view the options carry on `tty`, until it ends.
///
/// # Errors
///
/// When there is no view this screen draws, or the page cannot be started.
pub fn run(options: Options, tty: Tty<'_>) -> io::Result<Closed> {
    let mut trace = Trace::new();
    let (shown, bundle) = choose(options.views())?;
    tty.enter(ON, OFF)?;
    trace.mark(|| "the alternate screen entered".to_owned());
    let clock = Arc::new(Clock {
        started: Instant::now(),
    });
    let waker = tty.waker();
    // The page is started before the terminal is asked what it is: its interpreter is made on a
    // thread of its own, while the answers come.
    let page = open_page(&bundle, &clock, &waker)?;
    trace.mark(|| "the page started".to_owned());
    let mut decoder = Decoder::new();
    let mut early = Vec::new();
    let guess = Guess::from_environment();
    let found = match probe::ask(&tty, &mut decoder, &mut early, &guess) {
        Ok(found) => found,
        Err(signal) => return Ok(closed(shown, &Stop::by(EndedBy::Signal(signal)))),
    };
    trace.mark(|| {
        let how = if found.fenced {
            "answered to the end"
        } else {
            "did not answer to the end"
        };
        format!(
            "the terminal {how} (colours {}, Kitty graphics {}, Kitty keyboard {})",
            found.colors, found.graphics, found.keyboard
        )
    });
    let keys = found.keyboard && tty.write_all(b"\x1b[>1u").is_ok();
    if keys {
        tty.leaving(&off(true, false));
    }
    let conversation = Conversation::waking(options, &waker);
    let size = tty.size();
    let mut live = Live {
        tty,
        clock,
        waker,
        shown,
        bundle,
        page,
        conversation,
        decoder,
        screen: Screen::new(found.depth),
        found,
        size,
        fields: HashMap::new(),
        focus: None,
        dragging: None,
        pasteboard: None,
        press: None,
        early,
        dirty: true,
        waiting: false,
        waits: 0,
        last_frame: 0.0,
        escape_at: None,
        notes: Vec::new(),
        keys,
        pointer: None,
        told: Cursor::Arrow,
        pointer_at: None,
        trace,
    };
    let stop = live.lead();
    Ok(live.end(&stop))
}

/// The view this screen draws among the program's: the first of the kind, in the version it runs.
fn choose(views: &[View]) -> io::Result<(Shown, Bundle)> {
    let view = views
        .iter()
        .find(|view| view.kind() == Kind::Lua && view.versions().iter().any(|v| v == KIT_VERSION))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                "the program carries no `lua` view for this terminal to draw",
            )
        })?;
    let bundle = view
        .bundle(KIT_VERSION)
        .map_err(|error| io::Error::other(error.to_string()))?;
    Ok((
        Shown {
            kind: Kind::Lua,
            version: KIT_VERSION.to_owned(),
        },
        bundle,
    ))
}

/// The page of the bundle, running on its module's thread.
fn open_page(bundle: &Bundle, clock: &Arc<Clock>, waker: &Waker) -> io::Result<Page> {
    let files: Arc<dyn Assets> = Arc::new(Files(bundle.clone()));
    let entry = bundle.entry();
    let (_, source) = bundle
        .get(entry)
        .ok_or_else(|| io::Error::other("the view's bundle has no entry"))?;
    let script = Script {
        path: entry.to_owned(),
        source: source.to_vec(),
        bundle: Some(Arc::clone(&files)),
    };
    let setup = Setup {
        program: Arc::clone(clock) as Arc<dyn Program>,
        assets: Some(files),
        measure: Box::new(Cells::new(COLUMN, ROW)),
        ..Setup::default()
    };
    let wake = waker.clone();
    Ok(Page::script(script, setup, move || wake.wake()))
}

fn closed(shown: Shown, stop: &Stop) -> Closed {
    Closed {
        by: stop.by,
        view: shown,
        code: stop.code.clone(),
        message: stop.message.clone(),
    }
}

impl Live<'_> {
    /// Seconds on the clock the frames run on.
    fn now(&self) -> f64 {
        self.clock.started.elapsed().as_secs_f64()
    }

    /// Runs until something ends the screen.
    fn lead(&mut self) -> Stop {
        let mut buffer = vec![0_u8; 16 * 1024];
        let early = std::mem::take(&mut self.early);
        for event in early {
            if let Err(stop) = self.dispatch(event) {
                return stop;
            }
        }
        loop {
            if let Err(stop) = self.turn(&mut buffer) {
                return stop;
            }
        }
    }

    /// One wait and what it brought.
    fn turn(&mut self, buffer: &mut [u8]) -> Flow {
        let waited = self.timeout();
        let ready = self
            .tty
            .wait(waited)
            .map_err(|error| Stop::io(&self.tty, &error))?;
        if let Some(signal) = self.tty.take_signal() {
            return Err(Stop::by(EndedBy::Signal(signal)));
        }
        // A stop from outside is no more than a wake: the keys that stop a program are off.
        let _ = self.tty.take_stop();
        if self.tty.take_resize() {
            self.size = self.tty.size();
            self.screen.forget();
            self.dirty = true;
            let size = self.size;
            self.trace
                .mark(|| format!("the window is {} x {}", size.cols, size.rows));
        }
        if ready.input {
            self.read(buffer)?;
        } else if ready.hangup {
            return Err(Stop::by(EndedBy::Terminal));
        }
        self.escape()?;
        self.pump()?;
        self.show_pointer()?;
        self.maybe_frame()
    }

    /// How long to wait: till the next thing that is due, for ever where nothing is.
    fn timeout(&self) -> Option<Duration> {
        let now = self.now();
        let floor = self.last_frame + LEAST_GAP;
        let mut next: Option<f64> = None;
        let mut soon = |at: f64| next = Some(next.map_or(at, |was| was.min(at)));
        if self.dirty || self.page.unsettled() {
            soon(floor);
        }
        if let Some(at) = self.page.wake_at() {
            soon(at.max(floor));
        }
        if self.page.animating() {
            soon((self.last_frame + MOVING_GAP).max(floor));
        }
        if self.waiting {
            soon(self.last_frame + 0.05);
        }
        if let Some(at) = self.escape_at {
            soon(at);
        }
        next.map(|at| Duration::from_secs_f64((at - now).max(0.0)))
    }

    // --- The terminal's bytes -----------------------------------------------------------------

    fn read(&mut self, buffer: &mut [u8]) -> Flow {
        loop {
            let got = self
                .tty
                .read(buffer)
                .map_err(|error| Stop::io(&self.tty, &error))?;
            if got == 0 {
                return Ok(());
            }
            let mut events = Vec::new();
            self.decoder
                .feed(buffer.get(..got).unwrap_or_default(), &mut events);
            self.escape_at = self.decoder.lone_escape().then(|| self.now() + ESCAPE_WAIT);
            for event in events {
                self.dispatch(event)?;
            }
            if got < buffer.len() {
                return Ok(());
            }
        }
    }

    /// An escape that waited and nothing followed is the key.
    fn escape(&mut self) -> Flow {
        if self.escape_at.is_some_and(|at| at <= self.now()) {
            self.escape_at = None;
            let mut events = Vec::new();
            self.decoder.escape(&mut events);
            for event in events {
                self.dispatch(event)?;
            }
        }
        Ok(())
    }

    fn dispatch(&mut self, event: Event) -> Flow {
        // What the person does is done in order, and the page's answer to each is had before the
        // next: a key that asks for the keyboard for a field is answered before the keys typed
        // right after it (a burst, a paste with no brackets, a test) are told to the page.
        let answered = match event {
            Event::Key(key) => {
                self.key(key)?;
                true
            }
            Event::Mouse(mouse) => {
                let answered = matches!(mouse.kind, MouseKind::Press(_) | MouseKind::Release(_));
                self.mouse(mouse);
                answered
            }
            Event::Paste(text) => {
                self.paste(&text);
                true
            }
            Event::Focus(active) => {
                self.page.set_active(active);
                false
            }
            // The answer to a question that was not waited for.
            Event::Reply(_) => false,
        };
        if answered {
            self.page.module().flush(ANSWER);
            self.pump()?;
        }
        Ok(())
    }

    // --- The keyboard -------------------------------------------------------------------------

    fn key(&mut self, event: KeyEvent) -> Flow {
        let KeyEvent {
            key,
            mods,
            text,
            down,
        } = event;
        // ⌃C is the interrupt, wherever the keyboard is: as a key where the terminal reports keys
        // (it is not a byte the line discipline turns into a signal then).
        if mods.control
            && !mods.shift
            && !mods.alt
            && !mods.command
            && matches!(key, Key::Character(c) if c.eq_ignore_ascii_case(&'c'))
        {
            return Err(Stop::by(EndedBy::Signal(2)));
        }
        if !down {
            self.page.key_up(key, mods);
            return Ok(());
        }
        if let Some(id) = self.focused_field() {
            self.field_key(id, key, mods, text.as_deref());
            return Ok(());
        }
        match self.page.key(key, mods) {
            Route::Type => {
                let typed = text.or_else(|| match key {
                    Key::Character(c) => Some(c.to_string()),
                    _ => None,
                });
                if let Some(typed) = typed {
                    self.page.editors().type_text(&typed);
                }
            }
            Route::Copy(text) | Route::Cut(text) => self.copy(text, copy_key(key, mods))?,
            Route::Paste => {
                if let Some(text) = self.pasteboard.clone() {
                    self.page.editors().paste(&text);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The field that has the keyboard, if the page still has it.
    fn focused_field(&self) -> Option<Id> {
        let id = self.focus?;
        self.page
            .fields()
            .iter()
            .any(|field| field.id == id)
            .then_some(id)
    }

    fn kind_of(&self, id: Id) -> FieldKind {
        self.page
            .fields()
            .iter()
            .find(|field| field.id == id)
            .map_or(FieldKind::Line, |field| field.kind)
    }

    /// A key at a field: what it has a use for it keeps (the kit says which, `FieldState::takes`),
    /// and the page is told what it holds when that changed; the rest is the page's.
    fn field_key(&mut self, id: Id, key: Key, mods: Mods, text: Option<&str>) {
        let kind = self.kind_of(id);
        let field = self
            .fields
            .entry(id)
            .or_insert_with(|| TextField::new(kind));
        if !field.state().takes(key, mods) {
            self.page.input(Input::Key { key, mods });
            return;
        }
        let edited = field.key(key, mods, text);
        self.dirty = true;
        if edited.changed {
            let held = field.text().to_owned();
            self.page.text(id, &held);
        }
        if let Some((text, cut)) = edited.copy {
            // A copy or a cut by the field's own chord is the person's word.
            let _ = cut;
            self.pasteboard = Some(text.clone());
            self.write_pasteboard(&text);
        }
    }

    fn paste(&mut self, text: &str) {
        if let Some(id) = self.focused_field() {
            let kind = self.kind_of(id);
            let field = self
                .fields
                .entry(id)
                .or_insert_with(|| TextField::new(kind));
            if field.paste(text) {
                let held = field.text().to_owned();
                self.page.text(id, &held);
            }
            self.dirty = true;
        } else if self.page.focused_editor().is_some() {
            self.page.editors().paste(text);
        }
    }

    /// Text for the pasteboard: kept, and where the person's own word asked for it, written to the
    /// terminal's.
    fn copy(&mut self, text: String, by_person: bool) -> Flow {
        self.pasteboard = Some(text.clone());
        if by_person {
            self.write_pasteboard(&text);
        }
        Ok(())
    }

    /// The terminal's pasteboard, by OSC 52 (no more than a terminal takes: most hold 100 000
    /// bytes).
    fn write_pasteboard(&self, text: &str) {
        if text.len() > 100_000 {
            return;
        }
        let sequence = format!("\x1b]52;c;{}\x1b\\", base64(text.as_bytes()));
        let _ = self.tty.write_all(sequence.as_bytes());
    }

    // --- The pointer --------------------------------------------------------------------------

    fn point(mouse: &Mouse) -> Point {
        Point::new(
            (f32::from(mouse.col) + 0.5) * COLUMN,
            (f32::from(mouse.row) + 0.5) * ROW,
        )
    }

    fn mouse(&mut self, mouse: Mouse) {
        let at = Self::point(&mouse);
        self.pointer_at = Some(at);
        match mouse.kind {
            MouseKind::Move => self.page.input(Input::Move {
                at,
                mods: mouse.mods,
            }),
            MouseKind::Drag(_) => {
                if let Some(id) = self.dragging {
                    self.place_caret(id, &mouse, true);
                } else {
                    self.page.input(Input::Move {
                        at,
                        mods: mouse.mods,
                    });
                }
            }
            MouseKind::Press(button) => self.pressed(&mouse, button),
            MouseKind::Release(button) => {
                if self.dragging.take().is_none() {
                    self.page.input(Input::Release {
                        at,
                        button,
                        mods: mouse.mods,
                    });
                }
            }
            MouseKind::Wheel { dx, dy } => {
                self.page.wheel(
                    at,
                    f32::from(dx) * WHEEL_ROWS * COLUMN,
                    f32::from(dy) * WHEEL_ROWS * ROW,
                    Phase::Wheel,
                    false,
                );
            }
            // Said which button by the decoder.
            MouseKind::Up => {}
        }
    }

    fn pressed(&mut self, mouse: &Mouse, button: Button) {
        let at = Self::point(mouse);
        let hit = self
            .page
            .fields()
            .iter()
            .find(|field| {
                !field.covered
                    && at.x >= field.rect.x
                    && at.x < field.rect.x + field.rect.w
                    && at.y >= field.rect.y
                    && at.y < field.rect.y + field.rect.h
            })
            .map(|field| (field.id, field.kind));
        if let Some((id, kind)) = hit {
            self.fields
                .entry(id)
                .or_insert_with(|| TextField::new(kind));
            if self.focus != Some(id) {
                self.focus = Some(id);
                self.page.focus(Some(id));
            }
            self.dragging = Some(id);
            self.place_caret(id, mouse, mouse.mods.shift);
            return;
        }
        if self.focus.take().is_some() {
            self.page.focus(None);
            self.dirty = true;
        }
        let now = Instant::now();
        let cell = (mouse.col, mouse.row);
        let count = match &self.press {
            Some(last)
                if last.button == button
                    && last.cell == cell
                    && now.duration_since(last.at) < DOUBLE =>
            {
                last.count + 1
            }
            _ => 1,
        };
        self.press = Some(Press {
            cell,
            button,
            at: now,
            count,
        });
        self.page.input(Input::Press {
            at,
            button,
            mods: mouse.mods,
            count,
        });
    }

    /// The caret of the field `id` goes to the cell the pointer is over.
    fn place_caret(&mut self, id: Id, mouse: &Mouse, extend: bool) {
        let Some(room) = self
            .page
            .fields()
            .iter()
            .find(|field| field.id == id)
            .map(|field| field.rect)
        else {
            return;
        };
        let first = (room.x / COLUMN).round().max(0.0) as u16;
        let width = (room.w / COLUMN).round().max(1.0) as u16;
        if let Some(field) = self.fields.get_mut(&id) {
            let cell = usize::from(mouse.col.saturating_sub(first));
            field.place(cell, width, extend);
            self.dirty = true;
        }
    }

    // --- What the page says -------------------------------------------------------------------

    /// Takes what the page has said and gives it where it goes, and the other way round, until
    /// neither has more.
    fn pump(&mut self) -> Flow {
        for _ in 0..32 {
            let drained = self.page.drain();
            let mut effects = Effects::default();
            drained.deliver(&mut effects);
            let sent = !effects.sent.is_empty();
            self.apply(effects)?;
            let heard = self.hear()?;
            if !sent && !heard {
                break;
            }
        }
        Ok(())
    }

    fn apply(&mut self, effects: Effects) -> Flow {
        for message in &effects.sent {
            self.conversation.send(message);
        }
        if effects.redraw {
            self.dirty = true;
        }
        // The texts before the keyboard: the field that is told it has it holds what it is to edit.
        for (id, fill) in &effects.fills {
            let kind = self.kind_of(*id);
            self.fields
                .entry(*id)
                .or_insert_with(|| TextField::new(kind))
                .fill(fill);
            self.dirty = true;
        }
        if let Some(to) = effects.keyboard {
            self.focus = match to {
                Keyboard::Field(id) => Some(id),
                _ => None,
            };
            self.dirty = true;
        }
        if let Some(cursor) = effects.cursor {
            self.told = cursor;
        }
        for (text, by_person) in effects.copies {
            self.copy(text, by_person)?;
        }
        for note in effects.notes {
            if self.notes.len() < 20 && !self.notes.contains(&note) {
                self.notes.push(note);
            }
        }
        if let Some(fault) = &effects.ended {
            return Err(Stop::fault(fault));
        }
        if effects.close {
            return Err(Stop::by(EndedBy::View));
        }
        Ok(())
    }

    /// The shape the pointer is to have: the page's, and the I-beam over the room of a field of the
    /// host's where the page says the arrow (the field is the host's, and the page does not draw
    /// it).
    fn show_pointer(&mut self) -> Flow {
        let over_field = self.pointer_at.is_some_and(|at| {
            self.page.fields().iter().any(|field| {
                !field.covered
                    && at.x >= field.rect.x
                    && at.x < field.rect.x + field.rect.w
                    && at.y >= field.rect.y
                    && at.y < field.rect.y + field.rect.h
            })
        });
        let wanted = if self.told == Cursor::Arrow && over_field {
            Cursor::Text
        } else {
            self.told
        };
        self.take_pointer(wanted)
    }

    /// The pointer takes the shape asked for, where it is not the one it has: written only
    /// when it changes, and given back with the terminal (the first time one is chosen). The arrow
    /// is what the terminal has until a shape was chosen.
    fn take_pointer(&mut self, cursor: Cursor) -> Flow {
        if self.pointer.unwrap_or_default() == cursor {
            return Ok(());
        }
        if self.pointer.is_none() {
            self.tty.leaving(&off(self.keys, true));
        }
        self.pointer = Some(cursor);
        let bytes = format!("\x1b]22;{}\x1b\\", pointer_name(cursor));
        self.tty
            .write_all(bytes.as_bytes())
            .map_err(|error| Stop::io(&self.tty, &error))
    }

    /// What the program said, to the page. Whether it said anything.
    fn hear(&mut self) -> Result<bool, Stop> {
        let mut any = false;
        for _ in 0..256 {
            let Some(said) = self.conversation.try_receive() else {
                break;
            };
            any = true;
            match said {
                Said::Message(text) => self.page.message(&text),
                Said::Closed => return Err(Stop::by(EndedBy::Program)),
                Said::Reopened(views) => self.reopen(views.as_deref())?,
                Said::HandedOff(lent) => {
                    // Dropped, the program is told the command could not be started.
                    drop(lent);
                    let note = "the program asked to lend the terminal to a command, which this \
                                screen does not do"
                        .to_owned();
                    if !self.notes.contains(&note) {
                        self.notes.push(note);
                    }
                }
                _ => {}
            }
        }
        Ok(any)
    }

    /// The program shows its view anew: the page is made again, from the view it brings (or the
    /// same one).
    fn reopen(&mut self, views: Option<&[View]>) -> Flow {
        if let Some(views) = views
            && let Ok((shown, bundle)) = choose(views)
        {
            self.shown = shown;
            self.bundle = bundle;
        }
        let page = open_page(&self.bundle, &self.clock, &self.waker).map_err(|error| Stop {
            by: EndedBy::Error,
            code: Some("view".to_owned()),
            message: Some(error.to_string()),
        })?;
        self.page = page;
        self.fields.clear();
        self.focus = None;
        self.dragging = None;
        self.dirty = true;
        Ok(())
    }

    // --- Frames -------------------------------------------------------------------------------

    fn maybe_frame(&mut self) -> Flow {
        let now = self.now();
        let floor = self.last_frame + LEAST_GAP;
        let due = self.dirty
            || self.page.unsettled()
            || self.page.wake_at().is_some_and(|at| at <= now)
            || (self.page.animating() && now >= self.last_frame + MOVING_GAP)
            || (self.waiting && now >= self.last_frame + 0.05);
        if due && now >= floor {
            self.frame()?;
        }
        Ok(())
    }

    /// The page, laid out and painted for the window as it is, and what changed of it written.
    fn frame(&mut self) -> Flow {
        let (cols, rows) = (self.size.cols, self.size.rows);
        let now = self.now();
        self.last_frame = now;
        self.dirty = false;
        let mut painted = self.page.frame(FrameAt {
            size: (
                u32::from(cols) * COLUMN as u32,
                u32::from(rows) * ROW as u32,
            ),
            scale: 1.0,
            now,
            // A terminal redraws what moves cell by cell, and the page's glides cost a frame of
            // bytes each.
            reduce_motion: true,
        });
        let mut effects = Effects::default();
        painted.deliver_held(&mut effects);
        self.apply(effects)?;
        let present: Vec<Id> = self.page.fields().iter().map(|field| field.id).collect();
        self.fields.retain(|id, _| present.contains(id));
        if self.focus.is_some_and(|id| !present.contains(&id)) {
            self.focus = None;
        }
        let theme = self.found.theme;
        let options = Paint {
            col: COLUMN,
            row: ROW,
            images: if self.found.graphics {
                ImageMode::Kitty
            } else {
                ImageMode::Blocks
            },
            canvases: CanvasMode::Characters,
            pixels: self.cell_pixels(),
        };
        let mut frame = render(
            &painted.list,
            cols,
            rows,
            &theme,
            &self.page.images(),
            &options,
        );
        self.waits = if frame.waiting { self.waits + 1 } else { 0 };
        self.waiting = frame.waiting && self.waits < PATIENCE_FRAMES;
        let cursor = self.draw_fields(&mut frame.grid, &theme);
        let bytes = self.screen.draw(&frame, cursor);
        if self.trace.frames() < TRACED_FRAMES {
            let sent = bytes.len();
            self.trace.mark(|| format!("frame, {sent} bytes written"));
        }
        if !bytes.is_empty() {
            self.tty
                .write_all(&bytes)
                .map_err(|error| Stop::io(&self.tty, &error))?;
        }
        Ok(())
    }

    /// The pixels of a cell of the terminal, where it says (a guess where it does not).
    fn cell_pixels(&self) -> (u32, u32) {
        let (cols, rows) = (u32::from(self.size.cols), u32::from(self.size.rows));
        if self.size.width > 0 && self.size.height > 0 && cols > 0 && rows > 0 {
            (
                (u32::from(self.size.width) / cols).max(1),
                (u32::from(self.size.height) / rows).max(1),
            )
        } else {
            Paint::default().pixels
        }
    }

    /// The host's fields drawn over their rooms; where the caret of the one that has the keyboard
    /// is.
    fn draw_fields(&self, grid: &mut crate::Grid, theme: &Theme) -> Option<(u16, u16)> {
        draw_fields(
            grid,
            self.page.fields(),
            &self.fields,
            self.focus,
            (COLUMN, ROW),
            theme,
        )
    }

    // --- The end ------------------------------------------------------------------------------

    /// Gives the terminal back, and says what the page asked that this screen did not do.
    fn end(self, stop: &Stop) -> Closed {
        let Live {
            tty,
            shown,
            page,
            conversation,
            notes,
            trace,
            ..
        } = self;
        drop(tty);
        drop(page);
        drop(conversation);
        for note in notes {
            eprintln!("pencere: {note}");
        }
        for (at, words) in trace.marks.into_iter().flatten() {
            eprintln!("pencere: trace +{at:.3} s {words}");
        }
        closed(shown, stop)
    }
}
