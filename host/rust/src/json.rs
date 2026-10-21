//! JSON text as the wire writes it (specification §4.5): one line of compact
//! JSON in which every character a frame may not carry is escaped.
//!
//! A frame holds printable ASCII only. `serde_json` already escapes the
//! quotation mark, the backslash and every code point below U+0020 the way the
//! specification asks, but it writes DEL (U+007F) and everything from U+0080 up
//! as themselves. Those are escaped here, after the fact: outside a string JSON
//! is ASCII, so any such character found in the text is inside a string, where
//! `\uXXXX` (a surrogate pair from U+10000 up) means the same character.

use serde::Serialize;

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
    fn frame_text_is_the_printable_range_with_the_space() {
        assert!(is_frame_text(" ~"));
        assert!(!is_frame_text("\u{7f}"));
        assert!(!is_frame_text("a\nb"));
        assert!(!is_frame_text("\u{e9}"));
    }
}
