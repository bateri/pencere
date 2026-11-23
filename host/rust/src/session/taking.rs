//! What the host hands a program that is running for it (specification §10.9): an address, a file,
//! a folder, for a program whose `open` said it takes that kind. The session writes the `take`; it
//! has nothing to wait for, since the program does not answer.

use super::{Phase, Session};
use crate::bundle::Store;
use crate::frame::FileParams;
use crate::take::{Kind, Take};
use crate::token::Entropy;

impl<S: Store, E: Entropy> Session<S, E> {
    /// What the program of the view that runs said it takes while it runs (`takes` of its `open`,
    /// §10.9): each kind once, in the order it named them. Empty while no view runs, and for a
    /// program that said nothing. A program that opens a view says it anew in each `open`: after a
    /// restart (§10.7) the answer is empty until the program has opened again.
    #[must_use]
    pub fn takes(&self) -> &[Kind] {
        match &self.phase {
            Phase::Live(live) => &live.takes,
            Phase::Idle | Phase::Opening(_) => &[],
        }
    }

    /// Hands the program `take` (§10.9): writes a `take` and returns its id. Nothing is written, and
    /// `None` returned, when no view runs, when the program did not say it takes that kind of
    /// thing ([`Session::takes`]), or when `take` is not something the wire carries
    /// ([`Take::is_valid`]): a program is not sent what it did not ask for. The program does not
    /// answer, and the host does not know what became of it.
    pub fn hand_over(&mut self, take: &Take) -> Option<u32> {
        if self.ended || !self.takes().contains(&take.kind()) {
            return None;
        }
        let payload = take.payload()?;
        self.queue_id("take", None, FileParams::NONE, &payload, false)
    }
}
