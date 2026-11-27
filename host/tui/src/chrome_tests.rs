//! What the host draws over the page, in pixels: the draw list is drawn as a picture of the window and
//! what is in it, and where, is what the person sees. The faces are the ones the program carries,
//! so the same list makes the same pixels on any machine.

use std::collections::HashMap;

use pencere_drive::Field;
use pencere_drive::shot::{Fonts, Palette, raster};
use pencere_view::{
    Color, DrawList, FieldKind, Fill, Id, Key, Menu, MenuEntry, Mods, Paint, Panel, Point, Rect,
    Role, Shortcut,
};

use crate::chrome::{self, Paper};
use crate::field::TextField;
use crate::overlay::{LinkBar, MenuBox, PanelBar};
use crate::pixel::scale_for;

/// A cell of the terminal, in pixels, and the window of 60 by 12 cells it makes.
const CELL: (u32, u32) = (10, 20);
const WINDOW: (u32, u32) = (600, 240);

fn faces() -> (Fonts, f32) {
    let fonts = Fonts::carried();
    let scale = scale_for(&fonts, CELL.0);
    (fonts.scaled(scale), scale)
}

fn paper<'a>(fonts: &'a Fonts, scale: f32) -> Paper<'a> {
    Paper {
        fonts,
        scale,
        cell: (CELL.0 as f32, CELL.1 as f32),
        window: Rect::new(0.0, 0.0, WINDOW.0 as f32, WINDOW.1 as f32),
        foot: (WINDOW.1 - CELL.1) as f32,
    }
}

/// The pixels of a window with these items in it, on the ground.
fn drawn(items: &[Paint], fonts: &Fonts) -> Vec<u8> {
    let list = DrawList {
        items: items.to_vec(),
    };
    raster(&list, WINDOW, fonts, None, Palette::Dark)
        .expect("a picture")
        .rgba()
        .to_vec()
}

/// How many pixels of `rgba` inside `area` (pixels) are not the ground's.
fn ink_in(rgba: &[u8], ground: &[u8], area: Rect) -> usize {
    let (x0, y0) = (area.x.max(0.0) as u32, area.y.max(0.0) as u32);
    let (x1, y1) = (
        (area.right().ceil() as u32).min(WINDOW.0),
        (area.bottom().ceil() as u32).min(WINDOW.1),
    );
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let at = 4 * (y * WINDOW.0 + x) as usize;
            if rgba[at..at + 4] != ground[at..at + 4] {
                n += 1;
            }
        }
    }
    n
}

/// How many pixels of `rgba` outside `area` are not the ground's, past `margin` pixels all round.
fn ink_outside(rgba: &[u8], ground: &[u8], area: Rect, margin: f32) -> usize {
    let whole = ink_in(
        rgba,
        ground,
        Rect::new(0.0, 0.0, WINDOW.0 as f32, WINDOW.1 as f32),
    );
    let inner = Rect::new(
        area.x - margin,
        area.y - margin,
        area.w + 2.0 * margin,
        area.h + 2.0 * margin,
    );
    whole - ink_in(rgba, ground, inner)
}

fn texts(items: &[Paint]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| match item {
            Paint::Text(run) => Some(run.text.clone()),
            _ => None,
        })
        .collect()
}

fn fills(items: &[Paint], role: Role) -> Vec<Rect> {
    items
        .iter()
        .filter_map(|item| match item {
            Paint::Quad(quad) if quad.fill == Some(Color::from(role)) => Some(quad.rect),
            _ => None,
        })
        .collect()
}

fn room() -> Field {
    Field {
        id: Id::named("find"),
        kind: FieldKind::Line,
        rect: Rect::new(20.0, 30.0, 150.0, 18.0),
        text: Rect::new(20.0, 30.0, 150.0, 18.0),
        hint: Some("Find".to_owned()),
        label: None,
        covered: false,
    }
}

/// The room of `room()` in pixels at `scale`.
fn room_px(scale: f32) -> Rect {
    let r = room().rect;
    Rect::new(r.x * scale, r.y * scale, r.w * scale, r.h * scale)
}

#[test]
fn a_field_with_no_text_says_its_hint_dimmed_in_its_room() {
    let (fonts, scale) = faces();
    let items = chrome::fields(&paper(&fonts, scale), &[room()], &HashMap::new(), None);
    assert!(
        texts(&items).iter().any(|t| t == "Find"),
        "{:?}",
        texts(&items)
    );
    let ground = drawn(&[], &fonts);
    let rgba = drawn(&items, &fonts);
    let area = room_px(scale);
    assert!(
        ink_in(&rgba, &ground, area) > 20,
        "the hint is in the picture"
    );
    assert_eq!(
        ink_outside(&rgba, &ground, area, 1.0),
        0,
        "and nothing of it is outside the room"
    );
    // Dim: its colour is the kit's, not the text's.
    let dim = items.iter().any(|item| {
        matches!(item, Paint::Text(run) if run.text == "Find" && run.color == Color::from(Role::Dim))
    });
    assert!(dim, "the hint is in the dim colour");
}

#[test]
fn a_field_with_text_says_the_text_and_not_the_hint_and_the_caret_is_at_its_end() {
    let (fonts, scale) = faces();
    let mut text = TextField::new(FieldKind::Line);
    text.fill(&Fill::text("hello world"));
    let texts_by = HashMap::from([(room().id, text)]);
    let items = chrome::fields(&paper(&fonts, scale), &[room()], &texts_by, Some(room().id));
    let said = texts(&items);
    assert!(said.iter().any(|t| t == "hello world"), "{said:?}");
    assert!(
        !said.iter().any(|t| t == "Find"),
        "no hint over text: {said:?}"
    );
    let ground = drawn(&[], &fonts);
    let rgba = drawn(&items, &fonts);
    assert!(ink_in(&rgba, &ground, room_px(scale)) > 100);
    assert_eq!(ink_outside(&rgba, &ground, room_px(scale), 1.0), 0);
    // The caret: a thin upright bar the height of a line, past the last character.
    let caret = fills(&items, Role::Cursor);
    assert_eq!(caret.len(), 1, "one caret: {caret:?}");
    let caret = caret[0];
    assert!(caret.w <= 4.0 && caret.h >= 10.0, "{caret:?}");
    let end = room_px(scale).x + 11.0 * CELL.0 as f32;
    assert!(
        (caret.x - end).abs() <= 2.0,
        "at the end of the text: {caret:?} for {end}"
    );
    // A field that does not have the keyboard has none.
    let items = chrome::fields(&paper(&fonts, scale), &[room()], &texts_by, None);
    assert!(fills(&items, Role::Cursor).is_empty());
}

#[test]
fn a_line_longer_than_its_room_is_slid_so_that_the_caret_is_in_it() {
    let (fonts, scale) = faces();
    let long = "abcdefghij".repeat(3);
    let mut text = TextField::new(FieldKind::Line);
    text.fill(&Fill::text(long.as_str()));
    let texts_by = HashMap::from([(room().id, text)]);
    let items = chrome::fields(&paper(&fonts, scale), &[room()], &texts_by, Some(room().id));
    let said = texts(&items);
    let shown = said.first().expect("the words");
    let cols = (room_px(scale).w / CELL.0 as f32).floor() as usize;
    assert!(shown.chars().count() < cols, "{shown:?} in {cols} columns");
    assert!(
        long.ends_with(shown.as_str()),
        "the end of the line is what is in view: {shown:?}"
    );
    let caret = fills(&items, Role::Cursor)[0];
    assert!(
        caret.x >= room_px(scale).x && caret.right() <= room_px(scale).right(),
        "{caret:?}"
    );
    let rgba = drawn(&items, &fonts);
    assert_eq!(
        ink_outside(&rgba, &drawn(&[], &fonts), room_px(scale), 1.0),
        0
    );
}

#[test]
fn what_is_selected_in_a_field_stands_on_a_ground_under_its_text() {
    let (fonts, scale) = faces();
    let mut text = TextField::new(FieldKind::Line);
    text.fill(&Fill::text("hello world").select(0..5));
    let texts_by = HashMap::from([(room().id, text)]);
    let items = chrome::fields(&paper(&fonts, scale), &[room()], &texts_by, Some(room().id));
    let selection = fills(&items, Role::Selection);
    assert_eq!(selection.len(), 1, "{selection:?}");
    let width = 5.0 * CELL.0 as f32;
    assert!(
        (selection[0].w - width).abs() <= 2.0 && (selection[0].x - room_px(scale).x).abs() <= 2.0,
        "five characters from the start: {:?}",
        selection[0]
    );
    // Under the text in the order of the list.
    let at = |pick: fn(&Paint) -> bool| items.iter().position(pick).expect("an item");
    let ground =
        at(|item| matches!(item, Paint::Quad(q) if q.fill == Some(Color::from(Role::Selection))));
    let words = at(|item| matches!(item, Paint::Text(_)));
    assert!(ground < words);
}

#[test]
fn a_field_a_popover_lies_over_is_not_drawn_and_a_secret_is_dots() {
    let (fonts, scale) = faces();
    let covered = Field {
        covered: true,
        ..room()
    };
    let mut text = TextField::new(FieldKind::Line);
    text.fill(&Fill::text("hidden"));
    let texts_by = HashMap::from([(room().id, text)]);
    assert!(chrome::fields(&paper(&fonts, scale), &[covered], &texts_by, None).is_empty());
    let secret = Field {
        kind: FieldKind::Secret,
        ..room()
    };
    let mut text = TextField::new(FieldKind::Secret);
    text.fill(&Fill::text("hunter2"));
    let texts_by = HashMap::from([(room().id, text)]);
    let said = texts(&chrome::fields(
        &paper(&fonts, scale),
        &[secret],
        &texts_by,
        None,
    ));
    assert!(said.iter().all(|t| !t.contains("hunter2")), "{said:?}");
    assert!(
        said.iter()
            .any(|t| t.chars().all(|c| c == '\u{2022}') && t.chars().count() == 7)
    );
}

fn item(title: &str, keys: Option<&str>, enabled: bool) -> MenuEntry {
    MenuEntry::Item {
        title: title.to_owned(),
        keys: keys.and_then(Shortcut::parse),
        enabled,
    }
}

fn menu() -> Menu {
    Menu {
        at: Point::new(0.0, 0.0),
        entries: vec![
            item("Copy", Some("ctrl+c"), true),
            MenuEntry::Separator,
            item("Rename", None, false),
            item("Delete", None, true),
        ],
        serial: 3,
    }
}

#[test]
fn a_menu_is_a_card_with_its_lines_in_the_cells_the_page_asked_for() {
    let (fonts, scale) = faces();
    let mut shown = MenuBox::new(&menu(), (10, 3), (60, 12));
    let area = shown.area();
    let items = shown.paint(&paper(&fonts, scale));
    let said = texts(&items);
    for title in ["Copy", "Rename", "Delete"] {
        assert!(said.iter().any(|t| t == title), "{title}: {said:?}");
    }
    assert!(said.iter().any(|t| t == "Ctrl+C"), "its shortcut: {said:?}");
    let px = Rect::new(
        f32::from(area.col) * CELL.0 as f32,
        f32::from(area.row) * CELL.1 as f32,
        f32::from(area.cols) * CELL.0 as f32,
        f32::from(area.rows) * CELL.1 as f32,
    );
    let ground = drawn(&[], &fonts);
    let rgba = drawn(&items, &fonts);
    assert!(
        ink_in(&rgba, &ground, px) > 500,
        "the card is in the picture"
    );
    // The card's shadow is past its edge by a few pixels, and nothing is past that.
    assert_eq!(ink_outside(&rgba, &ground, px, 3.0 * CELL.1 as f32), 0);
    // Each line's text is in the row of cells its line is: the page's own entries, counted with the
    // separator, are the lines in order.
    for (title, line) in [("Copy", 0.0), ("Rename", 2.0), ("Delete", 3.0)] {
        let row = f32::from(area.row) + 1.0 + line;
        let at = items
            .iter()
            .find_map(|item| match item {
                Paint::Text(run) if run.text == title => Some(run.origin),
                _ => None,
            })
            .expect("the line");
        assert!(
            at.y >= row * CELL.1 as f32 - 2.0 && at.y < (row + 1.0) * CELL.1 as f32,
            "{title} is in the row {row}: {at:?}"
        );
    }
    // A line the pointer is over (or the arrows chose) is on a ground of its own, and a line that
    // cannot be picked is dim.
    assert!(fills(&items, Role::Selection).is_empty());
    assert!(shown.hover(area.col + 2, area.row + 1));
    let over = fills(&shown.paint(&paper(&fonts, scale)), Role::Selection);
    assert_eq!(over.len(), 1, "{over:?}");
    assert!(over[0].y >= (f32::from(area.row) + 1.0) * CELL.1 as f32 - 1.0);
    let dim = items.iter().any(|item| {
        matches!(item, Paint::Text(run) if run.text == "Rename" && run.color == Color::from(Role::Dim))
    });
    assert!(dim, "a line that is no use is dim");
    // And the key that chooses is the menu's: the line after the first.
    let _ = shown.key(Key::Down, Mods::default());
}

#[test]
fn the_line_of_a_panel_and_the_bar_of_a_link_are_a_row_of_cells_at_the_foot_of_the_window() {
    let (fonts, scale) = faces();
    let screen = (60, 12);
    let mut bar = PanelBar::new(Panel::Pick { multiple: false });
    bar.paste("~/notes.md");
    let items = bar.paint(&paper(&fonts, scale), screen);
    let said = texts(&items);
    assert!(
        said.iter().any(|t| t.contains("Open")),
        "its word: {said:?}"
    );
    assert!(
        said.iter().any(|t| t.contains("~/notes.md")),
        "its text: {said:?}"
    );
    let foot = Rect::new(0.0, 11.0 * CELL.1 as f32, WINDOW.0 as f32, CELL.1 as f32);
    let ground = drawn(&[], &fonts);
    let rgba = drawn(&items, &fonts);
    assert!(
        ink_in(&rgba, &ground, foot) > 1000,
        "the bar is in the picture"
    );
    assert_eq!(
        ink_outside(&rgba, &ground, foot, 0.0),
        0,
        "and only at the foot"
    );
    assert_eq!(
        fills(&items, Role::Cursor).len(),
        1,
        "the caret of the line"
    );

    let link = LinkBar::new("https://example.com/a/page".to_owned(), false);
    let items = link.paint(&paper(&fonts, scale), screen);
    let said = texts(&items);
    assert!(
        said.iter()
            .any(|t| t.contains("https://example.com/a/page")),
        "{said:?}"
    );
    let rgba = drawn(&items, &fonts);
    assert!(ink_in(&rgba, &ground, foot) > 1000);
    assert_eq!(ink_outside(&rgba, &ground, foot, 0.0), 0);
    // The address is underlined, as a link is.
    assert!(items.iter().any(|item| {
        matches!(item, Paint::Text(run) if run.underline && run.text.contains("https://"))
    }));
}

/// A box of the page's, filled with the ground: for the tests of where a strip at the foot starts.
fn boxed(x: f32, y: f32, w: f32, h: f32) -> Paint {
    let rect = Rect::new(x, y, w, h);
    let fills = Role::Raised(1);
    Paint::Quad(pencere_view::Quad {
        rect,
        radii: [0.0; 4],
        fill: Some(Color::from(fills)),
        gradient: None,
        border: None,
        shadow: None,
        clip: rect,
    })
}

#[test]
fn the_strip_a_page_has_at_the_foot_is_the_highest_box_across_the_window_that_ends_there() {
    let (width, height) = (WINDOW.0 as f32, WINDOW.1 as f32);
    let window = Rect::new(0.0, 0.0, width, height);
    let row = CELL.1 as f32;
    let status = boxed(0.0, height - 32.0, width, 32.0);
    // The window's own ground, a key in the strip, a box across the window that does not end at the
    // foot, and one that ends there but not across: none is the strip.
    let others = [
        boxed(0.0, 0.0, width, height),
        boxed(10.0, height - 26.0, 40.0, 20.0),
        boxed(0.0, 40.0, width, 30.0),
        boxed(0.0, height - 30.0, width / 2.0, 30.0),
    ];
    let mut items = others.to_vec();
    items.push(status);
    assert_eq!(
        chrome::foot_of(&items, window, row),
        height - 32.0,
        "the strip starts where its box does"
    );
    assert_eq!(
        chrome::foot_of(&others, window, row),
        height - row,
        "where the page has none the bar's own row is the foot"
    );
    // A strip no higher than the bar's row is covered by the row.
    let low = [boxed(0.0, height - 12.0, width, 12.0)];
    assert_eq!(chrome::foot_of(&low, window, row), height - row);
    // The ground of a bar is taken up to it, and no further down than the window.
    let (fonts, scale) = faces();
    let mut paper = paper(&fonts, scale);
    paper.foot = height - 32.0;
    let bar = paper.cells(0, 11, 60, 1);
    let ground = paper.over_foot(bar);
    assert_eq!((ground.y, ground.bottom()), (height - 32.0, height));
    paper.foot = height - row;
    assert_eq!(paper.over_foot(bar), bar);
}
