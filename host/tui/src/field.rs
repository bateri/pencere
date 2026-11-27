//! A text field of the host's: what the person has typed in it, in the cells of the room it is drawn in.
//!
//! The page puts a field over the room of an element, fills it (a text, what is selected) and is told
//! what is typed. Which keys the field keeps and which are the page's is the kit's to say
//! ([`FieldState::takes`]); this keeps the text, the caret and the selection and edits them (the
//! drawing is `chrome`'s).

use std::cell::Cell;

use pencere_view::{FieldKind, FieldState, Fill, Key, Mods, break_rows};
use unicode_width::UnicodeWidthChar;

/// The most a field remembers to undo.
const HISTORY: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    text: String,
    anchor: usize,
    head: usize,
}

/// What a key did to a field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Edited {
    /// The text is not what it was: the page is told.
    pub changed: bool,
    /// Text for the pasteboard, and whether it was cut.
    pub copy: Option<(String, bool)>,
}

/// What of a field is in its room: see [`TextField::slice`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Slice {
    pub text: String,
    pub selected: Option<std::ops::Range<usize>>,
    pub caret: usize,
}

/// One row of a field of lines in its room: the characters of it that are shown and the cells the
/// selection covers in it, counted from the start of the row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Row {
    pub text: String,
    pub selected: Option<std::ops::Range<usize>>,
}

/// What of a field of lines is in its room: the rows that are shown, from the first the room has
/// the caret in view from, and where the caret is among them (row, cell).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Block {
    pub rows: Vec<Row>,
    pub caret: (usize, usize),
    /// The row before the first shown and the row after the last, where there is one: a host that
    /// has padding round its rows shows what of them stands in it.
    pub before: Option<Row>,
    pub after: Option<Row>,
}

/// How many cells `text` takes.
fn cells_of(text: &str) -> usize {
    text.chars()
        .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
        .sum()
}

/// The rows `text` is broken into in a room `cols` cells across (the rows of
/// [`pencere_view::break_rows`], measured in cells): each a range of bytes, and at least one.
pub(crate) fn rows_of(text: &str, cols: u16) -> Vec<std::ops::Range<usize>> {
    #[allow(clippy::cast_precision_loss)]
    let cells = |run: &str| cells_of(run) as f32;
    break_rows(text, f32::from(cols.max(1)), &mut |run| cells(run))
}

/// One field.
#[derive(Clone, Debug)]
pub struct TextField {
    kind: FieldKind,
    text: String,
    /// The selection's fixed end and its moving end, bytes: the caret is the moving end.
    anchor: usize,
    head: usize,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// How a field of lines was last drawn: the first row it showed, and the cells across and the rows
    /// high the room was. A room that shows less than the text keeps the row it showed until the
    /// caret leaves it, and the keys that go by rows go as far as the room it was drawn in.
    top: Cell<usize>,
    cols: Cell<u16>,
    shown: Cell<usize>,
}

impl TextField {
    pub fn new(kind: FieldKind) -> Self {
        Self {
            kind,
            text: String::new(),
            anchor: 0,
            head: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            top: Cell::new(0),
            cols: Cell::new(0),
            shown: Cell::new(1),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Where the keyboard is in it, as the kit wants to be told, in characters.
    pub fn state(&self) -> FieldState {
        let at = |bytes: usize| {
            self.text
                .get(..bytes)
                .map_or(0, |head| head.chars().count())
        };
        let (a, b) = (at(self.anchor), at(self.head));
        FieldState {
            kind: self.kind,
            length: self.text.chars().count(),
            selection: a..b,
        }
    }

    /// What a reply put in: whether the text changed.
    pub fn fill(&mut self, fill: &Fill) -> bool {
        let mut changed = false;
        if let Some(text) = &fill.text {
            changed = *text != self.text;
            self.text.clone_from(text);
            self.head = self.text.len();
            self.anchor = self.head;
            self.undo.clear();
            self.redo.clear();
        }
        if let Some(range) = fill.selection_in(self.text.len()) {
            self.anchor = self.boundary_at_or_before(range.start);
            self.head = self.boundary_at_or_before(range.end);
        }
        changed
    }

    /// What a key does where the field keeps it. `typed` is the text the key types, if it types any.
    pub fn key(&mut self, key: Key, mods: Mods, typed: Option<&str>) -> Edited {
        let chord = mods.control || mods.command;
        let mut edited = Edited::default();
        match key {
            Key::Character(c) if chord => match c.to_ascii_lowercase() {
                'a' => {
                    self.anchor = 0;
                    self.head = self.text.len();
                }
                'z' if mods.shift => edited.changed = self.step(true),
                'z' => edited.changed = self.step(false),
                'y' => edited.changed = self.step(true),
                'c' | 'x' => {
                    let cut = c.eq_ignore_ascii_case(&'x');
                    if let Some(text) = self.selected()
                        && !matches!(self.kind, FieldKind::Secret)
                    {
                        edited.copy = Some((text.to_owned(), cut));
                        if cut {
                            edited.changed = self.replace_selection("");
                        }
                    }
                }
                _ => {}
            },
            Key::Character(_) => {
                if let Some(text) = typed {
                    edited.changed = self.replace_selection(text);
                }
            }
            Key::Enter => {
                if matches!(self.kind, FieldKind::Lines) {
                    edited.changed = self.replace_selection("\n");
                }
            }
            Key::Backspace => {
                if self.anchor == self.head {
                    let to = if mods.alt || mods.control {
                        self.word_before(self.head)
                    } else {
                        self.before(self.head)
                    };
                    self.anchor = to;
                }
                edited.changed = self.replace_selection("");
            }
            Key::Delete => {
                if self.anchor == self.head {
                    let to = if mods.alt || mods.control {
                        self.word_after(self.head)
                    } else {
                        self.after(self.head)
                    };
                    self.head = to;
                }
                edited.changed = self.replace_selection("");
            }
            Key::Left => self.travel(mods, |me, at| {
                if mods.alt || mods.control {
                    me.word_before(at)
                } else {
                    me.before(at)
                }
            }),
            Key::Right => self.travel(mods, |me, at| {
                if mods.alt || mods.control {
                    me.word_after(at)
                } else {
                    me.after(at)
                }
            }),
            Key::Home => self.travel(mods, |_, _| 0),
            Key::End => self.travel(mods, |me, _| me.text.len()),
            Key::Up => self.travel(mods, |me, at| me.vertical(at, 1, true)),
            Key::Down => self.travel(mods, |me, at| me.vertical(at, 1, false)),
            Key::PageUp => {
                let by = self.shown.get();
                self.travel(mods, |me, at| me.vertical(at, by, true));
            }
            Key::PageDown => {
                let by = self.shown.get();
                self.travel(mods, |me, at| me.vertical(at, by, false));
            }
            _ => {}
        }
        edited
    }

    /// What was pasted: whether the text changed.
    pub fn paste(&mut self, text: &str) -> bool {
        let text = if matches!(self.kind, FieldKind::Lines) {
            text.to_owned()
        } else {
            text.replace(['\n', '\r'], " ")
        };
        self.replace_selection(&text)
    }

    /// Moves the caret, with the other end left where it is where shift is held. A selection that
    /// is left with no shift ends where the caret goes toward.
    fn travel(&mut self, mods: Mods, to: impl Fn(&Self, usize) -> usize) {
        let (from, upto) = (self.anchor.min(self.head), self.anchor.max(self.head));
        let moved = if self.anchor == self.head || mods.shift {
            to(self, self.head)
        } else if to(self, self.head) < self.head {
            from
        } else {
            upto
        };
        self.head = moved;
        if !mods.shift {
            self.anchor = moved;
        }
    }

    fn selected(&self) -> Option<&str> {
        let (from, to) = (self.anchor.min(self.head), self.anchor.max(self.head));
        (from < to).then(|| self.text.get(from..to)).flatten()
    }

    /// `text` in place of the selection: whether anything changed.
    fn replace_selection(&mut self, text: &str) -> bool {
        let (from, to) = (self.anchor.min(self.head), self.anchor.max(self.head));
        if from == to && text.is_empty() {
            return false;
        }
        self.remember();
        self.text.replace_range(from..to, text);
        self.head = from + text.len();
        self.anchor = self.head;
        true
    }

    fn remember(&mut self) {
        if self.undo.len() >= HISTORY {
            self.undo.remove(0);
        }
        self.undo.push(self.snapshot());
        self.redo.clear();
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            anchor: self.anchor,
            head: self.head,
        }
    }

    /// Undoes or, with `forward`, redoes one edit: whether the text changed.
    fn step(&mut self, forward: bool) -> bool {
        let now = self.snapshot();
        let taken = if forward {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        let Some(snapshot) = taken else {
            return false;
        };
        if forward {
            self.undo.push(now.clone());
        } else {
            self.redo.push(now.clone());
        }
        let changed = snapshot.text != now.text;
        self.text = snapshot.text;
        self.anchor = snapshot.anchor;
        self.head = snapshot.head;
        changed
    }

    fn boundary_at_or_before(&self, at: usize) -> usize {
        let mut at = at.min(self.text.len());
        while !self.text.is_char_boundary(at) {
            at -= 1;
        }
        at
    }

    fn before(&self, at: usize) -> usize {
        self.text
            .get(..at)
            .and_then(|head| head.char_indices().next_back())
            .map_or(0, |(index, _)| index)
    }

    fn after(&self, at: usize) -> usize {
        self.text
            .get(at..)
            .and_then(|tail| tail.chars().next())
            .map_or(self.text.len(), |c| at + c.len_utf8())
    }

    fn word_before(&self, at: usize) -> usize {
        let head = self.text.get(..at).unwrap_or("");
        let trimmed = head.trim_end_matches(|c: char| !c.is_alphanumeric());
        let word = trimmed.trim_end_matches(char::is_alphanumeric);
        word.len()
    }

    fn word_after(&self, at: usize) -> usize {
        let tail = self.text.get(at..).unwrap_or("");
        let after_gap = tail.trim_start_matches(|c: char| !c.is_alphanumeric());
        let rest = after_gap.trim_start_matches(char::is_alphanumeric);
        self.text.len() - rest.len()
    }

    /// Where the caret stands, in cells from the start of the text.
    fn caret_cells(&self) -> usize {
        let line: &str = self.text.lines().next().unwrap_or("");
        let secret = matches!(self.kind, FieldKind::Secret);
        self.text
            .get(..self.head.min(line.len()))
            .unwrap_or("")
            .chars()
            .map(|c| UnicodeWidthChar::width(if secret { '\u{2022}' } else { c }).unwrap_or(0))
            .sum()
    }

    /// How many cells the text is slid left, so that the caret is in a room `cols` across.
    fn scrolled(&self, cols: u16) -> usize {
        (self.caret_cells() + 1).saturating_sub(usize::from(cols))
    }

    /// The person pressed the cell `cell` of the room (from its start, `cols` across): the caret goes
    /// to the nearest edge of a character, with the selection's other end left where it is where
    /// `extend`.
    pub fn place(&mut self, cell: usize, cols: u16, extend: bool) {
        let target = cell + self.scrolled(cols);
        let secret = matches!(self.kind, FieldKind::Secret);
        let line: &str = self.text.lines().next().unwrap_or("");
        let mut x = 0_usize;
        let mut at = line.len();
        for (index, c) in line.char_indices() {
            let w = UnicodeWidthChar::width(if secret { '\u{2022}' } else { c }).unwrap_or(0);
            if target < x + w.div_ceil(2) {
                at = index;
                break;
            }
            x += w;
        }
        self.head = at;
        if !extend {
            self.anchor = at;
        }
    }

    /// The rows the text is broken into in a room `cols` cells across: each a range of bytes, and
    /// at least one (an empty text is one row of nothing).
    pub(crate) fn rows(&self, cols: u16) -> Vec<std::ops::Range<usize>> {
        rows_of(&self.text, cols)
    }

    /// The bytes `from` to `to` of the text, none where they are not a part of it.
    fn part(&self, from: usize, to: usize) -> &str {
        self.text.get(from..to).unwrap_or("")
    }

    /// The row of `rows` the byte `at` is in: the last that starts at or before it.
    fn row_at(rows: &[std::ops::Range<usize>], at: usize) -> usize {
        rows.iter().rposition(|row| row.start <= at).unwrap_or(0)
    }

    /// The first row of `rows` a room `shown` rows high shows: the one it showed, until the caret's
    /// row is not in the room, and then as far as keeps it in.
    fn first_row(&self, rows: &[std::ops::Range<usize>], shown: usize) -> usize {
        let shown = shown.max(1);
        let caret = Self::row_at(rows, self.head);
        let mut top = self.top.get();
        if caret < top {
            top = caret;
        } else if caret >= top + shown {
            top = caret + 1 - shown;
        }
        top = top.min(rows.len().saturating_sub(shown));
        self.top.set(top);
        top
    }

    /// What of a field of lines is in a room `cols` cells across and `shown` rows high, as it is
    /// drawn there: the rows from the first that keeps the caret in the room, each with the
    /// cells the selection covers in it, and the caret's row and cell among them; and the row on
    /// either side of them.
    pub(crate) fn block(&self, cols: u16, shown: usize) -> Block {
        let rows = self.rows(cols);
        self.cols.set(cols);
        self.shown.set(shown.max(1));
        let first = self.first_row(&rows, shown);
        let (from, to) = (self.anchor.min(self.head), self.anchor.max(self.head));
        let row_of = |row: &std::ops::Range<usize>| {
            let text = self.part(row.start, row.end);
            let (lo, hi) = (from.max(row.start), to.min(row.end));
            let selected = (lo < hi).then(|| {
                let before = cells_of(self.part(row.start, lo));
                before..before + cells_of(self.part(lo, hi))
            });
            Row {
                text: text.to_owned(),
                selected,
            }
        };
        let mut block = Block::default();
        for row in rows.iter().skip(first).take(shown.max(1)) {
            block.rows.push(row_of(row));
        }
        block.before = first
            .checked_sub(1)
            .and_then(|row| rows.get(row))
            .map(row_of);
        block.after = rows.get(first + shown.max(1)).map(row_of);
        let at = Self::row_at(&rows, self.head);
        let start = rows.get(at).map_or(0, |row| row.start);
        let cell = cells_of(self.text.get(start..self.head).unwrap_or(""));
        // A caret after the last cell of a full row is just past the box's edge.
        block.caret = (at.saturating_sub(first), cell.min(usize::from(cols)));
        block
    }

    /// The byte of the row `row` of `rows` that is nearest the cell `cell` of it: the edge of a
    /// character that the cell is nearer to, and the end of the row where it is past its characters
    /// (where the row was broken at a space, before the space: the end of the row is the start of the
    /// next, and the caret stays on this one).
    fn byte_in_row(&self, rows: &[std::ops::Range<usize>], row: usize, cell: usize) -> usize {
        let Some(range) = rows.get(row) else {
            return self.text.len();
        };
        let words = self.part(range.start, range.end);
        let broken_here = words.ends_with(' ')
            && rows
                .get(row + 1)
                .is_some_and(|next| next.start == range.end);
        let mut at = if broken_here {
            range.end - 1
        } else {
            range.end
        };
        let mut x = 0_usize;
        for (index, c) in words.char_indices() {
            let w = UnicodeWidthChar::width(c).unwrap_or(0);
            if cell < x + w.div_ceil(2) {
                at = range.start + index;
                break;
            }
            x += w;
        }
        at
    }

    /// The person pressed the cell `cell` of the row `line` (counted from the first shown, the row
    /// before it being -1) of a room `cols` cells across and `shown` rows high: the caret goes to the
    /// nearest edge of a character of that row, with the selection's other end left where it is where
    /// `extend`. No further than the rows either side of those shown, which the padding shows: a
    /// host that shows none says a row of those shown.
    pub fn place_in_rows(
        &mut self,
        cell: usize,
        line: isize,
        cols: u16,
        shown: usize,
        extend: bool,
    ) {
        let rows = self.rows(cols);
        let first = self.first_row(&rows, shown);
        let last = rows.len().saturating_sub(1);
        // The row before the first shown and the row after the last are what the padding shows.
        let row = first
            .checked_add_signed(line.max(-1))
            .unwrap_or(0)
            .min(first + shown.max(1))
            .min(last);
        let at = self.byte_in_row(&rows, row, cell);
        self.head = at;
        if !extend {
            self.anchor = at;
        }
    }

    /// Where the caret goes moving `by` rows up or down from `at`, to the same cell of the row where
    /// it has one: the start of the text from the first row, its end from the last, and where the field
    /// was not drawn as lines (no rows to go by) the same.
    fn vertical(&self, at: usize, by: usize, up: bool) -> usize {
        let cols = self.cols.get();
        if cols == 0 || !matches!(self.kind, FieldKind::Lines) {
            return if up { 0 } else { self.text.len() };
        }
        let rows = self.rows(cols);
        let row = Self::row_at(&rows, at);
        let start = rows.get(row).map_or(0, |range| range.start);
        let cell = cells_of(self.part(start, at));
        let target = if up {
            let Some(target) = row.checked_sub(by) else {
                return 0;
            };
            target
        } else {
            let target = row + by;
            if target >= rows.len() {
                return self.text.len();
            }
            target
        };
        self.byte_in_row(&rows, target, cell)
    }

    /// What of the field is in a room `cols` cells across, as it is drawn there in pixels: the
    /// characters that are in it (a secret as dots), the cells the selection covers and the cell the
    /// caret is in, counted from the start of the room.
    pub(crate) fn slice(&self, cols: u16) -> Slice {
        let secret = matches!(self.kind, FieldKind::Secret);
        let line: &str = self.text.lines().next().unwrap_or("");
        let (from, to) = (self.anchor.min(self.head), self.anchor.max(self.head));
        let skip = self.scrolled(cols);
        let room = usize::from(cols);
        let mut slice = Slice::default();
        let mut x = 0_usize;
        for (index, c) in line.char_indices() {
            let shown = if secret { '\u{2022}' } else { c };
            let w = UnicodeWidthChar::width(shown).unwrap_or(0);
            if w == 0 {
                continue;
            }
            if x >= skip && x - skip + w <= room {
                slice.text.push(shown);
                if index >= from && index < to {
                    let cells = x - skip..x - skip + w;
                    slice.selected = Some(match slice.selected.take() {
                        Some(chosen) => chosen.start..cells.end,
                        None => cells,
                    });
                }
            }
            x += w;
        }
        let caret = self.caret_cells();
        slice.caret = (caret - skip.min(caret)).min(room.saturating_sub(1));
        slice
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Mods = Mods {
        shift: false,
        control: false,
        alt: false,
        command: false,
    };

    fn typed(field: &mut TextField, text: &str) {
        for c in text.chars() {
            field.key(Key::Character(c), NONE, Some(&c.to_string()));
        }
    }

    #[test]
    fn typing_backspace_and_the_caret() {
        let mut field = TextField::new(FieldKind::Line);
        typed(&mut field, "heşlo");
        assert_eq!(field.text(), "heşlo");
        field.key(Key::Backspace, NONE, None);
        field.key(Key::Left, NONE, None);
        field.key(Key::Backspace, NONE, None);
        assert_eq!(field.text(), "hel");
        assert_eq!(field.state().selection, 2..2);
        field.key(Key::Home, NONE, None);
        typed(&mut field, "x");
        assert_eq!(field.text(), "xhel");
    }

    #[test]
    fn shift_selects_and_a_character_replaces_the_selection() {
        let mut field = TextField::new(FieldKind::Line);
        typed(&mut field, "abc def");
        let shift = Mods {
            shift: true,
            ..NONE
        };
        for _ in 0..3 {
            field.key(Key::Left, shift, None);
        }
        let selection = field.state().selection;
        // The caret is the end that moved: the selection is read from where it started.
        assert_eq!((selection.start, selection.end), (7, 4));
        typed(&mut field, "X");
        assert_eq!(field.text(), "abc X");
        let control = Mods {
            control: true,
            ..NONE
        };
        field.key(Key::Character('a'), control, None);
        let edited = field.key(Key::Character('c'), control, None);
        assert_eq!(edited.copy, Some(("abc X".to_owned(), false)));
        let edited = field.key(Key::Character('x'), control, None);
        assert!(edited.changed && field.text().is_empty());
    }

    #[test]
    fn undo_and_redo() {
        let mut field = TextField::new(FieldKind::Line);
        typed(&mut field, "ab");
        let control = Mods {
            control: true,
            ..NONE
        };
        field.key(Key::Character('z'), control, None);
        assert_eq!(field.text(), "a");
        field.key(Key::Character('y'), control, None);
        assert_eq!(field.text(), "ab");
    }

    #[test]
    fn a_text_is_broken_at_a_word_where_a_row_is_full_and_a_space_that_does_not_fit_hangs() {
        let mut field = TextField::new(FieldKind::Lines);
        field.fill(&Fill::text("aaa bbb ccc"));
        // "aaa bbb" is the seven cells of a row, and the space after it stays on it.
        assert_eq!(field.rows(7), [0..8, 8..11]);
        // A word that is longer than a row is broken where it must be, and a paragraph has its own rows.
        field.fill(&Fill::text("abcdefghij\nxy"));
        assert_eq!(field.rows(4), [0..4, 4..8, 8..10, 11..13]);
        field.fill(&Fill::text(""));
        assert_eq!(field.rows(4), std::iter::once(0..0).collect::<Vec<_>>());
    }

    #[test]
    fn the_caret_and_the_selection_of_lines_are_on_their_rows_and_a_press_goes_to_a_row() {
        let mut field = TextField::new(FieldKind::Lines);
        field.fill(&Fill::text("aaa bbb ccc").select(4..10));
        let block = field.block(7, 2);
        let words: Vec<&str> = block.rows.iter().map(|row| row.text.as_str()).collect();
        assert_eq!(words, ["aaa bbb ", "ccc"]);
        // "bbb " of the first row (cells 4 to 8) and "cc" of the second (0 to 2).
        assert_eq!(block.rows[0].selected, Some(4..8));
        assert_eq!(block.rows[1].selected, Some(0..2));
        assert_eq!(block.caret, (1, 2));
        // A room of one row shows the row the caret is on.
        let block = field.block(7, 1);
        assert_eq!(block.rows.len(), 1);
        assert_eq!(block.rows[0].text, "ccc");
        assert_eq!(block.caret, (0, 2));
        field.place_in_rows(2, 0, 7, 2, false);
        assert_eq!(field.state().selection, 2..2);
        // Past the end of a row that was broken at a space the caret stays on the row, before it.
        field.place_in_rows(20, 0, 7, 2, false);
        assert_eq!(field.state().selection, 7..7);
        // Under the last row is the end of the text.
        field.place_in_rows(20, 5, 7, 2, false);
        assert_eq!(field.state().selection, 11..11);
    }

    #[test]
    fn a_room_that_shows_less_than_the_text_keeps_its_first_row_until_the_caret_leaves_it() {
        let mut field = TextField::new(FieldKind::Lines);
        field.fill(&Fill::text("aaa\nbbb\nccc\nddd\neee"));
        // The caret is at the end: the last two rows are in a room of two.
        assert_eq!(field.block(5, 2).rows[0].text, "ddd");
        // A press in its first row puts the caret there and the rows stay where they were.
        field.place_in_rows(1, 0, 5, 2, false);
        assert_eq!(field.state().selection, 13..13);
        assert_eq!(field.block(5, 2).rows[0].text, "ddd");
        // The caret goes above the room: the room goes up with it, to as many rows as keep it in.
        field.key(Key::Up, NONE, None);
        assert_eq!(field.block(5, 2).rows[0].text, "ccc");
        // A press below the rows is on the row after the last, which the padding below them shows,
        // and on no row further down.
        field.place_in_rows(0, 9, 5, 2, false);
        assert_eq!(field.state().selection, 16..16);
    }

    #[test]
    fn the_row_before_and_the_row_after_the_rows_shown_are_given_for_the_padding_to_show() {
        let mut field = TextField::new(FieldKind::Lines);
        field.fill(&Fill::text("aaa\nbbb\nccc\nddd\neee").select(0..0));
        // At the top there is no row before.
        let block = field.block(5, 2);
        assert_eq!(block.before, None);
        assert_eq!(
            block.after.as_ref().map(|row| row.text.as_str()),
            Some("ccc")
        );
        // The caret in the third row: the room goes down to keep it, and rows stand either side.
        field.fill(&Fill::text("aaa\nbbb\nccc\nddd\neee").select(9..9));
        let block = field.block(5, 2);
        let words: Vec<&str> = block.rows.iter().map(|row| row.text.as_str()).collect();
        assert_eq!(words, ["bbb", "ccc"]);
        assert_eq!(
            block.before.as_ref().map(|row| row.text.as_str()),
            Some("aaa")
        );
        assert_eq!(
            block.after.as_ref().map(|row| row.text.as_str()),
            Some("ddd")
        );
        // A press on the row after is on it, and so is one on the row before.
        field.place_in_rows(1, 2, 5, 2, false);
        assert_eq!(field.state().selection, 13..13);
        field.fill(&Fill::text("aaa\nbbb\nccc\nddd\neee").select(9..9));
        field.block(5, 2);
        field.place_in_rows(1, -1, 5, 2, false);
        assert_eq!(field.state().selection, 1..1);
        // At the end there is no row after.
        field.fill(&Fill::text("aaa\nbbb\nccc\nddd\neee"));
        assert_eq!(field.block(5, 2).after, None);
    }

    #[test]
    fn a_caret_after_the_last_cell_of_a_full_row_is_at_the_edge_and_up_and_down_go_by_rows() {
        let mut field = TextField::new(FieldKind::Lines);
        field.fill(&Fill::text("abcd efgh"));
        // "abcd " and "efgh": the caret at the end is after the fourth cell of a row of four.
        assert_eq!(field.block(4, 2).caret, (1, 4));
        // Up goes to the same cell of the row above, and the start of the text from the first row.
        field.key(Key::Up, NONE, None);
        assert_eq!(field.state().selection, 4..4);
        field.key(Key::Up, NONE, None);
        assert_eq!(field.state().selection, 0..0);
        // Down from the first row goes to its cell in the second, and the end of the text from the last.
        field.place_in_rows(2, 0, 4, 2, false);
        field.key(Key::Down, NONE, None);
        assert_eq!(field.state().selection, 7..7);
        field.key(Key::Down, NONE, None);
        assert_eq!(field.state().selection, 9..9);
        // A page goes by as many rows as the room shows.
        field.fill(&Fill::text("a\nb\nc\nd\ne").select(0..0));
        field.block(4, 2);
        field.key(Key::PageDown, NONE, None);
        assert_eq!(field.state().selection, 4..4);
        // A field that was not drawn as rows has none to go by: its start and its end.
        let mut field = TextField::new(FieldKind::Lines);
        field.fill(&Fill::text("a\nb\nc").select(2..2));
        field.key(Key::Up, NONE, None);
        assert_eq!(field.state().selection, 0..0);
    }

    #[test]
    fn a_press_past_the_end_of_a_row_broken_inside_a_word_is_at_the_start_of_the_next() {
        let mut field = TextField::new(FieldKind::Lines);
        field.fill(&Fill::text("abcdefghij"));
        field.place_in_rows(9, 0, 4, 3, false);
        assert_eq!(field.state().selection, 4..4);
    }

    #[test]
    fn a_fill_puts_in_a_text_and_a_selection() {
        let mut field = TextField::new(FieldKind::Line);
        assert!(field.fill(&Fill::text("query").select(1..3)));
        assert_eq!(field.state().selection, 1..3);
        assert!(!field.fill(&Fill::selection(0..usize::MAX)));
        assert_eq!(field.state().selection, 0..5);
    }

    #[test]
    fn a_press_puts_the_caret_at_the_nearest_edge_and_drags_the_selection() {
        let mut field = TextField::new(FieldKind::Line);
        typed(&mut field, "abcdef");
        field.place(2, 10, false);
        assert_eq!(field.state().selection, 2..2);
        field.place(5, 10, true);
        assert_eq!(field.state().selection, 2..5);
        // Past the end of the text is its end.
        field.place(9, 10, false);
        assert_eq!(field.state().selection, 6..6);
        // A room that holds less than the text slides it: cell 0 is a character on.
        field.place(0, 3, false);
        assert_eq!(field.state().selection, 4..4);
    }

    #[test]
    fn a_paste_of_lines_in_one_line_is_one_line() {
        let mut field = TextField::new(FieldKind::Line);
        field.paste("a\nb");
        assert_eq!(field.text(), "a b");
    }

    #[test]
    fn words_are_travelled_by_the_word() {
        let mut field = TextField::new(FieldKind::Line);
        typed(&mut field, "one two");
        let alt = Mods { alt: true, ..NONE };
        field.key(Key::Left, alt, None);
        assert_eq!(field.state().selection, 4..4);
        field.key(Key::Backspace, alt, None);
        assert_eq!(field.text(), "two");
    }
}
