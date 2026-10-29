//! The editors of a page, as a host keeps them: laid out in their rooms, under the pointer and the
//! keys, scrolled as a box is, and drawn in their places. The host's measure is a stand-in whose
//! letters are of three widths (`i` narrow, `m` wide), so that what is across is not a column.

// A test fails by panicking, and indexes what it has just built.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use pencere_editor::{EditorEvent, Editors, Route};
use pencere_view::edit::{
    Caret, CaretHeight, CaretShape, Change, Column, ColumnEvents, Decorate, Decoration,
    EditCommand, EditorAsk, EditorColors, EditorEvents, EditorOptions, Folds, Found, Gesture,
    GutterKind, GutterMark, GutterShape, HoverKind, KeyAction, Keybinding, Keys, LineHighlight,
    Look, Mark, Numbers, Pressing, Preview, Range, RulerLane, RulerMark, TargetKind, TargetSource,
    Tint, Whitespace, Widget, WidgetPart, WidgetSide, Zone, ZoneSide,
};
use pencere_view::{
    Button, Command, Cursor, DrawList, Element, Font, Id, Key, Laid, Measure, Metrics, Mods, Paint,
    Point, Rect, Role, Scrolls, Shortcut, Side, Size, div, layout,
};

const EDITOR: Id = Id::new(7);
const POPUP: Id = Id::new(8);
const RUN: Id = Id::new(9);

/// A line of text is 16 high (`Fake`); a row keeps 2 above it and 2 below.
const LINE: f32 = 16.0;
const LEAD: f32 = 2.0;
const ROW: f32 = LINE + 2.0 * LEAD;

/// `i` and `l` are 3 pixels, `m` and `w` 11, a space and every other character 7; a line is 16 high.
struct Fake;

fn width_of(c: char) -> f32 {
    match c {
        'i' | 'l' => 3.0,
        'm' | 'w' => 11.0,
        _ => 7.0,
    }
}

impl Measure for Fake {
    fn measure(&mut self, _font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().map(width_of).sum(),
            height: 16.0,
        }
    }
}

/// A host of one page: its layout, its scrolling and its editors, done as a host does them each
/// frame.
struct Host {
    editors: Editors,
    scrolls: Scrolls,
    laid: Laid,
    page: Box<dyn Fn() -> Element>,
    now: f64,
}

impl Host {
    fn new(page: impl Fn() -> Element + 'static) -> Self {
        let mut host = Self {
            editors: Editors::new(),
            scrolls: Scrolls::new(),
            laid: layout(div(), Size::new(1.0, 1.0), 1.0, &mut Fake),
            page: Box::new(page),
            now: 0.0,
        };
        // Twice: the first lays the editor out, the second lays the page out with what it said.
        host.frame();
        host.frame();
        host
    }

    /// The page laid out again, scrolled where its boxes are, and the editors with it.
    fn frame(&mut self) {
        let mut root = (self.page)();
        self.scrolls.apply(&mut root);
        let mut measure = Fake;
        let mut laid = layout(
            root,
            Size::new(400.0, 200.0),
            1.0,
            &mut self.editors.measure(&mut measure),
        );
        self.scrolls.place(&mut laid, self.now, false);
        self.editors.sync(&laid, &mut Fake);
        self.laid = laid;
    }

    /// What the editors asked of the scrolling done, and the page laid out again where it asks.
    fn settle(&mut self) {
        for ask in self.editors.take_scrolls() {
            self.scrolls.ask(ask);
        }
        self.now += 0.1;
        self.frame();
        // The tree again where a caret or a range an overlay stands by moved, as a host does.
        if self.editors.wants_layout() || self.laid.wants_layout() {
            self.frame();
        }
    }

    fn load(&mut self, text: &str) {
        let laid = self.laid.clone();
        self.editors.ask(
            &laid,
            EDITOR,
            EditorAsk::Load {
                text: text.into(),
                more: false,
            },
            &mut Fake,
            self.now,
        );
        self.settle();
    }

    fn key(&mut self, key: Key, mods: Mods) -> Route {
        let laid = self.laid.clone();
        let route = self.editors.key(&laid, key, mods, &mut Fake, self.now);
        self.settle();
        route
    }

    fn press(&mut self, at: Point, mods: Mods, count: u32) -> bool {
        let laid = self.laid.clone();
        let took = self.editors.press(&laid, at, mods, count, &mut Fake);
        self.settle();
        took
    }

    fn drag(&mut self, at: Point) {
        let laid = self.laid.clone();
        self.editors.drag_to(&laid, at, &mut Fake);
        self.settle();
    }

    fn ranges(&self) -> Vec<Range> {
        self.editors
            .buffer(EDITOR)
            .expect("the editor")
            .ranges()
            .to_vec()
    }

    fn text(&self) -> String {
        self.editors.buffer(EDITOR).expect("the editor").text()
    }

    fn paint(&mut self, caret_on: bool) -> DrawList {
        let list = self.laid.paint(&mut Fake);
        self.editors.paint(list, &mut Fake, caret_on)
    }

    /// The main caret put at `at`, the editor having the keyboard.
    fn caret_at(&mut self, at: usize) {
        self.editors.set_focus(Some(EDITOR));
        let laid = self.laid.clone();
        let version = self.editors.buffer(EDITOR).expect("the editor").version();
        self.editors.ask(
            &laid,
            EDITOR,
            EditorAsk::Select {
                version,
                ranges: vec![Range::caret(at)],
                main: 0,
            },
            &mut Fake,
            self.now,
        );
        self.settle();
    }

    /// The texts drawn, where and in which role's colour.
    fn texts(&mut self) -> Vec<(String, Point, Option<Role>)> {
        self.paint(true)
            .items
            .iter()
            .filter_map(|item| match item {
                Paint::Text(run) => Some((run.text.clone(), run.origin, run.color.role())),
                _ => None,
            })
            .collect()
    }

    /// The boxes filled, and the role of their colour.
    fn quads(&mut self) -> Vec<(Rect, Option<Role>)> {
        self.paint(true)
            .items
            .iter()
            .filter_map(|item| match item {
                Paint::Quad(quad) => Some((quad.rect, quad.fill.and_then(|f| f.role()))),
                _ => None,
            })
            .collect()
    }
}

fn plain() -> EditorOptions {
    EditorOptions {
        gutter: false,
        ..EditorOptions::default()
    }
}

/// An editor filling the page, 400 by 200, with no gutter: its text starts 4 pixels in and down.
fn page_with(options: EditorOptions) -> impl Fn() -> Element {
    move || div().child(div().editor(EDITOR, options.clone()).size_full())
}

fn none() -> Mods {
    Mods::default()
}

fn with(f: impl FnOnce(&mut Mods)) -> Mods {
    let mut mods = Mods::default();
    f(&mut mods);
    mods
}

#[test]
fn a_click_puts_the_caret_at_the_nearest_boundary_across_and_a_drag_selects() {
    let mut host = Host::new(page_with(plain()));
    host.load("mmm iii\nwww");
    assert!(host.editors.focused().is_none());
    // `mmm` ends at 4 + 33; a click at 32 is nearer it than the boundary before the third `m`…
    assert!(host.press(Point::new(32.0, 10.0), none(), 1));
    assert_eq!(host.editors.focused(), Some(EDITOR));
    assert_eq!(host.ranges(), vec![Range::caret(3)]);
    assert!(
        host.editors
            .take_events()
            .contains(&EditorEvent::Focus(Some(EDITOR)))
    );
    // …and one on the second line, a row down.
    host.drag(Point::new(16.0, 4.0 + ROW));
    assert_eq!(host.ranges(), vec![Range::new(3, 9)]);
    assert!(host.editors.release());
    // Two clicks select the word, three the line.
    host.press(Point::new(50.0, 10.0), none(), 2);
    assert_eq!(host.ranges(), vec![Range::new(4, 7)]);
    host.press(Point::new(50.0, 10.0), none(), 3);
    assert_eq!(host.ranges(), vec![Range::new(0, 8)]);
    host.editors.release();
    // Shift extends from where the caret was.
    host.press(Point::new(4.0, 10.0), none(), 1);
    host.editors.release();
    host.press(Point::new(16.0, 4.0 + ROW), with(|m| m.shift = true), 1);
    assert_eq!(host.ranges(), vec![Range::new(0, 9)]);
}

#[test]
fn a_move_down_keeps_the_place_across_and_not_the_column() {
    let mut host = Host::new(page_with(plain()));
    // `www` is 33 across; on `iiiiiiiiiii` (3 each) the nearest place to 33 is the eleventh.
    host.load("www\niiiiiiiiiiii\nwww");
    host.editors.set_focus(Some(EDITOR));
    let laid = host.laid.clone();
    host.editors.ask(
        &laid,
        EDITOR,
        EditorAsk::Select {
            version: 1,
            ranges: vec![Range::caret(3)],
            main: 0,
        },
        &mut Fake,
        0.0,
    );
    host.key(Key::Down, none());
    assert_eq!(host.ranges(), vec![Range::caret(4 + 11)]);
    host.key(Key::Down, none());
    assert_eq!(host.ranges(), vec![Range::caret(17 + 3)]);
    // Back up, it goes where it started across, not where the short line put it.
    host.key(Key::Up, none());
    host.key(Key::Up, none());
    assert_eq!(host.ranges(), vec![Range::caret(3)]);
}

#[test]
fn a_column_is_selected_by_the_places_across_on_each_line() {
    let mut host = Host::new(page_with(plain()));
    host.load("abcdef\nmmmmmm\niiiiii");
    // From 4 + 14 across on the first line (after `ab`) to 4 + 28 on the third.
    host.press(Point::new(18.0, 10.0), with(|m| m.alt = true), 1);
    host.drag(Point::new(32.0, 4.0 + 2.0 * ROW + 6.0));
    // `ab|cd|ef`: 14..28 is `cd`; `mmmmmm` (11 each): nearest 14 is 1, nearest 28 is 3 (33 is
    // nearer than 22); `iiiiii` (3 each): nearest 14 is the fifth (15), and 28 is past its end.
    assert_eq!(
        host.ranges(),
        vec![Range::new(2, 4), Range::new(8, 10), Range::new(19, 20)]
    );
}

#[test]
fn a_long_line_wraps_where_the_room_ends_after_a_space() {
    let options = EditorOptions {
        wrap: true,
        ..plain()
    };
    let mut host =
        Host::new(move || div().child(div().editor(EDITOR, options.clone()).size(100.0, 200.0)));
    // The room is 100 wide: 8 of text before and after leaves 92, thirteen characters of 7.
    host.load("aaaa bbbb cccc dddd");
    let metrics = host.editors.metrics(EDITOR).expect("laid out");
    // Two rows: "aaaa bbbb " and "cccc dddd" (the space stays at the end of the row it ends).
    assert_eq!(metrics.content, Size::new(100.0, 2.0 * ROW + 8.0));
    host.editors.set_focus(Some(EDITOR));
    let laid = host.laid.clone();
    host.editors.ask(
        &laid,
        EDITOR,
        EditorAsk::Select {
            version: 1,
            ranges: vec![Range::caret(12)],
            main: 0,
        },
        &mut Fake,
        0.0,
    );
    host.settle();
    // `cccc` starts the second row: the caret two into it is 14 across, a row down, as tall as the
    // row.
    let caret = host
        .editors
        .metrics(EDITOR)
        .and_then(|m| m.caret)
        .expect("a caret");
    assert_eq!(caret, Rect::new(4.0 + 14.0, 4.0 + ROW, 2.0, ROW));
    // A move up goes to the first row, by the place across.
    host.key(Key::Up, none());
    assert_eq!(host.ranges(), vec![Range::caret(2)]);
}

#[test]
fn a_folded_region_takes_no_rows_and_its_button_in_the_gutter_folds_it() {
    let mut host = Host::new(page_with(EditorOptions::default()));
    host.load("fn {\n  a\n  b\n}\nend");
    let laid = host.laid.clone();
    host.editors.ask(
        &laid,
        EDITOR,
        EditorAsk::Decorate(Decorate {
            layer: "folds".into(),
            version: 1,
            from: 0,
            to: 17,
            decorations: vec![Decoration::new(3, 13, Mark::Fold)],
        }),
        &mut Fake,
        0.0,
    );
    host.settle();
    let tall = host.editors.metrics(EDITOR).expect("laid out").content.h;
    // The gutter is a column of marks, two digits, room and a button: the button is at its right.
    let gutter_button = Point::new(30.0, 10.0);
    assert!(host.press(gutter_button, none(), 1));
    host.editors.release();
    let buffer = host.editors.buffer(EDITOR).expect("the editor");
    assert_eq!(buffer.folded(), &[(3, 13)]);
    let short = host.editors.metrics(EDITOR).expect("laid out").content.h;
    assert_eq!(tall - short, 3.0 * ROW);
    // The last line is the second row now.
    assert!(host.press(Point::new(60.0, 4.0 + ROW + 8.0), none(), 1));
    let head = host.ranges()[0].head;
    assert!(head >= 14, "on the line after the fold, not {head}");
}

#[test]
fn an_editor_that_folds_by_the_indent_has_a_button_by_each_line_that_folds_and_a_plain_one_none() {
    let text = "fn:\n  a\n  b\nend";
    let buttons = |host: &mut Host| -> Vec<(String, f32)> {
        host.texts()
            .into_iter()
            .filter(|(text, _, _)| text == "▾" || text == "▸")
            .map(|(text, at, _)| (text, at.y))
            .collect()
    };
    // An editor that says nothing of its folds: no button, and a click where one would be folds
    // nothing.
    let mut plain = Host::new(page_with(EditorOptions::default()));
    plain.load(text);
    assert!(buttons(&mut plain).is_empty());
    let gutter_button = Point::new(30.0, 10.0);
    plain.press(gutter_button, none(), 1);
    plain.editors.release();
    let buffer = plain.editors.buffer(EDITOR).expect("the editor");
    assert!(buffer.folded().is_empty());
    // One that folds by the indent: a button by `fn:`, and none by the lines further in.
    let mut host = Host::new(page_with(EditorOptions {
        folds: Folds::Indent,
        ..EditorOptions::default()
    }));
    host.load(text);
    let open = buttons(&mut host);
    assert_eq!(open.len(), 1, "{open:?}");
    assert_eq!(open[0].0, "▾");
    let tall = host.editors.metrics(EDITOR).expect("laid out").content.h;
    assert!(host.press(gutter_button, none(), 1));
    host.editors.release();
    let buffer = host.editors.buffer(EDITOR).expect("the editor");
    assert_eq!(buffer.folded(), &[(3, 11)]);
    let short = host.editors.metrics(EDITOR).expect("laid out").content.h;
    assert_eq!(tall - short, 2.0 * ROW);
    assert_eq!(buttons(&mut host)[0].0, "▸");
}

#[test]
fn only_the_rows_in_view_are_laid_out_and_the_page_is_told_which_lines_they_are() {
    let mut host = Host::new(page_with(plain()));
    let text: String = (0..10_000).map(|n| format!("line {n}\n")).collect();
    host.load(&text);
    let viewport = host
        .editors
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            EditorEvent::Viewport { first, last, .. } => Some((first, last)),
            _ => None,
        })
        .next_back();
    assert_eq!(viewport, Some((0, 9)));
    let extent = host.laid.scroll_extent(EDITOR).expect("the room scrolls");
    assert_eq!(extent.content.h, 10_001.0 * ROW + 8.0);
    // The host scrolls the room, as it scrolls a box: far down, the lines there are in view.
    host.scrolls.ask(pencere_view::ScrollTo {
        id: EDITOR,
        x: None,
        y: Some(100_000.0),
        animate: false,
    });
    host.settle();
    let viewport = host
        .editors
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            EditorEvent::Viewport { first, last, .. } => Some((first, last)),
            _ => None,
        })
        .next_back();
    assert_eq!(viewport, Some((4999, 5009)));
    let list = host.paint(false);
    let lines: Vec<&str> = list
        .items
        .iter()
        .filter_map(|item| match item {
            Paint::Text(run) => Some(run.text.as_str()),
            _ => None,
        })
        .collect();
    assert!(lines.contains(&"line 5000"), "{lines:?}");
    assert!(
        !lines.contains(&"line 10"),
        "a line out of view is not drawn"
    );
}

#[test]
fn a_hidden_editor_keeps_its_document_and_one_gone_from_the_page_does_not() {
    use std::cell::Cell;
    use std::rc::Rc;
    let shown = Rc::new(Cell::new(1u8));
    let state = shown.clone();
    let mut host = Host::new(move || {
        let editor = div().editor(EDITOR, plain()).size_full();
        match state.get() {
            0 => div(),
            1 => div().child(editor),
            _ => div().child(editor.hidden()),
        }
    });
    host.load("kept");
    shown.set(2);
    host.settle();
    assert_eq!(host.text(), "kept");
    assert!(
        host.editors.metrics(EDITOR).is_none(),
        "a hidden editor lays nothing out"
    );
    assert!(
        host.paint(true)
            .items
            .iter()
            .all(|item| !matches!(item, Paint::Text(_))),
        "and draws nothing"
    );
    shown.set(1);
    host.settle();
    assert_eq!(host.text(), "kept");
    shown.set(0);
    host.settle();
    assert!(host.editors.buffer(EDITOR).is_none());
}

/// A page whose editor runs what is under the caret by ⌘↩, and has a popup by its caret while
/// `open` is set, whose keys (↓, ↩) are commands only while it is open.
fn with_commands(keys: Keys, open: bool) -> impl Fn() -> Element {
    move || {
        let mut editor = div()
            .editor(EDITOR, EditorOptions { keys, ..plain() })
            .size_full()
            .focused(true)
            .command(Command::new(RUN, "Run").keys(Shortcut::parse("cmd+enter").expect("keys")));
        if open {
            editor = editor
                .command(Command::new(POPUP, "Next").keys(Shortcut::parse("down").expect("keys")))
                .child(
                    div()
                        .id(POPUP)
                        .size(60.0, 40.0)
                        .anchor(EDITOR.caret(), Side::Below),
                );
        }
        div().child(editor)
    }
}

#[test]
fn a_key_is_the_pages_command_first_then_the_editors_then_text() {
    let mut host = Host::new(with_commands(Keys::Editor, false));
    host.load("a\nb");
    host.editors.set_focus(Some(EDITOR));
    assert_eq!(
        host.key(Key::Enter, with(|m| m.command = true)),
        Route::Command(RUN)
    );
    assert_eq!(host.key(Key::Down, none()), Route::Done);
    assert_eq!(host.ranges(), vec![Range::caret(2)]);
    assert_eq!(host.key(Key::Character('x'), none()), Route::Type);
    assert_eq!(host.key(Key::Escape, none()), Route::Page);
    assert_eq!(
        host.key(Key::Character('c'), with(|m| m.command = true)),
        Route::Copy("b\n".into())
    );
    // What ⌥ types on a keyboard that has no key of its own for it (`@` is ⌥Q on a Turkish Mac,
    // `{` ⌥⇧8 on a German one) is typed; ⌘ or ⌃ with a character is not text.
    assert_eq!(
        host.key(Key::Character('@'), with(|m| m.alt = true)),
        Route::Type
    );
    assert_eq!(
        host.key(
            Key::Character('{'),
            with(|m| {
                m.alt = true;
                m.shift = true;
            })
        ),
        Route::Type
    );
    assert_eq!(
        host.key(Key::Character('b'), with(|m| m.control = true)),
        Route::Page
    );
    assert_eq!(
        host.key(Key::Character('k'), with(|m| m.command = true)),
        Route::Page
    );
    // With the popup open, ↓ is the page's.
    let mut host = Host::new(with_commands(Keys::Editor, true));
    host.load("a\nb");
    host.editors.set_focus(Some(EDITOR));
    assert_eq!(host.key(Key::Down, none()), Route::Command(POPUP));
    assert_eq!(host.ranges(), vec![Range::caret(0)]);
}

#[test]
fn while_an_input_method_composes_no_command_of_the_page_is_done() {
    let mut host = Host::new(with_commands(Keys::Editor, true));
    host.editors.set_focus(Some(EDITOR));
    let laid = host.laid.clone();
    host.editors
        .set_marked(&laid, "k", (1, 1), None, &mut Fake, 0.0);
    host.settle();
    assert_eq!(host.key(Key::Down, none()), Route::Type);
    assert_eq!(
        host.key(Key::Enter, with(|m| m.command = true)),
        Route::Type
    );
    let laid = host.laid.clone();
    host.editors.type_text(&laid, "か", &mut Fake, 0.1);
    host.settle();
    assert_eq!(host.text(), "か");
    assert_eq!(host.key(Key::Down, none()), Route::Command(POPUP));
}

#[test]
fn an_editor_that_gives_its_keys_to_the_page_types_none_of_them() {
    let mut host = Host::new(with_commands(Keys::Page, false));
    host.load("ab");
    host.editors.set_focus(Some(EDITOR));
    assert_eq!(host.key(Key::Character('x'), none()), Route::Page);
    assert_eq!(host.key(Key::Right, none()), Route::Page);
    assert_eq!(host.key(Key::Backspace, none()), Route::Page);
    // The page's own commands are still its commands.
    assert_eq!(
        host.key(Key::Enter, with(|m| m.command = true)),
        Route::Command(RUN)
    );
    // The page drives the editor.
    let laid = host.laid.clone();
    host.editors.ask(
        &laid,
        EDITOR,
        EditorAsk::Command(EditCommand::Insert("x".into())),
        &mut Fake,
        0.0,
    );
    assert_eq!(host.text(), "xab");
    assert_eq!(host.ranges(), vec![Range::caret(1)]);
}

#[test]
fn the_editor_is_drawn_in_its_place_its_caret_while_the_host_shows_it() {
    let mut host = Host::new(page_with(plain()));
    host.load("mi");
    host.editors.set_focus(Some(EDITOR));
    let laid = host.laid.clone();
    host.editors.ask(
        &laid,
        EDITOR,
        EditorAsk::Select {
            version: 1,
            ranges: vec![Range::caret(1)],
            main: 0,
        },
        &mut Fake,
        0.0,
    );
    host.settle();
    let list = host.paint(true);
    assert!(
        list.items
            .iter()
            .all(|item| !matches!(item, Paint::Editor(_)))
    );
    let run = list
        .items
        .iter()
        .find_map(|item| match item {
            Paint::Text(run) => Some(run),
            _ => None,
        })
        .expect("the text is drawn");
    assert_eq!(
        (run.text.as_str(), run.origin),
        ("mi", Point::new(4.0, 4.0 + LEAD))
    );
    let caret = list.items.iter().any(|item| {
        matches!(item, Paint::Quad(q) if q.fill.and_then(|f| f.role()) == Some(Role::Cursor)
            && q.rect == Rect::new(4.0 + 11.0, 4.0, 2.0, ROW))
    });
    assert!(caret, "the caret is after the `m`");
    let off = host.paint(false);
    assert!(!off.items.iter().any(|item| {
        matches!(item, Paint::Quad(q) if q.fill.and_then(|f| f.role()) == Some(Role::Cursor))
    }));
}

#[test]
fn the_words_of_an_editor_in_a_faint_box_keep_their_colour_and_say_how_faint() {
    let mut host = Host::new(|| {
        div().child(
            div()
                .opacity(0.5)
                .size_full()
                .child(div().editor(EDITOR, plain()).size_full()),
        )
    });
    host.load("mi");
    host.settle();
    let list = host.paint(false);
    let run = list
        .items
        .iter()
        .find_map(|item| match item {
            Paint::Text(run) if run.text == "mi" => Some(run),
            _ => None,
        })
        .expect("the text is drawn");
    assert_eq!(run.color.alpha, 1.0);
    assert_eq!(run.opacity, 0.5);
}

#[test]
fn the_colours_of_the_page_cut_the_text_into_runs() {
    let mut host = Host::new(page_with(plain()));
    host.load("select x");
    let laid = host.laid.clone();
    let accent = pencere_view::edit::Look {
        color: Some(Role::Accent.solid()),
        ..pencere_view::edit::Look::default()
    };
    host.editors.ask(
        &laid,
        EDITOR,
        EditorAsk::Decorate(Decorate {
            layer: "syntax".into(),
            version: 1,
            from: 0,
            to: 8,
            decorations: vec![Decoration::new(0, 6, Mark::Style(accent))],
        }),
        &mut Fake,
        0.0,
    );
    host.settle();
    let runs: Vec<(String, Option<Role>, f32)> = host
        .paint(false)
        .items
        .iter()
        .filter_map(|item| match item {
            Paint::Text(run) => Some((run.text.clone(), run.color.role(), run.origin.x)),
            _ => None,
        })
        .collect();
    assert_eq!(
        runs,
        vec![
            ("select".to_owned(), Some(Role::Accent), 4.0),
            // `select` is 38 across: its `l` is narrow.
            (" x".to_owned(), Some(Role::Foreground), 4.0 + 38.0),
        ]
    );
}

#[test]
fn typing_past_the_bottom_of_the_room_scrolls_it_to_keep_the_caret_in_view() {
    let mut host = Host::new(page_with(plain()));
    let text: String = (0..30).map(|n| format!("{n}\n")).collect();
    host.load(&text);
    host.editors.set_focus(Some(EDITOR));
    let laid = host.laid.clone();
    // Line 20 is 4 + 400 down: past the 200 of the room.
    host.editors.ask(
        &laid,
        EDITOR,
        EditorAsk::Select {
            version: 1,
            ranges: vec![Range::caret(50)],
            main: 0,
        },
        &mut Fake,
        0.0,
    );
    let asked = host.editors.take_scrolls();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].y, Some(4.0 + 20.0 * ROW + ROW + 4.0 - 200.0));
    for ask in asked {
        host.scrolls.ask(ask);
    }
    host.settle();
    let caret = host
        .laid
        .bounds(EDITOR.caret())
        .expect("the caret is in view");
    assert!(caret.y >= 0.0 && caret.bottom() <= 200.0, "{caret:?}");
}

#[test]
fn a_popup_on_the_caret_is_placed_again_when_the_caret_moves_with_no_frame_of_the_page() {
    let mut host = Host::new(with_commands(Keys::Editor, true));
    host.load("abc\ndef");
    host.editors.set_focus(Some(EDITOR));
    let before = host.laid.bounds(POPUP).expect("the popup");
    assert_eq!(before.y, 4.0 + ROW);
    let laid = host.laid.clone();
    host.editors.key(&laid, Key::Right, none(), &mut Fake, 0.0);
    host.editors.key(&laid, Key::Right, none(), &mut Fake, 0.0);
    assert!(
        host.editors.wants_layout(),
        "the caret moved under the popup"
    );
    host.frame();
    let after = host.laid.bounds(POPUP).expect("the popup");
    assert_eq!(after.x - before.x, 14.0);
}

#[test]
fn the_documents_of_a_page_together_are_held_to_what_a_page_may() {
    let mut host = Host::new(page_with(plain()));
    host.load("x");
    let laid = host.laid.clone();
    // A change of the page larger than an editor may hold is refused, and the page told.
    let large = "a".repeat(pencere_view::edit::limits::DOCUMENT_BYTES);
    host.editors.ask(
        &laid,
        EDITOR,
        EditorAsk::Change {
            base: 1,
            changes: vec![Change::new(1, 1, large)],
            history: true,
            tag: None,
        },
        &mut Fake,
        0.0,
    );
    let refused = host.editors.take_events().into_iter().any(|event| {
        matches!(
            event,
            EditorEvent::Refused {
                refusal: pencere_view::edit::Refusal::TooLarge,
                ..
            }
        )
    });
    assert!(refused);
    assert_eq!(host.text(), "x");
}

#[test]
fn the_page_says_how_tall_a_row_is_and_the_room_around_the_text() {
    // 2.4 times the font's 12.5 is 30: the line of 16 with 7 above and 7 below; 10 around it all.
    let mut host = Host::new(page_with(EditorOptions {
        line_height: 2.4,
        padding: 10.0,
        ..plain()
    }));
    host.load("mi\nwi");
    let metrics = host.editors.metrics(EDITOR).expect("laid out");
    assert_eq!(metrics.content.h, 2.0 * 30.0 + 20.0);
    host.caret_at(4);
    let caret = host
        .editors
        .metrics(EDITOR)
        .and_then(|m| m.caret)
        .expect("a caret");
    // `w` is 11 across, on the second row: the caret is as tall as the row.
    assert_eq!(caret, Rect::new(10.0 + 11.0, 10.0 + 30.0, 2.0, 30.0));
    // Past what an editor takes, a row is three times the font's size; not a number, the default.
    let held = EditorOptions {
        line_height: 9.0,
        padding: f32::NAN,
        ..EditorOptions::default()
    }
    .held();
    assert_eq!((held.line_height, held.padding), (3.0, 4.0));
}

#[test]
fn a_block_caret_is_as_wide_as_its_character_which_is_drawn_again_on_it() {
    let mut host = Host::new(page_with(EditorOptions {
        caret: Caret {
            shape: CaretShape::Block,
            color: Tint::Color(Role::Accent.solid()),
            text: Tint::Color(Role::Magenta.solid()),
            ..Caret::default()
        },
        ..plain()
    }));
    host.load("mi");
    host.caret_at(0);
    let block = Rect::new(4.0, 4.0, 11.0, ROW);
    assert!(host.quads().contains(&(block, Some(Role::Accent))));
    assert!(
        host.texts()
            .contains(&("m".into(), Point::new(4.0, 4.0 + LEAD), Some(Role::Magenta)))
    );
    // At the end of the line it is a space wide, and over no character.
    host.caret_at(2);
    let block = Rect::new(4.0 + 14.0, 4.0, 7.0, ROW);
    assert!(host.quads().contains(&(block, Some(Role::Accent))));
    assert!(
        !host
            .texts()
            .iter()
            .any(|(_, _, role)| *role == Some(Role::Magenta))
    );
}

#[test]
fn an_underline_caret_is_a_line_under_its_character_as_thick_as_the_page_says() {
    let mut host = Host::new(page_with(EditorOptions {
        caret: Caret {
            shape: CaretShape::Underline,
            width: 3.0,
            ..Caret::default()
        },
        ..plain()
    }));
    host.load("mi");
    host.caret_at(1);
    let line = Rect::new(4.0 + 11.0, 4.0 + ROW - 3.0, 3.0, 3.0);
    assert!(host.quads().contains(&(line, Some(Role::Cursor))));
    // What stands by the caret stands by its row, not on the text.
    let caret = host
        .editors
        .metrics(EDITOR)
        .and_then(|m| m.caret)
        .expect("a caret");
    assert_eq!(caret, Rect::new(4.0 + 11.0, 4.0, 3.0, ROW));
}

#[test]
fn a_caret_as_tall_as_the_text_leaves_the_rows_room_above_and_below_it() {
    let caret = |shape: CaretShape| {
        let mut host = Host::new(page_with(EditorOptions {
            caret: Caret {
                shape,
                height: CaretHeight::Text,
                ..Caret::default()
            },
            ..plain()
        }));
        host.load("mi");
        host.caret_at(1);
        let anchor = host
            .editors
            .metrics(EDITOR)
            .and_then(|m| m.caret)
            .expect("a caret");
        let drawn = host
            .quads()
            .into_iter()
            .find(|(_, role)| *role == Some(Role::Cursor))
            .map(|(rect, _)| rect)
            .expect("the caret is drawn");
        (drawn, anchor)
    };
    let text = Rect::new(4.0 + 11.0, 4.0 + LEAD, 2.0, LINE);
    assert_eq!(caret(CaretShape::Bar), (text, text));
    let under = Rect::new(4.0 + 11.0, 4.0 + LEAD + LINE - 2.0, 3.0, 2.0);
    assert_eq!(
        caret(CaretShape::Underline),
        (under, Rect::new(4.0 + 11.0, 4.0 + LEAD, 3.0, LINE))
    );
}

#[test]
fn relative_numbers_count_the_lines_from_the_caret_a_folded_region_as_one() {
    let mut host = Host::new(page_with(EditorOptions {
        numbers: Numbers::Relative,
        ..EditorOptions::default()
    }));
    // Lines `a` to `f`; the region from the end of `c` to the end of `d` folds `d` away.
    host.load("a\nb\nc\nd\ne\nf");
    let laid = host.laid.clone();
    host.editors.ask(
        &laid,
        EDITOR,
        EditorAsk::Decorate(Decorate {
            layer: "folds".into(),
            version: 1,
            from: 0,
            to: 11,
            decorations: vec![Decoration::new(5, 7, Mark::Fold)],
        }),
        &mut Fake,
        0.0,
    );
    host.caret_at(4);
    let laid = host.laid.clone();
    host.editors.ask(
        &laid,
        EDITOR,
        EditorAsk::Command(EditCommand::Fold),
        &mut Fake,
        0.0,
    );
    host.caret_at(2);
    let numbers: Vec<String> = host
        .texts()
        .into_iter()
        .map(|(text, _, _)| text)
        .filter(|text| text.chars().all(|c| c.is_ascii_digit()))
        .collect();
    // The caret's line is its own number; `e` is two lines down, `d` being folded.
    assert_eq!(numbers, vec!["1", "2", "1", "2", "3"]);
}

#[test]
fn hidden_numbers_keep_the_marks_and_the_fold_buttons_and_take_their_room() {
    let text_x = |numbers: Numbers| {
        let mut host = Host::new(page_with(EditorOptions {
            numbers,
            ..EditorOptions::default()
        }));
        host.load("mi");
        let texts = host.texts();
        let digits = texts
            .iter()
            .filter(|(text, _, _)| text.chars().all(|c| c.is_ascii_digit()))
            .count();
        let x = texts
            .iter()
            .find(|(text, _, _)| text == "mi")
            .map(|(_, at, _)| at.x)
            .expect("the text is drawn");
        (digits, x)
    };
    let (shown, at) = text_x(Numbers::Absolute);
    let (hidden, nearer) = text_x(Numbers::Hidden);
    assert_eq!((shown, hidden), (1, 0));
    // Two digits of 7 and the room after them.
    assert_eq!(at - nearer, 2.0 * 7.0 + 4.0);
}

#[test]
fn the_page_colours_what_the_editor_draws_and_what_it_turns_off_is_not_drawn() {
    let colours = |colors: EditorColors| {
        let mut host = Host::new(page_with(EditorOptions {
            colors,
            ..EditorOptions::default()
        }));
        host.load("mi\nwi");
        host.caret_at(1);
        let quads = host.quads();
        let texts = host.texts();
        (quads, texts)
    };
    // The theme's: the caret's line in the foreground's colour, the numbers quiet and dim.
    let (quads, texts) = colours(EditorColors::default());
    assert!(
        quads
            .iter()
            .any(|(_, role)| *role == Some(Role::Foreground))
    );
    let role_of = |texts: &[(String, Point, Option<Role>)], wanted: &str| {
        texts
            .iter()
            .find(|(text, _, _)| text == wanted)
            .and_then(|(_, _, role)| *role)
    };
    assert_eq!(role_of(&texts, "1"), Some(Role::Dim));
    assert_eq!(role_of(&texts, "2"), Some(Role::Quiet));
    let (quads, texts) = colours(EditorColors {
        current_line: Tint::Off,
        numbers: Tint::Color(Role::Accent.solid()),
        current_number: Tint::Off,
        gutter: Tint::Color(Role::Raised(1).solid()),
        ..EditorColors::default()
    });
    assert!(
        !quads
            .iter()
            .any(|(_, role)| *role == Some(Role::Foreground))
    );
    assert!(quads.iter().any(|(_, role)| *role == Some(Role::Raised(1))));
    let numbers: Vec<_> = texts
        .iter()
        .filter(|(text, _, _)| text.chars().all(|c| c.is_ascii_digit()))
        .collect();
    assert_eq!(
        numbers.len(),
        1,
        "the caret's line has no number: {numbers:?}"
    );
    assert_eq!(numbers[0].2, Some(Role::Accent));
}

#[test]
fn the_page_keeps_rows_between_the_caret_and_the_edge_of_the_room() {
    let mut host = Host::new(page_with(EditorOptions {
        scroll_margin: 3,
        ..plain()
    }));
    let text: String = (0..30).map(|n| format!("{n}\n")).collect();
    host.load(&text);
    host.editors.set_focus(Some(EDITOR));
    host.editors.take_scrolls();
    let laid = host.laid.clone();
    host.editors.ask(
        &laid,
        EDITOR,
        EditorAsk::Select {
            version: 1,
            ranges: vec![Range::caret(50)],
            main: 0,
        },
        &mut Fake,
        0.0,
    );
    // Line 20 ends 4 + 21 rows down; three rows more show below it, and the room's pad.
    let asked = host.editors.take_scrolls();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].y, Some(4.0 + 21.0 * ROW + 3.0 * ROW + 4.0 - 200.0));
}

/// What the page was told the search finds, in order.
fn found_told(events: Vec<EditorEvent>) -> Vec<Found> {
    events
        .into_iter()
        .filter_map(|event| match event {
            EditorEvent::Found { found, .. } => Some(found),
            _ => None,
        })
        .collect()
}

#[test]
fn what_is_typed_is_searched_again_once_a_frame_and_the_page_told_what_it_finds() {
    let mut host = Host::new(page_with(plain()));
    host.load("foo bar");
    host.editors.set_focus(Some(EDITOR));
    let laid = host.laid.clone();
    let find = EditCommand::Find {
        pattern: "foo".into(),
        regex: false,
        case: true,
        word: false,
    };
    host.editors
        .ask(&laid, EDITOR, EditorAsk::Command(find), &mut Fake, host.now);
    let version = host.editors.buffer(EDITOR).expect("the editor").version();
    assert_eq!(
        found_told(host.editors.take_events()),
        [Found {
            version,
            count: 1,
            current: Some(1),
            more: false,
        }]
    );
    // The caret moved off the match: it is on none of them now.
    let cmd = with(|mods| mods.command = true);
    host.editors.key(&laid, Key::Down, cmd, &mut Fake, host.now);
    assert_eq!(
        found_told(host.editors.take_events()),
        [Found {
            version,
            count: 1,
            current: None,
            more: false,
        }]
    );
    // Keys between two frames: what they typed is carried over, and searched again at the frame,
    // once.
    host.editors.type_text(&laid, " foo", &mut Fake, host.now);
    host.editors.type_text(&laid, " foo", &mut Fake, host.now);
    assert!(found_told(host.editors.take_events()).is_empty());
    let buffer = host.editors.buffer(EDITOR).expect("the editor");
    assert_eq!(buffer.found(), [(0, 3)]);
    host.frame();
    assert_eq!(
        found_told(host.editors.take_events()),
        [Found {
            version: version + 2,
            count: 3,
            current: None,
            more: false,
        }]
    );
    let buffer = host.editors.buffer(EDITOR).expect("the editor");
    assert_eq!(buffer.found(), [(0, 3), (8, 11), (12, 15)]);
    // A frame with nothing new to search tells nothing.
    host.frame();
    assert!(found_told(host.editors.take_events()).is_empty());
}

// --- What is not seen: the spaces and tabs, and the guides of the indent ------------------------

/// Where each character of `line` is across, from the start of the text (4 in): a tab to its next
/// stop, every 28 (four spaces of 7).
fn cells(line: &str) -> Vec<(f32, f32)> {
    let mut x = 0.0_f32;
    line.chars()
        .map(|c| {
            let width = if c == '\t' {
                ((x / 28.0).floor() + 1.0) * 28.0 - x
            } else {
                width_of(c)
            };
            let cell = (4.0 + x, 4.0 + x + width);
            x += width;
            cell
        })
        .collect()
}

/// What the editor draws of the spaces and tabs of its `lines` (each one row), in `role`'s colour:
/// each line with the characters a mark is drawn on put as their marks, `·` a dot, `→` an arrow
/// and `□` a box.
fn marks_drawn(host: &mut Host, role: Role, lines: &[&str]) -> Vec<String> {
    let mut drawn: Vec<Vec<char>> = lines.iter().map(|line| line.chars().collect()).collect();
    for item in host.paint(true).items {
        let (mark, rect) = match &item {
            Paint::Quad(quad)
                if quad.fill.and_then(|fill| fill.role()) == Some(role) && quad.radii[0] > 0.0 =>
            {
                ('·', quad.rect)
            }
            Paint::Quad(quad)
                if quad.border.as_ref().and_then(|border| border.color.role()) == Some(role) =>
            {
                ('□', quad.rect)
            }
            Paint::Figure(figure)
                if figure
                    .paths
                    .iter()
                    .any(|path| path.stroke.and_then(|line| line.color.role()) == Some(role)) =>
            {
                ('→', figure.rect)
            }
            _ => continue,
        };
        let (x, y) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
        let line = ((y - 4.0) / ROW).floor() as usize;
        let at = cells(lines[line])
            .iter()
            .position(|(left, right)| *left <= x && x < *right)
            .expect("a mark is on a character");
        drawn[line][at] = mark;
    }
    drawn
        .into_iter()
        .map(|chars| chars.into_iter().collect())
        .collect()
}

/// The guides of the indent drawn in `role`'s colour: for each of the first `rows` rows, the
/// columns they are at (a column is a space, 7 across), left to right.
fn guides_drawn(host: &mut Host, role: Role, rows: usize) -> Vec<Vec<usize>> {
    let mut drawn = vec![Vec::new(); rows];
    for item in host.paint(true).items {
        if let Paint::Quad(quad) = &item
            && quad.fill.and_then(|fill| fill.role()) == Some(role)
            && quad.radii == [0.0; 4]
        {
            assert_eq!(
                (quad.rect.w, quad.rect.h),
                (1.0, ROW),
                "a guide is a line a pixel wide down its row"
            );
            let row = ((quad.rect.y - 4.0) / ROW) as usize;
            drawn[row].push(((quad.rect.x - 4.0) / 7.0) as usize);
        }
    }
    drawn
}

/// An editor that shows its spaces and tabs as `whitespace` says, in magenta, and its guides of
/// the indent where `guides`, in cyan (the main caret's block's in yellow).
fn invisible(whitespace: Whitespace, guides: bool) -> EditorOptions {
    EditorOptions {
        whitespace,
        indent_guides: guides,
        colors: EditorColors {
            whitespace: Tint::Color(Role::Magenta.solid()),
            indent_guides: Tint::Color(Role::Cyan.solid()),
            active_indent_guide: Tint::Color(Role::Yellow.solid()),
            ..EditorColors::default()
        },
        ..plain()
    }
}

fn select(host: &mut Host, ranges: Vec<Range>) {
    let laid = host.laid.clone();
    let version = host.editors.buffer(EDITOR).expect("the editor").version();
    host.editors.ask(
        &laid,
        EDITOR,
        EditorAsk::Select {
            version,
            ranges,
            main: 0,
        },
        &mut Fake,
        host.now,
    );
    host.settle();
}

fn command(host: &mut Host, command: EditCommand) {
    let laid = host.laid.clone();
    host.editors.ask(
        &laid,
        EDITOR,
        EditorAsk::Command(command),
        &mut Fake,
        host.now,
    );
    host.settle();
}

#[test]
fn each_whitespace_shows_the_spaces_and_tabs_vs_code_shows() {
    // Spaces before the text, alone between words and two together, a tab in the middle and spaces
    // after the text; a line of spaces alone; a space alone between two words.
    let lines = ["  a b  c\td  ", "   ", "x y"];
    let text = lines.join("\n");
    let drawn = |whitespace: Whitespace, ranges: Option<Vec<Range>>| {
        let mut host = Host::new(page_with(invisible(whitespace, false)));
        host.load(&text);
        if let Some(ranges) = ranges {
            select(&mut host, ranges);
        }
        marks_drawn(&mut host, Role::Magenta, &lines)
    };
    assert_eq!(drawn(Whitespace::None, None), lines);
    assert_eq!(drawn(Whitespace::All, None), ["··a·b··c→d··", "···", "x·y"]);
    // All but a space alone between two words.
    assert_eq!(
        drawn(Whitespace::Boundary, None),
        ["··a b··c→d··", "···", "x y"]
    );
    // After the last word, every one of a line of spaces.
    assert_eq!(
        drawn(Whitespace::Trailing, None),
        ["  a b  c\td··", "···", "x y"]
    );
    // In what is selected, from the space after `a` to the tab and a space of the next line: the
    // editor need not have the keyboard.
    assert_eq!(
        drawn(
            Whitespace::Selection,
            Some(vec![Range::new(3, 9), Range::new(15, 14)])
        ),
        ["  a·b··c→d  ", " · ", "x y"]
    );
    assert_eq!(
        drawn(Whitespace::Selection, Some(vec![Range::caret(3)])),
        lines,
        "a caret selects nothing"
    );
}

#[test]
fn a_tabs_arrow_is_as_wide_as_the_tab_wherever_it_starts() {
    // Tabs from the start, a column in, three columns in (to the stop just after), a stop in, and
    // a pixel short of a stop (`i` is 3 across): too narrow for an arrow.
    let lines = ["\tx", "x\ty", "xxx\ty", "xxxx\ty", "iiiiiiiii\ty"];
    let mut host = Host::new(page_with(invisible(Whitespace::All, false)));
    host.load(&lines.join("\n"));
    let arrows: Vec<(f32, f32, f32)> = host
        .paint(true)
        .items
        .iter()
        .filter_map(|item| match item {
            Paint::Figure(figure) => Some((figure.rect.x, figure.rect.w, figure.rect.y)),
            _ => None,
        })
        .collect();
    let middle = |line: usize| 4.0 + line as f32 * ROW + ROW / 2.0;
    assert_eq!(arrows.len(), 4, "{arrows:?}");
    for (line, (x, w)) in [(4.0, 28.0), (11.0, 21.0), (25.0, 7.0), (32.0, 28.0)]
        .into_iter()
        .enumerate()
    {
        let (left, width, top) = arrows[line];
        assert_eq!((left, width), (x, w), "the tab of line {line}");
        assert!(
            top < middle(line) && middle(line) < top + 8.0,
            "the arrow of line {line} is along the middle of its row"
        );
    }
    // The line of an arrow runs from a seventh of a space in to the head at the other end.
    let Some(Paint::Figure(figure)) = host
        .paint(true)
        .items
        .into_iter()
        .find(|item| matches!(item, Paint::Figure(_)))
    else {
        panic!("an arrow")
    };
    let xs: Vec<f32> = figure.paths[0]
        .segments
        .iter()
        .filter_map(|segment| match segment {
            pencere_view::Segment::Move(at) | pencere_view::Segment::Line(at) => Some(at.x),
            _ => None,
        })
        .collect();
    assert_eq!(xs, [1.0, 27.0, 25.0, 27.0, 25.0]);
}

#[test]
fn other_spaces_show_as_boxes_a_control_as_nothing_and_a_space_with_an_accent_as_a_letter() {
    // A no-break space and an ideographic one between words, a `\r` before the line break; a space
    // after the text and before the `\r`; a space an accent is on.
    let lines = ["a\u{a0}b\u{3000}c\r", "a b \r", " \u{301}x"];
    let text = lines.join("\n");
    let drawn = |whitespace: Whitespace| {
        let mut host = Host::new(page_with(invisible(whitespace, false)));
        host.load(&text);
        marks_drawn(&mut host, Role::Magenta, &lines)
    };
    assert_eq!(drawn(Whitespace::All), ["a□b□c\r", "a·b·\r", " \u{301}x"]);
    // An other space alone between words is shown, as a tab is.
    assert_eq!(
        drawn(Whitespace::Boundary),
        ["a□b□c\r", "a b·\r", " \u{301}x"]
    );
    // The `\r` counts as a space where the text ends.
    assert_eq!(
        drawn(Whitespace::Trailing),
        ["a\u{a0}b\u{3000}c\r", "a b·\r", " \u{301}x"]
    );
}

#[test]
fn the_guides_show_each_level_of_indent_and_go_on_through_the_lines_with_no_text() {
    let guides = |text: &str, rows: usize| {
        let mut host = Host::new(page_with(invisible(Whitespace::None, true)));
        host.load(text);
        guides_drawn(&mut host, Role::Cyan, rows)
    };
    // A line with no text shows the guides of the block it is in: the one the line above opens
    // (before `}`), the one the line below ends.
    assert_eq!(
        guides("fn a {\n    if b {\n        c\n\n    }\n\n}", 7),
        [
            vec![],
            vec![0],
            vec![0, 4],
            vec![0, 4],
            vec![0],
            vec![0],
            vec![]
        ]
    );
    // A tab is a level, as far as its next stop; a level part filled is one; a line of spaces alone
    // is one with no text.
    assert_eq!(
        guides("\tx\n\t\ty\n  z\n      w\n        \n      v", 6),
        [
            vec![0],
            vec![0, 4],
            vec![0],
            vec![0, 4],
            vec![0, 4],
            vec![0, 4]
        ]
    );
    // None on a line with no text at the top or the bottom of the document.
    assert_eq!(guides("\n    x\n", 3), [vec![], vec![0], vec![]]);
}

#[test]
fn a_folded_line_and_a_wrapped_one_show_their_guides_on_the_rows_they_have() {
    // Folded by the indent, `if b` hides the two lines further in: nothing is drawn of them.
    let mut host = Host::new(page_with(EditorOptions {
        folds: Folds::Indent,
        ..invisible(Whitespace::All, true)
    }));
    let text = "fn a {\n    if b {\n        c\n        d\n    }\n}";
    host.load(text);
    host.caret_at(10);
    command(&mut host, EditCommand::Fold);
    let buffer = host.editors.buffer(EDITOR).expect("the editor");
    assert_eq!(buffer.hidden_lines(), [(2, 3)]);
    host.editors.set_focus(None);
    assert_eq!(
        guides_drawn(&mut host, Role::Cyan, 6),
        [vec![], vec![0], vec![0], vec![], vec![], vec![]]
    );
    let lines = ["fn a {", "    if b {", "    }", "}"];
    assert_eq!(
        marks_drawn(&mut host, Role::Magenta, &lines),
        ["fn·a·{", "····if·b·{", "····}", "}"]
    );
    // A line that wraps shows its guides on its first row: the rows after it start at the left.
    let options = EditorOptions {
        wrap: true,
        ..invisible(Whitespace::None, true)
    };
    let mut host =
        Host::new(move || div().child(div().editor(EDITOR, options.clone()).size(100.0, 200.0)));
    // 92 across: "    aaaa ", "bbbb cccc ", "dddd".
    host.load("    aaaa bbbb cccc dddd\n    x");
    assert_eq!(
        guides_drawn(&mut host, Role::Cyan, 4),
        [vec![0], vec![], vec![], vec![0]]
    );
}

#[test]
fn the_guide_of_the_block_the_caret_is_in_is_drawn_in_its_own_colour_while_it_has_the_keyboard() {
    let text = "fn a {\n    if b {\n        c\n\n    }\n\n}";
    let mut host = Host::new(page_with(invisible(Whitespace::None, true)));
    host.load(text);
    let at = |host: &mut Host, place: usize| {
        host.caret_at(place);
        (
            guides_drawn(host, Role::Cyan, 7),
            guides_drawn(host, Role::Yellow, 7),
        )
    };
    let none = || vec![Vec::<usize>::new(); 7];
    // In `c`'s block, on it, on the line that opens it and on the one that ends it.
    for place in [26, 10, 33] {
        let (guides, active) = at(&mut host, place);
        assert_eq!(
            active,
            [vec![], vec![], vec![4], vec![4], vec![], vec![], vec![]],
            "the caret at {place}"
        );
        assert_eq!(
            guides,
            [vec![], vec![0], vec![0], vec![0], vec![0], vec![0], vec![]]
        );
    }
    // On the line that opens the outer block and on the one that ends it: the block of `fn a`.
    for place in [0, 36] {
        let (_, active) = at(&mut host, place);
        assert_eq!(
            active,
            [vec![], vec![0], vec![0], vec![0], vec![0], vec![0], vec![]],
            "the caret at {place}"
        );
    }
    // Without the keyboard, no guide is the caret's.
    host.editors.set_focus(None);
    assert_eq!(guides_drawn(&mut host, Role::Yellow, 7), none());
}

#[test]
fn the_page_colours_the_marks_and_the_guides_and_what_it_turns_off_is_not_drawn() {
    let text = "fn a {\n    b c\n}";
    let colours = |colors: EditorColors| {
        let mut host = Host::new(page_with(EditorOptions {
            whitespace: Whitespace::All,
            indent_guides: true,
            colors,
            ..plain()
        }));
        host.load(text);
        host.caret_at(12);
        host.quads()
    };
    let count = |quads: &[(Rect, Option<Role>)], role: Role, round: bool| {
        quads
            .iter()
            .filter(|(rect, colour)| *colour == Some(role) && (rect.w == 2.0) == round)
            .count()
    };
    // The theme's: the dots in its border colour, the guides too, the caret's block's quiet.
    let quads = colours(EditorColors::default());
    assert_eq!(count(&quads, Role::Border, true), 7, "the dots: {quads:?}");
    assert_eq!(
        count(&quads, Role::Quiet, false),
        1,
        "the caret's block's guide"
    );
    assert_eq!(count(&quads, Role::Border, false), 0, "no other guide");
    // The page's own, and what it turns off.
    let quads = colours(EditorColors {
        whitespace: Tint::Off,
        indent_guides: Tint::Color(Role::Accent.solid()),
        active_indent_guide: Tint::Off,
        ..EditorColors::default()
    });
    assert_eq!(count(&quads, Role::Border, true), 0, "no dots");
    assert_eq!(
        count(&quads, Role::Accent, false),
        1,
        "the caret's block's guide is drawn as the others are"
    );
    let quads = colours(EditorColors {
        whitespace: Tint::Color(Role::Info.solid()),
        indent_guides: Tint::Off,
        ..EditorColors::default()
    });
    assert_eq!(count(&quads, Role::Info, true), 7);
    assert_eq!(
        count(&quads, Role::Quiet, false),
        1,
        "with the guides off, the caret's block's alone"
    );
    let quads = colours(EditorColors {
        indent_guides: Tint::Off,
        active_indent_guide: Tint::Off,
        ..EditorColors::default()
    });
    assert_eq!(count(&quads, Role::Quiet, false), 0, "no guide at all");
}

#[test]
fn an_editor_that_says_neither_draws_as_it_did_and_what_they_draw_is_only_added() {
    // Spaces and tabs under the page's grounds and looks, selected, found, folded, under a block
    // caret, a no-break space, a line too long for the room.
    let text = "fn a {\n\tif b {\n\t\tc  d\u{a0}e  \n\t}\n    f\n\n}\n    long long long long long long long long long long long long long long";
    let accent = Look {
        background: Some(Role::Accent.alpha(0.3)),
        italic: Some(true),
        ..Look::default()
    };
    let paints = |options: EditorOptions, focused: bool| {
        let mut host = Host::new(page_with(EditorOptions {
            folds: Folds::Indent,
            caret: Caret {
                shape: CaretShape::Block,
                ..Caret::default()
            },
            ..options
        }));
        host.load(text);
        let laid = host.laid.clone();
        host.editors.ask(
            &laid,
            EDITOR,
            EditorAsk::Decorate(Decorate {
                layer: "syntax".into(),
                version: 1,
                from: 0,
                to: 40,
                decorations: vec![
                    Decoration::new(14, 26, Mark::Style(accent.clone())),
                    Decoration::new(28, 31, Mark::Line(Role::Red.alpha(0.2))),
                ],
            }),
            &mut Fake,
            0.0,
        );
        host.settle();
        command(
            &mut host,
            EditCommand::Find {
                pattern: "d".into(),
                regex: false,
                case: true,
                word: false,
            },
        );
        host.caret_at(37);
        command(&mut host, EditCommand::Fold);
        select(&mut host, vec![Range::new(16, 24), Range::caret(48)]);
        if !focused {
            host.editors.set_focus(None);
        }
        host.paint(true).items
    };
    let marked = |item: &Paint| {
        let ours =
            |color: Option<Role>| matches!(color, Some(Role::Magenta | Role::Cyan | Role::Yellow));
        match item {
            Paint::Quad(quad) => {
                ours(quad.fill.and_then(|fill| fill.role()))
                    || ours(quad.border.as_ref().and_then(|border| border.color.role()))
            }
            Paint::Figure(figure) => figure
                .paths
                .iter()
                .any(|path| ours(path.stroke.and_then(|line| line.color.role()))),
            _ => false,
        }
    };
    for focused in [true, false] {
        let plain_editor = paints(plain(), focused);
        assert!(!plain_editor.iter().any(marked));
        // Saying neither is saying nothing.
        assert_eq!(
            paints(invisible(Whitespace::None, false), focused),
            plain_editor
        );
        // What they draw is all there is besides what a plain editor draws, in the same order.
        let shown = paints(invisible(Whitespace::All, true), focused);
        assert!(shown.len() > plain_editor.len() + 20, "they draw");
        let without: Vec<Paint> = shown.into_iter().filter(|item| !marked(item)).collect();
        assert_eq!(without, plain_editor);
    }
}

#[test]
fn whatever_the_text_rows_and_selections_the_marks_and_guides_are_drawn_in_the_room() {
    const PIECES: [&str; 13] = [
        " ", "  ", "\t", "\u{a0}", "\r", "x", "\n", "e\u{301}", " \u{301}", "ğ", "😀", "\u{3000}",
        "\n\n",
    ];
    let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
    let mut below = |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    let kinds = [
        Whitespace::None,
        Whitespace::Boundary,
        Whitespace::Selection,
        Whitespace::Trailing,
        Whitespace::All,
    ];
    for round in 0..60 {
        let text: String = (0..below(80))
            .map(|_| PIECES[below(PIECES.len())])
            .collect();
        let options = EditorOptions {
            wrap: round % 2 == 0,
            tab_width: 1 + below(8) as u8,
            folds: Folds::Indent,
            ..invisible(kinds[round % kinds.len()], true)
        };
        let mut host = Host::new(page_with(options));
        host.load(&text);
        let len = text.chars().count();
        host.caret_at(below(len + 1));
        command(&mut host, EditCommand::Fold);
        select(
            &mut host,
            vec![
                Range::new(below(len + 1), below(len + 1)),
                Range::caret(len),
            ],
        );
        let room = Rect::new(0.0, 0.0, 400.0, 200.0);
        for item in host.paint(true).items {
            let rect = match &item {
                Paint::Quad(quad) => quad.rect,
                Paint::Figure(figure) => figure.rect,
                _ => continue,
            };
            assert!(
                rect.w.is_finite() && rect.h.is_finite() && rect.w >= 0.0 && rect.h >= 0.0,
                "{text:?}: {item:?}"
            );
            assert!(
                rect.x < room.right() + 1.0 && rect.y < room.bottom() + ROW,
                "{text:?}: {item:?}"
            );
        }
    }
}

#[test]
fn selecting_every_occurrence_keeps_the_main_selection_in_order_within_the_most_that_are_selected()
{
    use pencere_editor::{Buffer, FIND_MATCHES};
    // One more of the word than are selected: the caret's, the last, is past the most a search
    // marks. It is selected still, and in its place in order; the one farthest from it goes.
    let words = FIND_MATCHES + 1;
    let mut buffer = Buffer::new(&"ab ".repeat(words)).expect("a document");
    let last = (words - 1) * 3;
    buffer.select(vec![Range::caret(last + 1)], 0);
    buffer.command(EditCommand::SelectOccurrences, 0.0);
    let ranges = buffer.ranges();
    assert_eq!(ranges.len(), FIND_MATCHES);
    assert_eq!(buffer.main(), Range::new(last, last + 2));
    assert_eq!(ranges.last(), Some(&Range::new(last, last + 2)));
    assert_eq!(
        ranges.first(),
        Some(&Range::new(3, 5)),
        "the first is the farthest"
    );
    assert!(
        ranges
            .windows(2)
            .all(|pair| pair[0].end() < pair[1].start())
    );
    // At the first word the main selection is among them: the first of the most there are.
    buffer.select(vec![Range::caret(1)], 0);
    buffer.command(EditCommand::SelectOccurrences, 0.0);
    assert_eq!(buffer.ranges().len(), FIND_MATCHES);
    assert_eq!(buffer.main(), Range::new(0, 2));
}

#[test]
fn the_caret_s_line_is_drawn_across_the_text_the_gutter_or_both_as_the_page_says() {
    // The gutter is 41 wide: marks 10, two digits and room 18, the fold buttons 13.
    let bands = |current_line: LineHighlight| {
        let mut host = Host::new(page_with(EditorOptions {
            current_line,
            ..EditorOptions::default()
        }));
        host.load("mi\nwi");
        host.caret_at(4);
        let mut bands: Vec<(f32, f32)> = host
            .quads()
            .into_iter()
            .filter(|(rect, role)| *role == Some(Role::Foreground) && rect.h == ROW)
            .map(|(rect, _)| (rect.x, rect.y))
            .collect();
        bands.sort_by(|a, b| a.0.total_cmp(&b.0));
        bands
    };
    let row = 4.0 + ROW;
    assert_eq!(bands(LineHighlight::Text), vec![(41.0, row)]);
    assert_eq!(bands(LineHighlight::Gutter), vec![(0.0, row)]);
    assert_eq!(bands(LineHighlight::All), vec![(0.0, row), (41.0, row)]);
}

// --- The gutter's columns, markers and events, the overview ruler, the zones ---------------

/// A gutter of the editor's `numbers` (two digits: 18 wide), a column of the page's `changes`
/// 6 wide and the buttons that fold (13): the page's column is at 18..24.
fn columns() -> EditorOptions {
    EditorOptions {
        gutters: vec![
            Column {
                pad: Some(0.0),
                ..Column::new("numbers")
            },
            Column {
                width: Some(6.0),
                ..Column::new("changes")
            },
            Column::new("folds"),
        ],
        ..EditorOptions::default()
    }
}

#[test]
fn the_numbers_keep_room_to_their_left_where_they_are_the_first_column_and_the_page_may_say_how_much()
 {
    let first = |pad: Option<f32>, others: Vec<Column>| {
        let mut gutters = vec![Column {
            pad,
            ..Column::new("numbers")
        }];
        gutters.extend(others);
        let mut host = Host::new(page_with(EditorOptions {
            gutters,
            ..EditorOptions::default()
        }));
        host.load("a\nb\nc\nd\ne\nf\ng\nh\ni\nj");
        // Where the text starts is where the gutter ends; `j` is the tenth, two digits.
        let texts = host.texts();
        let number = texts.iter().find(|(text, _, _)| text == "10").expect("10");
        let code = texts.iter().find(|(text, _, _)| text == "j").expect("j");
        (number.1.x, code.1.x)
    };
    let (number, code) = first(Some(0.0), vec![]);
    // The two digits stand against the edge of the room where the page keeps none.
    assert_eq!(number, 0.0);
    let (said, none_said) = first(None, vec![]);
    assert_eq!((said, none_said), (4.0, code + 4.0));
    let (at, ten) = first(Some(10.0), vec![]);
    assert_eq!((at, ten), (10.0, code + 10.0));
    // After another column there is none unless it is said.
    let (_, after) = first(None, vec![Column::new("folds")]);
    let (_, after_zero) = first(Some(0.0), vec![Column::new("folds")]);
    assert_eq!(after, after_zero + 4.0);
}

fn marker(shape: GutterShape, column: &str) -> GutterMark {
    GutterMark {
        column: column.into(),
        id: Some("m".into()),
        ..GutterMark::new(shape, Role::Green.solid())
    }
}

impl Host {
    fn decorate(&mut self, layer: &str, decorations: Vec<Decoration>) {
        let laid = self.laid.clone();
        let version = self.editors.buffer(EDITOR).expect("the editor").version();
        let to = self.editors.buffer(EDITOR).expect("the editor").len_chars();
        self.editors.ask(
            &laid,
            EDITOR,
            EditorAsk::Decorate(Decorate {
                layer: layer.into(),
                version,
                from: 0,
                to,
                decorations,
            }),
            &mut Fake,
            self.now,
        );
        self.settle();
    }

    fn hover(&mut self, at: Option<Point>) {
        let laid = self.laid.clone();
        self.editors.hover(&laid, at);
        self.settle();
    }

    fn cursor(&self, at: Point) -> Option<Cursor> {
        self.editors.cursor_at(&self.laid, at)
    }

    fn gutter_events(&mut self) -> Vec<pencere_view::edit::GutterEvent> {
        self.editors
            .take_events()
            .into_iter()
            .filter_map(|event| match event {
                EditorEvent::Gutter(event) => Some(event),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn the_pointer_is_the_i_beam_over_the_text_the_arrow_in_the_gutter_and_the_hand_where_something_answers()
 {
    let mut host = Host::new(page_with(columns()));
    host.load("fn {\n  a\n}\nend");
    host.decorate("folds", vec![Decoration::new(3, 7, Mark::Fold)]);
    // The text: the I-beam. Not over the editor: none.
    assert_eq!(host.cursor(Point::new(100.0, 10.0)), Some(Cursor::Text));
    // The numbers: the arrow. The column of the page's, which takes presses: the hand. The button
    // that folds: the hand where a region folds there, the arrow where none does.
    assert_eq!(host.cursor(Point::new(9.0, 10.0)), Some(Cursor::Arrow));
    assert_eq!(host.cursor(Point::new(20.0, 10.0)), Some(Cursor::Pointer));
    assert_eq!(host.cursor(Point::new(30.0, 10.0)), Some(Cursor::Pointer));
    assert_eq!(
        host.cursor(Point::new(30.0, 10.0 + 2.0 * ROW)),
        Some(Cursor::Arrow)
    );
    assert_eq!(host.cursor(Point::new(500.0, 10.0)), None);
}

#[test]
fn a_marker_says_its_own_cursor_and_the_dots_column_is_the_arrow_without_one() {
    let mut host = Host::new(page_with(EditorOptions::default()));
    host.load("a\nb");
    host.decorate(
        "m",
        vec![Decoration::new(
            0,
            0,
            Mark::Gutter(GutterMark {
                cursor: Some(Cursor::Grab),
                ..GutterMark::dot(Role::Red.solid())
            }),
        )],
    );
    assert_eq!(host.cursor(Point::new(3.0, 10.0)), Some(Cursor::Grab));
    assert_eq!(
        host.cursor(Point::new(3.0, 10.0 + ROW)),
        Some(Cursor::Arrow)
    );
}

#[test]
fn the_gutter_is_the_columns_the_page_lists_in_its_order_and_a_marker_stands_in_its_own() {
    let numbers_at = |options: EditorOptions| {
        let mut host = Host::new(page_with(options));
        host.load("mi");
        let texts = host.texts();
        let number = texts
            .iter()
            .find(|(text, _, _)| text == "1")
            .map(|(_, at, _)| at.x);
        let text = texts
            .iter()
            .find(|(text, _, _)| text == "mi")
            .map(|(_, at, _)| at.x);
        (number, text)
    };
    // The editor's: dots (10), numbers (18), buttons (13): the text is 4 past them.
    let (number, text) = numbers_at(EditorOptions::default());
    assert_eq!((number, text), (Some(10.0 + 18.0 - 4.0 - 7.0), Some(45.0)));
    // The page's: numbers at the left, a column of 6, the buttons.
    let (number, text) = numbers_at(columns());
    assert_eq!(
        (number, text),
        (Some(18.0 - 4.0 - 7.0), Some(18.0 + 6.0 + 13.0 + 4.0))
    );
    // With no buttons and no numbers: the column alone.
    let (number, text) = numbers_at(EditorOptions {
        gutters: vec![Column {
            width: Some(12.0),
            ..Column::new("breakpoints")
        }],
        ..EditorOptions::default()
    });
    assert_eq!((number, text), (None, Some(12.0 + 4.0)));
}

#[test]
fn a_bar_goes_along_every_line_of_its_range_and_a_dot_stands_on_the_first() {
    let mut host = Host::new(page_with(columns()));
    host.load("a\nb\nc\nd");
    // `b` and `c` (3..7, up to where `d` starts); a dot on `d`.
    host.decorate(
        "changes",
        vec![
            Decoration::new(2, 6, Mark::Gutter(marker(GutterShape::Bar, "changes"))),
            Decoration::new(6, 6, Mark::Gutter(marker(GutterShape::Dot, "changes"))),
        ],
    );
    let quads = host.quads();
    // The column of `changes` starts at 18; its 6 hold the bar (3) at the left of what the
    // pointer-over width (6) would take, so at 18.
    let bar = quads
        .iter()
        .find(|(rect, role)| *role == Some(Role::Green) && rect.h > ROW)
        .expect("a bar");
    assert_eq!(bar.0, Rect::new(18.0, 4.0 + ROW, 3.0, 2.0 * ROW));
    let dot = quads
        .iter()
        .find(|(rect, role)| *role == Some(Role::Green) && rect.h <= ROW)
        .expect("a dot")
        .0;
    assert_eq!(dot.y, 4.0 + 3.0 * ROW + (ROW - dot.h) / 2.0);
}

#[test]
fn lines_taken_away_are_a_triangle_on_the_edge_between_two_and_an_empty_range_is_a_place() {
    let mut host = Host::new(page_with(columns()));
    host.load("a\nb\nc");
    host.decorate(
        "changes",
        vec![
            Decoration::new(2, 2, Mark::Gutter(marker(GutterShape::Above, "changes"))),
            Decoration::new(3, 3, Mark::Gutter(marker(GutterShape::Below, "changes"))),
        ],
    );
    let figures: Vec<Rect> = host
        .paint(false)
        .items
        .iter()
        .filter_map(|item| match item {
            Paint::Figure(figure) => Some(figure.rect),
            _ => None,
        })
        .collect();
    assert_eq!(figures.len(), 2);
    // Above `b`'s line, and below it: the edges the row boundaries make, a triangle 9 tall.
    assert_eq!(figures[0].y + figures[0].h / 2.0, 4.0 + ROW);
    assert_eq!(figures[1].y + figures[1].h / 2.0, 4.0 + 2.0 * ROW);
}

#[test]
fn a_picture_from_the_bundle_is_tinted_in_its_column_and_the_rows_height() {
    let mut host = Host::new(page_with(columns()));
    host.load("a\nb");
    host.decorate(
        "bp",
        vec![Decoration::new(
            2,
            2,
            Mark::Gutter(marker(
                GutterShape::Image("assets/breakpoint.svg".into()),
                "changes",
            )),
        )],
    );
    let pictures: Vec<_> = host
        .paint(false)
        .items
        .iter()
        .filter_map(|item| match item {
            Paint::Image(picture) => Some(picture.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(pictures.len(), 1);
    assert_eq!(pictures[0].src, "assets/breakpoint.svg");
    assert_eq!(pictures[0].tint, Some(Role::Green.solid()));
    let rect = pictures[0].rect;
    assert!(
        rect.x >= 18.0 && rect.right() <= 24.0,
        "inside its column: {rect:?}"
    );
    assert!(
        rect.y >= 4.0 + ROW && rect.bottom() <= 4.0 + 2.0 * ROW,
        "on its line: {rect:?}"
    );
}

#[test]
fn a_press_in_a_column_of_the_pages_is_an_event_and_nothing_else_even_where_nothing_is() {
    let mut host = Host::new(page_with(columns()));
    host.load("a\nb\nc\nd");
    host.decorate(
        "changes",
        vec![Decoration::new(
            2,
            6,
            Mark::Gutter(marker(GutterShape::Bar, "changes")),
        )],
    );
    host.editors.take_events();
    host.caret_at(0);
    host.editors.take_events();
    // On the bar, on `c`'s line: the marker, the line pressed, the layer, and the cell.
    assert!(host.press(Point::new(20.0, 4.0 + 2.0 * ROW + 3.0), none(), 1));
    host.editors.release();
    let events = host.gutter_events();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(
        (event.column.as_str(), event.line, event.kind),
        ("changes", 2, GutterKind::Press)
    );
    assert_eq!((event.button, event.count), (Button::Left, 1));
    let hit = event.marker.as_ref().expect("the marker");
    assert_eq!(
        (hit.id.as_deref(), hit.layer.as_str(), hit.from, hit.to),
        (Some("m"), "changes", 2, 6)
    );
    assert_eq!(event.rect, Rect::new(18.0, 4.0 + 2.0 * ROW, 6.0, ROW));
    // The line was not selected and the caret did not move.
    assert_eq!(host.ranges(), vec![Range::caret(0)]);
    // On the first line, where there is none: an event with no marker, as well.
    assert!(host.press(Point::new(20.0, 6.0), none(), 1));
    host.editors.release();
    let events = host.gutter_events();
    assert_eq!(
        (events.len(), events[0].line, events[0].marker.is_none()),
        (1, 0, true)
    );
    // The numbers are as they were: the line is selected.
    assert!(host.press(Point::new(8.0, 4.0 + ROW + 3.0), none(), 1));
    host.editors.release();
    assert!(host.gutter_events().is_empty());
    assert_eq!(host.ranges(), vec![Range::new(2, 4)]);
}

#[test]
fn the_other_button_and_control_are_a_menu_and_a_column_that_hears_neither_is_not_told() {
    let events = |heard: Option<ColumnEvents>| {
        let mut options = columns();
        options.gutters[1].events = heard;
        let mut host = Host::new(page_with(options));
        host.load("a\nb");
        host.editors.take_events();
        let laid = host.laid.clone();
        let at = Point::new(20.0, 6.0);
        host.editors
            .press_other(&laid, at, (Button::Right, none(), 1), &mut Fake);
        host.editors
            .press(&laid, at, with(|m| m.control = true), 1, &mut Fake);
        host.editors.release();
        host.gutter_events()
    };
    let both = events(None);
    assert_eq!(both.len(), 2);
    assert_eq!(
        (both[0].button, both[1].mods.control),
        (Button::Right, true)
    );
    let only_press = events(Some(ColumnEvents {
        press: true,
        menu: false,
        hover: false,
    }));
    assert_eq!(only_press.len(), 0);
}

#[test]
fn the_pointer_coming_to_a_cell_and_going_is_told_to_a_column_that_hears_it_and_draws_its_preview()
{
    let mut options = columns();
    options.gutters[1].events = Some(ColumnEvents {
        press: true,
        menu: false,
        hover: true,
    });
    options.gutters[1].hover = Some(Preview {
        shape: GutterShape::Dot,
        color: Role::Red.alpha(0.4),
    });
    let mut host = Host::new(page_with(options));
    host.load("a\nb\nc");
    host.editors.take_events();
    let on_b = Point::new(20.0, 4.0 + ROW + 3.0);
    host.hover(Some(on_b));
    let events = host.gutter_events();
    assert_eq!(events.len(), 1);
    assert_eq!(
        (events[0].kind, events[0].line, events[0].column.as_str()),
        (GutterKind::Enter, 1, "changes")
    );
    // The preview is drawn in the empty cell, faint.
    let faint = |host: &mut Host| {
        host.quads()
            .iter()
            .filter(|(_, role)| *role == Some(Role::Red))
            .count()
    };
    assert_eq!(faint(&mut host), 1);
    // The same cell again says nothing; the next line's says it left one and came to the other.
    host.hover(Some(Point::new(21.0, 4.0 + ROW + 5.0)));
    assert!(host.gutter_events().is_empty());
    host.hover(Some(Point::new(20.0, 4.0 + 2.0 * ROW + 3.0)));
    let events = host.gutter_events();
    assert_eq!(
        events.iter().map(|e| (e.kind, e.line)).collect::<Vec<_>>(),
        vec![(GutterKind::Leave, 1), (GutterKind::Enter, 2)]
    );
    host.hover(None);
    assert_eq!(host.gutter_events().len(), 1);
    assert_eq!(faint(&mut host), 0);
    // A drawing only: the editor asks for a frame where the cell changed, and for none where it did not.
    host.hover(Some(on_b));
    assert!(host.editors.wants_redraw());
    host.hover(Some(Point::new(21.0, 4.0 + ROW + 4.0)));
    assert!(!host.editors.wants_redraw());
}

#[test]
fn a_bar_the_pointer_is_over_is_wider_and_lit_where_its_column_takes_events() {
    let mut host = Host::new(page_with(columns()));
    host.load("a\nb");
    host.decorate(
        "changes",
        vec![Decoration::new(
            0,
            3,
            Mark::Gutter(marker(GutterShape::Bar, "changes")),
        )],
    );
    let widths = |host: &mut Host| {
        host.quads()
            .iter()
            .filter(|(_, role)| *role == Some(Role::Green))
            .map(|(rect, _)| rect.w)
            .collect::<Vec<_>>()
    };
    assert_eq!(widths(&mut host), vec![3.0]);
    host.hover(Some(Point::new(20.0, 6.0)));
    // Wider, and a veil of the lit colour over it.
    let quads = host.quads();
    let green = quads
        .iter()
        .find(|(_, role)| *role == Some(Role::Green))
        .expect("the bar")
        .0;
    assert_eq!(green.w, 6.0);
    assert!(
        quads
            .iter()
            .any(|(rect, role)| *rect == green && *role == Some(Role::Foreground))
    );
    host.hover(None);
    assert_eq!(widths(&mut host), vec![3.0]);
}

#[test]
fn a_marker_in_a_region_that_is_folded_away_shows_on_the_line_the_fold_is_on() {
    let mut host = Host::new(page_with(columns()));
    host.load("fn {\n  a\n  b\n}\nend");
    host.decorate("folds", vec![Decoration::new(3, 13, Mark::Fold)]);
    host.decorate(
        "changes",
        vec![Decoration::new(
            7,
            8,
            Mark::Gutter(marker(GutterShape::Dot, "changes")),
        )],
    );
    let top = |host: &mut Host| {
        host.quads()
            .iter()
            .find(|(_, role)| *role == Some(Role::Green))
            .map(|(rect, _)| rect.y)
    };
    // Not folded: on `a`'s line, the second.
    assert!(top(&mut host).is_some_and(|y| y > 4.0 + ROW && y < 4.0 + 2.0 * ROW));
    assert!(host.press(Point::new(27.0, 10.0), none(), 1));
    host.editors.release();
    assert_eq!(
        host.editors.buffer(EDITOR).expect("it").folded(),
        &[(3, 13)]
    );
    // Folded: on the line of the fold, the first.
    assert!(top(&mut host).is_some_and(|y| y > 4.0 && y < 4.0 + ROW));
    // And a press there is of that marker.
    host.editors.take_events();
    assert!(host.press(Point::new(20.0, 6.0), none(), 1));
    let events = host.gutter_events();
    assert_eq!(
        events[0].marker.as_ref().map(|m| (m.from, m.to)),
        Some((7, 8))
    );
}

#[test]
fn the_overview_ruler_marks_where_the_lines_are_in_the_document_in_their_lanes() {
    let mut host = Host::new(page_with(columns()));
    let lines: Vec<String> = (0..20).map(|n| format!("l{n}")).collect();
    host.load(&lines.join("\n"));
    let ruler = |lane: RulerLane| {
        Mark::Ruler(RulerMark {
            color: Role::Blue.solid(),
            lane,
        })
    };
    // Lines 10 to 12 (3 of 20), full; line 0 on the left; 19 on the right.
    let start = |line: usize| {
        lines[..line]
            .iter()
            .map(|l| l.chars().count() + 1)
            .sum::<usize>()
    };
    host.decorate(
        "ruler",
        vec![
            Decoration::new(0, 1, ruler(RulerLane::Left)),
            Decoration::new(start(10), start(13), ruler(RulerLane::Full)),
            Decoration::new(start(19), start(19), ruler(RulerLane::Right)),
        ],
    );
    let marks: Vec<Rect> = host
        .quads()
        .iter()
        .filter(|(_, role)| *role == Some(Role::Blue))
        .map(|(rect, _)| rect)
        .copied()
        .collect();
    assert_eq!(marks.len(), 3);
    // The room is 200 tall for 20 rows: 10 each; 4 wide at 2 from the right edge, a third each lane.
    assert_eq!(marks[1], Rect::new(394.0, 100.0, 4.0, 30.0));
    assert_eq!((marks[0].x, marks[0].w), (marks[1].x, 1.0));
    assert_eq!((marks[2].x + marks[2].w), marks[1].right());
    assert!(marks[0].h >= 2.0 && marks[2].y <= 200.0 - marks[2].h);
}

#[test]
fn a_zone_opens_a_gap_after_its_line_that_the_text_after_goes_down_by_and_scrolls_with() {
    let zone = Id::named("peek");
    let page = move || {
        div().child(
            div()
                .editor(EDITOR, columns())
                .size_full()
                .child(div().id(zone).zone(zone).h(2.0 * ROW).bg(Role::Red.solid())),
        )
    };
    let mut host = Host::new(page);
    host.load("a\nb\nc");
    let before = host.editors.metrics(EDITOR).expect("laid out").content.h;
    host.decorate(
        "peek",
        vec![Decoration::new(
            2,
            2,
            Mark::Zone(Zone {
                id: zone,
                height: 2.0 * ROW,
                ..Zone::default()
            }),
        )],
    );
    let after = host.editors.metrics(EDITOR).expect("laid out").content.h;
    assert_eq!(after - before, 2.0 * ROW);
    // Where the gap is, in the content: after `b`'s row.
    let boxes = host.editors.zones(EDITOR);
    assert_eq!(
        boxes,
        vec![(zone, Rect::new(0.0, 4.0 + 2.0 * ROW, 400.0, 2.0 * ROW))]
    );
    // The page's element stands there, over the text of `c` that went down.
    let bounds = host.laid.bounds(zone).expect("the element is on the page");
    assert_eq!((bounds.y, bounds.h), (4.0 + 2.0 * ROW, 2.0 * ROW));
    // `c` is a gap lower.
    let c = host
        .texts()
        .iter()
        .find(|(text, _, _)| text == "c")
        .map(|(_, at, _)| at.y)
        .expect("c");
    assert_eq!(c, 4.0 + 4.0 * ROW + 2.0);
    // The gap goes when the decoration does, and the element with it.
    host.decorate("peek", vec![]);
    assert!(host.editors.zones(EDITOR).is_empty());
}

#[test]
fn a_zone_is_as_tall_as_the_page_said_and_not_as_the_rows_that_hold_it() {
    let zone = Id::named("peek");
    let height = 1.5 * ROW;
    let page = move || {
        div().child(
            div()
                .editor(EDITOR, columns())
                .size_full()
                .child(div().id(zone).zone(zone).h(height).bg(Role::Red.solid())),
        )
    };
    let mut host = Host::new(page);
    host.load("a\nb\nc");
    let before = host.editors.metrics(EDITOR).expect("laid out").content.h;
    host.decorate(
        "peek",
        vec![Decoration::new(
            2,
            2,
            Mark::Zone(Zone {
                id: zone,
                height,
                ..Zone::default()
            }),
        )],
    );
    let after = host.editors.metrics(EDITOR).expect("laid out").content.h;
    assert_eq!(after - before, height);
    assert_eq!(
        host.editors.zones(EDITOR),
        vec![(zone, Rect::new(0.0, 4.0 + 2.0 * ROW, 400.0, height))]
    );
    // `c` follows the gap at once.
    let c = host
        .texts()
        .iter()
        .find(|(text, _, _)| text == "c")
        .map(|(_, at, _)| at.y)
        .expect("c");
    assert_eq!(c, 4.0 + 2.0 * ROW + height + 2.0);
    // A press where `c` is finds `c` (what is over the gap is the page's).
    let at_c = Point::new(30.0, 4.0 + 2.0 * ROW + height + 8.0);
    assert!(host.press(at_c, none(), 1));
    host.editors.release();
    assert!(host.editors.buffer(EDITOR).expect("it").main().head >= 4);
}

#[test]
fn a_mark_on_the_ruler_stops_where_the_rows_of_its_line_do_and_the_gap_after_it_is_not_its_own() {
    let zone = Id::named("peek");
    let page = move || {
        div().child(
            div()
                .editor(EDITOR, columns())
                .size_full()
                .child(div().id(zone).zone(zone).h(3.0 * ROW).bg(Role::Red.solid())),
        )
    };
    let mut host = Host::new(page);
    let lines: Vec<String> = (0..10).map(|n| format!("l{n}")).collect();
    host.load(&lines.join("\n"));
    let ruler = Mark::Ruler(RulerMark {
        color: Role::Blue.solid(),
        lane: RulerLane::Full,
    });
    // Line 2 (offset 6) is the mark's, the gap is after it: 10 rows of text and 3 of the gap.
    host.decorate("ruler", vec![Decoration::new(6, 6, ruler)]);
    let marked = |host: &mut Host| -> Rect {
        host.quads()
            .iter()
            .filter(|(_, role)| *role == Some(Role::Blue))
            .map(|(rect, _)| *rect)
            .next()
            .expect("a mark")
    };
    let without = marked(&mut host);
    host.decorate(
        "peek",
        vec![Decoration::new(
            6,
            6,
            Mark::Zone(Zone {
                id: zone,
                height: 3.0 * ROW,
                ..Zone::default()
            }),
        )],
    );
    let with = marked(&mut host);
    // One row of 13: as tall as the room is for them, and no taller for the gap.
    assert!((with.h - 200.0 / 13.0).abs() < 0.01, "{with:?}");
    assert!(without.h > with.h);
}

#[test]
fn what_stands_in_a_zone_scrolls_with_the_text_and_is_cut_by_the_room() {
    let zone = Id::named("peek");
    let page = move || {
        div().child(
            div()
                .editor(EDITOR, columns())
                .size_full()
                .child(div().id(zone).zone(zone).h(2.0 * ROW).bg(Role::Red.solid())),
        )
    };
    let mut host = Host::new(page);
    let lines: Vec<String> = (0..30).map(|n| format!("line {n}")).collect();
    host.load(&lines.join("\n"));
    host.decorate(
        "peek",
        vec![Decoration::new(
            14,
            14,
            Mark::Zone(Zone {
                id: zone,
                height: 2.0 * ROW,
                ..Zone::default()
            }),
        )],
    );
    let at = |host: &Host| host.laid.bounds(zone).expect("the element").y;
    // Line 2 (14 is in `line 2`) ends at 4 + 3 rows.
    let rest = at(&host);
    assert_eq!(rest, 4.0 + 3.0 * ROW);
    host.scrolls.ask(pencere_view::ScrollTo {
        id: EDITOR,
        x: None,
        y: Some(30.0),
        animate: false,
    });
    host.frame();
    host.frame();
    assert_eq!(at(&host), rest - 30.0);
    // Scrolled out of the room's top: it is where the text went, and what draws it is cut there.
    host.scrolls.ask(pencere_view::ScrollTo {
        id: EDITOR,
        x: None,
        y: Some(150.0),
        animate: false,
    });
    host.frame();
    host.frame();
    assert_eq!(at(&host), rest - 150.0);
    let list = host.paint(false);
    let red = list.items.iter().find_map(|item| match item {
        Paint::Quad(quad) if quad.fill.and_then(|f| f.role()) == Some(Role::Red) => Some(quad.clip),
        _ => None,
    });
    assert!(red.is_none_or(|clip| clip.y >= 0.0 && clip.bottom() <= 200.0));
}

#[test]
fn the_gutter_below_the_text_and_beside_a_gap_is_no_cell_and_a_marker_in_the_editors_column_is_none()
 {
    let zone = Id::named("peek");
    let page = move || {
        div().child(
            div()
                .editor(EDITOR, columns())
                .size_full()
                .child(div().id(zone).zone(zone).h(2.0 * ROW).bg(Role::Red.solid())),
        )
    };
    let mut host = Host::new(page);
    host.load("a\nb\nc");
    host.decorate(
        "m",
        vec![
            Decoration::new(0, 0, Mark::Gutter(marker(GutterShape::Dot, "numbers"))),
            Decoration::new(
                2,
                2,
                Mark::Zone(Zone {
                    id: zone,
                    height: 2.0 * ROW,
                    ..Zone::default()
                }),
            ),
        ],
    );
    host.editors.take_events();
    // Over the gap (after `b`): the page's. Below `c`: no line. On `c`, shown a gap lower: the line.
    assert!(
        !host.press(Point::new(20.0, 4.0 + 2.0 * ROW + 5.0), none(), 1)
            || host.gutter_events().is_empty()
    );
    // (The zone's element is wide as the room: what is over the gap is the page's, not the editor's.)
    assert_eq!(host.cursor(Point::new(20.0, 4.0 + 2.0 * ROW + 5.0)), None);
    assert_eq!(
        host.cursor(Point::new(20.0, 4.0 + 7.0 * ROW)),
        Some(Cursor::Arrow)
    );
    host.editors.take_events();
    host.press(Point::new(20.0, 4.0 + 4.0 * ROW + 5.0), none(), 1);
    assert_eq!(
        host.gutter_events()
            .iter()
            .map(|e| e.line)
            .collect::<Vec<_>>(),
        vec![2]
    );
    host.editors.release();
    // A marker in a column of the editor's own is not drawn, and is not there to be pressed.
    host.hover(Some(Point::new(9.0, 6.0)));
    assert_eq!(host.cursor(Point::new(9.0, 6.0)), Some(Cursor::Arrow));
    assert!(host.gutter_events().is_empty());
}

#[test]
fn a_zone_that_takes_the_name_of_another_at_the_same_line_and_height_is_the_one_named() {
    let (a, b) = (Id::named("a"), Id::named("b"));
    let mut host = Host::new(page_with(columns()));
    host.load("a\nb");
    host.decorate(
        "z",
        vec![Decoration::new(
            0,
            0,
            Mark::Zone(Zone {
                id: a,
                height: ROW,
                ..Zone::default()
            }),
        )],
    );
    assert_eq!(
        host.editors
            .zones(EDITOR)
            .iter()
            .map(|z| z.0)
            .collect::<Vec<_>>(),
        vec![a]
    );
    host.decorate(
        "z",
        vec![Decoration::new(
            0,
            0,
            Mark::Zone(Zone {
                id: b,
                height: ROW,
                ..Zone::default()
            }),
        )],
    );
    assert_eq!(
        host.editors
            .zones(EDITOR)
            .iter()
            .map(|z| z.0)
            .collect::<Vec<_>>(),
        vec![b]
    );
}

#[test]
fn what_the_pointer_is_over_is_found_again_when_the_text_scrolls_under_it() {
    let mut options = columns();
    options.gutters[1].events = Some(ColumnEvents {
        press: true,
        menu: false,
        hover: true,
    });
    let mut host = Host::new(page_with(options));
    let lines: Vec<String> = (0..40).map(|n| format!("l{n}")).collect();
    host.load(&lines.join("\n"));
    host.editors.take_events();
    host.hover(Some(Point::new(20.0, 4.0 + 2.0 * ROW + 3.0)));
    assert_eq!(
        host.gutter_events()
            .iter()
            .map(|e| (e.kind, e.line))
            .collect::<Vec<_>>(),
        vec![(GutterKind::Enter, 2)]
    );
    // Scrolled three rows: what is under the pointer is line 5, and the page is told so.
    host.scrolls.ask(pencere_view::ScrollTo {
        id: EDITOR,
        x: None,
        y: Some(3.0 * ROW),
        animate: false,
    });
    host.settle();
    assert_eq!(
        host.gutter_events()
            .iter()
            .map(|e| (e.kind, e.line))
            .collect::<Vec<_>>(),
        vec![(GutterKind::Leave, 2), (GutterKind::Enter, 5)]
    );
}

// --- What the pointer rests on, and the ranges the page named ------------------------------------

/// An editor with no gutter that tells the page of the pointer's rests.
fn hearing() -> EditorOptions {
    EditorOptions {
        events: EditorEvents {
            hover: true,
            ..EditorEvents::default()
        },
        ..plain()
    }
}

impl Host {
    /// The pointer moved to `at` (or went), and the frame of `seconds` later is drawn.
    fn rest(&mut self, at: Option<Point>) {
        let laid = self.laid.clone();
        self.editors.rest(&laid, at, none(), self.now, &mut Fake);
    }

    /// `seconds` pass and the frame is drawn: what the editors tell is what is returned.
    fn wait(&mut self, seconds: f64) -> Vec<pencere_view::edit::HoverEvent> {
        self.now += seconds;
        let laid = self.laid.clone();
        self.editors.tick(&laid, self.now, &mut Fake);
        self.editors
            .take_events()
            .into_iter()
            .filter_map(|event| match event {
                EditorEvent::Hover(hover) => Some(hover),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn the_pointer_resting_on_a_word_is_told_once_after_the_delay_and_its_going_at_once() {
    let mut host = Host::new(page_with(hearing()));
    host.load("alpha beta\ngamma");
    host.editors.set_focus(None);
    // Over `alpha` (x 4 to 35).
    host.rest(Some(Point::new(10.0, 10.0)));
    let due = host.editors.rest_due().expect("it is due");
    assert!((due - (host.now + 0.3)).abs() < 1e-6, "{due}");
    assert!(host.wait(0.29).is_empty());
    let told = host.wait(0.02);
    assert_eq!(told.len(), 1);
    let dwell = &told[0];
    assert_eq!(dwell.kind, HoverKind::Dwell);
    assert_eq!((dwell.from, dwell.to, dwell.at, dwell.line), (0, 5, 0, 0));
    assert_eq!(dwell.rect, Rect::new(4.0, 4.0, 31.0, ROW));
    assert_eq!(
        dwell.version,
        host.editors.buffer(EDITOR).expect("it").version()
    );
    // Not again, and no more to wake for.
    assert!(host.wait(1.0).is_empty());
    assert_eq!(host.editors.rest_due(), None);
    // Moving within the word is nothing; to another, the first's leaving and no more.
    host.rest(Some(Point::new(20.0, 12.0)));
    assert!(host.wait(0.0).is_empty());
    host.rest(Some(Point::new(50.0, 10.0)));
    let told = host.wait(0.0);
    assert_eq!(told.len(), 1);
    assert_eq!(
        (told[0].kind, told[0].from, told[0].to),
        (HoverKind::Leave, 0, 5)
    );
    // The second word was not rested on long enough: leaving it tells nothing.
    host.rest(None);
    assert!(host.wait(1.0).is_empty());
    assert_eq!(host.editors.rest_due(), None);
}

#[test]
fn nothing_rests_on_a_gap_past_the_end_of_a_line_a_space_or_a_drag_and_a_page_that_does_not_hear_is_told_none()
 {
    let mut host = Host::new(page_with(hearing()));
    host.load("alpha beta\ngamma");
    host.editors.set_focus(None);
    // Past the end of `gamma`, below the last line, on the space between the words.
    for at in [(300.0, 30.0), (30.0, 150.0), (36.0, 10.0)] {
        host.rest(Some(Point::new(at.0, at.1)));
        assert_eq!(host.editors.rest_due(), None, "{at:?}");
        assert!(host.wait(1.0).is_empty(), "{at:?}");
    }
    // With a button held (a drag) it does not rest.
    assert!(host.press(Point::new(8.0, 10.0), none(), 1));
    host.rest(Some(Point::new(10.0, 10.0)));
    assert_eq!(host.editors.rest_due(), None);
    host.editors.release();
    host.rest(Some(Point::new(10.0, 10.0)));
    assert!(host.editors.rest_due().is_some());
    // The page that asked for none is told none.
    let mut host = Host::new(page_with(plain()));
    host.load("alpha");
    host.rest(Some(Point::new(10.0, 10.0)));
    assert_eq!(host.editors.rest_due(), None);
    assert!(host.wait(1.0).is_empty());
}

#[test]
fn a_key_a_press_an_edit_or_a_scroll_ends_the_rest_and_none_comes_before_the_pointer_moves() {
    let long: Vec<String> = (0..40).map(|n| format!("word{n}")).collect();
    let text = long.join("\n");
    let at = Point::new(10.0, 10.0);
    // A key, after the page was told: its leaving, and nothing more until the pointer moves.
    let mut host = Host::new(page_with(hearing()));
    host.load(&text);
    host.rest(Some(at));
    assert_eq!(host.wait(0.4).len(), 1);
    host.editors.set_focus(Some(EDITOR));
    host.key(Key::Right, none());
    let told = host.wait(0.0);
    assert_eq!(
        told.iter().map(|t| t.kind).collect::<Vec<_>>(),
        [HoverKind::Leave]
    );
    assert!(host.wait(2.0).is_empty());
    host.rest(Some(at));
    assert_eq!(host.wait(0.4).len(), 1);
    // A press before the delay: nothing is told of it at all.
    host.rest(None);
    host.wait(0.0);
    host.rest(Some(at));
    assert!(host.wait(0.1).is_empty());
    assert!(host.press(Point::new(60.0, 10.0), none(), 1));
    host.editors.release();
    assert!(host.wait(2.0).is_empty());
    // An edit under the pointer ends it.
    host.rest(Some(at));
    assert_eq!(host.wait(0.4).len(), 1);
    let laid = host.laid.clone();
    host.editors.type_text(&laid, "x", &mut Fake, host.now);
    host.settle();
    let told = host.wait(0.0);
    assert!(
        told.iter().all(|t| t.kind == HoverKind::Leave) && told.len() == 1,
        "{told:?}"
    );
    // The text scrolled under a pointer that has not moved: its rest ends, and it is told again once
    // it moves.
    host.rest(Some(at));
    assert_eq!(host.wait(0.4).len(), 1);
    host.scrolls.ask(pencere_view::ScrollTo {
        id: EDITOR,
        x: None,
        y: Some(100.0),
        animate: false,
    });
    host.settle();
    let told = host.wait(0.0);
    assert_eq!(
        told.iter().map(|t| t.kind).collect::<Vec<_>>(),
        [HoverKind::Leave]
    );
    assert!(host.wait(2.0).is_empty());
    host.rest(Some(at));
    assert_eq!(host.wait(0.4).len(), 1);
}

fn naming() -> impl Fn() -> Element {
    move || {
        div().child(
            div().editor(EDITOR, plain()).size_full().child(
                div()
                    .id(POPUP)
                    .size(60.0, 40.0)
                    .bg(Role::Red.solid())
                    .anchor(EDITOR.anchor("here"), Side::Below),
            ),
        )
    }
}

#[test]
fn a_range_the_page_named_is_a_box_an_overlay_stands_by_and_follows_the_text_and_the_scroll() {
    let lines: Vec<String> = (0..40).map(|n| format!("line {n}")).collect();
    let mut host = Host::new(naming());
    host.load(&lines.join("\n"));
    let name =
        |from: usize, to: usize| vec![Decoration::new(from, to, Mark::Anchor("here".into()))];
    // `line 2`'s `ne 2`: lines are 7 characters with their break, `line 2` starts at 14.
    host.decorate("names", name(16, 20));
    let boxed = host
        .laid
        .bounds(EDITOR.anchor("here"))
        .expect("the range is a box");
    // After `li` (3 + 3) and the room's 4, two rows down; `ne 2` is four characters of 7.
    assert_eq!(
        (boxed.x, boxed.y, boxed.h),
        (4.0 + 3.0 + 3.0, 4.0 + 2.0 * ROW, ROW)
    );
    assert_eq!(boxed.w, 28.0);
    // The overlay is under it.
    let popup = host.laid.bounds(POPUP).expect("the popup");
    assert_eq!((popup.x, popup.y), (boxed.x, boxed.bottom()));
    // A line added above: the box and the overlay go down with it, in a layout of the same tree.
    host.editors.set_focus(Some(EDITOR));
    host.caret_at(0);
    let laid = host.laid.clone();
    host.editors.type_text(&laid, "x\n", &mut Fake, host.now);
    host.settle();
    let moved = host.laid.bounds(EDITOR.anchor("here")).expect("the box");
    assert_eq!(moved.y, boxed.y + ROW);
    assert_eq!(host.laid.bounds(POPUP).expect("popup").y, moved.bottom());
    // Scrolled out of the room, what stands by it is not shown; back in, it is.
    let shown = |host: &mut Host| {
        host.quads()
            .iter()
            .any(|(_, role)| *role == Some(Role::Red))
    };
    assert!(shown(&mut host));
    host.scrolls.ask(pencere_view::ScrollTo {
        id: EDITOR,
        x: None,
        y: Some(600.0),
        animate: false,
    });
    host.settle();
    assert!(!shown(&mut host));
    host.scrolls.ask(pencere_view::ScrollTo {
        id: EDITOR,
        x: None,
        y: Some(0.0),
        animate: false,
    });
    host.settle();
    assert!(shown(&mut host));
    // A name said again is the later's; one the page took away is no box, and the overlay is not
    // placed by it.
    host.decorate("more", name(40, 44));
    let later = host.laid.bounds(EDITOR.anchor("here")).expect("the later");
    assert_ne!(later, moved);
    host.decorate("names", vec![]);
    host.decorate("more", vec![]);
    assert!(host.laid.bounds(EDITOR.anchor("here")).is_none());
    // The word at a place is the double click's: `line` of the text now `x`, a break, `line 0`.
    assert_eq!(host.editors.word_at(EDITOR, 3), Some((2, 6)));
    assert_eq!(host.editors.word_at(EDITOR, 1_000_000), None);
}

#[test]
fn a_decoration_with_a_name_is_told_in_the_hits_of_the_rest_over_it_and_names_are_held_to_64() {
    let mut host = Host::new(page_with(hearing()));
    host.load("alpha beta");
    host.editors.set_focus(None);
    let named = |from: usize, to: usize, id: &str| {
        Decoration::new(
            from,
            to,
            Mark::Style(Look {
                id: Some(id.into()),
                ..Look::default()
            }),
        )
    };
    host.decorate("diagnostics", vec![named(0, 5, "e1"), named(2, 3, "e2")]);
    host.decorate(
        "links",
        vec![Decoration::new(6, 10, Mark::Style(Look::default()))],
    );
    host.rest(Some(Point::new(10.0, 10.0)));
    let told = host.wait(0.4);
    assert_eq!(told.len(), 1);
    let ids: Vec<(&str, &str)> = told[0]
        .hits
        .iter()
        .map(|hit| (hit.layer.as_str(), hit.id.as_str()))
        .collect();
    assert_eq!(ids, [("diagnostics", "e1")]);
    // Over `beta`, whose style has no name: no hits.
    host.rest(Some(Point::new(50.0, 10.0)));
    let told = host.wait(0.4);
    assert_eq!(
        told.last().map(|t| (t.kind, t.hits.len())),
        Some((HoverKind::Dwell, 0))
    );
    // More than 64 names are refused.
    let many: Vec<Decoration> = (0..65)
        .map(|n| Decoration::new(0, 1, Mark::Anchor(format!("a{n}"))))
        .collect();
    host.decorate("many", many);
    let refused = host
        .editors
        .take_events()
        .into_iter()
        .any(|event| matches!(event, EditorEvent::Refused { .. }));
    assert!(refused);
}

// --- A press that is the page's, links, and the editor's keys as the page changed them -----------

fn cmd() -> Mods {
    with(|m| m.command = true)
}

/// An editor that hears of a ⌘-press, which is the page's, and puts the caret as it did for the rest.
fn linking() -> EditorOptions {
    EditorOptions {
        events: EditorEvents {
            hover: false,
            target: true,
        },
        gestures: vec![(
            Gesture {
                button: Button::Left,
                mods: cmd(),
                count: 1,
            },
            Pressing::Target,
        )],
        ..plain()
    }
}

impl Host {
    fn mods(&mut self, mods: Mods) {
        let laid = self.laid.clone();
        self.editors.set_mods(&laid, mods, self.now, &mut Fake);
    }

    fn targets(&mut self) -> Vec<pencere_view::edit::TargetEvent> {
        self.editors
            .take_events()
            .into_iter()
            .filter_map(|event| match event {
                EditorEvent::Target(target) => Some(target),
                _ => None,
            })
            .collect()
    }
}

fn link(id: &str) -> Mark {
    Mark::Style(Look {
        color: Some(Role::Red.solid()),
        id: Some(id.into()),
        held: Some(cmd()),
        over: true,
        ..Look::default()
    })
}

#[test]
fn a_press_that_is_the_pages_is_told_and_moves_no_caret_and_a_link_it_is_on_is_named() {
    let mut host = Host::new(page_with(linking()));
    host.load("alpha beta");
    host.editors.set_focus(None);
    host.decorate("links", vec![Decoration::new(0, 5, link("def:1"))]);
    host.editors.take_events();
    // A press with no ⌘ is the editor's: the caret goes.
    assert!(host.press(Point::new(50.0, 10.0), none(), 1));
    host.editors.release();
    let placed = host.ranges();
    assert_eq!(placed.len(), 1);
    host.targets();
    // With ⌘ it is the page's: told, on a word that is no link as the word; the caret stays.
    host.mods(cmd());
    assert!(host.press(Point::new(50.0, 10.0), cmd(), 1));
    host.editors.release();
    assert_eq!(host.ranges(), placed);
    let told = host.targets();
    let press = told
        .iter()
        .find(|t| t.kind == TargetKind::Press)
        .expect("a press");
    assert_eq!(
        (press.source, press.from, press.to, press.id.as_deref()),
        (TargetSource::Word, 6, 10, None)
    );
    // On a link the page drew (held, with a name), as the link, by its name.
    assert!(host.press(Point::new(10.0, 10.0), cmd(), 1));
    host.editors.release();
    let told = host.targets();
    let press = told
        .iter()
        .find(|t| t.kind == TargetKind::Press)
        .expect("a press");
    assert_eq!(
        (press.source, press.from, press.to, press.id.as_deref()),
        (TargetSource::Link, 0, 5, Some("def:1"))
    );
    assert_eq!(host.ranges(), placed);
}

#[test]
fn the_pointer_coming_to_a_word_with_the_pages_modifiers_held_is_told_once_for_each_and_taken_back()
{
    let mut host = Host::new(page_with(linking()));
    host.load("alpha beta gamma");
    host.editors.set_focus(None);
    host.editors.take_events();
    // Not held: nothing, over a word.
    host.rest(Some(Point::new(10.0, 10.0)));
    assert!(host.targets().is_empty());
    // Held: the word under the pointer is told.
    host.mods(cmd());
    let told = host.targets();
    assert_eq!(told.len(), 1);
    assert_eq!(
        (told[0].kind, told[0].from, told[0].to),
        (TargetKind::Enter, 0, 5)
    );
    // The pointer to another word: the first's leaving now, and the second's coming when the time
    // has passed (links.delay, 0.05): at most once in that.
    host.rest(Some(Point::new(50.0, 10.0)));
    let told = host.targets();
    assert_eq!(
        told.iter().map(|t| t.kind).collect::<Vec<_>>(),
        [TargetKind::Leave]
    );
    assert!(
        host.editors.rest_due().is_some(),
        "the coming waits its time"
    );
    host.now += 0.06;
    let laid = host.laid.clone();
    host.editors.tick(&laid, host.now, &mut Fake);
    let told = host.targets();
    assert_eq!(told.len(), 1);
    assert_eq!(
        (told[0].kind, told[0].from, told[0].to),
        (TargetKind::Enter, 6, 10)
    );
    // The modifier let go: taken back.
    host.mods(none());
    let told = host.targets();
    assert_eq!(
        told.iter().map(|t| t.kind).collect::<Vec<_>>(),
        [TargetKind::Leave]
    );
}

#[test]
fn a_look_that_is_held_and_over_is_drawn_only_while_both_are_so_and_gives_the_hand() {
    let mut host = Host::new(page_with(linking()));
    host.load("alpha beta");
    host.editors.set_focus(None);
    host.decorate("links", vec![Decoration::new(0, 5, link("l"))]);
    let red = |host: &mut Host| {
        host.texts()
            .iter()
            .any(|(text, _, role)| text == "alpha" && *role == Some(Role::Red))
    };
    assert!(!red(&mut host), "no ⌘");
    host.mods(cmd());
    assert!(!red(&mut host), "⌘ and the pointer not over it");
    host.rest(Some(Point::new(10.0, 10.0)));
    assert!(red(&mut host), "⌘ and the pointer over it");
    let laid = host.laid.clone();
    assert_eq!(
        host.editors.cursor_at(&laid, Point::new(10.0, 10.0)),
        Some(Cursor::Pointer)
    );
    host.rest(Some(Point::new(50.0, 10.0)));
    assert!(!red(&mut host), "the pointer went");
    host.rest(Some(Point::new(10.0, 10.0)));
    host.mods(none());
    assert!(!red(&mut host), "⌘ let go");
    assert_eq!(
        host.editors.cursor_at(&laid, Point::new(10.0, 10.0)),
        Some(Cursor::Text)
    );
}

#[test]
fn a_press_is_taken_as_the_page_says_and_as_the_editor_does_where_it_says_nothing() {
    let options = EditorOptions {
        gestures: vec![
            (
                Gesture {
                    button: Button::Left,
                    mods: with(|m| m.alt = true),
                    count: 1,
                },
                Pressing::AddCaret,
            ),
            (
                Gesture {
                    button: Button::Left,
                    mods: with(|m| m.shift = true),
                    count: 1,
                },
                Pressing::Nothing,
            ),
        ],
        ..plain()
    };
    let mut host = Host::new(page_with(options));
    host.load("alpha beta");
    assert!(host.press(Point::new(10.0, 10.0), none(), 1));
    host.editors.release();
    // ⌥ adds a caret now, where it started a column.
    assert!(host.press(Point::new(50.0, 10.0), with(|m| m.alt = true), 1));
    host.editors.release();
    assert_eq!(host.ranges().len(), 2);
    // ⇧ does nothing.
    let before = host.ranges();
    assert!(host.press(Point::new(30.0, 10.0), with(|m| m.shift = true), 1));
    host.editors.release();
    assert_eq!(host.ranges(), before);
    // ⌘ adds, as it did.
    assert!(host.press(Point::new(20.0, 10.0), cmd(), 1));
    host.editors.release();
    assert_eq!(host.ranges().len(), 3);
}

#[test]
fn the_keys_the_page_changed_do_what_it_says_and_a_chord_waits_for_its_next_key() {
    let shortcut = |words: &str| pencere_view::Shortcut::parse(words).expect("keys");
    let options = EditorOptions {
        keymap: vec![
            // ⌘A is nothing of the editor's now, and ⌘D is Select All.
            Keybinding {
                keys: vec![shortcut("cmd+a")],
                action: None,
            },
            Keybinding {
                keys: vec![shortcut("cmd+d")],
                action: Some(KeyAction::Command(EditCommand::SelectAll)),
            },
            // ⌘K ⌘U is Undo.
            Keybinding {
                keys: vec![shortcut("cmd+k"), shortcut("cmd+u")],
                action: Some(KeyAction::Command(EditCommand::Undo)),
            },
        ],
        ..plain()
    };
    let mut host = Host::new(page_with(options));
    host.load("alpha beta");
    host.editors.set_focus(Some(EDITOR));
    host.caret_at(2);
    assert_eq!(host.key(Key::Character('a'), cmd()), Route::Page);
    assert_eq!(host.key(Key::Character('d'), cmd()), Route::Done);
    assert_eq!(host.ranges(), vec![Range::new(0, 10)]);
    // The chord: ⌘K waits (the page is told, and nothing is typed), ⌘U finishes it.
    host.caret_at(2);
    host.editors.take_events();
    assert_eq!(host.key(Key::Character('k'), cmd()), Route::Done);
    let pending: Vec<Vec<String>> = host
        .editors
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            EditorEvent::KeymapPending { keys, .. } => Some(keys),
            _ => None,
        })
        .collect();
    assert_eq!(pending, vec![vec!["cmd+k".to_string()]]);
    assert!(host.editors.rest_due().is_some(), "the wait has its time");
    assert_eq!(host.key(Key::Character('u'), cmd()), Route::Done);
    // A key that is no key of the chord ends it and does nothing.
    assert_eq!(host.key(Key::Character('k'), cmd()), Route::Done);
    assert_eq!(host.key(Key::Character('x'), none()), Route::Done);
    assert_eq!(host.text(), "alpha beta", "nothing was typed");
    // And the time gives it up.
    assert_eq!(host.key(Key::Character('k'), cmd()), Route::Done);
    host.editors.take_events();
    host.now += 1.5;
    let laid = host.laid.clone();
    host.editors.tick(&laid, host.now, &mut Fake);
    assert!(host.editors.take_events().iter().any(|event| matches!(
        event,
        EditorEvent::KeymapPending { keys, .. } if keys.is_empty()
    )));
    // The keys it did not change are the editor's.
    assert_eq!(host.key(Key::Left, none()), Route::Done);
}

// --- Text that is not the document's, in its lines ---------------------------------------------------

fn hint(side: WidgetSide, text: &str) -> Mark {
    Mark::Widget(Widget {
        parts: vec![WidgetPart {
            text: text.into(),
            ..WidgetPart::default()
        }],
        side,
        ..Widget::default()
    })
}

#[test]
fn a_widget_takes_room_in_its_line_before_or_after_the_caret_and_is_no_part_of_the_text() {
    let mut host = Host::new(page_with(plain()));
    host.load("alpha beta");
    host.editors.set_focus(Some(EDITOR));
    let x_of = |host: &mut Host, word: &str| {
        host.texts()
            .iter()
            .find(|(text, _, _)| text.trim() == word)
            .map(|(_, at, _)| at.x)
            .expect("the word")
    };
    // `ab` (14 wide) and 3 of room each side, before `beta`: the word goes 20 across, the text
    // is the same.
    host.decorate(
        "hints",
        vec![Decoration::new(6, 6, hint(WidgetSide::Before, "ab"))],
    );
    assert_eq!(x_of(&mut host, "beta"), 4.0 + 38.0 + 20.0);
    assert_eq!(x_of(&mut host, "alpha"), 4.0);
    assert_eq!(host.text(), "alpha beta");
    // The caret at 6 is after the hint (`ab |beta`) and the caret at 5 is where it was.
    let laid = host.laid.clone();
    let _ = laid;
    host.caret_at(6);
    let caret = host
        .editors
        .metrics(EDITOR)
        .and_then(|m| m.caret)
        .expect("a caret");
    assert_eq!(caret.x, 4.0 + 38.0 + 20.0);
    // After a character: the caret is before the hint (`x|: int`).
    host.decorate(
        "hints",
        vec![Decoration::new(5, 5, hint(WidgetSide::After, "cd"))],
    );
    host.caret_at(5);
    let caret = host
        .editors
        .metrics(EDITOR)
        .and_then(|m| m.caret)
        .expect("a caret");
    assert_eq!(caret.x, 4.0 + 31.0);
    assert_eq!(x_of(&mut host, "beta"), 4.0 + 31.0 + 20.0);
    // A press on the hint puts the caret at the nearest place: before it or after, not in it.
    assert!(host.press(Point::new(4.0 + 31.0 + 4.0, 10.0), none(), 1));
    host.editors.release();
    let head = host.ranges()[0].head;
    assert!(head == 5 || head == 6, "{head}");
    // Select all and copy the document, with no hint in it.
    host.editors.take_events();
    assert_eq!(host.text(), "alpha beta");
}

#[test]
fn the_group_says_when_a_widget_shows_by_the_modifiers_and_one_at_a_lines_end_is_cut_where_the_room_ends()
 {
    let options = EditorOptions {
        widgets: vec![(
            "hints".to_string(),
            pencere_view::edit::WidgetGroup {
                show: pencere_view::edit::WidgetShow::UnlessHeld,
                ..pencere_view::edit::WidgetGroup::default()
            },
        )],
        ..plain()
    };
    let mut host = Host::new(page_with(options));
    host.load("alpha beta");
    let named = |side, text: &str| {
        Mark::Widget(Widget {
            group: Some("hints".into()),
            ..match hint(side, text) {
                Mark::Widget(widget) => widget,
                _ => unreachable!(),
            }
        })
    };
    host.decorate(
        "hints",
        vec![Decoration::new(6, 6, named(WidgetSide::Before, "ab"))],
    );
    let x_of = |host: &mut Host, word: &str| {
        host.texts()
            .iter()
            .find(|(text, _, _)| text.trim() == word)
            .map(|(_, at, _)| at.x)
            .expect("the word")
    };
    assert_eq!(x_of(&mut host, "beta"), 4.0 + 38.0 + 20.0);
    // ⌃⌥ held: hidden (unless held), the line as it was.
    host.mods(with(|m| {
        m.control = true;
        m.alt = true;
    }));
    host.frame();
    assert!(
        host.texts()
            .iter()
            .any(|(text, at, _)| text == "alpha beta" && at.x == 4.0)
    );
    host.mods(none());
    host.frame();
    assert_eq!(x_of(&mut host, "beta"), 4.0 + 38.0 + 20.0);
    // At the line's end, a gap beyond the text, and cut where the room (400 wide) ends.
    let long = "x".repeat(120);
    host.decorate(
        "hints",
        vec![Decoration::new(0, 0, named(WidgetSide::End, &long))],
    );
    let drawn: Vec<(String, Point, Option<Role>)> = host.texts();
    let end = drawn
        .iter()
        .find(|(text, _, _)| text.starts_with('x'))
        .expect("the widget at the end");
    assert!(end.0.ends_with('…'), "{}", end.0);
    assert!(end.1.x > 4.0 + 70.0, "{:?}", end.1);
}

#[test]
fn a_widget_with_a_name_is_what_the_pointer_rests_on_and_what_a_press_on_it_is() {
    let mut host = Host::new(page_with(EditorOptions {
        events: EditorEvents {
            hover: true,
            target: true,
        },
        ..plain()
    }));
    host.load("alpha beta");
    host.editors.set_focus(None);
    host.decorate(
        "hints",
        vec![Decoration::new(
            6,
            6,
            Mark::Widget(Widget {
                id: Some("type:int".into()),
                ..match hint(WidgetSide::Before, "ab") {
                    Mark::Widget(widget) => widget,
                    _ => unreachable!(),
                }
            }),
        )],
    );
    host.editors.take_events();
    // The hint is at 42 to 62; the hand over it, and a rest on it is of the widget.
    let over = Point::new(50.0, 10.0);
    host.rest(Some(over));
    let told = host.wait(0.4);
    assert_eq!(told.len(), 1);
    assert_eq!(
        (told[0].over, told[0].id.as_deref()),
        (pencere_view::edit::HoverOver::Widget, Some("type:int"))
    );
    let laid = host.laid.clone();
    assert_eq!(host.editors.cursor_at(&laid, over), Some(Cursor::Pointer));
    // A press on it is the page's, with no modifier, and moves no caret.
    let before = host.ranges();
    assert!(host.press(over, none(), 1));
    host.editors.release();
    let told = host.targets();
    let press = told
        .iter()
        .find(|t| t.kind == TargetKind::Press)
        .expect("a press");
    assert_eq!(
        (press.source, press.id.as_deref()),
        (TargetSource::Widget, Some("type:int"))
    );
    assert_eq!(host.ranges(), before);
}

// --- A gap above a line (a code lens) ------------------------------------------------------------------

#[test]
fn a_gap_above_a_line_is_between_it_and_the_line_before_or_over_the_first_and_goes_with_a_fold() {
    let lens = Id::named("lens");
    let page = move || {
        div().child(
            div()
                .editor(EDITOR, columns())
                .size_full()
                .child(div().id(lens).zone(lens).h(ROW).bg(Role::Red.solid())),
        )
    };
    let mut host = Host::new(page);
    host.load("a\nb\nc\nd");
    let y_of = |host: &mut Host, text: &str| {
        host.texts()
            .iter()
            .find(|(t, _, _)| t == text)
            .map(|(_, at, _)| at.y)
            .expect("the text")
    };
    let b_before = y_of(&mut host, "b");
    // Above `c` (offset 4): between `b` and `c`; `c` goes down by the gap, `b` does not.
    let above = |at: usize| {
        vec![Decoration::new(
            at,
            at,
            Mark::Zone(Zone {
                id: lens,
                height: ROW,
                side: ZoneSide::Above,
            }),
        )]
    };
    host.decorate("lens", above(4));
    assert_eq!(y_of(&mut host, "b"), b_before);
    assert_eq!(
        host.editors.zones(EDITOR),
        vec![(lens, Rect::new(0.0, 4.0 + 2.0 * ROW, 400.0, ROW))]
    );
    assert_eq!(y_of(&mut host, "c"), 4.0 + 3.0 * ROW + 2.0);
    // Above the first line: over everything, which goes down.
    host.decorate("lens", above(0));
    assert_eq!(
        host.editors.zones(EDITOR),
        vec![(lens, Rect::new(0.0, 4.0, 400.0, ROW))]
    );
    assert_eq!(y_of(&mut host, "a"), 4.0 + ROW + 2.0);
    // A press on the first row finds `a`, the gap is the page's.
    assert!(host.press(Point::new(100.0, 4.0 + ROW + 8.0), none(), 1));
    host.editors.release();
    assert!(host.ranges()[0].head <= 1);
    // A line folded away has no gap to show: above `c` while `b..c` are folded is none.
    host.decorate("lens", above(4));
    host.decorate("folds", vec![Decoration::new(2, 5, Mark::Fold)]);
    host.editors.set_focus(Some(EDITOR));
    let laid = host.laid.clone();
    host.editors.ask(
        &laid,
        EDITOR,
        EditorAsk::Command(EditCommand::FoldAll),
        &mut Fake,
        host.now,
    );
    host.settle();
    assert!(
        host.editors.zones(EDITOR).is_empty(),
        "{:?}",
        host.editors.zones(EDITOR)
    );
}

#[test]
fn a_widget_on_a_later_line_is_at_a_place_of_the_document_and_a_wrapped_line_takes_the_rows_its_widgets_make()
 {
    let mut host = Host::new(page_with(EditorOptions {
        events: EditorEvents {
            hover: true,
            target: true,
        },
        ..plain()
    }));
    host.load("alpha beta\ngamma delta");
    host.editors.set_focus(None);
    host.decorate(
        "hints",
        vec![Decoration::new(
            17,
            17,
            Mark::Widget(Widget {
                id: Some("h".into()),
                ..match hint(WidgetSide::Before, "ab") {
                    Mark::Widget(widget) => widget,
                    _ => unreachable!(),
                }
            }),
        )],
    );
    host.editors.take_events();
    // `delta` is at 17 in the document, 6 in its line; the hint is before it on the second row.
    let at = Point::new(64.0, 4.0 + ROW + 10.0);
    host.rest(Some(at));
    let told = host.wait(0.4);
    assert_eq!(told.len(), 1);
    assert_eq!(
        (told[0].at, told[0].from, told[0].line),
        (17, 17, 1),
        "{:?}",
        told[0]
    );

    // A line that wraps takes the rows its widgets make it take, and the next line starts below them.
    let mut host = Host::new(page_with(EditorOptions {
        wrap: true,
        ..plain()
    }));
    let long = "word ".repeat(70);
    host.load(&format!("{long}\nnext"));
    let rows_of_text = |host: &mut Host| {
        let content = host.editors.metrics(EDITOR).expect("laid out").content.h;
        ((content - 8.0) / ROW).round() as usize
    };
    let before = rows_of_text(&mut host);
    host.decorate(
        "hints",
        vec![Decoration::new(
            0,
            0,
            Mark::Widget(Widget {
                parts: vec![WidgetPart {
                    text: "x".repeat(40),
                    ..WidgetPart::default()
                }],
                side: WidgetSide::Before,
                ..Widget::default()
            }),
        )],
    );
    host.frame();
    let after = rows_of_text(&mut host);
    assert!(after >= before, "{before} {after}");
    let next = host
        .texts()
        .iter()
        .find(|(text, _, _)| text == "next")
        .map(|(_, at, _)| at.y)
        .expect("the next line");
    let last = host
        .texts()
        .iter()
        .filter(|(text, _, _)| text.starts_with("word") || text.starts_with(' '))
        .map(|(_, at, _)| at.y)
        .fold(0.0_f32, f32::max);
    assert!(
        next > last,
        "the next line is below the rows of the wrapped one: {next} {last}"
    );
}
