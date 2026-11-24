//! The files a page names as sounds: found by their names through [`Assets`], read and decoded on
//! a thread of its own, kept within what a view may hold, pitched to what is asked, and handed back
//! when they are ready.
//!
//! A page does not wait for a sound: [`Sounds::request`] answers at once with the sound if it is
//! decoded already, and with a ticket if it is not, and the sound arrives with that ticket through
//! [`Sounds::poll`] when the thread has it. One that has not by the time the page may wait
//! ([`SoundLimits::wait`]) is dropped and the author told, and what was decoded in the meantime
//! stays for the next ask.

use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use super::decode::decode;
use super::{Diagnostic, Failure, Sound, SoundLimits};
use crate::images::Assets;

/// The answer to a request for a sound.
#[derive(Clone, Debug)]
pub enum Ask {
    /// The sound is decoded, at the pitch asked for.
    Ready(Arc<Sound>),
    /// The sound is being made ready: it arrives with this ticket ([`Sounds::poll`]).
    Pending(u64),
    /// The sound is not there to be had: the author is told ([`Arrival::diagnostics`]), once.
    Refused,
}

/// What came of the requests since the host last looked ([`Sounds::poll`]).
#[derive(Clone, Debug, Default)]
pub struct Arrival {
    /// The sounds that were pending and are ready, each with the ticket it was given.
    pub ready: Vec<(u64, Arc<Sound>)>,
    /// The tickets of the requests that are dropped: the sound is no sound, or it was too late.
    pub failed: Vec<u64>,
    /// The sounds that are refused or late, each told once.
    pub diagnostics: Vec<Diagnostic>,
}

impl Arrival {
    /// Nothing came.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ready.is_empty() && self.failed.is_empty() && self.diagnostics.is_empty()
    }
}

/// A sound's place among what the view holds.
enum Entry {
    /// Decoded, and as it is in the file (the pitch is made for each request).
    Ready { sound: Arc<Sound>, used: u64 },
    /// No sound: the file is not one, and is not read again.
    Refused,
}

struct Pending {
    ticket: u64,
    path: String,
    asked: Instant,
}

#[derive(Default)]
struct State {
    /// The paths named, as many as the view may name.
    named: HashSet<String>,
    entries: HashMap<String, Entry>,
    /// Bytes of the samples held in `entries`.
    bytes: usize,
    /// What orders the use of the entries.
    clock: u64,
    next: u64,
    pending: Vec<Pending>,
    arrived: Arrival,
    /// The paths a diagnostic was written for: each once.
    told: HashSet<String>,
}

struct Shared {
    assets: Arc<dyn Assets>,
    limits: SoundLimits,
    wake: Box<dyn Fn() + Send + Sync>,
    state: Mutex<State>,
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        // A thread that panicked with the lock held left what was behind it as it was: nothing in
        // it is half-written.
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

struct Job {
    ticket: u64,
    path: String,
    pitch: f32,
}

/// The sounds of one view, from files.
pub struct Sounds {
    shared: Arc<Shared>,
    worker: OnceLock<Option<Sender<Job>>>,
}

impl Sounds {
    /// The sounds of a view whose files `assets` has. `wake` is called, from the thread that
    /// decodes, when something is there for [`Sounds::poll`] to give.
    pub fn new(
        assets: Arc<dyn Assets>,
        limits: SoundLimits,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                assets,
                limits,
                wake: Box::new(wake),
                state: Mutex::new(State::default()),
            }),
            worker: OnceLock::new(),
        }
    }

    /// The sound of the file at `path`, `pitch` times as fast (from 0.5 to 2; 1 is as it is).
    pub fn request(&self, path: &str, pitch: f32) -> Ask {
        let pitch = if pitch.is_finite() && pitch > 0.0 {
            pitch
        } else {
            1.0
        };
        let ticket = {
            let mut state = self.shared.state();
            if !state.named.contains(path) {
                if state.named.len() >= self.shared.limits.paths {
                    let failure = Failure::Paths {
                        most: self.shared.limits.paths,
                    };
                    tell(&mut state, path, &failure);
                    return Ask::Refused;
                }
                state.named.insert(path.to_owned());
            }
            state.clock += 1;
            let clock = state.clock;
            match state.entries.get_mut(path) {
                Some(Entry::Refused) => return Ask::Refused,
                Some(Entry::Ready { sound, used }) if (pitch - 1.0).abs() < f32::EPSILON => {
                    *used = clock;
                    return Ask::Ready(Arc::clone(sound));
                }
                _ => {}
            }
            state.next += 1;
            let ticket = state.next;
            state.pending.push(Pending {
                ticket,
                path: path.to_owned(),
                asked: Instant::now(),
            });
            ticket
        };
        let job = Job {
            ticket,
            path: path.to_owned(),
            pitch,
        };
        if self.post(job) {
            Ask::Pending(ticket)
        } else {
            let mut state = self.shared.state();
            state.pending.retain(|pending| pending.ticket != ticket);
            tell(
                &mut state,
                path,
                &Failure::Broken("no thread to decode it on".into()),
            );
            Ask::Refused
        }
    }

    fn post(&self, job: Job) -> bool {
        let worker = self.worker.get_or_init(|| {
            let (jobs, inbox) = channel();
            let shared = Arc::clone(&self.shared);
            let started = std::thread::Builder::new()
                .name("view-sounds".into())
                .spawn(move || serve(&shared, &inbox));
            started.is_ok().then_some(jobs)
        });
        worker.as_ref().is_some_and(|jobs| jobs.send(job).is_ok())
    }

    /// What came since the last look: the sounds that are ready, the requests that are dropped, and
    /// what to tell the author. A request that has waited past [`SoundLimits::wait`] is dropped
    /// here.
    pub fn poll(&self) -> Arrival {
        let mut state = self.shared.state();
        let wait = self.shared.limits.wait;
        let now = Instant::now();
        let (late, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut state.pending)
            .into_iter()
            .partition(|pending| now.duration_since(pending.asked) > wait);
        state.pending = waiting;
        for pending in late {
            state.arrived.failed.push(pending.ticket);
            let failure = Failure::Late {
                millis: wait.as_millis(),
            };
            tell(&mut state, &pending.path, &failure);
        }
        std::mem::take(&mut state.arrived)
    }

    /// When the oldest request that is waiting is dropped, if any is: when to look again if
    /// nothing wakes the host first.
    pub fn deadline(&self) -> Option<Instant> {
        let state = self.shared.state();
        state
            .pending
            .iter()
            .map(|pending| pending.asked + self.shared.limits.wait)
            .min()
    }

    /// Bytes of samples the view holds of its files' sounds.
    pub fn held(&self) -> usize {
        self.shared.state().bytes
    }
}

/// Writes what is wrong with the file at `path`, if it was not written already.
fn tell(state: &mut State, path: &str, failure: &Failure) {
    if state.told.insert(path.to_owned()) {
        state.arrived.diagnostics.push(Diagnostic {
            path: path.to_owned(),
            message: failure.to_string(),
        });
    }
}

fn serve(shared: &Shared, inbox: &Receiver<Job>) {
    while let Ok(job) = inbox.recv() {
        // A decoder that gives up by panicking costs the sound and not the thread.
        let made = catch_unwind(AssertUnwindSafe(|| prepare(shared, &job.path, job.pitch)))
            .unwrap_or_else(|_| Err(Failure::Broken("the decoder gave up".into())));
        {
            let mut state = shared.state();
            if let Some(at) = state
                .pending
                .iter()
                .position(|pending| pending.ticket == job.ticket)
            {
                state.pending.remove(at);
                match made {
                    Ok(sound) => state.arrived.ready.push((job.ticket, sound)),
                    Err(failure) => {
                        state.arrived.failed.push(job.ticket);
                        tell(&mut state, &job.path, &failure);
                    }
                }
            }
            // A request that was dropped for its time has its sound kept, but is not answered.
        }
        (shared.wake)();
    }
}

/// The sound of the file at `path`, decoded if it is not, at the pitch asked.
fn prepare(shared: &Shared, path: &str, pitch: f32) -> Result<Arc<Sound>, Failure> {
    let held = {
        let mut state = shared.state();
        state.clock += 1;
        let clock = state.clock;
        match state.entries.get_mut(path) {
            Some(Entry::Ready { sound, used }) => {
                *used = clock;
                Some(Ok(Arc::clone(sound)))
            }
            Some(Entry::Refused) => Some(Err(Failure::Broken("it was refused before".into()))),
            None => None,
        }
    };
    let sound = match held {
        Some(held) => held?,
        None => {
            let read = read(shared, path);
            let mut state = shared.state();
            match read {
                Ok(sound) => store(&mut state, shared, path, sound),
                Err(failure) => {
                    // A file that is not there may be later (a handle the person gives); what is
                    // there and is no sound is not read again.
                    if failure != Failure::Missing {
                        state.entries.insert(path.to_owned(), Entry::Refused);
                    }
                    return Err(failure);
                }
            }
        }
    };
    if (pitch - 1.0).abs() < f32::EPSILON {
        return Ok(sound);
    }
    // What it comes to at that pitch is held to what a view may hold before it is made.
    let bytes = (sound.bytes() as f64 / f64::from(pitch)) as usize;
    if bytes > shared.limits.decoded_bytes {
        return Err(Failure::Whole {
            bytes,
            most: shared.limits.decoded_bytes,
        });
    }
    Ok(Arc::new(sound.pitched(pitch)))
}

/// The file at `path`, decoded.
fn read(shared: &Shared, path: &str) -> Result<Sound, Failure> {
    let bytes = shared.assets.read(path).ok_or(Failure::Missing)?;
    if bytes.len() > shared.limits.file_bytes {
        return Err(Failure::Source {
            bytes: bytes.len(),
            most: shared.limits.file_bytes,
        });
    }
    decode(&bytes, shared.limits.decoded_bytes)
}

/// Keeps `sound` among what the view holds, letting go of what has been used least to make room.
/// A sound that is in use by the host stays with the host: it is only no longer kept here.
fn store(state: &mut State, shared: &Shared, path: &str, sound: Sound) -> Arc<Sound> {
    let sound = Arc::new(sound);
    let most = shared.limits.decoded_bytes;
    while state.bytes + sound.bytes() > most {
        let oldest = state
            .entries
            .iter()
            .filter_map(|(key, entry)| match entry {
                Entry::Ready { used, .. } => Some((*used, key.clone())),
                Entry::Refused => None,
            })
            .min();
        let Some((_, key)) = oldest else { break };
        if let Some(Entry::Ready { sound: gone, .. }) = state.entries.remove(&key) {
            state.bytes = state.bytes.saturating_sub(gone.bytes());
        }
    }
    state.clock += 1;
    state.bytes += sound.bytes();
    state.entries.insert(
        path.to_owned(),
        Entry::Ready {
            sound: Arc::clone(&sound),
            used: state.clock,
        },
    );
    sound
}
