//! The document's text: a rope of characters, every place of it checked before it is used, so that
//! nothing a page sends reaches the rope as a place it does not have. The editor counts in
//! characters; the page counts in bytes of UTF-8, and the two are turned into one another here.

use std::borrow::Cow;
use std::cell::Cell;
use std::fmt;

use pencere_view::edit::{Refusal, TextSnapshot};
use ropey::Rope;

#[cfg(test)]
thread_local! {
    /// How many times this thread copied a whole text into one string ([`Text::string`]): what the
    /// tests of a command that reads the document so count its copies by.
    pub(crate) static COPIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// How many times this thread looked a line up in the rope ([`Text::line_start`] and
    /// [`Text::line_of`], and so every `line_end` and `line`): what the tests of a pass over the
    /// lines of a document so count its searches by.
    pub(crate) static LINE_SEARCHES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// How many times this thread walked every line of a text to find the longest ([`Text::longest_line`]
    /// where it did not already know): what the tests of the width of a long document count by.
    pub(crate) static LONGEST_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The text of a document. Places are characters (code points), as the editor holds them; the
/// page's places are bytes ([`Text::char_of_byte`], [`Text::byte_of_char`]). A line ends at its
/// `\n`.
#[derive(Clone, Default)]
pub(crate) struct Text {
    rope: Rope,
    /// How many characters the longest line has, where that is known: a change keeps it known
    /// where it can say what it became ([`Text::replace`]), and where it cannot the next
    /// [`Text::longest_line`] walks the lines again.
    longest: Cell<Option<usize>>,
}

/// A change that touches more lines than this does not say what became of the longest line: the
/// walk that finds it is cheaper than looking up each of the lines it touched.
const TOUCHED: usize = 64;

impl fmt::Debug for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Text")
            .field("chars", &self.rope.len_chars())
            .field("lines", &self.rope.len_lines())
            .finish()
    }
}

impl Text {
    pub fn new(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            longest: Cell::new(None),
        }
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn len_bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    /// The characters `from..to`, or `None` for a range the text does not have.
    pub fn slice(&self, from: usize, to: usize) -> Option<String> {
        if from > to {
            return None;
        }
        self.rope.get_slice(from..to).map(|slice| slice.to_string())
    }

    /// The place of the character that starts at the byte `at`: the page's place, as the editor
    /// holds it. A byte past the end is out of the document, and one inside a character is
    /// refused ([`Refusal::MidCharacter`]), not rounded.
    pub fn char_of_byte(&self, at: usize) -> Result<usize, Refusal> {
        char_of_byte(&self.rope, at)
    }

    /// The byte the character at `at` starts at: the place the page is told. The end of the text
    /// is its length in bytes; a place past it is as far past it in bytes.
    pub fn byte_of_char(&self, at: usize) -> usize {
        byte_of_char(&self.rope, at)
    }

    /// The bytes of UTF-8 the characters `from..to` take.
    pub fn bytes_of(&self, from: usize, to: usize) -> Option<usize> {
        let start = self.rope.try_char_to_byte(from).ok()?;
        let end = self.rope.try_char_to_byte(to).ok()?;
        end.checked_sub(start)
    }

    pub fn char_at(&self, at: usize) -> Option<char> {
        self.rope.get_char(at)
    }

    /// The characters from `at` on (and, by `prev`, those before it).
    pub fn chars_at(&self, at: usize) -> Option<ropey::iter::Chars<'_>> {
        self.rope.get_chars_at(at)
    }

    /// The place of the first `needle` at or after the place `at`, or `None`.
    pub fn find_from(&self, at: usize, needle: &str) -> Option<usize> {
        if needle.is_empty() {
            return None;
        }
        let whole = self.string();
        let start = self.rope.try_char_to_byte(at).ok()?;
        let found = whole.get(start..)?.find(needle)? + start;
        Some(at + whole.get(start..found)?.chars().count())
    }

    /// The line the place `at` is on (the end of the text is on the last line).
    pub fn line_of(&self, at: usize) -> usize {
        #[cfg(test)]
        LINE_SEARCHES.with(|searches| searches.set(searches.get() + 1));
        let at = at.min(self.len_chars());
        self.rope.try_char_to_line(at).unwrap_or(0)
    }

    /// Where line `line` starts; the end of the text past the last.
    pub fn line_start(&self, line: usize) -> usize {
        if line >= self.len_lines() {
            return self.len_chars();
        }
        #[cfg(test)]
        LINE_SEARCHES.with(|searches| searches.set(searches.get() + 1));
        self.rope.try_line_to_char(line).unwrap_or(0)
    }

    /// Where line `line` ends: the place of its `\n`, or the end of the text for the last.
    pub fn line_end(&self, line: usize) -> usize {
        if line + 1 >= self.len_lines() {
            return self.len_chars();
        }
        self.line_start(line + 1).saturating_sub(1)
    }

    /// How many characters the longest line has, its `\n` not counted: what `line_end - line_start`
    /// says of each line. Kept from one change to the next where a change can say what it did to it
    /// ([`Text::replace`]); otherwise found in one pass over the rope's lines and not by a search for
    /// each.
    pub fn longest_line(&self) -> usize {
        if let Some(longest) = self.longest.get() {
            return longest;
        }
        #[cfg(test)]
        LONGEST_SCANS.with(|scans| scans.set(scans.get() + 1));
        let last = self.len_lines().saturating_sub(1);
        let longest = self
            .rope
            .lines()
            .enumerate()
            .map(|(line, text)| text.len_chars() - usize::from(line < last))
            .max()
            .unwrap_or(0);
        self.longest.set(Some(longest));
        longest
    }

    /// How many characters the longest of the lines `first..=last` has, or `None` where they are
    /// more than a change is let to look up ([`TOUCHED`]).
    fn longest_of(&self, first: usize, last: usize) -> Option<usize> {
        let last = last.min(self.len_lines().saturating_sub(1));
        if last.saturating_sub(first) >= TOUCHED {
            return None;
        }
        Some(
            (first..=last)
                .map(|line| self.line_end(line) - self.line_start(line))
                .max()
                .unwrap_or(0),
        )
    }

    /// The text of every line, without its `\n`, in one pass over the rope's lines: what `line` gives
    /// for each, without a search for each; a line that lies in one piece of the rope is not copied.
    pub fn lines(&self) -> impl Iterator<Item = Cow<'_, str>> {
        let last = self.len_lines().saturating_sub(1);
        self.rope.lines().enumerate().map(move |(line, text)| {
            // A line in one piece of the rope is that piece less its `\n`: not cut out of the tree
            // again, which costs a descent of it for each line of the document.
            if let Some(whole) = text.as_str() {
                let whole = if line < last {
                    whole.strip_suffix('\n').unwrap_or(whole)
                } else {
                    whole
                };
                return Cow::Borrowed(whole);
            }
            let text = if line < last {
                text.slice(..text.len_chars() - 1)
            } else {
                text
            };
            match text.as_str() {
                Some(text) => Cow::Borrowed(text),
                None => Cow::Owned(text.to_string()),
            }
        })
    }

    /// The text of line `line`, without its `\n`.
    pub fn line(&self, line: usize) -> String {
        self.slice(self.line_start(line), self.line_end(line))
            .unwrap_or_default()
    }

    /// The place `at` in units of UTF-16 (what macOS counts in), or `None` past the end.
    pub fn utf16_of(&self, at: usize) -> Option<usize> {
        self.rope.try_char_to_utf16_cu(at).ok()
    }

    /// The place that is `units` of UTF-16 into the text. A place in the middle of a character
    /// that takes two units is the place of that character.
    pub fn place_of_utf16(&self, units: usize) -> Option<usize> {
        self.rope.try_utf16_cu_to_char(units).ok()
    }

    /// Puts `insert` in place of `from..to`; `false`, and nothing done, for a range the text does
    /// not have.
    pub fn replace(&mut self, from: usize, to: usize, insert: &str) -> bool {
        if from > to || to > self.len_chars() {
            return false;
        }
        // The lines the change touches are those the range is in; the others are as long after it
        // as before. Where the longest line is not one of the touched, it is still there; where it
        // is, it is still the longest if a touched line is now as long as it was. The text before
        // `from` is as it was after the change, so the first touched line is the same line.
        let first = self.line_of(from);
        let was = self.longest.get().and_then(|longest| {
            self.longest_of(first, self.line_of(to))
                .map(|touched| (longest, touched))
        });
        self.longest.set(None);
        if from < to && self.rope.try_remove(from..to).is_err() {
            return false;
        }
        if !insert.is_empty() && self.rope.try_insert(from, insert).is_err() {
            return false;
        }
        if let Some((longest, touched)) = was {
            let breaks = insert.matches('\n').count();
            if let Some(now) = self.longest_of(first, first + breaks) {
                if touched < longest {
                    self.longest.set(Some(longest.max(now)));
                } else if now >= longest {
                    self.longest.set(Some(now));
                }
            }
        }
        true
    }

    pub fn snapshot(&self, version: u64) -> Snapshot {
        Snapshot {
            rope: self.rope.clone(),
            version,
        }
    }

    /// The whole text.
    pub fn string(&self) -> String {
        #[cfg(test)]
        COPIES.with(|copies| copies.set(copies.get() + 1));
        self.rope.to_string()
    }

    /// The characters around `at`, at most `reach` on each side, and where `at` is in them: what
    /// the boundaries of characters as a person reads them, and of words, are looked for in.
    pub fn around(&self, at: usize, reach: usize) -> (String, usize) {
        let at = at.min(self.len_chars());
        let start = at.saturating_sub(reach);
        let end = (at + reach).min(self.len_chars());
        (self.slice(start, end).unwrap_or_default(), at - start)
    }
}

/// The character that starts at the byte `at` of `rope`.
fn char_of_byte(rope: &Rope, at: usize) -> Result<usize, Refusal> {
    if at > rope.len_bytes() {
        return Err(Refusal::OutOfDocument);
    }
    // `try_byte_to_char` rounds a byte inside a character down to it: the byte that character
    // starts at must be the one asked for.
    let at_char = rope
        .try_byte_to_char(at)
        .map_err(|_| Refusal::OutOfDocument)?;
    match rope.try_char_to_byte(at_char) {
        Ok(start) if start == at => Ok(at_char),
        _ => Err(Refusal::MidCharacter),
    }
}

fn byte_of_char(rope: &Rope, at: usize) -> usize {
    match rope.try_char_to_byte(at) {
        Ok(byte) => byte,
        // Past the end it stays past it, as far.
        Err(_) => rope.len_bytes().saturating_add(at - rope.len_chars()),
    }
}

/// A document's text at one version, as the page is posted it to read.
#[derive(Clone)]
pub struct Snapshot {
    rope: Rope,
    version: u64,
}

impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Snapshot")
            .field("version", &self.version)
            .field("chars", &self.rope.len_chars())
            .finish()
    }
}

impl TextSnapshot for Snapshot {
    fn version(&self) -> u64 {
        self.version
    }

    fn len_bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    fn inside_character(&self, at: usize) -> bool {
        at <= self.rope.len_bytes() && char_of_byte(&self.rope, at) == Err(Refusal::MidCharacter)
    }

    fn slice(&self, from: usize, to: usize) -> Option<String> {
        if from > to {
            return None;
        }
        let (from, to) = (
            char_of_byte(&self.rope, from).ok()?,
            char_of_byte(&self.rope, to).ok()?,
        );
        self.rope.get_slice(from..to).map(|slice| slice.to_string())
    }

    fn line_start(&self, line: usize) -> Option<usize> {
        if line >= self.rope.len_lines() {
            return None;
        }
        self.rope.try_line_to_byte(line).ok()
    }

    fn line_of(&self, at: usize) -> Option<usize> {
        let at = char_of_byte(&self.rope, at).ok()?;
        self.rope.try_char_to_line(at).ok()
    }

    fn word_range(&self, at: usize) -> Option<(usize, usize)> {
        let at = char_of_byte(&self.rope, at).ok()?;
        let (from, to) = crate::segments::run_at(
            &Text {
                rope: self.rope.clone(),
                longest: Cell::new(None),
            },
            at,
        );
        Some((byte_of_char(&self.rope, from), byte_of_char(&self.rope, to)))
    }
}
