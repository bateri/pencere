//! A caret's glide: where a caret goes from a place to another, it goes over a short time instead of
//! at once (a terminal cursor's `cursor_motion`, VS Code's `cursorSmoothCaretAnimation`), on the
//! frames' clock, in the content's own coordinates (so that scrolling does not make it glide).
//!
//! The editor says where each caret is at every frame ([`CaretSpots`]) and is told how far from there
//! to draw it. The motions are a terminal's, numbers and all:
//!
//! - `spring` is a spring with no overshoot (critical damping): a caret that is sent somewhere else
//!   in flight keeps its speed, so that a key held down moves it on as one motion; it is settled when
//!   it is within a fiftieth of a row of its place and slower than a fifth of a row a second, and
//!   put there at once after 0.7 seconds, a belt that keeps a spring that does not settle from asking
//!   for frames for ever (a softer spring has the longer for it);
//! - `ease` is a curve that is fast at first and slow at the end over a time that does not depend on
//!   how far it goes; a caret sent somewhere else in flight starts it again from where it is.
//!
//! A caret glides where it went to another place of the document, the editor has the same number of
//! them, and both places are in the room; anywhere else it is at its place at once: Reduce Motion, a
//! caret that is new or gone, a place out of the room (the room scrolls to it), a layout that moved
//! it with no change of its place (a size, a fold, a wrapping), the pointer selecting and an input
//! method composing.

use pencere_editor::CaretSpots;
use pencere_view::edit::CaretMotion;
use pencere_view::{Id, Point, Rect};

/// How far a frame's time may be from the one before it, seconds: a view that was not seen for a
/// while does not step a whole glide at once.
const DT_MAX: f32 = 0.1;

/// How long a spring is let to settle, seconds, at a terminal's stiffness.
const TIME_CEILING: f32 = 0.7;

/// How near its place a caret is settled, rows, and how slow, rows a second.
const POSITION: f32 = 0.02;
const SPEED: f32 = 0.2;

/// Two places closer than this, pixels, are one: the layout puts the same caret where it was.
const SAME: f32 = 0.01;

/// A caret as it is drawn.
#[derive(Clone, Copy, Debug)]
struct Gliding {
    /// The place of the document it was at, and the box it is going to (content pixels): none until
    /// it has been seen on a line that shows.
    place: usize,
    target: Option<Point>,
    /// Where it is drawn now, and how fast it is going (pixels a second).
    at: Point,
    speed: (f32, f32),
    /// Where an `ease` began, and when; how long a `spring` has gone on.
    from: Point,
    since: f64,
    elapsed: f32,
    going: bool,
    /// It began at this frame, from rest: the frame shows it where it was, as an `ease`'s does.
    fresh: bool,
}

/// The glide of the carets of the editor that has the keyboard.
#[derive(Debug, Default)]
pub(crate) struct Glide {
    editor: Option<Id>,
    carets: Vec<Gliding>,
    time: Option<f64>,
    /// The editor asked this frame where to draw its carets.
    asked: bool,
}

impl Glide {
    /// Whether a caret is on its way to its place: the page wants frames for it.
    pub(crate) fn moving(&self) -> bool {
        self.carets.iter().any(|caret| caret.going)
    }

    /// The frame is drawn: where no editor asked where to draw its carets (the keyboard went to the
    /// page or to a field, the editor is hidden), nothing is on its way, and no frame is asked for.
    pub(crate) fn drawn(&mut self) {
        if !std::mem::take(&mut self.asked) {
            self.editor = None;
            self.carets.clear();
        }
    }

    /// How far from its place each caret of `spots` is drawn at the frame at `now` (the box's place
    /// less where it is drawn: nothing where it is at its place), `still` for Reduce Motion.
    pub(crate) fn shifts(&mut self, spots: &CaretSpots, now: f64, still: bool) -> Vec<Point> {
        self.asked = true;
        let motion = spots.motion;
        let dt = self.time.map_or(0.0, |before| {
            #[allow(clippy::cast_possible_truncation)]
            let step = (now - before).clamp(0.0, f64::from(DT_MAX)) as f32;
            step
        });
        if self.time.is_none_or(|before| now > before) {
            self.time = Some(now);
        }
        let steady = self.editor != Some(spots.editor)
            || self.carets.len() != spots.places.len()
            || motion == CaretMotion::Snap
            || still
            || spots.interacting;
        if steady {
            self.editor = Some(spots.editor);
            self.carets = spots
                .places
                .iter()
                .zip(&spots.at)
                .map(|(place, at)| Gliding::at(*place, *at))
                .collect();
            return vec![Point::default(); spots.places.len()];
        }
        let mut shifts = Vec::with_capacity(self.carets.len());
        for ((caret, place), at) in self.carets.iter_mut().zip(&spots.places).zip(&spots.at) {
            caret.see(*place, *at, spots.room, now);
            caret.advance(motion, now, dt, spots.row);
            shifts.push(match caret.target {
                Some(target) => Point::new(caret.at.x - target.x, caret.at.y - target.y),
                None => Point::default(),
            });
        }
        shifts
    }
}

impl Gliding {
    /// A caret at rest at `at`, of the place `place`.
    fn at(place: usize, at: Option<Point>) -> Self {
        let here = at.unwrap_or_default();
        Self {
            place,
            target: at,
            at: here,
            speed: (0.0, 0.0),
            from: here,
            since: 0.0,
            elapsed: 0.0,
            going: false,
            fresh: false,
        }
    }

    /// Takes where the editor says the caret is now: it goes there, or is there at once.
    fn see(&mut self, place: usize, at: Option<Point>, room: Rect, now: f64) {
        let Some(target) = at else {
            *self = Self::at(place, None);
            return;
        };
        let Some(was) = self.target else {
            *self = Self::at(place, at);
            return;
        };
        if place != self.place {
            if inside(room, was) && inside(room, target) {
                // On its way from where it is drawn: a spring keeps the speed it has.
                self.fresh = !self.going;
                self.from = self.at;
                self.since = now;
                self.elapsed = 0.0;
                self.going = true;
            } else {
                *self = Self::at(place, at);
            }
        } else if (target.x - was.x).abs() > SAME || (target.y - was.y).abs() > SAME {
            // The same place, put elsewhere by the layout: a size, a fold, a wrapping.
            *self = Self::at(place, at);
        }
        self.place = place;
        self.target = Some(target);
    }

    /// Moves it on at the frame at `now`, `dt` seconds after the one before.
    fn advance(&mut self, motion: CaretMotion, now: f64, dt: f32, row: f32) {
        let Some(target) = self.target else {
            return;
        };
        if !self.going {
            return;
        }
        let dt = if std::mem::take(&mut self.fresh) {
            0.0
        } else {
            dt
        };
        let done = match motion {
            CaretMotion::Snap => true,
            CaretMotion::Ease(seconds) => {
                #[allow(clippy::cast_possible_truncation)]
                let time = (now - self.since) as f32 / seconds.max(f32::EPSILON);
                let time = time.clamp(0.0, 1.0);
                self.at = Point::new(
                    ease(self.from.x, target.x, time),
                    ease(self.from.y, target.y, time),
                );
                time >= 1.0
            }
            CaretMotion::Spring(stiffness) => {
                let stiffness = stiffness.max(f32::EPSILON);
                self.elapsed += dt;
                let (x, vx) = spring(self.at.x, self.speed.0, target.x, dt, stiffness);
                let (y, vy) = spring(self.at.y, self.speed.1, target.y, dt, stiffness);
                self.at = Point::new(x, y);
                self.speed = (vx, vy);
                let (near, slow) = (POSITION * row, SPEED * row);
                let settled = (x - target.x).abs() <= near
                    && (y - target.y).abs() <= near
                    && vx.abs() <= slow
                    && vy.abs() <= slow;
                // A softer spring is let the longer.
                settled || self.elapsed > TIME_CEILING * (CaretMotion::SPRING / stiffness).max(1.0)
            }
        };
        if done {
            self.at = target;
            self.from = target;
            self.speed = (0.0, 0.0);
            self.going = false;
        }
    }
}

/// Whether `point` (the corner of a caret's box) is in the part of the content the room shows.
fn inside(room: Rect, point: Point) -> bool {
    point.x >= room.x && point.x < room.right() && point.y >= room.y && point.y < room.bottom()
}

/// `from` to `to`, `time` (0 to 1) of the way on a cubic curve that is fast at first.
fn ease(from: f32, to: f32, time: f32) -> f32 {
    let eased = 1.0 - (1.0 - time).powi(3);
    from + (to - from) * eased
}

/// One step of a critically damped spring of `stiffness` on one axis, from `position` going `speed`
/// toward `target`, `dt` seconds: the closed form, which is right however long the step is. An axis
/// that would cross its target is stopped on it.
fn spring(position: f32, speed: f32, target: f32, dt: f32, stiffness: f32) -> (f32, f32) {
    let before = position - target;
    let c = speed + stiffness * before;
    let decay = (-stiffness * dt).exp();
    let after = (before + c * dt) * decay;
    let speed = (c - stiffness * (before + c * dt)) * decay;
    if before != 0.0 && (before < 0.0) != (after < 0.0) {
        (target, 0.0)
    } else {
        (target + after, speed)
    }
}
