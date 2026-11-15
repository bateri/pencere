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

/// The number of the image the question of whether the terminal takes pixels that are packed is
/// about (the plain question is 31): the terminal's answer says which it is.
pub(crate) const COMPRESSION: u32 = 32;

/// The number of the image the question of whether the terminal changes a part of a picture's frame in
/// place (`a=f`) is about.
pub(crate) const FRAMES: u32 = 33;

/// The number of the image the question of whether the terminal copies a rectangle of a picture's frame
/// to another place in it (`a=c`) is about.
pub(crate) const COMPOSE: u32 = 34;

/// The first round: the colours, the text colour and ground and the sixteen of the palette; whether the
/// keyboard has the Kitty protocol; what the terminal is, by its version and by its kind; how many
/// pixels a cell is; the device attributes.
pub(crate) fn questions() -> Vec<u8> {
    let mut out = String::from("\x1b]10;?\x1b\\\x1b]11;?\x1b\\");
    for index in 0..16 {
        out.push_str(&format!("\x1b]4;{index};?\x1b\\"));
    }
    out.push_str("\x1b[?u");
    out.push_str("\x1b[>q\x1b[>c\x1b[16t\x1b[14t");
    // Whether the pointer may be told in pixels (the mode is asked and not set: it is set once the
    // answer is known).
    out.push_str("\x1b[?1016$p");
    out.push_str("\x1b[c");
    out.into_bytes()
}

/// The second round: whether the Kitty graphics protocol takes a picture, and takes one whose pixels
/// are packed (zlib: a pixel of nothing), and changes a part of a picture in place (a picture of two
/// pixels is sent, quietly, and one of its pixels edited: the edit is what is answered, and the
/// picture is given up again); the device attributes.
pub(crate) fn graphics_question() -> Vec<u8> {
    let mut out = String::from("\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\");
    let packed = crate::output::base64(&miniz_oxide::deflate::compress_to_vec_zlib(&[0; 4], 1));
    out.push_str(&format!(
        "\x1b_Gi={COMPRESSION},s=1,v=1,a=q,t=d,f=32,o=z;{packed}\x1b\\"
    ));
    let two = crate::output::base64(&[0; 8]);
    let one = crate::output::base64(&[0; 4]);
    out.push_str(&format!(
        "\x1b_Ga=t,i={FRAMES},s=2,v=1,f=32,q=2;{two}\x1b\\\
         \x1b_Ga=f,i={FRAMES},r=1,x=1,y=0,s=1,v=1,f=32,q=0;{one}\x1b\\\
         \x1b_Ga=d,d=I,i={FRAMES},q=2\x1b\\"
    ));
    // The same picture of two pixels, and the first of them copied over the second, in the picture's own
    // frame: the copy is what is answered.
    out.push_str(&format!(
        "\x1b_Ga=t,i={COMPOSE},s=2,v=1,f=32,q=2;{two}\x1b\\\
         \x1b_Ga=c,i={COMPOSE},r=1,c=1,X=0,Y=0,x=1,y=0,w=1,h=1,C=1,q=0\x1b\\\
         \x1b_Ga=d,d=I,i={COMPOSE},q=2\x1b\\"
    ));
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
    /// It takes pixels that are packed (`o=z`) too.
    pub compression: bool,
    /// It changes a part of a picture's frame in place (`a=f`).
    pub frames: bool,
    /// The terminal copies a rectangle of a picture's frame to another place in the frame (`a=c`).
    pub compose: bool,
    /// It reports the pointer in pixels (the SGR pixel mode, 1016).
    pub pixel_mouse: bool,
    /// How many pixels the window is, across and down, where the terminal said (`CSI 14 t`).
    pub window: Option<(u32, u32)>,
    /// How many pixels a cell is, across and down, where the terminal said.
    pub cell: Option<(u32, u32)>,
    /// The Kitty keyboard protocol is there.
    pub keyboard: bool,
    /// The terminal is tmux or screen: by what the environment says or by what the terminal said it
    /// is. It is not asked about pictures, and draws none.
    pub multiplexer: bool,
    /// The terminal answered to the last question (the device attributes), so that nothing more was
    /// waited for.
    pub fenced: bool,
}

impl Found {
    /// How many pixels a cell is, across and down: what the terminal said of it, or its window's pixels
    /// (what it said of them, or what its size has of them) over the cells the window has.
    pub(crate) fn cell_in(&self, size: &pencere::Size) -> Option<(u32, u32)> {
        if let Some(cell) = self.cell {
            return Some(cell);
        }
        let (cols, rows) = (u32::from(size.cols), u32::from(size.rows));
        let (width, height) = self.window.or_else(|| {
            (size.width > 0 && size.height > 0)
                .then(|| (u32::from(size.width), u32::from(size.height)))
        })?;
        (cols > 0 && rows > 0 && width >= cols && height >= rows)
            .then(|| (width / cols, height / rows))
    }
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
    let mut compression = false;
    let mut frames = false;
    let mut compose = false;
    let mut pixel_mouse = false;
    let mut window = None;
    let mut cell = None;
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
            Reply::Graphics { id, ok } if *id == COMPRESSION => compression |= *ok,
            Reply::Graphics { id, ok } if *id == FRAMES => frames |= *ok,
            Reply::Graphics { id, ok } if *id == COMPOSE => compose |= *ok,
            Reply::Graphics { ok, .. } => graphics |= *ok,
            Reply::CellSize { width, height } => cell = Some((*width, *height)),
            Reply::WindowSize { width, height } => window = Some((*width, *height)),
            // Known, set or not: the mode may be set. One it does not know it says so.
            Reply::Mode { mode: 1016, state } => pixel_mouse = matches!(state, 1 | 2),
            Reply::Mode { .. } => {}
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
        compression: compression && graphics && !multiplexer,
        frames: frames && graphics && !multiplexer,
        // Left out where the person says the copy is not to be used (`PENCERE_TUI_COMPOSE=0`).
        compose: compose
            && frames
            && graphics
            && !multiplexer
            && std::env::var("PENCERE_TUI_COMPOSE").as_deref() != Ok("0"),
        pixel_mouse,
        window,
        cell,
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
            Reply::Graphics { id: 31, ok: true },
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
    fn the_size_of_a_cell_and_whether_pixels_may_be_packed_are_what_the_terminal_says() {
        let replies = [
            Reply::Graphics { id: 31, ok: true },
            Reply::Graphics {
                id: COMPRESSION,
                ok: true,
            },
            Reply::CellSize {
                width: 13,
                height: 29,
            },
        ];
        let found = found(&guess(), &replies, true);
        assert!(found.graphics && found.compression);
        assert_eq!(found.cell, Some((13, 29)));
        // A terminal that took the plain picture and refused the packed one is sent pixels as they are.
        let refused = [
            Reply::Graphics { id: 31, ok: true },
            Reply::Graphics {
                id: COMPRESSION,
                ok: false,
            },
        ];
        let found = super::found(&guess(), &refused, true);
        assert!(found.graphics && !found.compression && found.cell.is_none());
        // The question is asked and the size is asked for.
        assert!(questions().windows(5).any(|window| window == b"\x1b[16t"));
        let asked = graphics_question();
        let has = |needle: &[u8]| asked.windows(needle.len()).any(|window| window == needle);
        assert!(has(b"i=32,") && has(b"o=z;"));
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
            Reply::Graphics { id: 31, ok: true },
        ];
        let found = found(&guess(), &replies, true);
        assert!(found.multiplexer && !found.graphics);
        let ghostty = [
            Reply::Name("ghostty 1.2".to_owned()),
            Reply::Graphics { id: 31, ok: true },
        ];
        let found = super::found(&guess(), &ghostty, true);
        assert!(!found.multiplexer && found.graphics);
        // And what the environment says is enough.
        let by_environment = Guess {
            multiplexer: true,
            ..guess()
        };
        let found = super::found(
            &by_environment,
            &[Reply::Graphics { id: 31, ok: true }],
            true,
        );
        assert!(found.multiplexer && !found.graphics);
    }

    #[test]
    fn what_the_terminal_says_of_edits_its_window_and_its_pointer_is_what_the_page_is_drawn_for() {
        let replies = [
            Reply::Graphics { id: 31, ok: true },
            Reply::Graphics {
                id: FRAMES,
                ok: true,
            },
            Reply::WindowSize {
                width: 2400,
                height: 1360,
            },
            Reply::Mode {
                mode: 1016,
                state: 2,
            },
        ];
        let found = found(&guess(), &replies, true);
        assert!(found.frames && found.pixel_mouse);
        assert_eq!(found.window, Some((2400, 1360)));
        // An edit refused, a mode no one knows, and a terminal that has no pictures to edit.
        let refused = [
            Reply::Graphics { id: 31, ok: true },
            Reply::Graphics {
                id: FRAMES,
                ok: false,
            },
            Reply::Mode {
                mode: 1016,
                state: 0,
            },
            Reply::Mode {
                mode: 2004,
                state: 2,
            },
        ];
        let found = super::found(&guess(), &refused, true);
        assert!(found.graphics && !found.frames && !found.pixel_mouse);
        let none = super::found(
            &guess(),
            &[Reply::Graphics {
                id: FRAMES,
                ok: true,
            }],
            true,
        );
        assert!(!none.frames, "no pictures, no edits of them");
        // The copy of a part of a picture onto another is asked after the edit, and is the answer
        // of a terminal that has pictures and edits them.
        let copies = [
            Reply::Graphics { id: 31, ok: true },
            Reply::Graphics {
                id: FRAMES,
                ok: true,
            },
            Reply::Graphics {
                id: COMPOSE,
                ok: true,
            },
        ];
        assert!(super::found(&guess(), &copies, true).compose);
        let no_edits = [
            Reply::Graphics { id: 31, ok: true },
            Reply::Graphics {
                id: COMPOSE,
                ok: true,
            },
        ];
        assert!(
            !super::found(&guess(), &no_edits, true).compose,
            "a copy is an edit"
        );
        assert!(!super::found(&guess(), &refused, true).compose);
        let asked = String::from_utf8_lossy(&graphics_question()).into_owned();
        assert!(
            asked
                .find("a=t,i=34")
                .is_some_and(|put| asked.find("a=c,i=34").is_some_and(|copy| put < copy)),
            "the picture is sent before it is copied: {asked:?}"
        );
        // The questions: the window's size and the pointer's mode in the first round, the edit in the
        // second, and what the edit changes is a picture the terminal has first.
        let has = |bytes: &[u8], needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
        assert!(has(&questions(), b"\x1b[14t") && has(&questions(), b"\x1b[?1016$p"));
        let asked = graphics_question();
        assert!(has(&asked, b"i=33,") && has(&asked, b"a=f,"));
        let sent = String::from_utf8_lossy(&asked).into_owned();
        assert!(
            sent.find("a=t,")
                .is_some_and(|put| sent.find("a=f,").is_some_and(|edit| put < edit)),
            "the picture is sent before it is edited"
        );
    }

    #[test]
    fn a_cell_is_what_the_terminal_says_or_its_window_over_its_cells() {
        let size = |width, height| pencere::Size {
            cols: 100,
            rows: 40,
            width,
            height,
        };
        let said = super::found(
            &guess(),
            &[Reply::CellSize {
                width: 13,
                height: 29,
            }],
            true,
        );
        assert_eq!(said.cell_in(&size(1600, 1600)), Some((13, 29)));
        let window = super::found(
            &guess(),
            &[Reply::WindowSize {
                width: 1600,
                height: 1360,
            }],
            true,
        );
        assert_eq!(window.cell_in(&size(0, 0)), Some((16, 34)));
        let silent = super::found(&guess(), &[], true);
        assert_eq!(silent.cell_in(&size(1000, 800)), Some((10, 20)));
        assert_eq!(silent.cell_in(&size(0, 0)), None);
    }
}
