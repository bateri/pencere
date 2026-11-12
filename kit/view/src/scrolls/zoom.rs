//! The zoom of a box that scrolls ([`crate::Element::zoom`]), kept by the host with how far the box
//! is scrolled: a pinch makes what the box holds larger or smaller about the point between the
//! fingers, so that the point of the content under them stays where it is; the wheel with the
//! command key held does the same by the turn, and a double tap with two fingers goes between the
//! size the page gave and the one it says (`smart`). A pinch past the range the page gave is
//! pulled as a scroll is past its end and springs back; a page asks for a scale
//! ([`ScrollTo::zoom`]) and the host goes there, over a moment where it is asked to.
//!
//! The scale is kept as a natural logarithm, so that a pinch's factors add and 0 is the size the
//! page gave. What the box holds is laid out at the scale ([`crate::Style::zoomed`], put in by
//! [`Scrolls::apply`]): the host lays the page out again for every step of a zoom
//! ([`Scrolls::zooming`]), as the scale is part of what is laid out and not a way of drawing it.

use super::{GLIDE, REST, SPRING, Scroller, Scrolls, rubber, unrubber};
use crate::{Ease, GesturePhase, Id, Laid, Point, Rect, ScrollTo, Zoom};

/// The pull of a zoom past its range counts in logs, which are small: the view of [`rubber`] is
/// one (a doubling is 0.69).
const VIEW: f32 = 1.0;

/// A scale pulled past its range, coming back: how far it was drawn past it, and since when.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ZoomSpring {
    from: f32,
    start: f64,
}

/// A scale going where the page asked, or where a double tap sent it, over time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ZoomGlide {
    from: f32,
    to: f32,
    start: f64,
}

/// What is kept of the zoom of one box.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Zooming {
    /// The scale within its range, as a natural logarithm.
    log: f32,
    /// How far the fingers pulled it past an end of the range, and how far past it the content is
    /// drawn for that ([`rubber`]), logs.
    pull: f32,
    stretch: f32,
    spring: Option<ZoomSpring>,
    glide: Option<ZoomGlide>,
    /// What the scale changed about: a point of the box, from its top-left corner, points.
    anchor: Point,
    /// The scale the page was last told, as a log.
    told: f32,
    /// From the last layout: what it may be zoomed to, and where the box is, points.
    rule: Option<Zoom>,
    frame: Rect,
    /// The scale changed since the page was laid out at it.
    dirty: bool,
}

impl Zooming {
    /// How many times the size the page gave the content is drawn at: its scale and what it is
    /// pulled past its range.
    pub(super) fn shown(&self) -> f32 {
        (self.log + self.stretch).exp()
    }

    /// The scale the page is told, which is not what a pull draws.
    pub(super) fn settled(&self) -> f32 {
        self.log.exp()
    }

    /// Whether the page was told the scale it is at.
    pub(super) fn told(&self) -> bool {
        self.told == self.log
    }

    pub(super) fn tell(&mut self) {
        self.told = self.log;
    }

    /// The page asked for a scale: it is told the one the box is at, as the host holds it, though it
    /// did not change (what is asked past the range, or of a box that does not zoom, is not what
    /// the page thinks it is).
    pub(super) fn untell(&mut self) {
        self.told = f32::NAN;
    }

    pub(super) fn moving(&self) -> bool {
        self.spring.is_some() || self.glide.is_some()
    }

    /// The page was laid out at the scale.
    pub(super) fn laid_out(&mut self) {
        self.dirty = false;
    }

    /// What the last layout says of the box: what it may be zoomed to, and where it is.
    pub(super) fn set_rule(&mut self, rule: Option<Zoom>, frame: Rect) {
        self.rule = rule;
        self.frame = frame;
        if rule.is_none() {
            if *self != Self::default() {
                // A box that says no zoom is at its size again, and the page is told so.
                let was = self.log != 0.0 || self.stretch != 0.0;
                *self = Self::default();
                if was {
                    self.untell();
                    self.dirty = true;
                }
            }
            return;
        }
        // A range that came or moved holds the scale: a box that may not be as small as it is
        // comes up to its least (the page is told, and laid out again at it).
        let log = self.within(self.log);
        if log != self.log {
            self.log = log;
            self.pull = 0.0;
            self.stretch = 0.0;
            self.spring = None;
            self.glide = None;
            self.dirty = true;
            self.untell();
        }
    }

    /// The range of the scale, as logs, where the box may be zoomed.
    fn range(&self) -> Option<(f32, f32)> {
        let rule = self.rule?;
        let low = rule.min.max(0.01).ln();
        let high = rule.max.max(rule.min).max(0.01).ln();
        Some((low, high))
    }

    /// What the page set the scale to where it is not the range's: within it.
    fn within(&self, log: f32) -> f32 {
        match self.range() {
            Some((low, high)) => log.clamp(low, high),
            None => 0.0,
        }
    }
}

/// Changes the zoom of the box by `change`, keeping the point of the content at `anchor` (from the
/// top-left corner of the box) where it is: the content is `ratio` times as far from the corner,
/// and the box is scrolled so that it is the same distance again.
fn rescale(b: &mut Scroller, anchor: Point, change: impl FnOnce(&mut Zooming)) {
    let before = b.zoom.shown();
    change(&mut b.zoom);
    let ratio = b.zoom.shown() / before;
    if ratio != 1.0 && ratio.is_finite() {
        b.offset = Point::new(
            (b.offset.x + anchor.x) * ratio - anchor.x,
            (b.offset.y + anchor.y) * ratio - anchor.y,
        );
        b.zoom.anchor = anchor;
        b.zoom.dirty = true;
    }
}

/// The box goes to the scale `to` (a log, within its range): over a glide, or at once.
fn go(b: &mut Scroller, anchor: Point, to: f32, animate: bool, now: f64) {
    b.zoom.spring = None;
    rescale(b, anchor, |z| {
        z.pull = 0.0;
        z.stretch = 0.0;
    });
    b.zoom.anchor = anchor;
    if animate && to != b.zoom.log {
        b.zoom.glide = Some(ZoomGlide {
            from: b.zoom.log,
            to,
            start: now,
        });
        b.zoom.dirty = true;
    } else {
        b.zoom.glide = None;
        rescale(b, anchor, |z| z.log = to);
    }
    b.moved_at = Some(now);
}

/// The fingers let the scale go: pulled past its range, it springs back (at once where motion is
/// reduced).
fn let_go(b: &mut Scroller, now: f64, reduce: bool) {
    b.zoom.pull = 0.0;
    if b.zoom.stretch == 0.0 {
        return;
    }
    if reduce {
        let anchor = b.zoom.anchor;
        rescale(b, anchor, |z| z.stretch = 0.0);
    } else {
        b.zoom.spring = Some(ZoomSpring {
            from: b.zoom.stretch,
            start: now,
        });
        b.zoom.dirty = true;
    }
    b.moved_at = Some(now);
}

/// Moves a scale that is going where it was asked, or coming back, on to `now`.
fn step(b: &mut Scroller, now: f64, reduce: bool) {
    let anchor = b.zoom.anchor;
    if let Some(glide) = b.zoom.glide {
        let t = if reduce {
            1.0
        } else {
            ((now - glide.start) / GLIDE) as f32
        };
        let log = if t >= 1.0 {
            b.zoom.glide = None;
            glide.to
        } else {
            let at = if t <= 0.0 { 0.0 } else { Ease::Out.at(t) };
            glide.from + (glide.to - glide.from) * at
        };
        rescale(b, anchor, |z| z.log = log);
        b.moved_at = Some(now);
    }
    if let Some(spring) = b.zoom.spring {
        let t = ((now - spring.start).max(0.0)) as f32;
        let left = (1.0 + SPRING * t) * (-SPRING * t).exp();
        let stretch = spring.from * left;
        let done = reduce || stretch.abs() < REST_LOG;
        rescale(b, anchor, |z| z.stretch = if done { 0.0 } else { stretch });
        if done {
            b.zoom.spring = None;
        }
        b.moved_at = Some(now);
    }
}

/// A pull of the scale smaller than this, as a log, is no pull (a quarter of a point of a box a
/// hundred points long, as [`REST`] is).
const REST_LOG: f32 = REST / 100.0;

impl Scrolls {
    /// The box under `at` (points of the page `laid`) that may be zoomed, the innermost.
    fn zoomable(&self, laid: &Laid, at: Point) -> Option<Id> {
        laid.ids_at(at)
            .into_iter()
            .find(|id| self.boxes.get(id).is_some_and(|b| b.zoom.rule.is_some()))
    }

    /// Where `at` is in the box `id`: from its top-left corner, points.
    fn anchor_in(&self, id: Id, at: Point) -> Point {
        let frame = self
            .boxes
            .get(&id)
            .map_or(Rect::default(), |b| b.zoom.frame);
        Point::new(at.x - frame.x, at.y - frame.y)
    }

    /// The fingers of a pinch over `at` (points of the page `laid`), the content to be `1 + by`
    /// times as large as at the gesture's last event: whether a box that may be zoomed took it
    /// (the page is told of it otherwise, as [`crate::Input::Magnify`]). A gesture stays with the
    /// box it began on, as a scroll does.
    pub fn magnify(
        &mut self,
        laid: &Laid,
        at: Point,
        by: f32,
        phase: GesturePhase,
        now: f64,
    ) -> bool {
        let reduce = self.reduce_motion;
        let elastic = self.elastic && !reduce;
        match phase {
            GesturePhase::Begin => {
                self.pinch = self.zoomable(laid, at);
                self.pinch_is_the_pages = self.pinch.is_none();
                let Some(id) = self.pinch else {
                    return false;
                };
                // A scale coming back is held where it is.
                if let Some(b) = self.boxes.get_mut(&id) {
                    b.zoom.glide = None;
                    if b.zoom.spring.take().is_some() {
                        b.zoom.pull = unrubber(b.zoom.stretch, VIEW);
                        b.moved_at = Some(now);
                    }
                }
                true
            }
            GesturePhase::Move => {
                // A pinch that began where no box zooms is the page's to its end, though a box
                // came under the fingers since.
                if self.pinch.is_none() && !self.pinch_is_the_pages {
                    self.pinch = self.zoomable(laid, at);
                }
                let Some(id) = self.pinch else {
                    return false;
                };
                let anchor = self.anchor_in(id, at);
                let Some(b) = self.boxes.get_mut(&id) else {
                    return false;
                };
                let Some((low, high)) = b.zoom.range() else {
                    return false;
                };
                b.zoom.glide = None;
                b.zoom.spring = None;
                let raw = b.zoom.log + b.zoom.pull + (1.0 + by).max(0.01).ln();
                let log = raw.clamp(low, high);
                let pull = if elastic { raw - log } else { 0.0 };
                rescale(b, anchor, |z| {
                    z.log = log;
                    z.pull = pull;
                    z.stretch = rubber(pull, VIEW);
                });
                b.moved_at = Some(now);
                true
            }
            GesturePhase::End => {
                let Some(id) = self.pinch.take() else {
                    self.pinch_is_the_pages = false;
                    return false;
                };
                if let Some(b) = self.boxes.get_mut(&id) {
                    let_go(b, now, reduce);
                }
                true
            }
        }
    }

    /// A double tap with two fingers over `at`: the innermost box there that may be zoomed goes
    /// to the size the page gave, or to the one it says (`smart`) where it is at the first.
    /// Whether one took it.
    pub fn smart_magnify(&mut self, laid: &Laid, at: Point, now: f64) -> bool {
        let Some(id) = self.zoomable(laid, at) else {
            return false;
        };
        let anchor = self.anchor_in(id, at);
        let animate = !self.reduce_motion;
        let Some(b) = self.boxes.get_mut(&id) else {
            return false;
        };
        let Some(rule) = b.zoom.rule else {
            return false;
        };
        let to = if b.zoom.log.abs() < 0.01 {
            b.zoom.within(rule.smart.max(0.01).ln())
        } else {
            b.zoom.within(0.0)
        };
        go(b, anchor, to, animate, now);
        true
    }

    /// The wheel over `at` with the command key held, by `dy` points (positive: the content moves
    /// up, which makes it smaller): whether a box that may be zoomed took it, and so no box
    /// scrolls. Not a gesture: no pull past the range.
    pub fn zoom_wheel(&mut self, laid: &Laid, at: Point, dy: f32, now: f64) -> bool {
        let Some(id) = self.zoomable(laid, at) else {
            return false;
        };
        let anchor = self.anchor_in(id, at);
        let Some(b) = self.boxes.get_mut(&id) else {
            return false;
        };
        let (Some(rule), Some((low, high))) = (b.zoom.rule, b.zoom.range()) else {
            return false;
        };
        if dy != 0.0 {
            let log = (b.zoom.log - dy * rule.wheel).clamp(low, high);
            b.zoom.glide = None;
            b.zoom.spring = None;
            rescale(b, anchor, |z| {
                z.log = log;
                z.pull = 0.0;
                z.stretch = 0.0;
            });
            b.moved_at = Some(now);
        }
        // A wheel that does not turn up or down (a touch of the fingers, the sideways of a
        // trackpad) zooms nothing, and is the box's to scroll or lift from.
        dy != 0.0
    }

    /// The scale of the box named `id`: how many times the size the page gave it the content is
    /// at (1 for a box that may not be zoomed).
    pub fn zoom_of(&self, id: Id) -> f32 {
        self.boxes.get(&id).map_or(1.0, |b| b.zoom.settled())
    }

    /// Does what the host owes the zoom before the page is laid out: the asks for a scale
    /// ([`ScrollTo::zoom`]) of the boxes it knows are done, and a scale going where it is going or
    /// coming back is moved on to `now`.
    pub fn advance_zoom(&mut self, now: f64) {
        let reduce = self.reduce_motion;
        let asks = std::mem::take(&mut self.asks);
        for ask in asks {
            let (Some(scale), Some(b)) = (ask.zoom, self.boxes.get_mut(&ask.id)) else {
                self.asks.push(ask);
                continue;
            };
            match b.zoom.rule {
                // A box that was laid out and says no zoom has none to go to: the page is told
                // the scale it is at (1).
                None if b.extent.is_some() => b.zoom.untell(),
                // A box that is not on the page yet waits for it.
                None => self.asks.push(ask),
                Some(_) => {
                    b.zoom.untell();
                    let anchor = ask.at.map_or_else(
                        || {
                            b.extent.map_or(Point::new(0.0, 0.0), |e| {
                                Point::new(e.view.w / 2.0, e.view.h / 2.0)
                            })
                        },
                        |at| Point::new(at.x - b.zoom.frame.x, at.y - b.zoom.frame.y),
                    );
                    let to = b.zoom.within(scale.max(0.01).ln());
                    go(b, anchor, to, ask.animate && !reduce, now);
                }
            }
        }
        for b in self.boxes.values_mut() {
            step(b, now, reduce);
        }
    }

    /// Whether an ask for a scale can be done at the next frame: the box is on the page and says a
    /// zoom. (What is asked of a box that is not yet on the page waits, and costs no frame.)
    pub(super) fn zoom_ask_is_due(&self, ask: &ScrollTo) -> bool {
        ask.zoom.is_some()
            && self
                .boxes
                .get(&ask.id)
                .is_some_and(|b| b.zoom.rule.is_some())
    }

    /// Whether the page is to be laid out again for the zoom: a scale changed, is going or is
    /// coming back, or a page asked for one of a box that can go to it.
    pub fn zooming(&self) -> bool {
        self.asks.iter().any(|ask| self.zoom_ask_is_due(ask))
            || self.boxes.values().any(|b| b.zoom.dirty || b.zoom.moving())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(id: u64, scale: f32) -> ScrollTo {
        ScrollTo {
            id: Id::new(id),
            x: None,
            y: None,
            animate: false,
            zoom: Some(scale),
            at: None,
        }
    }

    #[test]
    fn rescaling_keeps_the_point_under_the_anchor_where_it_is_for_any_offset_and_ratio() {
        for (offset, anchor, change) in [
            (Point::new(0.0, 0.0), Point::new(50.0, 60.0), 0.7f32),
            (Point::new(13.5, 77.25), Point::new(3.0, 99.0), 1.3),
            (Point::new(400.0, 1200.0), Point::new(199.0, 1.0), -0.4),
        ] {
            let mut b = Scroller {
                offset,
                ..Scroller::default()
            };
            // The content point under the anchor, in the content's own points.
            let before = b.zoom.shown();
            let under = Point::new(
                (b.offset.x + anchor.x) / before,
                (b.offset.y + anchor.y) / before,
            );
            rescale(&mut b, anchor, |z| z.log = change);
            let now = b.zoom.shown();
            assert!((now - change.exp()).abs() < 1e-5);
            // It is under the anchor again at the new scale.
            assert!((under.x * now - b.offset.x - anchor.x).abs() < 1e-3);
            assert!((under.y * now - b.offset.y - anchor.y).abs() < 1e-3);
        }
    }

    #[test]
    fn what_waits_for_a_box_that_is_not_on_the_page_is_not_kept_without_end() {
        let mut scrolls = Scrolls::new();
        for id in 0..1000 {
            scrolls.ask(ask(id, 2.0));
        }
        assert!(scrolls.asks.len() <= super::super::MOST_WAITING);
        // The newest are the ones kept.
        assert!(scrolls.asks.iter().any(|ask| ask.id == Id::new(999)));
        assert!(!scrolls.asks.iter().any(|ask| ask.id == Id::new(0)));
        // Waiting is no reason for a frame.
        assert!(!scrolls.zooming());
        assert!(!scrolls.moving(0.0));
    }

    #[test]
    fn the_scale_of_a_zoom_is_a_log_the_range_clamps() {
        let zooming = Zooming {
            rule: Some(Zoom {
                min: 0.5,
                max: 4.0,
                ..Zoom::default()
            }),
            ..Zooming::default()
        };
        assert!((zooming.within(10.0) - 4.0f32.ln()).abs() < 1e-6);
        assert!((zooming.within(-10.0) - 0.5f32.ln()).abs() < 1e-6);
        assert_eq!(zooming.within(0.0), 0.0);
        // No rule is no scale: the size the page gave.
        assert_eq!(Zooming::default().within(1.0), 0.0);
    }
}
