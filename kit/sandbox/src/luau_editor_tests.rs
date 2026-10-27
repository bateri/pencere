//! A Lua page with an editor, driven as a host drives one: the page laid out with the host's editors
//! (`pencere-editor`), what they tell sent to the page with the text it reads posted before it, what
//! the page asks of them done. The page is the example's (`examples/editor-lua`), and some short
//! scripts besides. Heavy: the interpreter is compiled and run (`make check-heavy`).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pencere_editor::{EditorEvent, Editors, Route};
use pencere_view::edit::{
    Caret, CaretHeight, CaretShape, CommentTokens, EditorAsk, EditorColors, EditorOptions, Folds,
    IndentRules, LineHighlight, Mark, Numbers, Origin, TextSnapshot, Tint, Whitespace,
};
use pencere_view::{
    Element, Event, Font, Id, Input, Key, Keyboard, Laid, Measure, Metrics, Mods, Paint, Point,
    Role, Scrolls, layout,
};

use crate::guests_tests::{Open, SIZE};
use crate::luau_tests::{failed, open_script, open_script_for, words};
use crate::{Fault, ModuleLimits, Program};

const PAGE: &str = include_str!("../../../examples/editor-lua/src/view/page.lua");

/// Every character 7 pixels across, a line 16 high.
struct Fixed;

impl Measure for Fixed {
    fn measure(&mut self, _font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().count() as f32 * 7.0,
            height: 16.0,
        }
    }
}

/// The host's clock, which stands where the rig puts it: the time the seconds a page asks to be
/// woken in are counted from.
struct Clock(Mutex<f64>);

impl Program for Clock {
    fn may_say(&self, _bytes: usize) -> bool {
        true
    }

    fn taken(&self, _bytes: usize) {}

    fn now(&self) -> Option<f64> {
        Some(*self.0.lock().unwrap())
    }
}

/// A frame of the host's, seconds.
const FRAME: f64 = 1.0 / 60.0;

/// A host of one page: the module, its layout, its scrolling and its editors.
struct Rig {
    page: Open,
    clock: Arc<Clock>,
    /// The time the page asked to be woken at, as the module last said.
    wake_due: Option<f64>,
    editors: Editors,
    scrolls: Scrolls,
    laid: Arc<Laid>,
    root: Element,
    now: f64,
    /// Everything the editors told the page, in order.
    told: Vec<EditorEvent>,
    /// What the module said to its program.
    sent: Vec<String>,
    /// Whether the last page the module made asked for the next frame.
    animating: bool,
    /// The most decorations one reply of the module's handed the editors.
    most: usize,
    /// Why the module closed, where a test expects it to ([`Rig::failing`]).
    fault: Option<Fault>,
    may_close: bool,
}

impl Rig {
    fn open(script: &str) -> Self {
        Self::start(script, false)
    }

    /// Runs a page that is to fail: why it did.
    fn failing(script: &str) -> String {
        let mut rig = Self::start(script, true);
        match rig.failure() {
            Some(Fault::Failed(why)) => why,
            other => panic!("expected the page to fail, it is {other:?}"),
        }
    }

    /// Why the page failed, waited for as long as a loaded machine may take: `run` comes to rest
    /// after 400 ms with nothing new, before a page that fails slowly (beside a build) has said so.
    fn failure(&mut self) -> Option<Fault> {
        if self.fault.is_none() && self.may_close {
            let update = self
                .page
                .until(Duration::from_secs(20), |u| u.closed.is_some());
            self.fault = update.closed.clone();
        }
        self.fault.clone()
    }

    fn start(script: &str, may_close: bool) -> Self {
        let clock = Arc::new(Clock(Mutex::new(0.0)));
        let mut rig = Self {
            page: open_script_for(script, ModuleLimits::default(), clock.clone()),
            clock,
            wake_due: None,
            editors: Editors::new(),
            scrolls: Scrolls::new(),
            laid: Arc::new(layout(pencere_view::div(), SIZE, 1.0, &mut Fixed)),
            root: pencere_view::div(),
            now: 0.0,
            told: Vec::new(),
            sent: Vec::new(),
            animating: false,
            most: 0,
            fault: None,
            may_close,
        };
        rig.page.module.render(SIZE, rig.now, false);
        // The interpreter is compiled before the first page: that is waited for.
        let first = rig.page.until(Duration::from_secs(30), |u| {
            u.frame.is_some() || u.closed.is_some()
        });
        rig.absorb(first);
        rig.run();
        rig
    }

    /// Lays the page out as it is, with the editors, and tells the module.
    fn lay_out(&mut self) {
        let mut root = self.root.clone();
        self.scrolls.apply(&mut root);
        let mut laid = layout(root, SIZE, 1.0, &mut self.editors.measure(&mut Fixed));
        self.scrolls.place(&mut laid, self.now, false);
        self.editors.sync(&laid, &mut Fixed);
        self.laid = Arc::new(laid);
        self.page.module.lay_out(Arc::clone(&self.laid));
    }

    /// What the editors have for the page, told it: the texts first, then the events.
    fn tell(&mut self) -> bool {
        for ask in self.editors.take_scrolls() {
            self.scrolls.ask(ask);
        }
        let events = self.editors.take_events();
        if events.is_empty() {
            return false;
        }
        let texts = self
            .editors
            .snapshots()
            .into_iter()
            .map(|(id, text)| (id, Arc::new(text) as Arc<dyn TextSnapshot>))
            .collect();
        self.page.module.texts(texts);
        for event in events {
            self.told.push(event.clone());
            self.page.module.event(event.into());
        }
        true
    }

    /// Goes on as a host does until nothing more comes: pages laid out, events told, replies done.
    fn run(&mut self) {
        for _ in 0..400 {
            let update = self.page.take(Duration::from_millis(400));
            if !self.absorb(update) {
                return;
            }
        }
        panic!("the page never came to rest");
    }

    /// Does what an update of the module asks: a page laid out, replies done, what the editors have
    /// told. Whether anything was.
    fn absorb(&mut self, update: crate::Update) -> bool {
        self.wake_due = update.wake;
        if let Some(fault) = &update.closed {
            if self.may_close {
                self.fault = Some(fault.clone());
                return false;
            }
            panic!("the page closed: {fault:?}");
        }
        self.sent.extend(update.sent.iter().cloned());
        for ask in &update.asks {
            self.scrolls.ask(*ask);
        }
        let mut busy = false;
        let mut redraw = false;
        let mut relay = false;
        if let Some(frame) = update.frame {
            self.root = frame.root;
            self.animating = frame.animating;
            relay = true;
            busy = true;
        }
        for answer in update.replies {
            busy = true;
            redraw |= answer.reply.redraw;
            let decorations = answer
                .reply
                .editor
                .iter()
                .map(|(_, ask)| match ask {
                    EditorAsk::Decorate(decorate) => decorate.decorations.len(),
                    _ => 0,
                })
                .sum();
            self.most = self.most.max(decorations);
            if let Some(Keyboard::Field(id)) = answer.reply.keyboard {
                self.editors.set_focus(Some(id));
            }
            let laid = Arc::clone(&self.laid);
            for (id, ask) in answer.reply.editor {
                self.editors.ask(&laid, id, ask, &mut Fixed, self.now);
            }
        }
        if relay || self.editors.wants_layout() || self.laid.wants_layout() {
            self.lay_out();
        }
        busy |= self.tell();
        if redraw {
            self.at(self.now + 0.1);
            self.page.module.render(SIZE, self.now, false);
        }
        busy || redraw
    }

    /// The host's time is `now`.
    fn at(&mut self, now: f64) {
        self.now = now;
        *self.clock.0.lock().unwrap() = now;
    }

    /// The page's next wake, as a host gives it: at the first frame at or after the time the page
    /// asked for (a frame after the last at the soonest), then the frame's page. Whether the page
    /// had asked for one.
    fn wake(&mut self) -> bool {
        let Some(due) = self.wake_due else {
            return false;
        };
        self.at((self.now + FRAME).max(due));
        self.page.module.wake(self.now);
        self.page.module.render(SIZE, self.now, false);
        // The frame comes after what the wake came to: what came is done as it comes.
        let began = Instant::now();
        loop {
            let update = self.page.take(Duration::from_millis(400));
            let framed = update.frame.is_some() || update.closed.is_some();
            self.absorb(update);
            if framed || self.fault.is_some() {
                break;
            }
            assert!(
                began.elapsed() < Duration::from_secs(20),
                "the frame after a wake never came"
            );
        }
        self.run();
        true
    }

    /// Wakes the page until it asks for no more wakes: how many it was woken.
    fn wakes(&mut self) -> usize {
        let mut count = 0;
        while self.wake() {
            count += 1;
            assert!(count < 500, "the page never stopped asking to be woken");
        }
        count
    }

    fn key(&mut self, key: Key, mods: Mods) -> Route {
        let laid = Arc::clone(&self.laid);
        let route = self.editors.key(&laid, key, mods, &mut Fixed, self.now);
        if let Route::Command(id) = route {
            self.page.module.event(Event::Command(id));
        }
        self.run();
        route
    }

    /// The next frame, as a host draws it when the page asked for it: rendered, laid out, what it
    /// came to told.
    fn frame(&mut self) {
        self.at(self.now + 0.1);
        self.page.module.render(SIZE, self.now, false);
        self.run();
    }

    /// The page's command `id`, as the host's list or its keys ask for it.
    fn command(&mut self, id: &str) {
        self.page.module.event(Event::Command(Id::named(id)));
        self.run();
    }

    fn type_text(&mut self, text: &str) {
        let laid = Arc::clone(&self.laid);
        self.editors.type_text(&laid, text, &mut Fixed, self.now);
        self.run();
    }

    fn text(&self, editor: &str) -> String {
        self.editors
            .buffer(Id::named(editor))
            .expect("the editor")
            .text()
    }
}

fn cmd() -> Mods {
    Mods {
        command: true,
        ..Mods::default()
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_example_loads_its_document_in_pieces_colours_what_is_in_view_and_folds_its_statements() {
    let rig = Rig::open(PAGE);
    let sql = Id::named("sql");
    let text = rig.text("sql");
    assert!(
        text.starts_with("-- A sample of SQL"),
        "{:?}",
        text.get(..40)
    );
    assert!(text.len() > 30_000);
    let loads = rig
        .told
        .iter()
        .filter(|event| {
            matches!(event, EditorEvent::Edited { edited, .. } if edited.origin == Origin::Load)
        })
        .count();
    assert!(loads > 2, "the sample came in {loads} pieces");
    let buffer = rig.editors.buffer(sql).expect("the editor");
    assert_eq!(buffer.layer_names(), vec!["syntax", "folds"]);
    // The first statement's words are coloured; one far out of view is not.
    let first = buffer.line_start(1);
    let coloured = |from: usize, to: usize| {
        buffer
            .decorations_in(from, to)
            .iter()
            .filter(|(_, d)| matches!(d.mark, Mark::Style(_)))
            .count()
    };
    assert!(coloured(first, first + 6) > 0, "`select` is coloured");
    let far = buffer.line_start(1500);
    assert_eq!(coloured(far, far + 40), 0, "a line out of view is not");
    assert!(buffer.folds().len() > 100, "each statement folds");
    // A page that searches for nothing is told nothing of a search.
    assert!(
        !rig.told
            .iter()
            .any(|event| matches!(event, EditorEvent::Found { .. })),
        "nothing was searched for"
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn the_examples_completions_stand_by_the_caret_and_take_their_keys_only_while_open() {
    let mut rig = Rig::open(PAGE);
    let sql = Id::named("sql");
    rig.editors.set_focus(Some(sql));
    assert_eq!(rig.key(Key::Up, cmd()), Route::Done);
    // ↓ is the editor's while no popup is open.
    assert_eq!(rig.key(Key::Down, Mods::default()), Route::Done);
    assert_eq!(rig.key(Key::Up, Mods::default()), Route::Done);
    assert_eq!(rig.key(Key::Character('s'), Mods::default()), Route::Type);
    rig.type_text("s");
    rig.type_text("e");
    let popup = rig
        .laid
        .bounds(Id::named("completions"))
        .expect("the popup is open");
    let caret = rig
        .laid
        .bounds(sql.caret())
        .expect("the caret is on the page");
    assert_eq!(popup.y, caret.bottom() + 2.0, "it stands below the caret");
    assert_eq!(popup.x, caret.x);
    // `se`: `select`, then `set`. ↓ is the page's now, and ↩ puts the chosen word in.
    assert_eq!(
        rig.key(Key::Down, Mods::default()),
        Route::Command(Id::named("completion-next"))
    );
    assert_eq!(
        rig.key(Key::Enter, Mods::default()),
        Route::Command(Id::named("completion-accept"))
    );
    assert!(
        rig.text("sql").starts_with("set -- A sample"),
        "{:?}",
        rig.text("sql").get(..20)
    );
    assert!(
        rig.laid.bounds(Id::named("completions")).is_none(),
        "it closed"
    );
    assert_eq!(rig.key(Key::Enter, Mods::default()), Route::Done);
    // ⌘↩ sends the statement under the caret to the program.
    assert_eq!(rig.key(Key::Enter, cmd()), Route::Command(Id::named("run")));
    assert!(
        rig.sent
            .iter()
            .any(|said| said.contains("\"method\":\"run\"")),
        "{:?}",
        rig.sent
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_page_reads_the_text_of_the_version_it_is_told_of_in_the_event_that_tells_it() {
    let rig = Rig::open(
        r#"
        local seen = "nothing"
        local asked = false
        return {
            render = function()
                return pencere.div():size_full()
                    :child(pencere.editor("e", { gutter = false }):grow(1))
                    :child(pencere.text(seen))
            end,
            handle = function(event)
                if event.type == "viewport" and not asked then
                    asked = true
                    return { editor = { e = { load = "hello\nwörld" } } }
                elseif event.type == "edit" then
                    local length = pencere.layout.editor_length("e")
                    seen = string.format("v%d=%s %s lines %d line1 %d of %s",
                        event.version, tostring(pencere.layout.editor_version("e")),
                        pencere.layout.editor_text("e", 6, length), pencere.layout.editor_lines("e"),
                        pencere.layout.line_start("e", 1), event.changes[1].insert)
                    return true
                end
            end,
        }
        "#,
    );
    assert_eq!(rig.text("e"), "hello\nwörld");
    let shown = words(&rig.root);
    assert_eq!(
        shown,
        vec!["v1=1 wörld lines 2 line1 6 of hello\nwörld".to_owned()]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn an_editor_that_gives_its_keys_to_the_page_is_driven_by_the_page() {
    let mut rig = Rig::open(
        r#"
        local keys = 0
        return {
            render = function()
                return pencere.div():size_full()
                    :child(pencere.editor("vim", { keys = "page" }):grow(1))
                    :child(pencere.text("keys " .. keys))
            end,
            handle = function(event)
                if event.type == "key" then
                    keys += 1
                    if event.key == "i" then
                        return { redraw = true, editor = { vim = { command = "insert", text = "x" } } }
                    end
                    return true
                end
            end,
        }
        "#,
    );
    let vim = Id::named("vim");
    rig.editors.set_focus(Some(vim));
    for key in ['i', 'i', 'j'] {
        let route = rig.key(Key::Character(key), Mods::default());
        assert_eq!(route, Route::Page);
        rig.page
            .module
            .event(Event::Input(pencere_view::Input::Key {
                key: Key::Character(key),
                mods: Mods::default(),
            }));
        rig.run();
    }
    assert_eq!(rig.text("vim"), "xx");
    assert_eq!(words(&rig.root), vec!["keys 3".to_owned()]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_change_the_editor_cannot_make_is_refused_and_the_page_told_why() {
    let rig = Rig::open(
        r#"
        local said = "nothing"
        local asked = false
        return {
            render = function()
                return pencere.div():size_full()
                    :child(pencere.editor("e"):grow(1))
                    :child(pencere.text(said))
            end,
            handle = function(event)
                if event.type == "viewport" and not asked then
                    asked = true
                    return { editor = { e = {
                        { change = { { from = 0, to = 0, insert = "x" } }, base = 99 },
                        { change = { { from = 3, to = 1, insert = "" } }, base = 0 },
                    } } }
                elseif event.type == "editor_refused" then
                    said = said == "nothing" and event.reason or (said .. ", " .. event.reason)
                    return true
                end
            end,
        }
        "#,
    );
    assert_eq!(rig.text("e"), "");
    assert_eq!(
        words(&rig.root),
        vec!["future-version, reversed".to_owned()]
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_page_is_told_what_its_search_finds_and_replaces_every_match_in_one_ask() {
    let rig = Rig::open(
        r#"
        local said = {}
        local asked, replaced = false, false
        return {
            render = function()
                return pencere.div():size_full()
                    :child(pencere.editor("e"):grow(1))
                    :child(pencere.text(table.concat(said, " ")))
            end,
            handle = function(event)
                if event.type == "viewport" and not asked then
                    asked = true
                    return { editor = { e = {
                        { load = "çiçek bir çiçekçi, Çiçek" },
                        { command = "find", pattern = "çiçek", word = true, case = false },
                    } } }
                elseif event.type == "found" then
                    table.insert(said, string.format("v%d:%d/%s%s", event.version, event.count,
                        tostring(event.current or "-"), event.more and "+" or ""))
                    if event.count == 2 and not replaced then
                        replaced = true
                        return { redraw = true, editor = { e = { command = "replace_all", text = "gül" } } }
                    end
                    return true
                end
            end,
        }
        "#,
    );
    // Whole words in either case: `çiçekçi` is not one. Every match went in one change, and the
    // search, gone through the document again at once, finds none.
    assert_eq!(rig.text("e"), "gül bir çiçekçi, gül");
    assert_eq!(words(&rig.root), vec!["v1:2/1 v2:0/-".to_owned()]);
    let buffer = rig.editors.buffer(Id::named("e")).expect("the editor");
    assert_eq!(buffer.version(), 2);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_reply_that_hands_an_editor_more_than_a_reply_may_is_not_a_reply() {
    let mut page = open_script(
        r#"
        return {
            render = function() return pencere.editor("e") end,
            handle = function()
                return { editor = { e = { load = string.rep("a", 1024 * 1024 + 1) } } }
            end,
        }
        "#,
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    page.until(Duration::from_secs(20), |u| u.frame.is_some());
    page.module.event(Event::Command(Id::named("anything")));
    let update = page.until(Duration::from_secs(20), |u| u.closed.is_some());
    assert!(
        matches!(&update.closed, Some(Fault::Malformed(why)) if why.contains("bytes of text for editors")),
        "{:?}",
        update.closed
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn how_an_editor_looks_is_the_pages_to_say_and_what_it_leaves_out_is_the_theme() {
    let rig = Rig::open(
        r#"
        return {
            render = function()
                return pencere.div():size_full():child(pencere.editor("vim", {
                    line_height = 1.8,
                    padding = 9,
                    scroll_margin = 3,
                    numbers = "relative",
                    current_line = "all",
                    caret = { shape = "block", height = "text", width = 3, color = "accent", text = { "magenta", 0.5 } },
                    colors = { current_line = false, selection = "raised1", numbers = "dim" },
                }):grow(1))
            end,
        }
        "#,
    );
    let room = rig
        .laid
        .editors()
        .into_iter()
        .find(|room| room.id == Id::named("vim"))
        .expect("the editor's room");
    assert_eq!(
        room.options,
        EditorOptions {
            line_height: 1.8,
            padding: 9.0,
            scroll_margin: 3,
            numbers: Numbers::Relative,
            current_line: LineHighlight::All,
            caret: Caret {
                shape: CaretShape::Block,
                height: CaretHeight::Text,
                width: 3.0,
                color: Tint::Color(Role::Accent.solid()),
                text: Tint::Color(Role::Magenta.alpha(0.5)),
            },
            colors: EditorColors {
                current_line: Tint::Off,
                selection: Tint::Color(Role::Raised(1).solid()),
                numbers: Tint::Color(Role::Dim.solid()),
                ..EditorColors::default()
            },
            ..EditorOptions::default()
        }
    );
    // What a page leaves out is the editor's default: the theme's colours, its rows and caret.
    let plain = Rig::open(r#"return { render = function() return pencere.editor("e") end }"#);
    let room = plain
        .laid
        .editors()
        .into_iter()
        .next()
        .expect("the editor's room");
    assert_eq!(room.options, EditorOptions::default());
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn an_option_an_editor_has_no_such_name_for_is_a_mistake_said_where_it_is_made() {
    for (options, said) in [
        ("{ colours = {} }", "an editor has no option colours"),
        (
            "{ caret = { colour = \"accent\" } }",
            "an editor has no option caret.colour",
        ),
        (
            "{ numbers = \"roman\" }",
            "\"roman\" is not one of the words it takes",
        ),
        ("{ caret = \"block\" }", "the caret of an editor is a table"),
        ("{ colors = { selection = true } }", "colour"),
        (
            "{ indent = { increse = \"then$\" } }",
            "an editor has no option indent.increse",
        ),
        (
            "{ comment = { block = \"/*\" } }",
            "the block comment of an editor is a pair",
        ),
        (
            "{ folds = \"syntax\" }",
            "\"syntax\" is not one of the words it takes",
        ),
        (
            "{ whitespace = \"dots\" }",
            "\"dots\" is not one of the words it takes",
        ),
        (
            "{ colors = { indent_guide = \"red\" } }",
            "an editor has no option colors.indent_guide",
        ),
    ] {
        let script =
            format!("return {{ render = function() return pencere.editor(\"e\", {options}) end }}");
        let mut page = open_script(&script, ModuleLimits::default());
        page.module.render(SIZE, 0.0, false);
        let update = page.until(Duration::from_secs(30), |u| u.closed.is_some());
        let why = failed(&update);
        assert!(why.contains(said), "{options}: {why}");
    }
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_page_says_how_its_language_comments_indents_and_folds_and_the_editor_does_it() {
    let mut rig = Rig::open(
        r#"
        local asked = false
        return {
            render = function()
                return pencere.div():size_full()
                    :child(pencere.editor("lua", {
                        comment = { line = "--", block = { "--[[", "]]" } },
                        indent = { increase = "\\b(then|do|else)\\s*$", decrease = "^\\s*(end|else)\\b" },
                        folds = "indent",
                    }):grow(1))
                    :child(pencere.editor("css", { comment = { block = { open = "/*", close = "*/" } } }):grow(1))
            end,
            handle = function(event)
                if event.type == "viewport" and not asked then
                    asked = true
                    return { editor = { css = { { load = "a { }" }, { command = "toggle_comment" } } } }
                end
            end,
        }
        "#,
    );
    let room = |name: &str| {
        rig.laid
            .editors()
            .into_iter()
            .find(|room| room.id == Id::named(name))
            .expect("the editor's room")
            .options
    };
    assert_eq!(
        room("lua"),
        EditorOptions {
            comment: CommentTokens {
                line: Some("--".into()),
                block: Some(("--[[".into(), "]]".into())),
            },
            indent: IndentRules {
                increase: Some(r"\b(then|do|else)\s*$".into()),
                decrease: Some(r"^\s*(end|else)\b".into()),
            },
            folds: Folds::Indent,
            ..EditorOptions::default()
        }
    );
    assert_eq!(room("css").comment, CommentTokens::block("/*", "*/"));
    // A language of block comments alone comments a line with them.
    assert_eq!(rig.text("css"), "/* a { } */");
    // A line break after `then` indents, and `end` typed goes back out.
    let lua = Id::named("lua");
    rig.editors.set_focus(Some(lua));
    rig.type_text("if x then");
    rig.key(Key::Enter, Mods::default());
    rig.type_text("y()");
    rig.key(Key::Enter, Mods::default());
    rig.type_text("end");
    assert_eq!(rig.text("lua"), "if x then\n    y()\nend");
    // The line with further-in lines after it folds by the indent.
    let buffer = rig.editors.buffer(lua).expect("the editor");
    assert_eq!(buffer.fold_region(0), Some((9, 17)));
    assert_eq!(buffer.fold_lines(0, 2), vec![0]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_rule_of_an_editors_indent_that_is_no_pattern_fails_the_page_by_the_options_name() {
    let mut page = open_script(
        r#"return { render = function()
            return pencere.editor("e", { indent = { increase = "(then" } })
        end }"#,
        ModuleLimits::default(),
    );
    page.module.render(SIZE, 0.0, false);
    let update = page.until(Duration::from_secs(30), |u| u.closed.is_some());
    assert!(
        matches!(&update.closed, Some(Fault::Malformed(why))
            if why.contains("indent.increase") && why.contains("(then") && why.contains("unclosed group")),
        "{:?}",
        update.closed
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_page_says_which_spaces_and_tabs_its_editor_shows_and_whether_its_indent_has_guides() {
    let mut rig = Rig::open(
        r#"
        local asked = false
        return {
            render = function()
                return pencere.div():size_full():child(pencere.editor("code", {
                    gutter = false,
                    whitespace = "boundary",
                    indent_guides = true,
                    colors = { whitespace = "accent", indent_guides = "magenta", active_indent_guide = false },
                }):grow(1))
            end,
            handle = function(event)
                if event.type == "viewport" and not asked then
                    asked = true
                    return { editor = { code = { { load = "if a then\n\tb  c\nend" } } } }
                end
            end,
        }
        "#,
    );
    let code = Id::named("code");
    let room = rig
        .laid
        .editors()
        .into_iter()
        .find(|room| room.id == code)
        .expect("the editor's room");
    assert_eq!(
        room.options,
        EditorOptions {
            gutter: false,
            whitespace: Whitespace::Boundary,
            indent_guides: true,
            colors: EditorColors {
                whitespace: Tint::Color(Role::Accent.solid()),
                indent_guides: Tint::Color(Role::Magenta.solid()),
                active_indent_guide: Tint::Off,
                ..EditorColors::default()
            },
            ..EditorOptions::default()
        }
    );
    assert_eq!(rig.text("code"), "if a then\n\tb  c\nend");
    // The tab and the two spaces together are shown, not a space alone between words; the line
    // further in has a guide.
    rig.editors.set_focus(Some(code));
    let list = rig.laid.paint(&mut Fixed);
    let painted = rig.editors.paint(list, &mut Fixed, true).items;
    let role = |color: Option<pencere_view::Color>| color.and_then(|color| color.role());
    let dots = painted
        .iter()
        .filter(|item| matches!(item, Paint::Quad(quad) if role(quad.fill) == Some(Role::Accent)))
        .count();
    let arrows = painted
        .iter()
        .filter(|item| {
            matches!(item, Paint::Figure(figure)
                if figure.paths.iter().any(|path| role(path.stroke.map(|line| line.color)) == Some(Role::Accent)))
        })
        .count();
    let guides = painted
        .iter()
        .filter(|item| matches!(item, Paint::Quad(quad) if role(quad.fill) == Some(Role::Magenta)))
        .count();
    assert_eq!((dots, arrows, guides), (2, 1, 1));
}

// --- A language of the kit's (`pencere.Syntax`) ---------------------------------------------------

/// A language of C's block comments, strings and numbers, for the tests of `pencere.Syntax`. It
/// counts the lines it reads (`read`) and the most one call of the page read (`most`, with
/// `counted`).
const C_LIKE: &str = r#"
local read, most = 0, 0
local language = pencere.Syntax.define({
    name = "c",
    start = function()
        return { comment = false }
    end,
    token = function(stream, state)
        if stream:sol() then
            read += 1
        end
        if state.comment then
            if stream:skip_to("*/") then
                stream:match("%*/")
                state.comment = false
            else
                stream:skip_to_end()
            end
            return "comment"
        end
        if stream:eat_space() then
            return nil
        end
        if stream:match("/%*") then
            state.comment = true
            return "comment"
        end
        if stream:match('"[^"]*"?') then
            return "string"
        end
        if stream:match("'[^']*'?") then
            return "string.single"
        end
        if stream:match("%d+") then
            return "number"
        end
        local word = stream:match("[%a_][%w_]*")
        if word == "if" then
            return "keyword strong"
        end
        if word == nil then
            stream:next()
        end
        return nil
    end,
    blank = function()
        read += 1
    end,
    styles = {
        comment = { color = "dim", italic = true },
        string = { weight = 600 },
        number = { weight = 700 },
        keyword = { color = "accent" },
        strong = { weight = 800 },
    },
})
local function counted(work)
    local before = read
    work()
    most = math.max(most, read - before)
end
"#;

/// A page of one editor coloured by `C_LIKE`, with the `options` of its syntax, which loads what
/// `document` (the body of a function) returns when it is first in view and shows what the language
/// read. Its commands change the document: `uncomment` and `comment` take the `/*` at the start of
/// line 20 out and put it back, `open` puts one at the start of line 100, `far` shows line 15 000.
/// The syntax goes on by itself (its timer wakes the page): `render` does nothing for it.
fn coloured(document: &str, options: &str) -> String {
    let page = r#"
local syntax = pencere.Syntax.new(language, "e", OPTIONS)
-- Each call's reading counted, the calls of the syntax's own timer as well.
local work = syntax.work
function syntax:work()
    counted(function()
        work(self)
    end)
end
local loaded = false
local function document()
    DOCUMENT
end
local function change(line, take, insert)
    local at = pencere.layout.line_start("e", line)
    return { change = { { from = at, to = at + take, insert = insert } }, base = pencere.layout.editor_version("e") }
end
return {
    render = function()
        return pencere.div():size_full()
            :command("uncomment", "Uncomment")
            :command("comment", "Comment")
            :command("open", "Open a comment")
            :command("far", "Go far down")
            :command("nothing", "Do nothing")
            :child(pencere.editor("e", { gutter = false }):grow(1))
            :child(pencere.text(string.format("read %d most %d", read, most)))
    end,
    handle = function(event)
        local ask = nil
        if event.type == "viewport" and not loaded then
            loaded = true
            ask = { load = document() }
        elseif event.type == "command" then
            if event.id == "uncomment" then
                ask = change(20, 2, "")
            elseif event.id == "comment" then
                ask = change(20, 0, "/*")
            elseif event.id == "open" then
                ask = change(100, 0, "/*")
            elseif event.id == "far" then
                ask = { reveal = pencere.layout.line_start("e", 15000), version = pencere.layout.editor_version("e") }
            end
        end
        syntax:handle(event)
        return syntax:reply(ask and { editor = { e = ask } })
    end,
}
"#;
    format!(
        "{C_LIKE}{}",
        page.replace("OPTIONS", options)
            .replace("DOCUMENT", document)
    )
}

/// The lines the language read, and the most one call read, as the page shows them.
fn read(rig: &Rig) -> (usize, usize) {
    let shown = words(&rig.root);
    let line = shown
        .iter()
        .find(|words| words.starts_with("read "))
        .expect("the page shows what it read");
    let numbers: Vec<usize> = line
        .split(' ')
        .filter_map(|word| word.parse().ok())
        .collect();
    (numbers[0], numbers[1])
}

/// What the `syntax` layer gives line `line` of the editor: each look from where to where in the
/// line (characters), by the style of `C_LIKE` it is.
fn looks(rig: &Rig, line: usize) -> Vec<(usize, usize, &'static str)> {
    let buffer = rig.editors.buffer(Id::named("e")).expect("the editor");
    let Some(layer) = buffer
        .layer_names()
        .iter()
        .position(|name| name == "syntax")
    else {
        return Vec::new();
    };
    let start = buffer.line_start(line);
    buffer
        .decorations_in(start, buffer.line_end(line))
        .into_iter()
        .filter(|(index, _)| *index == layer)
        .map(|(_, decoration)| {
            let Mark::Style(look) = &decoration.mark else {
                panic!("a syntax colours, it does not mark: {decoration:?}");
            };
            let style = match (look.italic, look.weight) {
                (Some(true), _) => "comment",
                (_, Some(600)) => "string",
                (_, Some(700)) => "number",
                (_, Some(800)) if look.color.is_some() => "keyword strong",
                _ => "other",
            };
            (decoration.from - start, decoration.to - start, style)
        })
        .collect()
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_language_colours_a_block_comment_over_its_lines_and_only_the_lines_in_view() {
    let mut rig = Rig::open(&coloured(
        r#"local lines = {}
        for i = 1, 3000 do
            lines[i] = string.format("v = %05d", i)
        end
        lines[3] = "a /* one"
        lines[4] = "two"
        lines[5] = "three */ 12"
        lines[6] = "if 'it' \"is\""
        return table.concat(lines, "\n")"#,
        "{}",
    ));
    rig.wakes();
    rig.frame();
    assert_eq!(looks(&rig, 1), vec![(4, 9, "number")]);
    assert_eq!(looks(&rig, 2), vec![(2, 8, "comment")]);
    assert_eq!(looks(&rig, 3), vec![(0, 3, "comment")]);
    assert_eq!(
        looks(&rig, 4),
        vec![(0, 8, "comment"), (9, 11, "number")],
        "the comment ends where it says"
    );
    // A token of two names has both looks; a dotted name the look of what is before its dot.
    assert_eq!(
        looks(&rig, 5),
        vec![
            (0, 2, "keyword strong"),
            (3, 7, "string"),
            (8, 12, "string")
        ]
    );
    assert_eq!(looks(&rig, 6), vec![(4, 9, "number")]);
    assert!(
        looks(&rig, 2500).is_empty(),
        "a line far out of view is not coloured"
    );
    let (lines, most) = read(&rig);
    assert!(
        lines < 200 && most < 200,
        "only what is in view is read: {lines}, {most}"
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn an_edit_reads_the_lines_again_only_until_their_state_is_what_it_was() {
    let mut rig = Rig::open(&coloured(
        r#"local lines = {}
        for i = 1, 2000 do
            lines[i] = string.format("v = %05d", i)
        end
        lines[21] = "/* open"
        lines[26] = "close */"
        return table.concat(lines, "\n")"#,
        "{}",
    ));
    rig.wakes();
    rig.frame();
    let (before, _) = read(&rig);
    assert_eq!(looks(&rig, 22), vec![(0, 9, "comment")]);
    // Without its `/*`, lines 20 to 25 are read again: the state after line 25 is what it was, and
    // the lines after it are not read.
    rig.command("uncomment");
    rig.wakes();
    rig.frame();
    let (after, _) = read(&rig);
    assert_eq!(after - before, 6);
    assert_eq!(looks(&rig, 22), vec![(4, 9, "number")]);
    assert_eq!(looks(&rig, 26), vec![(4, 9, "number")]);
    rig.command("comment");
    rig.wakes();
    rig.frame();
    assert_eq!(read(&rig).0 - after, 6);
    assert_eq!(looks(&rig, 22), vec![(0, 9, "comment")]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_language_colours_characters_where_a_line_has_characters_of_many_bytes() {
    let mut rig = Rig::open(&coloured(r#"return "çay \"şeker\" 12\né /* ğ */ x""#, "{}"));
    rig.wakes();
    rig.frame();
    assert_eq!(looks(&rig, 0), vec![(4, 11, "string"), (12, 14, "number")]);
    assert_eq!(looks(&rig, 1), vec![(2, 9, "comment")]);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_long_document_is_coloured_to_its_end_a_step_a_wake_with_no_event_of_the_persons() {
    let mut rig = Rig::open(&coloured(
        r#"local lines = {}
        for i = 1, 20000 do
            lines[i] = string.format("v = %05d", i)
        end
        return table.concat(lines, "\n")"#,
        "{ step = 20000 }",
    ));
    rig.wakes();
    assert_eq!(looks(&rig, 1), vec![(4, 9, "number")]);
    // From line 100 on, everything is a comment; far down, out of view, it is not read yet.
    rig.command("open");
    rig.command("far");
    // The editor scrolls there when the host lays the page out next.
    rig.frame();
    // The lines in view are coloured at once, from a guess, and the reading goes on by itself:
    // one call reads no more than its step (20 000 bytes, of lines of 10).
    assert_eq!(looks(&rig, 15000), vec![(4, 9, "number")]);
    assert!(
        rig.wake_due.is_some(),
        "the page asks to be woken while there is more to read"
    );
    // Nothing of the person's comes now: only the wakes the page asks for, a frame apart, each
    // a call's reading, what it found handed to the editor with the wake's reply.
    let told = rig.told.len();
    let woken = rig.wakes();
    assert_eq!(
        rig.told.len(),
        told,
        "the page was told nothing but its wakes"
    );
    assert!(woken >= 5, "the reading went on over {woken} wakes");
    assert_eq!(rig.wake_due, None, "the reading came to an end");
    assert_eq!(
        looks(&rig, 15000),
        vec![(0, 9, "comment")],
        "what the guess got wrong is coloured again"
    );
    // The lines out of view are not coloured any more.
    assert!(looks(&rig, 1).is_empty());
    rig.frame();
    let (_, most) = read(&rig);
    assert!(most <= 2001, "one call read {most} lines");
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_syntax_reads_nothing_for_an_event_that_is_not_its_editors_edit_or_view() {
    let mut rig = Rig::open(&coloured(
        r#"local lines = {}
        for i = 1, 20000 do
            lines[i] = string.format("v = %05d", i)
        end
        return table.concat(lines, "\n")"#,
        "{ step = 2000 }",
    ));
    rig.wakes();
    // Far down, the reading toward the lines in view goes on for many wakes.
    rig.command("far");
    // Laid out, the editor scrolls there and tells the view; drawn again, the page shows what was
    // read for it.
    rig.frame();
    rig.frame();
    assert!(rig.wake_due.is_some(), "there is more to read");
    let (before, _) = read(&rig);
    let told = rig.told.len();
    // The pointer moves over the page, and the page gives the syntax the event: it is no edit and
    // no view of its editor, and it reads nothing for it.
    for x in 0..20 {
        rig.page.module.event(Event::Input(Input::Move(Point {
            x: 10.0 + x as f32,
            y: 40.0,
        })));
        rig.run();
    }
    rig.frame();
    assert_eq!(rig.told.len(), told, "the editor told the page nothing");
    assert_eq!(read(&rig).0, before, "a pointer's moves read no line");
    // What is left is its timer's.
    assert!(rig.wake());
    rig.frame();
    assert!(read(&rig).0 > before);
}

/// A language of letters and what is not, which gives every character of `a1a1…` a decoration of its
/// own: a line of 10 000 characters is 10 000 decorations, half what one reply may hand the editors.
const DENSE: &str = r#"
local dense = pencere.Syntax.define({
    name = "dense",
    start = function()
        return {}
    end,
    token = function(stream)
        if stream:match("%a") then
            return "letter"
        end
        stream:next()
        return "other"
    end,
    styles = { letter = { color = "accent" }, other = { color = "warning" } },
})
"#;

/// How many decorations the layer `layer` of the editor `editor` has on line `line`.
fn decorated(rig: &Rig, editor: &str, layer: &str, line: usize) -> usize {
    let buffer = rig.editors.buffer(Id::named(editor)).expect("the editor");
    let Some(index) = buffer.layer_names().iter().position(|name| name == layer) else {
        return 0;
    };
    let start = buffer.line_start(line);
    buffer
        .decorations_in(start, buffer.line_end(line))
        .into_iter()
        .filter(|(at, decoration)| *at == index && decoration.from >= start)
        .count()
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn two_syntaxes_and_the_page_that_ask_more_than_a_reply_may_at_one_wake_hand_it_over_two() {
    let mut rig = Rig::open(&format!(
        "{DENSE}{}",
        r#"
local a = pencere.Syntax.new(dense, "a")
local b = pencere.Syntax.new(dense, "b")
local loaded, marked = {}, false
local line = string.rep("a1", 5000)
local document = table.concat({ line, line, line, line }, "\n")
-- The page's own decorations of `a`'s first ten characters, in the layer `layer`.
local function marks(layer)
    local made = {}
    for k = 0, 9 do
        made[#made + 1] = { from = k, to = k + 1, background = "accent" }
    end
    return { editor = { a = { decorate = layer, from = 0, to = 10,
        version = pencere.layout.editor_version("a"), decorations = made } } }
end
return {
    render = function()
        return pencere.div():size_full()
            :child(pencere.editor("a", { gutter = false }):grow(1))
            :child(pencere.editor("b", { gutter = false }):grow(1))
    end,
    handle = function(event)
        local ask = nil
        if event.type == "viewport" and not loaded[event.editor] then
            loaded[event.editor] = true
            ask = { editor = { [event.editor] = { load = document } } }
        end
        a:handle(event)
        b:handle(event)
        if event.type == "edit" and event.editor == "b" and not marked then
            marked = true
            -- The page's own, with the line `b` colours now, and at the wake both syntaxes go on
            -- at with a line each: there, 10 more than the 20 000 a reply may hand the editors.
            ask = marks("marks")
            pencere.time.after(0, function()
                return marks("more")
            end)
        end
        return a:reply(b:reply(ask))
    end,
}
"#
    ));
    rig.wakes();
    assert!(rig.most <= 20_000, "a reply handed {}", rig.most);
    for editor in ["a", "b"] {
        for line in 0..4 {
            assert_eq!(
                decorated(&rig, editor, "syntax", line),
                10_000,
                "{editor}, line {line}"
            );
        }
    }
    assert_eq!(decorated(&rig, "a", "marks", 0), 10);
    assert_eq!(decorated(&rig, "a", "more", 0), 10);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_syntax_holds_at_most_half_a_reply_however_many_decorations_a_line_has() {
    let mut rig = Rig::open(&format!(
        "{DENSE}{}",
        r#"
local syntax = pencere.Syntax.new(dense, "e")
local loaded = false
return {
    render = function()
        return pencere.editor("e", { gutter = false })
    end,
    handle = function(event)
        local ask = nil
        if event.type == "viewport" and not loaded then
            loaded = true
            -- Half a line, then lines of 10 000 decorations: the second does not go with the first.
            ask = { editor = { e = { load = string.rep("a1", 2500) .. "\n" .. string.rep("a1", 5000)
                .. "\n" .. string.rep("a1", 5000) } } }
        end
        syntax:handle(event)
        return syntax:reply(ask)
    end,
}
"#
    ));
    rig.wakes();
    assert!(rig.most <= 10_000, "a reply handed {}", rig.most);
    assert_eq!(decorated(&rig, "e", "syntax", 0), 5_000);
    assert_eq!(decorated(&rig, "e", "syntax", 1), 10_000);
    assert_eq!(decorated(&rig, "e", "syntax", 2), 10_000);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_token_that_reads_nothing_fails_the_page_with_its_languages_name_and_its_line() {
    let why = Rig::failing(
        r#"
        local language = pencere.Syntax.define({
            name = "stuck",
            start = function() return {} end,
            token = function(stream)
                if stream:peek() == "!" then
                    return "x"
                end
                stream:next()
            end,
        })
        local syntax = pencere.Syntax.new(language, "e")
        local loaded = false
        return {
            render = function()
                return pencere.editor("e", { gutter = false })
            end,
            handle = function(event)
                local reply = nil
                if event.type == "viewport" and not loaded then
                    loaded = true
                    reply = { editor = { e = { load = "fine\nnot!" } } }
                end
                syntax:handle(event)
                return syntax:reply(reply)
            end,
        }"#,
    );
    assert!(
        why.contains("stuck: line 2: token read nothing at column 4"),
        "{why}"
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_language_reads_a_line_from_a_stream_as_codemirrors_does() {
    let rig = Rig::open(
        r#"
        local seen = {}
        local language = pencere.Syntax.define({
            name = "probe",
            start = function() return { n = 0 } end,
            token = function(stream, state)
                state.n += 1
                if stream:sol() then
                    seen[#seen + 1] = stream:indentation()
                    stream:eat_space()
                    seen[#seen + 1] = stream:sol()
                    -- A pattern is read where the stream is, not further on.
                    seen[#seen + 1] = stream:match("b") or "no b"
                    seen[#seen + 1] = stream:match("(%a+)=(%d+)", false)
                    seen[#seen + 1] = stream:match("KEY", false, true)
                    seen[#seen + 1] = stream:match("[j-l]+", false, true)
                    seen[#seen + 1] = stream:eat(".") or "no dot"
                    seen[#seen + 1] = stream:eat("%a")
                    stream:backup(1)
                    stream:eat_while("[%w=]")
                    seen[#seen + 1] = stream:current()
                    stream:eat_space()
                    stream.start = stream.pos
                    seen[#seen + 1] = stream:column()
                    seen[#seen + 1] = stream:next()
                    seen[#seen + 1] = stream:skip_to("b") and stream.pos or "no b"
                    return "word"
                end
                stream:skip_to_end()
                return "rest"
            end,
        })
        local tokens, after = language:tokens("\t  Key=42 é ab")
        local said = {}
        for _, value in ipairs(seen) do
            said[#said + 1] = tostring(value)
        end
        for _, token in ipairs(tokens) do
            said[#said + 1] = token.name .. " " .. token.from .. "-" .. token.to
        end
        said[#said + 1] = "states " .. after.n
        return {
            render = function()
                return pencere.text(table.concat(said, ", "))
            end,
        }"#,
    );
    assert_eq!(
        words(&rig.root),
        vec![
            "6, false, no b, Key, Key, K, no dot, K, \t  Key=42, 13, é, 15, word 0-13, rest 13-14, \
             states 2"
                .to_owned()
        ]
    );
}

// --- Wakes and the kit's timers (`pencere.time`) -------------------------------------------------

/// The words of the page that start with `head`, the rest of the line.
fn shown(rig: &Rig, head: &str) -> String {
    words(&rig.root)
        .into_iter()
        .find_map(|line| line.strip_prefix(head).map(str::to_owned))
        .unwrap_or_else(|| panic!("the page shows no {head:?}: {:?}", words(&rig.root)))
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn timers_run_in_the_order_of_their_times_a_stopped_one_never_and_what_they_return_is_one_reply() {
    let mut rig = Rig::open(
        r#"
local ran = {}
local told = {}
local stopped = nil
return {
    render = function()
        return pencere.div():size_full():command("start", "Start")
            :child(pencere.editor("e", { gutter = false }):grow(1))
            :child(pencere.text("ran " .. table.concat(ran, " ")))
            :child(pencere.text("told " .. table.concat(told, " ")))
    end,
    handle = function(event)
        told[#told + 1] = event.type
        if event.type == "command" and event.id == "start" then
            pencere.time.after(0.3, function()
                ran[#ran + 1] = "c"
                return true
            end)
            pencere.time.after(0.1, function()
                ran[#ran + 1] = "a"
                return { editor = { e = { load = "from a timer", more = true } } }
            end)
            stopped = pencere.time.after(0.2, function()
                ran[#ran + 1] = "never"
                return true
            end)
            -- At the time of the first: run after it, in the order they were made.
            pencere.time.after(0.1, function()
                ran[#ran + 1] = "b"
                ran[#ran + 1] = tostring(stopped:cancel()) .. "/" .. tostring(stopped:cancel())
                    .. "/" .. tostring(stopped:active())
                return { redraw = true, editor = { e = { load = ", and another" } } }
            end)
        end
    end,
}
"#,
    );
    assert_eq!(
        rig.wake_due, None,
        "a page with no timer asks to be woken for nothing"
    );
    rig.command("start");
    // Set where the time was not known: the kit asks to be woken at once, to count from then.
    assert_eq!(rig.wake_due, Some(rig.now));
    let woken = rig.wakes();
    assert_eq!(woken, 3, "placed, then a and b together, then c");
    assert_eq!(shown(&rig, "ran "), "a b true/false/false c");
    // What two timers returned at one wake went to the editor as one reply, in their order.
    assert_eq!(rig.text("e"), "from a timer, and another");
    // The page's own `handle` never heard of the wakes its timers took.
    assert!(
        !shown(&rig, "told ").contains("wake"),
        "{}",
        shown(&rig, "told ")
    );
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_timer_set_as_the_script_starts_ticks_with_no_event_and_the_page_hears_only_its_own_wake() {
    let mut rig = Rig::open(
        r#"
local ticks = 0
local own = {}
local clock
clock = pencere.time.every(0.5, function()
    ticks += 1
    if ticks == 3 then
        -- The page's own wake, asked from a timer: its `handle` is told of it.
        return { wake = 0.25 }
    end
    return true
end)
return {
    render = function()
        return pencere.div()
            :child(pencere.text("ticks " .. ticks))
            :child(pencere.text("own " .. table.concat(own, " ")))
    end,
    handle = function(event)
        if event.type == "wake" then
            own[#own + 1] = event.now == pencere.time.now() and "now" or "not now"
            clock:cancel()
            return true
        end
    end,
}
"#,
    );
    // The first page asked for it: nothing else could, for nothing of the person's comes.
    assert_eq!(rig.wake_due, Some(0.0));
    assert!(rig.wake(), "placed");
    assert_eq!(shown(&rig, "ticks "), "0");
    assert!(rig.wake());
    assert_eq!(shown(&rig, "ticks "), "1");
    // A host that held the page's wakes (the view was out of sight) and wakes it four periods
    // late: the timer runs once, not once for each run it fell behind on.
    rig.at(rig.now + 2.0);
    assert!(rig.wake());
    assert_eq!(shown(&rig, "ticks "), "2");
    assert!(rig.wake());
    assert_eq!(shown(&rig, "ticks "), "3");
    assert_eq!(shown(&rig, "own "), "");
    // The page's own wake: its `handle` is told, with the time it is given as `pencere.time.now()`.
    assert!(rig.wake());
    assert_eq!(shown(&rig, "own "), "now");
    assert_eq!(
        rig.wake_due, None,
        "the clock was stopped and nothing is asked"
    );
    assert!(!rig.wake());
    assert_eq!(shown(&rig, "ticks "), "3");
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_page_that_sets_no_timer_and_asks_no_wake_is_asked_nothing_new_and_told_nothing_new() {
    let mut rig = Rig::open(
        r#"
local told = {}
return {
    render = function(size, now)
        return pencere.div():size_full():command("go", "Go")
            :child(pencere.editor("e", { gutter = false }):grow(1))
            :child(pencere.text("told " .. table.concat(told, " ")))
            :child(pencere.text(string.format("now %s", tostring(pencere.time.now() == now))))
    end,
    handle = function(event)
        told[#told + 1] = event.type
        return true
    end,
}
"#,
    );
    rig.type_text("plain words");
    rig.command("go");
    rig.frame();
    assert_eq!(rig.wake_due, None);
    assert!(!rig.wake());
    assert!(!shown(&rig, "told ").contains("wake"));
    assert!(shown(&rig, "told ").contains("command"));
    assert_eq!(shown(&rig, "now "), "true");
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_timer_set_as_the_page_settles_is_asked_for_with_no_event() {
    let mut rig = Rig::open(
        r#"
local set, ran = false, false
return {
    render = function()
        return pencere.text(ran and "ran" or "waits")
    end,
    settle = function()
        if not set then
            set = true
            pencere.time.after(0, function()
                ran = true
                return true
            end)
        end
        return false
    end,
}
"#,
    );
    // A settle has no reply: the kit asks for the page again, and its frame asks to be woken.
    assert!(rig.wake_due.is_some(), "the timer is asked for");
    assert_eq!(words(&rig.root), vec!["waits".to_owned()]);
    assert!(rig.wake());
    assert_eq!(words(&rig.root), vec!["ran".to_owned()]);
    assert_eq!(rig.wake_due, None);
}

#[test]
#[ignore = "heavy: compiles the interpreter or a module and drives it for seconds; make check-heavy"]
fn a_timer_that_fails_fails_the_page_at_its_line_and_a_wake_that_is_no_time_is_a_mistake() {
    // An error in a timer is the page's, said with its line.
    let mut rig = Rig::start(
        "pencere.time.after(0.1, function()\n    error(\"the timer broke\")\nend)\nreturn { render = function() return pencere.div() end }",
        true,
    );
    rig.wakes();
    let Some(Fault::Failed(why)) = &rig.failure() else {
        panic!("the page failed: {:?}", rig.fault);
    };
    assert!(why.starts_with("script.lua:2: the timer broke"), "{why}");
    // Set with what is not a time, a timer is a mistake where it is set.
    for (set, what) in [
        (
            "pencere.time.after(-1, print)",
            "pencere.time.after takes a number of seconds",
        ),
        (
            "pencere.time.every(0, print)",
            "the time between two runs is more than 0 seconds",
        ),
        ("pencere.time.after(1, 2)", "the function to run"),
    ] {
        let why = Rig::failing(&format!(
            "local x = 1\n{set}\nreturn {{ render = function() return pencere.div() end }}"
        ));
        assert!(why.starts_with("script.lua:2:"), "{set}: {why}");
        assert!(why.contains(what), "{set}: {why}");
    }
    // A reply's wake that is not a number of seconds fails the page, as a reply's mistakes do.
    for wake in ["-1", "\"soon\"", "0/0"] {
        let mut rig = Rig::start(
            &format!(
                r#"return {{
    render = function() return pencere.div():command("go", "Go") end,
    handle = function(event) if event.type == "command" then return {{ wake = {wake} }} end end,
}}"#
            ),
            true,
        );
        rig.command("go");
        let Some(Fault::Failed(why)) = &rig.failure() else {
            panic!("a wake of {wake} failed the page: {:?}", rig.fault);
        };
        assert!(
            why.contains("wake is a number of seconds, 0 or more"),
            "{why}"
        );
    }
}
