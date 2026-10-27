//! Finding: a pattern looked for through the whole document, what it found kept as ranges, carried
//! over the changes made after it and looked for again before the editor is next drawn; and what a
//! match is replaced with.

use pencere_view::edit::{Range, Refusal, limits};
use regex::{Regex, RegexBuilder};

use crate::changes::{Assoc, Step, map};

/// The most places a search marks; what it finds past them is not marked.
pub const FIND_MATCHES: usize = limits::FIND_MATCHES;

/// How much a search for whole words may read again, as a multiple of the text and a floor. A match
/// that is a part of a word is passed over, and the search goes on from the next character; text
/// made of such matches would read the same stretch again and again. Past this the search stops,
/// and says there may be more.
const RETRY_FACTOR: usize = 4;
const RETRY_FLOOR: usize = 1 << 16;

/// A match: the byte of the text it starts at, and the places it spans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Hit {
    pub byte: usize,
    pub range: (usize, usize),
}

/// What is looked for: a pattern, and how it is read.
#[derive(Clone, Debug)]
pub(crate) struct Query {
    /// A regular expression, not the words as they are.
    regex: bool,
    /// Only a match with no letter, digit or `_` just before it or just after it.
    word: bool,
    /// Nothing is looked for: an empty pattern finds nothing.
    empty: bool,
    compiled: Regex,
}

impl Query {
    /// `pattern`, read as a regular expression where `regex`, in either case where not `case`;
    /// refused where it is not one.
    pub fn new(pattern: &str, regex: bool, case: bool, word: bool) -> Result<Self, Refusal> {
        let source = if regex {
            pattern.to_owned()
        } else {
            regex::escape(pattern)
        };
        let compiled = RegexBuilder::new(&source)
            .case_insensitive(!case)
            .multi_line(true)
            .size_limit(limits::PATTERN_SIZE)
            .build()
            .map_err(|_| Refusal::BadPattern)?;
        Ok(Self {
            regex,
            word,
            empty: pattern.is_empty(),
            compiled,
        })
    }

    /// Where it is in `text`, in order, at most [`FIND_MATCHES`] matches; and whether the search
    /// stopped before the end of the text. A match with nothing in it is none.
    pub fn matches(&self, text: &str) -> (Vec<Hit>, bool) {
        let mut found = Vec::new();
        if self.empty {
            return (found, false);
        }
        // The places are characters: counted from one match to the next.
        let (mut counted, mut chars) = (0, 0);
        let budget = text
            .len()
            .saturating_mul(RETRY_FACTOR)
            .saturating_add(RETRY_FLOOR);
        let mut spent = 0usize;
        let mut at = 0;
        loop {
            if self.word {
                // A whole word begins only where no part of a word is before it.
                let Some(edge) = next_edge(text, at) else {
                    break;
                };
                at = edge;
            }
            let Some(hit) = self.compiled.find_at(text, at) else {
                break;
            };
            let empty = hit.start() == hit.end();
            if empty || (self.word && !is_whole(text, hit.start(), hit.end())) {
                // Passed over: the next try is from the character after where it starts.
                if !empty {
                    spent = spent.saturating_add(hit.end().saturating_sub(at) + 1);
                    if spent > budget {
                        return (found, true);
                    }
                }
                let Some(c) = text.get(hit.start()..).and_then(|rest| rest.chars().next()) else {
                    break;
                };
                at = hit.start() + c.len_utf8();
                continue;
            }
            if found.len() >= FIND_MATCHES {
                return (found, true);
            }
            chars += text
                .get(counted..hit.start())
                .map_or(0, |s| s.chars().count());
            let len = hit.as_str().chars().count();
            found.push(Hit {
                byte: hit.start(),
                range: (chars, chars + len),
            });
            chars += len;
            counted = hit.end();
            at = hit.end();
        }
        (found, false)
    }

    /// What goes in place of the match that starts at the byte `start` of `text`: `template` with
    /// what the match's groups hold put in (`$1`, `${1}`, `$name`, `${name}`, `$$`) where this is a
    /// regular expression, `template` as it is where it is not.
    pub fn replacement(&self, text: &str, start: usize, template: &str) -> String {
        if !self.regex {
            return template.to_owned();
        }
        let mut out = String::new();
        match self.compiled.captures_at(text, start) {
            Some(groups) => groups.expand(template, &mut out),
            None => out.push_str(template),
        }
        out
    }
}

/// Whether a character is part of a word: a letter or a digit of any script, or `_`.
pub(crate) fn is_word(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// Whether the bytes `start..end` of `text` have no part of a word just before or just after them.
fn is_whole(text: &str, start: usize, end: usize) -> bool {
    let before = text.get(..start).and_then(|head| head.chars().next_back());
    let after = text.get(end..).and_then(|tail| tail.chars().next());
    !before.is_some_and(is_word) && !after.is_some_and(is_word)
}

/// The first byte at or after `at` that has no part of a word before it; none where there is no
/// such place left.
fn next_edge(text: &str, at: usize) -> Option<usize> {
    if !text.get(..at)?.chars().next_back().is_some_and(is_word) {
        return Some(at);
    }
    // The place after a character is one when the character is not part of a word.
    text.get(at..)?
        .char_indices()
        .find(|(_, c)| !is_word(*c))
        .map(|(offset, c)| at + offset + c.len_utf8())
}

/// A search and what it found.
#[derive(Clone, Debug)]
pub(crate) struct Search {
    pub query: Query,
    /// The ranges it found, in order, at most [`FIND_MATCHES`].
    pub ranges: Vec<(usize, usize)>,
    /// It stopped before the end of the document: there may be more.
    pub more: bool,
    /// The document changed since: the ranges are carried over the changes, and the search is to
    /// be made again.
    pub stale: bool,
}

impl Search {
    /// `query` looked for in `text`.
    pub fn new(query: Query, text: &str) -> Self {
        let mut search = Self {
            query,
            ranges: Vec::new(),
            more: false,
            stale: true,
        };
        search.again(text);
        search
    }

    /// The search made again, in `text`.
    pub fn again(&mut self, text: &str) {
        let (found, more) = self.query.matches(text);
        self.ranges = found.into_iter().map(|hit| hit.range).collect();
        self.more = more;
        self.stale = false;
    }

    /// What it found, carried over `steps`; a range whose text went goes. The search is to be
    /// made again.
    pub fn map(&mut self, steps: &[Step]) {
        self.ranges.retain_mut(|(from, to)| {
            *from = map(steps, *from, Assoc::After);
            *to = map(steps, *to, Assoc::Before);
            from < to
        });
        self.stale = true;
    }

    /// The first range found that starts at `at` or after it, or the first of all past the last.
    pub fn after(&self, at: usize) -> Option<(usize, usize)> {
        self.ranges
            .iter()
            .find(|(from, _)| *from >= at)
            .or_else(|| self.ranges.first())
            .copied()
    }

    /// The last range found that ends before `at`, or the last of all before the first.
    pub fn before(&self, at: usize) -> Option<(usize, usize)> {
        self.ranges
            .iter()
            .rev()
            .find(|(from, _)| *from < at)
            .or_else(|| self.ranges.last())
            .copied()
    }

    /// Which of the ranges `range` is exactly, from 0.
    pub fn index_of(&self, range: Range) -> Option<usize> {
        self.ranges
            .binary_search(&(range.start(), range.end()))
            .ok()
    }
}
