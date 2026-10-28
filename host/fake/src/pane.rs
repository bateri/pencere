//! What a terminal does with a program's pane that is the same whatever it shows of the view: how
//! it reads what the program writes — cut where it must act in step with the stream (a question to
//! answer, the alternate screen switched), each frame of the protocol in its place — the session it
//! holds the conversation with (what it offers, its theme and its token), and its answer to the
//! question a program asks of the terminal it runs in.
//!
//! The stand-in terminal (the `pencere-fake-host` command) and a program whose view is run for
//! real ([`crate::program`]) read a pane the same way, here.

use std::fs::File;
use std::io::Read;

use pencere_host::frame::{Direction, Event, StreamDecoder};
use pencere_host::message::HostInfo;
use pencere_host::session::{Config, Offer, Size, ViewKind};
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
