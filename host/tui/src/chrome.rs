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
use pencere_view::{Color, Family, Font, Id, Measure, Paint, Quad, Rect, Role, Run};

use crate::field::TextField;
use crate::pixel::COLUMN_POINTS;

/// What the items are drawn in and on: the faces, the scale of the page, how many pixels a cell of the
/// terminal is, and how large the window is.
pub(crate) struct Paper<'a> {
    pub fonts: &'a Fonts,
    pub scale: f32,
    pub cell: (f32, f32),
    pub window: Rect,
}

impl Paper<'_> {
    /// The face of the terminal's family at the size one column of it is a cell across.
    pub(crate) fn face(&self, bold: bool) -> Font {
        Font {
            family: Family::Terminal,
            size: COLUMN_POINTS * self.scale,
            weight: if bold { 700 } else { 400 },
            italic: false,
        }
    }

    /// How high a line of `font` is, pixels.
    fn line(&self, font: &Font) -> f32 {
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
            clip,
            exact: None,
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
            clip,
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
        let room = Rect::new(
            field.rect.x * paper.scale,
            field.rect.y * paper.scale,
            field.rect.w * paper.scale,
            field.rect.h * paper.scale,
        );
        let untouched;
        let text = match texts.get(&field.id) {
            Some(text) => text,
            None => {
                untouched = TextField::new(field.kind);
                &untouched
            }
        };
        line(
            paper,
            &mut items,
            text,
            field.hint.as_deref(),
            room,
            focus == Some(field.id),
            Role::Foreground,
        );
    }
    items
}

/// One line of a field in `room`, whole cells across from its left edge: the selection, the words (or
/// `hint`, dim, where there are none) and, where it `has` the keyboard, the caret.
pub(crate) fn line(
    paper: &Paper<'_>,
    items: &mut Vec<Paint>,
    text: &TextField,
    hint: Option<&str>,
    room: Rect,
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
        items.push(paper.quad(rect, Role::Selection, room));
    }
    if text.text().is_empty() {
        if let Some(hint) = hint {
            let shown = fit(hint, cols);
            if !shown.is_empty() {
                items.push(paper.text((room.x, top), &shown, font, Role::Dim, room));
            }
        }
    } else if !slice.text.is_empty() {
        items.push(paper.text((room.x, top), &slice.text, font, ink, room));
    }
    if has {
        #[allow(clippy::cast_precision_loss)]
        let rect = Rect::new(
            room.x + slice.caret as f32 * paper.cell.0,
            top,
            paper.scale.ceil().max(1.0),
            height,
        );
        items.push(paper.quad(rect, Role::Cursor, room));
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
