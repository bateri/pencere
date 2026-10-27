//! The indent of the lines: how far in a line is, what the page's rules of its language's indent
//! do when a line is broken and when the person types ([`IndentRules`]), and the regions that fold
//! by the indent ([`Folds::Indent`]).
//!
//! The regions that fold by the indent are worked out when they are asked for, and never kept:
//! whether one starts on a line is a look at that line and at the next one with text (what the
//! gutter asks of the lines in view when it is drawn), and how far one goes is a walk down its
//! lines, made only when it is folded. Neither reads more of a document than that, however large
//! it is, nor anything at all at a key; and so neither is ever out of date after a change.

use pencere_view::edit::{Change, Folds, IndentRules, Origin, Range, limits};
use regex::{Regex, RegexBuilder};

use crate::buffer::Buffer;
use crate::history::Kind;

/// The longest line the rules read, characters: a longer one is no line a person indents by hand,
/// and reading it whole at every key would cost as much as the line.
const RULE_REACH: usize = 10_000;

/// How many lines with no text a look for the next (or the last) line with text goes past: a run
/// of more ends a block that folds, and has no line above it for the rule `decrease`.
pub(crate) const BLANK_REACH: usize = 1_000;

/// One of the page's patterns, compiled: the page says its options every frame, and a pattern is
/// compiled again only when its text changes. One that is no pattern (the sandbox fails a page
/// that says one; a host that drives the editor itself may not) matches nothing.
#[derive(Clone, Debug, Default)]
pub(crate) struct Pattern {
    source: Option<String>,
    compiled: Option<Regex>,
}

impl Pattern {
    fn set(&mut self, source: Option<&str>) {
        if self.source.as_deref() == source {
            return;
        }
        self.source = source.map(str::to_owned);
        self.compiled = source.and_then(|source| compile(source).ok());
    }

    /// Whether the page said a pattern that is one.
    pub fn is_set(&self) -> bool {
        self.compiled.is_some()
    }

    pub fn matches(&self, text: &str) -> bool {
        self.compiled
            .as_ref()
            .is_some_and(|pattern| pattern.is_match(text))
    }
}

/// A rule of a language's indent compiled as the editor compiles it: Rust's `regex`, in the case
/// written, held to [`limits::PATTERN_SIZE`].
pub fn compile(source: &str) -> Result<Regex, regex::Error> {
    RegexBuilder::new(source)
        .size_limit(limits::PATTERN_SIZE)
        .build()
}

/// The page's rules of its language's indent, compiled.
#[derive(Clone, Debug, Default)]
pub(crate) struct Rules {
    pub increase: Pattern,
    pub decrease: Pattern,
}

impl Rules {
    pub fn set(&mut self, rules: &IndentRules) {
        self.increase.set(rules.increase.as_deref());
        self.decrease.set(rules.decrease.as_deref());
    }
}

impl Buffer {
    /// How far in line `line` is, columns (a tab to its next stop); none where it has no text.
    pub(crate) fn indent_of(&self, line: usize) -> Option<usize> {
        let tab = self.tab_columns();
        let mut columns = 0;
        let mut indent = true;
        for c in self.text.chars_at(self.text.line_start(line))? {
            match c {
                '\n' => return None,
                ' ' if indent => columns += 1,
                '\t' if indent => columns = (columns / tab + 1) * tab,
                // Another space (a `\r` before the break): the indent ends, the text is still to come.
                c if c.is_whitespace() => indent = false,
                _ => return Some(columns),
            }
        }
        None
    }

    /// The text of line `line`, where it is no longer than the rules read.
    fn rule_text(&self, line: usize) -> Option<String> {
        let (start, end) = (self.text.line_start(line), self.text.line_end(line));
        (end - start <= RULE_REACH)
            .then(|| self.text.slice(start, end))
            .flatten()
    }

    /// Whether `text`, a line's text up to a place, opens a block: it matches the rule `increase`,
    /// or it ends with an opener of the pairs.
    pub(crate) fn opens(&self, text: &str) -> bool {
        let opener = text.trim_end().chars().last().is_some_and(|last| {
            self.options
                .pairs
                .iter()
                .any(|(open, close)| *open == last && open != close)
        });
        opener || (text.chars().count() <= RULE_REACH && self.rules.increase.matches(text))
    }

    /// Whether what follows the place `at` on its line closes a block: it matches the rule
    /// `decrease`.
    pub(crate) fn closes_from(&self, at: usize) -> bool {
        let end = self.text.line_end(self.text.line_of(at));
        self.rules.decrease.is_set()
            && end - at.min(end) <= RULE_REACH
            && self
                .text
                .slice(at, end)
                .is_some_and(|after| self.rules.decrease.matches(&after))
    }

    /// The indent line `line` gives the lines below it: its own, and a level more where it opens a
    /// block.
    fn level_below(&self, line: usize, columns: usize) -> usize {
        let opens = self.rule_text(line).is_some_and(|text| self.opens(&text));
        columns + if opens { self.tab_columns() } else { 0 }
    }

    /// The nearest line above `line` with text, and how far in it is.
    fn line_above(&self, line: usize) -> Option<(usize, usize)> {
        (line.saturating_sub(BLANK_REACH + 1)..line)
            .rev()
            .find_map(|above| self.indent_of(above).map(|columns| (above, columns)))
    }

    /// Whether the line of `range` matched the rule `decrease` before what is typed in place of the
    /// range: what is before it on its first line and what is after it on its last, put together.
    /// A line too long to read counts as one that did, and is left as it is.
    pub(crate) fn closed_before(&self, range: Range) -> bool {
        let (from, to) = (range.start(), range.end());
        let start = self.text.line_start(self.text.line_of(from));
        let end = self.text.line_end(self.text.line_of(to));
        if (from - start) + (end - to) > RULE_REACH {
            return true;
        }
        let mut line = self.text.slice(start, from).unwrap_or_default();
        line.push_str(&self.text.slice(to, end).unwrap_or_default());
        self.rules.decrease.matches(&line)
    }

    /// What was typed made the lines of the carets match the rule `decrease` (`closed` says, for
    /// each range before the typing, whether its line matched it already): each such line goes a
    /// level out from the indent the line above it gives, where it is not already further out. It
    /// is undone with what was typed.
    pub(crate) fn outdent_closed(&mut self, closed: &[bool], now: f64) {
        // A caret for each range typed in: where they came together, which line was which is not
        // known, and none is moved.
        if self.ranges.len() != closed.len() {
            return;
        }
        let mut lines: Vec<usize> = self
            .ranges
            .iter()
            .zip(closed)
            .filter(|(_, closed)| !**closed)
            .map(|(range, _)| self.text.line_of(range.start()))
            .collect();
        lines.dedup();
        let unit = self.tab_columns();
        let changes: Vec<Change> = lines
            .into_iter()
            .filter_map(|line| {
                let text = self.rule_text(line)?;
                if !self.rules.decrease.matches(&text) {
                    return None;
                }
                let columns = self.indent_of(line)?;
                let level = self
                    .line_above(line)
                    .map_or(0, |(above, columns)| self.level_below(above, columns));
                if columns < level {
                    return None;
                }
                self.reindent(line, &text, level.saturating_sub(unit))
            })
            .collect();
        if changes.is_empty() {
            return;
        }
        // Of the same kind and at the same time as what was typed, with nothing between: one group.
        let _ = self.edit(changes, Origin::Person, Kind::Typing, now, None);
    }

    /// The change that takes line `line` (its text `text`) to an indent of `columns`: as much of
    /// its indent as fits kept, the rest made up with the editor's own (tabs where it indents with
    /// them, spaces otherwise).
    fn reindent(&self, line: usize, text: &str, columns: usize) -> Option<Change> {
        let tab = self.tab_columns();
        let stop = |at: usize| (at / tab + 1) * tab;
        // `kept` of the `held` characters of the indent are kept: they come to `at` columns.
        let (mut kept, mut at, mut held) = (0, 0, 0);
        for c in text.chars() {
            let next = match c {
                ' ' => at + 1,
                '\t' => stop(at),
                _ => break,
            };
            if kept == held && next <= columns {
                kept += 1;
                at = next;
            }
            held += 1;
        }
        let mut fill = String::new();
        while self.options.hard_tabs && stop(at) <= columns {
            fill.push('\t');
            at = stop(at);
        }
        fill.push_str(&" ".repeat(columns - at));
        let start = self.text.line_start(line);
        let removed = text
            .chars()
            .skip(kept)
            .take(held - kept)
            .collect::<String>();
        (removed != fill).then(|| Change::new(start + kept, start + held, fill))
    }

    // --- Folding by the indent -------------------------------------------------------------

    /// Whether the editor folds by the indent, and a region that does starts on line `line`: the
    /// line has text, and the next line with text after it is further in.
    pub(crate) fn folds_by_indent(&self, line: usize) -> bool {
        if self.options.folds != Folds::Indent {
            return false;
        }
        let Some(columns) = self.indent_of(line) else {
            return false;
        };
        let last = self.text.len_lines();
        (line + 1..last.min(line + BLANK_REACH + 2))
            .find_map(|below| self.indent_of(below))
            .is_some_and(|below| below > columns)
    }

    /// The region that folds by the indent from line `line`: from the end of the line to the end
    /// of the last line further in after it, the lines with no text among them in it and those
    /// after the last of them not.
    pub(crate) fn indent_region(&self, line: usize) -> Option<(usize, usize)> {
        if !self.folds_by_indent(line) {
            return None;
        }
        let columns = self.indent_of(line)?;
        let (mut last, mut blank) = (line, 0);
        for below in line + 1..self.text.len_lines() {
            match self.indent_of(below) {
                None if blank < BLANK_REACH => blank += 1,
                Some(further) if further > columns => {
                    last = below;
                    blank = 0;
                }
                _ => break,
            }
        }
        (last > line).then(|| (self.text.line_end(line), self.text.line_end(last)))
    }
}
