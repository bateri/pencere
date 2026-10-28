//! How far the boxes of a page are scrolled: the host's, not the page's ([`pencere_view::Scrolls`]).
//! The wheel and the trackpad move the box under the pointer at once, on the page as it was last
//! laid out, the way a system moves its own views (the deltas carry the person's speed, direction
//! and momentum; the [`Phase`] says which part of a gesture each is); the bars are drawn as the
//! system's are ([`Scrolling`]) and answer the pointer before the page does; the page is told where
//! its boxes went and asks for one to go somewhere.

use pencere_view::{Bars, Input, Laid, Phase, Point, ScrollTo, Scrolls, Wheeled};

/// How a page scrolls on the host's system: how its bars are drawn (as the system's own scroll
/// bars are), whether a gesture pulls a box past its end, and what a click in a bar's track does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scrolling {
    pub bars: Bars,
    pub elastic: bool,
    /// A click in the track takes the box to that place (`false`: a page toward it).
    pub track_jumps: bool,
}

impl Default for Scrolling {
    fn default() -> Self {
        Self {
            bars: Bars::Overlay,
            elastic: false,
            track_jumps: false,
        }
    }
}

/// The host's scrolling of a page. The times are the drive's clock ([`crate::Page`]).
#[derive(Debug)]
pub(crate) struct HostScrolls {
    pub(crate) scrolls: Scrolls,
}

impl HostScrolls {
    pub(crate) fn new() -> Self {
        Self {
            scrolls: Scrolls::new(),
        }
    }

    pub(crate) fn set(&mut self, scrolling: Scrolling) {
        self.scrolls.set_bars(scrolling.bars);
        self.scrolls.set_elastic(scrolling.elastic);
        self.scrolls.set_track_jumps(scrolling.track_jumps);
    }

    /// The wheel over `at` (the page's points) on the page `laid`, at `now`: whether a box took it,
    /// and what of it is the page's.
    pub(crate) fn wheel(
        &mut self,
        laid: &Laid,
        at: Point,
        (dx, dy): (f32, f32),
        phase: Phase,
        now: f64,
    ) -> Wheeled {
        self.scrolls.wheel(laid, at, dx, dy, phase, now)
    }

    /// A pointer input (the page's points) offered to the bars first, at `now`: whether a bar took
    /// it (it is not the page's), and whether a frame is worth drawing for what the bars did.
    pub(crate) fn pointer(&mut self, input: &Input, now: f64) -> (bool, bool) {
        match *input {
            Input::Press { at, .. } => {
                let took = self.scrolls.press(at, now);
                (took, took)
            }
            Input::Move(at) => {
                let holding = self.scrolls.holding();
                let moved = self.scrolls.point(Some(at), now);
                (holding, moved)
            }
            Input::Release { .. } => {
                let took = self.scrolls.release(now);
                (took, took)
            }
            Input::Leave => {
                let holding = self.scrolls.holding();
                (holding, self.scrolls.point(None, now))
            }
            Input::Scroll { .. } | Input::Key { .. } | Input::KeyUp { .. } | Input::Mods(_) => {
                (false, false)
            }
        }
    }

    pub(crate) fn ask(&mut self, asks: impl IntoIterator<Item = ScrollTo>) {
        for ask in asks {
            self.scrolls.ask(ask);
        }
    }

    /// Whether anything of the scrolling is on its way at the frame of `now` (a box springing back
    /// or gliding, bars going): the host asks for the next frame.
    pub(crate) fn moving(&self, now: f64) -> bool {
        self.scrolls.moving(now)
    }

    /// When the bars start to go, if nothing else moves, after the frame of `now`.
    pub(crate) fn wake_at(&self, now: f64) -> Option<f64> {
        self.scrolls.wake_at(now)
    }
}
