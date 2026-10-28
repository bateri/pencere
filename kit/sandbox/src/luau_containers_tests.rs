//! The container list as a script (`kit/luau/examples/containers.lua`) against the engine's own
//! page (`containers_page::Containers`): the same inputs, the same steps of time and the same
//! messages go to both, and what the script hands out has to be, element for element, what the
//! page makes. The engine's page is the reference.
//!
//! The script computes in doubles where the page computes in single precision, so the trees are
//! compared as text with every number rounded to three places (the numbers inside a line of
//! text are not rounded: a text has to be the same text). Nothing else is forgiven.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use containers_page::{Containers, FIELD, Reading};
use pencere_view::{
    Button, Element, Event, Id, Input, Key, Keyboard, Laid, Mods, Phase, Point, Reply, Role,
    ScrollTo, Scrolls, Size, div, layout, text,
};

use crate::guests_tests::{Open, Rough, SIZE};
use crate::{Module, ModuleLimits, Program, Script, Unbounded, Update};

const SCRIPT: &str = include_str!("../../luau/examples/containers.lua");

/// The script's: seconds without a reading after which it says none is coming, and what it says.
const STALE_AFTER: f64 = 5.0;
const STALE_NOTE: &str = "No new readings from containers-rust";

/// How long a step may take before the test gives up on it.
const PATIENCE: Duration = Duration::from_secs(60);

/// What these tests are about is the page and what it does, not how fast: a call may take as long
/// as a machine running many tests at once makes it (the quarter of a second a module is held to
/// is the sandbox tests' business).
fn roomy() -> ModuleLimits {
    ModuleLimits {
        call: Duration::from_secs(20),
        start: Duration::from_secs(30),
        ..ModuleLimits::default()
    }
}

/// The application's clock, as a test keeps it: it stands at the time of the last frame it asked
/// for and moves when the test says. The module is made for this time when its program speaks,
/// and not for the time of the last frame and what the system's clock says has passed since, which
/// in a test is as long as the test took to get there: on a machine that is busy a different page
/// from the one the test is written for.
struct OnClock {
    program: Arc<dyn Program>,
    now: AtomicU64,
}

impl Program for OnClock {
    fn may_say(&self, bytes: usize) -> bool {
        self.program.may_say(bytes)
    }

    fn taken(&self, bytes: usize) {
        self.program.taken(bytes);
    }

    fn now(&self) -> Option<f64> {
        Some(f64::from_bits(self.now.load(Ordering::SeqCst)))
    }
}

/// The script's page open, with the clock the module is told the time by.
struct Scripted {
    open: Open,
    clock: Arc<OnClock>,
}

impl std::ops::Deref for Scripted {
    type Target = Open;

    fn deref(&self) -> &Open {
        &self.open
    }
}

impl std::ops::DerefMut for Scripted {
    fn deref_mut(&mut self) -> &mut Open {
        &mut self.open
    }
}

impl Scripted {
    /// Asks the module for a frame at `now`, which is the time on the clock from then on.
    fn render(&self, size: Size, now: f64, reduce_motion: bool) {
        self.clock.now.store(now.to_bits(), Ordering::SeqCst);
        self.open.module.render(size, now, reduce_motion);
    }
}

fn open_script(program: Arc<dyn Program>) -> Scripted {
    let (told, woken) = channel();
    let told = std::sync::Mutex::new(told);
    let clock = Arc::new(OnClock {
        program: Arc::clone(&program),
        now: AtomicU64::new(0.0_f64.to_bits()),
    });
    let module = Module::from_script(
        Script {
            path: "containers.lua".to_owned(),
            source: SCRIPT.as_bytes().to_vec(),
            bundle: None,
        },
        roomy(),
        clock.clone(),
        move || {
            let _ = told.lock().unwrap().send(());
        },
    );
    Scripted {
        open: Open {
            module,
            woken,
            seen: Update::default(),
        },
        clock,
    }
}

// --- Comparing -------------------------------------------------------------------------------

/// `debug` with every number that has a fraction or an exponent rounded to three places, but not
/// the ones inside a quoted text.
fn rounded(debug: &str) -> String {
    let chars: Vec<char> = debug.chars().collect();
    let digit = |at: usize| chars.get(at).is_some_and(char::is_ascii_digit);
    let mut out = String::new();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c == '"' {
            // A text: copied as it is, to its closing quote.
            out.push(c);
            at += 1;
            while at < chars.len() {
                let d = chars[at];
                out.push(d);
                at += 1;
                if d == '\\' && at < chars.len() {
                    out.push(chars[at]);
                    at += 1;
                } else if d == '"' {
                    break;
                }
            }
            continue;
        }
        let in_word = at > 0 && (chars[at - 1].is_alphanumeric() || chars[at - 1] == '_');
        let starts = !in_word && (digit(at) || (c == '-' && digit(at + 1)));
        if !starts {
            out.push(c);
            at += 1;
            continue;
        }
        let begin = at;
        if c == '-' {
            at += 1;
        }
        while digit(at) {
            at += 1;
        }
        let mut real = false;
        if chars.get(at) == Some(&'.') && digit(at + 1) {
            real = true;
            at += 1;
            while digit(at) {
                at += 1;
            }
        }
        if matches!(chars.get(at), Some('e' | 'E'))
            && (digit(at + 1) || (matches!(chars.get(at + 1), Some('-' | '+')) && digit(at + 2)))
        {
            real = true;
            at += 2;
            while digit(at) {
                at += 1;
            }
        }
        let number: String = chars[begin..at].iter().collect();
        if real {
            let value: f64 = number.parse().expect("a number the debug text wrote");
            let written = format!("{value:.3}");
            out.push_str(if written == "-0.000" {
                "0.000"
            } else {
                &written
            });
        } else {
            out.push_str(&number);
        }
    }
    out
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn numbers_are_rounded_and_texts_are_not() {
    assert_eq!(
        rounded("a(0.13, 7, -1e-7, 22.0004)"),
        "a(0.130, 7, 0.000, 22.000)"
    );
    assert_eq!(
        rounded(r#"Text("37.6% of 0.5") x 2.5"#),
        r#"Text("37.6% of 0.5") x 2.500"#
    );
    assert_eq!(
        rounded(r#"Text("a \" 1.5 \" b") 1.5"#),
        r#"Text("a \" 1.5 \" b") 1.500"#
    );
    assert_eq!(rounded("Id(12345) u8"), "Id(12345) u8");
    assert_eq!(rounded("[0.0004999, 0.0005001]"), "[0.000, 0.001]");
}

/// The nodes of a tree as text, depth-first, numbers rounded.
fn nodes(root: &Element) -> Vec<String> {
    root.fold()
        .iter()
        .map(|node| rounded(&format!("{node:?}")))
        .collect()
}

/// Says where the first element is that differs, with both, if any does.
fn assert_same(what: &str, lua: &Element, native: &Element) {
    let (lua, native) = (nodes(lua), nodes(native));
    for (at, (one, other)) in lua.iter().zip(&native).enumerate() {
        assert!(
            one == other,
            "{what}: the element {at} differs\n  script: {one}\n  page:   {other}"
        );
    }
    assert_eq!(
        lua.len(),
        native.len(),
        "{what}: the script has {} elements, the page {}",
        lua.len(),
        native.len()
    );
}

fn assert_same_reply(what: &str, lua: &Reply, native: &Reply) {
    let (lua, native) = (
        rounded(&format!("{lua:?}")),
        rounded(&format!("{native:?}")),
    );
    assert!(
        lua == native,
        "{what}: the replies differ\n  script: {lua}\n  page:   {native}"
    );
}

// --- A script and the page, driven together --------------------------------------------------

/// The script in the sandbox and the engine's page beside it, both told the same things the way
/// the application tells a page: a frame is made, laid out, and the module is told the layout;
/// an event that changed the page makes the next frame for the time of the last.
struct Pair {
    lua: Scripted,
    native: Containers,
    size: Size,
    /// The time of the last frame.
    now: f64,
    /// The page's last frame, and whether it said it was still moving.
    tree: Element,
    animating: bool,
    /// The page's last frame laid out: what both are asked of.
    laid: Arc<Laid>,
    /// What the filter field says.
    query: String,
    /// The person's Reduce Motion setting, as of the last frame.
    reduce_motion: bool,
    /// How many of the module's answers to events have been looked at.
    looked: usize,
    /// When the program's newest reading came: the page says so when none has for a while.
    last_reading: Option<f64>,
    /// How far the application has the boxes scrolled.
    scrolls: Scrolls,
    /// What the page asked the application to scroll, not yet held against what the script
    /// asked.
    asked: Vec<ScrollTo>,
}

impl Pair {
    fn open(size: Size) -> Self {
        let lua = open_script(Arc::new(Unbounded));
        lua.render(size, 0.0, false);
        let mut native = Containers::fixture();
        native.advance(0.0, false);
        let tree = native.view(size);
        let animating = native.animating();
        let laid = Arc::new(layout(tree.clone(), size, 1.0, &mut Rough));
        let mut pair = Self {
            lua,
            native,
            size,
            now: 0.0,
            tree,
            animating,
            laid,
            query: String::new(),
            reduce_motion: false,
            looked: 0,
            last_reading: None,
            scrolls: Scrolls::new(),
            asked: Vec::new(),
        };
        // (The first frame waits for the interpreter to be compiled.)
        pair.wait_for_frame();
        pair.lay_out();
        pair.check("the first page");
        pair
    }

    /// Takes until the module has put out a frame.
    fn wait_for_frame(&mut self) {
        let began = Instant::now();
        while self.lua.seen.frame.is_none() {
            assert!(began.elapsed() < PATIENCE, "no frame came");
            let update = self.lua.take(Duration::from_millis(50));
            assert_eq!(update.closed, None);
        }
    }

    /// The answer to the next event told, waited for.
    fn answer(&mut self) -> Reply {
        let began = Instant::now();
        while self.lua.seen.replies.len() <= self.looked {
            assert!(began.elapsed() < PATIENCE, "no answer came");
            let update = self.lua.take(Duration::from_millis(50));
            assert_eq!(update.closed, None);
        }
        self.looked += 1;
        self.lua.seen.replies[self.looked - 1].reply.clone()
    }

    /// Waits until the module has done all it was told: an event that changes nothing goes after
    /// it, and what the module does is done in order.
    fn sync(&mut self) {
        self.lua.module.event(Event::Text {
            field: FIELD,
            text: self.query.clone(),
        });
        let reply = self.answer();
        assert!(!reply.redraw, "the filter said again changed something");
    }

    /// The script's last frame is the page's, and it asked for the same scrolls.
    fn check(&mut self, what: &str) {
        self.sync();
        let (lua, native) = (
            rounded(&format!("{:?}", std::mem::take(&mut self.lua.seen.asks))),
            rounded(&format!("{:?}", std::mem::take(&mut self.asked))),
        );
        assert!(
            lua == native,
            "{what}: the scrolls asked differ\n  script: {lua}\n  page:   {native}"
        );
        let frame = self.lua.seen.frame.clone().expect("a frame");
        assert_same(what, &frame.root, &self.tree);
        assert_eq!(
            frame.animating, self.animating,
            "{what}: the script and the page differ in whether the page is still moving"
        );
        assert_eq!(
            frame.wake_at.is_some(),
            !self.animating && self.last_reading.is_some() && !self.stale(),
            "{what}: the script asks to be woken when no reading has come, and only then"
        );
    }

    /// No reading has come for a while (the script's `STALE_AFTER`).
    fn stale(&self) -> bool {
        self.last_reading
            .is_some_and(|at| self.now >= at + STALE_AFTER)
    }

    /// The engine's page for `size`, with the script's note over it when the readings stopped.
    fn view(&self, size: Size) -> Element {
        let list = self.native.view(size);
        if self.stale() {
            with_note(list, STALE_NOTE)
        } else {
            list
        }
    }

    /// The page was made anew: the application does the scrolls the page asked for, lays it out
    /// where its boxes are scrolled and tells the module, which settles what depends on the
    /// layout, and both where the boxes went; what changed the page makes it again, up to three
    /// times.
    fn lay_out(&mut self) {
        for _ in 0..3 {
            let asks = self.native.take_asks();
            self.asked.extend(asks.iter().copied());
            for ask in asks {
                self.scrolls.ask(ask);
            }
            let mut root = self.tree.clone();
            self.scrolls.apply(&mut root);
            let mut laid = layout(root, self.size, 1.0, &mut Rough);
            self.scrolls.place(&mut laid, self.now, true);
            if !self.placed(laid) {
                return;
            }
            self.again();
        }
    }

    /// The page is `laid` from now on: the module is told, both settle, and both are told where
    /// the boxes went. Whether that changed the page.
    fn placed(&mut self, laid: Laid) -> bool {
        self.laid = Arc::new(laid);
        self.lua.module.lay_out(Arc::clone(&self.laid));
        let mut again = self.native.settle(&*self.laid);
        let moved = self.scrolls.take_moved();
        if !moved.is_empty() {
            self.lua.module.event(Event::Scrolled(moved.clone()));
            let lua = self.answer();
            let native = self.native.scrolled(&moved);
            assert_same_reply("the boxes scrolled", &lua, &native);
            again |= native.redraw;
        }
        again
    }

    /// The wheel turned over `at` by `dy`: the application scrolls the box under it on the page
    /// as it is, or, where none took it, the page is told.
    fn wheel(&mut self, what: &str, at: Point, dy: f32) {
        let mut laid = (*self.laid).clone();
        let wheel = self
            .scrolls
            .wheel(&laid, at, 0.0, dy, Phase::Wheel, self.now);
        if !wheel.took {
            let (dx, dy) = (wheel.left.x, wheel.left.y);
            self.input(what, Input::Scroll { at, dx, dy });
            return;
        }
        self.scrolls.place(&mut laid, self.now, true);
        if self.placed(laid) {
            self.again();
            self.lay_out();
        }
        self.check(what);
    }

    /// The page again, for the time of the last frame.
    fn again(&mut self) {
        self.native.advance(self.now, self.reduce_motion);
        self.tree = self.view(self.size);
        self.animating = self.native.animating();
    }

    /// A frame for `now`, at the size the pair has.
    fn at(&mut self, now: f64) {
        self.render(self.size, now);
    }

    fn render(&mut self, size: Size, now: f64) {
        self.size = size;
        self.now = now;
        self.lua.render(size, now, self.reduce_motion);
        self.native.advance(now, self.reduce_motion);
        self.tree = self.view(size);
        self.animating = self.native.animating();
        self.lay_out();
        self.check(&format!("the page at {now} s, {} pt wide", size.w));
    }

    /// Steps the time by 50 ms until the page rests (and at most `most` times), checking each frame.
    fn rest(&mut self, most: usize) {
        for _ in 0..most {
            if !self.animating {
                return;
            }
            self.at(self.now + 0.05);
        }
        assert!(!self.animating, "the page did not come to rest");
    }

    /// Tells both the page what the person did. Returns the reply they agree on.
    fn event(&mut self, what: &str, event: Event) -> Reply {
        self.lua.module.event(event.clone());
        let lua = self.answer();
        let native = match event {
            Event::Input(Input::Key {
                key: Key::Character('U'),
                mods,
            }) if !mods.any_command() => {
                // The script's own key (the page nobody opened sends nothing).
                Reply::redraw()
            }
            Event::Input(input) => self.native.input(input, &*self.laid),
            Event::Text { text: words, .. } => {
                self.query.clone_from(&words);
                self.native.set_query(&words)
            }
            Event::Focus(_)
            | Event::Command(_)
            | Event::Files { .. }
            | Event::Pointer { .. }
            | Event::Edit { .. }
            | Event::Selection { .. }
            | Event::Viewport { .. }
            | Event::EditorRefused { .. }
            | Event::Found { .. }
            | Event::Wake { .. } => Reply::none(),
            Event::MenuPick(index) => self.native.pick(index),
            Event::Scrolled(boxes) => self.native.scrolled(&boxes),
        };
        assert_same_reply(what, &lua, &native);
        if native.redraw {
            self.again();
            self.lay_out();
        }
        self.check(what);
        native
    }

    fn input(&mut self, what: &str, input: Input) -> Reply {
        self.event(what, Event::Input(input))
    }

    fn key(&mut self, key: Key) -> Reply {
        self.key_with(key, Mods::default())
    }

    fn key_with(&mut self, key: Key, mods: Mods) -> Reply {
        self.input(&format!("{key:?} with {mods:?}"), Input::Key { key, mods })
    }

    /// Where the middle of the element named `name` stands on the page as it is laid out.
    fn middle(&self, name: &str) -> Point {
        let b = self
            .laid
            .bounds(Id::named(name))
            .unwrap_or_else(|| panic!("{name} is not on the page"));
        Point::new(b.x + b.w / 2.0, b.y + b.h / 2.0)
    }

    /// A click with the left button on the element named `name`.
    fn click(&mut self, name: &str, mods: Mods) {
        let at = self.middle(name);
        self.input(
            &format!("the press on {name}"),
            Input::Press {
                at,
                button: Button::Left,
                mods,
                count: 1,
            },
        );
        self.input(
            &format!("the release on {name}"),
            Input::Release {
                at,
                button: Button::Left,
            },
        );
    }

    /// The program says `message`: the page the script makes of it is checked against the
    /// engine's page with `readings` told to it (none: the message changes nothing).
    fn message(&mut self, message: &str, readings: &[Reading]) {
        let before = self.lua.seen.frame.take();
        self.lua.module.message(message.to_owned());
        if self.native.live(readings) {
            self.wait_for_frame();
            self.again();
            self.lay_out();
        } else {
            let update = self.lua.take(Duration::from_millis(400));
            assert_eq!(update.closed, None);
            assert!(update.frame.is_none(), "{message} made a page");
            self.lua.seen.frame = before;
        }
        self.check(message);
    }

    /// The program pushes to the page's subscription: whatever it carries, it is a frame — the
    /// page learns that readings are still coming — with `readings` told to the engine's page.
    fn reading(&mut self, message: &str, readings: &[Reading]) {
        self.lua.seen.frame = None;
        self.lua.module.message(message.to_owned());
        self.native.live(readings);
        self.last_reading = Some(self.now);
        self.wait_for_frame();
        self.again();
        self.lay_out();
        self.check(message);
    }
}

fn command() -> Mods {
    Mods {
        command: true,
        ..Mods::default()
    }
}

fn shifted() -> Mods {
    Mods {
        shift: true,
        ..Mods::default()
    }
}

// --- At rest ---------------------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_script_draws_the_page_the_engine_draws_and_asks_its_program_for_readings() {
    let mut pair = Pair::open(SIZE);
    assert!(!pair.animating);
    assert!(pair.laid.len() > 100, "{} elements", pair.laid.len());
    // At its start the page asked the program (if there is one) to be told the numbers.
    assert_eq!(
        pair.lua.seen.sent,
        [r#"{"subscribe":{"id":1,"topic":"stats"}}"#.to_owned()]
    );
    // It makes the same page at every width: the rail, the image, the ports and the memory
    // go as the page narrows, and the keys at the bottom with them.
    for (width, height) in [
        (1400.0, 700.0),
        (1000.0, 700.0),
        (820.0, 600.0),
        (780.0, 500.0),
        (700.0, 700.0),
        (600.0, 500.0),
        (560.0, 400.0),
        (460.0, 700.0),
        (300.0, 300.0),
    ] {
        pair.render(Size::new(width, height), 0.0);
    }
}

// --- The keys --------------------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn two_keys_down_move_the_cursor_and_the_highlight_comes_to_rest() {
    let mut pair = Pair::open(SIZE);
    for _ in 0..2 {
        let reply = pair.key(Key::Down);
        assert!(reply.redraw);
    }
    // The highlight moves with time: the page says it is moving until it has come to rest.
    assert!(pair.animating, "a highlight on its way is a moving page");
    let before = pair.tree.clone();
    pair.rest(30);
    assert!(before != pair.tree, "two keys down changed nothing");
    // At rest again, and the frame for a later time is the same.
    pair.at(5.0);
    assert!(!pair.animating);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_keys_of_the_list_move_fold_mark_and_filter_it_as_the_engines_page_does() {
    let mut pair = Pair::open(SIZE);
    let keys = [
        // Moving about.
        Key::Down,
        Key::Character('j'),
        Key::Up,
        Key::Character('k'),
        Key::PageDown,
        Key::PageUp,
        Key::End,
        Key::Home,
        Key::Character('G'),
        Key::Character('g'),
        // Folding the project the cursor is on, with ⏎, with ← and with →.
        Key::Enter,
        Key::Enter,
        Key::Left,
        Key::Right,
        Key::Right,
        Key::Down,
        Key::Left,
        Key::Down,
        Key::Down,
        // Marking: one, a run, all, none.
        Key::Character(' '),
        Key::Character(' '),
        Key::Character('a'),
        Key::Character('A'),
        Key::Character('A'),
        Key::Up,
        Key::Up,
        Key::Character(' '),
        Key::Character(' '),
        Key::Character(' '),
        // The status filter cycles, the grouping flips.
        Key::Character('f'),
        Key::Character('f'),
        Key::Character('f'),
        Key::Character('f'),
        Key::Character('P'),
        Key::Down,
        Key::Left,
        Key::Character('P'),
        // A project's row marked (all its containers) and cleared.
        Key::Home,
        Key::Character(' '),
        Key::Character(' '),
        // Where the keyboard goes, the end of the page, and what is no key of the list's.
        Key::Tab,
        Key::Character('/'),
        Key::Character('u'),
        Key::Character('1'),
        Key::Escape,
    ];
    for key in keys {
        pair.key(key);
        pair.at(pair.now + 0.05);
    }
    pair.rest(30);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_commands_of_the_list_act_on_the_cursor_row_and_on_the_marked_ones() {
    let mut pair = Pair::open(SIZE);
    // On `api`: stop it (a stopped one, `s` starts it), restart, pause and resume, its id.
    for letter in ['s', 's', 'r', 'p', 'p', 'y', 'K', 'r'] {
        let reply = pair.key(Key::Character(letter));
        if letter == 'y' {
            assert!(reply.copy.is_some(), "no id was copied");
        }
        pair.at(pair.now + 0.05);
    }
    // On the marked ones.
    for key in [
        Key::Down,
        Key::Down,
        Key::Down,
        Key::Character(' '),
        Key::Character(' '),
        Key::Character(' '),
        Key::Character('s'),
        Key::Character('p'),
        Key::Character('K'),
        Key::Character('s'),
        // A project's row: its containers.
        Key::Character('A'),
        Key::Home,
        Key::Character('y'),
        Key::Character('r'),
        // Removing: a container, the ones marked, the whole of a project.
        Key::Down,
        Key::Character('d'),
        Key::Character(' '),
        Key::Character('d'),
        Key::Home,
        Key::Character('d'),
    ] {
        pair.key(key);
    }
    pair.at(pair.now + 0.05);
    pair.rest(30);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_keys_of_the_application_are_not_the_lists() {
    let mut pair = Pair::open(SIZE);
    // ⌘, ⌃ and ⌥ are the application's: the list reads none of them.
    for key in [
        Key::Character('j'),
        Key::Down,
        Key::Character('s'),
        Key::Character('U'),
    ] {
        for mods in [
            command(),
            Mods {
                control: true,
                ..Mods::default()
            },
            Mods {
                alt: true,
                ..Mods::default()
            },
        ] {
            pair.key_with(key, mods);
        }
    }
    // Shift is not: a capital letter is typed with it.
    pair.key_with(Key::Character('G'), shifted());
    pair.key_with(Key::Character('A'), shifted());
}

// --- The filter ------------------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn what_is_typed_in_the_filter_narrows_the_list_and_what_finds_nothing_says_so() {
    let mut pair = Pair::open(SIZE);
    let before = pair.tree.fold().len();
    let reply = pair.event(
        "api",
        Event::Text {
            field: FIELD,
            text: "api".into(),
        },
    );
    assert!(reply.redraw);
    assert!(
        pair.tree.fold().len() < before,
        "the filter left the list as long"
    );
    pair.rest(30);
    // The same text again changes nothing.
    let reply = pair.event(
        "api again",
        Event::Text {
            field: FIELD,
            text: "api".into(),
        },
    );
    assert!(!reply.redraw);
    for words in [
        "  SHOP ",
        "\u{a0}api\u{a0}",
        "\u{3000}\u{2003}SHOP\u{2028}",
        "\u{a0}",
        "9200",
        "nginx",
        "no such container",
        "\u{e7}ok ola",
        "",
        "d",
        "",
    ] {
        pair.event(
            words,
            Event::Text {
                field: FIELD,
                text: words.into(),
            },
        );
        pair.rest(30);
    }
    // With a status chosen, a filter that finds nothing says which state has no container.
    pair.key(Key::Character('f'));
    pair.key(Key::Character('f'));
    pair.event(
        "nothing",
        Event::Text {
            field: FIELD,
            text: "zzz".into(),
        },
    );
    pair.event(
        "nothing again",
        Event::Text {
            field: FIELD,
            text: String::new(),
        },
    );
    pair.rest(30);
    // And so does a state with no container left in it: the issues, removed.
    pair.key(Key::Character('f'));
    pair.key(Key::Character('d'));
    pair.key(Key::Character('f'));
    pair.rest(30);
}

// --- The wheel -------------------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_wheel_scrolls_the_list_and_the_keys_keep_the_cursor_in_view() {
    let mut pair = Pair::open(Size::new(1000.0, 300.0));
    let over = pair.middle("containers list");
    for dy in [40.0, 90.0, 400.0, -30.0, -1000.0, 120.0, 5000.0, -1.0] {
        pair.wheel("the wheel", over, dy);
        pair.rest(30);
    }
    // The wheel over the toolbar has nothing to scroll.
    let toolbar = pair.middle("containers filter");
    pair.wheel("the wheel over the toolbar", toolbar, 100.0);
    // The cursor goes on down the list and takes the list with it, and back.
    for key in [
        Key::End,
        Key::PageUp,
        Key::PageUp,
        Key::Home,
        Key::PageDown,
        Key::PageDown,
        Key::Down,
    ] {
        pair.key(key);
        pair.rest(30);
    }
    // A list that gets shorter than its scroll takes the scroll back within its reach.
    pair.wheel("the wheel", over, 5000.0);
    pair.event(
        "a short list",
        Event::Text {
            field: FIELD,
            text: "api".into(),
        },
    );
    pair.rest(30);
    pair.event(
        "the whole list",
        Event::Text {
            field: FIELD,
            text: String::new(),
        },
    );
    pair.rest(30);
    // A window that grows takes in more rows.
    pair.render(Size::new(1000.0, 700.0), pair.now + 0.05);
    pair.rest(30);
}

// --- The pointer -----------------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_pointer_lights_the_row_it_is_over_and_the_light_comes_and_goes_with_time() {
    let mut pair = Pair::open(SIZE);
    let db = pair.middle("containers container db");
    let reply = pair.input("over db", Input::Move(db));
    assert!(reply.redraw);
    assert!(pair.animating, "a highlight on its way is a moving page");
    // A step of time, and another: the highlight comes up.
    pair.at(0.05);
    pair.at(0.10);
    // The pointer moving inside the row it is over changes nothing.
    let inside = Point::new(db.x + 20.0, db.y + 3.0);
    let reply = pair.input("inside db", Input::Move(inside));
    assert!(!reply.redraw);
    pair.at(0.15);
    // Onto another row while the first is still on its way up, and onto the filter's box, a tab,
    // a button and a place that answers nothing.
    for name in [
        "containers group shop",
        "containers filter",
        "containers status issues",
        "containers group",
        "containers sort",
        "containers container worker",
    ] {
        let at = pair.middle(name);
        pair.input(name, Input::Move(at));
        pair.at(pair.now + 0.05);
    }
    pair.input("over nothing", Input::Move(Point::new(1.0, 1.0)));
    pair.rest(30);
    pair.input("away", Input::Move(db));
    pair.input("the pointer left", Input::Leave);
    pair.rest(30);
    // The pointer leaving a page it was not on changes nothing.
    let reply = pair.input("left again", Input::Leave);
    assert!(!reply.redraw);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_page_that_moves_under_the_pointer_lights_the_row_that_comes_under_it() {
    let mut pair = Pair::open(Size::new(1000.0, 300.0));
    let at = Point::new(600.0, 150.0);
    pair.input("the pointer", Input::Move(at));
    pair.rest(30);
    // The wheel moves the list under a pointer that has not moved: the hover is asked again
    // where the pointer stands, once the page is laid out.
    pair.wheel("the wheel", at, 100.0);
    pair.rest(30);
    pair.at(pair.now + 0.05);
    pair.rest(30);
}

/// Under Reduce Motion a highlight does not fade: it is at its end at once, and the page never
/// says it is moving.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn under_reduce_motion_a_highlight_is_at_its_end_at_once() {
    let mut pair = Pair::open(SIZE);
    // A highlight on its way when the setting is turned on is put at its end.
    pair.key(Key::Down);
    assert!(pair.animating);
    pair.reduce_motion = true;
    pair.at(0.05);
    assert!(!pair.animating);
    let db = pair.middle("containers container db");
    pair.key(Key::Down);
    assert!(!pair.animating, "a key moved the highlight slowly");
    pair.input("over db", Input::Move(db));
    assert!(!pair.animating, "the pointer lit a row slowly");
    pair.click("containers status stopped", Mods::default());
    pair.click("containers group", Mods::default());
    pair.key(Key::Character('f'));
    pair.event(
        "api",
        Event::Text {
            field: FIELD,
            text: "api".into(),
        },
    );
    pair.input("the pointer left", Input::Leave);
    assert!(!pair.animating);
    // Turned off again, the highlights fade as they did.
    pair.reduce_motion = false;
    pair.at(1.0);
    pair.event(
        "the whole list",
        Event::Text {
            field: FIELD,
            text: String::new(),
        },
    );
    pair.key(Key::Home);
    pair.key(Key::Down);
    assert!(pair.animating);
    pair.rest(30);
}

// --- The clicks ------------------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_click_folds_a_project_and_chooses_the_status_the_grouping_and_a_row() {
    let mut pair = Pair::open(SIZE);
    // A project's row folds, and unfolds, and the project that was folded away opens.
    pair.click("containers group shop", Mods::default());
    pair.at(0.05);
    pair.click("containers group shop", Mods::default());
    pair.rest(30);
    pair.click("containers group stats", Mods::default());
    pair.rest(30);
    // A container's row takes the cursor.
    pair.click("containers container grafana", Mods::default());
    pair.rest(30);
    // The status tabs and the grouping button.
    for name in [
        "containers status running",
        "containers status stopped",
        "containers status issues",
        "containers status issues",
        "containers status all",
        "containers group",
        "containers group",
    ] {
        pair.click(name, Mods::default());
        pair.at(pair.now + 0.05);
    }
    pair.rest(30);
    // A button that does nothing, and the filter's box, which asks for the keyboard.
    pair.click("containers sort", Mods::default());
    let at = pair.middle("containers filter");
    let reply = pair.input(
        "the press on the filter",
        Input::Press {
            at,
            button: Button::Left,
            mods: Mods::default(),
            count: 1,
        },
    );
    assert_eq!(reply.keyboard, Some(Keyboard::Field(FIELD)));
    pair.input(
        "the release on the filter",
        Input::Release {
            at,
            button: Button::Left,
        },
    );
    // A press on nothing, and a release somewhere else than the press.
    let nowhere = Point::new(1.0, 1.0);
    pair.input(
        "a press on nothing",
        Input::Press {
            at: nowhere,
            button: Button::Left,
            mods: Mods::default(),
            count: 1,
        },
    );
    pair.input(
        "its release",
        Input::Release {
            at: nowhere,
            button: Button::Left,
        },
    );
    let down = pair.middle("containers container api");
    let elsewhere = pair.middle("containers container db");
    pair.input(
        "a press",
        Input::Press {
            at: down,
            button: Button::Left,
            mods: Mods::default(),
            count: 1,
        },
    );
    pair.input(
        "a release elsewhere",
        Input::Release {
            at: elsewhere,
            button: Button::Left,
        },
    );
    pair.rest(30);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_click_with_a_modifier_marks_and_does_not_fold() {
    let mut pair = Pair::open(SIZE);
    pair.click("containers container cache", command());
    pair.click("containers container db", shifted());
    pair.click(
        "containers container search",
        Mods {
            control: true,
            ..Mods::default()
        },
    );
    // A project's row marked, and cleared by the same click, and not folded.
    pair.click("containers group shop", shifted());
    pair.click("containers group shop", shifted());
    pair.click("containers group blog", command());
    // Option does not mark: the click folds.
    pair.click(
        "containers group blog",
        Mods {
            alt: true,
            ..Mods::default()
        },
    );
    pair.rest(30);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_right_click_opens_the_menu_of_the_row_and_what_is_picked_is_done() {
    let mut pair = Pair::open(SIZE);
    let press = |at| Input::Press {
        at,
        button: Button::Right,
        mods: Mods::default(),
        count: 1,
    };
    let release = |at| Input::Release {
        at,
        button: Button::Right,
    };
    // A container that runs.
    let at = pair.middle("containers container api");
    let reply = pair.input("the right press", press(at));
    let menu = reply.menu.expect("a menu");
    assert!(menu.entries.len() > 8, "{menu:?}");
    // A key written as one character is the keys of the entry, as a command's are written.
    assert!(
        matches!(
            &menu.entries[0],
            pencere_view::MenuEntry::Item { keys: Some(keys), .. }
                if keys.key == Key::Character('s') && keys.mods == Mods::default()
        ),
        "{menu:?}"
    );
    pair.input("the right release", release(at));
    // A disabled entry, a line, and then one that works (the menu is spent after the pick).
    pair.event("a disabled entry", Event::MenuPick(4));
    let reply = pair.input("the right press", press(at));
    assert!(reply.menu.is_some());
    pair.event("a line", Event::MenuPick(3));
    let reply = pair.input("the right press", press(at));
    assert!(reply.menu.is_some());
    let reply = pair.event("copy id", Event::MenuPick(7));
    assert!(reply.copy.is_some());
    pair.event("a pick with no menu", Event::MenuPick(0));
    pair.rest(30);
    // An entry of the menus of a running, a stopped, a paused and an unhealthy container.
    for (name, pick) in [
        ("containers container api", 0),
        ("containers container api", 0),
        ("containers container db", 2),
        ("containers container db", 2),
        ("containers container mailer", 0),
        ("containers container search", 9),
        ("containers container web", 10),
    ] {
        let at = pair.middle(name);
        let reply = pair.input("the right press", press(at));
        assert!(reply.menu.is_some(), "{name}");
        pair.event(&format!("{name} pick {pick}"), Event::MenuPick(pick));
        pair.at(pair.now + 0.05);
    }
    // The menu of a project's row is that of its containers; of marked ones, theirs.
    let at = pair.middle("containers group blog");
    let reply = pair.input("a project's menu", press(at));
    assert!(reply.menu.is_some());
    pair.event("start them", Event::MenuPick(0));
    pair.key(Key::Home);
    pair.key(Key::Character('a'));
    let reply = pair.key(Key::Character('.'));
    let menu = reply.menu.expect("the menu of the key");
    assert!(!menu.entries.is_empty());
    pair.event("kill them", Event::MenuPick(menu.entries.len() - 2));
    pair.rest(30);
    // The key that opens the menu, with nothing marked and the cursor on no row, and a press on
    // the filter.
    pair.key(Key::Character('A'));
    pair.event(
        "a filter that finds nothing",
        Event::Text {
            field: FIELD,
            text: "zzz".into(),
        },
    );
    let reply = pair.key(Key::Character('.'));
    assert!(reply.menu.is_none());
    let at = pair.middle("containers filter");
    pair.input("a press on the filter", press(at));
    pair.rest(30);
}

// --- What the program says ---------------------------------------------------------------------

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_script_shows_what_its_program_pushes() {
    let mut pair = Pair::open(SIZE);
    // The program pushes a reading; the page that comes is the engine's own page with it.
    let reading = Reading {
        name: "api".into(),
        cpu: 42.5,
        memory: "130MiB".into(),
        share: 0.3,
    };
    pair.reading(
        r#"{"push":{"id":1,"value":{"readings":[{"name":"api","cpu":42.5,"memory":"130MiB","share":0.3}]}}}"#,
        std::slice::from_ref(&reading),
    );
    // Several at once, one of a stopped container and one of none, and one that is not shaped so.
    pair.reading(
        concat!(
            r#"{"push":{"id":1,"value":{"readings":["#,
            r#"{"name":"db","cpu":7,"memory":"0.5GiB","share":0.9},"#,
            r#"{"name":"mailer","cpu":5,"memory":"1MiB","share":0.1},"#,
            r#"{"name":"nobody","cpu":5,"memory":"1MiB","share":0.1},"#,
            r#"{"name":"web","cpu":"x","memory":"1MiB","share":0.1},"#,
            r#"{"name":"cache","cpu":12.25,"memory":"9.8MiB","share":0.7}"#,
            r#"]}}}"#
        ),
        &[
            Reading {
                name: "db".into(),
                cpu: 7.0,
                memory: "0.5GiB".into(),
                share: 0.9,
            },
            Reading {
                name: "cache".into(),
                cpu: 12.25,
                memory: "9.8MiB".into(),
                share: 0.7,
            },
        ],
    );
    // What a program makes up is kept within what the page can hold.
    let long = "x".repeat(10_000);
    let accents = r"ç".repeat(18);
    pair.reading(
        &format!(
            r#"{{"push":{{"id":1,"value":{{"readings":[{{"name":"api","cpu":1e30,"memory":"{long}","share":7}},{{"name":"web","cpu":-3,"memory":"{accents}","share":-1}}]}}}}}}"#
        ),
        &[
            Reading {
                name: "api".into(),
                cpu: 1e30,
                memory: long,
                share: 7.0,
            },
            Reading {
                name: "web".into(),
                cpu: -3.0,
                memory: "\u{e7}".repeat(18),
                share: -1.0,
            },
        ],
    );
    // The line keeps thirty samples: a reading each second for a while.
    for tick in 0..32_u32 {
        let cpu = f64::from(tick) * 1.37 + 0.2;
        pair.reading(
            &format!(
                r#"{{"push":{{"id":1,"value":{{"readings":[{{"name":"worker","cpu":{cpu},"memory":"936KiB","share":0.01}}]}}}}}}"#
            ),
            &[Reading {
                name: "worker".into(),
                cpu: cpu as f32,
                memory: "936KiB".into(),
                share: 0.01,
            }],
        );
    }
    // A push of the page's own that carries no readings changes no row, but it is a frame: the
    // program is still speaking.
    for empty in [
        r#"{"push":{"id":1,"value":{"readings":"api"}}}"#,
        r#"{"push":{"id":1}}"#,
    ] {
        pair.reading(empty, &[]);
    }
    // A push for another subscription, what is not a message, and a response: all of it changes
    // nothing and makes no page.
    for noise in [
        r#"{"push":{"id":7,"value":{"readings":[]}}}"#,
        r#"{"cancel":{"id":1}}"#,
        "[]",
        "not json",
        r#"{"response":{"id":9,"result":{}}}"#,
    ] {
        pair.message(noise, &[]);
    }
    assert_eq!(pair.lua.seen.sent.len(), 1, "and it said nothing back");
}

/// The readings stop: until five seconds have passed the page asks to be woken then, and then it
/// says that none is coming; the next reading takes the note away.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_script_says_when_the_readings_stop_and_asks_to_be_woken_for_it() {
    let mut pair = Pair::open(SIZE);
    let push = r#"{"push":{"id":1,"value":{"readings":[]}}}"#;
    pair.at(1.0);
    pair.reading(push, &[]);
    let woken = pair.lua.seen.frame.as_ref().and_then(|frame| frame.wake_at);
    assert!(
        woken.is_some_and(|at| (at - (1.0 + STALE_AFTER)).abs() < 0.5),
        "{woken:?}"
    );
    pair.at(5.9);
    assert!(!pair.stale());
    pair.at(6.1);
    assert!(
        pair.stale(),
        "the note is on, and the script asks for nothing more"
    );
    pair.at(30.0);
    pair.reading(push, &[]);
    assert!(!pair.stale());
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_pushed_numbers_show_in_the_rows_of_every_width() {
    let mut pair = Pair::open(SIZE);
    pair.reading(
        r#"{"push":{"id":1,"value":{"readings":[{"name":"search","cpu":88.88,"memory":"990MiB","share":0.93},{"name":"cache","cpu":0.15,"memory":"8MiB","share":0.7}]}}}"#,
        &[
            Reading {
                name: "search".into(),
                cpu: 88.88,
                memory: "990MiB".into(),
                share: 0.93,
            },
            Reading {
                name: "cache".into(),
                cpu: 0.15,
                memory: "8MiB".into(),
                share: 0.7,
            },
        ],
    );
    for width in [1400.0, 700.0, 500.0, 400.0] {
        pair.render(Size::new(width, 700.0), pair.now);
    }
}

// --- The payload -------------------------------------------------------------------------------

/// A program that has room for so many messages.
struct Room {
    left: AtomicUsize,
}

impl Room {
    fn with(messages: usize) -> Arc<Self> {
        Arc::new(Self {
            left: AtomicUsize::new(messages),
        })
    }

    fn add(&self, messages: usize) {
        self.left.fetch_add(messages, Ordering::SeqCst);
    }
}

impl Program for Room {
    fn may_say(&self, _bytes: usize) -> bool {
        self.left
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            })
            .is_ok()
    }

    fn taken(&self, _bytes: usize) {}
}

/// How much there is of the payload, and how much a chunk carries.
const UPLOAD_BYTES: usize = 4 << 20;
const CHUNK: usize = 48 << 10;

/// The number the page gave a call, in the message it said.
fn call_id(message: &str) -> u64 {
    message
        .split(r#""id":"#)
        .nth(1)
        .and_then(|rest| rest.split([',', '}']).next())
        .and_then(|n| n.parse().ok())
        .expect("a call with a number")
}

/// The number, the data and whether it was the last, of each chunk of the payload in `sent`.
fn chunks(sent: &[String]) -> Vec<(u64, usize, bool)> {
    sent.iter()
        .filter(|message| message.contains(r#""method":"upload""#))
        .map(|message| {
            let after = |key: &str| message.split(key).nth(1).expect("a member");
            let seq = after(r#""seq":"#)
                .split([',', '}'])
                .next()
                .and_then(|n| n.parse().ok())
                .expect("a number");
            let data = after(r#""data":""#).split('"').next().expect("data").len();
            let last = after(r#""last":"#).starts_with("true");
            (seq, data, last)
        })
        .collect()
}

/// The page of the engine with the note over it, as the page that sends the payload draws it.
fn with_note(list: Element, note: &str) -> Element {
    div().size_full().child(list).child(
        div()
            .absolute()
            .inset(None, Some(12.0), Some(36.0), None)
            .px(12.0)
            .py(6.0)
            .rounded(6.0)
            .bg(Role::Raised(2))
            .border(1.0, Role::Border)
            .text_size(12.5)
            .color(Role::Foreground)
            .child(text(note)),
    )
}

/// What the page says of a payload that the program was told `told` bytes of.
fn sending(told: usize) -> String {
    format!(
        "Sending 4 MiB to containers-rust... {}%",
        told * 100 / UPLOAD_BYTES
    )
}

/// Takes until `done` is true of what the module has said, or fails.
fn until_sent(page: &mut Open, what: &str, done: impl Fn(&[String]) -> bool) {
    let began = Instant::now();
    while !done(&page.seen.sent) {
        assert!(began.elapsed() < PATIENCE, "{what}");
        let update = page.take(Duration::from_millis(50));
        assert_eq!(update.closed, None);
    }
}

/// Takes until the module has put out a frame (the frame that was before is forgotten), and until
/// it has no more to put out for a moment: the page asked for earlier may come after the one a
/// message made, on a machine busy enough, and the page is the last of them.
fn next_frame(page: &mut Open) -> Element {
    page.seen.frame = None;
    let began = Instant::now();
    while page.seen.frame.is_none() {
        assert!(began.elapsed() < PATIENCE, "no frame came");
        assert_eq!(page.take(Duration::from_millis(50)).closed, None);
    }
    // Until nothing has come for a while. A taking returns when the application is woken, and the
    // module wakes it for what it says before it posts the frame that goes with it: a taking that
    // returned early with no frame is not a quiet one, the frame is on its way.
    let quiet = Duration::from_millis(200);
    loop {
        let began = Instant::now();
        let update = page.take(quiet);
        assert_eq!(update.closed, None);
        if update.frame.is_none() && began.elapsed() >= quiet {
            break;
        }
    }
    page.seen.frame.clone().expect("a frame").root
}

/// A frame for `now`, asked for and waited for: what the module was told before it is done.
fn settled(page: &mut Scripted, now: f64) -> Element {
    page.seen.frame = None;
    page.render(SIZE, now, false);
    next_frame(page)
}

fn upload_key() -> Event {
    Event::Input(Input::Key {
        key: Key::Character('U'),
        mods: shifted(),
    })
}

/// Shift-U on the container list sends its program 4 MiB in chunks, as far as the program has the
/// room: it is told *full*, keeps the chunk, goes on when called for room, and in the end every
/// byte is said once, in order, the last chunk saying so; the program counts it and the page
/// says so.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_script_sends_its_payload_as_fast_as_its_program_has_room_and_loses_none() {
    let program = Room::with(1 /* the subscription */ + 3);
    let mut page = open_script(program.clone());
    let mut native = Containers::fixture();
    native.advance(0.0, false);
    let list = native.view(SIZE);
    // (The first frame waits for the interpreter to be compiled.)
    let first = settled(&mut page, 0.0);
    assert_same("the first page", &first, &list);

    // A page nobody opened has no one to send the payload to: Shift-U says nothing.
    page.module.event(upload_key());
    let began = Instant::now();
    while page.seen.replies.is_empty() {
        assert!(began.elapsed() < PATIENCE, "no answer came");
        assert_eq!(page.take(Duration::from_millis(50)).closed, None);
    }
    assert!(page.seen.replies[0].reply.redraw);
    assert_eq!(chunks(&page.seen.sent), []);
    assert_same("nobody is there", &settled(&mut page, 0.1), &list);

    // This one is told its program is there by its first word.
    page.module
        .message(r#"{"response":{"id":9,"result":{}}}"#.to_owned());
    page.module.event(upload_key());
    until_sent(&mut page, "three chunks, then told full", |sent| {
        chunks(sent).len() >= 3
    });
    let frame = settled(&mut page, 0.2);
    assert_eq!(
        chunks(&page.seen.sent).len(),
        3,
        "three chunks, then told full"
    );
    // It says it has not been read: the program had no room for the next.
    assert_same(
        "told full",
        &frame,
        &with_note(
            list.clone(),
            &format!("{} (it has not read the last yet)", sending(3 * CHUNK)),
        ),
    );
    // Shift-U again while it goes on starts nothing.
    page.module.event(upload_key());
    settled(&mut page, 0.25);
    assert_eq!(chunks(&page.seen.sent).len(), 3);

    // The program reads; the page is called for room and goes on, a few chunks in a call and the
    // next call being its next frame. Nothing is closed.
    let mut calls = 0;
    while chunks(&page.seen.sent).last().is_none_or(|last| !last.2) {
        calls += 1;
        assert!(calls < 200, "the payload never ended");
        program.add(1_000);
        page.module.room();
        page.render(SIZE, f64::from(calls) * 0.016, false);
        page.until(Duration::from_millis(100), |_| false);
        assert!(page.seen.closed.is_none(), "{:?}", page.seen.closed);
    }
    let all = chunks(&page.seen.sent);
    assert_eq!(
        all.iter().map(|c| c.0).collect::<Vec<_>>(),
        (1..=all.len() as u64).collect::<Vec<_>>()
    );
    assert_eq!(all.iter().map(|c| c.1).sum::<usize>(), UPLOAD_BYTES);
    assert_eq!(all.iter().filter(|c| c.2).count(), 1);
    assert_eq!(
        all.last().map(|c| c.0),
        Some(UPLOAD_BYTES.div_ceil(CHUNK) as u64)
    );

    // Every byte is told and the program has not counted yet: the page says all of it was sent.
    let frame = settled(&mut page, 10.0);
    assert_same(
        "all told",
        &frame,
        &with_note(list.clone(), &sending(UPLOAD_BYTES)),
    );

    // The program counts it and says how much it got: the answer to the last chunk.
    let last = page
        .seen
        .sent
        .iter()
        .find(|message| message.contains(r#""last":true"#))
        .expect("the last chunk");
    let answer = format!(
        r#"{{"response":{{"id":{},"result":{{"bytes":{UPLOAD_BYTES}}}}}}}"#,
        call_id(last)
    );
    page.seen.frame = None;
    page.module.message(answer.clone());
    let frame = next_frame(&mut page);
    assert_same(
        "counted",
        &frame,
        &with_note(
            list,
            &format!("Sent 4 MiB - containers-rust counted {UPLOAD_BYTES} bytes"),
        ),
    );
    // The same answer again is the answer to nothing.
    page.seen.frame = None;
    page.module.message(answer);
    assert!(page.take(Duration::from_millis(400)).frame.is_none());
    assert!(page.seen.closed.is_none());
}

/// A program with room for everything is told 8 chunks in a call (the page's own limit for one
/// call) and the page asks for the next frame itself until it has told it all.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_program_with_room_is_told_a_few_chunks_in_each_frame() {
    let mut page = open_script(Arc::new(Unbounded));
    settled(&mut page, 0.0);
    page.module
        .message(r#"{"response":{"id":9,"result":{}}}"#.to_owned());
    page.module.event(upload_key());
    let mut now = 0.0;
    let mut frames = 0;
    loop {
        now += 0.016;
        page.seen.frame = None;
        page.render(SIZE, now, false);
        next_frame(&mut page);
        frames += 1;
        let animating = page
            .seen
            .frame
            .as_ref()
            .is_some_and(|frame| frame.animating);
        if chunks(&page.seen.sent).last().is_some_and(|last| last.2) {
            // Everything is said: nothing asks for another frame.
            assert!(!animating);
            break;
        }
        assert!(animating, "the payload goes on and the page did not ask");
        assert!(frames < 100, "the payload never ended");
    }
    // Eight at the first Shift-U, then eight in each frame, and the rest.
    let all = chunks(&page.seen.sent);
    assert_eq!(all.len(), UPLOAD_BYTES.div_ceil(CHUNK));
    assert!(
        frames >= all.len() / 8 - 1,
        "{frames} frames for {} chunks",
        all.len()
    );
    assert!(page.seen.closed.is_none());
}

/// A subscription the program had no room for is said when it has.
#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_subscription_the_program_had_no_room_for_is_said_when_it_has() {
    let program = Room::with(0);
    let mut page = open_script(program.clone());
    settled(&mut page, 0.0);
    assert_eq!(page.seen.sent, [] as [String; 0]);
    program.add(1);
    page.module.room();
    until_sent(&mut page, "the subscription was not said", |sent| {
        !sent.is_empty()
    });
    assert_eq!(
        page.seen.sent,
        [r#"{"subscribe":{"id":1,"topic":"stats"}}"#.to_owned()]
    );
    assert!(page.seen.closed.is_none());
}
