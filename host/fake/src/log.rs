//! The record of what happened, one line each, on stderr: `program:` (what the
//! program wrote and how it ended), `host:` (what this host decided and wrote
//! into the pane's input), `view:` (what the scripted view received and sent)
//! and `screen:` (the alternate screen).
//!
//! The lines are the product of this program: a person reads them to see the
//! order of the conversation, and the end-to-end tests read them to check it.
//! They never carry a token.

use std::fmt::Display;
use std::io::{self, Write};

/// `text` with every control character written out as an escape. Some of what
/// the record says was written by the program (a tool's name, a reason), and
/// the record goes to the person's terminal: a program must not be able to
/// type into that terminal, or to end a line and forge the next one.
fn clean(text: &str) -> String {
    let mut clean = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_control() {
            clean.extend(c.escape_default());
        } else {
            clean.push(c);
        }
    }
    clean
}

pub fn line(channel: &str, text: impl Display) {
    // A closed stderr loses lines and nothing else.
    let _ = writeln!(
        io::stderr().lock(),
        "{channel}: {}",
        clean(&text.to_string())
    );
}

#[cfg(test)]
mod tests {
    use super::clean;

    #[test]
    fn a_program_cannot_put_a_control_character_in_the_record() {
        assert_eq!(clean("open tool=\u{1b}[2J"), "open tool=\\u{1b}[2J");
        assert_eq!(clean("a\x1b[2Jb"), "a\\u{1b}[2Jb");
        assert_eq!(clean("one\ntwo\r"), "one\\ntwo\\r");
        assert_eq!(clean("● web 8080→80"), "● web 8080→80");
    }
}
