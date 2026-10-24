//! The wire of pencere that the two ends of it share: what a program writes into its terminal
//! and what a terminal writes back, as bytes and as the types they are read into. A terminal
//! reads it with the host crate (`pencere-host`), a program writes it with the program library
//! (`pencere`), and a page module in the terminal's sandbox speaks its six view messages; none
//! of them has a second copy.
//!
//! The crate does no I/O and knows nothing of a session, of where files are kept or of what is
//! drawn. Every byte it reads comes from a stream anyone can write to: it never panics on it,
//! allocates for what it has not seen, or buffers more than a limit of the specification
//! allows.
//!
//! * [`frame`]: the APC strings of the wire, read ([`frame::BodyDecoder`] for bodies a
//!   terminal's parser hands over, [`frame::StreamDecoder`] for raw bytes) and written
//!   ([`frame::encode`], which refuses a byte a frame may not carry).
//! * [`message`]: the payloads of the handshake and of closing, as the host reads and writes
//!   them; [`json`] writes JSON as printable ASCII.
//! * [`bundle`]: manifests, digests and the verification of the files that arrive. Where a
//!   terminal keeps them is its own.
//! * [`messages`]: the six messages of a program and its view, that a terminal only carries.
//! * [`base64`]: the two forms of base64 the wire uses.
//!
//! The specification is `spec/draft/pencere.md`; its fixtures are the tests of this crate and
//! of the TypeScript SDK.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod base64;
pub mod bundle;
pub mod frame;
pub mod json;
pub mod message;
pub mod messages;
