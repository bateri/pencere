//! `text`: what an app needs of a string that Lua's byte strings do not give. A page counts a text in
//! UTF-16 units, as JavaScript does (the places of an edit, the length of a piece), and names files
//! in an order people read: `file9` before `file10`, `a` beside `A`.

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
            let Some(rest) = bytes.get(start..) else {
                return Ok(None);
            };
            let text = String::from_utf8_lossy(rest);
            let mut seen = 0usize;
            let mut at = 0usize;
            for ch in text.chars() {
                if seen == units {
                    return Ok(Some(start + at + 1));
                }
                if seen > units {
                    return Ok(None);
                }
                seen += ch.len_utf16();
                at += ch.len_utf8();
            }
            Ok((seen == units).then_some(start + rest.len() + 1))
        })?,
    )?;

    // text.wire_size(s) -> how many bytes s takes in a message to a page: JSON's escapes, and six
    // for every UTF-16 unit outside ASCII (the escape `\uXXXX`).
    table.set(
        "wire_size",
        lua.create_function(|_, s: mlua::String| {
            let text = String::from_utf8_lossy(&s.as_bytes()).into_owned();
            Ok(text.encode_utf16().map(unit_size).sum::<usize>())
        })?,
    )?;

    // text.wire_fit(s, i, budget) -> j, units: the end (as `string.sub` takes it) of the longest piece
    // of s that starts at the byte i and takes at most `budget` bytes in a message to a page, a
    // piece always having one character in it; and how many UTF-16 units that piece is. A piece
    // does not end inside a character.
    table.set(
        "wire_fit",
        lua.create_function(|_, (s, i, budget): (mlua::String, usize, usize)| {
            let bytes = s.as_bytes();
            let start = i.saturating_sub(1);
            let rest = String::from_utf8_lossy(bytes.get(start..).unwrap_or_default());
            let (mut used, mut taken, mut units) = (0usize, 0usize, 0usize);
            let mut buffer = [0u16; 2];
            for ch in rest.chars() {
                let size: usize = ch
                    .encode_utf16(&mut buffer)
                    .iter()
                    .map(|unit| unit_size(*unit))
                    .sum();
                if used + size > budget && taken > 0 {
                    break;
                }
                used += size;
                taken += ch.len_utf8();
                units += ch.len_utf16();
            }
            Ok((start + taken, units))
        })?,
    )?;

    Ok(table)
}

/// How many bytes a UTF-16 unit takes in a message to a page.
fn unit_size(unit: u16) -> usize {
    match unit {
        0x7f.. => 6,
        0x22 | 0x5c | 0x0a | 0x09 | 0x0d => 2,
        0..=0x1f => 6,
        _ => 1,
    }
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
    String::from_utf8_lossy(bytes)
        .chars()
        .map(char::len_utf16)
        .sum()
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
}
