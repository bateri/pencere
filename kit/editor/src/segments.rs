//! Where a character as a person reads it ends (a grapheme: a letter with its accents, an emoji of
//! several code points), and where a word does. They are looked for in the characters around a
//! place ([`REACH`] on each side): a document can be one line of many megabytes, and a person moves
//! a character at a time.

use unicode_segmentation::UnicodeSegmentation;

use crate::text::Text;

/// How many characters on each side of a place its boundaries are looked for in. A grapheme longer
/// than this (none that is written is) is broken where the reach ends.
const REACH: usize = 256;

/// The places, in characters of `window`, where its graphemes start, and its end.
fn grapheme_starts(window: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut chars = 0;
    let mut byte = 0;
    for (at, _) in window.grapheme_indices(true) {
        chars += window.get(byte..at).map_or(0, |s| s.chars().count());
        byte = at;
        out.push(chars);
    }
    out.push(window.chars().count());
    out
}

/// The start of the grapheme before `at` (where `at` is in the middle of one, its start).
pub(crate) fn grapheme_before(text: &Text, at: usize) -> usize {
    if at == 0 {
        return 0;
    }
    let (window, here) = text.around(at, REACH);
    let starts = grapheme_starts(&window);
    let before = starts
        .iter()
        .rev()
        .find(|start| **start < here)
        .copied()
        .unwrap_or(0);
    at - (here - before)
}

/// The end of the grapheme at `at` (where `at` is in the middle of one, its end).
pub(crate) fn grapheme_after(text: &Text, at: usize) -> usize {
    if at >= text.len_chars() {
        return text.len_chars();
    }
    let (window, here) = text.around(at, REACH);
    let starts = grapheme_starts(&window);
    let after = starts
        .iter()
        .find(|start| **start > here)
        .copied()
        .unwrap_or(here + 1);
    at + (after - here)
}

/// What a run of characters is to a move by words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Space,
    Line,
    Word,
    Punctuation,
}

fn class_of(segment: &str) -> Class {
    if segment.contains('\n') {
        Class::Line
    } else if segment.chars().all(char::is_whitespace) {
        Class::Space
    } else if segment.chars().any(|c| c.is_alphanumeric() || c == '_') {
        Class::Word
    } else {
        Class::Punctuation
    }
}

/// The words of `window` and what lies between them: each run's start and end, in characters, and
/// its class. A word is one by Unicode's rules, cut where a mark stands inside it (`bar.baz` is two
/// words to a person writing code); marks next to each other are one run, and so are spaces.
fn runs(window: &str) -> Vec<(usize, usize, Class)> {
    let mut out: Vec<(usize, usize, Class, usize)> = Vec::new();
    let mut chars = 0;
    for (index, segment) in window.split_word_bounds().enumerate() {
        let whole = class_of(segment);
        for grapheme in segment.graphemes(true) {
            let len = grapheme.chars().count();
            let class = match whole {
                Class::Word if class_of(grapheme) == Class::Punctuation => Class::Punctuation,
                other => other,
            };
            match out.last_mut() {
                // A word goes on inside the segment it is; marks, and spaces, run together across
                // them; a line break stands alone.
                Some(last)
                    if last.2 == class
                        && class != Class::Line
                        && (class != Class::Word || last.3 == index) =>
                {
                    last.1 += len;
                }
                _ => out.push((chars, chars + len, class, index)),
            }
            chars += len;
        }
    }
    out.into_iter()
        .map(|(from, to, class, _)| (from, to, class))
        .collect()
}

/// Where a move a word to the right from `at` ends: past the spaces, at the end of the word or the
/// marks after them; a line break is a stop of its own.
pub(crate) fn word_after(text: &Text, at: usize) -> usize {
    let (window, here) = text.around(at, REACH);
    let runs = runs(&window);
    let mut seen_space = false;
    for (from, to, class) in runs {
        if to <= here {
            continue;
        }
        match class {
            Class::Space => seen_space = true,
            Class::Line => {
                return at
                    + (if from >= here && !seen_space {
                        to
                    } else {
                        from.max(here)
                    } - here);
            }
            Class::Word | Class::Punctuation => return at + (to - here),
        }
    }
    text.len_chars().min(at + (window.chars().count() - here))
}

/// Where a move a word to the left from `at` ends: past the spaces, at the start of the word or
/// the marks before them.
pub(crate) fn word_before(text: &Text, at: usize) -> usize {
    let (window, here) = text.around(at, REACH);
    let runs = runs(&window);
    let mut seen_space = false;
    for (from, to, class) in runs.into_iter().rev() {
        if from >= here {
            continue;
        }
        match class {
            Class::Space => seen_space = true,
            Class::Line => {
                return at
                    - (here
                        - if to <= here && !seen_space {
                            from
                        } else {
                            to.min(here)
                        });
            }
            Class::Word | Class::Punctuation => return at - (here - from),
        }
    }
    at.saturating_sub(here)
}

/// The word at `at`, or touching it: its range; `None` where there is none.
pub(crate) fn word_at(text: &Text, at: usize) -> Option<(usize, usize)> {
    let (window, here) = text.around(at, REACH);
    let base = at - here;
    let runs = runs(&window);
    let inside = runs
        .iter()
        .find(|(from, to, class)| *class == Class::Word && *from <= here && here < *to)
        .or_else(|| {
            runs.iter()
                .find(|(_, to, class)| *class == Class::Word && *to == here)
        })?;
    Some((base + inside.0, base + inside.1))
}

/// The run at `at` a double click selects: a word, a run of marks or of spaces.
pub(crate) fn run_at(text: &Text, at: usize) -> (usize, usize) {
    let (window, here) = text.around(at, REACH);
    let base = at - here;
    let runs = runs(&window);
    let found = runs
        .iter()
        .find(|(from, to, class)| *from <= here && here < *to && *class != Class::Line)
        .or_else(|| runs.iter().find(|(_, to, _)| *to == here));
    match found {
        Some((from, to, _)) => (base + from, base + to),
        None => (at, at),
    }
}

/// How many graphemes `line` has before the character `upto`, a tab counted as the columns to the
/// next stop of `tab` columns: the column of a place.
pub(crate) fn column_of(line: &str, upto: usize, tab: usize) -> usize {
    let tab = tab.max(1);
    let mut column = 0;
    let mut chars = 0;
    for grapheme in line.graphemes(true) {
        if chars >= upto {
            break;
        }
        chars += grapheme.chars().count();
        column = if grapheme == "\t" {
            (column / tab + 1) * tab
        } else {
            column + 1
        };
    }
    column
}

/// The character of `line` at column `column` (or the end of the line where it is shorter), a
/// grapheme never split.
pub(crate) fn at_column(line: &str, column: usize, tab: usize) -> usize {
    let tab = tab.max(1);
    let mut at = 0;
    let mut chars = 0;
    for grapheme in line.graphemes(true) {
        let next = if grapheme == "\t" {
            (at / tab + 1) * tab
        } else {
            at + 1
        };
        if next > column {
            // Past the column: the nearer edge of the grapheme it falls in.
            if column - at > next - column {
                chars += grapheme.chars().count();
            }
            return chars;
        }
        at = next;
        chars += grapheme.chars().count();
    }
    chars
}
