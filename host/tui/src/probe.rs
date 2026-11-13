//! What the terminal can do, asked of it.
//!
//! The questions go out in two rounds, each ending with the one every terminal answers (the device
//! attributes), so that its answer says no more is coming and a terminal that knows none of the others
//! is not waited for. The first asks the colours, the keyboard and what the terminal is; the second,
//! the one whether it draws pictures, goes out only to a terminal that is not a multiplexer: tmux and
//! screen keep what they are sent in a string of that kind (the Kitty graphics query) for the title of
//! the pane, and do not draw the pictures of the terminal behind them. A terminal that never answers is
//! waited for [`PATIENCE`] at the most, once: the second round is not sent to one that did not answer
//! the first.

use std::time::{Duration, Instant};

use pencere::Tty;

use crate::input::{Decoder, Event, Reply, Slot};
use crate::output::Depth;
use crate::theme::Theme;

/// The longest the answers of a round are waited for.
const PATIENCE: Duration = Duration::from_millis(1000);

/// The first round: the colours, the text colour and ground and the sixteen of the palette; whether the
/// keyboard has the Kitty protocol; what the terminal is, by its version and by its kind; the device
/// attributes.
pub(crate) fn questions() -> Vec<u8> {
    let mut out = String::from("\x1b]10;?\x1b\\\x1b]11;?\x1b\\");
    for index in 0..16 {
        out.push_str(&format!("\x1b]4;{index};?\x1b\\"));
    }
    out.push_str("\x1b[?u");
    out.push_str("\x1b[>q\x1b[>c");
    out.push_str("\x1b[c");
    out.into_bytes()
}

/// The second round: whether the Kitty graphics protocol takes a picture; the device attributes.
pub(crate) fn graphics_question() -> Vec<u8> {
    let mut out = String::from("\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\");
    out.push_str("\x1b[c");
    out.into_bytes()
}

/// What the terminal said of itself.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Found {
    /// The colours of the terminal: what it answered, over the built-in palette.
    pub theme: Theme,
    /// Whether the terminal told any of its colours.
    pub colors: bool,
    pub depth: Depth,
    /// The Kitty graphics protocol took a picture.
    pub graphics: bool,
    /// The Kitty keyboard protocol is there.
    pub keyboard: bool,
    /// The terminal is tmux or screen: by what the environment says or by what the terminal said it
    /// is. It is not asked about pictures, and draws none.
    pub multiplexer: bool,
    /// The terminal answered to the last question (the device attributes), so that nothing more was
    /// waited for.
    pub fenced: bool,
}

/// How the terminal is guessed to be, before it says (and where it says nothing): the colours that
/// were asked for, and what `COLORTERM` says of the number of colours.
pub(crate) struct Guess {
    pub theme: Theme,
    pub depth: Depth,
    /// The environment says the program runs in tmux or screen.
    pub multiplexer: bool,
}

impl Guess {
    /// From the environment: `PENCERE_TUI_THEME` (`light` or `dark`), `COLORTERM` (`truecolor`
    /// or `24bit` is true colour, anything else the 256 of the palette), and whether it is tmux or
    /// screen's ([`pencere::in_multiplexer`]).
    pub(crate) fn from_environment() -> Self {
        let theme = match std::env::var("PENCERE_TUI_THEME").as_deref() {
            Ok("light") => Theme::light(),
            _ => Theme::dark(),
        };
        let depth = match std::env::var("COLORTERM").as_deref() {
            Ok("truecolor" | "24bit") => Depth::True,
            _ => Depth::Indexed,
        };
        Self {
            theme,
            depth,
            multiplexer: pencere::in_multiplexer(),
        }
    }
}

/// Whether a terminal that names itself so (`tmux 3.7c`, `screen`) is a multiplexer.
fn multiplexer_named(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("tmux") || name.starts_with("screen")
}

/// What the terminal answered, put together: the colours it gave over the guess's.
fn found(guess: &Guess, replies: &[Reply], fenced: bool) -> Found {
    let mut theme = guess.theme;
    let mut colors = false;
    let mut graphics = false;
    let mut keyboard = false;
    let mut multiplexer = guess.multiplexer;
    // The ground first: it says whether the palette that fills the rest is a light one's or a
    // dark one's.
    if let Some(background) = replies.iter().find_map(|reply| match reply {
        Reply::Color(Slot::Background, rgb) => Some(*rgb),
        _ => None,
    }) {
        let base = if Theme::new(background, background, theme.ansi).is_dark() {
            Theme::dark()
        } else {
            Theme::light()
        };
        theme = Theme::new(background, base.foreground, base.ansi);
    }
    for reply in replies {
        match reply {
            Reply::Color(Slot::Foreground, rgb) => {
                theme.foreground = *rgb;
                theme.cursor = *rgb;
                colors = true;
            }
            Reply::Color(Slot::Background, _) => colors = true,
            Reply::Color(Slot::Ansi(index), rgb) => {
                if let Some(slot) = theme.ansi.get_mut(usize::from(*index)) {
                    *slot = *rgb;
                }
                colors = true;
            }
            Reply::Graphics { ok } => graphics |= *ok,
            Reply::KeyboardFlags(_) => keyboard = true,
            Reply::Name(name) => multiplexer |= multiplexer_named(name),
            Reply::Attributes(_) | Reply::Cursor { .. } => {}
        }
    }
    Found {
        theme,
        colors,
        depth: guess.depth,
        // A multiplexer does not draw the pictures of the terminal behind it, whatever answered.
        graphics: graphics && !multiplexer,
        keyboard,
        multiplexer,
        fenced,
    }
}

/// Asks the terminal, and reads what it says until it says it is done. What the person typed
/// meanwhile is left in `early`, for the loop. `Err` is the signal that ended the wait.
pub(crate) fn ask(
    tty: &Tty<'_>,
    decoder: &mut Decoder,
    early: &mut Vec<Event>,
    guess: &Guess,
) -> Result<Found, i32> {
    let mut replies = Vec::new();
    let fenced = round(tty, decoder, early, &questions(), &mut replies)?;
    let said = found(guess, &replies, fenced);
    // The second round goes to a terminal that answered the first (one that is mute is not waited for
    // again) and is not a multiplexer (which would keep the question for its title).
    if !fenced || said.multiplexer {
        return Ok(said);
    }
    let fenced = round(tty, decoder, early, &graphics_question(), &mut replies)?;
    Ok(found(guess, &replies, fenced))
}

/// Writes `questions` and reads the replies into `replies` until the device attributes come, or the
/// terminal has been waited for [`PATIENCE`]: whether they came. `Err` is the signal that ended the
/// wait.
fn round(
    tty: &Tty<'_>,
    decoder: &mut Decoder,
    early: &mut Vec<Event>,
    questions: &[u8],
    replies: &mut Vec<Reply>,
) -> Result<bool, i32> {
    if tty.write_all(questions).is_err() {
        return Ok(false);
    }
    let deadline = Instant::now() + PATIENCE;
    let mut buffer = [0_u8; 4096];
    let mut done = false;
    while !done {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let ready = match tty.wait(Some(left)) {
            Ok(ready) => ready,
            Err(_) => break,
        };
        if let Some(signal) = tty.take_signal() {
            return Err(signal);
        }
        if ready.input {
            let Ok(got) = tty.read(&mut buffer) else {
                break;
            };
            let mut events = Vec::new();
            decoder.feed(buffer.get(..got).unwrap_or_default(), &mut events);
            for event in events {
                match event {
                    Event::Reply(Reply::Attributes(attributes)) => {
                        replies.push(Reply::Attributes(attributes));
                        done = true;
                    }
                    Event::Reply(reply) => replies.push(reply),
                    other => early.push(other),
                }
            }
        }
        if ready.hangup {
            break;
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgb;

    fn guess() -> Guess {
        Guess {
            theme: Theme::dark(),
            depth: Depth::Indexed,
            multiplexer: false,
        }
    }

    #[test]
    fn a_terminal_that_says_nothing_is_the_guess() {
        let found = found(&guess(), &[], false);
        assert_eq!(found.theme, Theme::dark());
        assert!(!found.colors && !found.graphics && !found.keyboard);
        assert_eq!(found.depth, Depth::Indexed);
    }

    #[test]
    fn what_the_terminal_says_of_its_colours_is_what_the_theme_is() {
        let light = Rgb::new(0xfa, 0xfa, 0xfa);
        let replies = [
            Reply::Color(Slot::Background, light),
            Reply::Color(Slot::Foreground, Rgb::new(0x10, 0x10, 0x10)),
            Reply::Color(Slot::Ansi(4), Rgb::new(0, 0, 0xff)),
            Reply::Graphics { ok: true },
            Reply::KeyboardFlags(0),
        ];
        let found = found(&guess(), &replies, true);
        assert!(found.colors && found.graphics && found.keyboard);
        assert_eq!(found.theme.background, light);
        assert_eq!(found.theme.foreground, Rgb::new(0x10, 0x10, 0x10));
        assert_eq!(found.theme.ansi.get(4), Some(&Rgb::new(0, 0, 0xff)));
        // A light ground has the light palette's other colours.
        assert_eq!(found.theme.ansi.get(1), Theme::light().ansi.get(1));
        assert!(!found.theme.is_dark());
    }

    #[test]
    fn the_questions_end_with_the_one_every_terminal_answers() {
        let questions = questions();
        assert!(questions.ends_with(b"\x1b[c"));
        assert!(questions.starts_with(b"\x1b]10;?"));
        assert!(graphics_question().ends_with(b"\x1b[c"));
    }

    #[test]
    fn the_question_of_pictures_is_not_in_the_first_round() {
        let has = |bytes: &[u8]| bytes.windows(4).any(|window| window == b"a=q,");
        assert!(!has(&questions()));
        assert!(has(&graphics_question()));
    }

    #[test]
    fn a_terminal_that_says_it_is_tmux_draws_no_pictures_whatever_answers() {
        let replies = [
            Reply::Name("tmux 3.7c".to_owned()),
            Reply::Graphics { ok: true },
        ];
        let found = found(&guess(), &replies, true);
        assert!(found.multiplexer && !found.graphics);
        let ghostty = [
            Reply::Name("ghostty 1.2".to_owned()),
            Reply::Graphics { ok: true },
        ];
        let found = super::found(&guess(), &ghostty, true);
        assert!(!found.multiplexer && found.graphics);
        // And what the environment says is enough.
        let by_environment = Guess {
            multiplexer: true,
            ..guess()
        };
        let found = super::found(&by_environment, &[Reply::Graphics { ok: true }], true);
        assert!(found.multiplexer && !found.graphics);
    }
}
