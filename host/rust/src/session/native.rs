//! A view that is a WebAssembly component or a Lua script (specification §9.8): the terminal
//! runs it in its own sandbox and draws it itself, and the host between the view and the
//! program does nothing but route.
//!
//! There is no document, no content security policy and no question to the person: the view
//! reaches nothing the terminal does not hand it. Its `msg` messages are the program's and the
//! component's own business, JSON objects whose members the host does not read — it carries
//! them from the program to the terminal ([`Action::ToView`](super::Action::ToView)) and from
//! the terminal to the program ([`Session::module_message`]), and only while the view is open.
//! That each is one complete JSON object is read with the same reader a page's messages are.

use super::{MAX_VIEW_MESSAGE, Phase, Session, page};
use crate::bundle::Store;
use crate::token::Entropy;

impl<S: Store, E: Entropy> Session<S, E> {
    /// A message of a view the terminal draws itself, as JSON text: it is written to the
    /// program as a `msg`, **only while the view is open**. After the view has closed, the
    /// program has ended, or when the message is not a JSON object, nothing is written (and the
    /// diagnostics count it): a message that came late would otherwise reach whatever reads the
    /// tty next as typed input (§4.8). A message over the size one message may have, as the
    /// view said it, ends the view as a failure (§9.3), since nothing could answer it; nothing is
    /// written then either.
    ///
    /// Returns whether it was written.
    pub fn module_message(&mut self, text: &str) -> bool {
        let open = matches!(&self.phase, Phase::Live(live) if live.kind.is_native());
        if !open {
            self.ignored();
            return false;
        }
        if text.len() > MAX_VIEW_MESSAGE {
            self.view_said_too_much();
            return false;
        }
        if !page::is_object(text) {
            self.ignored();
            return false;
        }
        self.send_json(text)
    }
}
