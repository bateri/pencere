//! The editing commands: what a key of the person's does (or a page's, for an editor of modes),
//! made of changes the editor makes as the person's, and moves of the carets.

use pencere_view::edit::{Change, EditCommand, Movement, Origin, Range};

use crate::buffer::{Buffer, Columns, Placement};
use crate::find::{FIND_MATCHES, Query, Search};
use crate::history::Kind;
use crate::segments::{
    column_of, grapheme_after, grapheme_before, run_at, word_after, word_at, word_before,
};

/// How far a matching bracket is looked for, characters.
const BRACKET_REACH: usize = 100_000;

/// How far past a range its block comment's tokens are looked for, characters, past spaces.
const COMMENT_REACH: usize = 50;

/// A range in a block comment: what is taken out to take it out of it, the token that opens the
/// comment and the one that closes it (each with a space beside it, where there is one).
struct Wrapped {
    open: (usize, usize),
    close: (usize, usize),
}

/// What one range becomes: `insert` in place of `from..to`, and the range selected after it, as
/// places into `insert` (anchor, head).
pub(crate) struct Part {
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) insert: String,
    pub(crate) select: (usize, usize),
}

impl Part {
    /// `insert` in place of the range, the caret after it.
    pub(crate) fn put(from: usize, to: usize, insert: String) -> Self {
        let len = insert.chars().count();
        Self {
            from,
            to,
            insert,
            select: (len, len),
        }
    }
}

impl Buffer {
    /// Does `command`, rows of the text being lines.
    pub fn command(&mut self, command: EditCommand, now: f64) {
        self.command_in(command, now, &Columns);
    }

    /// Does `command`, rows of the text being those of `place`.
    pub fn command_in(&mut self, command: EditCommand, now: f64, place: &dyn Placement) {
        match command {
            EditCommand::Move { movement, extend } => self.move_by(movement, extend, place),
            EditCommand::Insert(text) => self.type_text(&text, now),
            EditCommand::Newline => self.newline(now),
            EditCommand::DeleteBackward => self.delete_backward(now),
            EditCommand::DeleteForward => {
                self.delete_with(now, |b, at| grapheme_after(&b.text, at))
            }
            EditCommand::DeleteWordBackward => {
                self.delete_with(now, |b, at| word_before(&b.text, at));
            }
            EditCommand::DeleteWordForward => {
                self.delete_with(now, |b, at| word_after(&b.text, at));
            }
            EditCommand::Indent => self.indent(now),
            EditCommand::Outdent => self.outdent(now),
            EditCommand::MoveLinesUp => self.move_lines(true, now),
            EditCommand::MoveLinesDown => self.move_lines(false, now),
            EditCommand::DuplicateLines => self.duplicate_lines(now),
            EditCommand::ToggleComment => self.toggle_comment(now),
            EditCommand::ToggleBlockComment => self.toggle_block_comment(now),
            EditCommand::SelectAll => {
                let len = self.text.len_chars();
                self.select(vec![Range::new(0, len)], 0);
            }
            EditCommand::AddCaretAbove => self.add_caret(true, place),
            EditCommand::AddCaretBelow => self.add_caret(false, place),
            EditCommand::SelectNext => self.select_next(),
            EditCommand::SingleSelection => {
                let main = self.main();
                self.select(vec![main], 0);
            }
            EditCommand::Undo => self.undo(now),
            EditCommand::Redo => self.redo(now),
            EditCommand::Fold => self.fold(Some(true)),
            EditCommand::Unfold => self.fold(Some(false)),
            EditCommand::ToggleFold => self.fold(None),
            EditCommand::UnfoldAll => self.decorations.folded.clear(),
            EditCommand::FoldAll => self.fold_all(),
            EditCommand::DeleteLines => self.delete_lines(now),
            EditCommand::SelectLine => self.select_line(),
            EditCommand::DuplicateLinesUp => self.duplicate_lines_up(now),
            EditCommand::JoinLines => self.join_lines(now),
            EditCommand::InsertBlankLine => self.insert_blank_line(now),
            EditCommand::SplitLine => self.split_line(now),
            EditCommand::TransposeChars => self.transpose_chars(now),
            EditCommand::DeleteToLineStart => self.delete_with(now, |b, at| {
                let start = b.line_start(b.line_of(at));
                if start == at {
                    at.saturating_sub(1)
                } else {
                    start
                }
            }),
            EditCommand::DeleteToLineEnd => self.delete_with(now, |b, at| {
                let end = b.line_end(b.line_of(at));
                if end == at {
                    (at + 1).min(b.len_chars())
                } else {
                    end
                }
            }),
            EditCommand::Find {
                pattern,
                regex,
                case,
                word,
            } => self.find(&pattern, regex, case, word),
            EditCommand::FindNext => self.find_step(true),
            EditCommand::FindPrevious => self.find_step(false),
            EditCommand::StopFinding => self.found = None,
            EditCommand::Replace(text) => self.replace(&text, now),
            EditCommand::ReplaceAll(text) => self.replace_all(&text, now),
            EditCommand::SelectFound => self.select_found(),
            EditCommand::SelectOccurrences => self.select_occurrences(),
        }
    }

    /// Whether the person can change the text now.
    pub(crate) fn writable(&self) -> bool {
        !self.options.read_only && !self.loading
    }

    /// One level of indent.
    pub(crate) fn unit(&self) -> String {
        if self.options.hard_tabs {
            "\t".to_owned()
        } else {
            " ".repeat(self.tab_columns())
        }
    }

    /// Makes `parts` (one for each range, in order) as the person's, of `kind`.
    pub(crate) fn replace_parts(&mut self, parts: Vec<Part>, kind: Kind, now: f64) {
        let mut changes: Vec<Change> = Vec::with_capacity(parts.len());
        let mut ranges = Vec::with_capacity(parts.len());
        let mut gained: i128 = 0;
        let mut end = 0;
        for part in parts {
            // Two ranges whose changes would overlap (carets side by side, each taking a
            // character): the second takes what the first did not.
            let from = part.from.max(end);
            let to = part.to.max(from);
            end = to;
            let at = usize::try_from(from as i128 + gained).unwrap_or(0);
            ranges.push(Range::new(at + part.select.0, at + part.select.1));
            gained += part.insert.chars().count() as i128 - (to - from) as i128;
            changes.push(Change::new(from, to, part.insert));
        }
        let main = self.main;
        let _ = self.edit(changes, Origin::Person, kind, now, Some((ranges, main)));
    }

    // --- Moving ------------------------------------------------------------------------------

    fn move_by(&mut self, movement: Movement, extend: bool, place: &dyn Placement) {
        self.undo.close();
        let len = self.text.len_chars();
        let vertical = matches!(
            movement,
            Movement::Up | Movement::Down | Movement::PageUp | Movement::PageDown
        );
        let page = isize::try_from(place.page_rows().max(1)).unwrap_or(1);
        let mut goals = Vec::new();
        let ranges: Vec<Range> = self
            .ranges
            .iter()
            .enumerate()
            .map(|(index, range)| {
                let head = range.head;
                let collapse = !extend && !range.is_empty();
                let to = match movement {
                    Movement::Left if collapse => range.start(),
                    Movement::Right if collapse => range.end(),
                    Movement::Left => grapheme_before(&self.text, head),
                    Movement::Right => grapheme_after(&self.text, head),
                    Movement::WordLeft => word_before(&self.text, head),
                    Movement::WordRight => word_after(&self.text, head),
                    Movement::Up | Movement::Down | Movement::PageUp | Movement::PageDown => {
                        let goal = self
                            .goals
                            .get(index)
                            .copied()
                            .flatten()
                            .unwrap_or_else(|| place.x_of(self, head));
                        goals.push(goal);
                        let rows = match movement {
                            Movement::Up => -1,
                            Movement::Down => 1,
                            Movement::PageUp => -page,
                            _ => page,
                        };
                        place
                            .row_from(self, head, rows, goal)
                            .unwrap_or(if rows < 0 { 0 } else { len })
                    }
                    Movement::LineStart => {
                        let line = self.text.line_of(head);
                        let start = self.text.line_start(line);
                        let indent = self
                            .text
                            .line(line)
                            .chars()
                            .take_while(|c| *c == ' ' || *c == '\t')
                            .count();
                        if head == start + indent {
                            start
                        } else {
                            start + indent
                        }
                    }
                    Movement::LineEnd => self.text.line_end(self.text.line_of(head)),
                    Movement::DocumentStart => 0,
                    Movement::DocumentEnd => len,
                    Movement::Bracket => self.matching_bracket(head).map_or(head, |(_, to)| to),
                };
                if extend {
                    Range::new(range.anchor, to)
                } else {
                    Range::caret(to)
                }
            })
            .collect();
        let main = self.main;
        self.set_ranges(ranges, main);
        if vertical && goals.len() == self.ranges.len() {
            self.goals = goals.into_iter().map(Some).collect();
        }
    }

    /// The bracket at `at` (or just before it) and the one that matches it: where each is.
    pub fn matching_bracket(&self, at: usize) -> Option<(usize, usize)> {
        let pairs: Vec<(char, char)> = self
            .options
            .pairs
            .iter()
            .copied()
            .filter(|(open, close)| open != close)
            .collect();
        for here in [Some(at), at.checked_sub(1)].into_iter().flatten() {
            let Some(c) = self.text.char_at(here) else {
                continue;
            };
            let Some((open, close)) = pairs.iter().find(|(o, cl)| *o == c || *cl == c).copied()
            else {
                continue;
            };
            let mut depth = 0usize;
            if c == open {
                let mut chars = self.text.chars_at(here + 1)?;
                for (step, next) in (&mut chars).take(BRACKET_REACH).enumerate() {
                    if next == open {
                        depth += 1;
                    } else if next == close {
                        if depth == 0 {
                            return Some((here, here + 1 + step));
                        }
                        depth -= 1;
                    }
                }
            } else {
                let mut chars = self.text.chars_at(here)?;
                for step in 0..BRACKET_REACH {
                    let Some(prev) = chars.prev() else {
                        break;
                    };
                    if prev == close {
                        depth += 1;
                    } else if prev == open {
                        if depth == 0 {
                            return Some((here, here - 1 - step));
                        }
                        depth -= 1;
                    }
                }
            }
            return None;
        }
        None
    }

    fn add_caret(&mut self, above: bool, place: &dyn Placement) {
        let from = if above {
            self.ranges.first()
        } else {
            self.ranges.last()
        };
        let Some(from) = from.copied() else {
            return;
        };
        let x = place.x_of(self, from.head);
        let Some(at) = place.row_from(self, from.head, if above { -1 } else { 1 }, x) else {
            return;
        };
        let mut ranges = self.ranges.clone();
        ranges.push(Range::caret(at));
        let main = ranges.len() - 1;
        self.select(ranges, main);
    }

    /// The main range's text found once more after the last range, selected and made the main
    /// one; a caret alone selects the word it is in first.
    fn select_next(&mut self) {
        let main = self.main();
        if main.is_empty() {
            if let Some((from, to)) = word_at(&self.text, main.head) {
                let mut ranges = self.ranges.clone();
                if let Some(range) = ranges.get_mut(self.main) {
                    *range = Range::new(from, to);
                }
                let at = self.main;
                self.select(ranges, at);
            }
            return;
        }
        let Some(needle) = self.text.slice(main.start(), main.end()) else {
            return;
        };
        let len = needle.chars().count();
        let last = self.ranges.last().map_or(0, |range| range.end());
        let mut from = last;
        for _ in 0..=self.ranges.len() {
            let found = self
                .text
                .find_from(from, &needle)
                .or_else(|| self.text.find_from(0, &needle));
            let Some(at) = found else {
                return;
            };
            let range = Range::new(at, at + len);
            if self
                .ranges
                .iter()
                .any(|r| r.start() == range.start() && r.end() == range.end())
            {
                from = at + len;
                continue;
            }
            let mut ranges = self.ranges.clone();
            ranges.push(range);
            let main = ranges.len() - 1;
            self.select(ranges, main);
            return;
        }
    }

    // --- Typing and deleting ---------------------------------------------------------------

    /// The person typed `text` at every caret, in place of what each range selects. A bracket or a
    /// quote of the editor's pairs comes with its closer, or around what is selected; its closer
    /// typed before the same closer steps over it.
    ///
    /// Where the page says how its language indents ([`pencere_view::edit::IndentRules`]), a line
    /// that what is typed makes match the rule `decrease` goes a level out, undone with it.
    pub fn type_text(&mut self, text: &str, now: f64) {
        if !self.writable() || text.is_empty() {
            return;
        }
        // Which lines matched the rule before: only one that comes to match it is moved.
        let closed: Option<Vec<bool>> = (!text.contains('\n') && self.rules.decrease.is_set())
            .then(|| {
                self.ranges
                    .iter()
                    .map(|range| self.closed_before(*range))
                    .collect()
            });
        let version = self.version;
        let mut chars = text.chars();
        let paired = match (chars.next(), chars.next()) {
            (Some(only), None) => self.type_pair(only, now),
            _ => false,
        };
        if !paired {
            let kind = if text.contains('\n') {
                Kind::Other
            } else {
                Kind::Typing
            };
            let parts = self
                .ranges
                .iter()
                .map(|range| Part::put(range.start(), range.end(), text.to_owned()))
                .collect();
            self.replace_parts(parts, kind, now);
        }
        if let Some(closed) = closed
            && self.version != version
        {
            self.outdent_closed(&closed, now);
        }
    }

    /// Pastes `text`: one line for each caret where it has as many lines as there are carets, the
    /// whole of it at each otherwise. It is undone apart from what was typed around it.
    pub fn paste(&mut self, text: &str, now: f64) {
        if !self.writable() || text.is_empty() {
            return;
        }
        self.undo.close();
        let lines: Vec<&str> = text.lines().collect();
        let each = self.ranges.len() > 1 && lines.len() == self.ranges.len();
        let parts = self
            .ranges
            .iter()
            .enumerate()
            .map(|(index, range)| {
                let insert = if each {
                    lines.get(index).copied().unwrap_or_default().to_owned()
                } else {
                    text.to_owned()
                };
                Part::put(range.start(), range.end(), insert)
            })
            .collect();
        self.replace_parts(parts, Kind::Other, now);
        self.undo.close();
    }

    /// What a copy takes: what the ranges select, a line each; or, where none selects anything,
    /// the lines the carets are on.
    pub fn copy_text(&self) -> Option<String> {
        if self.ranges.iter().all(|range| range.is_empty()) {
            let mut lines: Vec<usize> = self
                .ranges
                .iter()
                .map(|range| self.text.line_of(range.head))
                .collect();
            lines.dedup();
            let text: String = lines
                .into_iter()
                .map(|line| format!("{}\n", self.text.line(line)))
                .collect();
            return Some(text);
        }
        let parts: Vec<String> = self
            .ranges
            .iter()
            .filter(|range| !range.is_empty())
            .filter_map(|range| self.text.slice(range.start(), range.end()))
            .collect();
        Some(parts.join("\n"))
    }

    /// What a cut takes ([`Buffer::copy_text`]), taken out of the text.
    pub fn cut(&mut self, now: f64) -> Option<String> {
        let copied = self.copy_text()?;
        if !self.writable() {
            return Some(copied);
        }
        self.undo.close();
        let parts = if self.ranges.iter().all(|range| range.is_empty()) {
            let mut lines: Vec<usize> = self
                .ranges
                .iter()
                .map(|range| self.text.line_of(range.head))
                .collect();
            lines.dedup();
            lines
                .into_iter()
                .map(|line| {
                    let from = self.text.line_start(line);
                    let to = self.text.line_start(line + 1);
                    // The last line has no break of its own: the one before it goes.
                    let from = if to == self.text.len_chars()
                        && line + 1 >= self.text.len_lines()
                        && from > 0
                    {
                        from - 1
                    } else {
                        from
                    };
                    Part::put(from, to, String::new())
                })
                .collect()
        } else {
            self.ranges
                .iter()
                .map(|range| Part::put(range.start(), range.end(), String::new()))
                .collect()
        };
        self.replace_parts(parts, Kind::Other, now);
        self.undo.close();
        Some(copied)
    }

    /// A closer typed before itself, or an opener: what the pairs do. Returns whether it was one.
    fn type_pair(&mut self, c: char, now: f64) -> bool {
        let pairs = self.options.pairs.clone();
        if pairs.is_empty() {
            return false;
        }
        let closes = pairs.iter().any(|(_, close)| *close == c);
        if closes
            && self
                .ranges
                .iter()
                .all(|range| range.is_empty() && self.text.char_at(range.head) == Some(c))
        {
            let ranges = self
                .ranges
                .iter()
                .map(|range| Range::caret(range.head + 1))
                .collect();
            let main = self.main;
            self.set_ranges(ranges, main);
            return true;
        }
        let Some((open, close)) = pairs.iter().find(|(open, _)| *open == c).copied() else {
            return false;
        };
        let parts = self
            .ranges
            .iter()
            .map(|range| {
                if !range.is_empty() {
                    let inner = self
                        .text
                        .slice(range.start(), range.end())
                        .unwrap_or_default();
                    let len = inner.chars().count();
                    let select = if range.anchor <= range.head {
                        (1, 1 + len)
                    } else {
                        (1 + len, 1)
                    };
                    return Part {
                        from: range.start(),
                        to: range.end(),
                        insert: format!("{open}{inner}{close}"),
                        select,
                    };
                }
                let at = range.head;
                if self.closes_here(at, open, close, &pairs) {
                    Part {
                        from: at,
                        to: at,
                        insert: format!("{open}{close}"),
                        select: (1, 1),
                    }
                } else {
                    Part::put(at, at, open.to_string())
                }
            })
            .collect();
        self.replace_parts(parts, Kind::Typing, now);
        true
    }

    /// Whether an opener typed at `at` comes with its closer: where what follows is nothing, a
    /// space or a closer, and (for a quote, whose opener is its closer) what comes before is not a
    /// letter or the quote.
    fn closes_here(&self, at: usize, open: char, close: char, pairs: &[(char, char)]) -> bool {
        let next = self.text.char_at(at);
        let after_ok = next.is_none_or(|n| n.is_whitespace() || pairs.iter().any(|(_, c)| *c == n));
        if open != close {
            return after_ok;
        }
        let before = at.checked_sub(1).and_then(|at| self.text.char_at(at));
        let before_ok = before.is_none_or(|p| !p.is_alphanumeric() && p != open);
        after_ok && before_ok && next != Some(open)
    }

    fn delete_backward(&mut self, now: f64) {
        if !self.writable() {
            return;
        }
        let tab = self.tab_columns();
        let parts = self
            .ranges
            .iter()
            .map(|range| {
                if !range.is_empty() {
                    return Part::put(range.start(), range.end(), String::new());
                }
                let at = range.head;
                if at == 0 {
                    return Part::put(0, 0, String::new());
                }
                let before = self.text.char_at(at - 1);
                let after = self.text.char_at(at);
                if let (Some(before), Some(after)) = (before, after)
                    && self.options.pairs.contains(&(before, after))
                {
                    return Part::put(at - 1, at + 1, String::new());
                }
                // In an indent of spaces, back to the stop before.
                let line = self.text.line_of(at);
                let start = self.text.line_start(line);
                let spaces = self
                    .text
                    .slice(start, at)
                    .is_some_and(|before| before.chars().all(|c| c == ' '));
                if spaces && !self.options.hard_tabs && at > start {
                    let n = (at - start - 1) % tab + 1;
                    return Part::put(at - n, at, String::new());
                }
                Part::put(grapheme_before(&self.text, at), at, String::new())
            })
            .collect();
        self.replace_parts(parts, Kind::Deleting, now);
    }

    /// Deletes what each range selects, or from its caret to where `to` says.
    pub(crate) fn delete_with(&mut self, now: f64, to: impl Fn(&Buffer, usize) -> usize) {
        if !self.writable() {
            return;
        }
        let parts = self
            .ranges
            .iter()
            .map(|range| {
                if !range.is_empty() {
                    return Part::put(range.start(), range.end(), String::new());
                }
                let other = to(self, range.head);
                Part::put(range.head.min(other), range.head.max(other), String::new())
            })
            .collect();
        self.replace_parts(parts, Kind::Deleting, now);
    }

    /// A line break at each caret: the new line has the indent of the line it came from, a level
    /// more after an opener or a line that matches the rule `increase`
    /// ([`pencere_view::edit::IndentRules`]), and between an opener and its closer (or between
    /// such a line and what matches the rule `decrease`) the closer goes on a line of its own.
    fn newline(&mut self, now: f64) {
        if !self.writable() {
            return;
        }
        self.undo.close();
        let unit = self.unit();
        let parts = self
            .ranges
            .iter()
            .map(|range| {
                let (from, to) = (range.start(), range.end());
                let line = self.text.line_of(from);
                let start = self.text.line_start(line);
                let before = self.text.slice(start, from).unwrap_or_default();
                let indent: String = before
                    .chars()
                    .take_while(|c| *c == ' ' || *c == '\t')
                    .collect();
                let opener = before.trim_end().chars().last().and_then(|last| {
                    self.options
                        .pairs
                        .iter()
                        .find(|(open, close)| *open == last && open != close)
                        .copied()
                });
                let opens = opener.is_some() || self.opens(&before);
                let closer = match opener {
                    Some((_, close)) => self.text.char_at(to) == Some(close),
                    None => opens && self.closes_from(to),
                };
                if closer {
                    let first = format!("\n{indent}{unit}");
                    let len = first.chars().count();
                    Part {
                        from,
                        to,
                        insert: format!("{first}\n{indent}"),
                        select: (len, len),
                    }
                } else if opens {
                    Part::put(from, to, format!("\n{indent}{unit}"))
                } else {
                    Part::put(from, to, format!("\n{indent}"))
                }
            })
            .collect();
        self.replace_parts(parts, Kind::Other, now);
        self.undo.close();
    }

    /// The tab key: a level of indent for the lines of the ranges where one selects something; an
    /// indent at each caret otherwise (spaces to the next stop, or a tab).
    pub fn tab(&mut self, now: f64) {
        if !self.writable() {
            return;
        }
        if self.ranges.iter().any(|range| !range.is_empty()) {
            self.indent(now);
            return;
        }
        let tab = self.tab_columns();
        let hard = self.options.hard_tabs;
        let parts = self
            .ranges
            .iter()
            .map(|range| {
                let at = range.head;
                let insert = if hard {
                    "\t".to_owned()
                } else {
                    let line = self.text.line_of(at);
                    let start = self.text.line_start(line);
                    let column = column_of(&self.text.line(line), at - start.min(at), tab);
                    " ".repeat(tab - column % tab)
                };
                Part::put(at, at, insert)
            })
            .collect();
        self.replace_parts(parts, Kind::Typing, now);
    }

    /// The lines the ranges are on, in order, each once: a range that ends at the start of a line
    /// (a whole line selected, with its break) is not on that one.
    pub(crate) fn lines(&self) -> Vec<(usize, usize)> {
        let mut blocks: Vec<(usize, usize)> = Vec::new();
        for range in &self.ranges {
            let first = self.text.line_of(range.start());
            let mut last = self.text.line_of(range.end());
            if !range.is_empty() && last > first && self.text.line_start(last) == range.end() {
                last -= 1;
            }
            match blocks.last_mut() {
                Some(block) if first <= block.1 + 1 => block.1 = block.1.max(last),
                _ => blocks.push((first, last)),
            }
        }
        blocks
    }

    fn each_line(&self) -> Vec<usize> {
        self.lines()
            .into_iter()
            .flat_map(|(first, last)| first..=last)
            .collect()
    }

    fn indent(&mut self, now: f64) {
        if !self.writable() {
            return;
        }
        let unit = self.unit();
        let lines = self.each_line();
        let any_text = lines
            .iter()
            .any(|line| !self.text.line(*line).trim().is_empty());
        let changes = lines
            .into_iter()
            .filter(|line| !any_text || !self.text.line(*line).trim().is_empty())
            .map(|line| {
                let at = self.text.line_start(line);
                Change::new(at, at, unit.clone())
            })
            .collect();
        self.undo.close();
        let _ = self.edit(changes, Origin::Person, Kind::Other, now, None);
    }

    fn outdent(&mut self, now: f64) {
        if !self.writable() {
            return;
        }
        let tab = self.tab_columns();
        let changes = self
            .each_line()
            .into_iter()
            .filter_map(|line| {
                let text = self.text.line(line);
                let at = self.text.line_start(line);
                let n = if text.starts_with('\t') {
                    1
                } else {
                    text.chars().take(tab).take_while(|c| *c == ' ').count()
                };
                (n > 0).then(|| Change::new(at, at + n, ""))
            })
            .collect();
        self.undo.close();
        let _ = self.edit(changes, Origin::Person, Kind::Other, now, None);
    }

    /// The lines of each range one line up (or down), past the line before (or after) them.
    fn move_lines(&mut self, up: bool, now: f64) {
        if !self.writable() {
            return;
        }
        let blocks = self.lines();
        let last_line = self.text.len_lines().saturating_sub(1);
        if (up && blocks.first().is_some_and(|b| b.0 == 0))
            || (!up && blocks.last().is_some_and(|b| b.1 >= last_line))
        {
            return;
        }
        let mut changes = Vec::new();
        let mut moves: Vec<(usize, usize, i128)> = Vec::new();
        for (first, last) in &blocks {
            let block = self
                .text
                .slice(self.text.line_start(*first), self.text.line_end(*last))
                .unwrap_or_default();
            if up {
                let other = self.text.line(first - 1);
                let from = self.text.line_start(first - 1);
                let to = self.text.line_end(*last);
                moves.push((
                    self.text.line_start(*first),
                    to,
                    -(other.chars().count() as i128 + 1),
                ));
                changes.push(Change::new(from, to, format!("{block}\n{other}")));
            } else {
                let other = self.text.line(last + 1);
                let from = self.text.line_start(*first);
                let to = self.text.line_end(last + 1);
                moves.push((
                    from,
                    self.text.line_end(*last),
                    other.chars().count() as i128 + 1,
                ));
                changes.push(Change::new(from, to, format!("{other}\n{block}")));
            }
        }
        let shift = |at: usize| -> usize {
            moves
                .iter()
                .find(|(from, to, _)| at >= *from && at <= *to)
                .map_or(at, |(_, _, by)| {
                    usize::try_from(at as i128 + by).unwrap_or(0)
                })
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

    /// The lines of each range once more below them; the ranges go with the copy.
    fn duplicate_lines(&mut self, now: f64) {
        if !self.writable() {
            return;
        }
        let blocks = self.lines();
        let mut changes = Vec::new();
        let mut moves: Vec<(usize, usize, i128)> = Vec::new();
        let mut gained: i128 = 0;
        for (first, last) in &blocks {
            let from = self.text.line_start(*first);
            let to = self.text.line_end(*last);
            let block = self.text.slice(from, to).unwrap_or_default();
            let len = block.chars().count() as i128 + 1;
            moves.push((from, to, gained + len));
            gained += len;
            changes.push(Change::new(to, to, format!("\n{block}")));
        }
        let shift = |at: usize| -> usize {
            moves
                .iter()
                .find(|(from, to, _)| at >= *from && at <= *to)
                .map_or(at, |(_, _, by)| {
                    usize::try_from(at as i128 + by).unwrap_or(0)
                })
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

    /// The lines of the ranges commented with the language's line comment, or uncommented where
    /// every line with text on it already is; where the language has no line comment, the lines of
    /// each range put inside its block comment, or taken out of it.
    fn toggle_comment(&mut self, now: f64) {
        if !self.writable() {
            return;
        }
        let line = self.options.comment.line.clone().filter(|t| !t.is_empty());
        match (line, self.block_tokens()) {
            (Some(token), _) => self.toggle_line_comment(&token, now),
            (None, Some((open, close))) => {
                let spans = self.line_spans();
                self.toggle_block(spans, &open, &close, now);
            }
            (None, None) => {}
        }
    }

    fn toggle_line_comment(&mut self, token: &str, now: f64) {
        let lines: Vec<(usize, String)> = self
            .each_line()
            .into_iter()
            .map(|line| (line, self.text.line(line)))
            .filter(|(_, text)| !text.trim().is_empty())
            .collect();
        if lines.is_empty() {
            return;
        }
        let commented = lines
            .iter()
            .all(|(_, text)| text.trim_start().starts_with(token));
        let token_len = token.chars().count();
        let changes: Vec<Change> = if commented {
            lines
                .iter()
                .map(|(line, text)| {
                    let indent = text.chars().take_while(|c| c.is_whitespace()).count();
                    let at = self.text.line_start(*line) + indent;
                    let space = text.chars().nth(indent + token_len) == Some(' ');
                    Change::new(at, at + token_len + usize::from(space), "")
                })
                .collect()
        } else {
            let indent = lines
                .iter()
                .map(|(_, text)| text.chars().take_while(|c| c.is_whitespace()).count())
                .min()
                .unwrap_or(0);
            lines
                .iter()
                .map(|(line, _)| {
                    let at = self.text.line_start(*line) + indent;
                    Change::new(at, at, format!("{token} "))
                })
                .collect()
        };
        self.undo.close();
        let _ = self.edit(changes, Origin::Person, Kind::Other, now, None);
    }

    /// Each range put inside the language's block comment, or taken out of it; a caret's range is
    /// its line's text.
    fn toggle_block_comment(&mut self, now: f64) {
        if !self.writable() {
            return;
        }
        let Some((open, close)) = self.block_tokens() else {
            return;
        };
        let mut spans: Vec<(usize, usize)> = Vec::with_capacity(self.ranges.len());
        for range in &self.ranges {
            let span = if range.is_empty() {
                self.text_of_lines(self.text.line_of(range.head), self.text.line_of(range.head))
            } else {
                (range.start(), range.end())
            };
            match spans.last_mut() {
                // Carets on one line are one range, and a caret's line takes in a range on it.
                Some(last) if span.0 < last.1 || span == *last => {
                    *last = (last.0.min(span.0), last.1.max(span.1));
                }
                _ => spans.push(span),
            }
        }
        self.toggle_block(spans, &open, &close, now);
    }

    /// The language's block comment, where it has one.
    fn block_tokens(&self) -> Option<(String, String)> {
        self.options
            .comment
            .block
            .clone()
            .filter(|(open, close)| !open.is_empty() && !close.is_empty())
    }

    /// The text of the lines `first` to `last`: from where the first's indent ends to where the
    /// last's text ends, its spaces at the end left out.
    fn text_of_lines(&self, first: usize, last: usize) -> (usize, usize) {
        let start = self.text.line_start(first);
        let indent = self.text.chars_at(start).map_or(0, |chars| {
            chars.take_while(|c| *c == ' ' || *c == '\t').count()
        });
        let from = (start + indent).min(self.text.line_end(first));
        let mut to = self.text.line_end(last);
        if let Some(mut back) = self.text.chars_at(to) {
            while to > from && back.prev().is_some_and(char::is_whitespace) {
                to -= 1;
            }
        }
        (from, to.max(from))
    }

    /// The text of the lines of each range (CodeMirror's comment of a language with block comments
    /// alone): ranges whose lines overlap are one.
    fn line_spans(&self) -> Vec<(usize, usize)> {
        let mut blocks: Vec<(usize, usize)> = Vec::new();
        for range in &self.ranges {
            let first = self.text.line_of(range.start());
            let mut last = self.text.line_of(range.end());
            if !range.is_empty() && last > first && self.text.line_start(last) == range.end() {
                last -= 1;
            }
            match blocks.last_mut() {
                Some(block) if first <= block.1 => block.1 = block.1.max(last),
                _ => blocks.push((first, last)),
            }
        }
        blocks
            .into_iter()
            .map(|(first, last)| self.text_of_lines(first, last))
            .collect()
    }

    /// `spans` (in order, apart) put inside `open` and `close`, a space between them and the text,
    /// those not inside them already; or, where every one is, taken out. The selections keep
    /// what they selected.
    fn toggle_block(&mut self, spans: Vec<(usize, usize)>, open: &str, close: &str, now: f64) {
        if spans.is_empty() {
            return;
        }
        let found: Vec<Option<Wrapped>> = spans
            .iter()
            .map(|(from, to)| self.wrapped(*from, *to, open, close))
            .collect();
        let changes: Vec<Change>;
        let mut after = None;
        if found.iter().all(Option::is_some) {
            changes = found
                .into_iter()
                .flatten()
                .flat_map(|w| {
                    [
                        Change::new(w.open.0, w.open.1, ""),
                        Change::new(w.close.0, w.close.1, ""),
                    ]
                })
                .collect();
        } else {
            let (open_len, close_len) = (open.chars().count() + 1, close.chars().count() + 1);
            let wrap: Vec<(usize, usize)> = spans
                .iter()
                .zip(&found)
                .filter(|(_, found)| found.is_none())
                .map(|(span, _)| *span)
                .collect();
            changes = wrap
                .iter()
                .flat_map(|(from, to)| {
                    if from == to {
                        vec![Change::new(*from, *to, format!("{open}  {close}"))]
                    } else {
                        vec![
                            Change::new(*from, *from, format!("{open} ")),
                            Change::new(*to, *to, format!(" {close}")),
                        ]
                    }
                })
                .collect();
            // A place at the start of a span or in it goes past what opens the comment; one at its
            // end stays before what closes it.
            let place = |at: usize| {
                at + wrap
                    .iter()
                    .map(|(from, to)| {
                        usize::from(at >= *from) * open_len + usize::from(at > *to) * close_len
                    })
                    .sum::<usize>()
            };
            let ranges = self
                .ranges
                .iter()
                .map(|range| Range::new(place(range.anchor), place(range.head)))
                .collect();
            after = Some((ranges, self.main));
        }
        self.undo.close();
        let _ = self.edit(changes, Origin::Person, Kind::Other, now, after);
        self.undo.close();
    }

    /// Where `open` and `close` are around `from..to`, with spaces or tabs alone between them and
    /// it (`/* x */` with `x` selected), or are its ends, with spaces alone before and after them
    /// (`/* x */` selected): what is taken out to uncomment it, a space or a tab beside each token
    /// with it. As CodeMirror looks, but a line break is never taken with a token.
    fn wrapped(&self, from: usize, to: usize, open: &str, close: &str) -> Option<Wrapped> {
        let (open_len, close_len) = (open.chars().count(), close.chars().count());
        let blank = |c: &char| *c == ' ' || *c == '\t';
        let len = self.text.len_chars();
        let before: Vec<char> = self
            .text
            .slice(from.saturating_sub(COMMENT_REACH + open_len), from)
            .unwrap_or_default()
            .chars()
            .collect();
        let after: Vec<char> = self
            .text
            .slice(to, (to + COMMENT_REACH + close_len).min(len))
            .unwrap_or_default()
            .chars()
            .collect();
        let space_before = before.iter().rev().take_while(|c| blank(c)).count();
        let space_after = after.iter().take_while(|c| blank(c)).count();
        let ends_with = |chars: &[char], token: &str| {
            let token: Vec<char> = token.chars().collect();
            chars.ends_with(&token)
        };
        let starts_with = |chars: &[char], token: &str| {
            let token: Vec<char> = token.chars().collect();
            chars.starts_with(&token)
        };
        if ends_with(
            before
                .get(..before.len() - space_before)
                .unwrap_or_default(),
            open,
        ) && starts_with(after.get(space_after..).unwrap_or_default(), close)
        {
            let open_end = from - space_before;
            let close_start = to + space_after;
            return Some(Wrapped {
                open: (open_end - open_len, open_end + space_before.min(1)),
                close: (close_start - space_after.min(1), close_start + close_len),
            });
        }
        // The tokens are the ends of the range: its first and last characters read, past spaces.
        let head: Vec<char> = self
            .text
            .slice(from, (from + COMMENT_REACH + open_len).min(to))
            .unwrap_or_default()
            .chars()
            .collect();
        let tail: Vec<char> = self
            .text
            .slice(to.saturating_sub(COMMENT_REACH + close_len).max(from), to)
            .unwrap_or_default()
            .chars()
            .collect();
        let lead = head.iter().take_while(|c| c.is_whitespace()).count();
        let trail = tail.iter().rev().take_while(|c| c.is_whitespace()).count();
        if !starts_with(head.get(lead..).unwrap_or_default(), open)
            || !ends_with(tail.get(..tail.len() - trail).unwrap_or_default(), close)
        {
            return None;
        }
        let open_at = from + lead;
        let close_end = to - trail;
        if open_at + open_len + close_len > close_end {
            return None;
        }
        let close_at = close_end - close_len;
        let open_space = self
            .text
            .char_at(open_at + open_len)
            .is_some_and(|c| blank(&c));
        let close_space = close_at > open_at + open_len
            && self.text.char_at(close_at - 1).is_some_and(|c| blank(&c));
        let open_cut = (open_at, open_at + open_len + usize::from(open_space));
        let close_cut = (
            (close_at - usize::from(close_space)).max(open_cut.1),
            close_end,
        );
        Some(Wrapped {
            open: open_cut,
            close: close_cut,
        })
    }

    // --- Folding and finding ---------------------------------------------------------------

    /// The fold region that starts on the main caret's line folded (`Some(true)`), unfolded
    /// (`Some(false)`) or the other way round (`None`).
    fn fold(&mut self, fold: Option<bool>) {
        let line = self.text.line_of(self.main().head);
        self.fold_at(line, fold);
    }

    /// The fold region that starts on line `line` folded (`Some(true)`), unfolded (`Some(false)`)
    /// or the other way round (`None`): what a fold button of the gutter does. Carets in what it
    /// hides go to the end of its first line. Returns whether a region starts there.
    pub fn fold_at(&mut self, line: usize, fold: Option<bool>) -> bool {
        // What is folded from a line is told by the line: a folded region has been carried over the
        // changes since, and the region worked out now need not be the same.
        let folded = self.is_folded(line);
        let want = fold.unwrap_or(!folded);
        if want == folded {
            return folded || !self.fold_lines(line, line).is_empty();
        }
        if !want {
            let text = &self.text;
            self.decorations
                .folded
                .retain(|(from, _)| text.line_of(*from) != line);
            return true;
        }
        let Some(region) = self.fold_region(line) else {
            return false;
        };
        let end = self.text.line_end(line);
        let ranges: Vec<Range> = self
            .ranges
            .iter()
            .map(|range| {
                let inside = |at: usize| if at > end && at <= region.1 { end } else { at };
                Range::new(inside(range.anchor), inside(range.head))
            })
            .collect();
        let main = self.main;
        self.set_ranges(ranges, main);
        self.decorations.folded.push(region);
        self.decorations.folded.sort_unstable();
        true
    }

    /// A word under a double click, or a run of marks or of spaces.
    pub fn word_range(&self, at: usize) -> (usize, usize) {
        run_at(&self.text, at)
    }

    /// Looks for `pattern`, and selects the first match at the main selection or after it.
    fn find(&mut self, pattern: &str, regex: bool, case: bool, word: bool) {
        match Query::new(pattern, regex, case, word) {
            Ok(query) => {
                let search = Search::new(query, &self.text.string());
                let first = search.after(self.main().start());
                self.found = Some(search);
                self.found_asked = true;
                if let Some((from, to)) = first {
                    self.select(vec![Range::new(from, to)], 0);
                }
            }
            Err(refusal) => {
                self.found = None;
                let _ = self.refuse(refusal);
            }
        }
    }

    /// Selects the next match from the main caret, or the one before.
    fn find_step(&mut self, forward: bool) {
        self.search_again();
        let Some(found) = &self.found else {
            return;
        };
        let main = self.main();
        let next = if forward {
            found.after(if main.is_empty() {
                main.head
            } else {
                main.end()
            })
        } else {
            found.before(main.start())
        };
        if let Some((from, to)) = next {
            self.select(vec![Range::new(from, to)], 0);
        }
    }

    /// Where the main selection is a match, `template` in its place and the next match selected;
    /// where it is not, the next match from it selected. Each replacement is undone apart.
    fn replace(&mut self, template: &str, now: f64) {
        if !self.writable() {
            return;
        }
        // The document as one string, copied once at most before the change: what a stale search
        // reads again, and what a regular expression's groups are read from.
        let mut copy = None;
        self.search_again_in(&mut copy);
        let Some(found) = &self.found else {
            return;
        };
        let main = self.main();
        let Some((from, to)) = found.after(main.start()) else {
            return;
        };
        if (from, to) != (main.start(), main.end()) {
            self.select(vec![Range::new(from, to)], 0);
            return;
        }
        let insert = if found.query.expands() {
            let text = copy.get_or_insert_with(|| self.text.string());
            let start = self.text.bytes_of(0, from).unwrap_or(0);
            found.query.replacement(text, start, template)
        } else {
            template.to_owned()
        };
        let end = from + insert.chars().count();
        self.undo.close();
        let done = self.edit(
            vec![Change::new(from, to, insert)],
            Origin::Person,
            Kind::Other,
            now,
            Some((vec![Range::caret(end)], 0)),
        );
        self.undo.close();
        if done.is_err() {
            return;
        }
        // What it finds now is told with the change, as a search asked for is.
        self.search_again();
        if let Some((from, to)) = self.found.as_ref().and_then(|found| found.after(end)) {
            self.select(vec![Range::new(from, to)], 0);
        }
    }

    /// `template` in place of every match in the document, as one change, undone as one: not only
    /// the [`FIND_MATCHES`] a search marks, which are read on from to the end of the document where
    /// they stop short of it. (A search for whole words stops where it runs out of what it may read
    /// again, a pattern that matches inside words again and again; what it found before is
    /// replaced, and the search, made again, says there may be more.)
    fn replace_all(&mut self, template: &str, now: f64) {
        if !self.writable() {
            return;
        }
        // The document as one string, copied once at most before the change: what a stale search
        // reads again, what a search cut short reads on, and what a regular expression's groups
        // are read from.
        let mut copy = None;
        self.search_again_in(&mut copy);
        let Some(found) = &self.found else {
            return;
        };
        let read_on;
        let matches = if found.more {
            let text = copy.get_or_insert_with(|| self.text.string());
            read_on = found.query.matches(text, usize::MAX).0;
            &read_on
        } else {
            &found.ranges
        };
        if matches.is_empty() {
            return;
        }
        let mut changes = Vec::with_capacity(matches.len());
        for &(from, to) in matches {
            let insert = if found.query.expands() {
                let text = copy.get_or_insert_with(|| self.text.string());
                let start = self.text.bytes_of(0, from).unwrap_or(0);
                found.query.replacement(text, start, template)
            } else {
                template.to_owned()
            };
            changes.push(Change::new(from, to, insert));
        }
        self.undo.close();
        let done = self.edit(changes, Origin::Person, Kind::Other, now, None);
        self.undo.close();
        if done.is_ok() {
            self.search_again();
        }
    }

    /// Every match selected, the main one the match the main selection is on, else the one a
    /// search would select.
    fn select_found(&mut self) {
        self.search_again();
        let Some(found) = &self.found else {
            return;
        };
        let main = self.main();
        let Some(first) = found.after(main.start()) else {
            return;
        };
        let main = found
            .index_of(main)
            .or_else(|| found.ranges.iter().position(|range| *range == first))
            .unwrap_or(0);
        let ranges = found
            .ranges
            .iter()
            .map(|(from, to)| Range::new(*from, *to))
            .collect();
        self.select(ranges, main);
    }

    /// Every place the main selection's text is, selected, or where it is a caret every place the
    /// word it is in is (the word `select_next` takes), as a whole word: in the case
    /// written, the words as they are. The main selection stays the main one; what is searched for
    /// stays as it was.
    fn select_occurrences(&mut self) {
        let main = self.main();
        let (from, to, word) = if main.is_empty() {
            // The word `select_next` takes.
            let Some((from, to)) = word_at(&self.text, main.head) else {
                return;
            };
            (from, to, true)
        } else {
            (main.start(), main.end(), false)
        };
        let Some(needle) = self.text.slice(from, to) else {
            return;
        };
        let Ok(query) = Query::new(&needle, false, true, word) else {
            return;
        };
        let (found, _) = query.matches(&self.text.string(), FIND_MATCHES);
        let mut ranges: Vec<Range> = found
            .into_iter()
            .map(|(from, to)| Range::new(from, to))
            .collect();
        let main =
            match ranges.binary_search_by_key(&(from, to), |range| (range.start(), range.end())) {
                Ok(index) => index,
                Err(at) => {
                    // Not among the places marked (they stopped short of it, or one overlaps it): it is
                    // put in its place, in order, and where that is one more than are selected, the
                    // place farthest from it goes.
                    ranges.insert(at, Range::new(from, to));
                    let first = ranges.first().map_or(from, |range| range.start());
                    let last = ranges.last().map_or(to, |range| range.end());
                    if ranges.len() <= FIND_MATCHES {
                        at
                    } else if at > 0 && from.saturating_sub(first) >= last.saturating_sub(to) {
                        ranges.remove(0);
                        at - 1
                    } else {
                        ranges.pop();
                        at
                    }
                }
            };
        self.select(ranges, main);
    }
}

#[cfg(test)]
mod tests {
    use pencere_view::edit::{Change, EditCommand};

    use crate::buffer::Buffer;
    use crate::text::COPIES;

    /// How many times `run` copied a document into one string.
    fn copies(run: impl FnOnce()) -> usize {
        let before = COPIES.with(std::cell::Cell::get);
        run();
        COPIES.with(std::cell::Cell::get) - before
    }

    fn searching(pattern: &str, regex: bool) -> Buffer {
        let mut buffer = Buffer::new("a=1 b=2 a=3 c=4").expect("a document");
        buffer.command(
            EditCommand::Find {
                pattern: pattern.into(),
                regex,
                case: true,
                word: false,
            },
            0.0,
        );
        buffer
    }

    /// A page's change since the search: what it found is to be looked for again.
    fn changed(buffer: &mut Buffer) {
        let base = buffer.version();
        buffer
            .change(base, vec![Change::new(15, 15, " d=5")], 0.0)
            .expect("the change");
    }

    #[test]
    fn a_replacement_reads_the_document_once_before_its_change_for_its_search_and_its_groups() {
        // Stale, and a regular expression whose groups are read: one copy for both, one after.
        let mut buffer = searching(r"(\w)=(\d)", true);
        changed(&mut buffer);
        let copied = copies(|| buffer.command(EditCommand::Replace("$2$1".into()), 1.0));
        assert_eq!(buffer.text(), "1a b=2 a=3 c=4 d=5");
        assert_eq!(
            copied, 2,
            "one before the change, one to search again after it"
        );
        let mut buffer = searching(r"(\w)=(\d)", true);
        changed(&mut buffer);
        let copied = copies(|| buffer.command(EditCommand::ReplaceAll("$2$1".into()), 1.0));
        assert_eq!(buffer.text(), "1a 2b 3a 4c 5d");
        assert_eq!(
            copied, 2,
            "one before the change, one to search again after it"
        );
        // Searched as it is, and no groups to read: only the search after the change copies.
        let mut buffer = searching("a=", false);
        let copied = copies(|| buffer.command(EditCommand::ReplaceAll("x".into()), 1.0));
        assert_eq!(buffer.text(), "x1 b=2 x3 c=4");
        assert_eq!(copied, 1);
        // What the next frame searches again it was searched for already.
        assert_eq!(copies(|| assert!(!buffer.search_again())), 0);
    }
}
