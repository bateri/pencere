//! What a view says to the program's log: the reserved call `pencere.log` (specification §9.2).
//!
//! A page has no terminal of its own, and the one under it is the program's, so a page's `print`
//! is a call of the program: the library answers it itself, with no handler of the program's, and
//! hands the line to [`Options::log`](crate::Options::log). The text is the view's and is read as
//! untrusted: it reaches the program with each control character but a tab and a line break
//! written out (`\x1b`), so that a view cannot act on a terminal that shows the log.

use std::borrow::Cow;
use std::fmt::Write as _;

use serde_json::Value;

/// The method of the call that says a line of the view's log.
pub(crate) const METHOD: &str = "pencere.log";

/// The names of methods and topics that begin with this belong to the protocol, not to a program
/// (specification §9.2): `serve` refuses a program that gives one.
pub const RESERVED_PREFIX: &str = "pencere.";

/// How serious a line of a view's log is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LogLevel {
    /// What the view says: its `print`.
    Info,
    /// What the view warns of: its `warn`.
    Warn,
}

/// What takes the lines of a view's log.
pub(crate) type LogFn = dyn Fn(LogLevel, &str) + Send + Sync;

/// The line a call of [`METHOD`] carries, or none when its `params` have no text. A `level` that is
/// not `"warn"` is `"info"`, whatever it is.
pub(crate) fn read(params: &Value) -> Option<(LogLevel, &str)> {
    let text = params.get("text")?.as_str()?;
    let level = match params.get("level").and_then(Value::as_str) {
        Some("warn") => LogLevel::Warn,
        _ => LogLevel::Info,
    };
    Some((level, text))
}

/// `text` with each control character but a tab and a line break written as `\xNN` (two lowercase
/// hexadecimal digits: `ESC` is `\x1b`, U+0085 is `\x85`).
pub(crate) fn visible(text: &str) -> Cow<'_, str> {
    let hidden = |c: char| c.is_control() && c != '\t' && c != '\n';
    if !text.chars().any(hidden) {
        return Cow::Borrowed(text);
    }
    let mut shown = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        if hidden(c) {
            // Every control character is below U+00A0: two digits are enough.
            let _ = write!(shown, "\\x{:02x}", u32::from(c));
        } else {
            shown.push(c);
        }
    }
    Cow::Owned(shown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_control_character_but_a_tab_and_a_line_break_is_written_out() {
        assert_eq!(
            visible("a\x1b[31mred\x1b[0m\r\nb\tc\0\u{7f}\u{85}"),
            "a\\x1b[31mred\\x1b[0m\\x0d\nb\tc\\x00\\x7f\\x85"
        );
    }

    #[test]
    fn text_with_nothing_to_write_out_is_not_copied() {
        assert!(matches!(visible("héllo 🎉\tworld\n"), Cow::Borrowed(_)));
    }

    #[test]
    fn a_level_that_is_not_warn_is_info_and_text_that_is_not_a_string_is_no_line() {
        assert_eq!(
            read(&json!({ "text": "a", "level": "warn" })),
            Some((LogLevel::Warn, "a"))
        );
        assert_eq!(
            read(&json!({ "text": "a", "level": "loud" })),
            Some((LogLevel::Info, "a"))
        );
        assert_eq!(
            read(&json!({ "text": "a", "level": 3 })),
            Some((LogLevel::Info, "a"))
        );
        assert_eq!(read(&json!({ "text": "a" })), Some((LogLevel::Info, "a")));
        assert_eq!(read(&json!({ "text": 1 })), None);
        assert_eq!(read(&json!({ "level": "warn" })), None);
        assert_eq!(read(&json!("text")), None);
        assert_eq!(read(&Value::Null), None);
    }
}
