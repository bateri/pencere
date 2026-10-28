//! The other places of the main selection's text, which an editor marks where they are in view
//! (CodeMirror's `highlightSelectionMatches`, VS Code's selection highlight): only drawn, the
//! selection as it is.

use pencere_view::edit::{Matches, limits};

use crate::buffer::Buffer;
use crate::find::is_word;
use crate::segments::word_at;

impl Buffer {
    /// The other places of the main selection's text in `from..to` (the lines in view), in order,
    /// as the editor's `matches` option says: the text of a selection on one line, of at most
    /// [`limits::MATCH_LENGTH`] characters and not all spaces, or with [`Matches::Word`] the word a
    /// caret is in; a whole word only where it is a whole word, in the case written; at most
    /// [`limits::MATCHES_IN_VIEW`], the selection itself and what a fold hides not among them.
    pub fn matches_in(&self, from: usize, to: usize) -> Vec<(usize, usize)> {
        let mode = self.options.matches;
        if mode == Matches::None {
            return Vec::new();
        }
        let main = self.main();
        let (start, end) = if main.is_empty() {
            match (mode, word_at(&self.text, main.head)) {
                (Matches::Word, Some(word)) => word,
                _ => return Vec::new(),
            }
        } else {
            (main.start(), main.end())
        };
        if end - start > limits::MATCH_LENGTH || self.text.line_of(start) != self.text.line_of(end)
        {
            return Vec::new();
        }
        let Some(needle) = self.text.slice(start, end) else {
            return Vec::new();
        };
        if needle.trim().is_empty() {
            return Vec::new();
        }
        let outside = |at: Option<char>| at.is_none_or(|c| !is_word(c));
        let edge = |at: usize| {
            at.checked_sub(1)
                .and_then(|before| self.text.char_at(before))
        };
        let whole = outside(edge(start)) && outside(self.text.char_at(end));
        let Some(hay) = self.text.slice(from, to.min(self.text.len_chars())) else {
            return Vec::new();
        };
        let length = needle.chars().count();
        // What a fold hides is not in view: its places are not marked, nor counted.
        let hidden = self.hidden_lines();
        let shown = |at: usize| {
            let line = self.text.line_of(at);
            !hidden
                .iter()
                .any(|(first, last)| (*first..=*last).contains(&line))
        };
        let mut out = Vec::new();
        let (mut byte, mut place) = (0, from);
        for (at, _) in hay.match_indices(needle.as_str()) {
            place += hay.get(byte..at).map_or(0, |gap| gap.chars().count());
            byte = at;
            let found = (place, place + length);
            if found == (start, end) {
                continue;
            }
            if whole && !(outside(edge(found.0)) && outside(self.text.char_at(found.1))) {
                continue;
            }
            if !shown(found.0) {
                continue;
            }
            out.push(found);
            if out.len() >= limits::MATCHES_IN_VIEW {
                break;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use pencere_view::edit::{EditorOptions, Matches, Range};

    use crate::Buffer;

    fn marked(text: &str, mode: Matches, range: (usize, usize)) -> Vec<(usize, usize)> {
        let mut buffer = Buffer::new(text).expect("a document");
        buffer.set_options(EditorOptions {
            matches: mode,
            ..EditorOptions::default()
        });
        buffer.select(vec![Range::new(range.0, range.1)], 0);
        buffer.matches_in(0, text.chars().count())
    }

    #[test]
    fn a_whole_word_selected_marks_the_whole_words_and_a_part_marks_every_place() {
        let text = "let total = sum(totals) + total";
        assert_eq!(
            marked(text, Matches::Selection, (4, 9)),
            [(26, 31)],
            "the word, not inside `totals`"
        );
        assert_eq!(
            marked(text, Matches::Selection, (5, 9)),
            [(17, 21), (27, 31)],
            "a part of a word, wherever it is"
        );
        assert_eq!(marked(text, Matches::None, (4, 9)), []);
    }

    #[test]
    fn what_a_fold_hides_is_not_marked() {
        let mut buffer = Buffer::new("x\n  x\n  x\nx x").expect("a document");
        buffer.set_options(EditorOptions {
            matches: Matches::Selection,
            folds: pencere_view::edit::Folds::Indent,
            ..EditorOptions::default()
        });
        assert!(buffer.fold_at(0, Some(true)));
        buffer.select(vec![Range::new(10, 11)], 0);
        assert_eq!(buffer.matches_in(0, 13), [(0, 1), (12, 13)]);
    }

    #[test]
    fn a_caret_marks_its_word_only_where_the_page_asks_and_spaces_mark_nothing() {
        let text = "año dos año";
        assert_eq!(marked(text, Matches::Selection, (1, 1)), []);
        assert_eq!(marked(text, Matches::Word, (1, 1)), [(8, 11)]);
        assert_eq!(marked("a  b  c", Matches::Selection, (1, 3)), []);
        // Over a line, or longer than may be: nothing.
        assert_eq!(marked("ab\nab", Matches::Selection, (0, 3)), []);
        let long = "x".repeat(201);
        assert_eq!(
            marked(&format!("{long} {long}"), Matches::Selection, (0, 201)),
            []
        );
    }
}
