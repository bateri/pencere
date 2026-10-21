//! The host side of pencere: what a terminal needs to let a program in one
//! of its panes show an MCP App view in that pane.
//!
//! The crate does no I/O. A terminal feeds it the bodies of the APC sequences
//! it parses from a pane's output, in whatever pieces its parser hands them
//! over, and writes the replies the crate builds into that pane's input. The
//! view itself, its sandbox and the bundle store belong to the terminal; the
//! crate holds the rules they follow, so every terminal that embeds it follows
//! the same ones.
