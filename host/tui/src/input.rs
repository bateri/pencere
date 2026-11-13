//! What a terminal sends, as what the page is told.
//!
//! The decoder takes the bytes as they come (a read may end in the middle of a sequence, or of a
//! character) and gives the events they make: keys from the old encodings and from the Kitty
//! keyboard protocol's `CSI u`, the mouse in SGR form, a paste between its brackets, the focus, and
//! the terminal's answers to what was asked of it. It knows nothing of the page.

use pencere_view::{Button, Key, Mods};

use crate::color::Rgb;

/// A key that went down, or came up where the terminal says so (the Kitty keyboard protocol).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub mods: Mods,
    /// The text the key types, when it types any: what a field takes. `None` for a key that is
    /// only a key (an arrow, a letter with Ctrl held).
    pub text: Option<String>,
    /// `false` for a key that went up (only the Kitty protocol says so).
    pub down: bool,
}

/// What the pointer did, in cells from the top-left corner (from 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseKind {
    Press(Button),
    Release(Button),
    /// A button went up, and the terminal does not say which (the old encoding): the decoder says it
    /// is the one that went down, so that no event of [`Decoder::feed`] has this.
    Up,
    /// Moved with no button down.
    Move,
    /// Moved with `Button` down.
    Drag(Button),
    /// The wheel, in steps: `dy` positive is down the page, `dx` positive is to the right.
    Wheel {
        dx: i8,
        dy: i8,
    },
}

/// A pointer event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mouse {
    pub kind: MouseKind,
    pub col: u16,
    pub row: u16,
    pub mods: Mods,
}

/// Which colour of the terminal an answer is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Foreground,
    Background,
    /// One of the 16 of the palette.
    Ansi(u8),
}

/// What the terminal answered to a question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    /// The device attributes (`CSI c`): the fence after the questions, which every terminal answers.
    Attributes(Vec<u16>),
    /// A colour (OSC 10, 11, 4).
    Color(Slot, Rgb),
    /// The Kitty keyboard protocol's flags in force: only a terminal that has it answers.
    KeyboardFlags(u32),
    /// The Kitty graphics protocol answered a query, the one whose image number was `id` (`ok` when it
    /// took it).
    Graphics { id: u32, ok: bool },
    /// How many pixels a cell of the terminal is (`CSI 16 t`), across and down.
    CellSize { width: u32, height: u32 },
    /// Where the cursor is (`CSI r;c R`), from 1.
    Cursor { row: u16, col: u16 },
    /// What the terminal says it is: its name and version (`CSI > q`: `tmux 3.7c`), or, from a
    /// terminal that does not answer that, what its secondary attributes (`CSI > c`) say it is.
    Name(String),
}

/// One thing the terminal said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Key(KeyEvent),
    Mouse(Mouse),
    /// What the person pasted, between the brackets of the terminal's paste mode.
    Paste(String),
    /// The terminal's window has the focus, or has not.
    Focus(bool),
    Reply(Reply),
}

/// The longest sequence that is waited for: past it the bytes are not a sequence of ours.
const LONGEST: usize = 8192;
/// The most a paste is taken to be.
const PASTE_MOST: usize = 4 << 20;

const PASTE_END: &[u8] = b"\x1b[201~";

/// Reads the terminal's bytes.
#[derive(Debug, Default)]
pub struct Decoder {
    buffer: Vec<u8>,
    /// Inside a paste: the bytes of it so far.
    paste: Option<Vec<u8>>,
    /// The button that is down, for a terminal that does not say which went up.
    down: Option<Button>,
}

/// What one try at the front of the buffer found.
enum Found {
    /// An event, from this many bytes.
    Event(Event, usize),
    /// Bytes that mean nothing to us, this many.
    Skip(usize),
    /// A sequence that has not all come.
    More,
}

impl Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes `bytes` and appends the events they complete to `out`.
    pub fn feed(&mut self, bytes: &[u8], out: &mut Vec<Event>) {
        self.buffer.extend_from_slice(bytes);
        loop {
            if self.paste.is_some() {
                if !self.take_paste(out) {
                    return;
                }
                continue;
            }
            if self.buffer.is_empty() {
                return;
            }
            let found = parse(&self.buffer);
            match found {
                Found::More => {
                    if self.buffer.len() > LONGEST {
                        self.buffer.clear();
                    }
                    return;
                }
                Found::Skip(used) => {
                    self.buffer.drain(..used.min(self.buffer.len()));
                }
                Found::Event(mut event, used) => {
                    self.buffer.drain(..used.min(self.buffer.len()));
                    self.track_buttons(&mut event);
                    if event == Event::Paste(String::new()) {
                        // The start of a paste: the bytes up to its end are its text.
                        self.paste = Some(Vec::new());
                    } else {
                        out.push(event);
                    }
                }
            }
        }
    }

    /// Which button is down, to say which went up where the terminal does not.
    fn track_buttons(&mut self, event: &mut Event) {
        let Event::Mouse(mouse) = event else {
            return;
        };
        match mouse.kind {
            MouseKind::Press(button) | MouseKind::Drag(button) => self.down = Some(button),
            MouseKind::Release(_) => self.down = None,
            MouseKind::Up => {
                mouse.kind = MouseKind::Release(self.down.take().unwrap_or(Button::Left));
            }
            MouseKind::Move | MouseKind::Wheel { .. } => {}
        }
    }

    /// Whether what is left is an ESC alone: either the Escape key or the start of a sequence that
    /// has not come yet; the caller decides by waiting a moment ([`Decoder::escape`]).
    pub fn lone_escape(&self) -> bool {
        self.paste.is_none() && self.buffer == [0x1b]
    }

    /// The ESC that waited was the Escape key.
    pub fn escape(&mut self, out: &mut Vec<Event>) {
        if self.lone_escape() {
            self.buffer.clear();
            out.push(Event::Key(KeyEvent {
                key: Key::Escape,
                mods: Mods::default(),
                text: None,
                down: true,
            }));
        }
    }

    /// The text of a paste, once its end has come: whether there was one to finish.
    fn take_paste(&mut self, out: &mut Vec<Event>) -> bool {
        let Some(text) = self.paste.as_mut() else {
            return false;
        };
        if let Some(at) = find(&self.buffer, PASTE_END) {
            text.extend_from_slice(self.buffer.get(..at).unwrap_or_default());
            self.buffer
                .drain(..(at + PASTE_END.len()).min(self.buffer.len()));
            let text = self.paste.take().unwrap_or_default();
            out.push(Event::Paste(clean(&text)));
            return true;
        }
        // The end may be half here: keep what could still be it.
        let keep = PASTE_END.len() - 1;
        let take = self.buffer.len().saturating_sub(keep);
        if take > 0 {
            if text.len() < PASTE_MOST {
                text.extend_from_slice(self.buffer.get(..take).unwrap_or_default());
            }
            self.buffer.drain(..take);
        }
        false
    }
}

/// A paste as text: line ends as `\n`, and no control character but the tab and the line end.
fn clean(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            '\n' | '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn key(key: Key, mods: Mods) -> Event {
    Event::Key(KeyEvent {
        key,
        mods,
        text: None,
        down: true,
    })
}

/// The modifiers of a CSI parameter: 1 plus the bits shift 1, alt 2, ctrl 4, super 8.
fn mods_of(parameter: u32) -> Mods {
    let bits = parameter.saturating_sub(1);
    Mods {
        shift: bits & 1 != 0,
        alt: bits & 2 != 0,
        control: bits & 4 != 0,
        command: bits & 8 != 0,
    }
}

fn parse(data: &[u8]) -> Found {
    let Some(&first) = data.first() else {
        return Found::More;
    };
    if first != 0x1b {
        return character(data, Mods::default());
    }
    let Some(&second) = data.get(1) else {
        return Found::More;
    };
    match second {
        b'[' => csi(data),
        b'O' => ss3(data),
        b']' => string(data, true),
        b'_' | b'P' | b'^' | b'X' => string(data, false),
        0x1b => Found::Event(key(Key::Escape, Mods::default()), 1),
        _ => {
            // Alt with a key: the ESC and then the key's own bytes.
            let alt = Mods {
                alt: true,
                ..Mods::default()
            };
            match character(data.get(1..).unwrap_or_default(), alt) {
                Found::Event(event, used) => Found::Event(event, used + 1),
                Found::Skip(used) => Found::Skip(used + 1),
                Found::More => Found::More,
            }
        }
    }
}

/// A character at the front of `data` and the key it is, with `mods` held besides.
fn character(data: &[u8], mods: Mods) -> Found {
    let Some(&byte) = data.first() else {
        return Found::More;
    };
    let named = |key_: Key| Found::Event(key(key_, mods), 1);
    match byte {
        0x00 => Found::Event(
            key(
                Key::Character(' '),
                Mods {
                    control: true,
                    ..mods
                },
            ),
            1,
        ),
        0x08 | 0x7f => named(Key::Backspace),
        0x09 => named(Key::Tab),
        0x0a | 0x0d => named(Key::Enter),
        0x01..=0x1a => {
            let letter = char::from(b'a' + byte - 1);
            Found::Event(
                key(
                    Key::Character(letter),
                    Mods {
                        control: true,
                        ..mods
                    },
                ),
                1,
            )
        }
        0x1c..=0x1f => {
            let symbol = char::from(byte - 0x1c + b'\\');
            let symbol = match symbol {
                '\\' | ']' | '^' | '_' => symbol,
                _ => '_',
            };
            Found::Event(
                key(
                    Key::Character(symbol),
                    Mods {
                        control: true,
                        ..mods
                    },
                ),
                1,
            )
        }
        0x1b => Found::Skip(1),
        _ => match decode(data) {
            Decoded::Char(c, used) => Found::Event(typed(c, mods), used),
            Decoded::More => Found::More,
            Decoded::Bad => Found::Skip(1),
        },
    }
}

/// A character that types itself.
fn typed(c: char, mods: Mods) -> Event {
    let shift = c.is_ascii_uppercase();
    Event::Key(KeyEvent {
        key: Key::Character(c),
        mods: Mods {
            shift: mods.shift || shift,
            ..mods
        },
        // With Alt held a key is a shortcut, not text.
        text: (!mods.alt && !mods.control && !mods.command).then(|| c.to_string()),
        down: true,
    })
}

enum Decoded {
    Char(char, usize),
    More,
    Bad,
}

/// The UTF-8 character at the front of `data`.
fn decode(data: &[u8]) -> Decoded {
    let Some(&lead) = data.first() else {
        return Decoded::More;
    };
    let length = match lead {
        0x00..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => return Decoded::Bad,
    };
    if data.len() < length {
        // Only a prefix of a character, if what is there so far is one.
        return if data
            .get(1..)
            .unwrap_or_default()
            .iter()
            .all(|byte| byte & 0xc0 == 0x80)
        {
            Decoded::More
        } else {
            Decoded::Bad
        };
    }
    match std::str::from_utf8(data.get(..length).unwrap_or_default())
        .ok()
        .and_then(|text| text.chars().next())
    {
        Some(c) => Decoded::Char(c, length),
        None => Decoded::Bad,
    }
}

/// `ESC O x`: the function keys F1 to F4 and the arrows of the application mode.
fn ss3(data: &[u8]) -> Found {
    let Some(&last) = data.get(2) else {
        return Found::More;
    };
    let mods = Mods::default();
    let event = match last {
        b'A' => key(Key::Up, mods),
        b'B' => key(Key::Down, mods),
        b'C' => key(Key::Right, mods),
        b'D' => key(Key::Left, mods),
        b'H' => key(Key::Home, mods),
        b'F' => key(Key::End, mods),
        b'P' => key(Key::Function(1), mods),
        b'Q' => key(Key::Function(2), mods),
        b'R' => key(Key::Function(3), mods),
        b'S' => key(Key::Function(4), mods),
        _ => return Found::Skip(3),
    };
    Found::Event(event, 3)
}

/// An OSC (to BEL or ST) or an APC/DCS/PM/SOS string (to ST).
fn string(data: &[u8], osc: bool) -> Found {
    let mut at = 2;
    while let Some(&byte) = data.get(at) {
        let body = data.get(2..at).unwrap_or_default();
        match byte {
            0x07 if osc => return osc_reply(body, at + 1),
            0x1b if data.get(at + 1) == Some(&b'\\') => {
                return if osc {
                    osc_reply(body, at + 2)
                } else if data.get(1) == Some(&b'_') {
                    graphics_reply(body, at + 2)
                } else if data.get(1) == Some(&b'P') {
                    name_reply(body, at + 2)
                } else {
                    Found::Skip(at + 2)
                };
            }
            0x1b if at + 1 >= data.len() => return Found::More,
            _ => at += 1,
        }
    }
    Found::More
}

/// The answer to `CSI > q`: `DCS > | name version ST`.
fn name_reply(body: &[u8], used: usize) -> Found {
    match body.strip_prefix(b">|") {
        Some(name) => Found::Event(
            Event::Reply(Reply::Name(String::from_utf8_lossy(name).into_owned())),
            used,
        ),
        None => Found::Skip(used),
    }
}

fn graphics_reply(body: &[u8], used: usize) -> Found {
    // `G` then the keys, `;`, the words: `OK` when it took what it was sent.
    if body.first() != Some(&b'G') {
        return Found::Skip(used);
    }
    let ok = body.ends_with(b";OK");
    // The keys are before the `;`: `i=` is the number of the image the question was about.
    let keys = body.get(1..).unwrap_or_default();
    let keys = keys.split(|byte| *byte == b';').next().unwrap_or_default();
    let id = std::str::from_utf8(keys)
        .unwrap_or("")
        .split(',')
        .find_map(|key| key.strip_prefix("i=")?.parse().ok())
        .unwrap_or(0);
    Found::Event(Event::Reply(Reply::Graphics { id, ok }), used)
}

fn osc_reply(body: &[u8], used: usize) -> Found {
    let text = String::from_utf8_lossy(body);
    let mut parts = text.splitn(2, ';');
    let (Some(command), Some(rest)) = (parts.next(), parts.next()) else {
        return Found::Skip(used);
    };
    let reply = match command {
        "10" => color(rest).map(|rgb| Reply::Color(Slot::Foreground, rgb)),
        "11" => color(rest).map(|rgb| Reply::Color(Slot::Background, rgb)),
        "4" => {
            let mut parts = rest.splitn(2, ';');
            let slot = parts.next().and_then(|n| n.parse::<u8>().ok());
            let rgb = parts.next().and_then(color);
            match (slot, rgb) {
                (Some(slot), Some(rgb)) if slot < 16 => Some(Reply::Color(Slot::Ansi(slot), rgb)),
                _ => None,
            }
        }
        _ => None,
    };
    match reply {
        Some(reply) => Found::Event(Event::Reply(reply), used),
        None => Found::Skip(used),
    }
}

/// `rgb:RRRR/GGGG/BBBB` (one to four hex digits a channel) or `#RRGGBB`.
fn color(text: &str) -> Option<Rgb> {
    if let Some(body) = text.strip_prefix("rgb:") {
        let mut channels = body.split('/').map(|part| {
            let digits = part.len();
            if digits == 0 || digits > 4 {
                return None;
            }
            let value = u32::from_str_radix(part, 16).ok()?;
            let most = (1_u32 << (4 * digits)) - 1;
            u8::try_from((value * 255 + most / 2) / most).ok()
        });
        let (r, g, b) = (channels.next()??, channels.next()??, channels.next()??);
        return channels.next().is_none().then_some(Rgb::new(r, g, b));
    }
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    u32::from_str_radix(hex, 16).ok().map(Rgb::hex)
}

/// A `CSI` sequence: its parameters, and the byte that ends it.
fn csi(data: &[u8]) -> Found {
    let mut at = 2;
    // Parameters (digits, `;`, `:`, `<`, `=`, `>`, `?`), intermediates, then the final byte.
    while data
        .get(at)
        .is_some_and(|byte| (0x30..=0x3f).contains(byte))
    {
        at += 1;
    }
    while data
        .get(at)
        .is_some_and(|byte| (0x20..=0x2f).contains(byte))
    {
        at += 1;
    }
    let Some(&last) = data.get(at) else {
        return Found::More;
    };
    if !(0x40..=0x7e).contains(&last) {
        return Found::Skip(at + 1);
    }
    let used = at + 1;
    let body = std::str::from_utf8(data.get(2..at).unwrap_or_default()).unwrap_or("");
    if last == b'M' && body.is_empty() {
        // The old mouse: `CSI M`, then the button, the column and the row, a byte each (32 more).
        return match data.get(used..used + 3) {
            Some(&[button, col, row]) => match legacy_mouse(button, col, row) {
                Some(event) => Found::Event(event, used + 3),
                None => Found::Skip(used + 3),
            },
            _ => Found::More,
        };
    }
    let event = match last {
        b'M' | b'm' if body.starts_with('<') => mouse(body.get(1..).unwrap_or(""), last == b'm'),
        b'I' if body.is_empty() => Some(Event::Focus(true)),
        b'O' if body.is_empty() => Some(Event::Focus(false)),
        b'u' => kitty_key(body),
        // The size of a cell: `CSI 6 ; height ; width t`.
        b't' if body.starts_with("6;") => match numbers(body).as_slice() {
            &[_, height, width] if height > 0 && width > 0 => {
                Some(Event::Reply(Reply::CellSize { width, height }))
            }
            _ => None,
        },
        // The secondary attributes: the first number is the terminal's kind. Two terminals that
        // draw nothing of their own are known by it, tmux (84, `T`) and screen (83, `S`).
        b'c' if body.starts_with('>') => match numbers(body.get(1..).unwrap_or("")).first() {
            Some(84) => Some(Event::Reply(Reply::Name("tmux".to_owned()))),
            Some(83) => Some(Event::Reply(Reply::Name("screen".to_owned()))),
            _ => None,
        },
        b'c' if body.starts_with('?') => Some(Event::Reply(Reply::Attributes(
            body.get(1..)
                .unwrap_or("")
                .split(';')
                .filter_map(|part| part.parse().ok())
                .collect(),
        ))),
        b'R' if !body.starts_with('?') && body.contains(';') => {
            let mut parts = body.split(';');
            match (
                parts.next().and_then(|p| p.parse().ok()),
                parts.next().and_then(|p| p.parse().ok()),
            ) {
                (Some(row), Some(col)) => Some(Event::Reply(Reply::Cursor { row, col })),
                _ => legacy_key(body, last),
            }
        }
        b'~' => {
            if body == "200" {
                return Found::Event(Event::Paste(String::new()), used);
            }
            legacy_key(body, last)
        }
        _ => legacy_key(body, last),
    };
    match event {
        Some(event) => Found::Event(event, used),
        None => Found::Skip(used),
    }
}

/// The numbers of a parameter list, `;`-separated, each as its first `:`-part.
fn numbers(body: &str) -> Vec<u32> {
    body.split(';')
        .map(|part| {
            part.split(':')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0)
        })
        .collect()
}

/// A key of the old encodings: `CSI A`, `CSI 1;5 C`, `CSI 3 ~`, `CSI Z`.
fn legacy_key(body: &str, last: u8) -> Option<Event> {
    let numbers = numbers(body);
    let mods = numbers.get(1).map_or(Mods::default(), |m| mods_of(*m));
    let first = numbers.first().copied().unwrap_or(0);
    let named = match last {
        b'A' => Key::Up,
        b'B' => Key::Down,
        b'C' => Key::Right,
        b'D' => Key::Left,
        b'H' => Key::Home,
        b'F' => Key::End,
        b'P' => Key::Function(1),
        b'Q' => Key::Function(2),
        b'S' => Key::Function(4),
        b'Z' => {
            return Some(key(
                Key::Tab,
                Mods {
                    shift: true,
                    ..mods
                },
            ));
        }
        b'~' => match first {
            1 | 7 => Key::Home,
            3 => Key::Delete,
            4 | 8 => Key::End,
            5 => Key::PageUp,
            6 => Key::PageDown,
            11..=15 => Key::Function(u8::try_from(first - 10).ok()?),
            17..=21 => Key::Function(u8::try_from(first - 11).ok()?),
            23..=26 => Key::Function(u8::try_from(first - 12).ok()?),
            28 | 29 => Key::Function(u8::try_from(first - 15).ok()?),
            31..=34 => Key::Function(u8::try_from(first - 17).ok()?),
            _ => return None,
        },
        _ => return None,
    };
    Some(key(named, mods))
}

/// `CSI code ; mods : event ; text u`, the Kitty keyboard protocol's key.
fn kitty_key(body: &str) -> Option<Event> {
    if body.starts_with(['?', '>', '<', '=']) {
        // `CSI ? flags u` is the answer to the query for the flags.
        if let Some(flags) = body.strip_prefix('?') {
            return flags
                .parse()
                .ok()
                .map(|f| Event::Reply(Reply::KeyboardFlags(f)));
        }
        return None;
    }
    let mut fields = body.split(';');
    let code: u32 = fields.next()?.split(':').next()?.parse().ok()?;
    let mut modifiers = fields.next().unwrap_or("").split(':');
    let mods = mods_of(modifiers.next().and_then(|m| m.parse().ok()).unwrap_or(1));
    let kind: u32 = modifiers.next().and_then(|e| e.parse().ok()).unwrap_or(1);
    let text: String = fields
        .next()
        .map(|codes| {
            codes
                .split(':')
                .filter_map(|c| c.parse::<u32>().ok().and_then(char::from_u32))
                .collect()
        })
        .unwrap_or_default();
    let key_ = match code {
        9 => Key::Tab,
        13 => Key::Enter,
        27 => Key::Escape,
        127 => Key::Backspace,
        57_399..=57_408 => Key::Character(char::from(b'0' + u8::try_from(code - 57_399).ok()?)),
        c => Key::Character(char::from_u32(c)?),
    };
    // The key is the one the keycap says, so a shifted letter comes with its case from `text`.
    let key_ = match (key_, text.chars().next()) {
        (Key::Character(_), Some(shown)) if mods.shift => Key::Character(shown),
        (key_, _) => key_,
    };
    let down = kind != 3;
    let types = !text.is_empty() && !mods.control && !mods.alt && !mods.command;
    Some(Event::Key(KeyEvent {
        key: key_,
        mods,
        text: types.then_some(text),
        down,
    }))
}

/// The mouse of the old encoding: the button byte says what and the modifiers as the new one's
/// number does (32 more), the place is 1-based and 32 more.
fn legacy_mouse(button: u8, col: u8, row: u8) -> Option<Event> {
    let number = u32::from(button.checked_sub(32)?);
    let place = |byte: u8| u16::from(byte).saturating_sub(33);
    let mods = Mods {
        shift: number & 4 != 0,
        alt: number & 8 != 0,
        control: number & 16 != 0,
        command: false,
    };
    let which = |low: u32| match low {
        0 => Button::Left,
        1 => Button::Middle,
        _ => Button::Right,
    };
    let kind = if number & 64 != 0 {
        match number & 3 {
            0 => MouseKind::Wheel { dx: 0, dy: -1 },
            1 => MouseKind::Wheel { dx: 0, dy: 1 },
            2 => MouseKind::Wheel { dx: -1, dy: 0 },
            _ => MouseKind::Wheel { dx: 1, dy: 0 },
        }
    } else if number & 32 != 0 {
        if number & 3 == 3 {
            MouseKind::Move
        } else {
            MouseKind::Drag(which(number & 3))
        }
    } else if number & 3 == 3 {
        MouseKind::Up
    } else {
        MouseKind::Press(which(number & 3))
    };
    Some(Event::Mouse(Mouse {
        kind,
        col: place(col),
        row: place(row),
        mods,
    }))
}

/// `< button ; col ; row` and `M` for down, `m` for up.
fn mouse(body: &str, up: bool) -> Option<Event> {
    let numbers = numbers(body);
    let (&button, &col, &row) = (numbers.first()?, numbers.get(1)?, numbers.get(2)?);
    let mods = Mods {
        shift: button & 4 != 0,
        alt: button & 8 != 0,
        control: button & 16 != 0,
        command: false,
    };
    let which = |low: u32| match low {
        0 => Button::Left,
        1 => Button::Middle,
        _ => Button::Right,
    };
    let kind = if button & 64 != 0 {
        match button & 3 {
            0 => MouseKind::Wheel { dx: 0, dy: -1 },
            1 => MouseKind::Wheel { dx: 0, dy: 1 },
            2 => MouseKind::Wheel { dx: -1, dy: 0 },
            _ => MouseKind::Wheel { dx: 1, dy: 0 },
        }
    } else if button & 32 != 0 {
        if button & 3 == 3 {
            MouseKind::Move
        } else {
            MouseKind::Drag(which(button & 3))
        }
    } else if up {
        MouseKind::Release(which(button & 3))
    } else {
        MouseKind::Press(which(button & 3))
    };
    Some(Event::Mouse(Mouse {
        kind,
        col: u16::try_from(col.saturating_sub(1)).ok()?,
        row: u16::try_from(row.saturating_sub(1)).ok()?,
        mods,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_all(bytes: &[u8]) -> Vec<Event> {
        let mut out = Vec::new();
        Decoder::new().feed(bytes, &mut out);
        out
    }

    fn only_key(bytes: &[u8]) -> KeyEvent {
        match decode_all(bytes).as_slice() {
            [Event::Key(k)] => k.clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn letters_and_controls_are_keys() {
        let a = only_key(b"a");
        assert_eq!((a.key, a.text.as_deref()), (Key::Character('a'), Some("a")));
        let big = only_key(b"K");
        assert!(big.mods.shift && big.text.as_deref() == Some("K"));
        let ctrl = only_key(&[0x0b]);
        assert_eq!(ctrl.key, Key::Character('k'));
        assert!(ctrl.mods.control && ctrl.text.is_none());
        assert_eq!(only_key(&[0x7f]).key, Key::Backspace);
        assert_eq!(only_key(&[0x0d]).key, Key::Enter);
        assert_eq!(only_key("ş".as_bytes()).key, Key::Character('ş'));
    }

    #[test]
    fn the_arrows_and_their_modifiers() {
        assert_eq!(only_key(b"\x1b[A").key, Key::Up);
        assert_eq!(only_key(b"\x1bOB").key, Key::Down);
        let shifted = only_key(b"\x1b[1;2C");
        assert_eq!(shifted.key, Key::Right);
        assert!(shifted.mods.shift);
        let control = only_key(b"\x1b[1;5D");
        assert!(control.mods.control && control.key == Key::Left);
        assert_eq!(only_key(b"\x1b[3~").key, Key::Delete);
        assert_eq!(only_key(b"\x1b[5~").key, Key::PageUp);
        assert_eq!(only_key(b"\x1b[15~").key, Key::Function(5));
        assert_eq!(only_key(b"\x1b[24~").key, Key::Function(12));
        let back = only_key(b"\x1b[Z");
        assert!(back.key == Key::Tab && back.mods.shift);
    }

    #[test]
    fn alt_is_an_escape_before_the_key() {
        let k = only_key(b"\x1bb");
        assert!(k.mods.alt && k.key == Key::Character('b') && k.text.is_none());
    }

    #[test]
    fn a_lone_escape_waits_and_then_is_the_key() {
        let mut decoder = Decoder::new();
        let mut out = Vec::new();
        decoder.feed(b"\x1b", &mut out);
        assert!(out.is_empty() && decoder.lone_escape());
        decoder.escape(&mut out);
        assert!(matches!(out.as_slice(), [Event::Key(k)] if k.key == Key::Escape));
        // The rest of a sequence that came late is the same sequence.
        let mut out = Vec::new();
        decoder.feed(b"\x1b", &mut out);
        decoder.feed(b"[A", &mut out);
        assert!(matches!(out.as_slice(), [Event::Key(k)] if k.key == Key::Up));
    }

    #[test]
    fn a_character_split_across_reads_is_one() {
        let bytes = "ğ".as_bytes();
        let mut decoder = Decoder::new();
        let mut out = Vec::new();
        decoder.feed(&bytes[..1], &mut out);
        assert!(out.is_empty());
        decoder.feed(&bytes[1..], &mut out);
        assert!(matches!(out.as_slice(), [Event::Key(k)] if k.key == Key::Character('ğ')));
    }

    #[test]
    fn the_mouse_in_sgr_form() {
        let events =
            decode_all(b"\x1b[<0;10;5M\x1b[<0;10;5m\x1b[<35;11;5M\x1b[<64;3;4M\x1b[<2;3;4M");
        let kinds: Vec<MouseKind> = events
            .iter()
            .map(|e| match e {
                Event::Mouse(m) => m.kind,
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            [
                MouseKind::Press(Button::Left),
                MouseKind::Release(Button::Left),
                MouseKind::Move,
                MouseKind::Wheel { dx: 0, dy: -1 },
                MouseKind::Press(Button::Right),
            ]
        );
        assert!(matches!(&events[0], Event::Mouse(m) if (m.col, m.row) == (9, 4)));
    }

    #[test]
    fn the_mouse_of_the_old_encoding() {
        // A press of the left button at column 10 row 5, a drag, and a release that says no button.
        let mut bytes = b"\x1b[M".to_vec();
        bytes.extend_from_slice(&[32, 32 + 10, 32 + 5]);
        bytes.extend_from_slice(b"\x1b[M");
        bytes.extend_from_slice(&[32 + 32, 32 + 11, 32 + 5]);
        bytes.extend_from_slice(b"\x1b[M");
        bytes.extend_from_slice(&[32 + 3, 32 + 11, 32 + 5]);
        bytes.extend_from_slice(b"\x1b[M");
        bytes.extend_from_slice(&[32 + 64, 32 + 3, 32 + 4]);
        let events = decode_all(&bytes);
        let kinds: Vec<MouseKind> = events
            .iter()
            .map(|e| match e {
                Event::Mouse(m) => m.kind,
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            [
                MouseKind::Press(Button::Left),
                MouseKind::Drag(Button::Left),
                MouseKind::Release(Button::Left),
                MouseKind::Wheel { dx: 0, dy: -1 },
            ]
        );
        assert!(matches!(&events[0], Event::Mouse(m) if (m.col, m.row) == (9, 4)));
        // Split in the middle of the three bytes.
        let mut decoder = Decoder::new();
        let mut out = Vec::new();
        decoder.feed(&bytes[..5], &mut out);
        assert!(out.is_empty());
        decoder.feed(&bytes[5..8], &mut out);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn a_paste_is_its_text_with_the_line_ends_made_one() {
        let events = decode_all(b"\x1b[200~one\r\ntwo\x1b[201~x");
        assert_eq!(events[0], Event::Paste("one\ntwo".to_owned()));
        assert!(matches!(&events[1], Event::Key(k) if k.key == Key::Character('x')));
        // Split in the middle of the end bracket.
        let mut decoder = Decoder::new();
        let mut out = Vec::new();
        decoder.feed(b"\x1b[200~ab\x1b[20", &mut out);
        assert!(out.is_empty());
        decoder.feed(b"1~", &mut out);
        assert_eq!(out, [Event::Paste("ab".to_owned())]);
    }

    #[test]
    fn the_answers_of_the_terminal() {
        let events = decode_all(
            b"\x1b]11;rgb:1e1e/1e1e/2e2e\x07\x1b]10;rgb:ff/ff/ff\x1b\\\x1b]4;1;rgb:cc/00/00\x07\x1b[?62;4c\x1b[?0u\x1b[I\x1b_Gi=31;OK\x1b\\\x1b_Gi=32;ENOENT:no\x1b\\\x1b[6;29;13t",
        );
        assert_eq!(
            events,
            [
                Event::Reply(Reply::Color(Slot::Background, Rgb::new(0x1e, 0x1e, 0x2e))),
                Event::Reply(Reply::Color(Slot::Foreground, Rgb::new(255, 255, 255))),
                Event::Reply(Reply::Color(Slot::Ansi(1), Rgb::new(0xcc, 0, 0))),
                Event::Reply(Reply::Attributes(vec![62, 4])),
                Event::Reply(Reply::KeyboardFlags(0)),
                Event::Focus(true),
                Event::Reply(Reply::Graphics { id: 31, ok: true }),
                Event::Reply(Reply::Graphics { id: 32, ok: false }),
                Event::Reply(Reply::CellSize {
                    width: 13,
                    height: 29
                }),
            ]
        );
    }

    #[test]
    fn a_terminal_says_what_it_is_by_its_version_or_its_kind() {
        let events = decode_all(
            b"\x1bP>|tmux 3.7c\x1b\\\x1b[>84;0;0c\x1b[>83;40003;0c\x1b[>41;388;0c\x1b[?1;2c",
        );
        assert_eq!(
            events,
            [
                Event::Reply(Reply::Name("tmux 3.7c".to_owned())),
                Event::Reply(Reply::Name("tmux".to_owned())),
                Event::Reply(Reply::Name("screen".to_owned())),
                Event::Reply(Reply::Attributes(vec![1, 2])),
            ]
        );
    }

    #[test]
    fn the_kitty_keyboard_protocol_says_the_key_and_whether_it_went_up() {
        let down = only_key(b"\x1b[107;6u");
        assert!(down.mods.control && down.mods.shift && down.key == Key::Character('k'));
        assert!(down.down && down.text.is_none());
        let up = only_key(b"\x1b[97;1:3u");
        assert!(!up.down);
        let typed = only_key(b"\x1b[97;2;65u");
        assert_eq!(
            (typed.key, typed.text.as_deref()),
            (Key::Character('A'), Some("A"))
        );
        assert_eq!(only_key(b"\x1b[13u").key, Key::Enter);
    }

    #[test]
    fn what_is_not_ours_is_dropped_and_what_follows_is_read() {
        let events = decode_all(b"\x1b[999;999Xa");
        assert!(matches!(events.last(), Some(Event::Key(k)) if k.key == Key::Character('a')));
    }
}
