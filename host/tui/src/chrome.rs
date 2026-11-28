//! What the host draws over the page, in pixels.
//!
//! The page's draw list has no text of the host's: the fields it puts over their rooms (the line
//! the person types in), a menu, the line of a panel and the address of a link are the host's, and
//! they are drawn here as more items of the list, in the face and the colours of the kit, so that
//! they are in the picture of the window as the rest is. What stands on the cells of the terminal
//! (a menu, the bar at the foot) is in whole cells, and its text is in the face of the terminal's
//! family at the size that makes a column as wide as a cell, so that a column is a cell here as it is
//! on the page.

use std::collections::HashMap;

use pencere_drive::Field;
use pencere_drive::shot::Fonts;
use pencere_view::{Color, FieldKind, Font, Id, Measure, Paint, Quad, Rect, Role, Run};

use crate::field::{TextField, rows_of};
use crate::pixel::column_face;

/// What the items are drawn in and on: the faces, the scale of the page, how many pixels a cell of the
/// terminal is, and how large the window is.
pub(crate) struct Paper<'a> {
    pub fonts: &'a Fonts,
    pub scale: f32,
    pub cell: (f32, f32),
    pub window: Rect,
    /// How far down the strip the page has along the foot of the window starts, pixels ([`foot_of`]);
    /// the window's bottom where it has none.
    pub foot: f32,
}

impl Paper<'_> {
    /// The face of the terminal's family at the size one column of it is a cell across.
    pub(crate) fn face(&self, bold: bool) -> Font {
        column_face(self.scale, bold)
    }

    /// How high a line of `font` is, pixels.
    pub(crate) fn line(&self, font: &Font) -> f32 {
        self.fonts.clone().measure(font, "M").height
    }

    /// The rectangle of the cells `col`, `row`, `cols`, `rows`, pixels.
    pub(crate) fn cells(&self, col: u16, row: u16, cols: u16, rows: u16) -> Rect {
        Rect::new(
            f32::from(col) * self.cell.0,
            f32::from(row) * self.cell.1,
            f32::from(cols) * self.cell.0,
            f32::from(rows) * self.cell.1,
        )
    }

    /// `row` (the bottom row of cells, where a bar stands) taken up to the top of the page's strip at
    /// the foot where that stands higher than the row: the ground of a bar covers the strip whole, and
    /// no edge of it is left to be seen over the bar.
    pub(crate) fn over_foot(&self, row: Rect) -> Rect {
        let top = row.y.min(self.foot);
        Rect::new(row.x, top, row.w, (row.bottom() - top).max(row.h))
    }

    /// How wide a line is, pixels: a hairline of the page at its scale.
    pub(crate) fn hairline(&self) -> f32 {
        self.scale.max(1.0).round()
    }

    /// A box of one colour of the theme, cut at `clip`.
    pub(crate) fn quad(&self, rect: Rect, fill: Role, clip: Rect) -> Paint {
        Paint::Quad(Quad {
            rect,
            radii: [0.0; 4],
            fill: Some(Color::from(fill)),
            gradient: None,
            border: None,
            shadow: None,
            clip: clip.into(),
        })
    }

    /// A line of words with its top-left at `at`, in a colour of the theme, cut at `clip`.
    pub(crate) fn text(
        &self,
        at: (f32, f32),
        text: &str,
        font: Font,
        color: Role,
        clip: Rect,
    ) -> Paint {
        Paint::Text(Run {
            origin: pencere_view::Point::new(at.0, at.1),
            text: text.to_owned(),
            font,
            color: Color::from(color),
            opacity: 1.0,
            grounds: Vec::new(),
            clip: clip.into(),
            underline: false,
            strike: false,
        })
    }

    /// Where the top of a line of `font` is that stands in the middle of a box `height` high from
    /// `top`.
    pub(crate) fn centred(&self, font: &Font, top: f32, height: f32) -> f32 {
        top + (height - self.line(font)) / 2.0
    }
}

/// The most rows of cells a strip along the foot of the window is, to be one: a status line, not the
/// page's own ground.
const MOST_FOOT_ROWS: f32 = 3.0;

/// How far down the strip the page has along the foot of the window starts, pixels: the top of the
/// highest box of the page's `items` that goes across the whole window, ends at its foot and is no
/// more than [`MOST_FOOT_ROWS`] of `row` (a row of cells) high; the top of the last row of cells where
/// there is none, or none that stands higher. A status line a page draws is such a box, and is higher
/// than a row of cells as often as not: what a bar of the host stands over has to cover it.
pub(crate) fn foot_of(items: &[Paint], window: Rect, row: f32) -> f32 {
    let mut top = window.bottom() - row;
    for item in items {
        let Paint::Quad(quad) = item else {
            continue;
        };
        if quad.fill.is_none() && quad.gradient.is_none() {
            continue;
        }
        let seen = quad.rect.intersect(quad.clip.rect);
        if seen.w <= 0.0 || seen.h <= 0.0 {
            continue;
        }
        let across = seen.x <= window.x + 1.0 && seen.right() >= window.right() - 1.0;
        let at_foot = seen.bottom() >= window.bottom() - 1.0;
        if across && at_foot && seen.h <= MOST_FOOT_ROWS * row && seen.y < top {
            top = seen.y;
        }
    }
    top.max(window.y)
}

/// The fields of the page, drawn over their rooms: each its text, or its hint dimmed where it has
/// none, the selection under the text, and the caret of the one that has the keyboard.
pub(crate) fn fields(
    paper: &Paper<'_>,
    fields: &[Field],
    texts: &HashMap<Id, TextField>,
    focus: Option<Id>,
) -> Vec<Paint> {
    let mut items = Vec::new();
    for field in fields {
        if field.covered {
            continue;
        }
        // The text stands in its box, inside the element's padding; the room is the pointer's.
        let room = Rect::new(
            field.text.x * paper.scale,
            field.text.y * paper.scale,
            field.text.w * paper.scale,
            field.text.h * paper.scale,
        );
        // What a field of lines is cut at: the room less its border, where the rows past the box run
        // on into the padding.
        let padding_box = Rect::new(
            field.padding_box.x * paper.scale,
            field.padding_box.y * paper.scale,
            field.padding_box.w * paper.scale,
            field.padding_box.h * paper.scale,
        );
        let untouched;
        let text = match texts.get(&field.id) {
            Some(text) => text,
            None => {
                untouched = TextField::new(field.kind);
                &untouched
            }
        };
        let (hint, has) = (field.hint.as_deref(), focus == Some(field.id));
        if field.kind == FieldKind::Lines {
            lines(
                paper,
                &mut items,
                text,
                hint,
                (room, padding_box),
                has,
                Role::Foreground,
            );
        } else {
            // A line is cut at the sides of its box and the top and bottom of its room, which its
            // element made at least a line high.
            let whole = Rect::new(
                room.x,
                field.rect.y * paper.scale,
                room.w,
                field.rect.h * paper.scale,
            );
            line(
                paper,
                &mut items,
                text,
                hint,
                (room, whole),
                has,
                Role::Foreground,
            );
        }
    }
    items
}

/// How many rows of a field of lines a box `height` high shows: at least one, and the room for a row
/// is a little more than it is where the box was made of whole rows and came out a hair short.
pub(crate) fn shown_in(paper: &Paper<'_>, height: f32) -> usize {
    let line = paper.line(&paper.face(false));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let rows = ((height + 1.0 / 64.0) / line).floor().max(1.0) as usize;
    rows
}

/// A field of lines in `room`, the box of its text: its text broken at a word where a row of cells is
/// full, row under row from the top, each with the selection under it; `hint`, dim and broken as
/// the text is, where there is none; and the caret, where it `has` the keyboard. All of it is cut at
/// `clip`, the room less the border (the padding box), and the row before the first the box shows and
/// the row after the last stand in the padding where there is any, as rows that are scrolled through
/// it do (the hint, which is not scrolled, runs on into it as far as it goes).
pub(crate) fn lines(
    paper: &Paper<'_>,
    items: &mut Vec<Paint>,
    text: &TextField,
    hint: Option<&str>,
    (room, clip): (Rect, Rect),
    has: bool,
    ink: Role,
) {
    let font = paper.face(false);
    let line = paper.line(&font);
    let cols = (room.w / paper.cell.0).floor().max(1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cols = cols as u16;
    let shown = shown_in(paper, room.h);
    let block = text.block(cols, shown);
    #[allow(clippy::cast_precision_loss)]
    let top = |row: isize| room.y + row as f32 * line;
    // The rows beside are drawn only where the padding is there for them.
    #[allow(clippy::cast_possible_wrap)]
    let after = block.rows.len() as isize;
    let (above, below) = padding_for_rows(room, clip);
    // What is selected is cut at the sides of the box: the space a row was broken at hangs past them.
    let sides = Rect::new(room.x, clip.y, room.w, clip.h);
    let before = block.before.as_ref().filter(|_| above);
    let beyond = block.after.as_ref().filter(|_| below);
    #[allow(clippy::cast_possible_wrap)]
    let stood = before
        .map(|row| (-1, row))
        .into_iter()
        .chain(
            block
                .rows
                .iter()
                .enumerate()
                .map(|(i, row)| (i as isize, row)),
        )
        .chain(beyond.map(|row| (after, row)));
    for (index, row) in stood {
        if let Some(selected) = &row.selected {
            #[allow(clippy::cast_precision_loss)]
            let rect = Rect::new(
                room.x + selected.start as f32 * paper.cell.0,
                top(index),
                (selected.end - selected.start) as f32 * paper.cell.0,
                line,
            );
            items.push(paper.quad(rect, Role::Selection, sides));
        }
        if !text.text().is_empty() && !row.text.is_empty() {
            items.push(paper.text((room.x, top(index)), &row.text, font, ink, clip));
        }
    }
    if let (true, Some(hint)) = (text.text().is_empty(), hint) {
        let rows = shown + usize::from(below);
        for (index, span) in rows_of(hint, cols).into_iter().take(rows).enumerate() {
            let words = &hint[span];
            if !words.is_empty() {
                #[allow(clippy::cast_possible_wrap)]
                let at = top(index as isize);
                items.push(paper.text((room.x, at), words, font, Role::Dim, clip));
            }
        }
    }
    if has {
        let width = paper.scale.ceil().max(1.0);
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_wrap)]
        let rect = Rect::new(
            room.x + block.caret.1 as f32 * paper.cell.0,
            top(block.caret.0 as isize),
            width,
            line,
        );
        // After the last cell of a full row the caret is at the box's edge: it is drawn there, and
        // that is in the padding where there is any.
        let reach = Rect::new(
            clip.x,
            clip.y,
            clip.w.max(room.right() + width - clip.x),
            clip.h,
        );
        items.push(paper.quad(rect, Role::Cursor, reach));
    }
}

/// The row of a field of lines the pointer `y` (pixels) is on, counted from the first its box `room`
/// (pixels) shows, and how many rows the box shows. The padding box (`clip`, pixels) shows the row
/// before the first and the row after the last where there is padding ([`lines`]), and a press there
/// is on that row; where there is none, the press is on the first or the last the box shows. So is a
/// `drag`: the pointer that rests in the padding does not carry the rows along under it.
pub(crate) fn row_pressed(
    paper: &Paper<'_>,
    room: Rect,
    clip: Rect,
    y: f32,
    drag: bool,
) -> (isize, usize) {
    let line = paper.line(&paper.face(false));
    let shown = shown_in(paper, room.h);
    let (above, below) = padding_for_rows(room, clip);
    let (above, below) = (above && !drag, below && !drag);
    #[allow(clippy::cast_possible_wrap)]
    let last = shown as isize - 1;
    #[allow(clippy::cast_possible_truncation)]
    let row = ((y - room.y) / line).floor() as isize;
    let row = row.max(if above { -1 } else { 0 });
    (row.min(if below { last + 1 } else { last }), shown)
}

/// Whether the padding box `clip` has padding above the box of the text `room` and below it, for the
/// rows beside the rows of the box to stand in (more than a hair of it: the boxes are made of pixels
/// that were rounded). A box with no padding has none, and shows what its rows are, however high it
/// is; with some, the row after the last stands from where the last row ends, in what is left of the
/// box and in the padding.
fn padding_for_rows(room: Rect, clip: Rect) -> (bool, bool) {
    const HAIR: f32 = 1.0 / 64.0;
    (clip.y + HAIR < room.y, clip.bottom() - HAIR > room.bottom())
}

/// One line of a field in `room`, the box of its text, whole cells across from its left edge, and cut at
/// the second box (the whole room of the field): the selection, the words (or `hint`, dim, where there
/// are none) and, where it `has` the keyboard, the caret.
pub(crate) fn line(
    paper: &Paper<'_>,
    items: &mut Vec<Paint>,
    text: &TextField,
    hint: Option<&str>,
    (room, clip): (Rect, Rect),
    has: bool,
    ink: Role,
) {
    let font = paper.face(false);
    let cols = (room.w / paper.cell.0).floor().max(1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let slice = text.slice(cols as u16);
    let top = paper.centred(&font, room.y, room.h);
    let height = paper.line(&font).min(room.h.max(paper.cell.1));
    if let Some(selected) = &slice.selected {
        #[allow(clippy::cast_precision_loss)]
        let rect = Rect::new(
            room.x + selected.start as f32 * paper.cell.0,
            top,
            (selected.end - selected.start) as f32 * paper.cell.0,
            height,
        );
        items.push(paper.quad(rect, Role::Selection, clip));
    }
    if text.text().is_empty() {
        if let Some(hint) = hint {
            let shown = fit(hint, cols);
            if !shown.is_empty() {
                items.push(paper.text((room.x, top), &shown, font, Role::Dim, clip));
            }
        }
    } else if !slice.text.is_empty() {
        items.push(paper.text((room.x, top), &slice.text, font, ink, clip));
    }
    if has {
        #[allow(clippy::cast_precision_loss)]
        let rect = Rect::new(
            room.x + slice.caret as f32 * paper.cell.0,
            top,
            paper.scale.ceil().max(1.0),
            height,
        );
        items.push(paper.quad(rect, Role::Cursor, clip));
    }
}

/// `text` as far as `cols` columns hold whole characters.
fn fit(text: &str, cols: f32) -> String {
    use unicode_width::UnicodeWidthChar;
    let mut used = 0.0;
    let mut out = String::new();
    for c in text.chars() {
        #[allow(clippy::cast_precision_loss)]
        let w = UnicodeWidthChar::width(c).unwrap_or(0) as f32;
        if used + w > cols {
            break;
        }
        used += w;
        out.push(c);
    }
    out
}
