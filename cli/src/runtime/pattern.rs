//! `regex`: patterns that are typed by a person, for a search. The standard library's tool for text
//! is Lua's own patterns, which have no alternation and no classes of letters; a program that lets the
//! person search with a pattern of their own needs the usual kind.
//!
//! The syntax is the one of regular expressions in most languages, without what needs a search to go
//! back: groups, `|`, repeats, `\d \w \s \b`, classes such as `\p{L}`, flags such as `(?i)`; no
//! back-references and no look-ahead or look-behind, which are refused with the words of what is
//! wrong. In return the time a search takes grows with the text and the size of the pattern, never
//! with the number of ways the pattern can match, so a pattern a person typed or a page sent cannot
//! hold the app.

use mlua::{Lua, Table, UserData, UserDataMethods};
use regex::{Regex, RegexBuilder};

/// How big a pattern may be once it is compiled (the size of the engine's program, in bytes). A
/// search takes time in proportion to it, and a pattern such as `(\w{1000}){1000}` is made to be big.
const SIZE_LIMIT: usize = 1 << 20;

/// How deep groups may nest in a pattern.
const NEST_LIMIT: u32 = 100;

/// How much a search for whole words may read again, as a multiple of the text and a floor. A match
/// that is a part of a word is passed over, and the search goes on from the next place; text made of
/// such matches would read the same stretch again and again. Past this the search says so and stops.
const RETRY_FACTOR: usize = 4;
const RETRY_FLOOR: usize = 1 << 16;

/// The table of `regex`.
pub fn natives(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;

    // regex.new(pattern[, { ignore_case =, whole_word = }]) -> a pattern with `:find(text[, init])`.
    // A pattern that is not one raises, with the words of what is wrong. `whole_word` is a match
    // that has no letter, digit or `_` before it or after it.
    table.set(
        "new",
        lua.create_function(|_, (pattern, options): (String, Option<Table>)| {
            let flag = |name: &str| -> mlua::Result<bool> {
                Ok(match &options {
                    Some(options) => options.get::<Option<bool>>(name)?.unwrap_or(false),
                    None => false,
                })
            };
            build(&pattern, flag("ignore_case")?, flag("whole_word")?)
                .map_err(|why| super::fault(format!("regex.new: {why}")))
        })?,
    )?;

    // regex.escape(text) -> a pattern that matches the text and nothing else.
    table.set(
        "escape",
        lua.create_function(|_, text: String| Ok(regex::escape(&text)))?,
    )?;
    Ok(table)
}

/// A pattern, compiled.
struct Pattern {
    re: Regex,
    /// Only a match with no letter, digit or `_` beside it counts.
    whole_word: bool,
}

fn build(pattern: &str, ignore_case: bool, whole_word: bool) -> Result<Pattern, String> {
    RegexBuilder::new(pattern)
        .case_insensitive(ignore_case)
        .size_limit(SIZE_LIMIT)
        .nest_limit(NEST_LIMIT)
        .build()
        .map(|re| Pattern { re, whole_word })
        .map_err(|why| readable(&why))
}

/// What is wrong with a pattern, in a line: the engine's own message has the pattern and a mark
/// under the place in it, over several lines.
fn readable(error: &regex::Error) -> String {
    match error {
        regex::Error::Syntax(text) => {
            let said = text
                .lines()
                .rev()
                .find_map(|line| line.trim().strip_prefix("error:"))
                .map_or(text.as_str(), str::trim);
            if said.contains("backreferences") {
                "this pattern uses a back-reference, which search does not support".to_owned()
            } else if said.contains("look-around") {
                "this pattern uses look-ahead or look-behind, which search does not support"
                    .to_owned()
            } else {
                said.to_owned()
            }
        }
        regex::Error::CompiledTooBig(_) => "the pattern is too big".to_owned(),
        other => other.to_string(),
    }
}

/// Whether a character is part of a word: a letter, a digit or `_`.
fn is_word(ch: char) -> bool {
    ch == '_' || ch.is_alphanumeric()
}

/// Whether the text from `start` to `end` has no part of a word beside it.
fn is_whole(text: &str, start: usize, end: usize) -> bool {
    let before = text.get(..start).and_then(|head| head.chars().next_back());
    let after = text.get(end..).and_then(|tail| tail.chars().next());
    !before.is_some_and(is_word) && !after.is_some_and(is_word)
}

/// The first place at or after `at` that has no part of a word before it, none where there is no
/// such place left.
fn next_edge(text: &str, at: usize) -> Option<usize> {
    if !text.get(..at)?.chars().next_back().is_some_and(is_word) {
        return Some(at);
    }
    // The place after a character is one when the character is not part of a word.
    text.get(at..)?
        .char_indices()
        .find(|(_, ch)| !is_word(*ch))
        .map(|(offset, ch)| at + offset + ch.len_utf8())
}

impl Pattern {
    /// The first match from the byte `from` (a place between characters), as the places it starts
    /// and ends.
    fn find(&self, text: &str, from: usize) -> Result<Option<(usize, usize)>, String> {
        if !self.whole_word {
            return Ok(self
                .re
                .find_at(text, from)
                .map(|found| (found.start(), found.end())));
        }
        // The search goes where a whole word can begin, and a match that is a part of a word is
        // passed over: the next try is from the character after its start.
        let budget = text
            .len()
            .saturating_mul(RETRY_FACTOR)
            .saturating_add(RETRY_FLOOR);
        let mut spent = 0usize;
        let mut at = from;
        loop {
            let Some(place) = next_edge(text, at) else {
                return Ok(None);
            };
            let Some(found) = self.re.find_at(text, place) else {
                return Ok(None);
            };
            if is_whole(text, found.start(), found.end()) {
                return Ok(Some((found.start(), found.end())));
            }
            spent = spent.saturating_add(found.end().saturating_sub(place) + 1);
            if spent > budget {
                return Err(
                    "the search gave up: this text has too many places where the pattern matches a part of a word"
                        .to_owned(),
                );
            }
            let Some(ch) = text
                .get(found.start()..)
                .and_then(|tail| tail.chars().next())
            else {
                return Ok(None);
            };
            at = found.start() + ch.len_utf8();
        }
    }
}

impl UserData for Pattern {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // pattern:find(text[, init]) -> first, last: where the first match from the byte `init` (1 by
        // default; negative counts from the end, as `string.find` does) is, as `string.find` says it;
        // last is first - 1 for a match with nothing in it. nil when there is none. The text is UTF-8.
        methods.add_method(
            "find",
            |_, this, (text, init): (mlua::String, Option<i64>)| {
                let text = text
                    .to_str()
                    .map_err(|_| super::fault("regex find: the text is not UTF-8"))?;
                let length = i64::try_from(text.len()).unwrap_or(i64::MAX);
                let init = init.unwrap_or(1);
                let from = match init {
                    1.. => init - 1,
                    0 => 0,
                    _ => (length + init).max(0),
                };
                let Ok(mut from) = usize::try_from(from) else {
                    return Ok((None, None));
                };
                if from > text.len() {
                    return Ok((None, None));
                }
                // A start in the middle of a character is the start of the next one.
                while !text.is_char_boundary(from) {
                    from += 1;
                }
                let found = this
                    .find(&text, from)
                    .map_err(|why| super::fault(format!("regex find: {why}")))?;
                Ok(match found {
                    Some((start, end)) => (Some(start + 1), Some(end)),
                    None => (None, None),
                })
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The places a pattern finds one after the other, as an app that goes through a text does.
    fn all(pattern: &str, ignore_case: bool, whole: bool, text: &str) -> Vec<(usize, usize)> {
        let pattern = build(pattern, ignore_case, whole).unwrap_or_else(|why| panic!("{why}"));
        let mut found = Vec::new();
        let mut at = 0;
        while let Some((start, end)) = pattern.find(text, at).unwrap_or_else(|why| panic!("{why}"))
        {
            found.push((start, end));
            at = end.max(start + 1);
        }
        found
    }

    #[test]
    fn a_whole_word_has_no_word_letters_beside_it() {
        assert_eq!(
            all("order", false, true, "order orders (order)_order"),
            [(0, 5), (14, 19)]
        );
        assert_eq!(all("foo\\(", false, true, "foo( xfoo("), [(0, 4)]);
        assert_eq!(
            all("orders?\\b", false, false, "order orders ordered"),
            [(0, 5), (6, 12)]
        );
        // Letters and digits of every script are parts of a word.
        assert_eq!(all("ab", false, true, "éab ab aé"), [(5, 7)]);
        // The word is looked at from where the search begins, too: the `a` of `ba` is not a word.
        let pattern = build("a", false, true).unwrap_or_else(|why| panic!("{why}"));
        assert_eq!(pattern.find("ba a", 1), Ok(Some((3, 4))));
    }

    #[test]
    fn what_needs_a_search_to_go_back_is_refused_in_words() {
        let why = |pattern: &str| match build(pattern, false, false) {
            Ok(_) => String::new(),
            Err(why) => why,
        };
        assert_eq!(
            why("(a)\\1"),
            "this pattern uses a back-reference, which search does not support"
        );
        assert_eq!(
            why("(?<=a)b"),
            "this pattern uses look-ahead or look-behind, which search does not support"
        );
        assert_eq!(
            why("a(?!b)"),
            "this pattern uses look-ahead or look-behind, which search does not support"
        );
        assert_eq!(why("("), "unclosed group");
        assert_eq!(why("(\\w{1000}){1000}"), "the pattern is too big");
        // A group cannot be left by a pattern that is a part of a larger one: it is a pattern alone.
        assert_eq!(why("a)|(b"), "unopened group");
    }

    #[test]
    fn a_pattern_that_would_backtrack_for_ever_is_searched_in_the_time_of_the_text() {
        let started = std::time::Instant::now();
        let text = format!("{}b", "a".repeat(200_000));
        assert_eq!(all("(a+)+$", false, false, &text), []);
        assert_eq!(all("(a|aa)+$", false, false, &text), []);
        assert_eq!(all("(x+x+)+y", false, false, &"x".repeat(200_000)), []);
        assert!(started.elapsed() < std::time::Duration::from_secs(30));
    }

    #[test]
    fn a_search_for_whole_words_over_text_made_of_parts_of_words_stops_instead_of_going_on() {
        // Every match is the run of spaces and the `x` after it, and a word follows the `x`: the
        // match is passed over, and the next try is from the next space.
        let text = format!("{}xy", " ".repeat(100_000));
        let pattern = build(" *x", false, true).unwrap_or_else(|why| panic!("{why}"));
        let started = std::time::Instant::now();
        let outcome = pattern.find(&text, 0);
        assert!(outcome.is_err(), "{outcome:?}");
        assert!(started.elapsed() < std::time::Duration::from_secs(30));
        // A run of one word is passed over in a step, not one for each of its letters.
        let word = format!("{}b c", "a".repeat(100_000));
        let pattern = build("a+", false, true).unwrap_or_else(|why| panic!("{why}"));
        assert_eq!(pattern.find(&word, 0), Ok(None));
    }
}
