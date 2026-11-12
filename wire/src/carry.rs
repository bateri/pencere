//! What a view may hand to its host's system drag (specification §9.8, §12.3): the text or the path
//! of something the person drags out of the view, and an address that goes with it. It leaves the
//! view by the person's own act, as a `copy` does, and is held to a rule of its own: one line, no
//! control characters, at most 64 KiB, and an address only of the kind a `link` may be. This is the
//! rule's one copy: the sandbox refuses a page that breaks it and a host does not hand on what
//! does.

use crate::link;

/// The most bytes the text or the path of a drag may have: what a reply's `copy` may have of a text
/// at the least a host gives a page (64 KiB).
pub const MAX_CARRY: usize = 64 << 10;

/// Whether `words` may be the text or the path of a drag out of a view: not empty, at most
/// [`MAX_CARRY`] bytes, all of it on one line. A line break, a control character (the C0 ones, the
/// delete character and the C1 ones, a tab among them), the separators of lines and paragraphs and
/// the controls of the direction of text are not allowed, so that what a terminal pane is handed can never be a second command.
pub fn is_carriable(words: &str) -> bool {
    !words.is_empty()
        && words.len() <= MAX_CARRY
        && !words.chars().any(|c| c.is_control() || is_unseen(c))
}

/// A character that is no control of the line and yet makes words look other than they are, or
/// breaks them where no key does: the separators of lines and paragraphs, the byte order mark, and
/// the embeddings, overrides and isolates of the direction of text (U+202A to U+202E, U+2066 to
/// U+2069).
fn is_unseen(c: char) -> bool {
    matches!(c, '\u{2028}' | '\u{2029}' | '\u{feff}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

/// Whether `url` may go with a drag: an address a view may ask its host to open ([`link::is_openable`]).
pub fn is_carriable_url(url: &str) -> bool {
    link::is_openable(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_line_of_any_script_is_carried() {
        for good in [
            "/Users/ayşe/Belgeler/rapor.pdf",
            "a b",
            "echo 'hi'",
            "日本語",
            "😀",
            "👨\u{200d}👩\u{200d}👧",
            "right to left: \u{05e9}\u{05dc}\u{05d5}\u{05dd}",
            "with trailing space ",
        ] {
            assert!(is_carriable(good), "{good:?}");
        }
    }

    #[test]
    fn nothing_that_ends_a_line_or_moves_a_terminal_is_carried() {
        for bad in [
            "",
            "a\nb",
            "a\r\nb",
            "a\rb",
            "tab\there",
            "nul\0",
            "esc\u{1b}[31m",
            "bell\u{7}",
            "delete\u{7f}",
            "next line\u{85}",
            "c1\u{9b}31m",
            "line\u{2028}separator",
            "paragraph\u{2029}separator",
            "\u{feff}",
            "\n",
            "\u{202e}gnp.exe",
            "a\u{202a}b",
            "a\u{202d}b",
            "a\u{2066}b",
            "a\u{2069}b",
        ] {
            assert!(!is_carriable(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_carried_text_is_at_most_64_kib() {
        assert!(is_carriable(&"a".repeat(MAX_CARRY)));
        assert!(!is_carriable(&"a".repeat(MAX_CARRY + 1)));
        // Bytes, not characters.
        assert!(!is_carriable(&"é".repeat(MAX_CARRY / 2 + 1)));
    }

    #[test]
    fn an_address_that_goes_with_a_drag_is_one_a_link_may_be() {
        assert!(is_carriable_url("https://example.com/a?b=c"));
        assert!(!is_carriable_url("file:///etc/passwd"));
        assert!(!is_carriable_url("javascript:alert(1)"));
        assert!(!is_carriable_url("https://example.com/\n"));
        assert!(!is_carriable_url(""));
    }
}
