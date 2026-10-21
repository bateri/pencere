//! The host side of pencere: what a terminal needs to let a program in one
//! of its panes show an MCP App view in that pane.
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
//! * [`frame`]: the APC strings of the wire, read ([`frame::BodyDecoder`] for
//!   bodies a terminal's parser hands over, [`frame::StreamDecoder`] for raw
//!   bytes) and written ([`frame::encode`], which refuses a byte a frame may
//!   not carry).
//! * [`message`] and [`jsonrpc`]: the payloads and the JSON-RPC messages the
//!   host reads and writes; [`json`] writes JSON as printable ASCII.
//! * [`session`]: one pane's conversation with one program, as a state machine
//!   that takes what the terminal learns and returns what the terminal does.
//! * [`bundle`]: manifests, verification of files and the [`bundle::Store`]
//!   the terminal supplies.
//! * [`sandbox`]: what a view declares, what the person is asked, and the
//!   content security policy built from the answer.
//! * [`theme`]: the terminal's look as MCP Apps styling.
//! * [`icon`]: the icons a program gives for itself and its tools, as a
//!   terminal may draw them.
//! * [`token`]: the single-use token and the source of bytes it is made from.
//! * [`base64`]: the two forms of base64 the wire uses.
//!
//! The specification is `spec/draft/pencere.md`; its fixtures are the tests of
//! this crate and of the TypeScript SDK.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod base64;
pub mod bundle;
pub mod frame;
pub mod icon;
pub mod json;
pub mod jsonrpc;
pub mod message;
pub mod sandbox;
pub mod session;
pub mod theme;
pub mod token;
