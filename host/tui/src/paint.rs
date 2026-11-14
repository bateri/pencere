//! A draw list drawn into the grid.
//!
//! The grid is drawn in two layers. Under is a plane of **pixels**: a column across and the half of a
//! row down, each of one colour, which is what boxes, borders and pictures in half blocks are drawn
//! into. Over it is the plane of **characters**: text, braille dots and the eighth blocks that are
//! the lines thinner than half a cell, each in a cell. When the two are put together a cell with no
//! character is a space on one ground or a half block on two, and a cell with one stands on the
//! ground of the half that was drawn last: the box in front is the one the words belong to. A page
//! laid out on the cells has its boxes on cell edges; what it writes smaller is drawn as a line.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use pencere_sandbox::draw_figure;
use pencere_view::{DrawList, Figure, Paint, Picture, Quad, Rect, Run};

use crate::color::Rgb;
use crate::grid::{Attrs, Bg, Cell, Grid};
use crate::measure::char_width;
use crate::output::{self, BEHIND_TEXT, GROUND, OVER_TEXT, Piece, Pixels, Placement};
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
        placed_at: Vec::new(),
        shown: Vec::new(),
        behind: None,
        lines: Vec::new(),
        bullets: Vec::new(),
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
            // An icon on a terminal that draws pictures on its own pixels is the size the page wrote, in the
            // middle of its cells, and is not stretched to them.
            Paint::Image(picture) => match picture.exact {
                Some(exact) if options.images == ImageMode::Kitty => {
                    painter.picture(&picture.refit(exact, picture.crop), true);
                }
                _ => painter.picture(picture, false),
            },
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

/// How far down a cell a line lying across it has its middle, to be a line through the middle and
/// not at the cell's top or bottom: from this far down, to this.
const MIDDLE: (f32, f32) = (0.35, 0.65);

/// The most pixels a picture the painter makes of a rectangle (a mark, a line) may have: a rectangle
/// that would take more is drawn on the cells.
const SLIVER_PIXELS: usize = 1 << 16;

/// The longest side, pixels, a picture is asked for as a whole at (the sandbox draws a part of one
/// that is more, as large as a side of its own holds), and the most pixels it has there.
const WHOLE_SIDE: f32 = 3900.0;
const WHOLE_PIXELS: f32 = 12_000_000.0;

/// The side of the pieces a picture is cut in where it is more than a screen holds ([`WHOLE_SIDE`],
/// [`WHOLE_PIXELS`]), pixels of the terminal: a grid laid over the picture that does not move, so that
/// a piece that is seen again is the same pixels (and is not sent again) whichever part of the page
/// they were drawn in.
const TILE: u32 = 1024;

/// How far from the size a picture is drawn at the pixels its pieces are cut from may be, as a share
/// of it.
const TILE_SCALE: f32 = 0.1;

/// How far from the page's own proportions a terminal's cell may be for a picture to be drawn at
/// the pixels the page laid it out at, as a share: a cell of 13 by 29 pixels is 0.9% off the page's
/// 8 by 18, which no eye sees in a page, and the edges of the pages then stand where the layout has
/// them; a cell that is further off draws a picture the largest it can be in its box with its
/// proportions kept (a square is square).
const PROPORTION_SLACK: f32 = 0.01;

/// How many times larger a picture is drawn than its raster is, at the most, to be sent as pixels of
/// its own size: a raster that is less than that (the page that stands in for one that is being
/// drawn) is sent as it is and the terminal stretches it.
const MOST_STRETCH: f32 = 2.0;

/// Where in a cell a line stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    Left,
    Right,
    Top,
    Bottom,
    /// The middle of the cell, lying: a bar that stands beside a label on the label's row.
    Middle,
    /// The same, as thick as an eighth of the cell or more.
    MiddleHeavy,
}

impl Edge {
    fn glyph(self) -> char {
        match self {
            Self::Left => '\u{258f}',
            Self::Right => '\u{2595}',
            Self::Top => '\u{2594}',
            Self::Bottom => '\u{2581}',
            Self::Middle => '\u{2500}',
            Self::MiddleHeavy => '\u{2501}',
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
    /// When each pixel was last given a ground that hides what is under it (the count of the item).
    solid: Vec<u32>,
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
            solid: vec![0; cols * rows * 2],
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

    /// Whether an item after the `tick`th gave a ground that hides what is under it to any pixel of
    /// the cells (`col`, `row`, `cols`, `rows`).
    fn hidden_after(&self, (col, row, cols, rows): (i32, i32, i32, i32), tick: u32) -> bool {
        (col..col + cols).any(|x| {
            (row * 2..(row + rows) * 2).any(|y| {
                self.pixel(x, y)
                    .and_then(|i| self.solid.get(i))
                    .is_some_and(|solid| *solid > tick)
            })
        })
    }

    /// The boxes of cells (`col`, `row`, the column and the row after the last) of the cells (`col`,
    /// `row`, `cols`, `rows`) that no item after the `tick`th gave a ground that hides what is under
    /// it, each row's runs joined with the same run of the row under it.
    fn open_runs(
        &self,
        (col, row, cols, rows): (i32, i32, i32, i32),
        tick: u32,
    ) -> Vec<(i32, i32, i32, i32)> {
        let mut boxes: Vec<(i32, i32, i32, i32)> = Vec::new();
        for y in row..row + rows {
            let mut x = col;
            while x < col + cols {
                if self.hidden_after((x, y, 1, 1), tick) {
                    x += 1;
                    continue;
                }
                let start = x;
                while x < col + cols && !self.hidden_after((x, y, 1, 1), tick) {
                    x += 1;
                }
                match boxes
                    .iter_mut()
                    .find(|(c0, r0, c1, r1)| *c0 == start && *c1 == x && *r1 == y && *r0 <= y)
                {
                    Some((_, _, _, r1)) => *r1 = y + 1,
                    None => boxes.push((start, y, x, y + 1)),
                }
            }
        }
        boxes
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
        if alpha >= 0.98
            && let Some(solid) = self.solid.get_mut(i)
        {
            *solid = tick;
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

/// A line lying along cells that have words in them, to be drawn when the list is done.
struct Line {
    /// The line (pixels of the page), cut by its clip.
    rect: Rect,
    row: i32,
    /// The columns it passes over, and the edge of its cell it is nearest to.
    cols: Vec<i32>,
    edge: Edge,
    color: (Rgb, f32),
    /// The count of the item it was drawn by.
    at: u32,
}

/// A dot the page drew (a small round fill: the mark of a space in the selection), to be put in its
/// cell as a character (`·`) once the whole list has been, where the cell holds nothing but a space.
struct Bullet {
    col: i32,
    row: i32,
    color: (Rgb, f32),
    /// The count of the item it was drawn by.
    at: u32,
}

/// The ground that was under the last box given a solid fill, as it was before the fill.
struct Behind {
    /// The box (pixels of the page) and the count of the item that filled it.
    rect: Rect,
    tick: u32,
    /// The pixels of the grid that were kept, row by row, and the rows' length.
    span: Span,
    ground: Vec<Rgb>,
}

/// How a picture at the pixels of the terminal lies: the part of the screen it is seen on (whole
/// pixels of the terminal), whether it covers what is under it altogether, and whether it is smaller
/// than its box (its proportions kept on the terminal's own pixels).
#[derive(Clone, Copy)]
struct Landing {
    pixels: (u32, u32, u32, u32),
    covers: bool,
    inset: bool,
}

/// Where a picture that is too large to be held whole is cut: the grid of [`TILE`] over it, and the
/// pieces of it that are seen.
struct Tiles {
    /// What names the picture (its file, its page), with `whole`: the same piece is the same pixels.
    key: u64,
    /// The whole picture as large as it is drawn, and where its top-left corner is, pixels of the
    /// terminal.
    whole: (u32, u32),
    corner: (i64, i64),
    /// The pieces that are seen: the columns and the rows of the grid, the last not included.
    columns: (u32, u32),
    rows: (u32, u32),
    /// What the pieces come to of the picture, as shares of it: what is asked of whoever has the
    /// pixels.
    shown: Rect,
    /// The part of the screen the picture is seen in, pixels of the terminal.
    seen: Rect,
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
    /// The count of the item each of `placements` was made by.
    placed_at: Vec<u32>,
    /// The pictures the terminal draws over the cells they cover, each as the box the page has it in
    /// and as the rectangle it is drawn on (the proportions it has on the terminal's pixels).
    shown: Vec<(Rect, Rect)>,
    /// What was under the box that was filled last: where a picture in that box covers a part of a
    /// cell, the rest of the cell is what was behind the box and not the box's own ground.
    behind: Option<Behind>,
    /// The lines along cells that have words in them, which are drawn once the whole list has been:
    /// whether the words cut a line is what the cells that nothing is laid over say.
    lines: Vec<Line>,
    /// The dots the page drew, which are put in their cells once the whole list has been.
    bullets: Vec<Bullet>,
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
            self.placed_at.push(self.canvas.tick);
            self.placements.push(Placement {
                col: c0 as u16,
                row: r0 as u16,
                cols: (c1 - c0) as u16,
                rows: (r1 - r0) as u16,
                offset: None,
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

    /// Where a picture that is the pixels `x0, y0` to `x1, y1` of the terminal is put: the cell its
    /// corner is in, the cells it touches (as many as are on the screen), and how many pixels its
    /// corner is from the corner of its cell.
    fn in_cells(&self, (x0, y0, x1, y1): (u32, u32, u32, u32)) -> (u16, u16, u16, u16, (u32, u32)) {
        let (cell_w, cell_h) = self.cell_pixels();
        let (col, row) = (x0 / cell_w, y0 / cell_h);
        let (dx, dy) = (x0 % cell_w, y0 % cell_h);
        let cols = (dx + (x1 - x0)).div_ceil(cell_w);
        let rows = (dy + (y1 - y0)).div_ceil(cell_h);
        let room = |from: u32, count: u32, limit: i32| -> u16 {
            count.min((limit.max(0) as u32).saturating_sub(from)).max(1) as u16
        };
        (
            col as u16,
            row as u16,
            room(col, cols, self.canvas.cols),
            room(row, rows, self.canvas.rows),
            (dx, dy),
        )
    }

    /// How many pixels a cell of the terminal is across and down.
    fn cell_pixels(&self) -> (u32, u32) {
        (self.options.pixels.0.max(1), self.options.pixels.1.max(1))
    }

    /// `full` (pixels of the page) as a picture of the terminal's own, made at the pixels it is: as
    /// many as it is across and down, in the colours of `fill` and with a border of `border` (the
    /// colour and the thickness) along its edges, and put where it stands, to the pixel, with the part
    /// of it that `seen` (inside `full`) is. A picture is as sharp as the terminal's pixels where
    /// the characters it is drawn over are a cell; it is named by its size and its colours and not by
    /// where it is, so that one that moves is sent once. It is not made (nothing is drawn, and this
    /// says so) where the terminal has no pictures, and for a rectangle that would take more pixels
    /// than a mark or a line has any need of.
    fn sliver(
        &mut self,
        full: Rect,
        seen: Rect,
        fill: Option<(Rgb, f32)>,
        border: Option<(Rgb, f32, f32)>,
        z: i32,
    ) -> bool {
        if self.options.images != ImageMode::Kitty {
            return false;
        }
        let sound = |length: f32| length.is_finite() && length > 0.0;
        if !(sound(full.w) && sound(full.h) && sound(seen.w) && sound(seen.h)) {
            return false;
        }
        let (cell_w, cell_h) = self.cell_pixels();
        let (across, down) = (cell_w as f32 / self.cw, cell_h as f32 / self.rh);
        let page = Rect::new(
            0.0,
            0.0,
            self.canvas.cols as f32 * self.cw,
            self.canvas.rows as f32 * self.rh,
        );
        let seen = intersect(&seen, &page);
        if seen.w <= 0.0 || seen.h <= 0.0 {
            return false;
        }
        // The rectangle's own pixels: the corner where the page says it is (rounded to a pixel, apart
        // from its size), and the size.
        let (origin_x, origin_y) = (
            (full.x * across).round() as i64,
            (full.y * down).round() as i64,
        );
        let width = ((full.w * across).round() as u32).max(1);
        let height = ((full.h * down).round() as u32).max(1);
        if width as usize * height as usize > SLIVER_PIXELS {
            return false;
        }
        // The part that shows, each side to the pixel it is nearest, and at least a pixel across.
        let side = |from: f32, to: f32, origin: i64, length: u32, scale: f32| -> (i64, i64) {
            let first =
                ((from * scale).round() as i64).clamp(origin, origin + i64::from(length) - 1);
            let last = ((to * scale).round() as i64).clamp(first + 1, origin + i64::from(length));
            (first, last)
        };
        let (x0, x1) = side(seen.x, seen.x + seen.w, origin_x, width, across);
        let (y0, y1) = side(seen.y, seen.y + seen.h, origin_y, height, down);
        let premultiplied = |(rgb, alpha): (Rgb, f32)| -> [u8; 4] {
            let a = alpha.clamp(0.0, 1.0);
            let scaled = |c: u8| (f32::from(c) * a).round().clamp(0.0, 255.0) as u8;
            [
                scaled(rgb.r),
                scaled(rgb.g),
                scaled(rgb.b),
                (a * 255.0).round() as u8,
            ]
        };
        let mut pixels = vec![0_u8; width as usize * height as usize * 4];
        let edge = border.map_or(0, |(_, _, line)| {
            ((line / self.cw * cell_w as f32).round() as u32).max(1)
        });
        for y in 0..height {
            for x in 0..width {
                let on_border = border.is_some()
                    && (x < edge || x + edge >= width || y < edge || y + edge >= height);
                let color = if on_border {
                    border.map(|(rgb, alpha, _)| premultiplied((rgb, alpha)))
                } else {
                    fill.map(premultiplied)
                };
                if let Some(color) = color {
                    let at = (y as usize * width as usize + x as usize) * 4;
                    if let Some(pixel) = pixels.get_mut(at..at + 4) {
                        pixel.copy_from_slice(&color);
                    }
                }
            }
        }
        let raster = Raster {
            id: None,
            width,
            height,
            pixels,
            crop: Rect::new(0.0, 0.0, 1.0, 1.0),
        };
        let (Ok(px0), Ok(py0), Ok(px1), Ok(py1)) = (
            u32::try_from(x0),
            u32::try_from(y0),
            u32::try_from(x1),
            u32::try_from(y1),
        ) else {
            return false;
        };
        let (col, row, cols, rows, offset) = self.in_cells((px0, py0, px1, py1));
        self.placed_at.push(self.canvas.tick);
        self.placements.push(Placement {
            col,
            row,
            cols,
            rows,
            offset: Some(offset),
            pixels: Pixels::of(Arc::new(raster), None, 1.0),
            x: (x0 - origin_x) as u32,
            y: (y0 - origin_y) as u32,
            width: (x1 - x0) as u32,
            height: (y1 - y0) as u32,
            z,
        });
        true
    }

    /// Where `rect`, a place on a picture the terminal draws (in the page's pixels, on the picture's box
    /// as the page has it), is on what it draws: the picture is drawn on a rectangle of its own that
    /// keeps its proportions on the terminal's pixels, so that a place in it is moved and scaled as the
    /// picture is.
    fn as_drawn(&self, rect: Rect) -> Rect {
        self.as_drawn_by(rect, rect)
    }

    /// The same, for the picture that `by` is on (its middle is on it): for a rectangle and a part of
    /// it, which are on one picture.
    fn as_drawn_by(&self, rect: Rect, by: Rect) -> Rect {
        let centre = (by.x + by.w / 2.0, by.y + by.h / 2.0);
        let Some((whole, drawn)) = self.shown.iter().rev().find(|(whole, _)| {
            centre.0 >= whole.x
                && centre.0 <= whole.x + whole.w
                && centre.1 >= whole.y
                && centre.1 <= whole.y + whole.h
        }) else {
            return rect;
        };
        let (across, down) = (
            drawn.w / whole.w.max(f32::EPSILON),
            drawn.h / whole.h.max(f32::EPSILON),
        );
        Rect::new(
            drawn.x + (rect.x - whole.x) * across,
            drawn.y + (rect.y - whole.y) * down,
            rect.w * across,
            rect.h * down,
        )
    }

    /// A box that is not opaque, over cells a picture of the terminal's shows through all of
    /// (a mark on the word of a page of a PDF): it is a picture of its own over that one, as large
    /// as the box and standing where it does, and not the cells it touches. Says whether it was
    /// drawn so; one that is not (a box over other cells too, a terminal with no pictures) is drawn
    /// on the cells.
    fn tinted_picture(&mut self, quad: &Quad) -> bool {
        let Some(fill) = quad.fill else {
            return false;
        };
        if self.options.images != ImageMode::Kitty
            || quad.gradient.is_some()
            || quad.shadow.is_some()
        {
            return false;
        }
        let (rgb, alpha) = self.theme.resolve(&fill);
        if alpha >= 0.98 {
            return false;
        }
        let border = match quad.border {
            None => None,
            Some(border) if border.width < self.cw / 2.0 && border.width < self.rh / 2.0 => {
                let (color, strength) = self.theme.resolve(&border.color);
                Some((color, strength, border.width))
            }
            Some(_) => return false,
        };
        // The box where the page wrote it where there is one (a place on a picture, which cells are
        // too coarse to say), and what shows of it, cut at its clip.
        let full = quad.exact.unwrap_or(quad.rect);
        let seen = intersect(&full, &quad.clip);
        if seen.w <= 0.0 || seen.h <= 0.0 {
            return false;
        }
        let full = self.as_drawn_by(full, seen);
        let seen = self.as_drawn(seen);
        let (c0, r0) = (
            (seen.x / self.cw).floor() as i32,
            (seen.y / self.rh).floor() as i32,
        );
        let c1 = ((seen.x + seen.w) / self.cw).ceil() as i32;
        let r1 = ((seen.y + seen.h) / self.rh).ceil() as i32;
        let through = (r0..r1).all(|row| {
            (c0..c1).all(|col| {
                self.canvas
                    .cell(col, row)
                    .and_then(|cell| self.canvas.hole.get(cell).copied())
                    .unwrap_or(false)
            })
        });
        through && self.sliver(full, seen, Some((rgb, alpha)), border, BEHIND_TEXT)
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
            // At an edge of its cell, the line of that edge; about the middle of it (a bar the
            // page put beside a label, centred in its row), a line through the middle.
            let edge = match centre - centre.floor() {
                above if above < MIDDLE.0 => Edge::Top,
                above if above >= MIDDLE.1 => Edge::Bottom,
                _ if rect.h >= self.rh / 8.0 => Edge::MiddleHeavy,
                _ => Edge::Middle,
            };
            let (c0, c1) = (self.col(rect.x), self.col(rect.x + rect.w));
            let cells: Vec<i32> = (c0..c1.max(c0 + 1))
                .filter(|col| Self::within(clip, *col, row))
                .collect();
            // A line along cells that have words in them is left until the list is done: if the words
            // are in cells that something is laid over, it is the characters of the cells that are
            // free; else where the terminal has pictures it is a picture of its own over the words,
            // and where it has none it is not drawn (a line with gaps in it is nowhere to be seen). A
            // line over the cells of one that is left is left too, to be drawn after it, as it is
            // over it in the list (a gauge's fill over its track).
            let behind = self.lines.iter().any(|earlier| {
                earlier.row == row && earlier.cols.iter().any(|col| cells.contains(col))
            });
            if behind || cells.iter().any(|col| self.has_text(*col, row)) {
                self.lines.push(Line {
                    rect: intersect(&quad.rect, &quad.clip),
                    row,
                    cols: cells,
                    edge,
                    color,
                    at: self.canvas.tick,
                });
                return true;
            }
            for col in cells {
                self.mark(col, row, edge, color);
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

    /// A small round fill (thinner than half a cell across and down, and round) is a dot: the mark an
    /// editor puts on a space it shows. It is the character `·` in the cell of its middle, where the cell
    /// holds a space, and not a block of the half cell a fill of any size is drawn with.
    fn bullet(&mut self, quad: &Quad) -> bool {
        let (Some(fill), rect) = (quad.fill, quad.rect) else {
            return false;
        };
        let sized = |length: f32| length.is_finite() && length > 0.0;
        let round = quad
            .radii
            .iter()
            .all(|radius| *radius >= rect.w.min(rect.h) / 2.0 - 0.01);
        if !(sized(rect.w) && sized(rect.h))
            || rect.w >= self.cw / 2.0
            || rect.h >= self.rh / 2.0
            || !round
            || quad.gradient.is_some()
            || quad.border.is_some()
            || quad.shadow.is_some()
        {
            return false;
        }
        let (col, row) = (
            ((rect.x + rect.w / 2.0) / self.cw).floor() as i32,
            ((rect.y + rect.h / 2.0) / self.rh).floor() as i32,
        );
        if Self::within(self.span(&quad.clip), col, row) {
            self.bullets.push(Bullet {
                col,
                row,
                color: self.theme.resolve(&fill),
                at: self.canvas.tick,
            });
        }
        true
    }

    fn quad(&mut self, quad: &Quad) {
        if self.bullet(quad) || self.hairline(quad) || self.tinted_picture(quad) {
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
                self.keep_behind(quad.rect, visible);
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

    /// `written`: the picture is an icon at the place the page wrote it (`Picture::exact`), which is drawn
    /// at the pixels of the terminal as any picture is; an icon the page wrote no place for fills the
    /// cells nearest its box, as a character does.
    fn picture(&mut self, picture: &Picture, written: bool) {
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
        // Where a terminal that draws pictures draws this one: the box it is in, with its proportions
        // kept on the terminal's pixels, which are not the page's (a cell is not a 1 to 2 of them, and
        // the picture is not stretched to the cells it touches).
        let (drawn, crop) = if mode == ImageMode::Kitty {
            self.fitted(picture, written)
        } else {
            (picture.rect, picture.crop)
        };
        let asked = if mode == ImageMode::Kitty {
            picture.refit(drawn, crop)
        } else {
            picture.clone()
        };
        // A picture is drawn at its pixels, where they are, and an icon the page wrote a place for is no other
        // (it is the size the page wrote, in the middle of its cells, and is not stretched to them): it is asked for as the
        // whole of the picture where that is not more than a screen holds (and not for the part that
        // is seen, which is another picture at each step of a scroll), so that what the terminal is
        // sent is the same pixels for every frame of it. One that turns (a spinner) is put over cells.
        let in_pixels =
            mode == ImageMode::Kitty && picture.turn == 0.0 && (picture.tint.is_none() || written);
        let mut request = self.request(&asked);
        // Where it is more than that, it is asked for in the pieces of a grid that is over it (what is
        // seen, rounded out to them), which are the same pixels for each frame of a scroll.
        let mut tiles = None;
        // A picture whose proportions are kept on the terminal's own pixels (it is not at the pixels the
        // layout has it at) is the whole of what the page's box shows of it: the box's fill beyond it
        // is not drawn.
        let inset = in_pixels && (drawn != picture.rect || crop != picture.crop);
        if in_pixels {
            let all = request.full();
            if all.w.max(all.h) <= WHOLE_SIDE && all.w * all.h <= WHOLE_PIXELS {
                request.shown = Rect::new(0.0, 0.0, 1.0, 1.0);
            } else if let Some(cut) = self.tiles_of(picture, drawn, crop) {
                request.shown = cut.shown;
                tiles = Some(cut);
            }
        }
        let Some(raster) = self.pictures.raster(&request) else {
            self.waiting = true;
            return;
        };
        let raster = Arc::new(raster);
        let (rx0, ry0) = (drawn.x, drawn.y);
        let (rx1, ry1) = (rx0 + drawn.w, ry0 + drawn.h);
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
                if in_pixels {
                    let drawn_in_pixels = match &tiles {
                        Some(cut) => self.picture_in_tiles(picture, drawn, &raster, cut, inset),
                        None => self.picture_in_pixels(picture, drawn, &raster, inset),
                    };
                    if drawn_in_pixels {
                        return;
                    }
                }
                // The cells the picture is seen in: those that the part of it its clip leaves
                // touches.
                let seen = intersect(&drawn, &picture.clip);
                if seen.w <= 0.0 || seen.h <= 0.0 {
                    return;
                }
                // An icon (a tinted picture) fills the cells nearest its box, as a character does; any
                // other picture is seen in every cell it touches, and in no cell that is not its
                // box's.
                let icon = picture.tint.is_some();
                let (mut c0, mut r0, mut c1, mut r1) = if icon {
                    (
                        self.col(seen.x),
                        self.row(seen.y),
                        self.col(seen.x + seen.w),
                        self.row(seen.y + seen.h),
                    )
                } else {
                    (
                        ((seen.x / self.cw) + 0.0001).floor() as i32,
                        ((seen.y / self.ch()) + 0.0001).floor() as i32,
                        (((seen.x + seen.w) / self.cw) - 0.0001).ceil() as i32,
                        (((seen.y + seen.h) / self.ch()) - 0.0001).ceil() as i32,
                    )
                };
                if !icon && let Some((b0, b1, b2, b3)) = self.box_cells(picture) {
                    (c0, r0, c1, r1) = (c0.max(b0), r0.max(b1), c1.min(b2), r1.min(b3));
                }
                let (c0, r0) = (c0.max(0), r0.max(0));
                let (c1, r1) = (
                    c1.max(c0 + 1).min(self.canvas.cols),
                    r1.max(r0 + 1).min(self.canvas.rows),
                );
                if c1 <= c0 || r1 <= r0 {
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
                    self.shown.push((picture.rect, drawn));
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

    /// The cells of the box the grid made for a picture (columns and rows, the last not included),
    /// on the screen; `None` where it has none.
    fn box_cells(&self, picture: &Picture) -> Option<(i32, i32, i32, i32)> {
        let span = self.solid(&picture.cells)?;
        Some((
            span.x0.max(0),
            span.y0.div_euclid(2).max(0),
            span.x1.min(self.canvas.cols),
            (span.y1 + 1).div_euclid(2).min(self.canvas.rows),
        ))
    }

    /// A picture, an icon too, at the pixels of the terminal: the pixels of the raster made as
    /// large as the picture is drawn (named by what they are of and that size, and never by where they
    /// stand), and put at the pixel the layout has the picture at, where it is seen (in its clip and on
    /// the screen: the cells of its box are not what it is cut at); a picture that is moved is only put
    /// again. The cells it covers a part of are the terminal's, and the rest of each is the ground that
    /// was behind the picture's box. Says whether it did: a raster that is far smaller than the size it
    /// is drawn at (the page that stands in for one that is drawn) is stretched over the cells instead,
    /// as the terminal does it for less than it would be sent as pixels.
    fn picture_in_pixels(
        &mut self,
        picture: &Picture,
        drawn: Rect,
        raster: &Arc<Raster>,
        inset: bool,
    ) -> bool {
        let (cell_w, cell_h) = self.cell_pixels();
        let (across, down) = (cell_w as f32 / self.cw, cell_h as f32 / self.ch());
        let sound = |length: f32| length.is_finite() && length > 0.0;
        let crop = raster.crop;
        if !(sound(across)
            && sound(down)
            && sound(drawn.w)
            && sound(drawn.h)
            && sound(crop.w)
            && sound(crop.h)
            && raster.width > 0
            && raster.height > 0)
        {
            return false;
        }
        let px = |rect: Rect| {
            Rect::new(
                rect.x * across,
                rect.y * down,
                rect.w * across,
                rect.h * down,
            )
        };
        // The whole raster as large as it is drawn, and where its corner is.
        let on_screen = px(drawn);
        let (wide, high) = (on_screen.w / crop.w, on_screen.h / crop.h);
        if wide / raster.width as f32 > MOST_STRETCH || high / raster.height as f32 > MOST_STRETCH {
            return false;
        }
        let size = (wide.round().max(1.0) as u32, high.round().max(1.0) as u32);
        let origin = (
            (on_screen.x - crop.x * wide).round() as i64,
            (on_screen.y - crop.y * high).round() as i64,
        );
        // What is seen of it: the part inside the clip and on the screen.
        let screen = Rect::new(
            0.0,
            0.0,
            (self.canvas.cols as u32 * cell_w) as f32,
            (self.canvas.rows as u32 * cell_h) as f32,
        );
        let seen = intersect(&intersect(&on_screen, &px(picture.clip)), &screen);
        if seen.w <= 0.0 || seen.h <= 0.0 {
            return true;
        }
        let side = |from: f32, to: f32, origin: i64, length: u32| -> (i64, i64) {
            let first = (from.round() as i64).clamp(origin, origin + i64::from(length) - 1);
            let last = (to.round() as i64).clamp(first + 1, origin + i64::from(length));
            (first, last)
        };
        let (x0, x1) = side(seen.x, seen.x + seen.w, origin.0, size.0);
        let (y0, y1) = side(seen.y, seen.y + seen.h, origin.1, size.1);
        let (Ok(px0), Ok(py0), Ok(px1), Ok(py1)) = (
            u32::try_from(x0),
            u32::try_from(y0),
            u32::try_from(x1),
            u32::try_from(y1),
        ) else {
            return true;
        };
        let (col, row, cols, rows, offset) = self.in_cells((px0, py0, px1, py1));
        let opacity = picture.opacity.clamp(0.0, 1.0);
        let tint = picture.tint.map(|tint| self.theme.resolve(&tint));
        let covers = tint.is_none() && opacity >= 0.999 && raster.is_opaque();
        let shown = Placement {
            col,
            row,
            cols,
            rows,
            offset: Some(offset),
            pixels: Pixels::scaled(Arc::clone(raster), size, tint, opacity),
            x: (x0 - origin.0) as u32,
            y: (y0 - origin.1) as u32,
            width: (x1 - x0) as u32,
            height: (y1 - y0) as u32,
            z: if covers { GROUND } else { BEHIND_TEXT },
        };
        let landing = Landing {
            pixels: (px0, py0, px1, py1),
            covers,
            inset,
        };
        self.lay_in_pixels(picture, drawn, raster, landing, vec![shown]);
        true
    }

    /// Where a picture that is too large to be held whole is cut, and which pieces of the grid are
    /// seen of it; `None` where it is not seen (or has no size).
    fn tiles_of(&self, picture: &Picture, drawn: Rect, crop: Rect) -> Option<Tiles> {
        let (cell_w, cell_h) = self.cell_pixels();
        let (across, down) = (cell_w as f32 / self.cw, cell_h as f32 / self.ch());
        let sound = |length: f32| length.is_finite() && length > 0.0;
        if !(sound(across)
            && sound(down)
            && sound(drawn.w)
            && sound(drawn.h)
            && sound(crop.w)
            && sound(crop.h))
        {
            return None;
        }
        let px = |rect: Rect| {
            Rect::new(
                rect.x * across,
                rect.y * down,
                rect.w * across,
                rect.h * down,
            )
        };
        let on_screen = px(drawn);
        let (wide, high) = (on_screen.w / crop.w, on_screen.h / crop.h);
        let whole = (wide.round().max(1.0) as u32, high.round().max(1.0) as u32);
        let corner = (
            (on_screen.x - crop.x * wide).round() as i64,
            (on_screen.y - crop.y * high).round() as i64,
        );
        let screen = Rect::new(
            0.0,
            0.0,
            (self.canvas.cols as u32 * cell_w) as f32,
            (self.canvas.rows as u32 * cell_h) as f32,
        );
        let seen = intersect(&intersect(&on_screen, &px(picture.clip)), &screen);
        if seen.w <= 0.0 || seen.h <= 0.0 {
            return None;
        }
        // What is seen of the picture itself, in its own pixels, and the pieces of the grid it is in.
        let along = |from: f32, to: f32, corner: i64, whole: u32| -> Option<(u32, u32)> {
            let first = (from.floor() as i64 - corner).clamp(0, i64::from(whole));
            let last = (to.ceil() as i64 - corner).clamp(0, i64::from(whole));
            (last > first).then_some((first as u32, last as u32))
        };
        let (a0, a1) = along(seen.x, seen.x + seen.w, corner.0, whole.0)?;
        let (b0, b1) = along(seen.y, seen.y + seen.h, corner.1, whole.1)?;
        let columns = (a0 / TILE, a1.div_ceil(TILE));
        let rows = (b0 / TILE, b1.div_ceil(TILE));
        let (x0, x1) = (columns.0 * TILE, (columns.1 * TILE).min(whole.0));
        let (y0, y1) = (rows.0 * TILE, (rows.1 * TILE).min(whole.1));
        let share =
            |from: u32, to: u32, of: u32| (from as f32 / of as f32, (to - from) as f32 / of as f32);
        let ((sx, sw), (sy, sh)) = (share(x0, x1, whole.0), share(y0, y1, whole.1));
        let mut hasher = DefaultHasher::new();
        (picture.src.as_str(), picture.page, whole).hash(&mut hasher);
        Some(Tiles {
            key: hasher.finish(),
            whole,
            corner,
            columns,
            rows,
            shown: Rect::new(sx, sy, sw, sh),
            seen,
        })
    }

    /// A picture that is too large to be held whole, at the pixels of the terminal: the pieces of the
    /// grid laid over it that are seen, each as the pixels of the raster it is cut from (named by what
    /// the picture is, how large it is drawn, and where in it the piece is, so that a piece that is
    /// seen again is not sent again, whichever part of the page the raster is), put at the pixel the
    /// layout has the picture at. A piece the raster does not hold is not drawn (the ground is seen
    /// where it is) and a raster that holds none of them is not drawn this way: says whether it was.
    fn picture_in_tiles(
        &mut self,
        picture: &Picture,
        drawn: Rect,
        raster: &Arc<Raster>,
        tiles: &Tiles,
        inset: bool,
    ) -> bool {
        let (cell_w, cell_h) = self.cell_pixels();
        let (across, down) = (cell_w as f32 / self.cw, cell_h as f32 / self.ch());
        let sound = |length: f32| length.is_finite() && length > 0.0;
        let crop = raster.crop;
        if !(sound(crop.w) && sound(crop.h) && raster.width > 0 && raster.height > 0) {
            return false;
        }
        let on_screen = Rect::new(
            drawn.x * across,
            drawn.y * down,
            drawn.w * across,
            drawn.h * down,
        );
        // Where the raster is in the picture as it is drawn: from its corner, pixels of the terminal.
        let (wide, high) = (on_screen.w / crop.w, on_screen.h / crop.h);
        let mut left = on_screen.x - crop.x * wide - tiles.corner.0 as f32;
        let mut top = on_screen.y - crop.y * high - tiles.corner.1 as f32;
        let (mut width, mut height) = (wide, high);
        let (raster_w, raster_h) = (raster.width as f32, raster.height as f32);
        if (width - raster_w).abs() < 1.0 && (height - raster_h).abs() < 1.0 {
            // Drawn at the size it is shown at: its pixels are the picture's, one to one.
            (left, top, width, height) = (left.round(), top.round(), raster_w, raster_h);
        }
        let step = (raster_w / width, raster_h / height);
        // Pixels that are not nearly the size the picture is drawn at (the page that stands in for a
        // part not yet drawn, or the most the sandbox draws of a side) are not kept as its pieces
        // for good: the picture is stretched over its cells until the pixels come.
        if !(1.0 - TILE_SCALE..=1.0 + TILE_SCALE).contains(&step.0)
            || !(1.0 - TILE_SCALE..=1.0 + TILE_SCALE).contains(&step.1)
        {
            self.waiting = true;
            return false;
        }
        // The pieces whose every pixel it holds.
        let held = |piece: u32, whole: u32, from: f32, length: f32| {
            let (a, b) = (piece * TILE, ((piece + 1) * TILE).min(whole));
            a as f32 >= from - 0.5 && b as f32 <= from + length + 0.5
        };
        let columns: Vec<u32> = (tiles.columns.0..tiles.columns.1)
            .filter(|&column| held(column, tiles.whole.0, left, width))
            .collect();
        let rows: Vec<u32> = (tiles.rows.0..tiles.rows.1)
            .filter(|&row| held(row, tiles.whole.1, top, height))
            .collect();
        let (Some(&c0), Some(&c1), Some(&r0), Some(&r1)) =
            (columns.first(), columns.last(), rows.first(), rows.last())
        else {
            self.waiting = true;
            return false;
        };
        if columns.len() < (tiles.columns.1 - tiles.columns.0) as usize
            || rows.len() < (tiles.rows.1 - tiles.rows.0) as usize
        {
            self.waiting = true;
        }
        let piece = |first: u32, last: u32, whole: u32, corner: i64| {
            (
                corner + i64::from(first * TILE),
                corner + i64::from(((last + 1) * TILE).min(whole)),
            )
        };
        let (held_x0, held_x1) = piece(c0, c1, tiles.whole.0, tiles.corner.0);
        let (held_y0, held_y1) = piece(r0, r1, tiles.whole.1, tiles.corner.1);
        // What is seen of those, in whole pixels of the terminal.
        let seen = tiles.seen;
        let x0 = (seen.x.round() as i64).max(held_x0);
        let x1 = ((seen.x + seen.w).round() as i64).min(held_x1);
        let y0 = (seen.y.round() as i64).max(held_y0);
        let y1 = ((seen.y + seen.h).round() as i64).min(held_y1);
        let (Ok(px0), Ok(py0), Ok(px1), Ok(py1)) = (
            u32::try_from(x0),
            u32::try_from(y0),
            u32::try_from(x1),
            u32::try_from(y1),
        ) else {
            return true;
        };
        if px1 <= px0 || py1 <= py0 {
            return true;
        }
        let opacity = picture.opacity.clamp(0.0, 1.0);
        let covers = opacity >= 0.999 && raster.is_opaque();
        let mut placed = Vec::new();
        for row in r0..=r1 {
            for column in c0..=c1 {
                let (a0, a1) = (column * TILE, ((column + 1) * TILE).min(tiles.whole.0));
                let (b0, b1) = (row * TILE, ((row + 1) * TILE).min(tiles.whole.1));
                let (tx0, ty0) = (
                    tiles.corner.0 + i64::from(a0),
                    tiles.corner.1 + i64::from(b0),
                );
                let (tx1, ty1) = (
                    tiles.corner.0 + i64::from(a1),
                    tiles.corner.1 + i64::from(b1),
                );
                let (sx0, sx1) = (x0.max(tx0), x1.min(tx1));
                let (sy0, sy1) = (y0.max(ty0), y1.min(ty1));
                let (Ok(u0), Ok(v0), Ok(u1), Ok(v1)) = (
                    u32::try_from(sx0),
                    u32::try_from(sy0),
                    u32::try_from(sx1),
                    u32::try_from(sy1),
                ) else {
                    continue;
                };
                if u1 <= u0 || v1 <= v0 {
                    continue;
                }
                let (col, row_at, cols, rows, offset) = self.in_cells((u0, v0, u1, v1));
                placed.push(Placement {
                    col,
                    row: row_at,
                    cols,
                    rows,
                    offset: Some(offset),
                    pixels: Pixels::tile(
                        Arc::clone(raster),
                        Piece {
                            key: tiles.key,
                            whole: tiles.whole,
                            at: (a0, b0),
                            size: (a1 - a0, b1 - b0),
                            scale: (
                                (step.0 * 20.0).round() as u32,
                                (step.1 * 20.0).round() as u32,
                            ),
                        },
                        ((a0 as f32 - left) * step.0, (b0 as f32 - top) * step.1),
                        step,
                        opacity,
                    ),
                    x: (sx0 - tx0) as u32,
                    y: (sy0 - ty0) as u32,
                    width: u1 - u0,
                    height: v1 - v0,
                    z: if covers { GROUND } else { BEHIND_TEXT },
                });
            }
        }
        let landing = Landing {
            pixels: (px0, py0, px1, py1),
            covers,
            inset,
        };
        self.lay_in_pixels(picture, drawn, raster, landing, placed);
        true
    }

    /// What goes with a picture at the pixels of the terminal, whether it is one placement or the
    /// pieces of one: the cells it covers altogether are drawn in the grid's pixels too, for what is
    /// laid over them; at its edge, where it covers a part of a cell, the cell is the terminal's and
    /// the rest of it is what was behind the picture's box.
    fn lay_in_pixels(
        &mut self,
        picture: &Picture,
        drawn: Rect,
        raster: &Arc<Raster>,
        landing: Landing,
        placed: Vec<Placement>,
    ) {
        let (cell_w, cell_h) = self.cell_pixels();
        let Landing {
            pixels,
            covers,
            inset,
        } = landing;
        let (px0, py0, px1, py1) = pixels;
        let (col, row, cols, rows, _) = self.in_cells(pixels);
        if covers {
            let touched = (
                i32::from(col),
                i32::from(row),
                i32::from(col + cols),
                i32::from(row + rows),
            );
            if inset {
                self.bare_the_margins(picture, touched);
            }
            let (whole_c0, whole_c1) = ((px0.div_ceil(cell_w)) as i32, (px1 / cell_w) as i32);
            let (whole_r0, whole_r1) = ((py0.div_ceil(cell_h)) as i32, (py1 / cell_h) as i32);
            if whole_c1 > whole_c0 && whole_r1 > whole_r0 {
                let whole = (drawn.x, drawn.y, drawn.x + drawn.w, drawn.y + drawn.h);
                let part = (
                    whole_c0 as f32 * self.cw,
                    whole_r0 as f32 * self.ch(),
                    whole_c1 as f32 * self.cw,
                    whole_r1 as f32 * self.ch(),
                );
                let region = Span {
                    x0: whole_c0,
                    y0: whole_r0 * 2,
                    x1: whole_c1,
                    y1: whole_r1 * 2,
                };
                self.mosaic(region, raster, shares(whole, part, raster.crop), 1.0);
            }
            self.shown.push((picture.rect, drawn));
            for row in touched.1..touched.3 {
                for col in touched.0..touched.2 {
                    self.canvas.open(col, row);
                }
            }
            self.under_the_edge(
                picture,
                touched,
                (whole_c0, whole_r0, whole_c1, whole_r1),
                (px0, py0, px1, py1),
            );
        }
        for placement in placed {
            self.placed_at.push(self.canvas.tick);
            self.placements.push(placement);
        }
    }

    /// Keeps the ground that is in `visible` (pixels of the grid) before the box `rect` is filled.
    fn keep_behind(&mut self, rect: Rect, visible: Span) {
        let span = visible.meet(self.canvas.bounds());
        if span.is_empty() {
            self.behind = None;
            return;
        }
        let mut ground = Vec::with_capacity(((span.x1 - span.x0) * (span.y1 - span.y0)) as usize);
        for y in span.y0..span.y1 {
            for x in span.x0..span.x1 {
                ground.push(self.canvas.ground_at(x, y));
            }
        }
        self.behind = Some(Behind {
            rect,
            tick: self.canvas.tick,
            span,
            ground,
        });
    }

    /// The ground at the pixel `x`, `y` of the grid that was behind the box `picture` is in: what was
    /// there before the box was filled where the box's fill is the item just before the picture, and
    /// else what is there.
    fn behind_ground(&self, picture: &Picture, x: i32, y: i32) -> Rgb {
        if let Some(behind) = self.kept_behind(picture) {
            let span = behind.span;
            if x >= span.x0 && x < span.x1 && y >= span.y0 && y < span.y1 {
                let at = ((y - span.y0) * (span.x1 - span.x0) + (x - span.x0)) as usize;
                if let Some(ground) = behind.ground.get(at) {
                    return *ground;
                }
            }
        }
        self.canvas.ground_at(x, y)
    }

    /// The ground kept before the box `picture` is in was filled, where its fill is the item just
    /// before the picture.
    fn kept_behind(&self, picture: &Picture) -> Option<&Behind> {
        let behind = self.behind.as_ref()?;
        let same = |a: f32, b: f32| (a - b).abs() < 1e-3;
        let (cells, rect) = (picture.cells, behind.rect);
        (behind.tick + 1 == self.canvas.tick
            && same(cells.x, rect.x)
            && same(cells.y, rect.y)
            && same(cells.w, rect.w)
            && same(cells.h, rect.h))
        .then_some(behind)
    }

    /// The box of a picture that is smaller than it is its fill only where the picture is: the cells
    /// the picture does not touch (`touched`) have the ground that was behind the box again, as the
    /// page laid the fill for a picture of the box's proportions and it is not that picture.
    fn bare_the_margins(&mut self, picture: &Picture, touched: (i32, i32, i32, i32)) {
        let Some(behind) = self.kept_behind(picture) else {
            return;
        };
        let span = behind.span;
        let width = (span.x1 - span.x0) as usize;
        let mut bare = Vec::new();
        for y in span.y0..span.y1 {
            let row = y.div_euclid(2);
            for x in span.x0..span.x1 {
                let inside = x >= touched.0 && x < touched.2 && row >= touched.1 && row < touched.3;
                let at = (y - span.y0) as usize * width + (x - span.x0) as usize;
                if let (false, Some(ground)) = (inside, behind.ground.get(at)) {
                    bare.push((x, y, *ground));
                }
            }
        }
        for (x, y, ground) in bare {
            self.canvas.paint(x, y, ground, 1.0);
        }
    }

    /// The cells of `touched` that the picture, at the pixels `(x0, y0, x1, y1)` of the terminal,
    /// covers a part of and not all (those that are not in `whole`): each is laid on the ground that was
    /// behind the picture's box, under the picture, in a picture of that colour stretched over runs of
    /// the cells that have it. (The cells the terminal has no ground for would be its own.)
    fn under_the_edge(
        &mut self,
        picture: &Picture,
        touched: (i32, i32, i32, i32),
        whole: (i32, i32, i32, i32),
        (x0, y0, x1, y1): (u32, u32, u32, u32),
    ) {
        let (cell_w, cell_h) = self.cell_pixels();
        let (cell_w, cell_h) = (cell_w as i32, cell_h as i32);
        let covered_in = |from: i32, to: i32, cell: i32, size: i32| -> (i32, i32) {
            (
                (from - cell * size).clamp(0, size),
                (to - cell * size).clamp(0, size),
            )
        };
        let mut rows: Vec<Vec<(i32, Option<Rgb>)>> = Vec::new();
        for row in touched.1..touched.3 {
            let mut cells = Vec::new();
            for col in touched.0..touched.2 {
                let inside = col >= whole.0 && col < whole.2 && row >= whole.1 && row < whole.3;
                if inside {
                    cells.push((col, None));
                    continue;
                }
                // The half of the cell that is more of what is left of it is the one whose ground it is.
                let (top, bottom) = covered_in(y0 as i32, y1 as i32, row, cell_h);
                let (left, right) = covered_in(x0 as i32, x1 as i32, col, cell_w);
                let half = if top > 0 || bottom < cell_h {
                    i32::from(cell_h - bottom > top)
                } else {
                    let _ = (left, right);
                    0
                };
                cells.push((col, Some(self.behind_ground(picture, col, row * 2 + half))));
            }
            rows.push(cells);
        }
        // Runs of one colour along a row, and the rows that have the same run one under the other.
        let mut boxes: Vec<(i32, i32, i32, i32, Rgb)> = Vec::new();
        for (n, cells) in rows.iter().enumerate() {
            let row = touched.1 + n as i32;
            for group in cells.chunk_by(|a, b| a.1 == b.1) {
                let (Some((first, Some(color))), Some((last, _))) = (group.first(), group.last())
                else {
                    continue;
                };
                let (c0, c1, color) = (*first, *last + 1, *color);
                match boxes.iter_mut().find(|(_, r1, b0, b1, rgb)| {
                    *r1 == row && *b0 == c0 && *b1 == c1 && *rgb == color
                }) {
                    Some((_, r1, _, _, _)) => *r1 = row + 1,
                    None => boxes.push((row, row + 1, c0, c1, color)),
                }
            }
        }
        for (r0, r1, c0, c1, color) in boxes {
            self.placed_at.push(self.canvas.tick);
            self.placements.push(Placement {
                col: c0 as u16,
                row: r0 as u16,
                cols: (c1 - c0) as u16,
                rows: (r1 - r0) as u16,
                offset: None,
                pixels: Pixels::solid([color.r, color.g, color.b, 255]),
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                z: GROUND - 1,
            });
        }
    }

    /// The rectangle a picture is drawn on, and what of the picture it shows: the box the page put it
    /// in, at the pixels the layout has it at, where the terminal's cell has the proportions of the
    /// page's to a hundredth ([`PROPORTION_SLACK`]); else with its proportions kept on the terminal's
    /// own pixels. The page's pixels are not square there (a cell of the grid is as high as two
    /// columns and the terminal's is not), so what the page drew to its proportions would be stretched
    /// by the difference; the whole of the box is filled (`cover`, the picture cut at its sides) or
    /// the picture is made smaller than it (`contain`), in the middle either way. Of the box the page
    /// wrote and not of the cells it is in: the cells are what the grid made of the box, and what the
    /// layout gave a picture is where it is, so that two pictures of one size are one size however
    /// their boxes fall on the cells. An icon (a tinted picture) fills its cells as a character does
    /// and is left as the page's grid put it, unless the page wrote it a place (`written`: it is the
    /// picture of that place, which is fitted as any is).
    fn fitted(&self, picture: &Picture, written: bool) -> (Rect, Rect) {
        let (rect, crop) = (picture.rect, picture.crop);
        let (across, down) = (
            self.options.pixels.0.max(1) as f32 / self.cw,
            self.options.pixels.1.max(1) as f32 / self.ch(),
        );
        let sound = |length: f32| length.is_finite() && length > 0.0;
        if picture.turn != 0.0
            || (picture.tint.is_some() && !written)
            || !(sound(rect.w) && sound(rect.h) && sound(crop.w) && sound(crop.h))
            || !(sound(across) && sound(down))
            || (across / down - 1.0).abs() <= PROPORTION_SLACK
        {
            return (rect, crop);
        }
        // The proportions of the whole picture, which the page laid out in its own square pixels, and
        // the box it fills on the terminal's pixels.
        let natural = (rect.w / crop.w) / (rect.h / crop.h);
        let (box_w, box_h) = (rect.w * across, rect.h * down);
        let centre = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
        let whole = crop.w >= 0.999 && crop.h >= 0.999;
        if whole {
            let (w, h) = if box_w / box_h > natural {
                (box_h * natural, box_h)
            } else {
                (box_w, box_w / natural)
            };
            let (w, h) = (w / across, h / down);
            (
                Rect::new(centre.0 - w / 2.0, centre.1 - h / 2.0, w, h),
                crop,
            )
        } else {
            let image_w = box_w.max(box_h * natural);
            let image_h = image_w / natural;
            let (shown_w, shown_h) = (box_w / image_w, box_h / image_h);
            (
                rect,
                Rect::new(
                    (1.0 - shown_w) / 2.0,
                    (1.0 - shown_h) / 2.0,
                    shown_w,
                    shown_h,
                ),
            )
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
        self.placed_at.push(self.canvas.tick);
        self.placements.push(Placement {
            col: c0 as u16,
            row: r0 as u16,
            cols: (c1 - c0) as u16,
            rows: (r1 - r0) as u16,
            offset: None,
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

    /// The lines that lie along words, drawn: only the cells that no box laid over them later hides are
    /// the line's. Where none of those has words in it, the line is the characters of them, as a line
    /// along no words is; where one has, it is a picture over the words for each run of them where the
    /// terminal has pictures, and nothing where it has none.
    fn resolve_lines(&mut self) {
        for line in std::mem::take(&mut self.lines) {
            let free: Vec<i32> = line
                .cols
                .iter()
                .copied()
                .filter(|col| !self.canvas.hidden_after((*col, line.row, 1, 1), line.at))
                .collect();
            let cut = free.iter().any(|col| self.has_text(*col, line.row));
            if cut && self.options.images != ImageMode::Kitty {
                continue;
            }
            let mut runs: Vec<(i32, i32)> = Vec::new();
            for col in free {
                match runs.last_mut() {
                    Some((_, end)) if *end == col => *end = col + 1,
                    _ => runs.push((col, col + 1)),
                }
            }
            for (from, to) in runs {
                let x0 = (from as f32 * self.cw).max(line.rect.x);
                let x1 = (to as f32 * self.cw).min(line.rect.x + line.rect.w);
                let drawn = cut
                    && x1 > x0
                    && self.sliver(
                        Rect::new(x0, line.rect.y, x1 - x0, line.rect.h),
                        Rect::new(x0, line.rect.y, x1 - x0, line.rect.h),
                        Some(line.color),
                        None,
                        OVER_TEXT,
                    );
                if !drawn {
                    for col in from..to {
                        self.mark(col, line.row, line.edge, line.color);
                    }
                }
            }
        }
    }

    /// The dots of the page put in the cells that are spaces and that nothing opaque was laid over
    /// after them.
    fn resolve_bullets(&mut self) {
        for bullet in std::mem::take(&mut self.bullets) {
            let Bullet {
                col,
                row,
                color: (rgb, alpha),
                at,
            } = bullet;
            if self.canvas.hidden_after((col, row, 1, 1), at) {
                continue;
            }
            let Some(cell) = self.canvas.cell(col, row) else {
                continue;
            };
            let free = match self.canvas.slots.get(cell) {
                Some(Slot::Empty | Slot::Line { .. }) => true,
                Some(Slot::Glyph { ch, wide, .. }) => *ch == ' ' && !*wide,
                _ => false,
            };
            if free {
                let ink = self.canvas.under(col, row).mix(rgb, alpha);
                self.canvas
                    .put(col, row, ('\u{b7}', ink, Attrs::NONE), false);
            }
        }
    }

    /// The planes put together.
    fn finish(mut self) -> Frame {
        self.resolve_bullets();
        self.resolve_lines();
        let Self {
            theme,
            canvas,
            placements,
            placed_at,
            waiting,
            overlaps,
            ..
        } = self;
        // A picture the terminal draws over the grounds (an icon, the tint over a picture) is not
        // hidden by a ground drawn after it, as one under the grounds is: a box laid over it (a
        // palette, a dialog) leaves it out; a colour stretched over cells (the scrim of a dialog, a
        // tint) stays over the cells that no box was laid over, and is not left out for the cells of
        // the box that is in the way of its rectangle.
        let placements: Vec<Placement> = placements
            .into_iter()
            .zip(placed_at)
            .flat_map(|(placed, at)| {
                let cells = (
                    i32::from(placed.col),
                    i32::from(placed.row),
                    i32::from(placed.cols),
                    i32::from(placed.rows),
                );
                if placed.z != BEHIND_TEXT || !canvas.hidden_after(cells, at) {
                    return vec![placed];
                }
                if placed.offset.is_some() || !placed.pixels.is_solid() {
                    return Vec::new();
                }
                canvas
                    .open_runs(cells, at)
                    .into_iter()
                    .map(|(c0, r0, c1, r1)| Placement {
                        col: c0 as u16,
                        row: r0 as u16,
                        cols: (c1 - c0) as u16,
                        rows: (r1 - r0) as u16,
                        ..placed.clone()
                    })
                    .collect()
            })
            .collect();
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
