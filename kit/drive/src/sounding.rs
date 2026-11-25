//! What a page asks of the sounds of its view, made ready for the host: the kit's own sounds
//! made (at the pitch asked for, with what makes the plays of a sound played often differ), a
//! file of the page's bundle decoded on a thread of its own, and the ones that are not decoded
//! yet kept until they are, or dropped when the page has waited for them too long.
//!
//! The page is never waited for: a sound that is not decoded is not in the reply that asked for
//! it ([`crate::Host::sound`]); it is given when it is ready ([`crate::Drained::sounds`]), after
//! the replies, unless the page ended the view's sounds in the meantime.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use pencere_sandbox::Assets;
use pencere_sandbox::sounds::{self, Diagnostic, Failure, SoundLimits, Sounds};
use pencere_view::sound::{Ask, Play, Source};

/// The sounds of one page.
pub(crate) struct Desk {
    /// Where the files are decoded and kept; none where the page has no bundle.
    files: Option<Sounds>,
    /// The sounds asked for that are being decoded, each with the ticket the decoder gave it, in
    /// the order they were asked for.
    waiting: Vec<(u64, Play)>,
    /// The sounds that were decoded since the host last looked, in the order they were asked for.
    late: Vec<Ask>,
    /// The number of the next play of a kit's sound: what each differs from the others by.
    draws: u64,
    /// The files that were told to be no file, where there is no bundle to ask: each once.
    told: HashSet<String>,
}

impl Desk {
    pub(crate) fn new(
        assets: Option<Arc<dyn Assets>>,
        limits: SoundLimits,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            files: assets.map(|assets| Sounds::new(assets, limits, wake)),
            waiting: Vec::new(),
            late: Vec::new(),
            draws: 0,
            told: HashSet::new(),
        }
    }

    /// What a reply asked of the sounds, made ready, in order: the sounds ready now and the ends
    /// of them; a sound that is being decoded waits, and the one that is no sound is told to the
    /// author (`told`, now or when the decoder says so). An end of every sound ends the ones
    /// waiting as well, and the ones decoded since the host last looked.
    pub(crate) fn resolve(&mut self, asks: Vec<Ask>, told: &mut Vec<String>) -> Vec<Ask> {
        let mut ready = Vec::with_capacity(asks.len());
        for ask in asks {
            let Ask::Play(mut play) = ask else {
                let tickets: Vec<u64> = self.waiting.drain(..).map(|(ticket, _)| ticket).collect();
                if let Some(files) = &self.files {
                    files.cancel(&tickets);
                }
                self.late.clear();
                ready.push(Ask::Stop);
                continue;
            };
            match play.source.clone() {
                Source::Kit(name) => {
                    let draw = self.draws;
                    self.draws = self.draws.wrapping_add(1);
                    play.samples = sounds::play_of(&name, play.pitch, draw);
                    if play.samples.is_some() {
                        ready.push(Ask::Play(play));
                    }
                }
                Source::File(path) => {
                    match self.files.as_ref().map(|f| f.request(&path, play.pitch)) {
                        Some(sounds::Ask::Ready(sound)) => {
                            play.samples = Some(sound);
                            ready.push(Ask::Play(play));
                        }
                        Some(sounds::Ask::Pending(ticket)) => self.waiting.push((ticket, play)),
                        Some(sounds::Ask::Refused) => {}
                        // A page with no bundle has no file: told once for each, as a bundle's are.
                        None => {
                            if self.told.insert(path.clone()) {
                                told.push(
                                    Diagnostic {
                                        path,
                                        message: Failure::Missing.to_string(),
                                    }
                                    .to_string(),
                                );
                            }
                        }
                    }
                }
            }
        }
        ready
    }

    /// Takes what the decoder has done since the last look: the sounds that are ready join the ones
    /// to give the host, the ones dropped (not a sound, or not decoded in time) are forgotten, and
    /// what the author is told goes to `told`.
    pub(crate) fn take(&mut self, told: &mut Vec<String>) {
        let Some(files) = &self.files else {
            return;
        };
        let arrival = files.poll();
        for (ticket, sound) in arrival.ready {
            if let Some(at) = self.waiting.iter().position(|(held, _)| *held == ticket) {
                let (_, mut play) = self.waiting.remove(at);
                play.samples = Some(sound);
                self.late.push(Ask::Play(play));
            }
        }
        for ticket in arrival.failed {
            self.waiting.retain(|(held, _)| *held != ticket);
        }
        told.extend(arrival.diagnostics.iter().map(ToString::to_string));
    }

    /// The sounds decoded since the host last looked, taken.
    pub(crate) fn late(&mut self) -> Vec<Ask> {
        std::mem::take(&mut self.late)
    }

    /// Whether a sound the page asked for is being decoded: what a host that settles a page waits for.
    pub(crate) fn waiting(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// When the oldest sound that is being decoded is given up on, if one is.
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.files.as_ref().and_then(Sounds::deadline)
    }
}
