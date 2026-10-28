//! What a terminal does with a program's pane that is the same whatever it shows of the view: how
//! it reads what the program writes — cut where it must act in step with the stream (a question to
//! answer, the alternate screen switched), each frame of the protocol in its place — the session it
//! holds the conversation with (what it offers, its theme and its token), and its answer to the
//! question a program asks of the terminal it runs in.
//!
//! The stand-in terminal (the `pencere-fake-host` command) and a program whose view is run for
//! real ([`crate::program`]) read a pane the same way, here.

use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;

use pencere_host::frame::{Direction, Event, StreamDecoder};
use pencere_host::message::HostInfo;
use pencere_host::session::{Config, Observed, Offer, Size, ViewKind};
use pencere_host::theme::{Rgb, Theme};

use crate::log;
use crate::pty;
use crate::screen::{Screen, Seen};

/// What a terminal answers to `CSI c`.
pub const DEVICE_ATTRIBUTES: &[u8] = b"\x1b[?62;4c";

/// The version of the interface of a view the terminal draws itself (a WebAssembly component or a
/// Lua script) that the stand-in speaks.
pub const KIT_VERSION: &str = "2.0";

/// A pixel is not a cell, but the view's container is told in pixels.
pub const CELL_WIDTH: u32 = 8;
pub const CELL_HEIGHT: u32 = 16;

/// What the session of a pane of `size` cells is: the stand-in host, a dark theme, and the views it
/// opens besides a page — a component and a Lua script, which a terminal draws itself.
pub fn config(size: pty::Size) -> Config {
    let theme = Theme::new(
        Rgb(0x11, 0x13, 0x18),
        Rgb(0xd8, 0xdb, 0xe2),
        "Menlo, monospace",
    );
    let host = HostInfo {
        name: "pencere-fake-host".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    };
    let mut config = Config::new(
        host,
        theme,
        Size {
            width: u32::from(size.cols) * CELL_WIDTH,
            height: u32::from(size.rows) * CELL_HEIGHT,
        },
    );
    config
        .views
        .push(Offer::new(ViewKind::Wasm, &[KIT_VERSION]));
    config.views.push(Offer::new(ViewKind::Lua, &[KIT_VERSION]));
    config
}

/// Where a session's tokens come from.
pub type Entropy = Box<dyn FnMut() -> [u8; 16]>;

/// The operating system's random bytes, which the token is made of, from a descriptor opened before
/// anything runs: a host that cannot get them has no token to give and does not start. Reading a
/// descriptor that is open does not fail; if it ever did, stopping is better than a token that is
/// not random.
pub fn entropy(mut urandom: File) -> Entropy {
    Box::new(move || {
        let mut bytes = [0u8; 16];
        if let Err(error) = urandom.read_exact(&mut bytes) {
            log::line(
                "host",
                format!("no secure random bytes for a token: {error}"),
            );
            std::process::exit(70);
        }
        bytes
    })
}

/// A piece of what a program wrote, in the order it came.
#[derive(Debug)]
pub enum Piece {
    /// A frame of the protocol, or one that was dropped.
    Frame(Event),
    /// `CSI c`: the terminal answers ([`DEVICE_ATTRIBUTES`]), after what came before it.
    Query,
    /// The alternate screen on (`true`) or off.
    Alternate(bool),
}

/// How a terminal reads a program's output.
pub struct Pane {
    screen: Screen,
    frames: StreamDecoder,
}

impl Default for Pane {
    fn default() -> Self {
        Self::new()
    }
}

impl Pane {
    pub fn new() -> Self {
        Self {
            screen: Screen::new(),
            frames: StreamDecoder::new(Direction::ToHost),
        }
    }

    /// What the program wrote, piece by piece: `each` is given every frame, question and switch of
    /// the screen in the order they are in the stream, and acts on it before the next is read — the
    /// welcome a `hello` makes is written before the answer to a `CSI c` after it (§5.2). Whether
    /// anything was written after the last switch of the screen.
    pub fn read(&mut self, bytes: &[u8], mut each: impl FnMut(Piece)) -> bool {
        let mut start = 0;
        for (at, &byte) in bytes.iter().enumerate() {
            let Some(seen) = self.screen.advance(byte) else {
                continue;
            };
            if let Some(part) = bytes.get(start..=at) {
                self.frames(part, &mut each);
            }
            start = at + 1;
            each(match seen {
                Seen::Query => Piece::Query,
                Seen::Alternate(on) => Piece::Alternate(on),
            });
        }
        if let Some(rest) = bytes.get(start..) {
            self.frames(rest, &mut each);
        }
        bytes.len() > start
    }

    fn frames(&mut self, part: &[u8], each: &mut impl FnMut(Piece)) {
        if part.is_empty() {
            return;
        }
        let mut events = Vec::new();
        self.frames.feed(part, &mut events);
        for event in events {
            each(Piece::Frame(event));
        }
    }
}

/// What a terminal can see of the pane when the program says something, for the checks of §6.2
/// and §6.5: there is no shell here to ask about, so only the screen and the tty's modes.
pub fn observed(alternate_screen: bool, modes: Option<pty::Modes>) -> Observed {
    Observed {
        prompt_editing: false,
        alternate_screen: Some(alternate_screen),
        shell_in_foreground: None,
        tty_canonical_or_echo: modes.map(|m| m.canonical || m.echo),
    }
}

/// Bytes for the pane's input, in the order they go, with which of them a view said
/// (`Action::WriteView`): everything the pane is typed goes through here, so that nothing lands
/// inside a frame (§4.8). The session bounds what a view says that waits ([`Typing::said`]); past
/// the bound the terminal drops the view's bytes it has not begun to write, and keeps its own.
#[derive(Debug, Default)]
pub struct Typing {
    bytes: Vec<u8>,
    /// The parts of `bytes`, first first: how many bytes each, and whether a view said it.
    parts: VecDeque<Part>,
}

#[derive(Debug, Clone, Copy)]
struct Part {
    len: usize,
    view: bool,
    /// Some of it was written already.
    begun: bool,
}

impl Typing {
    /// What waits for the tty, in order.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// The terminal's own bytes (a frame of the session's, an answer, a key of the person's).
    pub fn type_in(&mut self, bytes: &[u8]) {
        self.push(bytes, false);
    }

    /// What a view said, held while the session's bound allows it.
    pub fn view_said(&mut self, bytes: &[u8]) {
        self.push(bytes, true);
    }

    fn push(&mut self, bytes: &[u8], view: bool) {
        if bytes.is_empty() {
            return;
        }
        self.bytes.extend_from_slice(bytes);
        self.parts.push_back(Part {
            len: bytes.len(),
            view,
            begun: false,
        });
    }

    /// The bytes of what views said that wait: what the session bounds.
    pub fn said(&self) -> usize {
        self.parts
            .iter()
            .filter(|part| part.view)
            .map(|part| part.len)
            .sum()
    }

    /// The tty took the first `count` bytes.
    pub fn taken(&mut self, count: usize) {
        let mut left = count.min(self.bytes.len());
        self.bytes.drain(..left);
        while left > 0 {
            let Some(part) = self.parts.front_mut() else {
                break;
            };
            if part.len <= left {
                left -= part.len;
                self.parts.pop_front();
            } else {
                part.len -= left;
                part.begun = true;
                left = 0;
            }
        }
    }

    /// Nobody reads any more: nothing waits.
    pub fn clear(&mut self) {
        self.bytes.clear();
        self.parts.clear();
    }

    /// A view said more than its program reads: what views said that has not begun to be written
    /// goes, and the rest stays in its order.
    pub fn drop_said(&mut self) {
        let mut kept = Vec::with_capacity(self.bytes.len());
        let mut at = 0;
        self.parts.retain(|part| {
            let keep = !part.view || part.begun;
            if keep {
                kept.extend_from_slice(self.bytes.get(at..at + part.len).unwrap_or_default());
            }
            at += part.len;
            keep
        });
        self.bytes = kept;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a view said goes past the bound with the terminal's own bytes after it: the view's
    /// that were not begun go, the terminal's answer to `CSI c` after them stays, and so does the
    /// part of the view's already half written.
    #[test]
    fn a_views_bytes_are_dropped_and_the_terminals_own_kept_wherever_they_are() {
        let mut typing = Typing::default();
        typing.view_said(b"aaaa");
        typing.type_in(b"Q");
        typing.view_said(b"bb");
        assert_eq!(typing.said(), 6);
        typing.taken(2);
        assert_eq!(typing.bytes(), b"aaQbb");
        typing.drop_said();
        assert_eq!(
            typing.bytes(),
            b"aaQ",
            "the begun part and the terminal's own"
        );
        assert_eq!(typing.said(), 2);
        typing.taken(3);
        assert!(typing.is_empty());
        assert_eq!(typing.said(), 0);
    }
}
