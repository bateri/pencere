//! `ViewControl`: how the program that runs `serve` closes the view it shows, or shows another in
//! its place, without ending (specification §10.8).
//!
//! `serve` returns when the view has closed, which is what a program that is only a view wants. A
//! program that goes on after its view — a tool that shows a text interface for a while, or one
//! whose author reloads the view as the files change — makes a control, gives it to `serve` and
//! calls it from another thread while `serve` runs:
//!
//! ```no_run
//! use pencere::{Bundle, Options, View, ViewControl};
//!
//! # fn main() -> Result<(), pencere::Error> {
//! let control = ViewControl::new();
//! let page = Bundle::from_dir("view", "index.html")?;
//! let reloader = control.clone();
//! std::thread::spawn(move || {
//!     // ... later, when the files changed:
//!     if let Ok(page) = Bundle::from_dir("view", "index.html") {
//!         let _ = reloader.reopen(Some(vec![View::web(page)]));
//!     }
//! });
//! pencere::serve(Options::new().view(View::web(page)).control(control))?;
//! # Ok(())
//! # }
//! ```
//!
//! Both calls need a terminal that lists `dismiss` in its `welcome`; with another the view is not
//! touched and the answer says so.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};

use crate::handlers::{Outbox, Work};
use crate::{Error, NoView, Options, View};

/// What `ViewControl::reopen` came to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReopenResult {
    /// The old view is closed and the terminal has taken the open of the new one.
    Reopened,
    /// The terminal does not list `dismiss`. Nothing was sent and the view is as it was.
    Unsupported,
    /// There is no view to close: `serve` has not shown one yet, or has ended, or the view is
    /// already being closed or replaced; or someone else closed it (the person, the view) while
    /// this was under way. Nothing was sent, or what was sent no longer matters.
    Unavailable,
    /// The old view closed and no new one could be opened: the terminal did not answer in time
    /// ([`crate::NoViewReason::Timeout`]), refused or ended the open before the view ran. `serve`
    /// ends the same way and runs its fallback, as it does when the first open fails.
    NoView(NoView),
}

/// What `ViewControl::close` came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CloseResult {
    /// The view is closed; `serve` returns [`crate::EndedBy::Program`], with the terminal given back
    /// first, so that the program can show its own interface.
    Closed,
    /// The terminal does not list `dismiss`. Nothing was sent and the view is as it was.
    Unsupported,
    /// As for [`ReopenResult::Unavailable`]: there is no view to close.
    Unavailable,
}

/// What the loop is asked.
pub(crate) enum Request {
    Reopen {
        views: Option<Vec<View>>,
        reply: SyncSender<ReopenResult>,
    },
    Close {
        reply: SyncSender<CloseResult>,
    },
}

/// The program's hand on the view `serve` shows. Give it to [`Options::control`]; until `serve`
/// has a view, and once it has ended, its calls answer `Unavailable`. It can be cloned and sent to
/// the threads that call it.
#[derive(Clone, Default)]
pub struct ViewControl {
    bound: Arc<Mutex<Option<(u64, Outbox)>>>,
}

/// The numbers the loops a control is tied to are told apart by.
static GENERATIONS: AtomicU64 = AtomicU64::new(1);

impl std::fmt::Debug for ViewControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewControl").finish_non_exhaustive()
    }
}

/// The control is tied to the loop of a `serve` while this lives.
pub(crate) struct Bound {
    bound: Arc<Mutex<Option<(u64, Outbox)>>>,
    generation: u64,
}

impl Drop for Bound {
    fn drop(&mut self) {
        // Only its own: a later `serve` may have taken the control while this one wound up.
        if let Ok(mut slot) = self.bound.lock()
            && slot
                .as_ref()
                .is_some_and(|(held, _)| *held == self.generation)
        {
            *slot = None;
        }
    }
}

impl ViewControl {
    /// A control that is tied to no `serve` yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `serve` ties the control to the loop it runs. A control belongs to one `serve` at a time;
    /// one that takes it later is the one that has it.
    pub(crate) fn bind(&self, outbox: Outbox) -> Bound {
        let generation = GENERATIONS.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut slot) = self.bound.lock() {
            *slot = Some((generation, outbox));
        }
        Bound {
            bound: Arc::clone(&self.bound),
            generation,
        }
    }

    fn outbox(&self) -> Option<Outbox> {
        self.bound
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(|(_, outbox)| outbox.clone()))
    }

    /// Closes the view and opens one again: with the same files (the terminal keeps what it
    /// verified, so what did not change does not cross the stream a second time) when `next` is
    /// `None`, or with the kinds of view `next` lists, which are what the new `hello` offers from
    /// now on. Waits as long as the terminal takes to tear the old view down, which the terminal
    /// bounds and [`Options::reopen_timeout`] bounds from this side. What the old view had asked
    /// of the program is over with it, as for any view.
    ///
    /// # Errors
    ///
    /// When `next` is not what a program may offer ([`Error::Options`], [`Error::Bundle`]): the
    /// view is as it was.
    pub fn reopen(&self, next: Option<Vec<View>>) -> Result<ReopenResult, Error> {
        if let Some(views) = &next {
            Options::check_views(views)?;
        }
        let (reply, answer) = sync_channel(1);
        let Some(outbox) = self.outbox() else {
            return Ok(ReopenResult::Unavailable);
        };
        if !outbox.send(Work::Control(Request::Reopen { views: next, reply })) {
            return Ok(ReopenResult::Unavailable);
        }
        // A loop that ends before it answers drops the request, and with it the way to answer.
        Ok(answer.recv().unwrap_or(ReopenResult::Unavailable))
    }

    /// Closes the view and ends `serve` with [`crate::EndedBy::Program`], giving the terminal back
    /// first, so that the program can show its own interface.
    pub fn close(&self) -> CloseResult {
        let (reply, answer) = sync_channel(1);
        let Some(outbox) = self.outbox() else {
            return CloseResult::Unavailable;
        };
        if !outbox.send(Work::Control(Request::Close { reply })) {
            return CloseResult::Unavailable;
        }
        answer.recv().unwrap_or(CloseResult::Unavailable)
    }
}
