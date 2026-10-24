//! The host side of pencere: what a terminal needs to let a program in one of its panes show a
//! view in that pane, whatever the view is: a WebAssembly component or a Lua script the terminal
//! runs and draws itself, or a web page in a sandboxed web view.
//!
//! The crate does no I/O. A terminal feeds it the bodies of the APC sequences
//! it parses from a pane's output, in whatever pieces its parser hands them
//! over, and writes the replies the crate builds into that pane's input. The
//! view itself, its sandbox and the bundle store belong to the terminal; the
//! crate holds the rules they follow, so every terminal that embeds it follows
//! the same ones.
//!
//! Every byte the crate reads comes from a stream anyone can write to: it
//! never panics on it, allocates for what it has not seen, or buffers more than
//! a limit of the specification allows.
//!
//! * [`frame`], [`message`], [`json`], [`base64`]: the wire, which a program shares with a
//!   terminal and so is the `pencere-wire` crate's, re-exported here. The APC strings read
//!   ([`frame::BodyDecoder`] for bodies a terminal's parser hands over,
//!   [`frame::StreamDecoder`] for raw bytes) and written ([`frame::encode`]), and the payloads
//!   of the handshake and of closing.
//! * [`session`]: one pane's conversation with one program, as a state machine
//!   that takes what the terminal learns and returns what the terminal does. It
//!   carries the six messages of a program and its view between the two without reading
//!   them, and speaks the bridge of a page. What it writes to the program is [`session::Action::Write`]
//!   (its own words) and [`session::Action::WriteView`] (what a view says): a terminal that holds
//!   writes back for the tty bounds the second with [`session::Session::backlog`], which it calls
//!   before it holds one, so that a program that does not read cannot make a view cost memory
//!   without end.
//! * [`bundle`]: the manifests and digests of the wire, and the [`bundle::Store`]
//!   the terminal supplies.
//! * [`sandbox`]: what a page asks to reach, what the person is asked, and the
//!   content security policy built from the answer.
//! * [`theme`]: the terminal's look, as the roles a page is told.
//! * [`icon`]: the icons a program gives for itself, as a terminal may draw them.
//! * [`token`]: the single-use token and the source of bytes it is made from.
//!
//! The specification is `spec/draft/pencere.md`; its fixtures are the tests of
//! this crate and of the TypeScript SDK.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod bundle;
pub mod icon;
pub mod sandbox;
pub mod session;
mod text;
pub mod theme;
pub mod token;

pub use pencere_wire::{base64, frame, json, message};
