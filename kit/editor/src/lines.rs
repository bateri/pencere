//! The commands of whole lines and of the characters by a caret that an editor's standard keys do
//! in CodeMirror and VS Code, besides those of `commands`: folding all, deleting, selecting,
//! copying up and joining lines, a blank line or a break that leaves the caret, swapping two
//! characters, and deleting to an end of the line. None has a key of its own: a page asks for them
//! (its commands, by the keys it gives them).

use std::collections::BTreeSet;

use pencere_view::edit::{Change, Origin, Range};

use crate::buffer::Buffer;
use crate::commands::Part;
use crate::history::Kind;
use crate::segments::{grapheme_after, grapheme_before};

impl Buffer {
    /// Every region that folds and is inside none that does, folded, from the top down.
    pub(crate) fn fold_all(&mut self) {
        let count = self.text.len_lines();
        let mut line = 0;
        while line < count {
            match self.fold_region(line) {
                Some((_, to)) => {
                    self.fold_at(line, Some(true));
                    line = self.text.line_of(to).max(line) + 1;
                }
                None => line += 1,
            }
        }
    }

    /// The lines of each range taken out with their breaks; each caret on the line that comes in
    /// their place (the one after, or before them at the end), as far along it as it was.
    pub(crate) fn delete_lines(&mut self, now: f64) {
        if !self.writable() {
            return;
        }
        let blocks = self.lines();
        let count = self.text.len_lines();
        let len = self.text.len_chars();
        let main_line = self.text.line_of(self.main().head);
        let mut changes = Vec::with_capacity(blocks.len());
        let mut ranges = Vec::with_capacity(blocks.len());
        let mut main = 0;
        let mut removed = 0;
        for (index, (first, last)) in blocks.into_iter().enumerate() {
            let column = self
                .ranges
                .iter()
                .map(|range| range.head)
                .find(|at| (first..=last).contains(&self.text.line_of(*at)))
                .map_or(0, |at| at - self.text.line_start(self.text.line_of(at)));
            let width = |line: usize| self.text.line_end(line) - self.text.line_start(line);
            let (from, to, caret) = if last + 1 < count {
                let from = self.text.line_start(first);
                (
                    from,
                    self.text.line_start(last + 1),
                    from + column.min(width(last + 1)),
                )
            } else if first > 0 {
                let before = first - 1;
                let caret = self.text.line_start(before) + column.min(width(before));
                (self.text.line_end(before), len, caret)
            } else {
                (0, len, 0)
            };
            if (first..=last).contains(&main_line) {
                main = index;
            }
            ranges.push(Range::caret(caret - removed));
            removed += to - from;
            changes.push(Change::new(from, to, ""));
        }
        self.undo.close();
        let _ = self.edit(
            changes,
            Origin::Person,
            Kind::Other,
            now,
            Some((ranges, main)),
        );
    }

    /// The lines of each range selected whole, with the break after them; a range that is whole
    /// lines already takes the next one too.
    pub(crate) fn select_line(&mut self) {
        let count = self.text.len_lines();
        let len = self.text.len_chars();
        let ranges = self
            .ranges
            .iter()
            .map(|range| {
                let (start, end) = (range.start(), range.end());
                let first = self.text.line_of(start);
                let mut last = self.text.line_of(end);
                let ends_a_line = end == len || end == self.text.line_start(last);
                if !range.is_empty() && last > first && self.text.line_start(last) == end {
                    last -= 1;
                }
                if !range.is_empty() && start == self.text.line_start(first) && ends_a_line {
                    last += 1;
                }
                let last = last.min(count.saturating_sub(1));
                let to = if last + 1 < count {
                    self.text.line_start(last + 1)
                } else {
                    len
                };
                Range::new(self.text.line_start(first), to)
            })
            .collect();
        let main = self.main;
        self.select(ranges, main);
    }

    /// The lines of each range once more above them, the ranges on the copy.
    pub(crate) fn duplicate_lines_up(&mut self, now: f64) {
        if !self.writable() {
            return;
        }
        let blocks = self.lines();
        let mut changes = Vec::with_capacity(blocks.len());
        // Where each block starts, and what was put in before it.
        let mut shifts: Vec<(usize, usize, usize)> = Vec::with_capacity(blocks.len());
        let mut gained = 0;
        for (first, last) in &blocks {
            let from = self.text.line_start(*first);
            let to = self.text.line_end(*last);
            let block = self.text.slice(from, to).unwrap_or_default();
            shifts.push((from, to, gained));
            gained += block.chars().count() + 1;
            changes.push(Change::new(from, from, format!("{block}\n")));
        }
        let shift = |at: usize| {
            shifts
                .iter()
                .rev()
                .find(|(from, _, _)| at >= *from)
                .map_or(at, |(_, _, by)| at + by)
        };
        let ranges = self
            .ranges
            .iter()
            .map(|range| Range::new(shift(range.anchor), shift(range.head)))
            .collect();
        let main = self.main;
        self.undo.close();
        let _ = self.edit(
            changes,
            Origin::Person,
            Kind::Other,
            now,
            Some((ranges, main)),
        );
    }

    /// Each line of a range joined with the one after it (a caret's line with the next): the later
    /// line's indent taken away and a space between them, where neither ends in one or is empty.
    /// The caret where the last two of each met.
    pub(crate) fn join_lines(&mut self, now: f64) {
        if !self.writable() {
            return;
        }
        let count = self.text.len_lines();
        let main_line = self.text.line_of(self.main().head);
        let mut changes = Vec::new();
        let mut ranges = Vec::new();
        let mut main = 0;
        let mut gained: isize = 0;
        for (first, last) in self.lines() {
            let last = if last == first { first + 1 } else { last };
            if last >= count {
                // The last line, with nothing after it to join: its carets stay where they are.
                for range in &self.ranges {
                    if self.text.line_of(range.head) >= first {
                        if range.head == self.main().head {
                            main = ranges.len();
                        }
                        let moved = |at: usize| usize::try_from(at as isize + gained).unwrap_or(0);
                        ranges.push(Range::new(moved(range.anchor), moved(range.head)));
                    }
                }
                continue;
            }
            let mut caret = 0;
            for line in first..last {
                let from = self.text.line_end(line);
                let next = self.text.line(line + 1);
                let indent = next.chars().take_while(|c| *c == ' ' || *c == '\t').count();
                let ends_in_space = self
                    .text
                    .line(line)
                    .chars()
                    .last()
                    .is_none_or(char::is_whitespace);
                let space = if ends_in_space || next.trim().is_empty() {
                    ""
                } else {
                    " "
                };
                let to = self.text.line_start(line + 1) + indent;
                caret = usize::try_from(from as isize + gained).unwrap_or(0);
                gained += space.len() as isize - (to - from) as isize;
                changes.push(Change::new(from, to, space));
            }
            if (first..=last).contains(&main_line) {
                main = ranges.len();
            }
            ranges.push(Range::caret(caret));
        }
        if changes.is_empty() {
            return;
        }
        self.undo.close();
        let _ = self.edit(
            changes,
            Origin::Person,
            Kind::Other,
            now,
            Some((ranges, main)),
        );
    }

    /// A line after the line of each caret, indented as a line break at its end would indent it,
    /// the caret on it.
    pub(crate) fn insert_blank_line(&mut self, now: f64) {
        if !self.writable() {
            return;
        }
        let unit = self.unit();
        let lines: BTreeSet<usize> = self
            .ranges
            .iter()
            .map(|range| self.text.line_of(range.head))
            .collect();
        // One caret for each line, the main one on the main caret's line.
        let main_line = self.text.line_of(self.main().head);
        let main = lines
            .iter()
            .position(|line| *line == main_line)
            .unwrap_or(0);
        let parts = lines
            .into_iter()
            .map(|line| {
                let text = self.text.line(line);
                let indent: String = text
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect();
                let opener = text.trim_end().chars().last().is_some_and(|last| {
                    self.options
                        .pairs
                        .iter()
                        .any(|(open, close)| *open == last && open != close)
                });
                let at = self.text.line_end(line);
                if opener || self.opens(&text) {
                    Part::put(at, at, format!("\n{indent}{unit}"))
                } else {
                    Part::put(at, at, format!("\n{indent}"))
                }
            })
            .collect();
        self.undo.close();
        self.replace_parts_main(parts, Kind::Other, now, main);
        self.undo.close();
    }

    /// A line break in place of what each range selects, the caret staying before it.
    pub(crate) fn split_line(&mut self, now: f64) {
        if !self.writable() {
            return;
        }
        let parts = self
            .ranges
            .iter()
            .map(|range| Part {
                from: range.start(),
                to: range.end(),
                insert: "\n".to_owned(),
                select: (0, 0),
            })
            .collect();
        self.undo.close();
        self.replace_parts(parts, Kind::Other, now);
        self.undo.close();
    }

    /// The characters on either side of each caret swapped and the caret after them; at the end of
    /// a line, the two before it. A caret at the start of a line, and a range that selects
    /// something, stay as they are.
    pub(crate) fn transpose_chars(&mut self, now: f64) {
        if !self.writable() {
            return;
        }
        let parts = self
            .ranges
            .iter()
            .map(|range| {
                let still = Part {
                    from: range.start(),
                    to: range.start(),
                    insert: String::new(),
                    select: (range.anchor - range.start(), range.head - range.start()),
                };
                if !range.is_empty() {
                    return still;
                }
                let at = range.head;
                let line = self.text.line_of(at);
                let (start, end) = (self.text.line_start(line), self.text.line_end(line));
                if at == start {
                    return still;
                }
                let (from, middle, to) = if at == end {
                    let middle = grapheme_before(&self.text, at);
                    if middle == start {
                        return still;
                    }
                    (grapheme_before(&self.text, middle), middle, at)
                } else {
                    (
                        grapheme_before(&self.text, at),
                        at,
                        grapheme_after(&self.text, at),
                    )
                };
                let first = self.text.slice(from, middle).unwrap_or_default();
                let second = self.text.slice(middle, to).unwrap_or_default();
                Part::put(from, to, format!("{second}{first}"))
            })
            .collect();
        self.undo.close();
        self.replace_parts(parts, Kind::Other, now);
    }
}

#[cfg(test)]
mod tests {
    use pencere_view::edit::{EditCommand, EditorOptions, Folds, Range};

    use crate::Buffer;

    fn buffer(text: &str, ranges: &[(usize, usize)]) -> Buffer {
        let mut buffer = Buffer::new(text).expect("a document");
        buffer.select(ranges.iter().map(|(a, h)| Range::new(*a, *h)).collect(), 0);
        buffer
    }

    fn after(text: &str, ranges: &[(usize, usize)], command: EditCommand) -> (String, Vec<Range>) {
        let mut buffer = buffer(text, ranges);
        buffer.command(command, 0.0);
        (buffer.text(), buffer.ranges().to_vec())
    }

    #[test]
    fn delete_lines_takes_the_lines_and_puts_the_caret_on_the_next_as_far_along() {
        let (text, ranges) = after("one\ntwo\nthree", &[(5, 5)], EditCommand::DeleteLines);
        assert_eq!(text, "one\nthree");
        assert_eq!(ranges, [Range::caret(5)]);
        // The last line: the caret goes to the one before it.
        let (text, ranges) = after("one\ntwo", &[(6, 6)], EditCommand::DeleteLines);
        assert_eq!(text, "one");
        assert_eq!(ranges, [Range::caret(2)]);
        // A selection that ends at the start of a line does not take that line; two blocks.
        let (text, _) = after("a\nb\nc\nd\ne", &[(0, 2), (6, 6)], EditCommand::DeleteLines);
        assert_eq!(text, "b\nc\ne");
        let (text, ranges) = after("only", &[(2, 2)], EditCommand::DeleteLines);
        assert_eq!((text.as_str(), ranges), ("", vec![Range::caret(0)]));
    }

    #[test]
    fn select_line_selects_the_lines_and_takes_the_next_when_they_are_whole() {
        let mut buffer = buffer("one\ntwo\nthree", &[(1, 1)]);
        buffer.command(EditCommand::SelectLine, 0.0);
        assert_eq!(buffer.ranges(), [Range::new(0, 4)]);
        buffer.command(EditCommand::SelectLine, 0.0);
        assert_eq!(buffer.ranges(), [Range::new(0, 8)]);
        buffer.command(EditCommand::SelectLine, 0.0);
        buffer.command(EditCommand::SelectLine, 0.0);
        assert_eq!(buffer.ranges(), [Range::new(0, 13)]);
    }

    #[test]
    fn duplicate_lines_up_copies_above_and_keeps_the_ranges_on_the_copy() {
        let (text, ranges) = after("a\nbc\nd", &[(3, 3)], EditCommand::DuplicateLinesUp);
        assert_eq!(text, "a\nbc\nbc\nd");
        assert_eq!(ranges, [Range::caret(3)]);
        // Lines next to each other are one block, as for the other commands of lines.
        let (text, ranges) = after("x\ny", &[(0, 0), (2, 2)], EditCommand::DuplicateLinesUp);
        assert_eq!(text, "x\ny\nx\ny");
        assert_eq!(ranges, [Range::caret(0), Range::caret(2)]);
        let (text, ranges) = after("x\n-\ny", &[(0, 0), (4, 4)], EditCommand::DuplicateLinesUp);
        assert_eq!(text, "x\nx\n-\ny\ny");
        assert_eq!(ranges, [Range::caret(0), Range::caret(6)]);
    }

    #[test]
    fn join_lines_takes_the_indent_away_and_puts_one_space() {
        let (text, ranges) = after("if x {\n    y\n}", &[(2, 2)], EditCommand::JoinLines);
        assert_eq!(text, "if x { y\n}");
        assert_eq!(ranges, [Range::caret(6)]);
        let (text, _) = after("a \n  b\n\nc", &[(0, 6)], EditCommand::JoinLines);
        assert_eq!(text, "a b\n\nc");
        let (text, _) = after("a\n\nc", &[(0, 0)], EditCommand::JoinLines);
        assert_eq!(text, "a\nc");
        let (text, _) = after("last", &[(1, 1)], EditCommand::JoinLines);
        assert_eq!(text, "last");
    }

    #[test]
    fn a_blank_line_and_a_split_leave_the_caret_where_each_says() {
        let mut buffer = Buffer::new("  f() {\n  }").expect("a document");
        buffer.set_options(EditorOptions {
            pairs: vec![('{', '}')],
            ..EditorOptions::default()
        });
        buffer.select(vec![Range::caret(3)], 0);
        buffer.command(EditCommand::InsertBlankLine, 0.0);
        assert_eq!(buffer.text(), "  f() {\n      \n  }");
        assert_eq!(buffer.ranges(), [Range::caret(14)]);
        let (text, ranges) = after("ab", &[(1, 1)], EditCommand::SplitLine);
        assert_eq!((text.as_str(), ranges), ("a\nb", vec![Range::caret(1)]));
    }

    #[test]
    fn the_main_caret_stays_the_main_one_where_carets_share_a_line_or_one_has_nothing_to_join() {
        let mut blank = buffer("a\nb\nc", &[(0, 0), (1, 1), (4, 4)]);
        blank.select(vec![Range::caret(0), Range::caret(1), Range::caret(4)], 2);
        blank.command(EditCommand::InsertBlankLine, 0.0);
        assert_eq!(blank.text(), "a\n\nb\nc\n");
        assert_eq!(blank.main(), Range::caret(7));
        let mut join = buffer("a\n b\nc", &[(0, 0), (6, 6)]);
        join.select(vec![Range::caret(0), Range::caret(6)], 1);
        join.command(EditCommand::JoinLines, 0.0);
        assert_eq!(join.text(), "a b\nc");
        assert_eq!(join.ranges(), [Range::caret(1), Range::caret(5)]);
        assert_eq!(join.main(), Range::caret(5));
    }

    #[test]
    fn transpose_swaps_the_characters_by_the_caret_or_the_two_before_the_end() {
        let (text, ranges) = after("abc", &[(1, 1)], EditCommand::TransposeChars);
        assert_eq!((text.as_str(), ranges), ("bac", vec![Range::caret(2)]));
        let (text, ranges) = after("abc\nd", &[(3, 3)], EditCommand::TransposeChars);
        assert_eq!((text.as_str(), ranges), ("acb\nd", vec![Range::caret(3)]));
        let (text, _) = after("ab\ncd", &[(3, 3)], EditCommand::TransposeChars);
        assert_eq!(text, "ab\ncd");
    }

    #[test]
    fn delete_to_an_end_of_the_line_takes_the_break_where_the_caret_is_at_it() {
        let (text, _) = after("abc\ndef", &[(1, 1)], EditCommand::DeleteToLineEnd);
        assert_eq!(text, "a\ndef");
        let (text, _) = after("abc\ndef", &[(3, 3)], EditCommand::DeleteToLineEnd);
        assert_eq!(text, "abcdef");
        let (text, ranges) = after("abc\ndef", &[(6, 6)], EditCommand::DeleteToLineStart);
        assert_eq!((text.as_str(), ranges), ("abc\nf", vec![Range::caret(4)]));
        let (text, _) = after("abc\ndef", &[(4, 4)], EditCommand::DeleteToLineStart);
        assert_eq!(text, "abcdef");
    }

    #[test]
    fn fold_all_folds_the_outermost_regions() {
        let mut buffer = Buffer::new("a\n  b\n    c\n  d\ne\n  f").expect("a document");
        buffer.set_options(EditorOptions {
            folds: Folds::Indent,
            ..EditorOptions::default()
        });
        buffer.command(EditCommand::FoldAll, 0.0);
        let folded: Vec<usize> = buffer
            .folded()
            .iter()
            .map(|(from, _)| buffer.line_of(*from))
            .collect();
        assert_eq!(folded, [0, 4]);
        buffer.command(EditCommand::UnfoldAll, 0.0);
        assert!(buffer.folded().is_empty());
    }
}
