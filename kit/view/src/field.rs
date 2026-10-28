//! A text field of the host's: what a page puts in one, and which keys it keeps.
//!
//! A text field is the host's own, its system's, put over the room of an element that says it is
//! one ([`crate::Element::field`]). The page fills it ([`Fill`], in a reply) and is told what the
//! person types; the keys the field has no use for are the page's. Which those are is the kit's to
//! say and not each system's ([`FieldState::takes`]), so that a page is given the same keys on every
//! host: a list under a filter moves with the arrows, a dialog's field gives ⌫ to the page once it
//! is empty, a palette's takes ↑ and ↓ for its rows.

use std::ops::Range;

use crate::{FieldKind, Key, Mods};

/// What a reply puts in a field ([`crate::Reply::fill`]): a text in place of what it holds, what is
/// selected in it, or both — a field to rename a file in comes with the name and its stem
/// selected, one that holds a query to search again with the query selected, so that what is typed
/// takes its place.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fill {
    /// What the field holds from now on; `None` keeps what it holds.
    pub text: Option<String>,
    /// What is selected, in characters from 0 (code points, as an editor counts them): the caret
    /// alone where the range is empty. Its ends are taken in order, and one past the end of the
    /// text is its end, so that `0..usize::MAX` is all of it. `None`: where a text is put in, the
    /// caret goes where the host's system puts it (at the end); where none is, the selection stays.
    pub select: Option<Range<usize>>,
}

impl Fill {
    /// A text in place of what the field holds.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            select: None,
        }
    }

    /// What the field holds, kept, with `range` selected in it.
    pub fn selection(range: Range<usize>) -> Self {
        Self {
            text: None,
            select: Some(range),
        }
    }

    /// The same, with `range` selected (characters from 0; see [`Fill::select`]).
    #[must_use]
    pub fn select(mut self, range: Range<usize>) -> Self {
        self.select = Some(range);
        self
    }

    /// The same, with all of the text selected.
    #[must_use]
    pub fn select_all(self) -> Self {
        self.select(0..usize::MAX)
    }

    /// What is to be selected in a text of `length` characters: the ends in order and within it.
    pub fn selection_in(&self, length: usize) -> Option<Range<usize>> {
        self.select.as_ref().map(|range| {
            let (a, b) = (range.start.min(length), range.end.min(length));
            a.min(b)..a.max(b)
        })
    }
}

/// Where a host's field that has the keyboard is: what kind it is, how long its text is and what is
/// selected in it (the caret where nothing is), the length and the selection in the one unit the
/// host counts in (characters, or its system's UTF-16 units alike).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldState {
    pub kind: FieldKind,
    pub length: usize,
    pub selection: Range<usize>,
}

impl FieldState {
    /// Whether the field keeps `key`, pressed with `mods`, or it is the page's: told to it as a key
    /// (its command first, where one has those keys), the field leaving its text and its selection
    /// as they are. A field keeps what it has a use for where it is, and no more:
    ///
    /// - escape, tab and the function keys are the page's;
    /// - in one line (`Line`, `Secret`), enter, up, down, page up and page down are the page's
    ///   whatever is held with them; in lines, enter is the field's without command or control,
    ///   and up and page up, down and page down are as left and right are below;
    /// - a character is the field's but with command or control, where it is one of the field's
    ///   own chords: select all (`a`), paste (`v`), undo and redo (`z`, `y`), and copy and cut
    ///   (`c`, `x`) where something is selected in a field whose text may be copied; every other
    ///   chord is the page's (⌘K over a filter opens the page's palette);
    /// - a key that moves the caret or deletes is the field's where it does something, and the
    ///   page's where nothing is selected and the caret is at the end it goes toward: backspace,
    ///   left and home at the start (an empty field gives backspace to the page), delete, right and
    ///   end at the end, whatever is held with them.
    ///
    /// A host whose system gives its fields chords besides these (a Mac's ⌃A and ⌃E) may keep
    /// those as well.
    pub fn takes(&self, key: Key, mods: Mods) -> bool {
        let chord = mods.command || mods.control;
        let one_line = !matches!(self.kind, FieldKind::Lines);
        let (from, to) = (
            self.selection.start.min(self.selection.end),
            self.selection.start.max(self.selection.end),
        );
        let caret = from == to;
        let at_start = caret && from == 0;
        let at_end = caret && to >= self.length;
        match key {
            Key::Escape | Key::Tab | Key::Function(_) => false,
            Key::Enter => !one_line && !chord,
            Key::Up | Key::PageUp => !one_line && !at_start,
            Key::Down | Key::PageDown => !one_line && !at_end,
            Key::Left | Key::Home | Key::Backspace => !at_start,
            Key::Right | Key::End | Key::Delete => !at_end,
            Key::Character(_) if !chord => true,
            Key::Character(typed) => match typed.to_ascii_lowercase() {
                'a' | 'v' | 'z' | 'y' => true,
                'c' | 'x' => !caret && !matches!(self.kind, FieldKind::Secret),
                _ => false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(kind: FieldKind, length: usize, selection: Range<usize>) -> FieldState {
        FieldState {
            kind,
            length,
            selection,
        }
    }

    const NONE: Mods = Mods {
        shift: false,
        control: false,
        alt: false,
        command: false,
    };
    const CMD: Mods = Mods {
        command: true,
        ..NONE
    };

    #[test]
    fn an_empty_field_gives_the_page_backspace_and_the_arrows_and_types_what_is_typed() {
        let empty = field(FieldKind::Line, 0, 0..0);
        for key in [
            Key::Backspace,
            Key::Delete,
            Key::Left,
            Key::Right,
            Key::Home,
            Key::End,
        ] {
            assert!(!empty.takes(key, NONE), "{key:?}");
        }
        assert!(empty.takes(Key::Character('a'), NONE));
        assert!(empty.takes(Key::Character('v'), CMD));
    }

    #[test]
    fn a_key_that_moves_or_deletes_is_the_fields_where_it_does_something() {
        let start = field(FieldKind::Line, 4, 0..0);
        assert!(!start.takes(Key::Backspace, NONE));
        assert!(!start.takes(Key::Left, CMD));
        assert!(start.takes(Key::Right, NONE));
        assert!(start.takes(Key::Delete, NONE));
        let end = field(FieldKind::Line, 4, 4..4);
        assert!(end.takes(Key::Backspace, NONE));
        assert!(!end.takes(Key::Right, NONE));
        assert!(!end.takes(Key::End, NONE));
        assert!(!end.takes(Key::Delete, NONE));
        // What is selected is taken away or let go of: the field's, at either end.
        let all = field(FieldKind::Line, 4, Range { start: 4, end: 0 });
        for key in [Key::Backspace, Key::Delete, Key::Left, Key::Right] {
            assert!(all.takes(key, NONE), "{key:?}");
        }
    }

    #[test]
    fn one_line_gives_the_page_enter_and_the_keys_that_go_up_and_down() {
        let line = field(FieldKind::Line, 4, 2..2);
        for key in [Key::Enter, Key::Up, Key::Down, Key::PageUp, Key::PageDown] {
            assert!(!line.takes(key, NONE), "{key:?}");
        }
        let lines = field(FieldKind::Lines, 9, 4..4);
        assert!(lines.takes(Key::Enter, NONE));
        assert!(!lines.takes(Key::Enter, CMD));
        assert!(lines.takes(Key::Up, NONE) && lines.takes(Key::Down, NONE));
        assert!(!field(FieldKind::Lines, 9, 0..0).takes(Key::Up, NONE));
        assert!(!field(FieldKind::Lines, 9, 9..9).takes(Key::PageDown, NONE));
        for kind in [FieldKind::Line, FieldKind::Lines] {
            for key in [Key::Escape, Key::Tab, Key::Function(5)] {
                assert!(!field(kind, 9, 4..4).takes(key, NONE), "{key:?}");
            }
        }
    }

    #[test]
    fn a_chord_is_the_fields_only_where_it_edits_the_field() {
        let some = field(FieldKind::Line, 4, 1..3);
        let none = field(FieldKind::Line, 4, 2..2);
        assert!(some.takes(Key::Character('c'), CMD));
        assert!(!none.takes(Key::Character('c'), CMD));
        assert!(!none.takes(Key::Character('x'), CMD));
        assert!(!field(FieldKind::Secret, 4, 1..3).takes(Key::Character('c'), CMD));
        assert!(none.takes(Key::Character('A'), CMD));
        assert!(!none.takes(Key::Character('k'), CMD));
        let control = Mods {
            control: true,
            ..NONE
        };
        assert!(!none.takes(Key::Character('p'), control));
        let alt = Mods { alt: true, ..NONE };
        assert!(none.takes(Key::Character('ø'), alt));
    }

    #[test]
    fn a_selection_is_held_to_the_text_with_its_ends_in_order() {
        assert_eq!(
            Fill::text("draft.md").select(0..5).selection_in(8),
            Some(0..5)
        );
        assert_eq!(Fill::text("abc").select_all().selection_in(3), Some(0..3));
        let reversed = Range { start: 7, end: 2 };
        assert_eq!(Fill::selection(reversed).selection_in(4), Some(2..4));
        assert_eq!(Fill::text("x").selection_in(1), None);
    }
}
