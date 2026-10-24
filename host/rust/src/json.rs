//! JSON text as the wire writes it (specification §4.5): one line of compact
//! JSON in which every character a frame may not carry is escaped.
//!
//! A frame holds printable ASCII only. `serde_json` already escapes the
//! quotation mark, the backslash and every code point below U+0020 the way the
//! specification asks, but it writes DEL (U+007F) and everything from U+0080 up
//! as themselves. Those are escaped here, after the fact: outside a string JSON
//! is ASCII, so any such character found in the text is inside a string, where
//! `\uXXXX` (a surrogate pair from U+10000 up) means the same character.

use std::marker::PhantomData;

use serde::de::value::MapAccessDeserializer;
use serde::de::{Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};

/// Serialises `value` as compact JSON text that is printable ASCII throughout.
///
/// # Errors
///
/// Returns the serialiser's error when `value` cannot be represented as JSON
/// (a map with non-string keys, for instance).
pub fn to_wire<T: Serialize + ?Sized>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_string(value).map(|text| escape(&text))
}

/// Escapes DEL and every character from U+0080 up in already serialised JSON
/// text (see the module documentation for why that is sound).
pub fn escape(text: &str) -> String {
    if text.bytes().all(|byte| byte < 0x7f) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() + 16);
    let mut units = [0u16; 2];
    for ch in text.chars() {
        if ch == '\u{7f}' || u32::from(ch) >= 0x80 {
            for unit in ch.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// JSON text that is valid, as a frame can carry it: the whitespace between its tokens that a
/// frame has no byte for (tab, line feed and carriage return) left out, and [`escape`]d.
///
/// This is for a message of a page or of a view the terminal draws itself, which comes as text
/// anyone may have written and goes into the program's input as it is. In JSON text those
/// three characters are whitespace between tokens and nothing else (inside a string they are
/// written `\t`, `\n` and `\r`), so dropping them changes no value. The text has to be JSON
/// already: for anything else this would change what it says.
pub fn to_frame(text: &str) -> String {
    if text
        .bytes()
        .any(|byte| matches!(byte, b'\t' | b'\n' | b'\r'))
    {
        let compact: String = text
            .chars()
            .filter(|ch| !matches!(ch, '\t' | '\n' | '\r'))
            .collect();
        escape(&compact)
    } else {
        escape(text)
    }
}

/// How many bytes JSON text has once its `\uXXXX` and other escapes are undone: the length of the
/// text a message is, before the escapes of the wire (specification §4.5) made it longer. Each
/// escape counts as the UTF-8 of the character it stands for (a surrogate pair as one character
/// of four bytes), and every other byte as itself.
///
/// This reads text that may not be JSON: a backslash that does not begin an escape counts as
/// itself, and nothing is checked. It is the size of a message in either direction (§9.3), so
/// a program that has to write ASCII is not held to less than a page that need not.
pub fn unescaped_len(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut len = 0usize;
    let mut at = 0usize;
    while let Some(&byte) = bytes.get(at) {
        if byte != b'\\' {
            len += 1;
            at += 1;
            continue;
        }
        match bytes.get(at + 1) {
            Some(b'u') => match unit_at(bytes, at + 2) {
                Some(0xD800..=0xDBFF)
                    if matches!(bytes.get(at + 6..at + 8), Some([b'\\', b'u']))
                        && matches!(unit_at(bytes, at + 8), Some(0xDC00..=0xDFFF)) =>
                {
                    len += 4;
                    at += 12;
                }
                Some(unit) => {
                    len += match unit {
                        0..=0x7F => 1,
                        0x80..=0x7FF => 2,
                        _ => 3,
                    };
                    at += 6;
                }
                None => {
                    len += 1;
                    at += 1;
                }
            },
            Some(_) => {
                len += 1;
                at += 2;
            }
            None => {
                len += 1;
                at += 1;
            }
        }
    }
    len
}

/// The code unit written as four hex digits at `at`.
fn unit_at(bytes: &[u8], at: usize) -> Option<u32> {
    let digits = bytes.get(at..at + 4)?;
    digits.iter().try_fold(0u32, |unit, digit| {
        Some(unit * 16 + char::from(*digit).to_digit(16)?)
    })
}

/// Whether JSON text is an object (its first character is `{`, whitespace aside).
///
/// A struct that serde reads can also be read from an array of its fields in
/// order; every payload of this protocol is an object and nothing else is taken
/// for one.
pub fn is_object(text: &str) -> bool {
    text.trim_start_matches([' ', '\t', '\n', '\r'])
        .starts_with('{')
}

/// Reads an object as `T`; `None` for any other JSON, and for JSON that is not
/// a `T`.
pub fn from_object<T: for<'de> serde::Deserialize<'de>>(text: &str) -> Option<T> {
    if is_object(text) {
        serde_json::from_str(text).ok()
    } else {
        None
    }
}

/// A `T` that is read from a JSON object and from nothing else. A struct that serde reads can
/// also be read from an array of its fields in order (see [`is_object`]); this is for the
/// members of a payload, where an array standing for an object is not one.
pub(crate) struct Object<T>(pub T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for V<T> {
            type Value = Object<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Object<T>, A::Error> {
                T::deserialize(MapAccessDeserializer::new(map)).map(Object)
            }
        }
        deserializer.deserialize_map(V(PhantomData))
    }
}

/// Whether every byte of `text` may appear in a frame: printable ASCII, the
/// space included (0x20 to 0x7E).
pub fn is_frame_text(text: &str) -> bool {
    text.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn escapes_what_serde_json_leaves_raw() {
        let value = json!({ "a": "caf\u{e9} \u{7f} \u{2700} \u{1f600} \"q\" \\ \n\t\u{1}" });
        let text = to_wire(&value).unwrap();
        assert!(is_frame_text(&text), "{text}");
        let u = |code: &str| format!("{}u{code}", '\\');
        let expected = format!(
            "{{\"a\":\"caf{} {} {} {}{} \\\"q\\\" \\\\ \\n\\t{}\"}}",
            u("00e9"),
            u("007f"),
            u("2700"),
            u("d83d"),
            u("de00"),
            u("0001"),
        );
        assert_eq!(text, expected);
        // The same value comes back.
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&text).unwrap(),
            value
        );
    }

    #[test]
    fn whitespace_between_tokens_is_left_out_and_the_value_is_the_same() {
        let text = "{\r\n\t\"a\" :\t[ 1,\n2 ],\r\n  \"b\": \"caf\u{e9} \\t \\n\"\n}\n";
        assert!(!is_frame_text(text));
        let wire = to_frame(text);
        assert!(is_frame_text(&wire), "{wire:?}");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&wire).unwrap(),
            serde_json::from_str::<serde_json::Value>(text).unwrap()
        );
        // A space is a byte a frame has, and compact text is untouched.
        assert_eq!(to_frame(r#"{"a": [1, 2]}"#), r#"{"a": [1, 2]}"#);
    }

    #[test]
    fn the_size_of_a_message_is_that_of_its_text_before_the_escapes() {
        let text = "{\"a\":\"caf\u{e9} \u{7f} \u{2700} \u{1f600}\"}";
        assert_eq!(unescaped_len(text), text.len());
        // The wire's form is longer, the size is the same.
        let wire = to_wire(&serde_json::from_str::<serde_json::Value>(text).unwrap()).unwrap();
        assert!(wire.len() > text.len());
        assert_eq!(unescaped_len(&wire), text.len());
        // Escapes of one character: a pair is four bytes, a quote and a backslash one each.
        assert_eq!(unescaped_len(r"\ud83d\ude00"), 4);
        assert_eq!(unescaped_len(r"\u00e9\u2700\u007f"), 2 + 3 + 1);
        assert_eq!(unescaped_len(r#"\"\\\/\n"#), 4);
        // What is not an escape counts as itself, whatever it is.
        assert_eq!(unescaped_len(r"\u12"), 4);
        assert_eq!(unescaped_len(r"\ud83d"), 3);
        assert_eq!(unescaped_len(r"\ud83dx"), 4);
        assert_eq!(unescaped_len("\\"), 1);
        assert_eq!(unescaped_len(""), 0);
    }

    #[test]
    fn plain_ascii_is_untouched() {
        assert_eq!(escape(r#"{"a":[1,2,"x y"]}"#), r#"{"a":[1,2,"x y"]}"#);
    }

    #[test]
    fn only_an_object_is_taken_for_one() {
        assert!(is_object(r#"{"a":1}"#));
        assert!(is_object(" \n{}"));
        assert!(!is_object("[1]"));
        assert!(!is_object(""));
        assert!(!is_object("\"{\""));
        #[derive(serde::Deserialize)]
        struct Pair {
            #[allow(dead_code)]
            a: u32,
        }
        assert!(from_object::<Pair>(r#"{"a":1}"#).is_some());
        assert!(from_object::<Pair>("[1]").is_none());
        assert!(from_object::<Pair>(r#"{"a":"x"}"#).is_none());
    }

    #[test]
    fn an_object_member_is_not_read_from_an_array() {
        #[derive(Deserialize)]
        struct Pair {
            a: u32,
            b: u32,
        }
        let read = |text: &str| serde_json::from_str::<Object<Pair>>(text).map(|p| (p.0.a, p.0.b));
        assert_eq!(read(r#"{"b":2,"a":1,"c":[{}]}"#).unwrap(), (1, 2));
        assert!(read("[1,2]").is_err());
        assert!(read("7").is_err());
        assert!(read(r#"{"a":1}"#).is_err());
    }

    #[test]
    fn frame_text_is_the_printable_range_with_the_space() {
        assert!(is_frame_text(" ~"));
        assert!(!is_frame_text("\u{7f}"));
        assert!(!is_frame_text("a\nb"));
        assert!(!is_frame_text("\u{e9}"));
    }
}
