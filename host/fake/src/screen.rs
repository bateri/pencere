//! What a terminal's parser notices in a program's output besides the frames
//! of this protocol: the two things the host needs in step with the stream.
//!
//! * `CSI c` (primary device attributes) is a question the terminal answers,
//!   and the answer must come after what the stream before it made the host
//!   write (the `welcome` goes before the reply, specification §5.2);
//! * `CSI ? 1049 h` and `l` switch the alternate screen, which an `open` must
//!   be made from (specification §6.5).
//!
//! Frames are APC strings of printable ASCII between `ESC _` and `ESC \`, so
//! nothing inside one can look like a CSI sequence.

const ESC: u8 = 0x1b;

/// What the scanner found at the byte it was just given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seen {
    /// `CSI c` or `CSI 0 c`: a reply is due.
    Query,
    /// `CSI ? 1049 h` (true) or `l` (false).
    Alternate(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    Escape,
    Csi,
}

#[derive(Debug)]
pub struct Screen {
    state: State,
    parameters: Vec<u8>,
}

/// Longer than any sequence this reads, so a stream of parameter bytes that
/// never ends cannot make the scanner grow.
const MAX_PARAMETERS: usize = 32;

impl Screen {
    pub fn new() -> Self {
        Self {
            state: State::Ground,
            parameters: Vec::new(),
        }
    }

    pub fn advance(&mut self, byte: u8) -> Option<Seen> {
        match self.state {
            State::Ground => {
                if byte == ESC {
                    self.state = State::Escape;
                }
                None
            }
            State::Escape => {
                self.state = match byte {
                    b'[' => {
                        self.parameters.clear();
                        State::Csi
                    }
                    ESC => State::Escape,
                    _ => State::Ground,
                };
                None
            }
            State::Csi => match byte {
                // Parameter and intermediate bytes.
                0x20..=0x3f => {
                    if self.parameters.len() < MAX_PARAMETERS {
                        self.parameters.push(byte);
                    }
                    None
                }
                0x40..=0x7e => {
                    self.state = State::Ground;
                    self.finished(byte)
                }
                ESC => {
                    self.state = State::Escape;
                    None
                }
                _ => {
                    self.state = State::Ground;
                    None
                }
            },
        }
    }

    fn finished(&self, last: u8) -> Option<Seen> {
        let parameters = self.parameters.as_slice();
        match last {
            b'c' if parameters.is_empty() || parameters == b"0" => Some(Seen::Query),
            b'h' | b'l' if parameters.first() == Some(&b'?') => parameters
                .get(1..)?
                .split(|&b| b == b';')
                .any(|parameter| parameter == b"1049")
                .then_some(Seen::Alternate(last == b'h')),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(bytes: &[u8]) -> Vec<Seen> {
        let mut screen = Screen::new();
        bytes.iter().filter_map(|&b| screen.advance(b)).collect()
    }

    #[test]
    fn the_device_attributes_query_is_found_with_or_without_a_zero() {
        assert_eq!(seen(b"\x1b[c"), [Seen::Query]);
        assert_eq!(seen(b"\x1b[0c"), [Seen::Query]);
        assert_eq!(seen(b"\x1b[>c"), []);
        assert_eq!(seen(b"\x1b[1c"), []);
    }

    #[test]
    fn the_alternate_screen_is_found_among_other_modes() {
        assert_eq!(seen(b"\x1b[?1049h"), [Seen::Alternate(true)]);
        assert_eq!(seen(b"\x1b[?1049l"), [Seen::Alternate(false)]);
        assert_eq!(seen(b"\x1b[?25;1049h"), [Seen::Alternate(true)]);
        assert_eq!(seen(b"\x1b[?25h\x1b[?47h\x1b[1049h"), []);
    }

    #[test]
    fn a_frame_hides_nothing_and_shows_nothing() {
        let both = b"\x1b_pencere;hello;id=1;{}\x1b\\\x1b[c\x1b[?1049h";
        assert_eq!(seen(both), [Seen::Query, Seen::Alternate(true)]);
    }

    #[test]
    fn a_sequence_cut_anywhere_is_still_found() {
        let text = b"x\x1b[?1049hy\x1b[cz";
        let mut screen = Screen::new();
        let found: Vec<Seen> = text.iter().filter_map(|&b| screen.advance(b)).collect();
        assert_eq!(found, [Seen::Alternate(true), Seen::Query]);
    }

    #[test]
    fn a_sequence_that_never_ends_costs_nothing() {
        let mut screen = Screen::new();
        screen.advance(ESC);
        screen.advance(b'[');
        for _ in 0..10_000 {
            assert_eq!(screen.advance(b'1'), None);
        }
        assert!(screen.parameters.len() <= MAX_PARAMETERS);
        assert_eq!(screen.advance(b'c'), None);
    }
}
