//! The proof of the host's own route to the program's machine (specification §4.10): the host asks
//! the program for a listener on its machine's loopback that gives back a token, and connects to it
//! by the route it is trying, to see the route end where the program is.
//!
//! The session writes the `probe` (with a token of the terminal's randomness, as a handle is) and
//! reads the `probed` that answers it; the connecting is the terminal's, by its own route, and the
//! session does none of that I/O ([`Action::ProbeConnect`]). A probe is out only while a view runs
//! and ends with the view, whatever ended it.

use super::files::hex;
use super::{Action, Phase, Session};
use crate::bundle::Store;
use crate::frame::{FileParams, Message};
use crate::message;
use crate::token::Entropy;

/// The most seconds a listener may be asked to stay up (§4.10).
pub const MOST_TTL: u8 = 30;

/// A `probe` that is out: the id of the message, for the `re` of the answer, and the token it
/// carried, which the connection must give back.
pub(super) struct Out {
    id: u32,
    token: String,
}

impl<S: Store, E: Entropy> Session<S, E> {
    /// Sets whether the host proves its own route with a `probe` (§4.10), which it does not unless
    /// told: one that does lists `probe` in the `features` of its `welcome`s from now on.
    pub fn offer_probe(&mut self, offer: bool) {
        self.probe = offer;
    }

    /// Asks the program for a listener on its machine's loopback that stays up for `ttl` seconds
    /// (1 to [`MOST_TTL`]) and gives a token of the session's own to one connection (§4.10), and
    /// returns the id of the `probe`. The answer comes as [`Action::ProbeConnect`] or
    /// [`Action::ProbeFailed`], with that id. Nothing is written, and `None` returned, when the host
    /// does not offer the probe ([`Session::offer_probe`]), when no view runs, or when `ttl` is not
    /// one the listener is asked for. A probe that is out when this is called is given up: the answer
    /// to it, if one comes, is ignored.
    ///
    /// §4.10 has a host send one `probe` at a time, and that is the host's to keep: it asks again
    /// only once the probe that is out is answered, or once it has waited for the answer as long as it
    /// waits (the session has no clock). The program may still be answering the one given up then,
    /// and one with a listener up says `busy` to the next.
    pub fn probe(&mut self, ttl: u8) -> Option<u32> {
        if !self.probe || self.ended || !(1..=MOST_TTL).contains(&ttl) {
            return None;
        }
        if !matches!(self.phase, Phase::Live(_)) {
            return None;
        }
        let token = hex(&self.entropy.token_bytes());
        let id = self.queue_id(
            "probe",
            None,
            FileParams::NONE,
            &message::probe(&token, ttl),
            false,
        )?;
        if let Phase::Live(live) = &mut self.phase {
            live.probe = Some(Out { id, token });
        }
        Some(id)
    }

    /// The program's `probed`: the answer to the `probe` that is out. The first one whose `re` is
    /// its id is believed; any other is ignored, as one that names neither a port from 1 to 65535
    /// nor a code (a `probe` stays out until it is answered, or its view ends).
    pub(super) fn on_probed(&mut self, message: &Message) {
        let answer = crate::json::from_object::<message::Probed>(&message.payload)
            .and_then(message::Probed::answer);
        let re = message.re();
        let taken = match (&mut self.phase, answer) {
            (Phase::Live(live), Some(answer))
                if re.is_some() && live.probe.as_ref().map(|out| out.id) == re =>
            {
                live.probe.take().map(|out| (out, answer))
            }
            _ => None,
        };
        match taken {
            Some((out, Ok(port))) => self.actions.push(Action::ProbeConnect {
                probe: out.id,
                port,
                token: out.token,
            }),
            Some((out, Err(code))) => self.actions.push(Action::ProbeFailed {
                probe: out.id,
                code,
            }),
            None => self.ignored(),
        }
    }
}
