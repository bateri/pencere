//! An editor's document in its room: its lines measured by the host ([`Measure`]), broken where the
//! room ends when the editor wraps, the folded ones left out, laid out in rows; what is under a
//! point; and what the room draws.
//!
//! Only the rows in view (and a few about them) are laid out. The others count as many rows as their
//! length says they take, until they are laid out; a line of the room's width takes one. Places
//! across are pixels: of the content (from the room's left edge, as nothing is scrolled) or of a
//! line's text (from where it starts).

use std::cell::RefCell;
use std::collections::HashMap;
use std::iter::Peekable;
use std::rc::Rc;
use std::slice;

use pencere_view::edit::{
    CaretHeight, CaretShape, Column, ColumnEvents, Decoration, EditorOptions, GutterMark,
    GutterShape, LineHighlight, Look, Mark, Matches, Numbers, Preview, Range, RulerLane, Underline,
    Whitespace, limits,
};
use pencere_view::{
    Border, Color, Cursor, EditorRoom, EditorSlot, Figure, Font, Id, Measure, Mods, Paint, Picture,
    Point, Quad, Rect, Role, Run, Size, path,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::buffer::{Buffer, Placement};
use crate::guides::{Levels, active_guide};
use crate::segments::grapheme_after;

/// The most lines whose measure is kept.
const SHAPED: usize = 4096;
/// How many rows above and below those in view are laid out.
const AROUND: usize = 4;

/// What the other places of the main selection's text were found for: the version of the
/// document, the main selection, the text in view and the editor's `matches`.
type MatchesOf = (u64, Range, usize, usize, Matches);

/// The other places found, and what for.
type FoundMatches = (MatchesOf, Rc<Vec<(usize, usize)>>);

/// A line measured: where each of its graphemes starts across, and where it ends.
#[derive(Debug, PartialEq)]
pub(crate) struct Shaped {
    /// Characters into the line, and pixels across, of each boundary: the first `(0, 0)`, the last
    /// the end of the line.
    pub stops: Vec<(usize, f32)>,
    /// The boundaries (characters into the line) that come after a space or a tab: where a row may
    /// break.
    pub breaks: Vec<usize>,
}

impl Shaped {
    fn width(&self) -> f32 {
        self.stops.last().map_or(0.0, |stop| stop.1)
    }

    /// Where across the boundary at `chars` into the line is (the nearest one before it, inside a
    /// grapheme).
    fn x_at(&self, chars: usize) -> f32 {
        let at = self.stops.partition_point(|stop| stop.0 <= chars);
        at.checked_sub(1)
            .and_then(|at| self.stops.get(at))
            .map_or(0.0, |stop| stop.1)
    }

    /// The boundary nearest `x` across between the characters `from` and `to` of the line.
    fn nearest(&self, x: f32, from: usize, to: usize) -> usize {
        let mut best = (from, f32::INFINITY);
        for (chars, at) in &self.stops {
            if *chars < from || *chars > to {
                continue;
            }
            let distance = (at - x).abs();
            if distance < best.1 {
                best = (*chars, distance);
            }
        }
        best.0
    }
}

/// One row: a line, or the part of a wrapping line, laid out.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub line: usize,
    /// Where the line starts in the document.
    pub base: usize,
    /// Characters into the line where the row starts and ends.
    pub from: usize,
    pub to: usize,
    /// The row's place among all rows, from the top.
    pub index: usize,
    pub shaped: Rc<Shaped>,
    /// Where across the line the row starts: what is taken from its stops.
    pub shift: f32,
    /// The last row of its line.
    pub last: bool,
}

impl Row {
    /// Where across the row's text the place `chars` into the line is.
    fn x(&self, chars: usize) -> f32 {
        self.shaped.x_at(chars) - self.shift
    }
}

/// The word under the pointer ([`View::word_under`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Word {
    /// The character the pointer is over.
    pub at: usize,
    pub from: usize,
    pub to: usize,
    /// Its box, pixels of the content.
    pub rect: Rect,
}

/// What the rows of the lines were counted for: the version, the lines hidden, the width of the text.
type Counted = (u64, Vec<(usize, usize)>, u32, Vec<(usize, u32, Id)>);

/// An editor's layout.
#[derive(Debug)]
pub(crate) struct View {
    pub font: Font,
    pub color: Color,
    pub scale: f32,
    /// The height of a row and of the text in it, the room a row keeps above its text and as much
    /// below, the width of a space and of a digit, pixels.
    pub row: f32,
    pub line: f32,
    pub lead: f32,
    pub space: f32,
    pub digit: f32,
    /// The host draws no text: a grapheme is a space wide.
    plain: bool,
    /// The room on the page, pixels, and how far it is scrolled.
    pub room: Rect,
    pub scroll: Point,
    pub wrap: bool,
    pub tab: usize,
    pub gutter_shown: bool,
    /// How wide the gutter is, and the room kept around the text, pixels.
    pub gutter: f32,
    pub pad: f32,
    /// What the page said of the room, held to what an editor takes: its rows, caret, gutter and
    /// colours.
    pub options: EditorOptions,
    /// The modifiers held, and the character the pointer is over: what a look that is `held` or
    /// `over` is drawn by.
    pub mods: Mods,
    pub pointer_char: Option<usize>,
    /// The columns of the gutter, laid out.
    pub cols: Vec<Col>,
    /// The cell of the gutter the pointer is over, and the one it is pressing.
    pub hover: Option<Cell>,
    pub pressed: Option<Cell>,
    shaped: RefCell<HashMap<String, Rc<Shaped>>>,
    /// The other places of the main selection's text last found, and for what: the version, the
    /// main selection and the text in view. A frame that changes none of them (the caret's blink,
    /// a spinner) does not look again.
    matches: RefCell<Option<FoundMatches>>,
    /// The row each line starts at, and after the last the number of rows; hidden lines take none.
    tops: Vec<usize>,
    /// What `tops` was counted for.
    counted: Option<Counted>,
    /// The gaps kept open for the page, in order of their lines: the line each is after, how tall
    /// it is (pixels, as the page said) and its name.
    zones: Vec<(usize, f32, Id)>,
    /// How many rows of gaps are after each line that has any (a line folded away has none).
    zone_rows: HashMap<usize, usize>,
    /// How much higher than `index * row` the rows of each line are drawn (pixels, from the top of
    /// the content, lines counted as `tops`): the gaps take whole rows, and what a row takes more
    /// than the gap is tall is taken back after it. Empty where no gap has such a remainder.
    shrinks: Vec<f32>,
    /// The rows laid out: those in view and some about them.
    pub rows: Vec<Row>,
    /// How many characters the longest line has, and of which version.
    widest: Option<(u64, usize)>,
    /// The widest row laid out, pixels.
    measured: f32,
}

impl Default for View {
    fn default() -> Self {
        Self {
            font: Font {
                family: pencere_view::Family::Terminal,
                size: 12.5,
                weight: 400,
                italic: false,
            },
            color: Role::Foreground.solid(),
            scale: 1.0,
            row: 0.0,
            line: 0.0,
            lead: 0.0,
            space: 7.0,
            digit: 7.0,
            plain: false,
            room: Rect::default(),
            scroll: Point::default(),
            wrap: false,
            tab: 4,
            gutter_shown: true,
            gutter: 0.0,
            pad: 4.0,
            options: EditorOptions::default(),
            mods: Mods::default(),
            pointer_char: None,
            cols: Vec::new(),
            hover: None,
            pressed: None,
            shaped: RefCell::new(HashMap::new()),
            matches: RefCell::new(None),
            tops: vec![0, 1],
            counted: None,
            zones: Vec::new(),
            zone_rows: HashMap::new(),
            shrinks: Vec::new(),
            rows: Vec::new(),
            widest: None,
            measured: 0.0,
        }
    }
}

impl View {
    /// The room is `room` now, its text as the page's boxes set it, `scale` pixels to a point.
    pub fn set_room(&mut self, room: &EditorRoom, scale: f32, measure: &mut dyn Measure) {
        let options = room.options.clone().held();
        let look_changed = room.font != self.font
            || scale != self.scale
            || options.line_height != self.options.line_height
            || options.padding != self.options.padding;
        let tab = usize::from(room.options.tab_width.max(1));
        if look_changed || tab != self.tab {
            self.shaped.borrow_mut().clear();
            self.counted = None;
            self.rows.clear();
        }
        if look_changed || self.row <= 0.0 {
            self.font = room.font;
            self.scale = scale;
            let line = measure.measure(&room.font, "Mg");
            let space = measure.measure(&room.font, " ").width;
            let digit = measure.measure(&room.font, "0").width;
            self.plain = space <= 0.0 || digit <= 0.0;
            self.line = if line.height > 0.0 {
                line.height.ceil()
            } else {
                (room.font.size * 1.25).ceil()
            };
            // A row is as tall as the page says, times the font's size (1.6, the golden ratio
            // editors of code read comfortably at, where it says nothing), its text in the middle,
            // with a point at least above and below it where the font's own line is that tall
            // already. The room is whole points, so a row is as tall on every screen.
            let s = scale.max(0.01);
            let points = |pixels: f32| (pixels / s).round() * s;
            let target = points(room.font.size * options.line_height);
            self.lead = points((target - self.line) / 2.0).max(s);
            self.row = self.line + 2.0 * self.lead;
            self.space = if self.plain {
                room.font.size * 0.6
            } else {
                space
            };
            self.digit = if self.plain {
                room.font.size * 0.6
            } else {
                digit
            };
            self.pad = (options.padding * scale).round();
        }
        if room.options.wrap != self.wrap
            || room.options.gutter != self.gutter_shown
            || options.numbers != self.options.numbers
            || options.gutters != self.options.gutters
        {
            self.counted = None;
        }
        if (room.pixels.w - self.room.w).abs() > 0.5 && room.options.wrap {
            self.counted = None;
        }
        self.color = room.color;
        self.tab = tab;
        self.wrap = room.options.wrap;
        self.gutter_shown = room.options.gutter;
        self.room = room.pixels;
        self.options = options;
    }

    /// Where the text starts across the content, pixels: past the gutter.
    pub fn text_x(&self) -> f32 {
        if self.gutter_shown {
            self.gutter + self.pad
        } else {
            self.pad
        }
    }

    /// How wide the text of a row may be before it wraps, pixels.
    fn text_width(&self) -> f32 {
        (self.room.w - self.text_x() - self.pad).max(self.space * 4.0)
    }

    /// How large the content is.
    pub fn content(&self) -> Size {
        let rows = self.tops.last().copied().unwrap_or(1).max(1);
        let height = self.y_of(rows) + 2.0 * self.pad;
        let width = if self.wrap {
            self.room.w
        } else {
            let widest =
                self.widest.map_or(0, |(_, chars)| chars) as f32 * self.digit.max(self.space);
            self.text_x() + widest.max(self.measured) + self.pad + self.space
        };
        Size::new(width, height)
    }

    // --- Lines ------------------------------------------------------------------------------

    /// Line `text`, measured (kept for the next time it is asked for).
    pub fn shape(&self, text: &str, measure: &mut dyn Measure) -> Rc<Shaped> {
        if let Some(found) = self.shaped.borrow().get(text) {
            return found.clone();
        }
        let tab = self.space * self.tab as f32;
        let mut stops = vec![(0, 0.0)];
        let mut breaks = Vec::new();
        let (mut x, mut chars) = (0.0_f32, 0);
        for piece in text.split_inclusive('\t') {
            let (words, tabbed) = match piece.strip_suffix('\t') {
                Some(words) => (words, true),
                None => (piece, false),
            };
            if !words.is_empty() {
                let mut ends = Vec::new();
                let mut counts = Vec::new();
                let (mut byte, mut count) = (0, 0);
                for grapheme in words.graphemes(true) {
                    byte += grapheme.len();
                    count += grapheme.chars().count();
                    ends.push(byte);
                    counts.push(count);
                    if grapheme.chars().all(char::is_whitespace) {
                        breaks.push(chars + count);
                    }
                }
                let offsets = if self.plain {
                    (1..=ends.len()).map(|n| n as f32 * self.space).collect()
                } else {
                    measure.offsets(&self.font, words, &ends)
                };
                let mut width = 0.0;
                for (count, offset) in counts.iter().zip(offsets) {
                    stops.push((chars + count, x + offset));
                    width = offset;
                }
                x += width;
                chars += words.chars().count();
            }
            if tabbed {
                x = ((x / tab).floor() + 1.0) * tab;
                chars += 1;
                stops.push((chars, x));
                breaks.push(chars);
            }
        }
        let shaped = Rc::new(Shaped { stops, breaks });
        let mut kept = self.shaped.borrow_mut();
        if kept.len() >= SHAPED {
            kept.clear();
        }
        kept.insert(text.to_owned(), shaped.clone());
        shaped
    }

    /// Where a line of `shaped` breaks into rows: the characters each starts and ends at. A line
    /// that does not wrap is one row; one that does breaks after the last space that fits, or where
    /// it must (a grapheme at least on each row).
    fn breaks(&self, shaped: &Shaped) -> Vec<(usize, usize)> {
        let end = shaped.stops.last().map_or(0, |stop| stop.0);
        if !self.wrap || shaped.width() <= self.text_width() {
            return vec![(0, end)];
        }
        let width = self.text_width();
        let mut rows = Vec::new();
        let (mut start, mut start_x) = (0, 0.0);
        let mut previous = 0;
        for (chars, x) in shaped.stops.iter().skip(1) {
            if x - start_x > width && previous > start {
                let space = shaped
                    .breaks
                    .iter()
                    .rev()
                    .find(|at| **at > start && **at <= previous)
                    .copied();
                let cut = space.unwrap_or(previous);
                rows.push((start, cut));
                start = cut;
                start_x = shaped.x_at(cut);
            }
            previous = *chars;
        }
        rows.push((start, end));
        rows
    }

    /// How many rows line `line` takes, as far as it is known: laid out, or guessed from its length.
    fn rows_of(&self, buffer: &Buffer, line: usize) -> usize {
        if !self.wrap {
            return 1;
        }
        let text = buffer.line(line);
        if let Some(shaped) = self.shaped.borrow().get(&text) {
            return self.breaks(shaped).len().max(1);
        }
        let guess = text.chars().count() as f32 * self.digit.max(self.space);
        ((guess / self.text_width()).ceil() as usize).max(1)
    }

    /// The rows of the lines counted again, where what they depend on changed.
    pub fn count(&mut self, buffer: &Buffer) {
        let hidden = merged(buffer.hidden_lines());
        // The gaps the page keeps open: whole rows, after a line that shows.
        let zones: Vec<(usize, f32, Id)> = if self.row > 0.0 {
            buffer
                .zones()
                .into_iter()
                .map(|(line, zone)| {
                    let most = limits::ZONE_ROWS as f32 * self.row;
                    (line, (zone.height.max(0.0) * self.scale).min(most), zone.id)
                })
                .collect()
        } else {
            Vec::new()
        };
        let key = (
            buffer.version(),
            hidden.clone(),
            self.text_width().round() as u32,
            zones
                .iter()
                .map(|(line, height, id)| (*line, height.to_bits(), *id))
                .collect::<Vec<_>>(),
        );
        if self.counted.as_ref() == Some(&key) {
            return;
        }
        let lines = buffer.len_lines();
        let mut tops = Vec::with_capacity(lines + 1);
        let mut row = 0;
        // A line's gaps are as tall as the page said, one after the other; they take the rows that
        // hold them, and what is left of the last is taken back below.
        let mut heights: HashMap<usize, f32> = HashMap::new();
        for (line, height, _) in &zones {
            *heights.entry(*line).or_default() += height;
        }
        let mut shown_gaps = HashMap::new();
        let mut shrinks = Vec::with_capacity(lines + 1);
        let mut shrink = 0.0_f32;
        let mut folds = hidden.iter().peekable();
        for line in 0..lines {
            tops.push(row);
            shrinks.push(shrink);
            while folds.peek().is_some_and(|(_, last)| *last < line) {
                folds.next();
            }
            let hidden = folds
                .peek()
                .is_some_and(|(first, last)| line >= *first && line <= *last);
            if !hidden {
                row += self.rows_of(buffer, line);
                if let Some(height) = heights.get(&line) {
                    let rows = (height / self.row).ceil() as usize;
                    row += rows;
                    shrink += rows as f32 * self.row - height;
                    shown_gaps.insert(line, rows);
                }
            }
        }
        tops.push(row);
        shrinks.push(shrink);
        self.shrinks = if shrink > 0.0 { shrinks } else { Vec::new() };
        self.tops = tops;
        self.zones = zones;
        self.zone_rows = shown_gaps;
        self.counted = Some(key);
        if self
            .widest
            .is_none_or(|(version, _)| version != buffer.version())
        {
            let widest = (0..lines)
                .map(|line| buffer.line_end(line) - buffer.line_start(line))
                .max()
                .unwrap_or(0);
            self.widest = Some((buffer.version(), widest));
        }
    }

    /// How many rows the gaps kept open after line `line` take.
    fn zone_rows(&self, line: usize) -> usize {
        self.zone_rows.get(&line).copied().unwrap_or(0)
    }

    /// The gaps kept open for the page, each by its name and its box in the content (pixels): a gap
    /// is after its line's own rows, wide as the room, one after another where a line has several;
    /// one that is after a line folded away is nowhere.
    pub fn zone_boxes(&self) -> Vec<(Id, Rect)> {
        let mut out = Vec::new();
        let mut last: Option<(usize, f32)> = None;
        for (line, height, id) in &self.zones {
            let (Some(top), Some(next)) = (self.tops.get(*line), self.tops.get(*line + 1)) else {
                continue;
            };
            if next <= top {
                continue;
            }
            let used = match last {
                Some((at, used)) if at == *line => used,
                _ => 0.0,
            };
            out.push((
                *id,
                Rect::new(0.0, self.line_bottom(*line) + used, self.room.w, *height),
            ));
            last = Some((*line, used + height));
        }
        out
    }

    /// How many rows there are.
    fn total(&self) -> usize {
        self.tops.last().copied().unwrap_or(1).max(1)
    }

    /// The line row `row` is of.
    fn line_at_row(&self, row: usize) -> usize {
        let row = row.min(self.total().saturating_sub(1));
        let lines = self.tops.len().saturating_sub(1);
        let after = self
            .tops
            .get(..lines)
            .map_or(0, |tops| tops.partition_point(|top| *top <= row));
        after.saturating_sub(1)
    }

    /// The rows of line `line`, laid out.
    fn line_rows(&self, buffer: &Buffer, line: usize, measure: &mut dyn Measure) -> Vec<Row> {
        let shaped = self.shape(&buffer.line(line), measure);
        let breaks = self.breaks(&shaped);
        let top = self.tops.get(line).copied().unwrap_or(0);
        let count = breaks.len();
        breaks
            .into_iter()
            .enumerate()
            .map(|(at, (from, to))| Row {
                line,
                base: buffer.line_start(line),
                from,
                to,
                index: top + at,
                shift: shaped.x_at(from),
                shaped: shaped.clone(),
                last: at + 1 == count,
            })
            .collect()
    }

    /// The rows in view, scrolled `scroll` (pixels), laid out; lines found to take more or fewer
    /// rows than was guessed are counted again.
    pub fn lay_out(&mut self, buffer: &Buffer, scroll: Point, measure: &mut dyn Measure) {
        self.scroll = scroll;
        for _ in 0..2 {
            self.count(buffer);
            let first = self.row_at(scroll.y - self.pad);
            // The rows in the room may be more than it is tall in rows: the gaps are taken back what
            // they are not tall.
            let last = self.row_at(scroll.y - self.pad + self.room.h) + AROUND + 1;
            let first = first.saturating_sub(AROUND);
            let mut rows = Vec::new();
            let mut line = self.line_at_row(first);
            let mut guessed_wrong = false;
            let mut measured: f32 = 0.0;
            while line < buffer.len_lines() && rows.len() < last - first + AROUND {
                let top = self.tops.get(line).copied().unwrap_or(0);
                let next = self.tops.get(line + 1).copied().unwrap_or(top);
                if next == top {
                    // Folded away.
                    line += 1;
                    continue;
                }
                if top > last {
                    break;
                }
                let laid = self.line_rows(buffer, line, measure);
                if laid.len() != next - top - self.zone_rows(line) {
                    guessed_wrong = true;
                }
                for row in &laid {
                    measured = measured.max(row.shaped.width());
                }
                rows.extend(laid);
                line += 1;
            }
            self.measured = self.measured.max(measured);
            self.rows = rows;
            if !guessed_wrong {
                break;
            }
            self.counted = None;
        }
    }

    // --- Places -----------------------------------------------------------------------------

    /// The row the place `at` is on, laid out, and the characters into its line; `None` for a
    /// place folded away.
    fn row_of(
        &self,
        buffer: &Buffer,
        at: usize,
        measure: &mut dyn Measure,
    ) -> Option<(Row, usize)> {
        let line = buffer.line_of(at);
        let into = at - buffer.line_start(line).min(at);
        let top = self.tops.get(line).copied()?;
        let next = self.tops.get(line + 1).copied().unwrap_or(top + 1);
        if next == top {
            return None;
        }
        let found = self
            .rows
            .iter()
            .filter(|row| row.line == line)
            .find(|row| into >= row.from && (into < row.to || row.last))
            .cloned();
        if let Some(row) = found {
            return Some((row, into));
        }
        let rows = self.line_rows(buffer, line, measure);
        let count = rows.len();
        rows.into_iter()
            .enumerate()
            .find(|(index, row)| into >= row.from && (into < row.to || index + 1 == count))
            .map(|(_, row)| (row, into))
    }

    /// Where the place `at` is, pixels of the content: the top-left of its caret.
    pub fn point_of(&self, buffer: &Buffer, at: usize, measure: &mut dyn Measure) -> Option<Point> {
        let (row, into) = self.row_of(buffer, at, measure)?;
        Some(Point::new(
            self.text_x() + row.x(into),
            self.pad + self.y_of(row.index),
        ))
    }

    /// The caret's box at `at`, pixels of the content: as wide as the caret and as tall as the text,
    /// whatever its shape, so that what stands by it (an overlay, an input method's window) stands
    /// clear of the text it is on.
    pub fn caret_of(&self, buffer: &Buffer, at: usize, measure: &mut dyn Measure) -> Option<Rect> {
        let (row, into) = self.row_of(buffer, at, measure)?;
        let x = self.text_x() + row.x(into);
        let top = self.pad + self.y_of(row.index);
        let drawn = self.caret_box(buffer, &row, at, x, top);
        let (y, h) = self.caret_span(top);
        Some(Rect::new(drawn.x, y, drawn.w, h))
    }

    /// The character the pointer at `point` (pixels of the content) is over: the one before the place
    /// it is nearest where it is left of it, the one after where it is not. `None` over the gutter,
    /// past the end of a line and in a gap kept open for the page.
    pub fn char_under(
        &self,
        buffer: &Buffer,
        point: Point,
        measure: &mut dyn Measure,
    ) -> Option<usize> {
        if self.row <= 0.0 || point.x < self.text_x() || point.y < self.pad {
            return None;
        }
        let row = self.row_at(point.y - self.pad);
        if row >= self.total() {
            return None;
        }
        let line = self.line_at_row(row);
        if row >= self.own_end(line) {
            return None;
        }
        let place = self.place_at(buffer, point, measure);
        let (found, into) = self.row_of(buffer, place, measure)?;
        let here = self.text_x() + found.x(into);
        let (start, end) = (buffer.line_start(line), buffer.line_end(line));
        if point.x < here {
            place.checked_sub(1).filter(|at| *at >= start)
        } else {
            (place < end).then_some(place)
        }
    }

    /// The word (or the run of marks the double click takes) the pointer at `point` (pixels of the
    /// content) is over: the place it is on, the range of the run and its box (pixels of the
    /// content). `None` where there is no character under it ([`View::char_under`]) and over a run
    /// of spaces.
    pub fn word_under(
        &self,
        buffer: &Buffer,
        point: Point,
        measure: &mut dyn Measure,
    ) -> Option<Word> {
        let char_at = self.char_under(buffer, point, measure)?;
        let (from, to) = buffer.word_range(char_at);
        if to <= from || buffer.slice(from, to)?.chars().all(char::is_whitespace) {
            return None;
        }
        let boxed = self.range_box(buffer, from, to, measure)?;
        Some(Word {
            at: char_at,
            from,
            to,
            rect: boxed,
        })
    }

    /// The box of the range `from..to`, pixels of the content: from where it starts across, as wide
    /// as its first row's part of it is, from the top of its first row to the bottom of its last. A
    /// range that is a place is as wide as a caret; `None` where its start is not on a line that
    /// shows (one folded away).
    pub fn range_box(
        &self,
        buffer: &Buffer,
        from: usize,
        to: usize,
        measure: &mut dyn Measure,
    ) -> Option<Rect> {
        let (first, into) = self.row_of(buffer, from, measure)?;
        let x = self.text_x() + first.x(into);
        let top = self.pad + self.y_of(first.index);
        let (last, last_into) = self.row_of(buffer, to.max(from), measure)?;
        let bottom = self.pad + self.y_of(last.index) + self.row;
        let end = if last.index == first.index {
            self.text_x() + last.x(last_into)
        } else {
            self.text_x() + first.x(first.to.max(into))
        };
        let thick = (self.options.caret.width * self.scale).round().max(1.0);
        Some(Rect::new(
            x,
            top,
            (end - x).max(thick),
            (bottom - top).max(self.row),
        ))
    }

    /// Where a caret on the row whose top is `top` starts down, and how tall it is (pixels): the
    /// row's, or its text's.
    fn caret_span(&self, top: f32) -> (f32, f32) {
        match self.options.caret.height {
            CaretHeight::Row => (top, self.row),
            CaretHeight::Text => (top + self.lead, self.line),
        }
    }

    /// The caret's box at `at` on `row`, `x` where the place is across and `top` the row's top
    /// (pixels): a bar or a block as tall as the row or the text, or a line under it; a block and
    /// a line as wide as the character there (a space at the end of a line).
    fn caret_box(&self, buffer: &Buffer, row: &Row, at: usize, x: f32, top: f32) -> Rect {
        let caret = self.options.caret;
        let thick = (caret.width * self.scale).round().max(1.0);
        let (y, h) = self.caret_span(top);
        let wide = || {
            let end = (row.base + row.to).min(grapheme_after(&buffer.text, at));
            if end > at {
                (row.x(end - row.base) - row.x(at - row.base)).max(thick)
            } else {
                self.space
            }
        };
        match caret.shape {
            CaretShape::Bar => Rect::new(x, y, thick, h),
            CaretShape::Block => Rect::new(x, y, wide(), h),
            CaretShape::Underline => Rect::new(x, y + h - thick, wide(), thick),
        }
    }

    /// The place under `point` (pixels of the content): on the row it is on (the first or the last
    /// where it is above or below them all), the boundary nearest it across.
    pub fn place_at(&self, buffer: &Buffer, point: Point, measure: &mut dyn Measure) -> usize {
        let row = self.row_at(point.y - self.pad);
        self.place_on_row(buffer, row.min(self.total() - 1), point.x, measure)
    }

    /// The place nearest `x` (pixels of the content) on row `row`.
    fn place_on_row(
        &self,
        buffer: &Buffer,
        row: usize,
        x: f32,
        measure: &mut dyn Measure,
    ) -> usize {
        let line = self.line_at_row(row);
        let rows = self.line_rows(buffer, line, measure);
        let into = row.saturating_sub(self.tops.get(line).copied().unwrap_or(0));
        let Some(found) = rows.get(into).or_else(|| rows.last()) else {
            return buffer.line_start(line);
        };
        let x = x - self.text_x() + found.shift;
        // A row that wraps ends where the next starts: a place there is the next row's.
        let to = if found.last {
            found.to
        } else {
            found.to.saturating_sub(1).max(found.from)
        };
        found.base + found.shaped.nearest(x, found.from, to)
    }

    /// Which row the place `at` is on, from the top, where it shows.
    pub fn row_index(
        &self,
        buffer: &Buffer,
        at: usize,
        measure: &mut dyn Measure,
    ) -> Option<usize> {
        self.row_of(buffer, at, measure).map(|(row, _)| row.index)
    }

    /// The lines whose rows are in view (the first and the last), scrolled as the view is.
    pub fn lines_in_view(&self) -> (usize, usize) {
        let top = self.scroll.y - self.pad;
        let bottom = top + self.room.h;
        let shown = |row: &&Row| {
            let y = self.y_of(row.index);
            y + self.row > top && y < bottom
        };
        let first = self.rows.iter().find(shown).map_or(0, |row| row.line);
        let last = self
            .rows
            .iter()
            .rev()
            .find(shown)
            .map_or(first, |row| row.line);
        (first, last)
    }

    // --- Drawing ---------------------------------------------------------------------------

    /// What the room draws: in `slot` (its place on the page, how far it is scrolled), the editor
    /// having the keyboard or not, the carets showing or not (the host blinks them).
    pub fn paint(
        &self,
        buffer: &Buffer,
        slot: &EditorSlot,
        focused: bool,
        caret_on: bool,
        measure: &mut dyn Measure,
    ) -> Vec<Paint> {
        let mut out = Vec::new();
        let fade = |color: Color| color.faded(slot.opacity);
        let room = slot.rect;
        let gutter = Rect::new(
            room.x,
            room.y,
            if self.gutter_shown { self.gutter } else { 0.0 },
            room.h,
        );
        let text_clip = slot.clip.intersect(Rect::new(
            room.x + gutter.w,
            room.y,
            room.w - gutter.w,
            room.h,
        ));
        let gutter_clip = slot.clip.intersect(gutter);
        let origin_x = room.x + self.text_x() - slot.scroll.x;
        let origin_y = room.y + self.pad - slot.scroll.y;
        let row_y = |row: &Row| origin_y + self.y_of(row.index);
        let quad = |rect: Rect, fill: Color, clip: Rect| {
            Paint::Quad(Quad {
                rect,
                radii: [0.0; 4],
                fill: Some(fill),
                gradient: None,
                border: None,
                shadow: None,
                clip,
            })
        };
        let in_view = |row: &&Row| {
            let y = row_y(row);
            y + self.row > room.y && y < room.bottom()
        };
        let rows: Vec<&Row> = self.rows.iter().filter(in_view).collect();
        let ranges = buffer.ranges();
        let main = buffer.main();
        let colors = &self.options.colors;

        // The line of the main caret: across the text, the gutter or both, as the page says.
        let band = if focused && main.is_empty() {
            rows.iter()
                .find(|row| self.on_row(row, main.head))
                .zip(colors.current_line.or(Role::Foreground.alpha(0.05)))
                .map(|(row, ground)| (row_y(row), ground))
        } else {
            None
        };
        if let Some((y, ground)) = band
            && self.options.current_line != LineHighlight::Gutter
        {
            out.push(quad(
                Rect::new(room.x + gutter.w, y, room.w - gutter.w, self.row),
                fade(ground),
                text_clip,
            ));
        }
        let found_ground = colors.found.or(Role::Warning.alpha(0.3));
        // The other places of the main selection's text, under what was found and what is selected.
        let matches = match (rows.first(), rows.last()) {
            (Some(first), Some(last)) if self.options.matches != Matches::None => {
                let (from, to) = (first.base + first.from, last.base + last.to);
                let of = (buffer.version(), main, from, to, self.options.matches);
                let mut kept = self.matches.borrow_mut();
                match kept.as_ref() {
                    Some((was, found)) if *was == of => Rc::clone(found),
                    _ => {
                        let found = Rc::new(buffer.matches_in(from, to));
                        *kept = Some((of, Rc::clone(&found)));
                        found
                    }
                }
            }
            _ => Rc::new(Vec::new()),
        };
        let matches_ground = colors.matches.or(Role::Selection.alpha(0.4));
        let selected_ground = if focused {
            colors.selection.or(Role::Selection.solid())
        } else {
            colors.selection_blurred.or(Role::Selection.alpha(0.5))
        };
        // The grounds of lines the page marked, what was found, what is selected.
        for row in &rows {
            let (from, to) = (row.base + row.from, row.base + row.to);
            let y = row_y(row);
            for (_, item) in buffer.decorations_in(from, to) {
                if let Mark::Line(color) = &item.mark {
                    out.push(quad(
                        Rect::new(room.x + gutter.w, y, room.w - gutter.w, self.row),
                        fade(*color),
                        text_clip,
                    ));
                }
            }
            if let Some(ground) = matches_ground {
                for (start, end) in matches.iter() {
                    if let Some(rect) = self.span(row, *start, *end, false, origin_x, y) {
                        out.push(quad(rect, fade(ground), text_clip));
                    }
                }
            }
            if let Some(ground) = found_ground {
                for (start, end) in buffer.found() {
                    if let Some(rect) = self.span(row, *start, *end, false, origin_x, y) {
                        out.push(quad(rect, fade(ground), text_clip));
                    }
                }
            }
            if let Some(ground) = selected_ground {
                for range in ranges.iter().filter(|range| !range.is_empty()) {
                    if let Some(rect) =
                        self.span(row, range.start(), range.end(), true, origin_x, y)
                    {
                        out.push(quad(rect, fade(ground), text_clip));
                    }
                }
            }
        }
        // The text, in the looks the page gave it, over the guides of the indent and the marks of
        // the spaces and tabs shown (each over the grounds the page's looks give the text).
        let whitespace = self.options.whitespace;
        let marks = match whitespace {
            Whitespace::None => None,
            _ => colors.whitespace.or(Role::Border.solid()).map(fade),
        };
        let guides = self.guides(buffer, &rows, (focused, main.head), origin_x, text_clip);
        let mut grounds = slot.grounds.clone();
        for (index, row) in rows.iter().enumerate() {
            let y = row_y(row);
            let line = buffer.line(row.line);
            let under = Under {
                whitespace,
                marks,
                ranges,
                guides: guides.get(index).map_or(&[], Vec::as_slice),
            };
            self.paint_row(
                buffer,
                row,
                &line,
                origin_x,
                y,
                text_clip,
                &mut grounds,
                slot,
                &under,
                &mut out,
            );
        }
        if buffer.len_chars() == 0
            && let Some(hint) = self.options.hint.clone().filter(|hint| !hint.is_empty())
            && let Some(color) = colors.hint.or(Role::Quiet.solid())
        {
            out.push(Paint::Text(Run {
                origin: Point::new(origin_x, origin_y + self.lead),
                text: hint,
                font: self.font,
                color,
                opacity: slot.opacity,
                grounds: slot.grounds.clone(),
                clip: text_clip,
                underline: false,
                strike: false,
            }));
        }
        // What an input method composes is underlined.
        if let Some((from, to)) = buffer.marked_range() {
            for row in &rows {
                if let Some(rect) = self.span(row, from, to, false, origin_x, row_y(row)) {
                    let thick = self.scale.round().max(1.0);
                    let bottom = rect.bottom() - self.lead;
                    let line = Rect::new(rect.x, bottom - thick * 2.0, rect.w, thick);
                    out.push(quad(line, fade(self.color), text_clip));
                }
            }
        }
        // The brackets that match, where the main caret is by one.
        if focused
            && main.is_empty()
            && let Some(ring) = colors.brackets.or(Role::Dim.alpha(0.6))
            && let Some((here, there)) = buffer.matching_bracket(main.head)
        {
            for at in [here, there] {
                if let Some(row) = rows.iter().find(|row| self.on_row(row, at))
                    && let Some(rect) = self.span(row, at, at + 1, false, origin_x, row_y(row))
                {
                    let rect = self.text_box(rect);
                    out.push(Paint::Quad(Quad {
                        rect,
                        radii: [2.0 * self.scale; 4],
                        fill: None,
                        gradient: None,
                        border: Some(pencere_view::Border {
                            width: self.scale.round().max(1.0),
                            color: fade(ring),
                        }),
                        shadow: None,
                        clip: text_clip,
                    }));
                }
            }
        }
        // The carets; a block's character is drawn again on it.
        let caret = self.options.caret;
        if focused
            && caret_on
            && let Some(color) = caret.color.or(Role::Cursor.solid())
        {
            for range in ranges {
                if let Some(row) = rows.iter().find(|row| self.on_row(row, range.head)) {
                    let at = range.head;
                    let x = origin_x + row.x(at - row.base);
                    let rect = self.caret_box(buffer, row, at, x, row_y(row));
                    out.push(quad(rect, fade(color), text_clip));
                    if caret.shape == CaretShape::Block
                        && let Some(text_color) = caret.text.or(Role::Background.solid())
                    {
                        let colors = (fade(color), text_color);
                        let opacity = slot.opacity;
                        let place = (rect, row_y(row) + self.lead);
                        self.paint_under_block(
                            buffer, row, at, place, colors, opacity, text_clip, &mut out,
                        );
                    }
                }
            }
        }
        if self.gutter_shown {
            let band = band.filter(|_| self.options.current_line != LineHighlight::Text);
            self.paint_gutter(
                buffer,
                &rows,
                slot,
                (gutter, gutter_clip),
                band,
                measure,
                &mut out,
            );
        }
        self.paint_ruler(buffer, slot, &mut out);
        out
    }

    /// Whether the place `at` is on `row`: inside it, or at its end where it is the last of its line.
    fn on_row(&self, row: &Row, at: usize) -> bool {
        at >= row.base + row.from
            && (at < row.base + row.to || (row.last && at == row.base + row.to))
    }

    /// The box `from..to` takes on `row` (pixels of the page; `origin_x` the start of the text, `y`
    /// the row's top); `newline` takes a space past the end of a row whose line break is in it.
    fn span(
        &self,
        row: &Row,
        from: usize,
        to: usize,
        newline: bool,
        origin_x: f32,
        y: f32,
    ) -> Option<Rect> {
        let (start, end) = (row.base + row.from, row.base + row.to);
        if from >= to || to < start || from > end || (from == end && !row.last) {
            return None;
        }
        let a = from.max(start) - row.base;
        let b = to.min(end) - row.base;
        let mut right = row.x(b);
        if newline && row.last && to > end {
            right += self.space;
        }
        let left = row.x(a);
        (right > left).then(|| Rect::new(origin_x + left, y, right - left, self.row))
    }

    /// The part of a row's box `rect` its text takes, the room above and below left out.
    fn text_box(&self, rect: Rect) -> Rect {
        Rect::new(rect.x, rect.y + self.lead, rect.w, self.line)
    }

    /// One row's text: cut where the look the page gave it changes and at its tabs, each run drawn
    /// in its look, with what is under and through it.
    #[allow(clippy::too_many_arguments)]
    fn paint_row(
        &self,
        buffer: &Buffer,
        row: &Row,
        line: &str,
        origin_x: f32,
        y: f32,
        clip: Rect,
        grounds: &mut Vec<Color>,
        slot: &EditorSlot,
        under: &Under<'_>,
        out: &mut Vec<Paint>,
    ) {
        let (start, end) = (row.base + row.from, row.base + row.to);
        let styles: Vec<(usize, usize, &Look)> = buffer
            .decorations_in(start, end)
            .into_iter()
            .filter_map(|(_, item)| match &item.mark {
                Mark::Style(look) => Some((item.from, item.to, look)),
                _ => None,
            })
            .collect();
        // Where a run ends: where a look starts or ends, and around each tab.
        let chars: Vec<char> = line.chars().collect();
        let mut cuts: Vec<usize> = vec![row.from, row.to];
        for (from, to, _) in &styles {
            for at in [*from, *to] {
                let at = at.saturating_sub(row.base);
                if at > row.from && at < row.to {
                    cuts.push(at);
                }
            }
        }
        for (at, c) in chars.iter().enumerate() {
            if *c == '\t' && at >= row.from && at < row.to {
                cuts.push(at);
                cuts.push(at + 1);
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        let fade = |color: Color| color.faded(slot.opacity);
        // Under each run and over its ground: the guides before its end, the marks of its spaces.
        let mut guides = under.guides.iter().peekable();
        let marks = under
            .marks
            .map(|color| Marks::of(&chars, row.base, under.whitespace, color, under.ranges));
        let marks = marks.as_ref();
        for pair in cuts.windows(2) {
            let (Some(a), Some(b)) = (pair.first().copied(), pair.get(1).copied()) else {
                continue;
            };
            let text: String = chars.get(a..b).unwrap_or_default().iter().collect();
            if text.is_empty() {
                continue;
            }
            if text == "\t" {
                let around = (origin_x, y, clip, slot.opacity);
                self.paint_under(row, &mut guides, marks, (a, b), around, out);
                continue;
            }
            let (from, to) = (row.base + a, row.base + b);
            // The look of the last layer, and its last decoration, that covers the run.
            let look = styles
                .iter()
                .rev()
                .find(|(f, t, look)| {
                    *f <= from && *t >= to && {
                        let over = self.pointer_char.is_some_and(|at| at >= *f && at < *t);
                        look.shows(self.mods, over)
                    }
                })
                .map(|(_, _, look)| *look);
            let x = origin_x + row.x(a);
            let width = row.x(b) - row.x(a);
            let mut font = self.font;
            let mut color = self.color;
            let mut strike = false;
            if let Some(look) = look {
                if let Some(c) = look.color {
                    color = c;
                }
                if let Some(weight) = look.weight {
                    font.weight = weight;
                }
                if let Some(italic) = look.italic {
                    font.italic = italic;
                }
                strike = look.strike;
                if let Some(ground) = look.background {
                    out.push(Paint::Quad(Quad {
                        rect: Rect::new(x, y, width, self.row),
                        radii: [0.0; 4],
                        fill: Some(fade(ground)),
                        gradient: None,
                        border: None,
                        shadow: None,
                        clip,
                    }));
                }
            }
            let around = (origin_x, y, clip, slot.opacity);
            self.paint_under(row, &mut guides, marks, (a, b), around, out);
            if !text.trim().is_empty() {
                grounds.clone_from(&slot.grounds);
                if let Some(ground) = look.and_then(|look| look.background) {
                    grounds.insert(0, ground);
                }
                out.push(Paint::Text(Run {
                    origin: Point::new(x, y + self.lead),
                    text,
                    font,
                    color,
                    opacity: slot.opacity,
                    grounds: grounds.clone(),
                    clip,
                    underline: false,
                    strike,
                }));
            }
            if let Some(underline) = look.and_then(|look| look.underline) {
                self.paint_underline(
                    underline,
                    self.text_box(Rect::new(x, y, width, self.row)),
                    clip,
                    slot.opacity,
                    out,
                );
            }
        }
        // The guides past the row's text: those of a line with none.
        for (x, color) in guides {
            out.push(self.guide(*x, y, color.faded(slot.opacity), clip));
        }
        // A folded region is a "⋯" after its first line.
        if row.last
            && buffer
                .folded()
                .iter()
                .any(|(from, _)| buffer.line_of(*from) == row.line)
        {
            let x = origin_x + row.shaped.width() - row.shift + self.space;
            let y = y + self.lead;
            let marker = Rect::new(x, y + self.line * 0.15, self.space * 3.0, self.line * 0.7);
            let colors = &self.options.colors;
            if let Some(ground) = colors.folded.or(Role::Foreground.alpha(0.08)) {
                out.push(Paint::Quad(Quad {
                    rect: marker,
                    radii: [self.line * 0.2; 4],
                    fill: Some(fade(ground)),
                    gradient: None,
                    border: None,
                    shadow: None,
                    clip,
                }));
            }
            if let Some(color) = colors.folds.or(Role::Dim.solid()) {
                out.push(Paint::Text(Run {
                    origin: Point::new(x + self.space * 0.5, y),
                    text: "⋯".into(),
                    font: self.font,
                    color,
                    opacity: slot.opacity,
                    grounds: slot.grounds.clone(),
                    clip,
                    underline: false,
                    strike: false,
                }));
            }
        }
    }

    /// The guides of the indent each of `rows` shows, where across (pixels of the page) and in what
    /// colour, left to right: on the first row of each line (a row that goes on with its line has
    /// none, as its text starts at the left), those of its guides in `clip` (`origin_x` where the
    /// text starts). The guide of the block the main caret (`caret`, a place) is in has its own
    /// colour while the editor has the keyboard.
    fn guides(
        &self,
        buffer: &Buffer,
        rows: &[&Row],
        (focused, caret): (bool, usize),
        origin_x: f32,
        clip: Rect,
    ) -> Vec<Vec<(f32, Color)>> {
        let colors = &self.options.colors;
        let guide = colors.indent_guides.or(Role::Border.solid());
        let active = colors.active_indent_guide.or(Role::Quiet.solid());
        let step = self.space * self.tab as f32;
        if !self.options.indent_guides || (guide.is_none() && active.is_none()) || step <= 0.0 {
            return Vec::new();
        }
        let mut levels = Levels::new(buffer);
        let block = match (rows.first(), rows.last()) {
            (Some(top), Some(bottom)) if focused && active.is_some() => {
                active_guide(&mut levels, buffer.line_of(caret), top.line, bottom.line)
            }
            _ => None,
        };
        let thick = self.scale.round().max(1.0);
        rows.iter()
            .map(|row| {
                if row.from != 0 {
                    return Vec::new();
                }
                let level = levels.of(row.line);
                let first = ((clip.x - thick - origin_x) / step).floor().max(0.0) as usize;
                let past = ((clip.right() - origin_x) / step).floor().max(0.0) as usize;
                (first..level.min(past.saturating_add(1)))
                    .filter_map(|at| {
                        let in_block = block.is_some_and(|(index, from, to)| {
                            index == at && (from..=to).contains(&row.line)
                        });
                        let color = if in_block { active.or(guide) } else { guide }?;
                        Some(((origin_x + at as f32 * step).round(), color))
                    })
                    .collect()
            })
            .collect()
    }

    /// What is drawn on `row` under its run from `a` to `b` (characters into the line) and over the
    /// run's ground: the `guides` left to right before the run's end, and the marks of the run's
    /// spaces and tabs `marks` shows; `around` is where the text starts across, the row's top, the
    /// clip and how faint what is drawn is.
    fn paint_under(
        &self,
        row: &Row,
        guides: &mut Peekable<slice::Iter<'_, (f32, Color)>>,
        marks: Option<&Marks<'_>>,
        (a, b): (usize, usize),
        (origin_x, y, clip, opacity): (f32, f32, Rect, f32),
        out: &mut Vec<Paint>,
    ) {
        let end = origin_x + row.x(b);
        while let Some((x, color)) = guides.next_if(|(x, _)| *x < end) {
            out.push(self.guide(*x, y, color.faded(opacity), clip));
        }
        let Some(marks) = marks else {
            return;
        };
        for at in a..b {
            let Some(&c) = marks.chars.get(at) else {
                break;
            };
            if !c.is_whitespace() {
                continue;
            }
            let (left, right) = (origin_x + row.x(at), origin_x + row.x(at + 1));
            if left > clip.right() {
                break;
            }
            // A space with an accent on it is a letter.
            if right <= left || right < clip.x || !marks.shows(at) {
                continue;
            }
            match c {
                ' ' => out.push(self.dot((left, right), y, marks.color, clip)),
                '\t' => out.extend(self.arrow((left, right), y, marks.color, clip)),
                c if !c.is_control() => {
                    out.push(self.space_box((left, right), y, marks.color, clip))
                }
                _ => {}
            }
        }
    }

    /// A guide of the indent at `x` on the row whose top is `y`: a line a pixel wide down the row,
    /// so that those of the rows one under the other are one line.
    fn guide(&self, x: f32, y: f32, color: Color, clip: Rect) -> Paint {
        Paint::Quad(Quad {
            rect: Rect::new(x, y, self.scale.round().max(1.0), self.row),
            radii: [0.0; 4],
            fill: Some(color),
            gradient: None,
            border: None,
            shadow: None,
            clip,
        })
    }

    /// How large the marks of the spaces are, pixels: the radius of a space's dot and the width of
    /// the line of a tab's arrow, a seventh of a space (as VS Code draws them).
    fn mark_unit(&self) -> f32 {
        self.space / 7.0
    }

    /// The dot of a space from `left` to `right` across, on the row whose top is `y`: in its
    /// middle, whole pixels wide.
    fn dot(&self, (left, right): (f32, f32), y: f32, color: Color, clip: Rect) -> Paint {
        let d = (2.0 * self.mark_unit()).round().max(1.0);
        let (x, y) = ((left + right) / 2.0, y + self.row / 2.0);
        Paint::Quad(Quad {
            rect: Rect::new((x - d / 2.0).round(), (y - d / 2.0).round(), d, d),
            radii: [d / 2.0; 4],
            fill: Some(color),
            gradient: None,
            border: None,
            shadow: None,
            clip,
        })
    }

    /// The arrow of a tab from `left` to `right` across, on the row whose top is `y`: a line along
    /// the middle of the row as wide as the tab (a seventh of a space in from each end), its head at
    /// the right; none for a tab too narrow for one. The figure's box is whole pixels, and its line
    /// on them, so that it is drawn sharp.
    fn arrow(&self, (left, right): (f32, f32), y: f32, color: Color, clip: Rect) -> Option<Paint> {
        let unit = self.mark_unit();
        let length = right - left - 2.0 * unit;
        if length <= 0.0 {
            return None;
        }
        let head = (2.0 * unit).min(length / 2.0);
        let stroke = unit.round().max(1.0);
        // An odd width of line is drawn sharp about the middle of a pixel.
        let middle = (y + self.row / 2.0).round() + if stroke % 2.0 == 1.0 { 0.5 } else { 0.0 };
        let (x, top) = (left.floor(), (middle - head - stroke).floor());
        let rect = Rect::new(
            x,
            top,
            (right.ceil() - x).max(1.0),
            (middle + head + stroke).ceil() - top,
        );
        // The figure's points, from its box's corner.
        let s = self.scale.max(0.01);
        let (start, tip, mid) = (
            (left + unit - x) / s,
            (right - unit - x) / s,
            (middle - top) / s,
        );
        let (back, head) = (tip - head / s, head / s);
        let line = path()
            .move_to(start, mid)
            .line_to(tip, mid)
            .move_to(back, mid - head)
            .line_to(tip, mid)
            .line_to(back, mid + head)
            .stroke(stroke / s, color);
        Some(Paint::Figure(Figure {
            rect,
            scale: s,
            paths: vec![line],
            clip: clip.intersect(rect),
        }))
    }

    /// The box of another space (a no-break space, an ideographic one) from `left` to `right`
    /// across, on the row whose top is `y`: as wide as it is and as tall as the text, a line a
    /// pixel wide round it.
    fn space_box(&self, (left, right): (f32, f32), y: f32, color: Color, clip: Rect) -> Paint {
        let width = right - left;
        Paint::Quad(Quad {
            rect: self.text_box(Rect::new(left, y, width, self.row)),
            radii: [(2.0 * self.scale).min(width / 2.0); 4],
            fill: None,
            gradient: None,
            border: Some(Border {
                width: self.scale.round().max(1.0),
                color,
            }),
            shadow: None,
            clip,
        })
    }

    /// The character at `at` on `row` drawn again on the block caret `rect`, its text's top at
    /// `text_y`: `colors` the block's and the character's, in the weight and slant the page's
    /// looks give it there.
    #[allow(clippy::too_many_arguments)]
    fn paint_under_block(
        &self,
        buffer: &Buffer,
        row: &Row,
        at: usize,
        (rect, text_y): (Rect, f32),
        (block, color): (Color, Color),
        opacity: f32,
        clip: Rect,
        out: &mut Vec<Paint>,
    ) {
        let end = (row.base + row.to).min(grapheme_after(&buffer.text, at));
        let Some(text) = buffer.slice(at, end).filter(|text| !text.trim().is_empty()) else {
            return;
        };
        let mut font = self.font;
        let look =
            buffer
                .decorations_in(at, end)
                .into_iter()
                .rev()
                .find_map(|(_, item)| match &item.mark {
                    Mark::Style(look) if item.from <= at && item.to >= end => Some(look),
                    _ => None,
                });
        if let Some(look) = look {
            if let Some(weight) = look.weight {
                font.weight = weight;
            }
            if let Some(italic) = look.italic {
                font.italic = italic;
            }
        }
        out.push(Paint::Text(Run {
            origin: Point::new(rect.x, text_y),
            text,
            font,
            color,
            opacity,
            grounds: vec![block],
            clip: clip.intersect(rect),
            underline: false,
            strike: false,
        }));
    }

    fn paint_underline(
        &self,
        underline: Underline,
        rect: Rect,
        clip: Rect,
        opacity: f32,
        out: &mut Vec<Paint>,
    ) {
        let thick = self.scale.round().max(1.0);
        match underline {
            Underline::Straight(color) => out.push(Paint::Quad(Quad {
                rect: Rect::new(rect.x, rect.bottom() - 2.0 * thick, rect.w, thick),
                radii: [0.0; 4],
                fill: Some(color.faded(opacity)),
                gradient: None,
                border: None,
                shadow: None,
                clip,
            })),
            Underline::Wavy(color) => {
                // A figure is drawn in points of its box: up and down every two points.
                let s = self.scale.max(0.01);
                let (w, h) = (rect.w / s, 4.0);
                let figure_rect = Rect::new(rect.x, rect.bottom() - h * s - thick, rect.w, h * s);
                let mut wave = path().move_to(0.0, h * 0.75);
                let mut x = 0.0;
                let mut up = true;
                while x < w {
                    x = (x + 2.0).min(w);
                    wave = wave.line_to(x, if up { h * 0.25 } else { h * 0.75 });
                    up = !up;
                }
                out.push(Paint::Figure(Figure {
                    rect: figure_rect,
                    scale: s,
                    paths: vec![wave.stroke(1.0, color.faded(opacity))],
                    clip: clip.intersect(figure_rect),
                }));
            }
        }
    }

    /// The gutter: its ground (so that what scrolls across goes under it), the main caret's line
    /// where `band` says (its top and colour), the number of each line, the buttons of the regions
    /// that fold and the markers of the page's columns.
    #[allow(clippy::too_many_arguments)]
    fn paint_gutter(
        &self,
        buffer: &Buffer,
        rows: &[&Row],
        slot: &EditorSlot,
        (gutter, clip): (Rect, Rect),
        band: Option<(f32, Color)>,
        measure: &mut dyn Measure,
        out: &mut Vec<Paint>,
    ) {
        let fade = |color: Color| color.faded(slot.opacity);
        let colors = &self.options.colors;
        let under = slot
            .grounds
            .iter()
            .find(|color| color.is_opaque())
            .copied()
            .unwrap_or(Role::Background.solid());
        let ground = colors.gutter.or(under);
        if let Some(ground) = ground {
            out.push(Paint::Quad(Quad {
                rect: gutter,
                radii: [0.0; 4],
                fill: Some(fade(ground)),
                gradient: None,
                border: None,
                shadow: None,
                clip,
            }));
        }
        // The main caret's line where the page draws it in the gutter too, `y` its top.
        if let Some((y, ground)) = band {
            out.push(Paint::Quad(Quad {
                rect: Rect::new(gutter.x, y, gutter.w, self.row),
                radii: [0.0; 4],
                fill: Some(fade(ground)),
                gradient: None,
                border: None,
                shadow: None,
                clip,
            }));
        }
        let grounds = ground.map_or_else(|| slot.grounds.clone(), |ground| vec![ground]);
        let main_line = buffer.line_of(buffer.main().head);
        let hidden = merged(buffer.hidden_lines());
        let relative = if self.options.numbers == Numbers::Relative {
            hidden.clone()
        } else {
            Vec::new()
        };
        // The lines a region folds from are asked for those in view alone: one that folds by the
        // indent is worked out from the line and the next with text.
        let lines = rows.iter().filter(|row| row.from == 0).map(|row| row.line);
        let folds = match (lines.clone().min(), lines.max()) {
            (Some(first), Some(last)) => buffer.fold_lines(first, last),
            _ => Vec::new(),
        };
        let origin_y = slot.rect.y + self.pad - slot.scroll.y;
        let numbers = self.cols.iter().find(|col| col.kind == Kind::Numbers);
        let folding = self.cols.iter().find(|col| col.kind == Kind::Folds);
        for row in rows.iter().filter(|row| row.from == 0) {
            let y = origin_y + self.y_of(row.index);
            let text_y = y + self.lead;
            let current = row.line == main_line;
            if let Some(col) = numbers {
                let number = match self.options.numbers {
                    Numbers::Absolute => Some(row.line + 1),
                    Numbers::Relative if current => Some(row.line + 1),
                    Numbers::Relative => Some(lines_between(&relative, row.line, main_line)),
                    Numbers::Hidden => None,
                };
                let color = if current {
                    colors.current_number.or(Role::Dim.solid())
                } else {
                    colors.numbers.or(Role::Quiet.solid())
                };
                if let (Some(number), Some(color)) = (number, color) {
                    let number = number.to_string();
                    let width = number.chars().count() as f32 * self.digit;
                    let x = gutter.x + col.x + col.w - self.pad - width;
                    out.push(Paint::Text(Run {
                        origin: Point::new(x, text_y),
                        text: number,
                        font: self.font,
                        color,
                        opacity: slot.opacity,
                        grounds: grounds.clone(),
                        clip,
                        underline: false,
                        strike: false,
                    }));
                }
            }
            if let Some(col) = folding
                && folds.binary_search(&row.line).is_ok()
                && let Some(color) = colors.folds.or(Role::Dim.solid())
            {
                let shut = buffer.is_folded(row.line);
                out.push(Paint::Text(Run {
                    origin: Point::new(gutter.x + col.x, text_y),
                    text: if shut { "▸" } else { "▾" }.into(),
                    font: self.font,
                    color,
                    opacity: slot.opacity,
                    grounds: grounds.clone(),
                    clip,
                    underline: false,
                    strike: false,
                }));
            }
        }
        self.paint_markers(buffer, rows, &hidden, slot, (gutter, clip), measure, out);
    }

    /// Where line `line` is as a person sees it: itself, or where it is folded away the line the
    /// fold is on.
    fn shown_line(hidden: &[(usize, usize)], line: usize) -> usize {
        match hidden
            .iter()
            .find(|(first, last)| line >= *first && line <= *last)
        {
            Some((first, _)) => first.saturating_sub(1),
            None => line,
        }
    }

    /// The lines a marker is by, as a person sees them: those a bar goes along, the first line of
    /// the range for the rest; a line that is folded away is the line of the fold. A range that ends
    /// where a line starts stops at the line before.
    fn mark_lines(
        buffer: &Buffer,
        hidden: &[(usize, usize)],
        item: &Decoration,
        shape: &GutterShape,
    ) -> (usize, usize) {
        let first = buffer.line_of(item.from);
        let last = if *shape == GutterShape::Bar && item.to > item.from {
            buffer.line_of(item.to - 1).max(first)
        } else {
            first
        };
        (
            Self::shown_line(hidden, first),
            Self::shown_line(hidden, last),
        )
    }

    /// The bottom of line `line`'s own rows, pixels of the content.
    pub fn line_bottom(&self, line: usize) -> f32 {
        self.pad + self.y_of(self.own_end(line))
    }

    /// The top of line `line`, pixels of the content.
    pub fn line_top(&self, line: usize) -> f32 {
        self.pad + self.y_of(self.tops.get(line).copied().unwrap_or(0))
    }

    /// The top of line `line` (pixels of the page, `origin_y` the content's top).
    fn top_of(&self, origin_y: f32, line: usize) -> f32 {
        origin_y + self.y_of(self.tops.get(line).copied().unwrap_or(0))
    }

    /// Where line `line`'s own rows end: the gaps kept open after it are below.
    fn end_of(&self, origin_y: f32, line: usize) -> f32 {
        origin_y + self.y_of(self.own_end(line))
    }

    /// The row line `line`'s own rows end at: the gaps kept open after it are below.
    fn own_end(&self, line: usize) -> usize {
        self.tops
            .get(line + 1)
            .copied()
            .unwrap_or(0)
            .saturating_sub(self.zone_rows(line))
    }

    /// The top of row `index`, pixels from the top of the content (its padding left out): the rows
    /// one under the other, less what the gaps before it took more than they are tall.
    fn y_of(&self, index: usize) -> f32 {
        index as f32 * self.row - self.shrink_at(index)
    }

    /// What the rows from `index` on are drawn higher than `index * row`, pixels.
    fn shrink_at(&self, index: usize) -> f32 {
        let (Some(last), true) = (self.shrinks.last(), self.tops.last().is_some()) else {
            return 0.0;
        };
        if index >= self.tops.last().copied().unwrap_or(0) {
            return *last;
        }
        let line = self.line_at_row(index);
        self.shrinks.get(line).copied().unwrap_or(*last)
    }

    /// The row at `y` pixels from the top of the content, its padding left out: the first for a
    /// place above them all, and a row of a gap where one is.
    fn row_at(&self, y: f32) -> usize {
        if y <= 0.0 {
            return 0;
        }
        if self.shrinks.is_empty() {
            return (y / self.row).floor() as usize;
        }
        // The rows are one under the other, so the first one that starts below `y` is found by
        // halving.
        let (mut low, mut high) = (0, self.total().max(1));
        while low < high {
            let middle = low + (high - low) / 2;
            if self.y_of(middle) <= y {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low.saturating_sub(1)
    }

    /// The markers of column `col`, with the layer each is of and its decoration, in the order they
    /// are drawn (the later over the earlier) that stand by `line`.
    pub fn markers_at<'a>(
        &self,
        buffer: &'a Buffer,
        hidden: &[(usize, usize)],
        col: usize,
        line: usize,
    ) -> Vec<(usize, &'a Decoration, &'a GutterMark)> {
        let Some(column) = self
            .cols
            .get(col)
            .filter(|column| matches!(column.kind, Kind::Marks | Kind::Page))
        else {
            return Vec::new();
        };
        let from = buffer.line_start(line);
        // The lines folded away after this one are the fold's: what is marked in them is shown here.
        let after = hidden
            .iter()
            .find(|(first, _)| *first == line + 1)
            .map_or(line, |(_, last)| *last);
        let to = buffer.line_start(after + 1).max(from);
        buffer
            .decorations_in(from, to)
            .into_iter()
            .filter_map(|(layer, item)| {
                let Mark::Gutter(mark) = &item.mark else {
                    return None;
                };
                let (first, last) = Self::mark_lines(buffer, hidden, item, &mark.shape);
                (column.holds(&mark.column) && (first..=last).contains(&line))
                    .then_some((layer, item, mark))
            })
            .collect()
    }

    /// The gutter's markers: each in the column it names, a dot on the first row of its line, a bar
    /// along every row of its lines, a triangle on the edge above or below its line, a picture; the
    /// one the pointer is over lit, the one it is pressing pressed, and in the cell it is over where
    /// nothing stands the column's preview.
    #[allow(clippy::too_many_arguments)]
    fn paint_markers(
        &self,
        buffer: &Buffer,
        rows: &[&Row],
        hidden: &[(usize, usize)],
        slot: &EditorSlot,
        (gutter, clip): (Rect, Rect),
        measure: &mut dyn Measure,
        out: &mut Vec<Paint>,
    ) {
        let (Some(first), Some(last)) = (rows.first(), rows.last()) else {
            return;
        };
        let origin_y = slot.rect.y + self.pad - slot.scroll.y;
        let lit = self
            .options
            .colors
            .mark_hover
            .or(Role::Foreground.alpha(0.3));
        let (start, end) = (first.base + first.from, last.base + last.to);
        let shown = |line: usize| rows.iter().any(|row| row.line == line);
        for (layer, item) in buffer.decorations_in(start, end) {
            let Mark::Gutter(mark) = &item.mark else {
                continue;
            };
            let Some((at, col)) = self
                .cols
                .iter()
                .enumerate()
                .find(|(_, col)| col.holds(&mark.column) && col.kind != Kind::Numbers)
                .filter(|(_, col)| col.kind != Kind::Folds)
            else {
                continue;
            };
            let (from, to) = Self::mark_lines(buffer, hidden, item, &mark.shape);
            let held = Held {
                layer,
                from: item.from,
                to: item.to,
                id: mark.id.clone(),
                cursor: mark.cursor,
            };
            let over = |cell: &Option<Cell>| {
                cell.as_ref()
                    .is_some_and(|cell| cell.col == at && cell.marker.as_ref() == Some(&held))
            };
            let (hovering, pressing) = (over(&self.hover), over(&self.pressed));
            let color = match (pressing, hovering) {
                (true, _) if mark.pressed.is_some() => mark.pressed,
                (_, true) if mark.hover.is_some() => mark.hover,
                _ => None,
            }
            .unwrap_or(mark.color);
            let veil = ((hovering && mark.hover.is_none()) || (pressing && mark.pressed.is_none()))
                .then_some(lit)
                .flatten();
            // The lines of a marker that are all above or all below the rows laid out are not in view.
            if from > last.line || to < first.line {
                continue;
            }
            let single = (self.top_of(origin_y, from), self.end_of(origin_y, from));
            let span = (self.top_of(origin_y, from), self.end_of(origin_y, to));
            self.paint_shape(
                &mark.shape,
                color,
                veil,
                hovering || pressing,
                col,
                (gutter, clip),
                (single, span),
                slot,
                measure,
                out,
            );
        }
        // Where the pointer is over a cell with nothing in it, what the column shows to be put.
        if let Some(cell) = &self.hover
            && cell.marker.is_none()
            && let Some(col) = self.cols.get(cell.col)
            && let Some(preview) = &col.hover
            && shown(cell.line)
        {
            let top = self.top_of(origin_y, cell.line);
            let single = (top, self.end_of(origin_y, cell.line));
            self.paint_shape(
                &preview.shape,
                preview.color,
                None,
                false,
                col,
                (gutter, clip),
                (single, single),
                slot,
                measure,
                out,
            );
        }
    }

    /// One marker's shape in column `col`: `single` the top and the bottom of its first line and
    /// `span` those of all its lines, pixels of the page; `wide` the pointer is on it.
    #[allow(clippy::too_many_arguments)]
    fn paint_shape(
        &self,
        shape: &GutterShape,
        color: Color,
        veil: Option<Color>,
        wide: bool,
        col: &Col,
        (gutter, clip): (Rect, Rect),
        (single, span): ((f32, f32), (f32, f32)),
        slot: &EditorSlot,
        measure: &mut dyn Measure,
        out: &mut Vec<Paint>,
    ) {
        let looks = &self.options.marks;
        let s = self.scale.max(0.01);
        let x0 = gutter.x + col.x;
        let fill = |rect: Rect, color: Color, radius: f32| {
            Paint::Quad(Quad {
                rect,
                radii: [radius; 4],
                fill: Some(color.faded(slot.opacity)),
                gradient: None,
                border: None,
                shadow: None,
                clip,
            })
        };
        match shape {
            GutterShape::Dot => {
                let size = (col.w * 0.6).max(1.0).min(self.row);
                let rect = Rect::new(
                    x0 + (col.w - size) / 2.0,
                    single.0 + (self.row - size) / 2.0,
                    size,
                    size,
                );
                out.push(fill(rect, color, size / 2.0));
                if let Some(veil) = veil {
                    out.push(fill(rect, veil, size / 2.0));
                }
            }
            GutterShape::Bar => {
                let widest = looks.hover.max(looks.bar) * s;
                let width = (if wide { looks.hover } else { looks.bar } * s)
                    .round()
                    .max(1.0);
                let rect = Rect::new(
                    x0 + ((col.w - widest) / 2.0).max(0.0),
                    span.0,
                    width,
                    span.1 - span.0,
                );
                if rect.h <= 0.0 {
                    return;
                }
                out.push(fill(rect, color, 0.0));
                if let Some(veil) = veil {
                    out.push(fill(rect, veil, 0.0));
                }
            }
            GutterShape::Above | GutterShape::Below => {
                let edge = if *shape == GutterShape::Below {
                    single.1
                } else {
                    single.0
                };
                if single.1 <= single.0 {
                    return;
                }
                // A triangle pointing at the text, its height three bars, its width half that.
                let side =
                    ((if wide { looks.hover } else { looks.bar }) * 3.0).min(self.row / s * 0.9);
                let widest = looks.hover.max(looks.bar) * s;
                let rect = Rect::new(
                    x0 + ((col.w - widest) / 2.0).max(0.0),
                    edge - side * s / 2.0,
                    side * s / 2.0,
                    side * s,
                );
                let triangle = path()
                    .move_to(0.0, 0.0)
                    .line_to(side / 2.0, side / 2.0)
                    .line_to(0.0, side)
                    .close();
                let mut paths = vec![triangle.clone().fill(color.faded(slot.opacity))];
                if let Some(veil) = veil {
                    paths.push(triangle.fill(veil.faded(slot.opacity)));
                }
                out.push(Paint::Figure(Figure {
                    rect,
                    scale: s,
                    paths,
                    clip: clip.intersect(rect),
                }));
            }
            GutterShape::Image(src) => {
                let room = (col.w.min(self.row) * 0.8).max(1.0);
                let size = match measure.image(src) {
                    Some(natural) if natural.w > 0.0 && natural.h > 0.0 => {
                        let fit = (room / natural.w).min(room / natural.h);
                        Size::new(natural.w * fit, natural.h * fit)
                    }
                    _ => Size::new(room, room),
                };
                let rect = Rect::new(
                    x0 + (col.w - size.w) / 2.0,
                    single.0 + (self.row - size.h) / 2.0,
                    size.w,
                    size.h,
                );
                out.push(Paint::Image(Picture {
                    src: src.clone(),
                    rect,
                    crop: Rect::new(0.0, 0.0, 1.0, 1.0),
                    tint: Some(color),
                    opacity: slot.opacity,
                    radii: [0.0; 4],
                    clip: clip.intersect(rect),
                    turn: 0.0,
                }));
                if let Some(veil) = veil {
                    out.push(Paint::Image(Picture {
                        src: src.clone(),
                        rect,
                        crop: Rect::new(0.0, 0.0, 1.0, 1.0),
                        tint: Some(veil),
                        opacity: slot.opacity,
                        radii: [0.0; 4],
                        clip: clip.intersect(rect),
                        turn: 0.0,
                    }));
                }
            }
        }
    }

    /// The marks of the overview ruler: down the right edge of the room, each where its lines are in
    /// the document as the room is as tall as the document, at least two points tall, in its lane.
    fn paint_ruler(&self, buffer: &Buffer, slot: &EditorSlot, out: &mut Vec<Paint>) {
        let marks = buffer.rulers();
        if marks.is_empty() {
            return;
        }
        let s = self.scale.max(0.01);
        let total = self.y_of(self.total()).max(1.0);
        let room = slot.rect;
        let width = (self.options.marks.ruler * s).round().max(3.0);
        let strip = room.right() - width - 2.0 * s;
        for (_, item) in marks {
            let Mark::Ruler(mark) = &item.mark else {
                continue;
            };
            let (top, end) = self.ruler_rows(buffer, item);
            let (y_top, y_end) = (self.y_of(top), self.y_of(end));
            let y = room.y + room.h * y_top / total;
            let h = (room.h * (y_end - y_top) / total).max((2.0 * s).round());
            let third = (width / 3.0).floor().max(1.0);
            let (x, w) = match mark.lane {
                RulerLane::Full => (strip, width),
                RulerLane::Left => (strip, third),
                RulerLane::Center => (strip + third, third),
                RulerLane::Right => (strip + width - third, third),
            };
            out.push(Paint::Quad(Quad {
                rect: Rect::new(x, y.min(room.bottom() - h).max(room.y), w, h),
                radii: [0.0; 4],
                fill: Some(mark.color.faded(slot.opacity)),
                gradient: None,
                border: None,
                shadow: None,
                clip: slot.clip.intersect(room),
            }));
        }
    }

    /// The columns laid out again for a document of `lines` lines: those the page lists (the
    /// editor's three where it lists none), each as wide as it says or as its kind is.
    pub fn size_gutter(&mut self, lines: usize) {
        self.cols.clear();
        if !self.gutter_shown {
            self.gutter = 0.0;
            return;
        }
        let default: Vec<Column> = ["marks", "numbers", "folds"]
            .into_iter()
            .map(Column::new)
            .collect();
        let list = if self.options.gutters.is_empty() {
            &default
        } else {
            &self.options.gutters
        };
        let digits = lines.max(1).to_string().len().max(2);
        let s = self.scale.max(0.01);
        let mut x = 0.0;
        for column in list.iter().take(limits::COLUMNS) {
            if self.cols.iter().any(|col| col.name == column.name) {
                continue;
            }
            let kind = match column.name.as_str() {
                "marks" => Kind::Marks,
                "numbers" => Kind::Numbers,
                "folds" => Kind::Folds,
                _ => Kind::Page,
            };
            let own = match kind {
                Kind::Numbers if self.options.numbers == Numbers::Hidden => 0.0,
                Kind::Numbers => {
                    let left = column
                        .pad
                        .map_or(if x == 0.0 { self.pad } else { 0.0 }, |points| {
                            (points.clamp(*limits::COLUMN.start(), *limits::COLUMN.end()) * s)
                                .round()
                        });
                    left + digits as f32 * self.digit + self.pad
                }
                Kind::Folds => (self.line * 0.8).round(),
                Kind::Marks | Kind::Page => (self.line * 0.6).round(),
            };
            let w = column.width.map_or(own, |points| {
                (points.clamp(*limits::COLUMN.start(), *limits::COLUMN.end()) * s).round()
            });
            self.cols.push(Col {
                name: column.name.clone(),
                kind,
                x,
                w,
                heard: column.heard(),
                hover: column.hover.clone(),
            });
            x += w;
        }
        self.gutter = x;
    }

    /// What is in the gutter at `y` (pixels of the content) and `x` (pixels of the room): the
    /// column, the line, and the marker that stands there, the later of two over the earlier.
    pub fn cell_at(&self, buffer: &Buffer, x: f32, y: f32) -> Option<Cell> {
        if !self.gutter_shown || x < 0.0 || x >= self.gutter {
            return None;
        }
        let row = self.row_at(y - self.pad);
        // Below the last row there is no line, nor in a gap kept open for the page.
        if row >= self.total() {
            return None;
        }
        let line = self.line_at_row(row);
        if row + self.zone_rows(line) >= self.tops.get(line + 1).copied().unwrap_or(0) {
            return None;
        }
        let col = self
            .cols
            .iter()
            .position(|col| x >= col.x && x < col.x + col.w)?;
        let hidden = merged(buffer.hidden_lines());
        let marker =
            self.markers_at(buffer, &hidden, col, line)
                .last()
                .map(|(layer, item, mark)| Held {
                    layer: *layer,
                    from: item.from,
                    to: item.to,
                    id: mark.id.clone(),
                    cursor: mark.cursor,
                });
        Some(Cell { col, line, marker })
    }

    /// What the markers in view say of themselves: the column each is in, the lines it is by (the
    /// first and the last) and its words.
    pub fn labels(&self, buffer: &Buffer) -> Vec<(usize, usize, usize, String)> {
        let (Some(first), Some(last)) = (self.rows.first(), self.rows.last()) else {
            return Vec::new();
        };
        let hidden = merged(buffer.hidden_lines());
        let mut out = Vec::new();
        for (_, item) in buffer.decorations_in(first.base + first.from, last.base + last.to) {
            let Mark::Gutter(mark) = &item.mark else {
                continue;
            };
            let Some(label) = &mark.label else {
                continue;
            };
            let Some(col) = self.cols.iter().position(|col| {
                col.holds(&mark.column) && matches!(col.kind, Kind::Marks | Kind::Page)
            }) else {
                continue;
            };
            let (from, to) = Self::mark_lines(buffer, &hidden, item, &mark.shape);
            out.push((col, from, to, label.clone()));
        }
        out
    }

    /// The lines a mark of the overview ruler covers, as the rows they take (`start..end`), over
    /// all the rows there are.
    fn ruler_rows(&self, buffer: &Buffer, item: &Decoration) -> (usize, usize) {
        let first = buffer.line_of(item.from);
        let last = if item.to > item.from {
            buffer.line_of(item.to - 1).max(first)
        } else {
            first
        };
        let top = self.tops.get(first).copied().unwrap_or(0);
        // The gaps kept open after the last line are not its rows.
        let end = self.own_end(last);
        (top, end.max(top))
    }
}

/// What a column of the gutter is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// The editor's column of dots.
    Marks,
    Numbers,
    Folds,
    /// A column of the page's.
    Page,
}

/// One column of the gutter, laid out.
#[derive(Clone, Debug)]
pub(crate) struct Col {
    pub name: String,
    pub kind: Kind,
    /// Where it starts across the gutter and how wide it is, pixels.
    pub x: f32,
    pub w: f32,
    pub heard: ColumnEvents,
    pub hover: Option<Preview>,
}

impl Col {
    /// Whether a marker that names column `name` (none: the dots') stands in it.
    fn holds(&self, name: &str) -> bool {
        self.name == if name.is_empty() { "marks" } else { name }
    }
}

/// A marker: the layer of decorations it is of and its range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Held {
    pub layer: usize,
    pub from: usize,
    pub to: usize,
    pub id: Option<String>,
    pub cursor: Option<Cursor>,
}

/// A cell of the gutter: a column and a line, and the marker that stands there if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Cell {
    pub col: usize,
    pub line: usize,
    pub marker: Option<Held>,
}

/// What a row draws under its text and over its grounds, besides the grounds of the page's looks.
struct Under<'a> {
    /// Which spaces and tabs are shown, and in what colour (none: they are not); the ranges
    /// selected, for those shown in them.
    whitespace: Whitespace,
    marks: Option<Color>,
    ranges: &'a [Range],
    /// The guides of the indent on the row: where across (pixels of the page) and in what colour,
    /// left to right.
    guides: &'a [(f32, Color)],
}

/// Which of a line's spaces and tabs are shown ([`Whitespace`]), and in what colour.
struct Marks<'a> {
    whitespace: Whitespace,
    color: Color,
    chars: &'a [char],
    /// The first and the last of the line's characters that are not spaces; none on a line of
    /// spaces alone.
    text: Option<(usize, usize)>,
    /// Where the line starts in the document, and the ranges selected, in order.
    base: usize,
    ranges: &'a [Range],
}

impl<'a> Marks<'a> {
    fn of(
        chars: &'a [char],
        base: usize,
        whitespace: Whitespace,
        color: Color,
        ranges: &'a [Range],
    ) -> Self {
        let text = |c: &char| !c.is_whitespace();
        Self {
            whitespace,
            color,
            chars,
            text: chars
                .iter()
                .position(text)
                .zip(chars.iter().rposition(text)),
            base,
            ranges,
        }
    }

    /// Whether the space or tab `at` characters into the line is shown (VS Code's rules).
    fn shows(&self, at: usize) -> bool {
        match self.whitespace {
            Whitespace::None => false,
            Whitespace::All => true,
            Whitespace::Trailing => self.text.is_none_or(|(_, last)| at > last),
            Whitespace::Selection => {
                let place = self.base + at;
                let next = self.ranges.partition_point(|range| range.end() <= place);
                self.ranges
                    .get(next)
                    .is_some_and(|range| range.start() <= place)
            }
            Whitespace::Boundary => {
                // Every one but a space alone between two words.
                let between = self
                    .text
                    .is_some_and(|(first, last)| at > first && at < last);
                let space = |at: Option<usize>| {
                    at.and_then(|at| self.chars.get(at))
                        .is_some_and(|c| c.is_whitespace())
                };
                !between
                    || self.chars.get(at) != Some(&' ')
                    || space(at.checked_sub(1))
                    || space(at.checked_add(1))
            }
        }
    }
}

/// How many lines apart `a` and `b` are as a person moves over them: the `hidden` lines (merged,
/// first and last of each range) are not counted, a folded region being one line, its first.
fn lines_between(hidden: &[(usize, usize)], a: usize, b: usize) -> usize {
    let (low, high) = (a.min(b), a.max(b));
    let skipped: usize = hidden
        .iter()
        .map(|(first, last)| (*last.min(&high) + 1).saturating_sub(*first.max(&(low + 1))))
        .sum();
    (high - low).saturating_sub(skipped)
}

/// The ranges of hidden lines in order, those that overlap made one.
fn merged(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    ranges.sort_unstable();
    let mut out: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (first, last) in ranges {
        match out.last_mut() {
            Some(open) if first <= open.1 + 1 => open.1 = open.1.max(last),
            _ => out.push((first, last)),
        }
    }
    out
}

/// The rows of a view, for moving up and down by them: what [`Buffer::command_in`] is given.
pub(crate) struct Rows<'a> {
    pub view: &'a View,
    pub measure: RefCell<&'a mut dyn Measure>,
}

impl Placement for Rows<'_> {
    fn x_of(&self, buffer: &Buffer, at: usize) -> f32 {
        let mut measure = self.measure.borrow_mut();
        self.view
            .point_of(buffer, at, &mut **measure)
            .map_or(0.0, |point| point.x)
    }

    fn row_from(&self, buffer: &Buffer, at: usize, rows: isize, x: f32) -> Option<usize> {
        let mut measure = self.measure.borrow_mut();
        let row = self.view.row_index(buffer, at, &mut **measure)?;
        let target = row.checked_add_signed(rows)?;
        if target >= self.view.total() {
            return None;
        }
        Some(self.view.place_on_row(buffer, target, x, &mut **measure))
    }

    fn page_rows(&self) -> usize {
        ((self.view.room.h / self.view.row).floor() as usize)
            .saturating_sub(1)
            .max(1)
    }
}
