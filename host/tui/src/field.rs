//! A text field of the host's: what the person has typed in it, in the cells of the room it is drawn in.
//!
//! The page puts a field over the room of an element, fills it (a text, what is selected) and is told
//! what is typed. Which keys the field keeps and which are the page's is the kit's to say
//! ([`FieldState::takes`]); this keeps the text, the caret and the selection and edits them (the
//! drawing is `chrome`'s).

use pencere_view::{FieldKind, FieldState, Fill, Key, Mods};
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
            Key::Home | Key::Up | Key::PageUp => self.travel(mods, |_, _| 0),
            Key::End | Key::Down | Key::PageDown => self.travel(mods, |me, _| me.text.len()),
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
