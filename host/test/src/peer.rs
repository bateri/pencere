//! The program of a page under test, of whichever kind: a table of calls the test answers, the
//! app's own program in this process, or a program on a pty.

use std::time::Duration;

use pencere_drive::headless::{Calls, InProcess, Peer};
use pencere_fake_host::program::Program;

/// The three programs a test's page may speak to, as one.
pub enum Any {
    /// Stand-in handlers the test wrote, and calls it answers itself.
    Calls(Calls),
    /// The app's own program, in this process.
    InProcess(Box<InProcess>),
    /// A program on a pty, of any language.
    Program(Box<Program>),
}

impl Any {
    /// Whether the program keeps its own time: what the page says of it is waited for.
    pub fn waits(&self) -> bool {
        !matches!(self, Self::Calls(_))
    }

    pub fn calls(&mut self) -> Option<&mut Calls> {
        match self {
            Self::Calls(calls) => Some(calls),
            _ => None,
        }
    }

    pub fn in_process(&mut self) -> Option<&mut InProcess> {
        match self {
            Self::InProcess(program) => Some(program),
            _ => None,
        }
    }
}

impl Peer for Any {
    fn opened(&mut self) {
        match self {
            Self::Calls(peer) => peer.opened(),
            Self::InProcess(peer) => peer.opened(),
            Self::Program(peer) => peer.opened(),
        }
    }

    fn heard(&mut self, message: &str) {
        match self {
            Self::Calls(peer) => peer.heard(message),
            Self::InProcess(peer) => peer.heard(message),
            Self::Program(peer) => peer.heard(message),
        }
    }

    fn said(&mut self) -> Vec<String> {
        match self {
            Self::Calls(peer) => peer.said(),
            Self::InProcess(peer) => peer.said(),
            Self::Program(peer) => peer.said(),
        }
    }

    fn busy(&mut self) -> bool {
        match self {
            Self::Calls(peer) => peer.busy(),
            Self::InProcess(peer) => peer.busy(),
            Self::Program(peer) => peer.busy(),
        }
    }

    fn wait(&mut self, within: Duration) {
        match self {
            Self::Calls(peer) => peer.wait(within),
            Self::InProcess(peer) => peer.wait(within),
            Self::Program(peer) => peer.wait(within),
        }
    }

    fn closed(&mut self) -> Option<String> {
        match self {
            Self::Calls(peer) => peer.closed(),
            Self::InProcess(peer) => peer.closed(),
            Self::Program(peer) => peer.closed(),
        }
    }
}
