//! `text`: what an app needs of a string that Lua's byte strings do not give. A page counts a text in
//! UTF-16 units, as JavaScript does (the places of an edit, the length of a piece), and names files
//! in an order people read: `file9` before `file10`, `a` beside `A`.
//!
//! A Lua string is bytes, and a file that is read may not be UTF-8. These functions walk the bytes
//! they are given, and every place they say is a place in those bytes: a run of bytes that is not
//! UTF-8 counts as one character (what a page makes of it is U+FFFD, one unit and, in a message,
//! three bytes), as `text.lossy` makes it, and they do not drift from the real offsets. They also
//! walk no further than they need, so that a text sent in pieces costs the text and not the text
//! once for each piece.

use std::cmp::Ordering;

use mlua::{Lua, Table};

/// The table of `text`.
pub fn natives(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;

    // text.compare(a, b) -> -1, 0 or 1: the order names are listed in. Runs of digits compare as
    // numbers (`file9` < `file10`), capitals are not told from small letters (`Zeta` is after
    // `alpha`), and a mark or a space comes before a digit, a digit before a letter. Two names that
    // differ only in case compare as equal; `text.compare(a, b, true)` breaks that tie by the bytes,
    // so that an order made with it is the same wherever the names come from.
    table.set(
        "compare",
        lua.create_function(
            |_, (a, b, tie): (mlua::String, mlua::String, Option<bool>)| {
                let (left, right) = (a.as_bytes(), b.as_bytes());
                let (left_text, right_text) = (
                    String::from_utf8_lossy(&left),
                    String::from_utf8_lossy(&right),
                );
                let mut order = natural(&left_text, &right_text);
                if order == Ordering::Equal && tie.unwrap_or(false) {
                    order = left.as_ref().cmp(right.as_ref());
                }
                Ok(match order {
                    Ordering::Less => -1,
                    Ordering::Equal => 0,
                    Ordering::Greater => 1,
                })
            },
        )?,
    )?;

    // text.valid(s) -> whether s is UTF-8 text.
    table.set(
        "valid",
        lua.create_function(|_, s: mlua::String| Ok(std::str::from_utf8(&s.as_bytes()).is_ok()))?,
    )?;

    // text.lossy(s) -> s with every byte that is not UTF-8 replaced by U+FFFD, as a decoder that
    // does not give up does.
    table.set(
        "lossy",
        lua.create_function(|lua, s: mlua::String| {
            lua.create_string(String::from_utf8_lossy(&s.as_bytes()).as_bytes())
        })?,
    )?;

    // text.units(s[, i[, j]]) -> how many UTF-16 units the bytes i..j of s are (as `string.sub`
    // counts them; all of s by default).
    table.set(
        "units",
        lua.create_function(|_, (s, i, j): (mlua::String, Option<i64>, Option<i64>)| {
            let bytes = s.as_bytes();
            let (from, to) = span(bytes.len(), i, j);
            Ok(units_in(bytes.get(from..to).unwrap_or_default()))
        })?,
    )?;

    // text.index(s, units[, i]) -> the byte index (as `string.sub` takes them, 1 for the first) of
    // the character that starts `units` UTF-16 units after the byte i of s (the start by default);
    // #s + 1 for the end. nil where that is past the end, or in the middle of a character that
    // takes two units.
    table.set(
        "index",
        lua.create_function(|_, (s, units, i): (mlua::String, usize, Option<usize>)| {
            let bytes = s.as_bytes();
            let start = i.unwrap_or(1).saturating_sub(1);
            Ok(index_of(&bytes, start, units).map(|at| at + 1))
        })?,
    )?;

    // text.wire_size(s) -> how many bytes s takes in a message: the size the protocol counts, which
    // is the one of the JSON text before its escapes, each counted as the UTF-8 of the character it
    // stands for. A quotation mark, a newline or a control character is one byte, whatever escape
    // a message writes it with; `ş` is two, an emoji four.
    table.set(
        "wire_size",
        lua.create_function(|_, s: mlua::String| Ok(wire_len(&s.as_bytes())))?,
    )?;

    // text.wire_fit(s, i, budget) -> j, units: the end (as `string.sub` takes it) of the longest piece
    // of s that starts at the byte i and takes at most `budget` bytes in a message (see wire_size),
    // a piece always having one character in it; and how many UTF-16 units that piece is. A piece
    // does not end inside a character. It reads the piece and no more, so a text can be sent in
    // pieces by asking for each from where the last one ended.
    table.set(
        "wire_fit",
        lua.create_function(|_, (s, i, budget): (mlua::String, usize, usize)| {
            let (end, units) = fit(&s.as_bytes(), i.saturating_sub(1), budget);
            Ok((end, units))
        })?,
    )?;

    Ok(table)
}

/// The characters of some bytes, each with the bytes it takes: a character of valid UTF-8, or a run
/// of bytes that is not (one character, U+FFFD, for the run that `str::from_utf8` calls an error).
struct Chars<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Iterator for Chars<'_> {
    /// The character and how many bytes it takes.
    type Item = (char, usize);

    fn next(&mut self) -> Option<Self::Item> {
        // No character takes more than four bytes, so a window of four holds the next one whole.
        let window = self.bytes.get(self.at..)?;
        let window = window.get(..window.len().min(4))?;
        if window.is_empty() {
            return None;
        }
        let (ch, size) = match std::str::from_utf8(window) {
            Ok(text) => text.chars().next().map(|ch| (ch, ch.len_utf8()))?,
            Err(error) if error.valid_up_to() > 0 => {
                let text = std::str::from_utf8(window.get(..error.valid_up_to())?).ok()?;
                text.chars().next().map(|ch| (ch, ch.len_utf8()))?
            }
            // A run that is not UTF-8 (`None`: one that the end of the bytes cut short).
            Err(error) => (
                char::REPLACEMENT_CHARACTER,
                error.error_len().unwrap_or(window.len()),
            ),
        };
        self.at += size;
        Some((ch, size))
    }
}

fn chars_from(bytes: &[u8], at: usize) -> Chars<'_> {
    Chars { bytes, at }
}

/// How many bytes a character takes in a message: as the protocol counts a message, its UTF-8,
/// whichever escape writes it (specification §9.3). A run that is not UTF-8 is U+FFFD there.
fn wire_size_of(ch: char) -> usize {
    ch.len_utf8()
}

/// How many bytes `bytes` take in a message.
fn wire_len(bytes: &[u8]) -> usize {
    chars_from(bytes, 0).map(|(ch, _)| wire_size_of(ch)).sum()
}

/// The end of the longest piece of `bytes` that starts at `start` and takes at most `budget` bytes in
/// a message (one character at least), as the index of the last byte of it in `string.sub`'s terms,
/// and the piece's length in UTF-16 units.
fn fit(bytes: &[u8], start: usize, budget: usize) -> (usize, usize) {
    let (mut used, mut taken, mut units) = (0usize, 0usize, 0usize);
    for (ch, size) in chars_from(bytes, start) {
        let cost = wire_size_of(ch);
        if used + cost > budget && taken > 0 {
            break;
        }
        used += cost;
        taken += size;
        units += ch.len_utf16();
    }
    (start + taken, units)
}

/// The place (0-based) of the character that starts `units` UTF-16 units after `start`, `bytes.len()`
/// for the end; none past the end, or inside a character of two units.
fn index_of(bytes: &[u8], start: usize, units: usize) -> Option<usize> {
    bytes.get(start..)?;
    let (mut seen, mut at) = (0usize, start);
    for (ch, size) in chars_from(bytes, start) {
        if seen == units {
            return Some(at);
        }
        if seen > units {
            return None;
        }
        seen += ch.len_utf16();
        at += size;
    }
    (seen == units).then_some(bytes.len())
}

/// The bytes i..j (inclusive, 1-based, negative from the end) as the range `from..to` of a string
/// of `length` bytes.
fn span(length: usize, i: Option<i64>, j: Option<i64>) -> (usize, usize) {
    let length_i = i64::try_from(length).unwrap_or(i64::MAX);
    let place = |n: i64| if n < 0 { (length_i + n + 1).max(0) } else { n };
    let from = place(i.unwrap_or(1)).max(1) - 1;
    let to = place(j.unwrap_or(-1)).min(length_i);
    let (from, to) = (
        usize::try_from(from).unwrap_or(length),
        usize::try_from(to).unwrap_or(0),
    );
    (from.min(length), to.max(from.min(length)))
}

/// The UTF-16 units of some UTF-8 (a cut character counts as one).
fn units_in(bytes: &[u8]) -> usize {
    chars_from(bytes, 0).map(|(ch, _)| ch.len_utf16()).sum()
}

// ---- the order of names ---------------------------------------------------------------------

/// One thing a name is made of, for the order: a number, or a character.
#[derive(Debug)]
enum Piece<'a> {
    Number(&'a str),
    Mark(char),
}

fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut found = Vec::new();
    let mut digits_from: Option<usize> = None;
    for (at, ch) in text.char_indices() {
        if ch.is_ascii_digit() {
            digits_from.get_or_insert(at);
            continue;
        }
        if let Some(from) = digits_from.take() {
            found.push(Piece::Number(text.get(from..at).unwrap_or_default()));
        }
        for small in ch.to_lowercase() {
            found.push(Piece::Mark(small));
        }
    }
    if let Some(from) = digits_from {
        found.push(Piece::Number(text.get(from..).unwrap_or_default()));
    }
    found
}

/// Marks and spaces, then numbers, then letters.
fn class(piece: &Piece<'_>) -> u8 {
    match piece {
        Piece::Mark(ch) if !ch.is_alphabetic() => 0,
        Piece::Number(_) => 1,
        Piece::Mark(_) => 2,
    }
}

fn natural(a: &str, b: &str) -> Ordering {
    let (left, right) = (pieces(a), pieces(b));
    for (x, y) in left.iter().zip(right.iter()) {
        let order = class(x).cmp(&class(y)).then_with(|| match (x, y) {
            (Piece::Number(m), Piece::Number(n)) => {
                let (m, n) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                m.len().cmp(&n.len()).then_with(|| m.cmp(n))
            }
            (Piece::Mark(m), Piece::Mark(n)) => m.cmp(n),
            _ => Ordering::Equal,
        });
        if order != Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_ordered_as_people_read_them() {
        let mut names = vec![
            "file10", "b.txt", "A.txt", "file9", "_x", "file09b", "Zeta", "alpha", "10", "9", "é",
        ];
        names.sort_by(|a, b| natural(a, b).then_with(|| a.cmp(b)));
        assert_eq!(
            names,
            [
                "_x", "9", "10", "A.txt", "alpha", "b.txt", "file9", "file09b", "file10", "Zeta",
                "é"
            ]
        );
    }

    #[test]
    fn the_counting_is_in_utf16_units() {
        assert_eq!(units_in("aş😀".as_bytes()), 4);
        assert_eq!(span(5, Some(2), Some(3)), (1, 3));
        assert_eq!(span(5, None, None), (0, 5));
        assert_eq!(span(5, Some(-2), None), (3, 5));
    }

    #[test]
    fn a_run_of_bytes_that_is_not_utf8_is_one_character_and_the_places_stay_in_the_real_bytes() {
        // a, a byte that is nothing, b, and the start of a three-byte character that the end cut.
        let bytes = b"a\xffb\xe2\x82";
        assert_eq!(units_in(bytes), 4);
        assert_eq!(index_of(bytes, 0, 2), Some(2));
        assert_eq!(index_of(bytes, 0, 3), Some(3));
        assert_eq!(index_of(bytes, 0, 4), Some(5));
        assert_eq!(index_of(bytes, 0, 5), None);
        assert_eq!(index_of(bytes, 9, 0), None);
        assert_eq!(fit(bytes, 0, 100), (5, 4));
        // Each run is U+FFFD in a message: three bytes. `a` and the first run take four.
        assert_eq!(wire_len(bytes), 1 + 3 + 1 + 3);
        assert_eq!(fit(bytes, 0, 4), (2, 2));
        assert_eq!(fit(bytes, 2, 4), (5, 2));
    }

    #[test]
    fn the_size_of_a_text_in_a_message_is_the_one_the_protocol_counts() {
        for text in [
            "",
            "plain",
            "a\"b\\c/d",
            "line\nbreak\ttab\r",
            "\u{1}\u{1f}",
            "\u{7f}",
            "ş",
            "日本語",
            "😀 emoji",
            "mixed ş😀\"\n\u{7f}",
        ] {
            let wire = pencere_wire::json::to_wire(text).unwrap_or_default();
            // The quotation marks around a string are two bytes of the message, not of the text.
            assert_eq!(
                pencere_wire::json::unescaped_len(&wire),
                wire_len(text.as_bytes()) + 2,
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_text_sent_in_pieces_costs_the_text_and_not_the_text_for_each_piece() {
        let text = "ş😀abc ".repeat(400_000).into_bytes();
        let started = std::time::Instant::now();
        let (mut at, mut pieces, mut units) = (0usize, 0usize, 0usize);
        while at < text.len() {
            let (end, piece) = fit(&text, at, 100);
            assert!(end > at);
            at = end;
            units += piece;
            pieces += 1;
        }
        // Every piece asks for the text to be read from where it starts; a piece that read the rest
        // of the text, as one that decodes it all would, takes minutes here.
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        assert_eq!(at, text.len());
        assert!(pieces > 30_000, "{pieces}");
        assert_eq!(units, units_in(&text));
    }
}
