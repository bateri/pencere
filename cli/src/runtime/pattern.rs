//! `regex`: patterns that are typed by a person, for a search. The standard library's tool for text
//! is Lua's own patterns, which have no alternation and no classes of letters; a program that lets the
//! person search with a pattern of their own needs the usual kind.
//!
//! The syntax is the one of regular expressions in most languages: groups, `|`, `\d \w \s \b`,
//! classes such as `\p{L}`, lookahead and lookbehind, and back-references. A pattern that backtracks
//! for ever is stopped (an error), not waited for.

use fancy_regex::{Regex, RegexBuilder};
use mlua::{Lua, Table, UserData, UserDataMethods};

/// How many steps of backtracking a search may take before it is given up.
const BACKTRACK_LIMIT: usize = 200_000;

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
            let mut source = pattern;
            if flag("whole_word")? {
                source = format!("(?<![\\p{{L}}\\p{{N}}_])(?:{source})(?![\\p{{L}}\\p{{N}}_])");
            }
            if flag("ignore_case")? {
                source = format!("(?i){source}");
            }
            RegexBuilder::new(&source)
                .backtrack_limit(BACKTRACK_LIMIT)
                .build()
                .map(Pattern)
                .map_err(|why| super::fault(format!("regex.new: {why}")))
        })?,
    )?;

    // regex.escape(text) -> a pattern that matches the text and nothing else.
    table.set(
        "escape",
        lua.create_function(|_, text: String| Ok(fancy_regex::escape(&text).into_owned()))?,
    )?;
    Ok(table)
}

/// A pattern, compiled.
struct Pattern(Regex);

impl UserData for Pattern {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // pattern:find(text[, init]) -> first, last: where the first match from the byte `init` (1 by
        // default) is, as `string.find` says it; last is first - 1 for a match with nothing in it.
        // nil when there is none. The text is UTF-8.
        methods.add_method(
            "find",
            |_, this, (text, init): (mlua::String, Option<usize>)| {
                let text = text
                    .to_str()
                    .map_err(|_| super::fault("regex find: the text is not UTF-8"))?;
                let mut from = init.unwrap_or(1).saturating_sub(1);
                if from > text.len() {
                    return Ok((None, None));
                }
                // A start in the middle of a character is the start of the next one.
                while !text.is_char_boundary(from) {
                    from += 1;
                }
                let found = this
                    .0
                    .find_from_pos(&*text, from)
                    .map_err(|why| super::fault(format!("regex find: {why}")))?;
                Ok(match found {
                    Some(found) => (Some(found.start() + 1), Some(found.end())),
                    None => (None, None),
                })
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_word_has_no_word_letters_beside_it() {
        let find = |pattern: &str, whole: bool, text: &str| -> Vec<(usize, usize)> {
            let mut source = pattern.to_owned();
            if whole {
                source = format!("(?<![\\p{{L}}\\p{{N}}_])(?:{source})(?![\\p{{L}}\\p{{N}}_])");
            }
            let re = Regex::new(&source).expect("a pattern");
            let mut found = Vec::new();
            let mut at = 0;
            while let Some(m) = re.find_from_pos(text, at).expect("a search") {
                found.push((m.start(), m.end()));
                at = m.end().max(m.start() + 1);
            }
            found
        };
        assert_eq!(
            find("order", true, "order orders (order)_order"),
            [(0, 5), (14, 19)]
        );
        assert_eq!(find("foo\\(", true, "foo( xfoo("), [(0, 4)]);
        assert_eq!(
            find("orders?\\b", false, "order orders ordered"),
            [(0, 5), (6, 12)]
        );
    }
}
