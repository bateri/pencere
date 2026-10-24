//! The words a program supplies for a terminal to draw (specification §7.3): its name, the
//! name of its view, the words a failed view ends with. The program wrote them, so they are
//! cut and cleaned of what a terminal would act on before the session passes them on.

use crate::session::MAX_LABEL_CHARS;

/// Whether `c` is a format character (Unicode general category Cf): a character
/// that is no mark on the screen but does something to what is next to it, or to
/// nothing at all. The ranges are Unicode 16's, written out because the standard
/// library has no general categories and the crate takes no data table for one.
fn is_format(c: char) -> bool {
    matches!(
        c as u32,
        0x00AD
            | 0x0600..=0x0605
            | 0x061C
            | 0x06DD
            | 0x070F
            | 0x0890..=0x0891
            | 0x08E2
            | 0x180E
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x206F
            | 0xFEFF
            | 0xFFF9..=0xFFFB
            | 0x110BD
            | 0x110CD
            | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0001
            | 0xE0020..=0xE007F
    )
}

/// Whether `c` stays out of a name the terminal draws: control characters, which
/// let a program type into the terminal or end a line, the line and paragraph
/// separators, and the format characters, which let it show text other than what
/// it wrote (the bidirectional embeddings, overrides and isolates) or make two
/// names that differ look like one (the zero-width space, the word joiner, the
/// byte order mark, the Arabic letter mark, the soft hyphen, the tag characters).
///
/// Four format characters stay, because text needs them:
///
/// - the zero-width joiner U+200D, which makes an emoji sequence one emoji (a
///   person at a laptop) and shapes several scripts, and the zero-width non-joiner
///   U+200C, which Persian, Kurdish and others write inside words;
/// - the left-to-right and right-to-left marks U+200E and U+200F, which a name
///   or title in a right-to-left script needs for the punctuation beside it to
///   land where its author put it. They only settle how neutral characters at the
///   edge of a run are placed, and, unlike the embeddings and overrides, cannot
///   make characters read in another order or hide them.
///
/// These four are as invisible as what is left out, so a program can still give
/// two tools names that differ and look alike: a name is for people to read, and
/// tells two programs apart no better than the program's say-so does. What this
/// closes is the rest of the invisible characters, which have no use in a name.
/// The three flags of England, Scotland and Wales are written with tag characters
/// and lose them, and show as a black flag.
fn is_hostile(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{2028}' | '\u{2029}')
        || (is_format(c) && !matches!(c, '\u{200c}'..='\u{200f}'))
}

/// A name the program supplied, as the host passes it to the terminal: without
/// the characters of [`is_hostile`], at most [`MAX_LABEL_CHARS`] characters (cut
/// at a character, not at a grapheme) and without the blanks at its ends. `None`
/// when nothing is left.
pub(crate) fn label(text: &str) -> Option<String> {
    words(text, MAX_LABEL_CHARS)
}

/// `text` without the characters of [`is_hostile`], at most `most` characters and without the
/// blanks at its ends: what a label is, for any length. `None` when nothing is left.
pub(crate) fn words(text: &str, most: usize) -> Option<String> {
    let kept: String = text
        .chars()
        .filter(|c| !is_hostile(*c))
        .take(most)
        .collect();
    let kept = kept.trim();
    (!kept.is_empty()).then(|| kept.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_keeps_what_people_write() {
        assert_eq!(label("Containers").as_deref(), Some("Containers"));
        assert_eq!(
            label("  Café \u{2014} cache  ").as_deref(),
            Some("Café \u{2014} cache")
        );
        // A joiner is part of an emoji and of several scripts; the marks of
        // right-to-left text stay too.
        assert_eq!(
            label("a\u{200d}b\u{200f}").as_deref(),
            Some("a\u{200d}b\u{200f}")
        );
    }

    /// Every character of the Unicode 16 general category Cf, as ranges, from the Unicode
    /// Character Database: written out here from the source, not from `is_format`.
    const CF: [(u32, u32); 21] = [
        (0x00AD, 0x00AD),
        (0x0600, 0x0605),
        (0x061C, 0x061C),
        (0x06DD, 0x06DD),
        (0x070F, 0x070F),
        (0x0890, 0x0891),
        (0x08E2, 0x08E2),
        (0x180E, 0x180E),
        (0x200B, 0x200F),
        (0x202A, 0x202E),
        (0x2060, 0x2064),
        (0x2066, 0x206F),
        (0xFEFF, 0xFEFF),
        (0xFFF9, 0xFFFB),
        (0x110BD, 0x110BD),
        (0x110CD, 0x110CD),
        (0x13430, 0x1343F),
        (0x1BCA0, 0x1BCA3),
        (0x1D173, 0x1D17A),
        (0xE0001, 0xE0001),
        (0xE0020, 0xE007F),
    ];

    #[test]
    fn a_label_loses_the_invisible_characters_that_make_two_names_look_like_one() {
        // The word joiner, the zero-width space, the byte order mark and the Arabic letter
        // mark among them: a name with one in it looks like the name without.
        for (first, last) in CF {
            for code in first..=last {
                let c = char::from_u32(code).unwrap();
                if matches!(c, '\u{200c}'..='\u{200f}') {
                    continue;
                }
                let text = format!("Cont{c}ainers");
                assert_eq!(label(&text).as_deref(), Some("Containers"), "U+{code:04X}");
            }
        }
        // What the ends of a name hold is no part of it either.
        assert_eq!(
            label("\u{feff}\u{200b}Containers\u{2060}").as_deref(),
            Some("Containers")
        );
        assert_eq!(label("\u{200b}\u{2060}\u{feff}"), None);
    }

    #[test]
    fn a_label_keeps_the_joiners_and_the_marks_that_text_needs() {
        // An emoji sequence is one emoji by its joiners; Persian is written with the non-joiner.
        let emoji = "\u{1f469}\u{200d}\u{1f4bb} laptop";
        assert_eq!(label(emoji).as_deref(), Some(emoji));
        let persian = "\u{645}\u{6cc}\u{200c}\u{62e}\u{648}\u{627}\u{647}\u{645}";
        assert_eq!(label(persian).as_deref(), Some(persian));
        // A right-to-left name with its punctuation placed by the mark.
        let hebrew = "\u{5e9}\u{5dc}\u{5d5}\u{5dd}\u{200f} (beta)";
        assert_eq!(label(hebrew).as_deref(), Some(hebrew));
        let ltr = "a\u{200e}b";
        assert_eq!(label(ltr).as_deref(), Some(ltr));
    }

    #[test]
    fn a_label_loses_what_would_act_in_the_terminal_or_hide_what_it_says() {
        for hostile in [
            "\u{1b}", "\u{7}", "\n", "\r", "\t", "\u{0}", "\u{7f}", "\u{9b}", "\u{2028}",
            "\u{2029}", "\u{202a}", "\u{202d}", "\u{202e}", "\u{2066}", "\u{2069}",
        ] {
            let text = format!("a{hostile}b");
            assert_eq!(label(&text).as_deref(), Some("ab"), "{hostile:?}");
        }
        assert_eq!(label("\u{1b}[2J").as_deref(), Some("[2J"));
    }

    #[test]
    fn a_label_is_cut_at_a_character_and_what_it_loses_does_not_count() {
        let long = "\u{e9}".repeat(MAX_LABEL_CHARS + 50);
        let cut = label(&long).unwrap();
        assert_eq!(cut.chars().count(), MAX_LABEL_CHARS);
        let buried = format!("{}ok", "\u{1}".repeat(10_000));
        assert_eq!(label(&buried).as_deref(), Some("ok"));
        // The blanks the cut leaves at its end go too.
        let spaced = format!("{}{}", "a".repeat(MAX_LABEL_CHARS - 1), " b");
        assert_eq!(label(&spaced), Some("a".repeat(MAX_LABEL_CHARS - 1)));
    }

    #[test]
    fn nothing_left_is_no_label() {
        for blank in ["", " ", "\u{1b}\u{7}", " \n\t "] {
            assert_eq!(label(blank), None, "{blank:?}");
        }
    }
}
