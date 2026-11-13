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
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use pencere::{
    Bundle, Closed, Conversation, EndedBy, Kind, Lent, Options, Said, Shown, Size, Tty, View, Waker,
};
use pencere_drive::{CarryOut, FrameAt, Host, Page, Route, Setup};
use pencere_sandbox::{Assets, Fault, Program, Script};
use pencere_view::{
    Button, Cursor, Drawing, FieldKind, FileSource, Fill, Id, Input, Key, KeyConvention, Keyboard,
    Menu, Mods, Panel, Phase, Point, copy_key,
};

use crate::field::{TextField, draw_fields};
use crate::input::{Decoder, Event, KeyEvent, Mouse, MouseKind, Reply};
use crate::measure::Cells;
use crate::memory;
use crate::output::base64;
use crate::overlay::{self, Answered, Chose, Handles, LinkBar, MenuBox, PanelBar};
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
/// back, the keyboard's flags taken back where they were asked for, the pointer's shape where the
/// page chose one and the cursor's where it was made a bar, and the pictures the terminal keeps
/// given up where it was sent some.
fn off(keys: bool, pointer: bool, pictures: bool, bar: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    if pictures {
        bytes.extend_from_slice(crate::output::FREE_ALL.as_bytes());
    }
    if bar {
        bytes.extend_from_slice(crate::screen::OWN_SHAPE.as_bytes());
    }
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

/// The files the program attached to the view (specification §8.8), by the name the page draws them
/// by (`sha256:` and the digest): the screen holds them, within the limits a bundle has, for as long as
/// the program says (they are kept when the view is shown again, as a program's files are kept when
/// it opens its view again: the page asks for them by name, and the program names them again).
#[derive(Default)]
struct Attached {
    files: Mutex<HashMap<String, Arc<[u8]>>>,
}

impl Attached {
    /// Holds `files`, all or none: not when one is larger than a file may be, or when they are more
    /// than the view may have (the ones already held, one that is held again not counted twice).
    fn hold(&self, files: &[(String, Arc<[u8]>)]) -> bool {
        let mut held = self.files.lock().unwrap_or_else(PoisonError::into_inner);
        let mut count = held.len();
        let mut total: u64 = held.values().map(|bytes| bytes.len() as u64).sum();
        let mut new: Vec<&str> = Vec::new();
        for (name, bytes) in files {
            let size = bytes.len() as u64;
            if size > pencere::MAX_ATTACHED_FILE {
                return false;
            }
            if held.contains_key(name) || new.contains(&name.as_str()) {
                continue;
            }
            count += 1;
            total += size;
            if count > pencere::MAX_ATTACHED_FILES || total > pencere::MAX_ATTACHED_TOTAL {
                return false;
            }
            new.push(name);
        }
        for (name, bytes) in files {
            held.insert(name.clone(), Arc::clone(bytes));
        }
        true
    }

    fn get(&self, name: &str) -> Option<Arc<[u8]>> {
        self.files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(name)
            .cloned()
    }

    /// Lets `names` go; the ones that were held.
    fn forget(&self, names: &[String]) -> Vec<String> {
        let mut held = self.files.lock().unwrap_or_else(PoisonError::into_inner);
        names
            .iter()
            .filter(|name| held.remove(name.as_str()).is_some())
            .cloned()
            .collect()
    }
}

/// The files of the view's bundle, as the page reads them, the files the program attached to it
/// (`sha256:` and the digest), and the files the person gave it: those by the handle the page was
/// given with the file (`file:` and the handle), never by a path.
struct Files {
    bundle: Bundle,
    handles: Arc<Handles>,
    attached: Arc<Attached>,
}

impl Assets for Files {
    fn read(&self, path: &str) -> Option<Arc<[u8]>> {
        if let Some(handle) = path.strip_prefix("file:") {
            return self.handles.read(handle).map(Arc::from);
        }
        if path.starts_with("sha256:") {
            return self.attached.get(path);
        }
        self.bundle.get(path).map(|(_, data)| Arc::from(data))
    }

    fn media_type(&self, path: &str) -> Option<String> {
        if let Some(handle) = path.strip_prefix("file:") {
            return self
                .handles
                .path_of(handle)
                .map(|path| overlay::media_type_of(&path));
        }
        // An attached file is read for what it is by its bytes: it has no type of its own.
        if path.starts_with("sha256:") {
            return None;
        }
        self.bundle
            .get(path)
            .map(|(media_type, _)| media_type.to_owned())
    }
}

/// How long a page that asks what is drawn waits for the terminal to say what it is: the probe's
/// own end (a terminal that does not answer is given up on much sooner) and a little more.
const LONGEST_ASKED: Duration = Duration::from_secs(10);

/// What the page asks of its program's system: the time on the clock the frames run on, what the
/// screen draws, and the keyboard of a terminal that is not a Mac's. The program has the room for
/// whatever the page says (the handlers are read as fast as they write).
struct Clock {
    started: Instant,
    /// Whether the terminal draws pictures (Kitty graphics), once the probe has found out: what is
    /// drawn is said from it, and the page is started before the probe, so a page that asks first
    /// waits here (on its own thread) for the answer.
    graphics: Mutex<Option<bool>>,
    found_out: Condvar,
}

impl Clock {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            graphics: Mutex::new(None),
            found_out: Condvar::new(),
        }
    }

    /// The probe is over: the terminal draws pictures or does not (`false` also when it did not
    /// answer, which is the guess that costs nothing).
    fn found(&self, graphics: bool) {
        if let Ok(mut said) = self.graphics.lock() {
            *said = Some(graphics);
        }
        self.found_out.notify_all();
    }

    /// Whether the terminal draws pictures, waiting for the probe; `false` when it takes too long.
    fn pictures(&self) -> bool {
        let Ok(said) = self.graphics.lock() else {
            return false;
        };
        match self
            .found_out
            .wait_timeout_while(said, LONGEST_ASKED, |said| said.is_none())
        {
            Ok((said, _)) => (*said).unwrap_or(false),
            Err(_) => false,
        }
    }
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

    /// A screen draws in the cells of a terminal, and a terminal that draws no pictures draws the
    /// ones a page has in half blocks.
    fn draws(&self) -> Vec<Drawing> {
        if self.pictures() {
            vec![Drawing::Cells]
        } else {
            vec![Drawing::Cells, Drawing::CoarseImages]
        }
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
    /// What the page asked to show the person: a menu, a panel, a link. One of each at most, the
    /// first: the screen shows one thing at a time.
    menu: Option<Menu>,
    panel: Option<Panel>,
    link: Option<String>,
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
        self.link.get_or_insert(url);
    }

    fn menu(&mut self, menu: Menu) {
        self.menu.get_or_insert(menu);
    }

    fn panel(&mut self, panel: Panel) {
        self.panel.get_or_insert(panel);
    }

    fn diagnostic(&mut self, words: String) {
        self.notes.push(words);
    }

    // A terminal has no text size to enlarge (its cells are the size they are) and no system drag
    // to start: the page is laid out at the scale of 1 and is not told `carried`, as a page is not
    // by a host that never `set_carry`s.
    fn text_size(&mut self, _points: i32) {}

    fn carry(&mut self, _carry: CarryOut) {}

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
/// mark with the seconds since the screen was entered and the memory the process holds then, and at
/// the end where the time of the keys and the frames went and the most memory the process held.
struct Trace {
    began: Instant,
    marks: Option<Vec<(f64, String)>>,
    /// When memory was last noted on its own, seconds since the screen was entered.
    noted: f64,
    spent: Spent,
}

/// What the keys and the frames took, in all.
#[derive(Default)]
struct Spent {
    keys: u32,
    /// Telling the page of the key.
    told: Duration,
    /// Waiting for the page to have done what it was told.
    answered: Duration,
    /// Giving what the page said to where it goes.
    pumped: Duration,
    frames: u32,
    drawn: Duration,
    /// The pictures the frames carried, and the bytes of their pixels.
    pictures: u64,
    pixels: u64,
    /// The bytes written to the terminal, in all.
    written: u64,
    /// What the program has said to the page.
    heard: u32,
}

/// How often the trace notes memory with nothing else to say.
const MEMORY_EVERY: f64 = 30.0;

/// The frames of the start the trace says the size of.
const TRACED_FRAMES: usize = 6;

impl Trace {
    fn new() -> Self {
        Self {
            began: Instant::now(),
            marks: std::env::var_os("PENCERE_TUI_TRACE").map(|_| Vec::new()),
            noted: 0.0,
            spent: Spent::default(),
        }
    }

    /// Notes what has been done now. The words are made only where the trace is asked for.
    fn mark(&mut self, words: impl FnOnce() -> String) {
        let at = self.began.elapsed().as_secs_f64();
        if let Some(marks) = &mut self.marks {
            let (now, most) = (memory::mib(memory::resident()), memory::mib(memory::peak()));
            marks.push((at, format!("{} (memory {now} MiB, most {most})", words())));
            self.noted = at;
        }
    }

    /// Notes that the program said something to the page: the first is a mark, for the memory the
    /// data takes.
    fn heard(&mut self) {
        self.spent.heard += 1;
        if self.spent.heard == 1 {
            self.mark(|| "the program's first message heard".to_owned());
        }
    }

    /// Notes the memory where it has not been for a while.
    fn remember(&mut self) {
        if self.marks.is_some() && self.began.elapsed().as_secs_f64() - self.noted >= MEMORY_EVERY {
            self.mark(|| "memory".to_owned());
        }
    }

    /// What the end says of the time and the memory, where the trace is asked for.
    fn summary(&self) -> Option<String> {
        self.marks.as_ref()?;
        let Spent {
            keys,
            told,
            answered,
            pumped,
            frames,
            drawn,
            pictures,
            pixels,
            written,
            heard: _,
        } = &self.spent;
        Some(format!(
            "{keys} keys: told the page {:.3} s, waited for it {:.3} s, gave on what it said {:.3} s; \
             {frames} frames drawn in {:.3} s, {written} bytes written, {pictures} pictures of {:.1} MiB of pixels in all; \
             the most memory held {} MiB",
            told.as_secs_f64(),
            answered.as_secs_f64(),
            pumped.as_secs_f64(),
            drawn.as_secs_f64(),
            *pixels as f64 / (1024.0 * 1024.0),
            memory::mib(memory::peak()),
        ))
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

/// Whether `event` is a key of movement (an arrow, a page, the ends) that the page takes as a key,
/// and `next` is the same key again: a key held down, or sent faster than the page answers. The
/// page is told each and waited for after the last: what a key of movement asks of the host (the
/// keyboard for a field) is not asked by the second of them where it was not by the first, and the
/// page is made once for the run (the engine does it for keys that follow one another, and only
/// for those the page takes as keys: one it binds to a command is a page each).
fn held(page: &Page, event: &Event, next: Option<&Event>) -> bool {
    let (Event::Key(key), Some(Event::Key(again))) = (event, next) else {
        return false;
    };
    key.down
        && again.down
        && key.key == again.key
        && key.mods == again.mods
        && !(key.mods.control || key.mods.alt || key.mods.command)
        && matches!(
            key.key,
            Key::Up
                | Key::Down
                | Key::Left
                | Key::Right
                | Key::PageUp
                | Key::PageDown
                | Key::Home
                | Key::End
        )
        && !page.binds(key.key, key.mods)
}

/// What is shown over the page.
enum Overlay {
    Menu(MenuBox),
    Panel(PanelBar),
    Link(LinkBar),
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
    /// How many pixels a cell is, across and down, as the terminal last said it (`CSI 16 t`); none
    /// before it has, and while it is asked again after the window changed.
    cell: Option<(u32, u32)>,
    size: Size,
    /// The host's text fields, by the name the page gave them, and which has the keyboard.
    fields: HashMap<Id, TextField>,
    focus: Option<Id>,
    /// The field a press is dragging the selection of.
    dragging: Option<Id>,
    pasteboard: Option<String>,
    press: Option<Press>,
    /// What is shown over the page (a menu, the line of a panel, a link) and has the keys and the
    /// pointer while it is.
    overlay: Option<Overlay>,
    /// The press that an overlay took: its release is the overlay's too, and the page, which would
    /// act on a button under the menu, is not told of it.
    press_taken: bool,
    /// The files the person gave the page.
    handles: Arc<Handles>,
    /// The files the program attached to the view: kept when the page is opened again (the view
    /// shown again, the terminal given back), let go when the program says so.
    attached: Arc<Attached>,
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
    /// The terminal was lent to a command, and the view is not shown again yet.
    lent_back: bool,
    /// Pictures were sent to the terminal, which keeps them: they are given up when it is given back.
    pictures: bool,
    /// The terminal's cursor was made a bar, which is given back to its own shape when it is.
    bar: bool,
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
    let clock = Arc::new(Clock::new());
    let waker = tty.waker();
    // The page is started before the terminal is asked what it is: its interpreter is made on a
    // thread of its own, while the answers come.
    let handles = Handles::new();
    let attached = Arc::new(Attached::default());
    let page = open_page(&bundle, &clock, &waker, &handles, &attached)?;
    trace.mark(|| "the page started".to_owned());
    let mut decoder = Decoder::new();
    let mut early = Vec::new();
    let guess = Guess::from_environment();
    let found = match probe::ask(&tty, &mut decoder, &mut early, &guess) {
        Ok(found) => found,
        Err(signal) => {
            clock.found(false);
            return Ok(closed(shown, &Stop::by(EndedBy::Signal(signal))));
        }
    };
    clock.found(found.graphics);
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
        tty.leaving(&off(true, false, false, false));
    }
    let conversation = Conversation::waking(options, &waker);
    let size = tty.size();
    let mut screen = Screen::new(found.depth);
    screen.compress_pixels(found.compression);
    let mut live = Live {
        tty,
        clock,
        waker,
        shown,
        bundle,
        page,
        conversation,
        decoder,
        screen,
        cell: found.cell,
        found,
        size,
        fields: HashMap::new(),
        focus: None,
        dragging: None,
        pasteboard: None,
        press: None,
        overlay: None,
        press_taken: false,
        handles,
        attached,
        early,
        dirty: true,
        waiting: false,
        waits: 0,
        last_frame: 0.0,
        escape_at: None,
        lent_back: false,
        pictures: false,
        bar: false,
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
fn open_page(
    bundle: &Bundle,
    clock: &Arc<Clock>,
    waker: &Waker,
    handles: &Arc<Handles>,
    attached: &Arc<Attached>,
) -> io::Result<Page> {
    let files: Arc<dyn Assets> = Arc::new(Files {
        bundle: bundle.clone(),
        handles: Arc::clone(handles),
        attached: Arc::clone(attached),
    });
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
    let mut page = Page::script(script, setup, move || wake.wake());
    // The terminal draws the caret, as its own cursor, and blinks it as the person set it to.
    page.set_host_caret(true);
    Ok(page)
}

/// Whether a key that had the editor copy or cut is the person's word for the terminal's pasteboard:
/// ⌘C and ⌘X, from a terminal that reports the Super key. Another key that the page's own `keymap`
/// bound to a copy or a cut is the page's word, and goes to the page's own pasteboard only.
fn by_person(key: Key, mods: Mods) -> bool {
    copy_key(key, mods)
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
            if let Err(stop) = self.dispatch(event, false) {
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
        self.trace.remember();
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
            self.ask_cell();
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
            let mut next = events.into_iter().peekable();
            while let Some(event) = next.next() {
                let held = held(&self.page, &event, next.peek());
                self.dispatch(event, held)?;
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
                self.dispatch(event, false)?;
            }
        }
        Ok(())
    }

    /// Does what the person did. `held`: the next event is the same key of movement again, and the
    /// page is not waited for to answer this one: the last of the run is waited for.
    fn dispatch(&mut self, event: Event, held: bool) -> Flow {
        // What the person does is done in order, and the page's answer to each is had before the
        // next: a key that asks for the keyboard for a field is answered before the keys typed
        // right after it (a burst, a paste with no brackets, a test) are told to the page.
        let began = Instant::now();
        let keyed = matches!(event, Event::Key(_));
        let answered = match event {
            Event::Key(key) => {
                self.key(key)?;
                true
            }
            Event::Mouse(mouse) => {
                let answered = matches!(mouse.kind, MouseKind::Press(_) | MouseKind::Release(_));
                self.mouse(mouse)?;
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
            // The answer to the question of how large a cell is, asked again after the window
            // changed: the page is drawn again to it.
            Event::Reply(Reply::CellSize { width, height }) => {
                if self.cell != Some((width, height)) {
                    self.cell = Some((width, height));
                    self.dirty = true;
                }
                false
            }
            // The answer to a question that was not waited for.
            Event::Reply(_) => false,
        };
        let told = began.elapsed();
        if answered && !held {
            let waiting = Instant::now();
            self.page.module().flush(ANSWER);
            let answered = waiting.elapsed();
            let pumping = Instant::now();
            self.pump()?;
            if keyed {
                let spent = &mut self.trace.spent;
                spent.keys += 1;
                spent.told += told;
                spent.answered += answered;
                spent.pumped += pumping.elapsed();
            }
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
        if self.overlay.is_some() {
            if down {
                self.overlay_key(key, mods, text.as_deref())?;
            }
            return Ok(());
        }
        if !down {
            self.page.key_up(key, mods);
            return Ok(());
        }
        if let Some(id) = self.focused_field() {
            self.field_key(id, key, mods, text.as_deref());
            return Ok(());
        }
        // ⌃⇧C copies what is selected where a terminal tells it from ⌃C, which is the interrupt
        // (one that reports no modifiers of a letter's key sends the same byte for both, and the
        // interrupt wins). Not in the editor's own table, for it is the person's terminal's.
        if mods.control
            && mods.shift
            && !mods.alt
            && !mods.command
            && matches!(key, Key::Character(c) if c.eq_ignore_ascii_case(&'c'))
            && let Some(selected) = self.selected_text()
        {
            return self.copy(selected, true);
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
            Route::Copy(text) | Route::Cut(text) => self.copy(text, by_person(key, mods))?,
            Route::Paste => {
                if let Some(text) = self.pasteboard.clone() {
                    self.page.editors().paste(&text);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// What is selected in the editor that has the keyboard, if anything is.
    fn selected_text(&mut self) -> Option<String> {
        let editors = self.page.editors();
        let (start, len) = editors.selected().filter(|(_, len)| *len > 0)?;
        editors.text(start, len)
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
        if let Some(overlay) = &mut self.overlay {
            if let Overlay::Panel(bar) = overlay {
                bar.paste(text);
                self.dirty = true;
            }
            return;
        }
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

    fn mouse(&mut self, mouse: Mouse) -> Flow {
        let at = Self::point(&mouse);
        self.pointer_at = Some(at);
        if self.overlay.is_some() || self.press_taken {
            return self.overlay_mouse(&mouse);
        }
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
        Ok(())
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

    // --- What is over the page ----------------------------------------------------------------

    fn cell_of(&self, mouse: &Mouse) -> (u16, u16) {
        (mouse.col, mouse.row)
    }

    /// The pointer while something is over the page: it is the overlay's, and the page hears none of
    /// it, the release of the press that picked a line included.
    fn overlay_mouse(&mut self, mouse: &Mouse) -> Flow {
        let (col, row) = self.cell_of(mouse);
        let Some(overlay) = &mut self.overlay else {
            // The overlay went with a press: its release is not the page's.
            if matches!(mouse.kind, MouseKind::Release(_)) {
                self.press_taken = false;
            }
            return Ok(());
        };
        match (&mut *overlay, mouse.kind) {
            (Overlay::Menu(menu), MouseKind::Move | MouseKind::Drag(_)) => {
                if menu.hover(col, row) {
                    self.dirty = true;
                }
            }
            (Overlay::Menu(menu), MouseKind::Press(_)) => {
                self.press_taken = true;
                match menu.press(col, row) {
                    Chose::Nothing => {}
                    Chose::Pick(index) => {
                        let serial = menu.serial();
                        self.overlay = None;
                        self.dirty = true;
                        self.pick(serial, index)?;
                    }
                    Chose::Dismissed => {
                        self.overlay = None;
                        self.dirty = true;
                    }
                }
            }
            (Overlay::Panel(bar), MouseKind::Press(_)) => {
                self.press_taken = true;
                let area = bar.area((self.size.cols, self.size.rows));
                if area.contains(col, row) && row + 1 == area.row + area.rows {
                    bar.press(col, mouse.mods.shift);
                    self.dirty = true;
                }
            }
            (Overlay::Link(_), MouseKind::Press(_)) => {
                // The terminal opens a link by its own click (with the key it asks for) where the
                // mouse is reported; a press anywhere else turns the line away.
                self.press_taken = true;
                self.overlay = None;
                self.dirty = true;
            }
            (_, MouseKind::Release(_)) => self.press_taken = false,
            _ => {}
        }
        Ok(())
    }

    /// A key while something is over the page.
    fn overlay_key(&mut self, key: Key, mods: Mods, text: Option<&str>) -> Flow {
        let handles = Arc::clone(&self.handles);
        let Some(overlay) = &mut self.overlay else {
            return Ok(());
        };
        self.dirty = true;
        match overlay {
            Overlay::Menu(menu) => match menu.key(key, mods).0 {
                Chose::Nothing => {}
                Chose::Pick(index) => {
                    let serial = menu.serial();
                    self.overlay = None;
                    self.pick(serial, index)?;
                }
                Chose::Dismissed => self.overlay = None,
            },
            Overlay::Panel(bar) => match bar.key(key, mods, text, &handles) {
                Answered::Open => {}
                Answered::Turned => {
                    let panel = bar.panel().clone();
                    self.overlay = None;
                    self.panel_answered(&panel, Answered::Turned)?;
                }
                answer @ (Answered::Gave(_) | Answered::Chose(_)) => {
                    let panel = bar.panel().clone();
                    self.overlay = None;
                    self.panel_answered(&panel, answer)?;
                }
            },
            // Any key turns the line away.
            Overlay::Link(_) => self.overlay = None,
        }
        Ok(())
    }

    /// The person picked the `index`th line of the menu the page asked for as number `serial`.
    fn pick(&mut self, serial: u64, index: usize) -> Flow {
        let mut effects = Effects::default();
        self.page.pick(serial, index, &mut effects);
        self.apply(effects)?;
        self.pump()
    }

    /// What the person did with the panel: the page is told, a panel turned away included.
    fn panel_answered(&mut self, panel: &Panel, answer: Answered) -> Flow {
        match answer {
            Answered::Gave(entries) => {
                self.page.give(Point::default(), FileSource::Pick, entries);
            }
            Answered::Chose(saving) => self.page.saving(Some(saving)),
            Answered::Turned | Answered::Open => match panel {
                Panel::Pick { .. } => {
                    self.page
                        .give(Point::default(), FileSource::Pick, Vec::new())
                }
                Panel::Save { .. } => self.page.saving(None),
            },
        }
        self.pump()
    }

    /// Shows what the page asked to show, where nothing is shown over it already: a menu and a panel
    /// asked for while another is up are passed over (the page is not told), a link replaces a link.
    fn show(&mut self, effects: &mut Effects) {
        let size = (self.size.cols, self.size.rows);
        if let Some(url) = effects.link.take()
            && overlay::openable(&url)
            && matches!(self.overlay, None | Some(Overlay::Link(_)))
        {
            self.overlay = Some(Overlay::Link(LinkBar::new(url)));
            self.dirty = true;
        }
        if self.overlay.is_some() {
            return;
        }
        if let Some(menu) = effects.menu.take() {
            let at = (
                (menu.at.x / COLUMN).floor().max(0.0) as u16,
                (menu.at.y / ROW).floor().max(0.0) as u16,
            );
            // A menu that was only a paste has nothing to show: it is as if it had been turned away.
            let shown = MenuBox::new(&menu, at, size);
            if !shown.is_empty() {
                self.overlay = Some(Overlay::Menu(shown));
                self.dirty = true;
            }
        } else if let Some(panel) = effects.panel.take() {
            self.overlay = Some(Overlay::Panel(PanelBar::new(panel)));
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

    fn apply(&mut self, mut effects: Effects) -> Flow {
        self.show(&mut effects);
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

    /// What is written when the terminal is given back, as the screen stands.
    fn leaving(&self) {
        self.tty.leaving(&off(
            self.keys,
            self.pointer.is_some(),
            self.pictures,
            self.bar,
        ));
    }

    /// The pointer takes the shape asked for, where it is not the one it has: written only
    /// when it changes, and given back with the terminal (the first time one is chosen). The arrow
    /// is what the terminal has until a shape was chosen.
    fn take_pointer(&mut self, cursor: Cursor) -> Flow {
        if self.pointer.unwrap_or_default() == cursor {
            return Ok(());
        }
        self.pointer = Some(cursor);
        self.leaving();
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
                Said::Message(text) => {
                    self.trace.heard();
                    self.page.message(&text);
                }
                Said::Closed => return Err(Stop::by(EndedBy::Program)),
                Said::Reopened(views) => {
                    self.lent_back = false;
                    self.reopen(views.as_deref())?;
                }
                // The files are held before what names them is heard: the program waits for this.
                Said::Attached(attached) => {
                    if self.attached.hold(attached.files()) {
                        attached.held();
                    } else {
                        attached.refused("limit");
                    }
                }
                Said::Detached(names) => {
                    let gone = self.attached.forget(&names);
                    if !gone.is_empty()
                        && let Some(images) = self.page.images()
                    {
                        images.forget(&gone);
                    }
                }
                Said::HandedOff(lent) => {
                    self.lend(lent)?;
                    // The view is shown again when the command is done, as the program's session
                    // shows it: a reopening that came meanwhile says it with the kinds of view it
                    // brought, and is the one to do it.
                    self.lent_back = true;
                }
                _ => {}
            }
        }
        if std::mem::take(&mut self.lent_back) {
            self.reopen(None)?;
        }
        Ok(any)
    }

    /// The program lent the terminal to a command: the modes are given back and the command runs
    /// as the terminal's foreground job, which this waits for; then the screen is taken again, and
    /// all of it is drawn anew (the terminal had it cleared, and what the command wrote is not
    /// ours).
    fn lend(&mut self, lent: Lent) -> Flow {
        self.tty
            .lend(lent, ON)
            .map_err(|error| Stop::io(&self.tty, &error))?;
        if self.keys {
            self.tty
                .write_all(b"\x1b[>1u")
                .map_err(|error| Stop::io(&self.tty, &error))?;
        }
        self.pointer = None;
        self.pictures = false;
        self.bar = false;
        self.leaving();
        self.decoder = Decoder::new();
        self.escape_at = None;
        self.press = None;
        self.size = self.tty.size();
        self.ask_cell();
        self.screen.forget();
        self.dirty = true;
        Ok(())
    }

    /// The terminal is asked again how many pixels a cell is, where it said before: a window of
    /// another size has cells of another, and what it said is not true until it says so again (the
    /// pixels of the window, over the cells it has, are what is drawn by meanwhile).
    fn ask_cell(&mut self) {
        if self.found.cell.is_some() {
            self.cell = None;
            let _ = self.tty.write_all(b"\x1b[16t");
        }
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
        let page = open_page(
            &self.bundle,
            &self.clock,
            &self.waker,
            &self.handles,
            &self.attached,
        )
        .map_err(|error| Stop {
            by: EndedBy::Error,
            code: Some("view".to_owned()),
            message: Some(error.to_string()),
        })?;
        self.page = page;
        self.fields.clear();
        self.focus = None;
        self.dragging = None;
        self.overlay = None;
        self.press_taken = false;
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
        let drawing = Instant::now();
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
        let mut cursor = self.draw_fields(&mut frame.grid, &theme);
        if cursor.is_none() && self.overlay.is_none() {
            cursor = self.editor_cursor();
        }
        if let Some(overlay) = &self.overlay {
            cursor = None;
            let area = match overlay {
                Overlay::Menu(menu) => {
                    menu.draw(&mut frame.grid, &theme);
                    Some(menu.area())
                }
                Overlay::Panel(bar) => {
                    cursor = bar.draw(&mut frame.grid, &theme);
                    Some(bar.area((cols, rows)))
                }
                Overlay::Link(bar) => {
                    frame.links.extend(bar.draw(&mut frame.grid, &theme));
                    Some(bar.area((cols, rows)))
                }
            };
            // A picture that is drawn over the grounds would cover what is over the page.
            if let Some(area) = area {
                area.hides(&mut frame.placements);
            }
        }
        let bytes = self.screen.draw(&frame, cursor);
        if self.screen.holds_pictures() && !self.pictures {
            self.pictures = true;
            self.leaving();
        }
        if self.screen.bars_cursor() && !self.bar {
            self.bar = true;
            self.leaving();
        }
        if self.trace.frames() < TRACED_FRAMES {
            let sent = bytes.len();
            self.trace.mark(|| format!("frame, {sent} bytes written"));
        }
        if !bytes.is_empty() {
            self.tty
                .write_all(&bytes)
                .map_err(|error| Stop::io(&self.tty, &error))?;
        }
        let spent = &mut self.trace.spent;
        spent.frames += 1;
        spent.drawn += drawing.elapsed();
        spent.written += bytes.len() as u64;
        spent.pictures += frame.placements.len() as u64;
        spent.pixels += frame
            .placements
            .iter()
            .map(|p| 4 * u64::from(p.width) * u64::from(p.height))
            .sum::<u64>();
        Ok(())
    }

    /// The cell the terminal's cursor goes to for the editor that has the keyboard: the cell its
    /// caret is in, where that is on the screen. The terminal draws it, and blinks it, as the person
    /// set it to; the page's own caret is not drawn.
    fn editor_cursor(&mut self) -> Option<(u16, u16)> {
        let mut editors = self.page.editors();
        editors.focused()?;
        let at = editors.caret()?;
        let rect = editors.rect(at)?;
        // The caret's box starts at its place between two characters: the nearer cell edge. A tall
        // line's caret is in the row its middle is in.
        let col = (rect.x / COLUMN + 0.5).floor();
        let row = ((rect.y + rect.h / 2.0) / ROW).floor();
        let on = |at: f32, of: u16| at >= 0.0 && at < f32::from(of);
        (on(col, self.size.cols) && on(row, self.size.rows)).then_some((col as u16, row as u16))
    }

    /// The pixels of a cell of the terminal, where it says (a guess where it does not).
    fn cell_pixels(&self) -> (u32, u32) {
        if let Some((width, height)) = self.cell {
            return (width.max(1), height.max(1));
        }
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
        let summary = trace.summary();
        for (at, words) in trace.marks.into_iter().flatten() {
            eprintln!("pencere: trace +{at:.3} s {words}");
        }
        if let Some(summary) = summary {
            eprintln!("pencere: trace {summary}");
        }
        closed(shown, stop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screen_says_it_draws_in_cells_and_whether_its_pictures_are_coarse() {
        let with = Clock::new();
        with.found(true);
        assert_eq!(with.draws(), [Drawing::Cells]);
        let without = Clock::new();
        without.found(false);
        assert_eq!(without.draws(), [Drawing::Cells, Drawing::CoarseImages]);
    }

    #[test]
    fn a_page_that_asks_before_the_probe_is_over_waits_for_what_it_found() {
        let clock = Arc::new(Clock::new());
        let page = Arc::clone(&clock);
        let asking = std::thread::spawn(move || page.draws());
        // The probe answers a moment after the page asked.
        std::thread::sleep(Duration::from_millis(50));
        clock.found(true);
        assert_eq!(asking.join().ok(), Some(vec![Drawing::Cells]));
    }

    fn file(name: &str, bytes: Arc<[u8]>) -> (String, Arc<[u8]>) {
        (name.to_owned(), bytes)
    }

    #[test]
    fn files_the_program_attaches_are_held_to_the_limits_of_a_bundle_and_let_go_by_name() {
        let held = Attached::default();
        let small: Arc<[u8]> = Arc::from(vec![7_u8; 10]);
        assert!(held.hold(&[file("sha256:a", Arc::clone(&small))]));
        assert_eq!(held.get("sha256:a").as_deref(), Some(&small[..]));
        // The same file again is the one held, and counts once.
        assert!(held.hold(&[file("sha256:a", Arc::clone(&small))]));
        // A file larger than a file may be is not held, nor anything that came with it.
        let most = usize::try_from(pencere::MAX_ATTACHED_FILE).expect("a size");
        let largest: Arc<[u8]> = Arc::from(vec![0_u8; most]);
        let too_large: Arc<[u8]> = Arc::from(vec![0_u8; most + 1]);
        assert!(!held.hold(&[
            file("sha256:b", Arc::clone(&small)),
            file("sha256:c", too_large)
        ]));
        assert!(held.get("sha256:b").is_none() && held.get("sha256:c").is_none());
        // The bytes of all of them are held to a limit too: seven of the largest are held, an eighth is
        // not (the little one counts), and what is let go makes room.
        let names: Vec<String> = (0..7).map(|at| format!("sha256:big{at}")).collect();
        for name in &names {
            assert!(held.hold(&[file(name, Arc::clone(&largest))]), "{name}");
        }
        assert!(!held.hold(&[file("sha256:big7", Arc::clone(&largest))]));
        assert_eq!(held.forget(&names[..2]), names[..2].to_vec());
        assert!(held.hold(&[file("sha256:big7", Arc::clone(&largest))]));
        // A name that was not held is not said to be let go.
        assert_eq!(
            held.forget(&["sha256:none".to_owned()]),
            Vec::<String>::new()
        );
        // And there are not more of them than a bundle has files.
        let many = Attached::default();
        let one: Arc<[u8]> = Arc::from(vec![1_u8]);
        for at in 0..pencere::MAX_ATTACHED_FILES {
            assert!(many.hold(&[file(&format!("sha256:{at}"), Arc::clone(&one))]));
        }
        assert!(!many.hold(&[file("sha256:more", one)]));
    }
}
