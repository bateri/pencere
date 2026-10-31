//! What the sandbox's tests of a page share: a measure that lays a page out, a page that is open
//! and what it has told so far, and the size the tests render at.

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use pencere_view::{Font, Measure, Metrics, Size};

use crate::{Module, Update};

/// Every line is as wide as it has characters: enough to lay a page out.
pub(crate) struct Rough;

impl Measure for Rough {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().count() as f32 * font.size * 0.6,
            height: font.size * 1.3,
        }
    }
}

pub(crate) struct Open {
    pub(crate) module: Module,
    pub(crate) woken: Receiver<()>,
    pub(crate) seen: Update,
}

impl Open {
    /// Takes what is waiting, waiting up to `within` for there to be something.
    pub(crate) fn take(&mut self, within: Duration) -> Update {
        let _ = self.woken.recv_timeout(within);
        let update = self.module.poll();
        if update.frame.is_some() {
            self.seen.frame = update.frame.clone();
        }
        self.seen.replies.extend(update.replies.iter().cloned());
        self.seen.sent.extend(update.sent.iter().cloned());
        self.seen.asks.extend(update.asks.iter().copied());
        self.seen.wake = update.wake;
        update
    }

    /// Takes until `done` says so of what came, or `within` has passed.
    pub(crate) fn until(&mut self, within: Duration, done: impl Fn(&Update) -> bool) -> Update {
        let began = Instant::now();
        loop {
            let left = within.saturating_sub(began.elapsed());
            let update = self.take(left.max(Duration::from_millis(1)));
            if done(&update) || left.is_zero() {
                return update;
            }
        }
    }
}

pub(crate) const SIZE: Size = Size {
    w: 1000.0,
    h: 700.0,
};
