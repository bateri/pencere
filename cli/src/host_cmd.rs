//! `pencere host`: which terminal is this? The terminal's own answer to the handshake, printed as
//! `<name> <version>`, for finding out where an app runs and for a bug report. Nothing is shown in
//! the pane: the question is the one every program asks first, and the view is never opened.

use std::process::ExitCode;

/// The characters that reorder or hide text, and say nothing a person reads: soft hyphen, zero-width
/// and direction marks, the line and paragraph separators, the isolates and embeddings, the BOM.
fn unseen(character: char) -> bool {
    matches!(
        character,
        '\u{ad}'
            | '\u{200b}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2060}'..='\u{206f}'
            | '\u{feff}'
    )
}

/// The words of the terminal, with each control character, each one that hides or reorders text, and
/// each backslash written out (`\x1b` for `ESC`, `\u{202e}`): they are the terminal's, read as
/// untrusted, and this prints them where a terminal may be reading.
fn plain(text: &str) -> String {
    let mut written = String::with_capacity(text.len());
    for character in text.chars() {
        if character.is_control() || character == '\\' {
            written.push_str(&format!("\\x{:02x}", u32::from(character)));
        } else if unseen(character) {
            written.push_str(&format!("\\u{{{:x}}}", u32::from(character)));
        } else {
            written.push(character);
        }
    }
    written
}

/// Asks the terminal this command runs in, and says what it answered: its name and version on the
/// standard output (so that `$(pencere host)` has them), and where there is no pencere host in the
/// terminal, a sentence on the standard error and the status 1; when the question itself failed
/// (the terminal could not be taken), the status 2.
pub fn command(args: &[String]) -> ExitCode {
    if let Some(argument) = args.first() {
        return crate::usage(format!("host takes no arguments, not {argument:?}"));
    }
    match pencere::ask_host() {
        Ok(Some(host)) => {
            println!("{} {}", plain(&host.name), plain(&host.version));
            ExitCode::SUCCESS
        }
        Ok(None) => {
            eprintln!("pencere: no pencere host in this terminal");
            ExitCode::from(1)
        }
        // ⌃C while it waited: the status a shell gives it, and nothing to say.
        Err(pencere::Error::Io(why)) if why.kind() == std::io::ErrorKind::Interrupted => {
            ExitCode::from(130)
        }
        Err(why) => {
            eprintln!("pencere: {why}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::plain;

    #[test]
    fn the_words_of_a_terminal_are_written_out_where_they_would_act() {
        assert_eq!(plain("bateri"), "bateri");
        assert_eq!(plain("0.9.1 (build 7)"), "0.9.1 (build 7)");
        assert_eq!(plain("a\x1b[2Jb\n"), "a\\x1b[2Jb\\x0a");
        assert_eq!(plain("\u{9b}31m"), "\\x9b31m");
        assert_eq!(plain("a\\b"), "a\\x5cb");
        assert_eq!(plain("bateri\u{202e}gpj"), "bateri\\u{202e}gpj");
        assert_eq!(plain("a\u{200b}b\u{feff}"), "a\\u{200b}b\\u{feff}");
    }
}
