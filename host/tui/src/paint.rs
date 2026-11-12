//! A draw list drawn into the grid.
//!
//! The grid is drawn in two layers. Under is a plane of **pixels**: a column across and the half of a
//! row down, each of one colour, which is what boxes, borders and pictures in half blocks are drawn
//! into. Over it is the plane of **characters**: text, braille dots and the eighth blocks that are
//! the lines thinner than half a cell, each in a cell. When the two are put together a cell with no
//! character is a space on one ground or a half block on two, and a cell with one stands on the
//! ground of the half that was drawn last: the box in front is the one the words belong to. A page
//! laid out on the cells has its boxes on cell edges; what it writes smaller is drawn as a line.

use std::sync::Arc;

use pencere_sandbox::draw_figure;
use pencere_view::{DrawList, Figure, Paint, Picture, Quad, Rect, Run};

use crate::color::Rgb;
use crate::grid::{Attrs, Bg, Cell, Grid};
use crate::measure::char_width;
use crate::output::{self, BEHIND_TEXT, GROUND, Pixels, Placement};
use crate::pictures::{Pictures, Raster};
use crate::theme::Theme;

/// How a picture of the page is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageMode {
    /// By the terminal, in its pixels (Kitty's graphics protocol), under the text.
    Kitty,
    /// In half blocks, a pixel of the grid each: for a terminal that draws no pictures.
    Blocks,
    /// Not at all.
    Off,
}

/// How a canvas of the page is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanvasMode {
    /// In braille dots, eight to a cell, where the figure covers them (and no more: the ground is the
    /// box's under the canvas); a canvas of a cell or two is an icon, its ink filling the cells, in
    /// quarter blocks where it is flat (an arrow).
    Characters,
    /// As a picture, in the terminal's pixels ([`ImageMode::Kitty`]).
    Picture,
}

/// How a draw list is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    /// How many pixels of the draw list a column is across and a row is down: the measure of the
    /// page ([`crate::Cells`]) has said the same.
    pub col: f32,
    pub row: f32,
    pub images: ImageMode,
    pub canvases: CanvasMode,
    /// The pixels of the terminal's cell, which the pictures are drawn at where it draws them.
    pub pixels: (u32, u32),
}

impl Default for Options {
    fn default() -> Self {
        Self {
            col: 1.0,
            row: 2.0,
            images: ImageMode::Kitty,
            canvases: CanvasMode::Characters,
            pixels: (12, 24),
        }
    }
}

/// What a draw was like, for whoever chooses how a page is laid out for the grid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// The cells that have a character.
    pub text_cells: usize,
    /// The characters drawn over another's.
    pub overlaps: usize,
    /// The cells whose character stands where the two halves of the ground are not one.
    pub split: usize,
}

/// A run of cells on one row that is a link the terminal can open (OSC 8): the columns `from` up to
/// `to`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub row: u16,
    pub from: u16,
    pub to: u16,
    pub url: String,
}

/// A page drawn: its cells, and the pictures the terminal is to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub grid: Grid,
    pub placements: Vec<Placement>,
    /// The runs of cells that are links.
    pub links: Vec<Link>,
    /// Whether a picture had no pixels yet: ask again when it has.
    pub waiting: bool,
    pub stats: Stats,
}

impl Frame {
    /// The characters, a line to a row ([`Grid::text`]).
    pub fn text(&self) -> String {
        self.grid.text()
    }

    /// What a terminal is sent to show the frame.
    pub fn ansi(&self) -> Vec<u8> {
        output::ansi(self)
    }
}

/// `list` drawn into a grid of `cols` by `rows` cells, in `theme`.
pub fn render(
    list: &DrawList,
    cols: u16,
    rows: u16,
    theme: &Theme,
    pictures: &dyn Pictures,
    options: &Options,
) -> Frame {
    let mut painter = Painter {
        theme,
        pictures,
        options,
        cw: if options.col.is_finite() && options.col > 0.0 {
            options.col
        } else {
            1.0
        },
        rh: if options.row.is_finite() && options.row > 0.0 {
            options.row
        } else {
            2.0
        },
        canvas: Canvas::new(cols, rows, theme.background),
        texts: vec![0; usize::from(cols) * usize::from(rows)],
        placements: Vec::new(),
        waiting: false,
        overlaps: 0,
    };
    for (n, item) in list.items.iter().enumerate() {
        if let Paint::Text(run) = item {
            painter.note_text(run, n as u32 + 1);
        }
    }
    for item in &list.items {
        painter.canvas.tick += 1;
        match item {
            Paint::Quad(quad) => painter.quad(quad),
            Paint::Text(run) => painter.text(run),
            Paint::Image(picture) => painter.picture(picture),
            Paint::Figure(figure) => painter.figure(figure),
            // The drive puts what an editor draws into the list; the place of one that was not
            // filled shows the room's own ground.
            Paint::Editor(_) => {}
        }
    }
    painter.finish()
}

/// A rectangle of pixels of the grid: columns across, halves of a row down. It is empty when an
/// end is not past its start.
#[derive(Clone, Copy, Debug)]
struct Span {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

impl Span {
    fn is_empty(self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    fn meet(self, other: Self) -> Self {
        Self {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        }
    }
}

/// The one rounding of the grid: to the nearest line, halves away from zero.
fn snap(value: f32) -> i32 {
    if value.is_finite() {
        value.round().clamp(-1e6, 1e6) as i32
    } else {
        0
    }
}

/// What a cell of the character plane holds.
#[derive(Clone, Copy, Debug)]
enum Slot {
    Empty,
    Glyph {
        ch: char,
        fg: Rgb,
        attrs: Attrs,
        wide: bool,
        /// A ground of its own, where a line passed over it and the cell is drawn the other way
        /// about.
        back: Option<Rgb>,
    },
    /// A line thinner than a cell, drawn with a block of an eighth of one. A character put over it
    /// takes its place: a line never covers one.
    Line {
        ch: char,
        fg: Rgb,
    },
    /// The right column of a wide character.
    Tail,
}

/// Where in a cell a line stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    fn glyph(self) -> char {
        match self {
            Self::Left => '\u{258f}',
            Self::Right => '\u{2595}',
            Self::Top => '\u{2594}',
            Self::Bottom => '\u{2581}',
        }
    }

    fn upright(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

struct Canvas {
    cols: i32,
    rows: i32,
    /// The pixel plane, `cols` by `2 * rows`.
    ground: Vec<Rgb>,
    /// When each pixel was drawn last: the count of the item.
    stamp: Vec<u32>,
    /// The cells a picture the terminal draws shows through.
    hole: Vec<bool>,
    slots: Vec<Slot>,
    tick: u32,
}

impl Canvas {
    fn new(cols: u16, rows: u16, ground: Rgb) -> Self {
        let (cols, rows) = (usize::from(cols), usize::from(rows));
        Self {
            cols: cols as i32,
            rows: rows as i32,
            ground: vec![ground; cols * rows * 2],
            stamp: vec![0; cols * rows * 2],
            hole: vec![false; cols * rows],
            slots: vec![Slot::Empty; cols * rows],
            tick: 0,
        }
    }

    fn bounds(&self) -> Span {
        Span {
            x0: 0,
            y0: 0,
            x1: self.cols,
            y1: self.rows * 2,
        }
    }

    fn pixel(&self, x: i32, y: i32) -> Option<usize> {
        let inside = (0..self.cols).contains(&x) && (0..self.rows * 2).contains(&y);
        inside.then(|| (y * self.cols + x) as usize)
    }

    fn cell(&self, col: i32, row: i32) -> Option<usize> {
        let inside = (0..self.cols).contains(&col) && (0..self.rows).contains(&row);
        inside.then(|| (row * self.cols + col) as usize)
    }

    fn ground_at(&self, x: i32, y: i32) -> Rgb {
        self.pixel(x, y)
            .and_then(|i| self.ground.get(i).copied())
            .unwrap_or_default()
    }

    /// The colour a character at the cell stands on: the half that was drawn last.
    fn under(&self, col: i32, row: i32) -> Rgb {
        let (top, bottom) = (self.pixel(col, row * 2), self.pixel(col, row * 2 + 1));
        let stamped = |i: Option<usize>| i.and_then(|i| self.stamp.get(i).copied()).unwrap_or(0);
        if stamped(bottom) > stamped(top) {
            self.ground_at(col, row * 2 + 1)
        } else {
            self.ground_at(col, row * 2)
        }
    }

    /// Lays `color` over the pixel with `alpha` of it.
    fn paint(&mut self, x: i32, y: i32, color: Rgb, alpha: f32) {
        let Some(i) = self.pixel(x, y) else { return };
        if alpha <= 0.0 {
            return;
        }
        let tick = self.tick;
        if let Some(ground) = self.ground.get_mut(i) {
            *ground = if alpha >= 0.999 {
                color
            } else {
                ground.mix(color, alpha)
            };
        }
        if let Some(stamp) = self.stamp.get_mut(i) {
            *stamp = tick;
        }
        let Some(cell) = self.cell(x, y.div_euclid(2)) else {
            return;
        };
        if alpha > 0.02
            && let Some(hole) = self.hole.get_mut(cell)
        {
            *hole = false;
        }
        let other = if y.rem_euclid(2) == 0 { y + 1 } else { y - 1 };
        let whole = self
            .pixel(x, other)
            .and_then(|i| self.stamp.get(i))
            .is_some_and(|stamp| *stamp == tick);
        if alpha >= 0.98 {
            // Drawn in front of a character, which it covers where it covers the cell: an edge in
            // the middle of a cell is a rounding, and the character of the row above is not under it.
            if whole {
                self.clear(cell);
            }
        } else if y.rem_euclid(2) == 0
            && let Some(Slot::Glyph { fg, .. }) = self.slots.get_mut(cell)
        {
            *fg = fg.mix(color, alpha);
        }
    }

    /// Takes the character out of a cell, and the other half of it where it is wide.
    fn clear(&mut self, cell: usize) {
        match self.slots.get(cell).copied() {
            Some(Slot::Glyph { wide: true, .. }) => {
                if let Some(next) = self.slots.get_mut(cell + 1) {
                    *next = Slot::Empty;
                }
            }
            Some(Slot::Tail) => {
                if let Some(previous) = cell.checked_sub(1).and_then(|i| self.slots.get_mut(i)) {
                    *previous = Slot::Empty;
                }
            }
            _ => {}
        }
        if let Some(slot) = self.slots.get_mut(cell) {
            *slot = Slot::Empty;
        }
    }

    /// Puts a character in a cell, and in the one after it where it is wide; says whether it went
    /// over another.
    fn put(&mut self, col: i32, row: i32, glyph: (char, Rgb, Attrs), wide: bool) -> Option<bool> {
        let cell = self.cell(col, row)?;
        if wide && col + 1 >= self.cols {
            return None;
        }
        let taken = |slots: &[Slot], i: usize| {
            !matches!(slots.get(i), Some(Slot::Empty | Slot::Line { .. }) | None)
        };
        let over = taken(&self.slots, cell) || (wide && taken(&self.slots, cell + 1));
        self.clear(cell);
        if wide {
            self.clear(cell + 1);
        }
        let (ch, fg, attrs) = glyph;
        if let Some(slot) = self.slots.get_mut(cell) {
            *slot = Slot::Glyph {
                ch,
                fg,
                attrs,
                wide,
                back: None,
            };
        }
        if wide && let Some(slot) = self.slots.get_mut(cell + 1) {
            *slot = Slot::Tail;
        }
        Some(over)
    }

    /// Makes a cell the terminal's: a picture it draws shows through.
    fn open(&mut self, col: i32, row: i32) {
        if let Some(cell) = self.cell(col, row) {
            self.clear(cell);
            if let Some(hole) = self.hole.get_mut(cell) {
                *hole = true;
            }
        }
    }
}

struct Painter<'a> {
    theme: &'a Theme,
    pictures: &'a dyn Pictures,
    options: &'a Options,
    /// The pixels a column is across, and a row is down.
    cw: f32,
    rh: f32,
    canvas: Canvas,
    /// For each cell, the count of the last item of the list that puts text in it (0: none).
    texts: Vec<u32>,
    placements: Vec<Placement>,
    waiting: bool,
    overlaps: usize,
}

/// The braille dot of a cell's column and row of dots.
const DOTS: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

/// How much of a dot a line covers for the dot to be one of the line's.
const INK: f32 = 0.4;

/// How many samples a dot of a canvas is drawn from, across and down.
const SAMPLES: f32 = 4.0;

/// A canvas of this many cells or fewer is an icon.
const SMALL: f32 = 2.0;

/// How much wider than tall an icon's ink is for it to be drawn in quarter blocks.
const FLAT: f32 = 1.3;

/// How far a box's fill is from the ground under it, as a share of the way from the ground of the
/// theme to its foreground, for the box to be told apart by its fill.
const APART: f32 = 0.07;

/// How much of its border's colour (and opacity) a box that has no ring has laid over it, so that it
/// reads as a bar of a tone of the border's: where the fill does not tell it from the ground, and
/// where it does.
const BAR: f32 = 0.3;
const BAR_APART: f32 = 0.1;

/// The quarter blocks of a cell, by which of its quarters are covered: 1 the upper left, 2 the upper
/// right, 4 the lower left, 8 the lower right.
const QUARTERS: [char; 16] = [
    ' ', '\u{2598}', '\u{259d}', '\u{2580}', '\u{2596}', '\u{258c}', '\u{259e}', '\u{259b}',
    '\u{2597}', '\u{259a}', '\u{2590}', '\u{259c}', '\u{2584}', '\u{2599}', '\u{259f}', '\u{2588}',
];

/// How far apart two colours of a cell's dots may be (the sum of the channels' differences) for the
/// dots to count as of one colour.
const SAME: f32 = 96.0;

/// The dots of one cell that a figure covers, with the colour of what covers them.
#[derive(Default)]
struct Dots {
    bits: u8,
    /// What covers each dot: its colour, not premultiplied, and how much of the dot is covered.
    covers: Vec<([f32; 3], f32)>,
}

impl Dots {
    fn add(&mut self, bit: u8, premultiplied: [f32; 3], alpha: f32) {
        self.bits |= bit;
        let weight = alpha.max(f32::EPSILON);
        self.covers
            .push((premultiplied.map(|channel| channel / weight), alpha));
    }

    /// The colour of what covers the dots: where lines of two colours cross in a cell, the colour that
    /// has more of the cover (colours near each other count as one, and are averaged by cover), the
    /// brighter where they have as much. A mean of the two would be a colour neither line has.
    fn color(&self) -> Rgb {
        let near = |one: &[f32; 3], other: &[f32; 3]| {
            one.iter()
                .zip(other)
                .map(|(a, b)| (a - b).abs())
                .sum::<f32>()
                <= SAME
        };
        let light = |color: &[f32; 3]| color.iter().sum::<f32>();
        let mut best: Option<(f32, [f32; 3])> = None;
        for (color, _) in &self.covers {
            let (mut total, mut sum) = (0.0_f32, [0.0_f32; 3]);
            for (other, cover) in &self.covers {
                if near(color, other) {
                    total += cover;
                    for (sum, channel) in sum.iter_mut().zip(other) {
                        *sum += channel * cover;
                    }
                }
            }
            let mean = sum.map(|channel| channel / total.max(f32::EPSILON));
            let better = best.as_ref().is_none_or(|(most, was)| {
                total > *most + 0.05 || (total >= *most - 0.05 && light(&mean) > light(was))
            });
            if better {
                best = Some((total, mean));
            }
        }
        let [r, g, b] = best
            .map_or([0.0; 3], |(_, mean)| mean)
            .map(|channel| channel.round().clamp(0.0, 255.0) as u8);
        Rgb::new(r, g, b)
    }
}

impl Painter<'_> {
    fn ch(&self) -> f32 {
        self.rh
    }

    fn col(&self, x: f32) -> i32 {
        snap(x / self.cw)
    }

    fn row(&self, y: f32) -> i32 {
        snap(y / self.ch())
    }

    fn half(&self, y: f32) -> i32 {
        snap(y / (self.rh / 2.0))
    }

    /// `rect` in pixels of the grid, rounded edge by edge.
    fn span(&self, rect: &Rect) -> Span {
        Span {
            x0: self.col(rect.x),
            y0: self.half(rect.y),
            x1: self.col(rect.x + rect.w),
            y1: self.half(rect.y + rect.h),
        }
    }

    /// The same, for a box that is drawn: one that has any size keeps a pixel.
    fn solid(&self, rect: &Rect) -> Option<Span> {
        let sized = |length: f32| length.is_finite() && length > 0.0;
        if !(sized(rect.w) && sized(rect.h)) {
            return None;
        }
        let mut span = self.span(rect);
        span.x1 = span.x1.max(span.x0 + 1);
        span.y1 = span.y1.max(span.y0 + 1);
        Some(span)
    }

    /// `span` tinted with `rgb` to `alpha` where it is not opaque. Over the cells a picture of the
    /// terminal's shows through it is a picture of its own over that one, a pixel stretched over the
    /// cells, so that the picture stays what it is under the tint; elsewhere the tint is laid on the
    /// grounds.
    fn tint(&mut self, span: Span, rgb: Rgb, alpha: f32) {
        let span = span.meet(self.canvas.bounds());
        if span.is_empty() {
            return;
        }
        // The runs of cells that are the terminal's, a row of them each.
        let mut runs: Vec<(i32, i32, i32)> = Vec::new();
        for row in span.y0.div_euclid(2)..=(span.y1 - 1).div_euclid(2) {
            let mut open: Option<(i32, i32)> = None;
            for col in span.x0..span.x1 {
                let hole = self
                    .canvas
                    .cell(col, row)
                    .and_then(|cell| self.canvas.hole.get(cell).copied())
                    .unwrap_or(false);
                if hole {
                    open = Some(open.map_or((col, col + 1), |(c0, _)| (c0, col + 1)));
                    continue;
                }
                if let Some((c0, c1)) = open.take() {
                    runs.push((row, c0, c1));
                }
                for y in span.y0.max(row * 2)..span.y1.min(row * 2 + 2) {
                    self.canvas.paint(col, y, rgb, alpha);
                }
            }
            if let Some((c0, c1)) = open {
                runs.push((row, c0, c1));
            }
        }
        // Rows of the same columns, one under the other, are one picture.
        let mut boxes: Vec<(i32, i32, i32, i32)> = Vec::new();
        for (row, c0, c1) in runs {
            match boxes.last_mut() {
                Some((_, r1, b0, b1)) if *r1 == row && *b0 == c0 && *b1 == c1 => *r1 = row + 1,
                _ => boxes.push((row, row + 1, c0, c1)),
            }
        }
        for (r0, r1, c0, c1) in boxes {
            self.placements.push(Placement {
                col: c0 as u16,
                row: r0 as u16,
                cols: (c1 - c0) as u16,
                rows: (r1 - r0) as u16,
                pixels: Pixels::solid([
                    rgb.r,
                    rgb.g,
                    rgb.b,
                    (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
                ]),
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                z: BEHIND_TEXT,
            });
        }
    }

    fn fill(&mut self, span: Span, color: impl Fn(i32, i32) -> (Rgb, f32)) {
        let span = span.meet(self.canvas.bounds());
        for y in span.y0..span.y1 {
            for x in span.x0..span.x1 {
                let (rgb, alpha) = color(x, y);
                self.canvas.paint(x, y, rgb, alpha);
            }
        }
    }

    /// A line in the cell at (`col`, `row`), as the edge of it it is nearest. A character in the cell
    /// is not covered by it: a line that stands upright (a caret, a bar) turns the cell the other
    /// way about, so that the character shows on the line's colour, and one that lies is left out.
    fn mark(&mut self, col: i32, row: i32, edge: Edge, (rgb, alpha): (Rgb, f32)) {
        let Some(cell) = self.canvas.cell(col, row) else {
            return;
        };
        let ground = self.canvas.under(col, row);
        let color = ground.mix(rgb, alpha);
        let Some(slot) = self.canvas.slots.get_mut(cell) else {
            return;
        };
        match *slot {
            Slot::Empty | Slot::Line { .. } => {
                *slot = Slot::Line {
                    ch: edge.glyph(),
                    fg: color,
                };
            }
            Slot::Glyph {
                ch,
                attrs,
                wide,
                back,
                ..
            } if edge.upright() => {
                *slot = Slot::Glyph {
                    ch,
                    fg: ground,
                    attrs,
                    wide,
                    back: back.or(Some(color)),
                };
            }
            Slot::Glyph { .. } | Slot::Tail => {}
        }
    }

    /// Whether `rect` is a cell wholly inside `clip`.
    fn within(clip: Span, col: i32, row: i32) -> bool {
        col >= clip.x0 && col < clip.x1 && row * 2 >= clip.y0 && row * 2 + 2 <= clip.y1
    }

    /// A fill thinner than half a cell on one axis only is a line: it is drawn with the eighth of a
    /// cell that is nearest to where it stands, over the ground it is on, and what it passes over
    /// keeps its character.
    fn hairline(&mut self, quad: &Quad) -> bool {
        let (Some(fill), rect) = (quad.fill, quad.rect) else {
            return false;
        };
        let sized = |length: f32| length.is_finite() && length > 0.0;
        if !(sized(rect.w) && sized(rect.h)) {
            return false;
        }
        let (thin_x, thin_y) = (rect.w < self.cw / 2.0, rect.h < self.rh / 2.0);
        if thin_x == thin_y {
            return false;
        }
        let color = self.theme.resolve(&fill);
        let clip = self.span(&quad.clip);
        if thin_x {
            let centre = (rect.x + rect.w / 2.0) / self.cw;
            let col = centre.floor() as i32;
            let edge = if centre - centre.floor() < 0.5 {
                Edge::Left
            } else {
                Edge::Right
            };
            let (r0, r1) = (self.row(rect.y), self.row(rect.y + rect.h));
            for row in r0..r1.max(r0 + 1) {
                if Self::within(clip, col, row) {
                    self.mark(col, row, edge, color);
                }
            }
        } else {
            let centre = (rect.y + rect.h / 2.0) / self.rh;
            let row = centre.floor() as i32;
            let edge = if centre - centre.floor() < 0.5 {
                Edge::Top
            } else {
                Edge::Bottom
            };
            let (c0, c1) = (self.col(rect.x), self.col(rect.x + rect.w));
            for col in c0..c1.max(c0 + 1) {
                if Self::within(clip, col, row) {
                    self.mark(col, row, edge, color);
                }
            }
        }
        true
    }

    /// The ground at the middle of `span`.
    fn ground_in(&self, span: Span) -> Rgb {
        self.canvas
            .ground_at((span.x0 + span.x1) / 2, (span.y0 + span.y1) / 2)
    }

    /// The border of a box that has no ring, as a tone: `rgb` laid over the box with a share of
    /// `alpha`, `before` being the ground under the box before its fill was drawn. A fill that tells
    /// the box from the ground has the tone laid over it lightly (the border still says a state: a
    /// field in focus is not as one that is not); one that does not, as a bar of the border's tone.
    fn bar(&mut self, span: Span, before: Rgb, (rgb, alpha): (Rgb, f32)) {
        if alpha < 0.02 || span.is_empty() {
            return;
        }
        let distance = |a: Rgb, b: Rgb| {
            [(a.r, b.r), (a.g, b.g), (a.b, b.b)]
                .iter()
                .map(|(x, y)| f32::from(x.abs_diff(*y)))
                .sum::<f32>()
        };
        let reach = distance(self.theme.background, self.theme.foreground).max(1.0);
        let apart = distance(before, self.ground_in(span)) / reach >= APART;
        self.tint(span, rgb, alpha * if apart { BAR_APART } else { BAR });
    }

    /// Notes the cells a line of text is drawn in, as the item `item` of the list.
    fn note_text(&mut self, run: &Run, item: u32) {
        let row = self.row(run.origin.y);
        let mut col = self.col(run.origin.x);
        for ch in run.text.chars() {
            let width = char_width(ch) as i32;
            if width == 0 {
                continue;
            }
            for taken in col..col + width {
                if let Some(cell) = self
                    .canvas
                    .cell(taken, row)
                    .and_then(|cell| self.texts.get_mut(cell))
                {
                    *cell = item;
                }
            }
            col += width;
        }
    }

    /// Whether a cell holds text, or is to hold some of an item after the one being drawn.
    fn has_text(&self, col: i32, row: i32) -> bool {
        let Some(cell) = self.canvas.cell(col, row) else {
            return false;
        };
        matches!(
            self.canvas.slots.get(cell),
            Some(Slot::Glyph { .. } | Slot::Tail)
        ) || self
            .texts
            .get(cell)
            .is_some_and(|last| *last > self.canvas.tick)
    }

    /// The border of a box that has one on every side, drawn as a ring of box-drawing characters in
    /// the cells along the edge of the box, with the corners the box rounds (`radii`: top left, top
    /// right, bottom right, bottom left): the one line of them closes where an eighth of a cell, which
    /// is a line along the edge of a cell, cannot hold a corner. The ring is the colour of the border
    /// on the ground of the box. A box of a cell on a side, and one with text in a cell of its edge
    /// (a box with no room inside), has no ring: its border is the lines along the edges. Says whether
    /// the ring was drawn.
    fn ring(
        &mut self,
        (c0, r0, c1, r1): (i32, i32, i32, i32),
        radii: [f32; 4],
        (rgb, alpha): (Rgb, f32),
        clip: Span,
    ) -> bool {
        if c1 <= c0 || r1 <= r0 {
            return false;
        }
        let edge = |col: i32, row: i32| col == c0 || col == c1 || row == r0 || row == r1;
        let held = (c0..=c1)
            .flat_map(|col| (r0..=r1).map(move |row| (col, row)))
            .any(|(col, row)| edge(col, row) && self.has_text(col, row));
        if held {
            return false;
        }
        if alpha < 0.02 {
            // A border nobody sees.
            return true;
        }
        let corner = |round: f32, rounded: char, square: char| {
            if round > 0.0 { rounded } else { square }
        };
        let [top_left, top_right, bottom_right, bottom_left] = radii;
        for row in r0..=r1 {
            for col in c0..=c1 {
                if !edge(col, row) || !Self::within(clip, col, row) {
                    continue;
                }
                let ch = match (col == c0, col == c1, row == r0, row == r1) {
                    (true, _, true, _) => corner(top_left, '\u{256d}', '\u{250c}'),
                    (_, true, true, _) => corner(top_right, '\u{256e}', '\u{2510}'),
                    (_, true, _, true) => corner(bottom_right, '\u{256f}', '\u{2518}'),
                    (true, _, _, true) => corner(bottom_left, '\u{2570}', '\u{2514}'),
                    (_, _, true, _) | (_, _, _, true) => '\u{2500}',
                    _ => '\u{2502}',
                };
                let color = self.canvas.under(col, row).mix(rgb, alpha);
                self.canvas.put(col, row, (ch, color, Attrs::NONE), false);
            }
        }
        true
    }

    fn quad(&mut self, quad: &Quad) {
        if self.hairline(quad) {
            return;
        }
        let Some(rect) = self.solid(&quad.rect) else {
            return;
        };
        let clip = self.span(&quad.clip);
        let visible = rect.meet(clip);
        let before = self.ground_in(visible);
        if let Some(fill) = quad.fill {
            let (rgb, alpha) = self.theme.resolve(&fill);
            if alpha < 0.98 {
                self.tint(visible, rgb, alpha);
            } else {
                self.fill(visible, |_, _| (rgb, alpha));
            }
        } else if let Some(gradient) = quad.gradient {
            let (from, from_alpha) = self.theme.resolve(&gradient.from);
            let (to, to_alpha) = self.theme.resolve(&gradient.to);
            let (sin, cos) = gradient.angle.to_radians().sin_cos();
            let (w, h) = ((rect.x1 - rect.x0) as f32, (rect.y1 - rect.y0) as f32);
            let (cx, cy) = (rect.x0 as f32 + w / 2.0, rect.y0 as f32 + h / 2.0);
            let length = (w * sin).abs() + (h * cos).abs();
            self.fill(visible, |x, y| {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let t = if length > 0.0 {
                    (0.5 + (dx * sin - dy * cos) / length).clamp(0.0, 1.0)
                } else {
                    0.5
                };
                (from.mix(to, t), from_alpha + (to_alpha - from_alpha) * t)
            });
        }
        if let Some(border) = quad.border {
            let (rgb, alpha) = self.theme.resolve(&border.color);
            if border.width < self.cw / 2.0 && border.width < self.rh / 2.0 {
                // A border thinner than half a cell is the line along the edge of the cells it
                // encloses.
                let (r0, r1) = (rect.y0.div_euclid(2), (rect.y1 - 1).div_euclid(2));
                let (c0, c1) = (rect.x0, rect.x1 - 1);
                let color = (rgb, alpha);
                if self.ring((c0, r0, c1, r1), quad.radii, color, clip) {
                    return;
                }
                // No ring: a line in part of its edge would be a frame that does not close. The
                // box is a bar, told from what is under it by its fill, and by the tone of its
                // border laid over it.
                self.bar(visible, before, color);
                return;
            }
            let width = snap(border.width / self.cw).max(1);
            let top_end = (rect.y0 + width).min(rect.y1);
            let bottom_start = (rect.y1 - width).max(top_end);
            let left_end = (rect.x0 + width).min(rect.x1);
            let right_start = (rect.x1 - width).max(left_end);
            let strips = [
                Span {
                    y1: top_end,
                    ..rect
                },
                Span {
                    y0: bottom_start,
                    ..rect
                },
                Span {
                    x1: left_end,
                    y0: top_end,
                    y1: bottom_start,
                    ..rect
                },
                Span {
                    x0: right_start,
                    y0: top_end,
                    y1: bottom_start,
                    ..rect
                },
            ];
            for strip in strips {
                self.fill(strip.meet(clip), |_, _| (rgb, alpha));
            }
        }
    }

    fn text(&mut self, run: &Run) {
        let (color, alpha) = self.theme.resolve(&run.color);
        let alpha = alpha * run.opacity.clamp(0.0, 1.0);
        if !alpha.is_finite() || alpha < 0.02 {
            return;
        }
        let clip = self.span(&run.clip);
        let row = self.row(run.origin.y);
        // A row is in the clip where its middle is.
        let middle = row * 2 + 1;
        if middle < clip.y0 || middle > clip.y1 {
            return;
        }
        let mut attrs = Attrs::NONE;
        if run.font.weight >= 600 {
            attrs = attrs.with(Attrs::BOLD);
        }
        if run.font.italic {
            attrs = attrs.with(Attrs::ITALIC);
        }
        if run.underline {
            attrs = attrs.with(Attrs::UNDERLINE);
        }
        if run.strike {
            attrs = attrs.with(Attrs::STRIKE);
        }
        let mut col = self.col(run.origin.x);
        for ch in run.text.chars() {
            let width = char_width(ch) as i32;
            if width == 0 {
                continue;
            }
            let (start, end) = (col, col + width);
            col = end;
            if start < clip.x0 || end > clip.x1 {
                continue;
            }
            let fg = self.canvas.under(start, row).mix(color, alpha);
            if self.canvas.put(start, row, (ch, fg, attrs), width == 2) == Some(true) {
                self.overlaps += 1;
            }
        }
    }

    /// The picture as it is asked of whoever has its pixels: the rectangle in the terminal's own
    /// pixels, so that the pixels are as many as the terminal shows.
    fn request(&self, picture: &Picture) -> Picture {
        let (across, down) = (
            self.options.pixels.0 as f32 / self.cw,
            self.options.pixels.1 as f32 / self.ch(),
        );
        let rect = picture.rect;
        Picture {
            rect: Rect::new(
                rect.x * across,
                rect.y * down,
                rect.w * across,
                rect.h * down,
            ),
            ..picture.clone()
        }
    }

    fn picture(&mut self, picture: &Picture) {
        let mode = self.options.images;
        if mode == ImageMode::Off {
            return;
        }
        let Some(rect) = self.solid(&picture.rect) else {
            return;
        };
        // A picture the terminal does not draw is a mosaic of the grid's pixels, where it is large: an
        // icon is a shape in a handful of them, and is left out.
        let large = rect.x1 - rect.x0 >= 8 && rect.y1 - rect.y0 >= 8;
        if mode == ImageMode::Blocks && (picture.tint.is_some() || !large) {
            return;
        }
        let Some(raster) = self.pictures.raster(&self.request(picture)) else {
            self.waiting = true;
            return;
        };
        let raster = Arc::new(raster);
        let (rx0, ry0) = (picture.rect.x, picture.rect.y);
        let (rx1, ry1) = (rx0 + picture.rect.w, ry0 + picture.rect.h);
        let opacity = picture.opacity.clamp(0.0, 1.0);
        match mode {
            ImageMode::Off => {}
            ImageMode::Blocks => {
                let visible = rect
                    .meet(self.span(&picture.clip))
                    .meet(self.canvas.bounds());
                if visible.is_empty() {
                    return;
                }
                let whole = (
                    rect.x0 as f32,
                    rect.y0 as f32,
                    rect.x1 as f32,
                    rect.y1 as f32,
                );
                let part = (
                    visible.x0 as f32,
                    visible.y0 as f32,
                    visible.x1 as f32,
                    visible.y1 as f32,
                );
                self.mosaic(visible, &raster, shares(whole, part, raster.crop), opacity);
            }
            ImageMode::Kitty => {
                // The cells the picture is seen in: those of its box that its clip leaves.
                let seen = intersect(&picture.rect, &picture.clip);
                let (c0, r0) = (self.col(seen.x), self.row(seen.y));
                let c1 = self.col(seen.x + seen.w).max(c0 + 1);
                let r1 = self.row(seen.y + seen.h).max(r0 + 1);
                let (c0, r0) = (c0.max(0), r0.max(0));
                let (c1, r1) = (c1.min(self.canvas.cols), r1.min(self.canvas.rows));
                if c1 <= c0 || r1 <= r0 || seen.w <= 0.0 || seen.h <= 0.0 {
                    return;
                }
                let whole = (rx0, ry0, rx1, ry1);
                let part = (
                    c0 as f32 * self.cw,
                    r0 as f32 * self.ch(),
                    c1 as f32 * self.cw,
                    r1 as f32 * self.ch(),
                );
                let source = shares(whole, part, raster.crop);
                let tint = picture.tint.map(|tint| self.theme.resolve(&tint));
                let covers = tint.is_none() && opacity >= 0.999 && raster.is_opaque();
                let cells = (c0, r0, c1, r1);
                if covers {
                    // What the terminal draws under the grounds of its cells is not seen by what is
                    // laid over it, so the plane keeps the picture's colours too: a highlight that
                    // is not solid is laid over them.
                    let region = Span {
                        x0: c0,
                        y0: r0 * 2,
                        x1: c1,
                        y1: r1 * 2,
                    };
                    self.mosaic(region, &raster, source, 1.0);
                    for row in r0..r1 {
                        for col in c0..c1 {
                            self.canvas.open(col, row);
                        }
                    }
                }
                self.place(
                    cells,
                    &raster,
                    source,
                    tint,
                    opacity,
                    if covers { GROUND } else { BEHIND_TEXT },
                );
            }
        }
    }

    /// The pixels of the grid in `region` as the means of what the part `source` of the raster
    /// shows there.
    fn mosaic(
        &mut self,
        region: Span,
        raster: &Raster,
        source: (f32, f32, f32, f32),
        opacity: f32,
    ) {
        let (w, h) = (
            (region.x1 - region.x0).max(1) as f32,
            (region.y1 - region.y0).max(1) as f32,
        );
        let (u0, v0, u1, v1) = source;
        let visible = region.meet(self.canvas.bounds());
        for y in visible.y0..visible.y1 {
            for x in visible.x0..visible.x1 {
                let (fx0, fx1) = ((x - region.x0) as f32 / w, (x + 1 - region.x0) as f32 / w);
                let (fy0, fy1) = ((y - region.y0) as f32 / h, (y + 1 - region.y0) as f32 / h);
                let from = (u0 + (u1 - u0) * fx0, v0 + (v1 - v0) * fy0);
                let to = (u0 + (u1 - u0) * fx1, v0 + (v1 - v0) * fy1);
                let [r, g, b, a] = raster.mean(from, to);
                let cover = a / 255.0 * opacity;
                if cover < 0.01 {
                    continue;
                }
                let ground = self.canvas.ground_at(x, y);
                let over = |ground: u8, ink: f32| {
                    (f32::from(ground) * (1.0 - cover) + ink * opacity)
                        .round()
                        .clamp(0.0, 255.0) as u8
                };
                let color = Rgb::new(over(ground.r, r), over(ground.g, g), over(ground.b, b));
                self.canvas.paint(x, y, color, 1.0);
            }
        }
    }

    /// A picture the terminal draws: the part `source` of the raster, over the cells.
    fn place(
        &mut self,
        (c0, r0, c1, r1): (i32, i32, i32, i32),
        raster: &Arc<Raster>,
        source: (f32, f32, f32, f32),
        tint: Option<(Rgb, f32)>,
        opacity: f32,
        z: i32,
    ) {
        let (u0, v0, u1, v1) = source;
        let edge = |share: f32, side: u32| (share.clamp(0.0, 1.0) * side as f32).round() as u32;
        let x0 = edge(u0.min(u1), raster.width).min(raster.width.saturating_sub(1));
        let x1 = edge(u0.max(u1), raster.width).clamp(x0 + 1, raster.width.max(1));
        let y0 = edge(v0.min(v1), raster.height).min(raster.height.saturating_sub(1));
        let y1 = edge(v0.max(v1), raster.height).clamp(y0 + 1, raster.height.max(1));
        self.placements.push(Placement {
            col: c0 as u16,
            row: r0 as u16,
            cols: (c1 - c0) as u16,
            rows: (r1 - r0) as u16,
            pixels: Pixels::of(Arc::clone(raster), tint, opacity),
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
            z,
        });
    }

    fn figure(&mut self, figure: &Figure) {
        let sized = |length: f32| length.is_finite() && length > 0.0;
        if !(sized(figure.rect.w) && sized(figure.rect.h) && sized(figure.scale)) {
            return;
        }
        let rect = &figure.rect;
        let (c0, r0) = (self.col(rect.x), self.row(rect.y));
        let c1 = self.col(rect.x + rect.w).max(c0 + 1);
        let r1 = self.row(rect.y + rect.h).max(r0 + 1);
        let clip = self.span(&figure.clip);
        // The box in the grid is the cells it rounds to; the figure is drawn to fill them.
        let (cols, rows) = ((c1 - c0) as f32, (r1 - r0) as f32);
        let (w_pt, h_pt) = (rect.w / figure.scale, rect.h / figure.scale);
        let resolve = |color: pencere_view::Color| {
            let (rgb, alpha) = self.theme.resolve(&color);
            [
                f32::from(rgb.r) / 255.0,
                f32::from(rgb.g) / 255.0,
                f32::from(rgb.b) / 255.0,
                alpha,
            ]
        };
        match self.options.canvases {
            CanvasMode::Picture => {
                let across = self.options.pixels.0 as f32 * cols / w_pt;
                let down = self.options.pixels.1 as f32 * rows / h_pt;
                let scale = across.max(down).min(8192.0 / w_pt.max(h_pt));
                let drawn = Figure {
                    rect: Rect::new(0.0, 0.0, w_pt * scale, h_pt * scale),
                    scale,
                    paths: figure.paths.clone(),
                    clip: figure.clip,
                };
                let Some(bitmap) = draw_figure(&drawn, resolve) else {
                    return;
                };
                let raster = Arc::new(Raster {
                    id: None,
                    width: bitmap.width,
                    height: bitmap.height,
                    pixels: bitmap.pixels().to_vec(),
                    crop: Rect::new(0.0, 0.0, 1.0, 1.0),
                });
                let seen = intersect(rect, &figure.clip);
                let (v0, v1) = (c0.max(self.col(seen.x)), c1.min(self.col(seen.x + seen.w)));
                let (w0, w1) = (r0.max(self.row(seen.y)), r1.min(self.row(seen.y + seen.h)));
                let (v0, w0) = (v0.max(0), w0.max(0));
                let (v1, w1) = (v1.min(self.canvas.cols), w1.min(self.canvas.rows));
                if v1 <= v0 || w1 <= w0 {
                    return;
                }
                let whole = (c0 as f32, r0 as f32, c1 as f32, r1 as f32);
                let part = (v0 as f32, w0 as f32, v1 as f32, w1 as f32);
                let source = shares(whole, part, Rect::new(0.0, 0.0, 1.0, 1.0));
                self.place((v0, w0, v1, w1), &raster, source, None, 1.0, BEHIND_TEXT);
            }
            CanvasMode::Characters => {
                let (dots_across, dots_down) = (cols * 2.0, rows * 4.0);
                // A canvas of a cell or two is an icon, small in the room it was given. Its lines are
                // thinner than a dot, and its ink is what is drawn, to fill the cells; a larger
                // canvas keeps what it is given, and a line is at least a dot wide in it (one that is
                // thinner than a dot would cover too little of any to be one).
                let small = cols * rows <= SMALL;
                let dot = if small {
                    0.0
                } else {
                    (w_pt / dots_across).max(h_pt / dots_down)
                };
                let scale = ((dots_across * SAMPLES / w_pt).max(dots_down * SAMPLES / h_pt))
                    .min(8192.0 / w_pt.max(h_pt));
                let paths = figure
                    .paths
                    .iter()
                    .map(|path| {
                        let mut path = path.clone();
                        if let Some(stroke) = path.stroke.as_mut() {
                            stroke.width = stroke.width.max(dot);
                        }
                        path
                    })
                    .collect();
                let drawn = Figure {
                    rect: Rect::new(0.0, 0.0, w_pt * scale, h_pt * scale),
                    scale,
                    paths,
                    clip: figure.clip,
                };
                let Some(bitmap) = draw_figure(&drawn, resolve) else {
                    return;
                };
                let raster = Raster {
                    id: None,
                    width: bitmap.width,
                    height: bitmap.height,
                    pixels: bitmap.pixels().to_vec(),
                    crop: Rect::new(0.0, 0.0, 1.0, 1.0),
                };
                let ink = if small {
                    let Some(ink) = raster.ink_bounds() else {
                        return;
                    };
                    ink
                } else {
                    Rect::new(0.0, 0.0, 1.0, 1.0)
                };
                // An icon that is flat (a chevron, an arrow: twice as wide as it is tall) reads best in
                // quarter blocks, whose pixels are then square; a squarer one (a cross, a square) in
                // braille, which has twice the rows.
                let flat = small && ink.w * w_pt >= FLAT * ink.h * h_pt;
                for row in r0..r1 {
                    let middle = row * 2 + 1;
                    if middle < clip.y0 || middle > clip.y1 {
                        continue;
                    }
                    for col in c0..c1 {
                        if col < clip.x0 || col >= clip.x1 {
                            continue;
                        }
                        if flat {
                            let first = (((col - c0) * 2) as f32, ((row - r0) * 2) as f32);
                            let quarters = (dots_across, rows * 2.0);
                            self.quarters(col, row, &raster, ink, first, quarters);
                        } else {
                            let first = (((col - c0) * 2) as f32, ((row - r0) * 4) as f32);
                            self.braille(col, row, &raster, ink, first, (dots_across, dots_down));
                        }
                    }
                }
            }
        }
    }

    /// The cell at `col`, `row` drawn as braille from the dots of the raster that start at `first`
    /// (the grid of dots of the whole canvas is `dots` across and down).
    ///
    /// A canvas makes no ground: the ground of a cell is the box under the canvas, as the boxes drew
    /// it. Only the dots a figure covers (`INK`) are braille dots, in the colour of what covers them;
    /// what it covers less (a faint grid, the fill under a line) is left out.
    fn braille(
        &mut self,
        col: i32,
        row: i32,
        raster: &Raster,
        zoom: Rect,
        first: (f32, f32),
        dots: (f32, f32),
    ) {
        let mut ink = Dots::default();
        for (across, column) in DOTS.iter().enumerate() {
            for (down, bit) in column.iter().enumerate() {
                let (x, y) = (first.0 + across as f32, first.1 + down as f32);
                let at =
                    |x: f32, y: f32| (zoom.x + zoom.w * x / dots.0, zoom.y + zoom.h * y / dots.1);
                let [r, g, b, a] = raster.mean(at(x, y), at(x + 1.0, y + 1.0));
                let alpha = a / 255.0;
                if alpha >= INK {
                    ink.add(*bit, [r, g, b], alpha);
                }
            }
        }
        if ink.bits == 0 {
            return;
        }
        let ch = char::from_u32(0x2800 + u32::from(ink.bits)).unwrap_or(' ');
        self.put_figure(col, row, ch, ink.color());
    }

    /// The cell at `col`, `row` of a canvas of a cell or two, drawn as quarter blocks (▘▝▖▗ and the
    /// halves and corners they make): dots are too few of them to read a small shape by. `first` is the
    /// quarter of the canvas the cell's first is, the canvas is `quarters` across and down, and what is
    /// drawn in them is the part of the raster that `zoom` (shares of it) says.
    fn quarters(
        &mut self,
        col: i32,
        row: i32,
        raster: &Raster,
        zoom: Rect,
        first: (f32, f32),
        quarters: (f32, f32),
    ) {
        let mut ink = Dots::default();
        for (bit, (across, down)) in [
            (1_u8, (0.0, 0.0)),
            (2, (1.0, 0.0)),
            (4, (0.0, 1.0)),
            (8, (1.0, 1.0)),
        ] {
            let (x, y) = (first.0 + across, first.1 + down);
            let at = |x: f32, y: f32| {
                (
                    zoom.x + zoom.w * x / quarters.0,
                    zoom.y + zoom.h * y / quarters.1,
                )
            };
            let [r, g, b, a] = raster.mean(at(x, y), at(x + 1.0, y + 1.0));
            let alpha = a / 255.0;
            if alpha >= INK {
                ink.add(bit, [r, g, b], alpha);
            }
        }
        let Some(ch) = QUARTERS
            .get(usize::from(ink.bits))
            .copied()
            .filter(|ch| *ch != ' ')
        else {
            return;
        };
        self.put_figure(col, row, ch, ink.color());
    }

    /// A character of a figure at the cell, in `color`, over the ground the boxes made.
    fn put_figure(&mut self, col: i32, row: i32, ch: char, color: Rgb) {
        if self.canvas.put(col, row, (ch, color, Attrs::NONE), false) == Some(true) {
            self.overlaps += 1;
        }
    }

    /// The planes put together.
    fn finish(self) -> Frame {
        let Self {
            theme,
            canvas,
            placements,
            waiting,
            overlaps,
            ..
        } = self;
        let mut stats = Stats {
            overlaps,
            ..Stats::default()
        };
        let mut cells = Vec::with_capacity(canvas.slots.len());
        for row in 0..canvas.rows {
            for col in 0..canvas.cols {
                let index = canvas.cell(col, row);
                let slot = index
                    .and_then(|i| canvas.slots.get(i).copied())
                    .unwrap_or(Slot::Empty);
                let open = index
                    .and_then(|i| canvas.hole.get(i).copied())
                    .unwrap_or(false);
                let (top, bottom) = (
                    canvas.ground_at(col, row * 2),
                    canvas.ground_at(col, row * 2 + 1),
                );
                let ground = if open {
                    Bg::Default
                } else {
                    Bg::Rgb(canvas.under(col, row))
                };
                let blank = Cell {
                    ch: ' ',
                    fg: theme.foreground,
                    bg: ground,
                    attrs: Attrs::NONE,
                    tail: false,
                };
                cells.push(match slot {
                    Slot::Glyph {
                        ch,
                        fg,
                        attrs,
                        back,
                        ..
                    } => {
                        stats.text_cells += 1;
                        if top != bottom && !open && back.is_none() {
                            stats.split += 1;
                        }
                        Cell {
                            ch,
                            fg,
                            bg: back.map_or(ground, Bg::Rgb),
                            attrs,
                            tail: false,
                        }
                    }
                    Slot::Line { ch, fg } => Cell {
                        ch,
                        fg,
                        bg: ground,
                        ..blank
                    },
                    Slot::Tail => Cell {
                        tail: true,
                        ..blank
                    },
                    Slot::Empty if open || top == bottom => blank,
                    Slot::Empty => Cell {
                        ch: '▀',
                        fg: top,
                        bg: Bg::Rgb(bottom),
                        ..blank
                    },
                });
            }
        }
        Frame {
            grid: Grid::new(canvas.cols as u16, canvas.rows as u16, cells),
            placements,
            links: Vec::new(),
            waiting,
            stats,
        }
    }
}

/// What the two rectangles have in common (nothing, a rectangle with no size).
fn intersect(a: &Rect, b: &Rect) -> Rect {
    let (x0, y0) = (a.x.max(b.x), a.y.max(b.y));
    let (x1, y1) = ((a.x + a.w).min(b.x + b.w), (a.y + a.h).min(b.y + b.h));
    Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

/// What of a picture's pixels the `part` of its box shows, as shares of the pixels: the part as
/// shares of the `whole` box (each `x0, y0, x1, y1`), taken through the `crop` that says what the
/// box shows of them.
fn shares(
    whole: (f32, f32, f32, f32),
    part: (f32, f32, f32, f32),
    crop: Rect,
) -> (f32, f32, f32, f32) {
    let along = |p0: f32, p1: f32, w0: f32, w1: f32, c0: f32, length: f32| {
        let size = (w1 - w0).max(f32::EPSILON);
        (
            c0 + ((p0 - w0) / size).clamp(0.0, 1.0) * length,
            c0 + ((p1 - w0) / size).clamp(0.0, 1.0) * length,
        )
    };
    let (u0, u1) = along(part.0, part.2, whole.0, whole.2, crop.x, crop.w);
    let (v0, v1) = along(part.1, part.3, whole.1, whole.3, crop.y, crop.h);
    (u0, v0, u1, v1)
}
