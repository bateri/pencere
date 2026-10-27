//! The editors of a page, as a host keeps them: laid out in their rooms, under the pointer and the
//! keys, scrolled as a box is, and drawn in their places. The host's measure is a stand-in whose
//! letters are of three widths (`i` narrow, `m` wide), so that what is across is not a column.

// A test fails by panicking, and indexes what it has just built.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use pencere_editor::{EditorEvent, Editors, Route};
use pencere_view::edit::{
    Caret, CaretShape, Change, Decorate, Decoration, EditCommand, EditorAsk, EditorColors,
    EditorOptions, Keys, Mark, Numbers, Range, Tint,
};
use pencere_view::{
    Command, DrawList, Element, Font, Id, Key, Laid, Measure, Metrics, Mods, Paint, Point, Rect,
    Role, Scrolls, Shortcut, Side, Size, div, layout,
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
        if self.editors.wants_layout() {
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
    // text.
    let caret = host
        .editors
        .metrics(EDITOR)
        .and_then(|m| m.caret)
        .expect("a caret");
    assert_eq!(caret, Rect::new(4.0 + 14.0, 4.0 + ROW + LEAD, 2.0, LINE));
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
            && q.rect == Rect::new(4.0 + 11.0, 4.0 + LEAD, 2.0, LINE))
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
    assert_eq!(before.y, 4.0 + LEAD + LINE);
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
    // `w` is 11 across, on the second row: the caret is as tall as the text, in the row's middle.
    assert_eq!(caret, Rect::new(10.0 + 11.0, 10.0 + 30.0 + 7.0, 2.0, LINE));
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
    let block = Rect::new(4.0, 4.0 + LEAD, 11.0, LINE);
    assert!(host.quads().contains(&(block, Some(Role::Accent))));
    assert!(
        host.texts()
            .contains(&("m".into(), Point::new(4.0, 4.0 + LEAD), Some(Role::Magenta)))
    );
    // At the end of the line it is a space wide, and over no character.
    host.caret_at(2);
    let block = Rect::new(4.0 + 14.0, 4.0 + LEAD, 7.0, LINE);
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
    let line = Rect::new(4.0 + 11.0, 4.0 + LEAD + LINE - 3.0, 3.0, 3.0);
    assert!(host.quads().contains(&(line, Some(Role::Cursor))));
    // What stands by the caret stands by the text it is under, not on it.
    let caret = host
        .editors
        .metrics(EDITOR)
        .and_then(|m| m.caret)
        .expect("a caret");
    assert_eq!(caret, Rect::new(4.0 + 11.0, 4.0 + LEAD, 3.0, LINE));
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
