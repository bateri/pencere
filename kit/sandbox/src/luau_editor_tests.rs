//! A Lua page with an editor, driven as a host drives one: the page laid out with the host's editors
//! (`pencere-editor`), what they tell sent to the page with the text it reads posted before it, what
//! the page asks of them done. The page is the example's (`examples/editor-lua`), and some short
//! scripts besides. Heavy: the interpreter is compiled and run (`make check-heavy`).

use std::sync::Arc;
use std::time::Duration;

use pencere_editor::{EditorEvent, Editors, Route};
use pencere_view::edit::{
    Caret, CaretHeight, CaretShape, EditorColors, EditorOptions, LineHighlight, Mark, Numbers,
    Origin, TextSnapshot, Tint,
};
use pencere_view::{
    Element, Event, Font, Id, Key, Keyboard, Laid, Measure, Metrics, Mods, Role, Scrolls, layout,
};

use crate::guests_tests::{Open, SIZE};
use crate::luau_tests::{failed, open_script, words};
use crate::{Fault, ModuleLimits};

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

/// A host of one page: the module, its layout, its scrolling and its editors.
struct Rig {
    page: Open,
    editors: Editors,
    scrolls: Scrolls,
    laid: Arc<Laid>,
    root: Element,
    now: f64,
    /// Everything the editors told the page, in order.
    told: Vec<EditorEvent>,
    /// What the module said to its program.
    sent: Vec<String>,
}

impl Rig {
    fn open(script: &str) -> Self {
        let mut rig = Self {
            page: open_script(script, ModuleLimits::default()),
            editors: Editors::new(),
            scrolls: Scrolls::new(),
            laid: Arc::new(layout(pencere_view::div(), SIZE, 1.0, &mut Fixed)),
            root: pencere_view::div(),
            now: 0.0,
            told: Vec::new(),
            sent: Vec::new(),
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
        if let Some(fault) = &update.closed {
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
            relay = true;
            busy = true;
        }
        for answer in update.replies {
            busy = true;
            redraw |= answer.reply.redraw;
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
            self.now += 0.1;
            self.page.module.render(SIZE, self.now, false);
        }
        busy || redraw
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
