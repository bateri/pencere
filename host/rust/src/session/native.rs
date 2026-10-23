//! A view that is a WebAssembly component or a Lua script (specification §8.8): the terminal
//! runs it in its own sandbox and draws it itself, and the host between the view and the
//! program does nothing but route.
//!
//! There is no MCP session, no document, no content security policy and no question to the
//! person: the view reaches nothing the terminal does not hand it. Its `msg` messages are
//! the program's and the component's own business, JSON objects the host does not read — it
//! carries them from the program to the terminal ([`Action::ToView`]) and from the terminal to
//! the program ([`Session::module_message`]), and only while the view is open.

use super::{
    Action, Cause, Found, Live, MAX_FAILURE_CHARS, MAX_VIEW_MESSAGE, Phase, Session, ViewSource,
    ViewStart, ViewState, mcp,
};
use crate::bundle::Store;
use crate::frame::Message;
use crate::sandbox::{Grant, UiMeta};
use crate::token::Entropy;

impl<S: Store, E: Entropy> Session<S, E> {
    /// The files of a view the terminal draws itself are here (or none were missing): the view
    /// starts.
    pub(super) fn begin_native(&mut self) {
        let Some(open) = self.take_opening() else {
            return;
        };
        let super::Opening {
            kind,
            tool,
            bundle,
            program,
            title,
            announces,
            ..
        } = *open;
        let start = ViewStart {
            kind,
            source: ViewSource::Bundle,
            meta: UiMeta::default(),
            grant: Grant::none(),
            policy: String::new(),
            allow: String::new(),
            tool: String::new(),
            tool_label: None,
            title: title.clone(),
            program: program.clone(),
            tool_icons: Vec::new(),
            program_icons: Vec::new(),
        };
        self.phase = Phase::Live(Box::new(Live {
            kind,
            tool,
            found: Found {
                tool: serde_json::value::RawValue::NULL.to_owned(),
                uri: String::new(),
                title,
                program,
                tool_icons: Vec::new(),
                program_icons: Vec::new(),
            },
            tools: Vec::new(),
            bundle,
            grant: Grant::none(),
            // It never initializes: the host sends it nothing of MCP Apps.
            state: ViewState::AwaitInitialize,
            pane_aware: false,
            display_mode: "inline",
            open_call: None,
            forwards: Vec::new(),
            links: Vec::new(),
            announces,
            refresh: None,
        }));
        self.actions.push(Action::StartView(Box::new(start)));
    }

    /// A `msg` of the program while a view the terminal draws itself is up, or being opened: it
    /// is for the view and goes to it as it is. Returns whether the message was this kind's to
    /// handle.
    pub(super) fn native_msg(&mut self, message: &Message) -> bool {
        let up = match &self.phase {
            Phase::Live(live) if live.kind.is_native() => true,
            Phase::Opening(open) if open.kind.is_native() => false,
            _ => return false,
        };
        // Whoever can write the stream can send this, so it is held to being an object (the
        // component reads nothing else) and to the size one message may have.
        if up
            && message.payload.len() <= MAX_VIEW_MESSAGE
            && crate::json::is_object(&message.payload)
        {
            self.actions.push(Action::ToView(message.payload.clone()));
        } else {
            self.ignored();
        }
        true
    }

    /// A message of a view the terminal draws itself, as JSON text: it is written to the
    /// program as a `msg`, **only while the view is open**. After the view has closed, the
    /// program has ended, or when the message is not an object or is over the size one message
    /// may have, nothing is written (and the diagnostics count it): a message that came late
    /// would otherwise reach whatever reads the tty next as typed input (§4.8).
    ///
    /// Returns whether it was written.
    pub fn module_message(&mut self, text: &str) -> bool {
        let open = matches!(&self.phase, Phase::Live(live) if live.kind.is_native());
        if !open || text.len() > MAX_VIEW_MESSAGE || !crate::json::is_object(text) {
            self.ignored();
            return false;
        }
        self.send_json(text)
    }

    /// A view the terminal draws itself ended on its own failure: the component trapped, went
    /// over a limit the terminal holds it to, handed out something that is not a page, or a
    /// script made an error. The view is closed as for any other end, and the program is told
    /// `closed` with `reason` `error`, `code` `view` and, in `message`, `why` (§8.8): the
    /// words the view failed with, which a script's error has the line in. They are the view's
    /// own, so they are cut and cleaned of what a terminal would act on before they are passed
    /// on.
    pub fn view_failed(&mut self, why: &str) {
        // Only a view that is there has words to carry: a failure told late (the view is gone)
        // must not leave them for the next, unrelated close.
        if matches!(self.phase, Phase::Opening(_) | Phase::Live(_)) {
            self.failure = mcp::words(why, MAX_FAILURE_CHARS);
        }
        self.close(Cause::Failed("view"), None);
    }
}
