//! The rows of a frame that are said to be rows of the frame before it are, to the pixel: two lists
//! drawn from the start, one a page scrolled by a part of a pixel from the other, and every row the
//! one says is the other's is the other's.

#![allow(clippy::expect_used, clippy::indexing_slicing)]

use pencere_view::{
    Border, DrawList, Family, Font, Gradient, Paint, Point, Quad, Rect, Role, Run, Shadow,
};

use crate::shot::{Fonts, Palette, Side, raster, reach, slide};

const SIZE: (u32, u32) = (400, 300);
/// The room of the lines: what they are cut to.
const ROOM: Rect = Rect::new(100.0, 40.0, 300.0, 200.0);
const WINDOW: Rect = Rect::new(0.0, 0.0, 400.0, 300.0);

/// What a page has that is not like the lines of its editor.
#[derive(Clone, Copy, Default)]
struct Extras {
    /// A line with a line under its words.
    underline: bool,
    /// A card with a shadow over the room, which does not scroll.
    shadow: bool,
    /// A box that moves with the lines and is cut by the room where it goes past it.
    selection: bool,
    /// Translucent boxes with edges between pixels.
    veil: bool,
    /// A bar at the right edge of the room, which is not where the lines are.
    bar: bool,
    /// A line of words in the room that stands still.
    fixed: bool,
    /// A room whose edges are between pixels.
    between: bool,
    /// Boxes with a border and round corners that move with the lines.
    cards: bool,
    /// Boxes with a gradient that move with the lines.
    gradient: bool,
}

struct Seed(u32);

impl Seed {
    fn next(&mut self, most: u32) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) % most
    }
}

fn quad(rect: Rect, fill: pencere_view::Color, radii: f32, clip: Rect) -> Paint {
    Paint::Quad(Quad {
        rect,
        radii: [radii; 4],
        fill: Some(fill),
        gradient: None,
        border: None,
        shadow: None,
        clip: clip.into(),
    })
}

fn words(x: f32, y: f32, text: &str, underline: bool, clip: Rect) -> Paint {
    Paint::Text(Run {
        origin: Point::new(x, y),
        text: text.to_owned(),
        font: Font {
            family: Family::Terminal,
            size: 16.0,
            weight: 400,
            italic: false,
        },
        color: Role::Foreground.solid(),
        opacity: 1.0,
        grounds: Vec::new(),
        clip: clip.into(),
        underline,
        strike: false,
    })
}

fn line(n: usize) -> String {
    const WORDS: [&str; 7] = ["fn", "let", "match", "return", "impl", "struct", "where"];
    format!(
        "{} item_{}({}, {}) {{ // {:x}{}",
        WORDS[n % 7],
        n * 31 % 97,
        n * 7 % 13,
        "xyzw".chars().nth(n % 4).unwrap_or('q'),
        n * 2_654_435_761 % 4096,
        // Characters no carried face has are drawn as boxes, and one with an accent is drawn.
        if n % 9 == 4 {
            " \u{6f22}\u{1f600} \u{e9}g"
        } else {
            ""
        }
    )
}

/// A page: a ground, a side of lines that stand still, a room with its ground and its lines (and
/// the numbers beside them), each `row` pixels below the one before it and the room scrolled `scroll`
/// pixels, and what `extras` say.
fn page(scroll: f32, row: f32, extras: Extras) -> DrawList {
    let room = if extras.between {
        Rect::new(100.0, 40.3, 300.0, 199.6)
    } else {
        ROOM
    };
    let gutter = Rect::new(room.x, room.y, 30.0, room.h);
    let mut items = vec![
        quad(WINDOW, Role::Background.solid(), 0.0, WINDOW),
        quad(
            Rect::new(0.0, 0.0, 100.0, 300.0),
            Role::Raised(1).solid(),
            0.0,
            WINDOW,
        ),
        quad(room, Role::Raised(2).solid(), 0.0, room),
    ];
    for k in 0..6 {
        items.push(words(
            8.0,
            10.0 + k as f32 * 40.0,
            &format!("side {k}"),
            false,
            Rect::new(0.0, 0.0, 100.0, 300.0),
        ));
    }
    if extras.fixed {
        items.push(words(140.0, 120.0, "this one stands still", false, room));
    }
    let text_clip = Rect::new(130.0, room.y, 270.0, room.h);
    for n in 0..60 {
        let y = room.y + 4.0 + n as f32 * row - scroll;
        if y + row <= room.y || y >= room.bottom() {
            continue;
        }
        items.push(words(
            134.0,
            y,
            &line(n),
            extras.underline && n % 5 == 2,
            text_clip,
        ));
        items.push(words(104.0, y, &format!("{n:>3}"), false, gutter));
        if extras.selection && n % 4 == 1 {
            items.push(quad(
                Rect::new(134.0, y, 120.0, row),
                Role::Selection.solid(),
                3.0,
                text_clip,
            ));
        }
        if extras.gradient && n % 6 == 0 {
            items.push(Paint::Quad(Quad {
                rect: Rect::new(236.0, y, 80.0, 3.0 * row),
                radii: [0.0; 4],
                fill: None,
                gradient: Some(Gradient {
                    angle: 180.0,
                    from: Role::Raised(3).solid(),
                    to: Role::Accent.solid(),
                }),
                border: None,
                shadow: None,
                clip: text_clip.into(),
            }));
        }
        if extras.cards && n % 6 == 3 {
            items.push(Paint::Quad(Quad {
                rect: Rect::new(236.2, y + 2.2, 80.4, 2.5 * row),
                radii: [5.0; 4],
                fill: Some(Role::Raised(3).solid()),
                gradient: None,
                border: Some(Border {
                    width: 3.0,
                    color: Role::Border.solid(),
                }),
                shadow: None,
                clip: text_clip.into(),
            }));
        }
        if extras.veil && n % 3 == 0 {
            items.push(quad(
                Rect::new(135.5, y + 0.3, 90.0, row - 0.6),
                Role::Accent.alpha(0.3),
                0.0,
                text_clip,
            ));
        }
    }
    if extras.bar {
        items.push(quad(
            Rect::new(392.0, room.y + scroll * 0.4, 6.0, 60.0),
            Role::Accent.solid(),
            0.0,
            room,
        ));
    }
    if extras.shadow {
        items.push(Paint::Quad(Quad {
            rect: Rect::new(180.0, 90.0, 120.0, 60.0),
            radii: [6.0; 4],
            fill: Some(Role::Raised(3).solid()),
            gradient: None,
            border: Some(Border {
                width: 1.0,
                color: Role::Border.solid(),
            }),
            shadow: Some(Shadow {
                dx: 0.0,
                dy: 4.0,
                blur: 10.0,
                color: Role::Shadow.alpha(0.5),
            }),
            clip: WINDOW.into(),
        }));
    }
    DrawList { items }
}

/// Which rows of `new` are said to be rows of `old`, and a check that they are, in the columns said:
/// the rows drawn from the start. How many rows were said.
fn said(old: &DrawList, new: &DrawList, fonts: &Fonts, what: &str) -> usize {
    let reaches =
        |list: &DrawList| -> Vec<_> { list.items.iter().map(|item| reach(item, fonts)).collect() };
    let (before, now) = (reaches(old), reaches(new));
    let Some(slid) = slide(
        Side {
            list: old,
            reach: &before,
        },
        Side {
            list: new,
            reach: &now,
        },
        fonts,
        SIZE,
        &[(0, SIZE.1)],
    ) else {
        return 0;
    };
    let (a, b) = (
        raster(old, SIZE, fonts, None, Palette::Dark).expect("a picture"),
        raster(new, SIZE, fonts, None, Palette::Dark).expect("a picture"),
    );
    let width = SIZE.0 as usize * 4;
    let (from, to) = (slid.columns.0 as usize * 4, slid.columns.1 as usize * 4);
    let mut count = 0;
    for (row, shift) in slid.rows.iter().enumerate() {
        let Some(shift) = shift else {
            continue;
        };
        let source = row as i64 + i64::from(*shift);
        assert!(
            (0..i64::from(SIZE.1)).contains(&source),
            "{what}: row {row} is said to be row {source}"
        );
        let source = source as usize;
        assert_eq!(
            &b.rgba()[row * width + from..row * width + to],
            &a.rgba()[source * width + from..source * width + to],
            "{what}: row {row} is said to be row {source} of the frame before, in the columns {:?}",
            slid.columns
        );
        count += 1;
    }
    count
}

/// Whole pixels, and parts of them, down and up, a row of a whole pixels and of parts: rows of one
/// frame said to be rows of another are, whatever the page has besides its lines.
#[test]
fn the_rows_said_to_be_the_rows_before_are_the_pixels_drawn_from_the_start() {
    let fonts = Fonts::carried();
    let extras = [
        Extras::default(),
        Extras {
            underline: true,
            ..Extras::default()
        },
        Extras {
            shadow: true,
            bar: true,
            ..Extras::default()
        },
        Extras {
            selection: true,
            veil: true,
            fixed: true,
            ..Extras::default()
        },
        Extras {
            underline: true,
            shadow: true,
            selection: true,
            veil: true,
            bar: true,
            fixed: true,
            between: false,
            cards: true,
            gradient: true,
        },
        Extras {
            selection: true,
            veil: true,
            cards: true,
            between: true,
            ..Extras::default()
        },
        Extras {
            underline: true,
            between: true,
            bar: true,
            cards: true,
            ..Extras::default()
        },
    ];
    let mut seed = Seed(11);
    let mut most = 0;
    for (variant, extras) in extras.into_iter().enumerate() {
        for row in [20.0, 21.6, 17.35, 24.0, 19.5] {
            for _ in 0..6 {
                // Scrolled from where it was by lines and by a part of a line, down or up.
                let before = seed.next(300) as f32 / 4.0;
                let by = (1 + seed.next(5)) as f32 * row + seed.next(100) as f32 / 50.0 - 1.0;
                for delta in [by, -by.min(before)] {
                    let (old, new) = (page(before, row, extras), page(before + delta, row, extras));
                    let what = format!("variant {variant}, row {row}, from {before}, by {delta}");
                    most = most.max(said(&old, &new, &fonts, &what));
                }
            }
        }
    }
    assert!(most > 40, "rows are found: {most} at the most");
}

/// A page that scrolled by a whole number of rows of pixels has its rows found where its lines are, in
/// most of the room; the rows by a clip's edge, and the rows a shadow reaches, are not.
#[test]
fn rows_of_a_page_scrolled_by_whole_pixels_are_found_but_not_where_a_shadow_or_a_clip_is() {
    let fonts = Fonts::carried();
    let plain = Extras::default();
    let (old, new) = (page(0.0, 20.0, plain), page(40.0, 20.0, plain));
    let count = said(&old, &new, &fonts, "two rows of text up");
    assert!(
        count > 120,
        "most of the room (200 rows) is found: {count} rows"
    );
    let shadowed = Extras {
        shadow: true,
        ..Extras::default()
    };
    // A box with a gradient (a third of the room's lines are in one) is not one fill across: the rows it
    // reaches are not found, and what is found is still the pixels drawn.
    let graded = Extras {
        gradient: true,
        ..Extras::default()
    };
    let (old, new) = (page(0.0, 20.0, graded), page(40.0, 20.0, graded));
    let with_a_gradient = said(&old, &new, &fonts, "with a gradient");
    assert!(
        with_a_gradient + 60 <= count,
        "{with_a_gradient} rows with gradients in the room, {count} without"
    );
    let (old, new) = (page(0.0, 20.0, shadowed), page(40.0, 20.0, shadowed));
    let with_a_shadow = said(&old, &new, &fonts, "with a shadow");
    // The card does not move: the rows its box and its shadow reach (60 rows and the blur) are not found.
    assert!(
        with_a_shadow + 60 <= count + 10,
        "{with_a_shadow} rows with a shadow in the room, {count} without"
    );
    // A page of no faces draws its words as boxes, which this does not find the rows of.
    let boxes = Fonts::none();
    let (old, new) = (page(0.0, 20.0, plain), page(40.0, 20.0, plain));
    assert_eq!(said(&old, &new, &boxes, "no faces"), 0);
}
