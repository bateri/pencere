//! Finding: a pattern looked for through the whole document, what it found kept as ranges and
//! carried over the changes made after.

use pencere_view::edit::Refusal;
use regex::RegexBuilder;

use crate::changes::{Assoc, Step, map};

/// The most places a search marks; what it finds past them is not marked.
pub const FIND_MATCHES: usize = 10_000;

/// A search and what it found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Found {
    pub pattern: String,
    pub regex: bool,
    pub case: bool,
    /// The ranges it found, in order, at most [`FIND_MATCHES`].
    pub ranges: Vec<(usize, usize)>,
}

impl Found {
    /// Looks for `pattern` in `text`.
    pub fn search(text: &str, pattern: &str, regex: bool, case: bool) -> Result<Self, Refusal> {
        let source = if regex {
            pattern.to_owned()
        } else {
            regex::escape(pattern)
        };
        let found = RegexBuilder::new(&source)
            .case_insensitive(!case)
            .multi_line(true)
            .size_limit(1 << 22)
            .build()
            .map_err(|_| Refusal::BadPattern)?;
        let mut ranges = Vec::new();
        // The places are characters: counted from one match to the next.
        let (mut byte, mut chars) = (0, 0);
        if !pattern.is_empty() {
            for hit in found.find_iter(text) {
                if hit.start() == hit.end() {
                    continue;
                }
                chars += text.get(byte..hit.start()).map_or(0, |s| s.chars().count());
                let len = hit.as_str().chars().count();
                ranges.push((chars, chars + len));
                chars += len;
                byte = hit.end();
                if ranges.len() >= FIND_MATCHES {
                    break;
                }
            }
        }
        Ok(Self {
            pattern: pattern.to_owned(),
            regex,
            case,
            ranges,
        })
    }

    /// What it found, carried over `steps`; a range whose text went goes.
    pub fn map(&mut self, steps: &[Step]) {
        self.ranges.retain_mut(|(from, to)| {
            *from = map(steps, *from, Assoc::After);
            *to = map(steps, *to, Assoc::Before);
            from < to
        });
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
}
