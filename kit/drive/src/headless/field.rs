//! The headless host's text fields: a stand-in for a system's, with a text, a caret and what is
//! selected, and the kit's rule of which keys a field keeps ([`pencere_view::FieldState::takes`]).
//!
//! It edits as a field of one line does, a character at a time: what is typed takes the place of
//! what is selected, backspace and delete take a character (or what is selected), the arrows move
//! the caret a character (shift selects), home and end and, in lines, the keys that go up and down
//! go to an end, ⌘A selects all and ⌘C, ⌘X and ⌘V use the host's pasteboard. Places are characters.

use std::ops::Range;

use pencere_view::{FieldKind, FieldState, Fill, Key, Mods};

/// What one field holds: its text, and where its selection starts and where its caret is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Held {
    pub(super) text: String,
    anchor: usize,
    head: usize,
}

/// What a key did in a field.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Done {
    /// The field has no use for it: it is the page's.
    Passed,
    /// It moved the caret or the selection.
    Moved,
    /// It changed the text: the page is told what the field holds.
    Changed,
}

impl Held {
    /// A field that holds `text`, the caret at its end.
    pub(super) fn holding(text: &str) -> Self {
        let end = text.chars().count();
        Self {
            text: text.to_owned(),
            anchor: end,
            head: end,
        }
    }

    fn length(&self) -> usize {
        self.text.chars().count()
    }

    /// What is selected, in order.
    pub(super) fn selection(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    /// What a page's reply puts in the field.
    pub(super) fn fill(&mut self, fill: &Fill) {
        if let Some(text) = &fill.text {
            *self = Self::holding(text);
        }
        if let Some(range) = fill.selection_in(self.length()) {
            self.anchor = range.start;
            self.head = range.end;
        }
    }

    /// `text` in place of what is selected, the caret after it.
    pub(super) fn insert(&mut self, text: &str) {
        let range = self.selection();
        let start = byte_at(&self.text, range.start);
        let end = byte_at(&self.text, range.end);
        self.text.replace_range(start..end, text);
        self.anchor = range.start + text.chars().count();
        self.head = self.anchor;
    }

    fn selected(&self) -> String {
        let range = self.selection();
        self.text
            .chars()
            .skip(range.start)
            .take(range.end - range.start)
            .collect()
    }

    fn caret_to(&mut self, at: usize, extend: bool) {
        self.head = at;
        if !extend {
            self.anchor = at;
        }
    }

    /// The person presses `key` with `mods` in the field, a field of `kind`; `pasteboard` is the
    /// host's.
    pub(super) fn key(
        &mut self,
        kind: FieldKind,
        key: Key,
        mods: Mods,
        pasteboard: &mut Option<String>,
    ) -> Done {
        let state = FieldState {
            kind,
            length: self.length(),
            selection: self.selection(),
        };
        if !state.takes(key, mods) {
            return Done::Passed;
        }
        let range = self.selection();
        let caret = range.is_empty();
        let end = self.length();
        let extend = mods.shift;
        match key {
            Key::Character(typed) if !(mods.command || mods.control) => {
                self.insert(&typed.to_string());
                Done::Changed
            }
            Key::Character(chord) => match chord.to_ascii_lowercase() {
                'a' => {
                    self.anchor = 0;
                    self.head = end;
                    Done::Moved
                }
                'c' => {
                    *pasteboard = Some(self.selected());
                    Done::Moved
                }
                'x' => {
                    *pasteboard = Some(self.selected());
                    self.insert("");
                    Done::Changed
                }
                'v' => {
                    let text = pasteboard.clone().unwrap_or_default();
                    self.insert(&text);
                    Done::Changed
                }
                // Undo and redo: the stand-in keeps no history.
                _ => Done::Moved,
            },
            Key::Enter => {
                self.insert("\n");
                Done::Changed
            }
            Key::Backspace | Key::Delete => {
                if caret {
                    let at = range.start;
                    if key == Key::Backspace {
                        self.anchor = at - 1;
                    } else {
                        self.head = at + 1;
                    }
                }
                self.insert("");
                Done::Changed
            }
            Key::Left if !extend && !caret => {
                self.caret_to(range.start, false);
                Done::Moved
            }
            Key::Right if !extend && !caret => {
                self.caret_to(range.end, false);
                Done::Moved
            }
            Key::Left => {
                self.caret_to(self.head.saturating_sub(1), extend);
                Done::Moved
            }
            Key::Right => {
                self.caret_to((self.head + 1).min(end), extend);
                Done::Moved
            }
            Key::Home | Key::Up | Key::PageUp => {
                self.caret_to(0, extend);
                Done::Moved
            }
            Key::End | Key::Down | Key::PageDown => {
                self.caret_to(end, extend);
                Done::Moved
            }
            Key::Escape | Key::Tab | Key::Function(_) => Done::Passed,
        }
    }
}

/// Where the `at`th character of `text` starts, bytes.
fn byte_at(text: &str, at: usize) -> usize {
    text.char_indices()
        .nth(at)
        .map_or(text.len(), |(byte, _)| byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(held: &mut Held, key: Key, mods: Mods) -> Done {
        held.key(FieldKind::Line, key, mods, &mut None)
    }

    #[test]
    fn a_field_types_over_its_selection_and_gives_the_page_what_it_cannot_use() {
        let mut held = Held::holding("draft.md");
        held.fill(&Fill::selection(0..5));
        assert_eq!(held.selection(), 0..5);
        assert_eq!(
            key(&mut held, Key::Character('n'), Mods::default()),
            Done::Changed
        );
        assert_eq!(held.text, "n.md");
        assert_eq!(held.selection(), 1..1);
        let shift = Mods {
            shift: true,
            ..Mods::default()
        };
        assert_eq!(key(&mut held, Key::Left, shift), Done::Moved);
        assert_eq!(
            key(&mut held, Key::Backspace, Mods::default()),
            Done::Changed
        );
        assert_eq!(held.text, ".md");
        assert_eq!(
            key(&mut held, Key::Backspace, Mods::default()),
            Done::Passed
        );
        assert_eq!(key(&mut held, Key::Left, Mods::default()), Done::Passed);
        assert_eq!(key(&mut held, Key::Delete, Mods::default()), Done::Changed);
        assert_eq!(held.text, "md");
    }

    #[test]
    fn a_fill_with_no_text_keeps_the_text_and_one_with_no_selection_puts_the_caret_at_its_end() {
        let mut held = Held::holding("é ü");
        held.fill(&Fill::selection(Range { start: 9, end: 1 }));
        assert_eq!(held.selection(), 1..3);
        held.fill(&Fill::text("ab"));
        assert_eq!((held.text.as_str(), held.selection()), ("ab", 2..2));
        held.insert("ç");
        assert_eq!(held.text, "abç");
    }
}
