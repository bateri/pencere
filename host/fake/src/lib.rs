//! The stand-in terminal's parts that are not its command line: the pty a program runs on, how a
//! terminal reads what the program writes (`pane`), the store of a bundle's files, the record — and
//! a program on a pty whose view is run for real, under the drive's host with no screen
//! (`program`), so that a program and its page are tested together as a terminal shows them.

#![deny(unsafe_code)]

pub mod log;
pub mod pane;
pub mod program;
pub mod pty;
mod screen;
pub mod store;
