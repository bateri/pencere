//! Frames: the APC strings of the wire (specification §4), read and written.
//!
//! A frame is `ESC _ pencere; kind ; control ; payload ESC \`, and everything
//! between the two escapes is printable ASCII. A message is one or more frames
//! (chunks) with the same kind and id; [`Event::Message`] is what reading a
//! message's last chunk delivers.
//!
//! There are two readers and they follow the same rules:
//!
//! * [`BodyDecoder`] is for a terminal whose parser has already found the APC
//!   strings and hands over their bodies in the slices its reads made
//!   ([`start`](BodyDecoder::start), [`put`](BodyDecoder::put),
//!   [`end`](BodyDecoder::end), the shape of the `vte` fork's `apc_*` calls);
//! * [`StreamDecoder`] is for raw bytes (a program library reading its input,
//!   a test harness): it finds the strings itself.
//!
//! Both are written for a stream anyone can write to: nothing a frame says is
//! allocated before it has been seen, a string that is not this protocol's is
//! dropped at its first differing byte, and a frame or a message that is too
//! long is dropped as soon as it is.

use std::fmt;

/// What starts the body of every frame of this protocol.
pub const PREFIX: &[u8; 8] = b"pencere;";

const ESC: u8 = 0x1b;
/// `ESC _` before the body and `ESC \` after it.
const FRAME_CLOSURE: usize = 4;
/// What a frame has besides its kind, its control text and its payload:
/// `ESC _`, `pencere;`, two `;` and `ESC \`.
const FRAME_FIXED: usize = 2 + 8 + 2 + 2;

/// The two directions of the wire (specification §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Program to host: bytes the program writes to its tty.
    ToHost,
    /// Host to program: bytes the host writes into the input side of the tty.
    ToProgram,
}

/// The limits of one direction (specification §4.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireLimits {
    /// The most bytes a frame may have, `ESC _` and `ESC \` included.
    pub frame: usize,
    /// The most bytes a message's payload may have.
    pub message: usize,
}

impl Direction {
    /// The limits of this direction.
    pub const fn limits(self) -> WireLimits {
        match self {
            Self::ToHost => WireLimits {
                frame: 4096,
                message: 16 * 1024 * 1024,
            },
            // A message of a view is at most 1 MiB as the view said it, and the escapes of the
            // wire (§4.5) make a character up to six times as long (a DEL is `\u007f`): six
            // times the limit of a view's message is what always fits.
            Self::ToProgram => WireLimits {
                frame: 511,
                message: 6 * 1024 * 1024,
            },
        }
    }

    /// The direction's name in the specification and the fixtures.
    pub const fn name(self) -> &'static str {
        match self {
            Self::ToHost => "to-host",
            Self::ToProgram => "to-program",
        }
    }

    /// The direction a name stands for.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "to-host" => Some(Self::ToHost),
            "to-program" => Some(Self::ToProgram),
            _ => None,
        }
    }
}

/// Why a frame or a message was dropped (specification §4.7). Reporting these is
/// for diagnostics only: a drop has no effect beyond ending the message it hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DecodeError {
    /// The frame does not follow the grammar, or holds a byte it may not.
    Malformed,
    /// The frame is over the frame limit of its direction.
    TooLong,
    /// A continuation chunk does not continue the open message.
    ChunkOrder,
    /// A new message began before the open one ended; the open one is dropped.
    Abandoned,
    /// The payload so far is over the message limit of its direction.
    MessageTooLong,
}

impl DecodeError {
    /// The name the specification and the fixtures use.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::TooLong => "too-long",
            Self::ChunkOrder => "chunk-order",
            Self::Abandoned => "abandoned",
            Self::MessageTooLong => "message-too-long",
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl std::error::Error for DecodeError {}

/// A complete message: its kind, its id, the parameters of its first chunk
/// besides `id`, `seq` and `more`, and its payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// The message's kind (`hello`, `open`, `msg`, ...). Decoders deliver every
    /// well-formed message whatever its kind; the receiver ignores what it does
    /// not know.
    pub kind: String,
    /// The message's id.
    pub id: u32,
    /// The first chunk's other parameters, in the order they were written:
    /// `re`, `d`, and any key this version does not know.
    pub control: Vec<(String, String)>,
    /// The payloads of the chunks, concatenated. Printable ASCII.
    pub payload: String,
}

impl Message {
    /// The value of a parameter of the first chunk.
    pub fn param(&self, key: &str) -> Option<&str> {
        self.control
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    /// The `re` parameter: the message of the other side this one answers.
    pub fn re(&self) -> Option<u32> {
        self.param("re").and_then(|value| value.parse().ok())
    }

    /// The `d` parameter: a file's SHA-256 digest, 64 lowercase hex digits.
    pub fn digest(&self) -> Option<&str> {
        self.param("d")
    }

    /// The `h` parameter: the handle of a file the person gave, 32 lowercase hex digits.
    pub fn handle(&self) -> Option<&str> {
        self.param("h")
    }

    /// The `o` parameter: where in a file a `write` puts its bytes.
    pub fn offset(&self) -> Option<u64> {
        self.param("o").and_then(offset)
    }

    /// The `end` parameter: whether a `write` is the last of its file. Absent means `0`.
    pub fn end(&self) -> bool {
        self.param("end") == Some("1")
    }

    /// The `e` parameter: why a `data` carries no bytes.
    pub fn error_code(&self) -> Option<&str> {
        self.param("e")
    }
}

/// What a decoder delivers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A message ended.
    Message(Message),
    /// A frame or a message was dropped.
    Error(DecodeError),
}

// ---- one frame -------------------------------------------------------------

struct Chunk<'a> {
    kind: &'a str,
    id: u32,
    seq: u32,
    more: bool,
    /// Every parameter besides `id`, `seq` and `more`, in the order written.
    control: Vec<(&'a str, &'a str)>,
    payload: &'a str,
}

fn is_kind(text: &str) -> bool {
    (1..=16).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
}

fn is_key(text: &str) -> bool {
    (1..=8).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_lowercase())
}

fn is_value(text: &str) -> bool {
    (1..=64).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// A decimal number as the wire writes it: digits, no leading zero (but `0`),
/// at most 4294967295.
fn decimal(text: &str) -> Option<u32> {
    let plain = !text.is_empty()
        && text.len() <= 10
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    if plain { text.parse().ok() } else { None }
}

fn is_digest(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The handle of a file the person gave (`h`, §4.3): 32 lowercase hex digits.
pub fn is_handle(text: &str) -> bool {
    text.len() == 32 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The greatest offset `o` may say: the greatest whole number a JSON number holds in the
/// language a page is written in, as for the ids of the six messages (§9.2).
pub const MAX_OFFSET: u64 = (1 << 53) - 1;

/// An offset as the wire writes it (`o`, §4.3): digits, no leading zero (but `0`), at most
/// [`MAX_OFFSET`].
fn offset(text: &str) -> Option<u64> {
    let plain = !text.is_empty()
        && text.len() <= 16
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    if plain {
        text.parse().ok().filter(|&n| n <= MAX_OFFSET)
    } else {
        None
    }
}

/// An error code (`e`, §4.3): 1 to 32 of `a-z` and `-`.
pub fn is_code(text: &str) -> bool {
    (1..=32).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
}

/// Parses the body of a frame (everything between `ESC _` and `ESC \`, `pencere;`
/// included) with the grammar of §4.1 and the parameters of §4.3.
fn parse_chunk(body: &[u8]) -> Option<Chunk<'_>> {
    let text = std::str::from_utf8(body).ok()?;
    let rest = text.strip_prefix("pencere;")?;
    let (kind, rest) = rest.split_once(';')?;
    let (control, payload) = rest.split_once(';')?;
    if !is_kind(kind) {
        return None;
    }
    let (mut id, mut seq, mut more) = (None, None, None);
    let mut others: Vec<(&str, &str)> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    if !control.is_empty() {
        for pair in control.split(',') {
            let (key, value) = pair.split_once('=')?;
            if !is_key(key) || !is_value(value) || seen.contains(&key) {
                return None;
            }
            seen.push(key);
            match key {
                "id" => id = Some(decimal(value).filter(|&n| n >= 1)?),
                "seq" => seq = Some(decimal(value)?),
                "more" => {
                    more = Some(match value {
                        "0" => false,
                        "1" => true,
                        _ => return None,
                    });
                }
                "re" => {
                    decimal(value).filter(|&n| n >= 1)?;
                    others.push((key, value));
                }
                "d" => {
                    if !is_digest(value) {
                        return None;
                    }
                    others.push((key, value));
                }
                "h" => {
                    if !is_handle(value) {
                        return None;
                    }
                    others.push((key, value));
                }
                "o" => {
                    offset(value)?;
                    others.push((key, value));
                }
                "end" => {
                    if !matches!(value, "0" | "1") {
                        return None;
                    }
                    others.push((key, value));
                }
                "e" => {
                    if !is_code(value) {
                        return None;
                    }
                    others.push((key, value));
                }
                _ => others.push((key, value)),
            }
        }
    }
    Some(Chunk {
        kind,
        id: id?,
        seq: seq.unwrap_or(0),
        more: more.unwrap_or(false),
        control: others,
        payload,
    })
}

// ---- messages out of chunks ------------------------------------------------

struct Open {
    kind: String,
    id: u32,
    /// The `seq` the next chunk must have.
    next: u64,
    control: Vec<(String, String)>,
    payload: String,
}

/// Puts chunks together (specification §4.4).
struct Assembler {
    message_limit: usize,
    open: Option<Open>,
}

impl Assembler {
    fn chunk(&mut self, chunk: Chunk<'_>, out: &mut Vec<Event>) {
        if chunk.seq == 0 {
            if self.open.take().is_some() {
                out.push(Event::Error(DecodeError::Abandoned));
            }
            self.open = Some(Open {
                kind: chunk.kind.to_owned(),
                id: chunk.id,
                next: 0,
                control: chunk
                    .control
                    .iter()
                    .map(|&(key, value)| (key.to_owned(), value.to_owned()))
                    .collect(),
                payload: String::new(),
            });
        } else {
            let continues = self.open.as_ref().is_some_and(|open| {
                open.kind == chunk.kind && open.id == chunk.id && open.next == u64::from(chunk.seq)
            });
            if !continues {
                self.open = None;
                out.push(Event::Error(DecodeError::ChunkOrder));
                return;
            }
        }
        let Some(open) = self.open.as_mut() else {
            return;
        };
        open.next += 1;
        open.payload.push_str(chunk.payload);
        if open.payload.len() > self.message_limit {
            self.open = None;
            out.push(Event::Error(DecodeError::MessageTooLong));
            return;
        }
        if !chunk.more {
            if let Some(done) = self.open.take() {
                out.push(Event::Message(Message {
                    kind: done.kind,
                    id: done.id,
                    control: done.control,
                    payload: done.payload,
                }));
            }
        }
    }
}

// ---- the body decoder ------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Body {
    /// Between strings.
    Idle,
    /// A string that may still be this protocol's.
    Collecting,
    /// A string that is not, or that has been dropped: its rest is ignored.
    Skipping,
}

/// Reads frames from APC bodies a terminal's parser hands over in slices.
///
/// Call [`start`](Self::start) when an APC string begins, [`put`](Self::put) with
/// each piece of its body as the parser delivers it and [`end`](Self::end) when it
/// ends; `end` takes whether the parser cancelled the string instead of
/// terminating it. A string whose first bytes are not `pencere;` is dropped at
/// the first byte that differs, without buffering and without an error.
pub struct BodyDecoder {
    limits: WireLimits,
    state: Body,
    body: Vec<u8>,
    assembler: Assembler,
}

impl BodyDecoder {
    /// A decoder for the frames of `direction`.
    pub fn new(direction: Direction) -> Self {
        let limits = direction.limits();
        Self {
            limits,
            state: Body::Idle,
            body: Vec::new(),
            assembler: Assembler {
                message_limit: limits.message,
                open: None,
            },
        }
    }

    /// An APC string begins. A string still open is dropped without a word.
    pub fn start(&mut self) {
        self.body.clear();
        self.state = Body::Collecting;
    }

    /// A piece of the body of the string that began. Bytes of a string that is
    /// not being collected are ignored.
    pub fn put(&mut self, data: &[u8], out: &mut Vec<Event>) {
        self.push(data, out);
    }

    /// The string ended; `cancelled` is whether the parser abandoned it (a `CAN`
    /// or `SUB`, an `ESC` followed by anything but `\`) rather than terminated it.
    pub fn end(&mut self, cancelled: bool, out: &mut Vec<Event>) {
        let state = std::mem::replace(&mut self.state, Body::Idle);
        let body = std::mem::take(&mut self.body);
        // A string that ended before `pencere;` is complete is not ours.
        if state != Body::Collecting || body.len() < PREFIX.len() {
            return;
        }
        if cancelled {
            out.push(Event::Error(DecodeError::Malformed));
            return;
        }
        match parse_chunk(&body) {
            Some(chunk) => self.assembler.chunk(chunk, out),
            None => out.push(Event::Error(DecodeError::Malformed)),
        }
    }

    /// Whether a string that may be ours is being collected.
    fn collecting(&self) -> bool {
        self.state == Body::Collecting
    }

    /// Gives up the string without a word (the stream decoder, once the string is
    /// over inside a slice).
    fn drop_string(&mut self) {
        self.state = Body::Idle;
        self.body.clear();
    }

    /// Takes the bytes of `data` that belong to the string and returns how many it
    /// used: fewer than `data.len()` when the string ended inside it (the byte
    /// that ended it is used, the rest is the caller's).
    fn push(&mut self, data: &[u8], out: &mut Vec<Event>) -> usize {
        if self.state != Body::Collecting {
            return data.len();
        }
        let mut used = 0;
        while self.body.len() < PREFIX.len() {
            let Some(&byte) = data.get(used) else {
                return used;
            };
            used += 1;
            if PREFIX.get(self.body.len()) != Some(&byte) {
                self.state = Body::Skipping;
                return used;
            }
            self.body.push(byte);
        }
        let rest = data.get(used..).unwrap_or_default();
        let room = self
            .limits
            .frame
            .saturating_sub(FRAME_CLOSURE)
            .saturating_sub(self.body.len());
        let bad = rest.iter().position(|b| !(0x20..=0x7e).contains(b));
        let printable = bad.unwrap_or(rest.len());
        if printable > room {
            self.body
                .extend_from_slice(rest.get(..room).unwrap_or_default());
            self.state = Body::Skipping;
            out.push(Event::Error(DecodeError::TooLong));
            return used + room + 1;
        }
        self.body
            .extend_from_slice(rest.get(..printable).unwrap_or_default());
        used += printable;
        if bad.is_some() {
            self.state = Body::Skipping;
            out.push(Event::Error(DecodeError::Malformed));
            return used + 1;
        }
        used
    }
}

// ---- the stream decoder ----------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scan {
    Ground,
    Esc,
    Body,
    BodyEsc,
}

/// Reads frames from raw bytes (specification §4.7): it finds `ESC _` in the
/// noise, collects what follows and delivers messages and errors as they end.
/// It may be fed in pieces of any size, down to one byte; the result does not
/// depend on where they are cut. A stream that ends inside a frame produces
/// nothing.
pub struct StreamDecoder {
    scan: Scan,
    body: BodyDecoder,
}

impl StreamDecoder {
    /// A decoder for the frames of `direction`.
    pub fn new(direction: Direction) -> Self {
        Self {
            scan: Scan::Ground,
            body: BodyDecoder::new(direction),
        }
    }

    /// Feeds the next bytes of the stream.
    pub fn feed(&mut self, data: &[u8], out: &mut Vec<Event>) {
        let mut rest = data;
        while let Some(&byte) = rest.first() {
            match self.scan {
                Scan::Ground => match rest.iter().position(|&b| b == ESC) {
                    Some(at) => {
                        rest = rest.get(at + 1..).unwrap_or_default();
                        self.scan = Scan::Esc;
                    }
                    None => return,
                },
                Scan::Esc => {
                    rest = rest.get(1..).unwrap_or_default();
                    match byte {
                        b'_' => {
                            self.body.start();
                            self.scan = Scan::Body;
                        }
                        ESC => {}
                        _ => self.scan = Scan::Ground,
                    }
                }
                Scan::Body => {
                    let end = rest.iter().position(|&b| b == ESC).unwrap_or(rest.len());
                    let piece = rest.get(..end).unwrap_or_default();
                    let used = self.body.push(piece, out);
                    if !self.body.collecting() {
                        // The string is over (not ours, malformed or too long):
                        // scanning resumes after the byte that ended it.
                        self.body.drop_string();
                        self.scan = Scan::Ground;
                        rest = rest.get(used..).unwrap_or_default();
                    } else if end < rest.len() {
                        rest = rest.get(end + 1..).unwrap_or_default();
                        self.scan = Scan::BodyEsc;
                    } else {
                        return;
                    }
                }
                Scan::BodyEsc => {
                    rest = rest.get(1..).unwrap_or_default();
                    match byte {
                        b'\\' => {
                            self.body.end(false, out);
                            self.scan = Scan::Ground;
                        }
                        b'_' => {
                            self.body.end(true, out);
                            self.body.start();
                            self.scan = Scan::Body;
                        }
                        ESC => {
                            self.body.end(true, out);
                            self.scan = Scan::Esc;
                        }
                        _ => {
                            self.body.end(true, out);
                            self.scan = Scan::Ground;
                        }
                    }
                }
            }
        }
    }
}

// ---- writing ---------------------------------------------------------------

/// Counts a sender's message ids: from 1, upwards, each used once (§4.4). After
/// 4294967295 it starts over at 1; nothing sends that many in one pane's life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ids {
    next: u32,
}

impl Ids {
    /// A counter whose first id is 1.
    pub const fn new() -> Self {
        Self { next: 1 }
    }

    /// The next id.
    pub fn next_id(&mut self) -> u32 {
        let id = self.next;
        self.next = id.checked_add(1).unwrap_or(1);
        id
    }
}

impl Default for Ids {
    fn default() -> Self {
        Self::new()
    }
}

/// A message to write.
#[derive(Debug, Clone, Copy)]
pub struct Outgoing<'a> {
    /// The kind, 1 to 16 of `a-z` and `-`.
    pub kind: &'a str,
    /// The id, at least 1.
    pub id: u32,
    /// The `re` parameter, an id.
    pub re: Option<u32>,
    /// The `d` parameter, 64 lowercase hex digits.
    pub digest: Option<&'a str>,
    /// The parameters of the messages about the person's files: `h`, `o`, `end`, `e`.
    pub file: FileParams<'a>,
    /// The payload: printable ASCII, 0x20 to 0x7E. JSON must have been through
    /// [`crate::json::to_wire`]; base64 satisfies it by construction.
    pub payload: &'a str,
}

/// The parameters only the messages about the person's files carry (§4.3): written after `d`, in
/// the order `h`, `o`, `end`, `e`, on the first chunk.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileParams<'a> {
    /// `h`: the handle, 32 lowercase hex digits.
    pub handle: Option<&'a str>,
    /// `o`: the offset, at most [`MAX_OFFSET`].
    pub offset: Option<u64>,
    /// `end`: written as `end=1` when true, left out when false.
    pub end: bool,
    /// `e`: an error code, 1 to 32 of `a-z` and `-`.
    pub error: Option<&'a str>,
}

impl FileParams<'_> {
    /// None of them: what every message but `write` and `data` carries.
    pub const NONE: Self = Self {
        handle: None,
        offset: None,
        end: false,
        error: None,
    };
}

/// How a message is cut into chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    /// Every chunk but the last holds as much payload as its direction's frame
    /// limit lets it (what §4.4 recommends, and what the canonical fixtures are).
    Fill,
    /// Every chunk but the last holds this much payload (rounded down to a
    /// multiple of four for `file`, `write` and `data`).
    Fixed(usize),
}

/// Why a message could not be written. Nothing is written in that case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WriteError {
    /// The kind does not follow the grammar.
    Kind,
    /// The id is 0.
    Id,
    /// The `d` parameter is not 64 lowercase hex digits.
    Digest,
    /// The `h`, `o` or `e` parameter is outside what §4.3 allows.
    FileParam,
    /// The payload holds a byte a frame may not carry (a C0 control, DEL, a byte
    /// from 0x80 up). A reply written with one would be typed input to whatever
    /// reads the tty next.
    Payload,
    /// The payload is over the message limit of the direction.
    MessageTooLong,
    /// A frame cannot hold any payload, or is over the frame limit.
    NoRoom,
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Kind => "the kind is not 1 to 16 of a-z and -",
            Self::Id => "the id is 0",
            Self::Digest => "the digest is not 64 lowercase hex digits",
            Self::FileParam => "a handle, an offset or an error code is outside its range",
            Self::Payload => "the payload has a byte a frame may not carry",
            Self::MessageTooLong => "the payload is over the message limit",
            Self::NoRoom => "no frame can carry this",
        })
    }
}

impl std::error::Error for WriteError {}

fn control_text(msg: &Outgoing<'_>, seq: u32, more: bool) -> String {
    let mut text = format!("id={}", msg.id);
    if seq > 0 {
        text.push_str(&format!(",seq={seq}"));
    }
    if more {
        text.push_str(",more=1");
    }
    if seq == 0 {
        if let Some(re) = msg.re {
            text.push_str(&format!(",re={re}"));
        }
        if let Some(digest) = msg.digest {
            text.push_str(&format!(",d={digest}"));
        }
        if let Some(handle) = msg.file.handle {
            text.push_str(&format!(",h={handle}"));
        }
        if let Some(offset) = msg.file.offset {
            text.push_str(&format!(",o={offset}"));
        }
        if msg.file.end {
            text.push_str(",end=1");
        }
        if let Some(error) = msg.file.error {
            text.push_str(&format!(",e={error}"));
        }
    }
    text
}

fn push_frame(out: &mut Vec<u8>, kind: &str, control: &str, payload: &[u8]) {
    out.extend_from_slice(&[ESC, b'_']);
    out.extend_from_slice(PREFIX);
    out.extend_from_slice(kind.as_bytes());
    out.push(b';');
    out.extend_from_slice(control.as_bytes());
    out.push(b';');
    out.extend_from_slice(payload);
    out.extend_from_slice(&[ESC, b'\\']);
}

/// Writes `msg` as one or more frames for `direction`, appending them to `out`:
/// keys in the order `id`, `seq`, `more`, `re`, `d`, `h`, `o`, `end`, `e`, `seq`, `more` and `end`
/// left out when they are 0, the keys after `more` on the first chunk only (§4.3, §4.4).
///
/// The same message and the same split give the same bytes on every
/// implementation, which is what the fixtures rely on.
///
/// # Errors
///
/// Refuses a message that does not follow the wire (see [`WriteError`]), and
/// leaves `out` as it was.
pub fn encode(
    direction: Direction,
    msg: &Outgoing<'_>,
    split: Split,
    out: &mut Vec<u8>,
) -> Result<(), WriteError> {
    let limits = direction.limits();
    if !is_kind(msg.kind) {
        return Err(WriteError::Kind);
    }
    if msg.id == 0 || msg.re == Some(0) {
        return Err(WriteError::Id);
    }
    if msg.digest.is_some_and(|d| !is_digest(d)) {
        return Err(WriteError::Digest);
    }
    if msg.file.handle.is_some_and(|h| !is_handle(h))
        || msg.file.offset.is_some_and(|o| o > MAX_OFFSET)
        || msg.file.error.is_some_and(|e| !is_code(e))
    {
        return Err(WriteError::FileParam);
    }
    if !crate::json::is_frame_text(msg.payload) {
        return Err(WriteError::Payload);
    }
    if msg.payload.len() > limits.message {
        return Err(WriteError::MessageTooLong);
    }
    let start = out.len();
    let result = encode_chunks(limits, msg, split, out);
    if result.is_err() {
        out.truncate(start);
    }
    result
}

fn encode_chunks(
    limits: WireLimits,
    msg: &Outgoing<'_>,
    split: Split,
    out: &mut Vec<u8>,
) -> Result<(), WriteError> {
    // Base64 is cut on four-character groups, so that a receiver may decode as it goes (§4.4).
    let align = matches!(msg.kind, "file" | "write" | "data");
    let payload = msg.payload.as_bytes();
    let room = |control: &str| {
        limits
            .frame
            .saturating_sub(FRAME_FIXED + msg.kind.len() + control.len())
    };
    let mut offset = 0usize;
    let mut seq = 0u32;
    loop {
        let rest = payload.get(offset..).unwrap_or_default();
        let last = control_text(msg, seq, false);
        let last_cap = match split {
            Split::Fill => room(&last),
            Split::Fixed(n) => n,
        };
        if rest.len() <= last_cap {
            if FRAME_FIXED + msg.kind.len() + last.len() + rest.len() > limits.frame {
                return Err(WriteError::NoRoom);
            }
            push_frame(out, msg.kind, &last, rest);
            return Ok(());
        }
        let more = control_text(msg, seq, true);
        let mut cap = match split {
            Split::Fill => room(&more),
            Split::Fixed(n) => n,
        };
        if align {
            cap -= cap % 4;
        }
        let Some(piece) = rest.get(..cap).filter(|piece| !piece.is_empty()) else {
            return Err(WriteError::NoRoom);
        };
        if FRAME_FIXED + msg.kind.len() + more.len() + piece.len() > limits.frame {
            return Err(WriteError::NoRoom);
        }
        push_frame(out, msg.kind, &more, piece);
        offset += cap;
        seq = seq.checked_add(1).ok_or(WriteError::NoRoom)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(direction: Direction, bytes: &[u8]) -> Vec<Event> {
        let mut decoder = StreamDecoder::new(direction);
        let mut out = Vec::new();
        decoder.feed(bytes, &mut out);
        out
    }

    fn messages(events: &[Event]) -> Vec<&Message> {
        events
            .iter()
            .filter_map(|event| match event {
                Event::Message(m) => Some(m),
                Event::Error(_) => None,
            })
            .collect()
    }

    fn errors(events: &[Event]) -> Vec<DecodeError> {
        events
            .iter()
            .filter_map(|event| match event {
                Event::Error(e) => Some(*e),
                Event::Message(_) => None,
            })
            .collect()
    }

    fn frame(body: &str) -> Vec<u8> {
        let mut bytes = vec![ESC, b'_'];
        bytes.extend_from_slice(body.as_bytes());
        bytes.extend_from_slice(&[ESC, b'\\']);
        bytes
    }

    #[test]
    fn a_hello_is_read_and_its_writer_gives_it_back() {
        let bytes = frame(r#"pencere;hello;id=1;{"versions":[1]}"#);
        let events = stream(Direction::ToHost, &bytes);
        let [Event::Message(m)] = events.as_slice() else {
            panic!("{events:?}")
        };
        assert_eq!(
            (m.kind.as_str(), m.id, m.payload.as_str()),
            ("hello", 1, r#"{"versions":[1]}"#)
        );
        let mut written = Vec::new();
        let msg = Outgoing {
            kind: "hello",
            id: 1,
            re: None,
            digest: None,
            file: FileParams::NONE,
            payload: &m.payload,
        };
        encode(Direction::ToHost, &msg, Split::Fill, &mut written).unwrap();
        assert_eq!(written, bytes);
    }

    #[test]
    fn keys_may_come_in_any_order_and_unknown_ones_are_kept_for_the_receiver() {
        let events = stream(
            Direction::ToHost,
            &frame("pencere;hello;x=7,more=0,seq=0,id=3;{}"),
        );
        let [Event::Message(m)] = events.as_slice() else {
            panic!("{events:?}")
        };
        assert_eq!(m.id, 3);
        assert_eq!(m.control, vec![("x".to_owned(), "7".to_owned())]);
    }

    #[test]
    fn values_outside_the_table_are_malformed_not_ignored() {
        for control in [
            "id=0",
            "id=01",
            "id=4294967296",
            "id=1,seq=00",
            "id=1,seq=-1",
            "id=1,more=2",
            "id=1,re=0",
            "id=1,re=x",
            "id=1,d=ABCDEF",
            "id=1,id=2",
            "id=1,",
            "id=1,seq",
            "id=+1",
            "",
        ] {
            let events = stream(
                Direction::ToHost,
                &frame(&format!("pencere;hello;{control};{{}}")),
            );
            assert_eq!(errors(&events), vec![DecodeError::Malformed], "{control:?}");
            assert!(messages(&events).is_empty(), "{control:?}");
        }
    }

    #[test]
    fn a_string_that_is_not_ours_is_dropped_at_its_first_differing_byte() {
        // A kitty graphics command and an APC of another shape, then a hello.
        let mut bytes = frame("Ga=T,f=100;AAAA");
        bytes.extend(frame("pencerf;nope"));
        bytes.extend(frame(r#"pencere;hello;id=1;{"versions":[1]}"#));
        let events = stream(Direction::ToHost, &bytes);
        assert_eq!(messages(&events).len(), 1);
        assert!(errors(&events).is_empty());
    }

    #[test]
    fn a_string_that_ends_before_the_prefix_is_complete_is_not_ours() {
        for ending in [
            vec![ESC, b'\\'],
            vec![ESC, b'_'],
            vec![ESC, ESC],
            vec![ESC, b'X'],
        ] {
            let mut bytes = vec![ESC, b'_'];
            bytes.extend_from_slice(b"pencer");
            bytes.extend_from_slice(&ending);
            let events = stream(Direction::ToHost, &bytes);
            assert!(events.is_empty(), "{ending:?}: {events:?}");
        }
    }

    #[test]
    fn an_escape_inside_a_frame_ends_it_and_scanning_goes_on_at_the_next_byte() {
        let mut bytes = b"\x1b_pencere;msg;id=1;{\"a\":".to_vec();
        bytes.extend(frame(r#"pencere;hello;id=2;{"versions":[1]}"#));
        let events = stream(Direction::ToHost, &bytes);
        assert_eq!(errors(&events), vec![DecodeError::Malformed]);
        assert_eq!(messages(&events).len(), 1);
        // ESC ESC _ : the second escape begins a new sequence.
        let mut bytes = b"\x1b_pencere;msg;id=1;{\x1b".to_vec();
        bytes.extend(frame(r#"pencere;hello;id=2;{"versions":[1]}"#));
        let events = stream(Direction::ToHost, &bytes);
        assert_eq!(errors(&events), vec![DecodeError::Malformed]);
        assert_eq!(messages(&events).len(), 1);
    }

    #[test]
    fn the_frame_limit_is_per_direction_and_exact() {
        for (direction, limit) in [(Direction::ToHost, 4096usize), (Direction::ToProgram, 511)] {
            let head = r#"pencere;msg;id=1;{"p":""#;
            let tail = r#""}"#;
            let fill = limit - FRAME_CLOSURE - head.len() - tail.len();
            let body = |n: usize| format!("{head}{}{tail}", "a".repeat(n));
            let ok = stream(direction, &frame(&body(fill)));
            assert_eq!(messages(&ok).len(), 1, "{direction:?}");
            let over = stream(direction, &frame(&body(fill + 1)));
            assert_eq!(errors(&over), vec![DecodeError::TooLong], "{direction:?}");
            assert!(messages(&over).is_empty());
        }
    }

    #[test]
    fn a_message_is_dropped_as_soon_as_it_is_over_its_limit() {
        for direction in [Direction::ToHost, Direction::ToProgram] {
            let limit = direction.limits().message;
            // A payload one byte over the limit, in full frames.
            let payload = "a".repeat(limit + 1);
            let mut bytes = Vec::new();
            let msg = Outgoing {
                kind: "msg",
                id: 9,
                re: None,
                digest: None,
                file: FileParams::NONE,
                payload: &payload,
            };
            assert_eq!(
                encode(direction, &msg, Split::Fill, &mut bytes),
                Err(WriteError::MessageTooLong)
            );
            // A writer refuses; a hostile one does not, so build the chunks by hand.
            let frame_limit = direction.limits().frame;
            let mut seq = 0u32;
            let mut sent = 0usize;
            while sent <= limit {
                let ctl = if seq == 0 {
                    "id=9,more=1".to_owned()
                } else {
                    format!("id=9,seq={seq},more=1")
                };
                let room = frame_limit - FRAME_FIXED - 3 - ctl.len();
                let take = room.min(limit + 1 - sent);
                push_frame(&mut bytes, "msg", &ctl, &payload.as_bytes()[..take]);
                sent += take;
                seq += 1;
            }
            let events = stream(direction, &bytes);
            assert_eq!(
                errors(&events),
                vec![DecodeError::MessageTooLong],
                "{direction:?}"
            );
            assert!(messages(&events).is_empty());
        }
    }

    #[test]
    fn chunks_must_follow_one_another() {
        let mut bytes = frame("pencere;msg;id=4,more=1;aa");
        bytes.extend(frame("pencere;msg;id=4,seq=1,more=1;bb"));
        bytes.extend(frame("pencere;msg;id=4,seq=2;cc"));
        let events = stream(Direction::ToHost, &bytes);
        let [Event::Message(m)] = events.as_slice() else {
            panic!("{events:?}")
        };
        assert_eq!(m.payload, "aabbcc");
        // A new message abandons the open one.
        let mut bytes = frame("pencere;msg;id=4,more=1;aa");
        bytes.extend(frame("pencere;msg;id=5;zz"));
        let events = stream(Direction::ToHost, &bytes);
        assert_eq!(errors(&events), vec![DecodeError::Abandoned]);
        assert_eq!(messages(&events).first().map(|m| m.id), Some(5));
        // A repeated chunk does not continue it, and the message is gone.
        let mut bytes = frame("pencere;msg;id=4,more=1;aa");
        bytes.extend(frame("pencere;msg;id=4,seq=1,more=1;bb"));
        bytes.extend(frame("pencere;msg;id=4,seq=1;bb"));
        let events = stream(Direction::ToHost, &bytes);
        assert_eq!(errors(&events), vec![DecodeError::ChunkOrder]);
        assert!(messages(&events).is_empty());
        // The parameters of a message are those of its first chunk.
        let mut bytes = frame(&format!("pencere;file;id=4,more=1,d={};aa", "0".repeat(64)));
        bytes.extend(frame("pencere;file;id=4,seq=1,re=9;bb"));
        let events = stream(Direction::ToHost, &bytes);
        let [Event::Message(m)] = events.as_slice() else {
            panic!("{events:?}")
        };
        assert_eq!(m.digest(), Some("0".repeat(64).as_str()));
        assert_eq!(m.re(), None);
    }

    #[test]
    fn a_decode_does_not_depend_on_where_the_stream_is_cut() {
        let mut bytes = b"noise\x1b_Gx\x1b\\".to_vec();
        bytes.extend(frame("pencere;msg;id=4,more=1;aa"));
        bytes.extend(b"\x1b[?62;4cls\r");
        bytes.extend(frame("pencere;msg;id=4,seq=1;bb"));
        bytes.extend(frame("pencere;bad;id=0;x"));
        let whole = stream(Direction::ToHost, &bytes);
        assert_eq!(messages(&whole).len(), 1);
        for at in 0..=bytes.len() {
            let mut decoder = StreamDecoder::new(Direction::ToHost);
            let mut out = Vec::new();
            decoder.feed(&bytes[..at], &mut out);
            decoder.feed(&bytes[at..], &mut out);
            assert_eq!(out, whole, "cut at {at}");
        }
        let mut decoder = StreamDecoder::new(Direction::ToHost);
        let mut out = Vec::new();
        for byte in &bytes {
            decoder.feed(std::slice::from_ref(byte), &mut out);
        }
        assert_eq!(out, whole);
    }

    #[test]
    fn the_body_decoder_agrees_with_the_stream_decoder() {
        let bodies = [
            r#"pencere;hello;id=1;{"versions":[1]}"#.to_owned(),
            "pencere;msg;id=2,more=1;aa".to_owned(),
            "pencere;msg;id=2,seq=1;bb".to_owned(),
            "pencere;msg;id=0;x".to_owned(),
            "Gnot ours".to_owned(),
            "pencere;msg;id=3;".to_owned() + &"x".repeat(5000),
        ];
        let mut bytes = Vec::new();
        for body in &bodies {
            bytes.extend(frame(body));
        }
        let expected = stream(Direction::ToHost, &bytes);
        for piece in [1usize, 2, 3, 7, 4096] {
            let mut decoder = BodyDecoder::new(Direction::ToHost);
            let mut out = Vec::new();
            for body in &bodies {
                decoder.start();
                for part in body.as_bytes().chunks(piece) {
                    decoder.put(part, &mut out);
                }
                decoder.end(false, &mut out);
            }
            assert_eq!(out, expected, "pieces of {piece}");
        }
    }

    #[test]
    fn a_cancelled_string_of_ours_is_malformed_and_one_of_others_is_silent() {
        let mut decoder = BodyDecoder::new(Direction::ToHost);
        let mut out = Vec::new();
        decoder.start();
        decoder.put(b"pencere;msg;id=1;{", &mut out);
        decoder.end(true, &mut out);
        decoder.start();
        decoder.put(b"Gxx", &mut out);
        decoder.end(true, &mut out);
        assert_eq!(out, vec![Event::Error(DecodeError::Malformed)]);
    }

    #[test]
    fn bytes_a_parser_passes_through_are_refused_inside_a_body() {
        for byte in [0x00u8, 0x0a, 0x0d, 0x03, 0x7f, 0x80, 0x9c, 0xff] {
            let mut decoder = BodyDecoder::new(Direction::ToProgram);
            let mut out = Vec::new();
            decoder.start();
            decoder.put(b"pencere;msg;id=1;{", &mut out);
            decoder.put(&[byte], &mut out);
            decoder.put(b"}", &mut out);
            decoder.end(false, &mut out);
            assert_eq!(out, vec![Event::Error(DecodeError::Malformed)], "{byte:#x}");
        }
    }

    #[test]
    fn the_writer_cuts_a_message_as_the_wire_says() {
        let digest = "ab".repeat(32);
        // A file: chunks but the last are multiples of four, re and d on the first.
        let payload = "A".repeat(1300);
        let msg = Outgoing {
            kind: "file",
            id: 12,
            re: None,
            digest: Some(&digest),
            file: FileParams::NONE,
            payload: &payload,
        };
        let mut bytes = Vec::new();
        encode(Direction::ToProgram, &msg, Split::Fill, &mut bytes).unwrap();
        let events = stream(Direction::ToProgram, &bytes);
        let [Event::Message(m)] = events.as_slice() else {
            panic!("{events:?}")
        };
        assert_eq!(m.payload, payload);
        assert_eq!(m.digest(), Some(digest.as_str()));
        let text = String::from_utf8(bytes).unwrap();
        let frames: Vec<&str> = text.split("\x1b\\").filter(|f| !f.is_empty()).collect();
        assert!(frames.len() >= 3);
        for f in &frames {
            assert!(f.len() + 2 <= 511, "{}", f.len());
        }
        assert!(frames[0].starts_with("\x1b_pencere;file;id=12,more=1,d="));
        assert!(frames[1].starts_with("\x1b_pencere;file;id=12,seq=1,more=1;"));
        for f in &frames[..frames.len() - 1] {
            let body = f.rsplit_once(';').unwrap().1;
            assert_eq!(body.len() % 4, 0, "{}", body.len());
        }
        assert!(frames[frames.len() - 1].starts_with(&format!(
            "\x1b_pencere;file;id=12,seq={};",
            frames.len() - 1
        )));
    }

    #[test]
    fn the_writer_refuses_what_a_frame_may_not_carry() {
        let mut out = vec![1, 2, 3];
        let base = Outgoing {
            kind: "msg",
            id: 1,
            re: None,
            digest: None,
            file: FileParams::NONE,
            payload: "{}",
        };
        for payload in [
            "a\nb",
            "a\rb",
            "a\u{3}b",
            "a\u{7f}b",
            "caf\u{e9}",
            "a\u{1b}b",
            "a\tb",
        ] {
            let msg = Outgoing { payload, ..base };
            assert_eq!(
                encode(Direction::ToProgram, &msg, Split::Fill, &mut out),
                Err(WriteError::Payload),
                "{payload:?}"
            );
        }
        for (msg, error) in [
            (
                Outgoing {
                    kind: "Msg",
                    ..base
                },
                WriteError::Kind,
            ),
            (Outgoing { kind: "", ..base }, WriteError::Kind),
            (
                Outgoing {
                    kind: "a".repeat(17).leak(),
                    ..base
                },
                WriteError::Kind,
            ),
            (Outgoing { id: 0, ..base }, WriteError::Id),
            (
                Outgoing {
                    re: Some(0),
                    ..base
                },
                WriteError::Id,
            ),
            (
                Outgoing {
                    digest: Some("xyz"),
                    ..base
                },
                WriteError::Digest,
            ),
        ] {
            assert_eq!(
                encode(Direction::ToProgram, &msg, Split::Fill, &mut out),
                Err(error)
            );
        }
        assert_eq!(
            out,
            vec![1, 2, 3],
            "a refused message leaves nothing behind"
        );
        let msg = Outgoing {
            kind: "msg",
            id: 1,
            re: None,
            digest: None,
            file: FileParams::NONE,
            payload: "{}",
        };
        assert_eq!(
            encode(Direction::ToProgram, &msg, Split::Fixed(0), &mut out),
            Err(WriteError::NoRoom),
            "a chunk that can hold nothing does not make progress"
        );
        let long = "a".repeat(40);
        let msg = Outgoing {
            payload: &long,
            ..msg
        };
        assert_eq!(
            encode(Direction::ToProgram, &msg, Split::Fixed(600), &mut out),
            Ok(()),
            "a chunk size over the frame limit does not matter while the message fits one frame"
        );
        let long = "a".repeat(600);
        let msg = Outgoing {
            payload: &long,
            ..msg
        };
        let before = out.len();
        assert_eq!(
            encode(Direction::ToProgram, &msg, Split::Fixed(600), &mut out),
            Err(WriteError::NoRoom)
        );
        assert_eq!(out.len(), before);
    }

    #[test]
    fn ids_count_from_one() {
        let mut ids = Ids::new();
        assert_eq!((ids.next_id(), ids.next_id(), ids.next_id()), (1, 2, 3));
        let mut ids = Ids { next: u32::MAX };
        assert_eq!((ids.next_id(), ids.next_id()), (u32::MAX, 1));
    }

    #[test]
    fn hostile_bytes_never_panic_or_grow_without_bound() {
        // A small xorshift: the same bytes on every run.
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let alphabet: &[&[u8]] = &[
            b"\x1b_",
            b"\x1b\\",
            b"\x1b",
            b"pencere;",
            b"msg;",
            b"hello;",
            b"id=1",
            b"id=1,more=1",
            b"seq=1",
            b",",
            b";",
            b"{}",
            b"\n",
            b"\x7f",
            b"\xe2\x9c\x80",
            b"G",
            b"x",
        ];
        for direction in [Direction::ToHost, Direction::ToProgram] {
            let mut stream = StreamDecoder::new(direction);
            let mut body = BodyDecoder::new(direction);
            let mut out = Vec::new();
            for _ in 0..20_000 {
                let piece = alphabet[(next() % alphabet.len() as u64) as usize];
                stream.feed(piece, &mut out);
                match next() % 5 {
                    0 => body.start(),
                    1 | 2 => body.put(piece, &mut out),
                    3 => body.end(false, &mut out),
                    _ => body.end(true, &mut out),
                }
                out.clear();
            }
        }
    }
}
