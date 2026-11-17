//! The program's view, shown on the terminal it was started in.
//!
//! [`run`] is what a program's options hand a terminal to when no terminal hosts the program's view
//! ([`pencere::Options::screen`]): it takes the view the program carries (the first of the kind this
//! crate draws), runs its page in this process, speaks to the program's handlers as a terminal
//! would carry the six messages (`pencere::Conversation`), and draws the page's frames as pictures
//! in the terminal, writing only what changed.
//!
//! One thread does it all and waits where there is nothing to do: for the terminal's bytes, for
//! the page (its module and its images wake the wait), for the handlers (they wake it too), for a
//! signal, and for the time the page asked to be woken at.

use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use pencere::{
    Bundle, Closed, Conversation, EndedBy, Kind, Lent, Options, Said, Shown, Size, Tty, View, Waker,
};
use pencere_drive::shot::Fonts;
use pencere_drive::{CarryOut, FrameAt, Host, Page, Route, Setup};
use pencere_sandbox::{Assets, Fault, Program, Script};
use pencere_view::{
    Button, Cursor, Drawing, FieldKind, FileSource, Fill, Id, Input, Key, KeyConvention, Keyboard,
    Menu, Mods, Panel, Phase, Point, Rect, copy_key,
};

use crate::chord;
use crate::chrome::{self, Paper};
use crate::field::TextField;
use crate::files::{Disk, Handles};
use crate::input::{Decoder, Event, KeyEvent, Mouse, MouseKind, Reply};
use crate::link;
use crate::memory;
use crate::output::base64;
use crate::overlay::{self, Answered, Chose, LinkBar, MenuBox, NoticeBar, PanelBar};
use crate::pixel::{self, Pixel};
use crate::probe::{self, Found, Guess};

/// The version of the kit's interface (the `lua` view's) this screen runs.
pub const KIT_VERSION: &str = "0.1";

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
/// How long the screen's words over the foot of the page stay (seconds).
const NOTICE_FOR: f64 = 2.0;

/// How long a second press is the same click's.
const DOUBLE: Duration = Duration::from_millis(400);

/// The modes the screen is put in: no wrap at the edge, no cursor until a field has one, the mouse
/// (buttons, drags and motion, in the long form), pastes in brackets, and the focus.
const ON: &[u8] = b"\x1b[?7l\x1b[?25l\x1b[?1002h\x1b[?1003h\x1b[?1006h\x1b[?2004h\x1b[?1004h";
/// What puts them back, and gives the terminal its own ground back (`OSC 111`: the terminal's
/// default is what it was before the page's was set, wherever the screen was left).
const OFF: &[u8] = b"\x1b[?1016l\x1b[?1004l\x1b[?2004l\x1b[?1006l\x1b[?1003l\x1b[?1002l\x1b[?25h\x1b[?7h\x1b[0m\x1b]111\x1b\\";

/// The modes the screen is put in ([`ON`]), and the terminal's ground set to the page's (`OSC 11`):
/// what the window shows past the picture, left and above it and where the terminal's window is not
/// whole cells, is that colour and not the terminal's own. [`OFF`] gives it back, and is what is
/// written whenever the terminal is given back (at the end, on a signal, in a panic, and before the
/// terminal is lent to a command, which this is written after).
fn on(ground: [u8; 3]) -> Vec<u8> {
    let mut bytes = ON.to_vec();
    bytes.extend_from_slice(
        format!(
            "\x1b]11;rgb:{:02x}/{:02x}/{:02x}\x1b\\",
            ground[0], ground[1], ground[2]
        )
        .as_bytes(),
    );
    bytes
}
/// The pointer told in the pixels of the window and not its cells, where the terminal says it does
/// (switched on once it has).
const PIXEL_MOUSE: &[u8] = b"\x1b[?1016h";
/// The Kitty keyboard protocol's flags asked for: the keys are told apart (`1`, so that ⌃⇧C is not ⌃C)
/// and with the alternate keys (`4`: the key a shifted sign makes, so that ⌃⇧7 on a Turkish keyboard is
/// the slash it types).
const KEYS_ON: &[u8] = b"\x1b[>5u";
/// The Kitty keyboard protocol's flags taken back, which comes before the rest where it was asked for.
const KEYS_OFF: &[u8] = b"\x1b[<u";
/// The pointer's shape given back to the terminal's own (an empty `OSC 22`), before the screen is
/// left, where the page asked for one.
const POINTER_OFF: &[u8] = b"\x1b]22;\x1b\\";

/// What is written, before the screen is left, whenever the terminal is given back: the modes put
/// back, the keyboard's flags taken back where they were asked for, the pointer's shape where the
/// page chose one, and the pictures the terminal keeps given up where it was sent some.
fn off(keys: bool, pointer: bool, pictures: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    if pictures {
        bytes.extend_from_slice(crate::output::FREE_ALL.as_bytes());
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
                .given_path(handle)
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

/// What the page asks of its program's system: the time on the clock the frames run on, what the
/// screen draws, and the keyboard of a terminal that is not a Mac's. The program has the room for
/// whatever the page says (the handlers are read as fast as they write).
struct Clock {
    started: Instant,
}

impl Clock {
    fn new() -> Self {
        Self {
            started: Instant::now(),
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

    /// The terminal is sent the page as pixels, shadows and all.
    fn draws(&self) -> Vec<Drawing> {
        vec![Drawing::Shadow]
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

/// The events of one read of the terminal's bytes with each run of pointer movements cut to its last:
/// `Move`s one after another, or `Drag`s of the same button and modifiers one after another. A person's
/// hand makes hundreds of them in a second, and on a machine that is busy the page is behind them: each
/// one told to it is a caret placed and a frame asked for, and the page goes on after the person has
/// let go. Where the pointer is is the last of them; what the ones before it passed over is nothing
/// the page needs. A press, a release, a key, the wheel and a paste end a run and are kept, in their
/// places.
fn latest_pointer(events: Vec<Event>) -> Vec<Event> {
    let mut kept: Vec<Event> = Vec::with_capacity(events.len());
    for event in events {
        if let (Event::Mouse(now), Some(Event::Mouse(before))) = (&event, kept.last())
            && matches!(now.kind, MouseKind::Move | MouseKind::Drag(_))
            && now.kind == before.kind
            && now.mods == before.mods
        {
            kept.pop();
        }
        kept.push(event);
    }
    kept
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
    /// The files the person gave the page and the places they chose to save in.
    handles: Arc<Handles>,
    /// What the program reads of the first and writes to the second, on a thread of its own.
    disk: Disk,
    /// The program is in an ssh session: its person is on another machine, and a link is copied for
    /// them rather than opened here.
    remote: bool,
    /// The links an opener failed to open, which are copied instead.
    failed: Arc<link::Failed>,
    /// Words the screen says of what it did, over the foot of the page, until the time (on the
    /// clock of the frames).
    notice: Option<(String, f64)>,
    /// How far down the strip the page has along the foot of the window starts, pixels, as of the last
    /// frame ([`chrome::foot_of`]): what a bar there covers, so a press on it is a press on the bar.
    foot: f32,
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
    /// What the screen is put in whenever it is taken again (after a command it was lent to): the
    /// modes and the page's ground.
    on: Vec<u8>,
    /// The shape the pointer was last given by `OSC 22`; none while the terminal's own is as it
    /// was.
    pointer: Option<Cursor>,
    /// The terminal was lent to a command, and the view is not shown again yet.
    lent_back: bool,
    /// Pictures were sent to the terminal, which keeps them: they are given up when it is given back.
    pictures: bool,
    /// The shape the page last asked for, and where the pointer last was (in the page's points).
    told: Cursor,
    pointer_at: Option<Point>,
    /// The window as pixels: the page is laid out in them, and what it draws is sent as a picture.
    pixel: Pixel,
    /// The terminal tells the pointer in pixels (the SGR pixel mode), not in cells.
    pixel_mouse: bool,
    trace: Trace,
}

/// Shows the view the options carry on `tty`, until it ends.
///
/// The view is drawn as pictures: the terminal is sent the page as pixels (Kitty's graphics
/// protocol), the faces of its own machine drawing the text.
///
/// # Errors
///
/// When there is no view this screen draws, or the page cannot be started. An error of the kind
/// [`io::ErrorKind::Unsupported`] is a terminal that cannot show the pictures: one that says it
/// draws none, or does not say how large its cells are, or a multiplexer. Nothing has been written
/// to it by then that [`Tty`] does not give back.
pub fn run(options: Options, tty: Tty<'_>) -> io::Result<Closed> {
    let mut trace = Trace::new();
    let (shown, bundle) = choose(options.views())?;
    let guess = Guess::from_environment();
    if guess.multiplexer {
        // Neither tmux nor screen draws the pictures of the terminal behind it.
        return Err(not_shown(
            "the terminal is a multiplexer, which draws no pictures",
        ));
    }
    let clock = Arc::new(Clock::new());
    let waker = tty.waker();
    // The faces of the machine are looked for while the terminal answers: they measure the page
    // and draw it.
    let looking = std::thread::spawn(Fonts::installed);
    // The page is started before the terminal is asked what it is: its interpreter is made on a
    // thread of its own, while the answers come.
    let handles = Handles::new();
    let attached = Arc::new(Attached::default());
    let mut page = open_page(&bundle, &clock, &waker, &handles, &attached)?;
    trace.mark(|| "the page started".to_owned());
    let mut decoder = Decoder::new();
    let mut early = Vec::new();
    let found = match probe::ask(&tty, &mut decoder, &mut early, &guess) {
        Ok(found) => found,
        Err(signal) => {
            return Ok(closed(shown, &Stop::by(EndedBy::Signal(signal))));
        }
    };
    trace.mark(|| {
        let how = if found.fenced {
            "answered to the end"
        } else {
            "did not answer to the end"
        };
        format!(
            "the terminal {how} (Kitty graphics {}, edits {}, Kitty keyboard {})",
            found.graphics, found.frames, found.keyboard
        )
    });
    let size = tty.size();
    let cell = found.cell_in(&size);
    let Some(cell) = cell.filter(|_| found.graphics && !found.multiplexer) else {
        return Err(not_shown(if found.graphics {
            "the terminal does not say how many pixels its cells are"
        } else {
            "the terminal draws no pictures"
        }));
    };
    // The terminal draws the pictures: only now does it leave its screen for ours. One that does
    // not is given nothing but the questions, and the program takes its fallback on the screen the
    // person has.
    let on = on(pixel::ground_of(pixel::palette_of(&found)));
    tty.enter(&on, OFF)?;
    trace.mark(|| "the alternate screen entered".to_owned());
    let fonts = looking.join().unwrap_or_else(|_| Fonts::carried());
    trace.mark(|| "the faces of the machine were found".to_owned());
    let pixel = Pixel::new(fonts, &found, cell, (size.cols, size.rows));
    dress(&mut page, &pixel);
    let pixel_mouse = found.pixel_mouse && tty.write_all(PIXEL_MOUSE).is_ok();
    let keys = found.keyboard && tty.write_all(KEYS_ON).is_ok();
    if keys {
        tty.leaving(&off(true, false, false));
    }
    let conversation = Conversation::waking(options, &waker);
    let mut live = Live {
        tty,
        clock,
        waker,
        shown,
        bundle,
        page,
        conversation,
        decoder,
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
        disk: Disk::new(Arc::clone(&handles)),
        remote: link::remote(|name| std::env::var(name).ok()),
        failed: Arc::new(link::Failed::default()),
        notice: None,
        foot: f32::INFINITY,
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
        notes: Vec::new(),
        keys,
        on,
        pointer: None,
        told: Cursor::Arrow,
        pointer_at: None,
        pixel,
        pixel_mouse,
        trace,
    };
    let stop = live.lead();
    Ok(live.end(&stop))
}

/// Why this screen is not shown: the terminal cannot show it, which is for the program to take its
/// fallback for and no fault to tell of.
fn not_shown(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, why.to_owned())
}

/// The page laid out and drawn as pixels: measured in the faces that draw it, and with no cursor
/// of the terminal's for its caret (the caret is a part of the picture).
fn dress(page: &mut Page, pixel: &Pixel) {
    page.set_measure(pixel.measure());
    page.set_host_caret(false);
}

/// The view this screen draws among the program's: the first of the kind, in the version it runs.
fn choose(views: &[View]) -> io::Result<(Shown, Bundle)> {
    let view = views
        .iter()
        .find(|view| view.kind() == Kind::Lua && view.versions().iter().any(|v| v == KIT_VERSION))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
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
            self.refit();
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
        self.opened()?;
        self.untell();
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
        if let Some((_, until)) = &self.notice {
            soon(*until);
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
            let mut next = latest_pointer(events).into_iter().peekable();
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
                let cells = (self.size.cols, self.size.rows);
                if self.pixel.fit((width, height), cells) {
                    self.page.set_measure(self.pixel.measure());
                }
                self.dirty = true;
                false
            }
            // The answer to an edit of the picture that is the window: a refusal is not made again.
            Event::Reply(Reply::Graphics { id, ok }) => {
                if self.pixel.answered(id, ok) {
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
        // The keys the page writes for ⌘ are ⌃⇧ and the character here (`chord`). The page is told
        // the key as ⌘'s, and what is held after it as it is: the terminal says no more of the
        // modifiers (no key is let go), and a ⌘ the page kept from the key would be the one a
        // wheel turn is a zoom with.
        let held = mods;
        let mods = self.chord_of(key, mods);
        let flow = self.key_to_page(key, mods, text, down);
        if mods != held {
            self.page.input(Input::Mods(held));
        }
        flow
    }

    /// A key (as the page is to have it) that is no overlay's: the page's, or the field's that has
    /// the keyboard.
    fn key_to_page(&mut self, key: Key, mods: Mods, text: Option<String>, down: bool) -> Flow {
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

    /// What the key is to the page: where the terminal tells ⌃⇧ from ⌃ (the Kitty keyboard), ⌃⇧ and
    /// a character is the page's ⌘ and that character, so that ⌃⇧C copies, ⌃⇧X cuts and ⌃⇧/ comments
    /// where the page wrote ⌘C, ⌘X and ⌘/; the page's own binding of the chord wins.
    fn chord_of(&self, key: Key, mods: Mods) -> Mods {
        if !self.keys {
            return mods;
        }
        chord::pressed(key, mods, |key, mods| self.page.binds(key, mods))
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

    /// How many points of the page a cell of the terminal is, across and down.
    fn cell_points(&self) -> (f32, f32) {
        let pixel = &self.pixel;
        #[allow(clippy::cast_precision_loss)]
        let (width, height) = (pixel.cell().0 as f32, pixel.cell().1 as f32);
        (width / pixel.scale(), height / pixel.scale())
    }

    /// Where the page is pointed at: the middle of the cell the terminal says, or the pixel (in
    /// pixels, a terminal that was asked to say so, over the pixels to a point).
    fn point(&self, mouse: &Mouse) -> Point {
        if self.pixel_mouse {
            let scale = self.pixel.scale();
            return Point::new(
                (f32::from(mouse.col) + 0.5) / scale,
                (f32::from(mouse.row) + 0.5) / scale,
            );
        }
        let (column, row) = self.cell_points();
        Point::new(
            (f32::from(mouse.col) + 0.5) * column,
            (f32::from(mouse.row) + 0.5) * row,
        )
    }

    fn mouse(&mut self, mouse: Mouse) -> Flow {
        let at = self.point(&mouse);
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
                let (column, row) = self.cell_points();
                self.page.wheel(
                    at,
                    f32::from(dx) * WHEEL_ROWS * column,
                    f32::from(dy) * WHEEL_ROWS * row,
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
        let at = self.point(mouse);
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
        let cell = self.cell_of(mouse);
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

    /// The caret of the field `id` goes to the edge of a character the pointer is nearest to.
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
        // The room is as many whole cells as fit in it, from its left edge, and the nearer edge of a
        // cell is the one the caret goes to.
        let scale = self.pixel.scale();
        #[allow(clippy::cast_precision_loss)]
        let across = self.pixel.cell().0.max(1) as f32;
        let x = self.point(mouse).x * scale;
        let from = room.x * scale;
        let (cell, width) = (
            ((x - from) / across + 0.5).floor().max(0.0) as usize,
            (room.w * scale / across).floor().max(1.0) as u16,
        );
        if let Some(field) = self.fields.get_mut(&id) {
            field.place(cell, width, extend);
            self.dirty = true;
        }
    }

    // --- What is over the page ----------------------------------------------------------------

    /// The cell the pointer is in: what the terminal says, or the cell the pixel it says is in.
    fn cell_of(&self, mouse: &Mouse) -> (u16, u16) {
        if self.pixel_mouse {
            let (width, height) = self.pixel.cell();
            (
                mouse.col / u16::try_from(width).unwrap_or(u16::MAX).max(1),
                mouse.row / u16::try_from(height).unwrap_or(u16::MAX).max(1),
            )
        } else {
            (mouse.col, mouse.row)
        }
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
            (Overlay::Link(bar), MouseKind::Press(button)) => {
                // The bar is a picture, so the terminal has no link to open in it: a press of the
                // main button on its row is the person's yes, any other turns it away.
                self.press_taken = true;
                let area = bar.area((self.size.cols, self.size.rows));
                let covered = area.contains(col, row)
                    || (self.pixel_mouse && f32::from(mouse.row) >= self.foot);
                let url = (button == Button::Left && covered).then(|| bar.url().to_owned());
                self.overlay = None;
                self.dirty = true;
                if let Some(url) = url {
                    self.follow(url)?;
                }
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
            // Enter is the person's yes; any other key turns the line away.
            Overlay::Link(bar) => {
                let url = matches!(key, Key::Enter).then(|| bar.url().to_owned());
                self.overlay = None;
                if let Some(url) = url {
                    self.follow(url)?;
                }
            }
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

    /// The person said yes to a link: where they are it is opened, by the system's opener, and where
    /// they are not (an ssh session), or where there is no opener or it fails, the address is put on
    /// the terminal's pasteboard and they are told so. The page is told nothing (§9.7).
    fn follow(&mut self, url: String) -> Flow {
        if !self.remote && link::can_open(|name| std::env::var(name).ok()) {
            let waker = self.waker.clone();
            if link::open(link::opener(), &url, &self.failed, move || waker.wake()).is_ok() {
                return Ok(());
            }
        }
        self.copy_link(url)
    }

    /// The address of a link, for the pasteboard the person's own is: they pressed it.
    fn copy_link(&mut self, url: String) -> Flow {
        self.copy(url, true)?;
        self.tell("Link copied");
        Ok(())
    }

    /// The links whose opener failed since: copied, as they are where there is none.
    fn opened(&mut self) -> Flow {
        for url in self.failed.take() {
            self.copy_link(url)?;
        }
        Ok(())
    }

    /// Says `words` over the foot of the page for a while.
    fn tell(&mut self, words: &str) {
        self.notice = Some((words.to_owned(), self.now() + NOTICE_FOR));
        self.dirty = true;
    }

    /// Takes away what was said once its time has come.
    fn untell(&mut self) {
        if self
            .notice
            .as_ref()
            .is_some_and(|(_, until)| *until <= self.now())
        {
            self.notice = None;
            self.dirty = true;
        }
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
            let copies = self.remote || !link::can_open(|name| std::env::var(name).ok());
            self.overlay = Some(Overlay::Link(LinkBar::new(url, copies)));
            self.dirty = true;
        }
        if self.overlay.is_some() {
            return;
        }
        if let Some(mut menu) = effects.menu.take() {
            // A menu's lines show the keys a person presses: ⌃⇧ and a character for the page's ⌘.
            if self.keys {
                chord::menu(&mut menu, |key, mods| self.page.binds(key, mods));
            }
            let (column, row) = self.cell_points();
            let at = (
                (menu.at.x / column).floor().max(0.0) as u16,
                (menu.at.y / row).floor().max(0.0) as u16,
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
        self.tty
            .leaving(&off(self.keys, self.pointer.is_some(), self.pictures));
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
                // The person's files are read and written where they chose, by their handles.
                Said::Read(reading) => self.disk.read(reading),
                Said::Write(writing) => self.disk.write(writing),
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
            .lend(lent, &self.on)
            .map_err(|error| Stop::io(&self.tty, &error))?;
        if self.keys {
            self.tty
                .write_all(KEYS_ON)
                .map_err(|error| Stop::io(&self.tty, &error))?;
        }
        if self.pixel_mouse {
            self.tty
                .write_all(PIXEL_MOUSE)
                .map_err(|error| Stop::io(&self.tty, &error))?;
        }
        self.pointer = None;
        self.pictures = false;
        self.leaving();
        self.decoder = Decoder::new();
        self.escape_at = None;
        self.press = None;
        self.size = self.tty.size();
        self.ask_cell();
        // What the terminal held of the window was cleared with the screen.
        self.pixel.forget();
        self.refit();
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

    /// The window has cells of another number: the picture is made for them. Its cells are as many
    /// pixels as they were until the terminal says otherwise (where it does), or as the window's
    /// pixels over them.
    fn refit(&mut self) {
        let size = self.size;
        let pixel = &mut self.pixel;
        let cell = if self.found.cell.is_none()
            && size.width > 0
            && size.height > 0
            && size.cols > 0
            && size.rows > 0
        {
            (
                (u32::from(size.width) / u32::from(size.cols)).max(1),
                (u32::from(size.height) / u32::from(size.rows)).max(1),
            )
        } else {
            pixel.cell()
        };
        if pixel.fit(cell, (size.cols, size.rows)) {
            self.page.set_measure(pixel.measure());
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
        dress(&mut self.page, &self.pixel);
        // The places the view was saving to are nothing once it is shown again (§10.7).
        self.disk.abandon();
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

    /// The host's fields are the ones the page has: one it no longer has is let go of, and so is
    /// the keyboard it had.
    fn keep_fields(&mut self) {
        let present: Vec<Id> = self.page.fields().iter().map(|field| field.id).collect();
        self.fields.retain(|id, _| present.contains(id));
        if self.focus.is_some_and(|id| !present.contains(&id)) {
            self.focus = None;
        }
    }

    /// The page, laid out in the pixels of the window and painted into them, and what changed of the
    /// picture the terminal holds written to it.
    fn frame(&mut self) -> Flow {
        let now = self.now();
        let drawing = Instant::now();
        self.last_frame = now;
        self.dirty = false;
        let (size, scale) = (self.pixel.size(), f64::from(self.pixel.scale()));
        let mut painted = self.page.frame(FrameAt {
            size,
            scale,
            now,
            // A terminal is sent what moves as pictures, and the page's glides cost a frame of
            // bytes each.
            reduce_motion: true,
        });
        let mut effects = Effects::default();
        painted.deliver_held(&mut effects);
        self.apply(effects)?;
        self.keep_fields();
        let pixel = &mut self.pixel;
        // What the host draws over the page: its fields, and a menu, a panel or a link over those.
        #[allow(clippy::cast_precision_loss)]
        let cell = (pixel.cell().0 as f32, pixel.cell().1 as f32);
        let window = Rect::new(0.0, 0.0, size.0 as f32, size.1 as f32);
        let paper = Paper {
            fonts: pixel.fonts(),
            scale: pixel.scale(),
            cell,
            window,
            foot: chrome::foot_of(&painted.list.items, window, cell.1),
        };
        self.foot = paper.foot;
        let mut over = chrome::fields(&paper, self.page.fields(), &self.fields, self.focus);
        let screen = (self.size.cols, self.size.rows);
        match &self.overlay {
            Some(Overlay::Menu(menu)) => over.extend(menu.paint(&paper)),
            Some(Overlay::Panel(bar)) => over.extend(bar.paint(&paper, screen)),
            Some(Overlay::Link(bar)) => over.extend(bar.paint(&paper, screen)),
            None => {
                if let Some((words, _)) = &self.notice {
                    over.extend(NoticeBar::new(words).paint(&paper, screen));
                }
            }
        }
        painted.list.items.extend(over);
        let written = pixel.draw(painted.list, self.page.images());
        self.waits = if written.waiting { self.waits + 1 } else { 0 };
        self.waiting = written.waiting && self.waits < PATIENCE_FRAMES;
        let bytes = written.bytes;
        if !bytes.is_empty() && !self.pictures {
            self.pictures = true;
            self.leaving();
        }
        if self.trace.frames() < TRACED_FRAMES {
            let sent = bytes.len();
            let how = if written.whole {
                "the window".to_owned()
            } else {
                format!("{} bands", written.bands.len())
            };
            self.trace
                .mark(|| format!("frame, {sent} bytes written ({how})"));
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
        spent.pictures += if written.whole {
            1
        } else {
            written.bands.len() as u64
        };
        spent.pixels += if written.whole {
            4 * u64::from(size.0) * u64::from(size.1)
        } else {
            written
                .bands
                .iter()
                .map(|(_, rows)| 4 * u64::from(size.0) * u64::from(*rows))
                .sum::<u64>()
        };
        Ok(())
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
    fn a_screen_says_it_draws_pictures() {
        assert_eq!(Clock::new().draws(), [Drawing::Shadow]);
    }

    #[test]
    fn whatever_puts_the_terminal_back_gives_its_ground_back_too() {
        let ground = b"\x1b]111\x1b\\";
        let has = |bytes: &[u8]| bytes.windows(ground.len()).any(|w| w == ground);
        assert!(has(OFF));
        for bits in 0..8_u8 {
            let bytes = off(bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
            assert!(has(&bytes), "off({bits:#06b}) gives the ground back");
        }
    }

    #[test]
    fn the_terminals_ground_is_set_to_the_pages_in_the_palette_the_page_is_drawn_in() {
        use pencere_drive::shot::Palette;
        let dark = on(pixel::ground_of(Palette::Dark));
        assert!(dark.starts_with(ON));
        assert!(dark.ends_with(b"\x1b]11;rgb:0d/11/17\x1b\\"));
        let light = on(pixel::ground_of(Palette::Light));
        assert!(light.ends_with(b"\x1b]11;rgb:ff/ff/ff\x1b\\"));
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

    fn mouse(kind: MouseKind, col: u16, row: u16) -> Event {
        Event::Mouse(Mouse {
            kind,
            col,
            row,
            mods: Mods::default(),
        })
    }

    #[test]
    fn twenty_drags_in_one_read_reach_the_page_as_one_and_what_is_pressed_and_let_go_keeps_its_place()
     {
        let left = Button::Left;
        let mut read = vec![mouse(MouseKind::Press(left), 3, 4)];
        read.extend((0..20).map(|at| mouse(MouseKind::Drag(left), 4 + at, 4)));
        read.push(mouse(MouseKind::Release(left), 23, 4));
        read.extend((0..5).map(|at| mouse(MouseKind::Move, 30 + at, 9)));
        assert_eq!(
            latest_pointer(read),
            [
                mouse(MouseKind::Press(left), 3, 4),
                mouse(MouseKind::Drag(left), 23, 4),
                mouse(MouseKind::Release(left), 23, 4),
                mouse(MouseKind::Move, 34, 9),
            ],
            "a run is its last; the press and the release stay, with the run between them"
        );
    }

    #[test]
    fn a_run_of_pointer_moves_is_cut_at_what_is_not_its_kind() {
        let (left, right) = (Button::Left, Button::Right);
        let shifted = Mods {
            shift: true,
            ..Mods::default()
        };
        let key = Event::Key(KeyEvent {
            key: Key::Down,
            mods: Mods::default(),
            text: None,
            down: true,
        });
        let read = vec![
            mouse(MouseKind::Drag(left), 1, 1),
            mouse(MouseKind::Drag(left), 2, 1),
            // Another button, then the same again: neither is the run of the one before.
            mouse(MouseKind::Drag(right), 3, 1),
            mouse(MouseKind::Drag(left), 4, 1),
            // A key between two of the same kind: both are kept.
            key.clone(),
            mouse(MouseKind::Move, 5, 1),
            key.clone(),
            mouse(MouseKind::Move, 6, 1),
            // Other modifiers: not the same run.
            Event::Mouse(Mouse {
                kind: MouseKind::Move,
                col: 7,
                row: 1,
                mods: shifted,
            }),
            mouse(MouseKind::Wheel { dx: 0, dy: 1 }, 7, 1),
            mouse(MouseKind::Wheel { dx: 0, dy: 1 }, 7, 1),
        ];
        let kept = latest_pointer(read.clone());
        assert_eq!(kept.len(), read.len() - 1, "{kept:?}");
        assert_eq!(kept[0], mouse(MouseKind::Drag(left), 2, 1));
        assert_eq!(&kept[1..], &read[2..]);
    }
}
