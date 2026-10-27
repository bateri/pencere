//! The document's text: a rope of characters, every place of it checked before it is used, so that
//! nothing a page sends reaches the rope as a place it does not have.

use std::fmt;

use pencere_view::edit::TextSnapshot;
use ropey::Rope;

/// The text of a document. Places are characters (code points); a line ends at its `\n`.
#[derive(Clone, Default)]
pub(crate) struct Text {
    rope: Rope,
}

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
        let at = at.min(self.len_chars());
        self.rope.try_char_to_line(at).unwrap_or(0)
    }

    /// Where line `line` starts; the end of the text past the last.
    pub fn line_start(&self, line: usize) -> usize {
        if line >= self.len_lines() {
            return self.len_chars();
        }
        self.rope.try_line_to_char(line).unwrap_or(0)
    }

    /// Where line `line` ends: the place of its `\n`, or the end of the text for the last.
    pub fn line_end(&self, line: usize) -> usize {
        if line + 1 >= self.len_lines() {
            return self.len_chars();
        }
        self.line_start(line + 1).saturating_sub(1)
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
        if from < to && self.rope.try_remove(from..to).is_err() {
            return false;
        }
        insert.is_empty() || self.rope.try_insert(from, insert).is_ok()
    }

    pub fn snapshot(&self, version: u64) -> Snapshot {
        Snapshot {
            rope: self.rope.clone(),
            version,
        }
    }

    /// The whole text.
    pub fn string(&self) -> String {
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

    fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    fn slice(&self, from: usize, to: usize) -> Option<String> {
        if from > to {
            return None;
        }
        self.rope.get_slice(from..to).map(|slice| slice.to_string())
    }

    fn line_start(&self, line: usize) -> Option<usize> {
        if line >= self.rope.len_lines() {
            return None;
        }
        self.rope.try_line_to_char(line).ok()
    }

    fn line_of(&self, at: usize) -> Option<usize> {
        self.rope.try_char_to_line(at).ok()
    }
}
