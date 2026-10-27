//! How far each box of a page is scrolled, kept by the **host** and not by the page: the wheel and
//! the trackpad move a box at once, on the page as it was last laid out, and the page is told
//! where its boxes stand ([`Scrolls::take_moved`]) to build what has come into view. A page that
//! wants a box elsewhere (the row the keyboard went to, the end of a log that grows) asks for it
//! ([`ScrollTo`]); the host does it before it next lays the page out.
//!
//! The way a box moves is the host's system's: the deltas it reads already have the person's
//! speed, direction and momentum in them, and the host says which part of a gesture each is
//! ([`Phase`]), whether a box is pulled past its end and springs back ([`Scrolls::set_elastic`]),
//! how its scroll bars are drawn ([`Bars`]) and what a click in a bar's track does
//! ([`Scrolls::set_track_jumps`]).
//!
//! A host keeps one `Scrolls` for a page, and for every frame:
//!
//! 1. [`Scrolls::apply`] to the tree the page handed out, so that it is laid out where its boxes
//!    are scrolled;
//! 2. lays it out ([`crate::layout`] or [`crate::Motions::layout`]);
//! 3. [`Scrolls::place`] on the result: the page's asks are done, a box that can no longer go as
//!    far as it was is brought back, what is moving is moved, and the bars are put in.
//!
//! When only a box moved (the wheel, a bar dragged, a box springing back), the host does step 3
//! again on the page it already laid out: a box is moved without the page being laid out again
//! ([`crate::Laid`] moves what is inside it), and nothing is asked of the page.
//!
//! A box is scrolled by the host only if it has a name ([`crate::Element::id`]): its offset is
//! kept by that name.

use std::collections::HashMap;

use crate::element::Element;
use crate::layout::{Laid, snap};
use crate::paint::Quad;
use crate::style::Overflow;
use crate::{Ease, Extent, Id, Point, Rect, Role};

/// What part of a gesture a turn of the wheel is, as the system says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    /// A wheel that turns in steps, or a device that says no more than the delta: the box under
    /// the pointer moves, and one at its end passes the turn to the box around it.
    #[default]
    Wheel,
    /// Fingers are on the trackpad and have not moved yet. A box springing back is caught where
    /// it is.
    Begin,
    /// The fingers move: the box the gesture started on moves with them, and past its end is
    /// pulled ([`Scrolls::set_elastic`]).
    Move,
    /// The fingers left: a box pulled past its end springs back.
    Lift,
    /// The momentum the system gives the motion after the fingers left.
    Coast,
    /// The momentum ran out.
    Stop,
}

/// How a host draws the bars of a box that scrolls: as its system does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Bars {
    /// Thin, over the content, while the box moves and a moment after, or while the pointer is
    /// over them (macOS with a trackpad: "Show scroll bars: automatically / when scrolling").
    #[default]
    Overlay,
    /// Always there, over the content, in a track ("Show scroll bars: always", or a mouse).
    Always,
    /// None.
    Hidden,
}

/// A page's ask to have a box scrolled: to `x` across and `y` down (points; one not given stays
/// where it is; past the end is the end), at once or `animate`d.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollTo {
    pub id: Id,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub animate: bool,
}

/// How far a box is pulled past its end against how far the fingers went past it: less and less,
/// never more than the box itself (the curve of a scroll view of macOS).
const RUBBER: f32 = 0.55;
/// How fast a box pulled past its end comes back, per second: it is there in about a third of a
/// second, without going past.
const SPRING: f32 = 13.0;
/// A pull smaller than this, points, is no pull.
const REST: f32 = 0.25;
/// How long an animated ask takes, seconds.
const GLIDE: f64 = 0.22;
/// How long overlay bars stay after the box stopped moving, and how long they take to go, seconds.
const SHOW: f64 = 0.9;
const FADE: f64 = 0.3;
/// The bars, points: how thick, how thick under the pointer, how far from the box's edge, the
/// least length of the knob, and how near the edge the pointer is over a bar.
const THICK: f32 = 6.0;
const THICK_OVER: f32 = 9.0;
const INSET: f32 = 2.0;
const KNOB_MIN: f32 = 20.0;
const ZONE: f32 = 12.0;

/// A box pulled past its end, coming back: how far it was, and since when.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Spring {
    from: Point,
    start: f64,
}

/// A box going where the page asked, over time.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Glide {
    from: Point,
    to: Point,
    start: f64,
}

/// What is kept of one box.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Scroller {
    /// How far it is scrolled, within its reach, points.
    offset: Point,
    /// How far the fingers went past an end (negative: before the start), points.
    pull: Point,
    /// How far the content is drawn past the end for that ([`RUBBER`]), points.
    stretch: Point,
    spring: Option<Spring>,
    glide: Option<Glide>,
    /// From the last layout: how far it reaches, and what part of the page it shows (points).
    extent: Option<Extent>,
    rect: Rect,
    /// When it last moved: the overlay bars show for a while after.
    moved_at: Option<f64>,
    /// The offset the page was last told.
    told: Point,
}

/// A bar's knob held and moved: the box, which way, where the pointer took it and the offset then.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Held {
    id: Id,
    across: bool,
    from: Point,
    offset: f32,
}

/// What the host keeps of how a page's boxes are scrolled.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scrolls {
    boxes: HashMap<Id, Scroller>,
    asks: Vec<ScrollTo>,
    /// The box the gesture under way moves, and whether its momentum was spent against an end.
    latch: Option<(Id, bool)>,
    bars: Bars,
    elastic: bool,
    track_jumps: bool,
    reduce_motion: bool,
    /// The bar the pointer is over (the box, and whether it is the one across).
    over: Option<(Id, bool)>,
    held: Option<Held>,
    /// A bar took the press the button is held for: its release is the bar's too.
    pressed: bool,
}

impl Scrolls {
    /// Nothing scrolled, overlay bars, no pulling past the ends (the host turns it on where its
    /// system does it), a click in the track a page.
    pub fn new() -> Self {
        Self::default()
    }

    /// How the bars are drawn.
    pub fn set_bars(&mut self, bars: Bars) {
        self.bars = bars;
    }

    /// Whether a gesture pulls a box past its end, to spring back when the fingers leave.
    pub fn set_elastic(&mut self, elastic: bool) {
        self.elastic = elastic;
    }

    /// What a click in a bar's track does: `true` takes the box to that place, `false` a page
    /// toward it (macOS: "Click in the scroll bar to").
    pub fn set_track_jumps(&mut self, jumps: bool) {
        self.track_jumps = jumps;
    }

    /// How far the box named `id` is scrolled, points.
    pub fn offset(&self, id: Id) -> Point {
        self.boxes
            .get(&id)
            .map_or(Point::new(0.0, 0.0), |b| b.offset)
    }

    /// The page asks for a box to be scrolled: done at the next [`Scrolls::place`].
    pub fn ask(&mut self, ask: ScrollTo) {
        self.asks.push(ask);
    }

    /// Puts each box of `root` that scrolls and has a name where it is scrolled, before the tree is
    /// laid out.
    pub fn apply(&self, root: &mut Element) {
        if let (Some(id), Overflow::Scroll) = (root.meta.id, root.style.overflow)
            && let Some(b) = self.boxes.get(&id)
        {
            root.style.scroll = (b.offset.x + b.stretch.x, b.offset.y + b.stretch.y);
        }
        for child in &mut root.children {
            self.apply(child);
        }
    }

    /// The boxes whose offset the page has not been told, with where they are now: what the host
    /// tells the page ([`crate::Event::Scrolled`]).
    pub fn take_moved(&mut self) -> Vec<(Id, Point)> {
        let mut moved: Vec<(Id, Point)> = self
            .boxes
            .iter_mut()
            .filter(|(_, b)| b.told != b.offset)
            .map(|(id, b)| {
                b.told = b.offset;
                (*id, b.offset)
            })
            .collect();
        moved.sort_by_key(|(id, _)| id.number());
        moved
    }

    // --- The wheel -------------------------------------------------------------------------

    /// The wheel turned, or the fingers moved on the trackpad, over `at` (points of the page
    /// `laid`) by `dx` and `dy` (positive: the content moves left and up). Returns whether a box
    /// took it: one that did not is the page's to have ([`crate::Input::Scroll`]). After a box took
    /// it the host draws again, by [`Scrolls::place`] on the same page.
    pub fn wheel(
        &mut self,
        laid: &Laid,
        at: Point,
        dx: f32,
        dy: f32,
        phase: Phase,
        now: f64,
    ) -> bool {
        match phase {
            Phase::Wheel => {
                self.latch = None;
                self.chain(laid, at, dx, dy, now)
            }
            Phase::Begin => {
                self.latch = None;
                self.catch(laid, at, now);
                // The touch moves nothing: there is nothing for the page.
                true
            }
            Phase::Move | Phase::Coast => {
                if dx == 0.0 && dy == 0.0 {
                    return self.latch.is_some();
                }
                if self.latch.is_none() {
                    self.latch = self.choose(laid, at, dx, dy).map(|id| (id, false));
                }
                let Some((id, spent)) = self.latch else {
                    return false;
                };
                if !spent {
                    let spent = self.pull_by(id, dx, dy, phase == Phase::Coast, now);
                    self.latch = Some((id, spent));
                }
                true
            }
            Phase::Lift | Phase::Stop => {
                let latched = self.latch.map(|(id, _)| id);
                if phase == Phase::Stop {
                    self.latch = None;
                }
                if let Some(b) = latched.and_then(|id| self.boxes.get_mut(&id)) {
                    let reduce = self.reduce_motion;
                    let_go(b, now, reduce);
                }
                latched.is_some()
            }
        }
    }

    /// A wheel that turns in steps: for each way, the innermost box under the pointer that has
    /// room that way moves; one at its end passes the turn to the box around it.
    fn chain(&mut self, laid: &Laid, at: Point, dx: f32, dy: f32, now: f64) -> bool {
        let under = laid.ids_at(at);
        let mut moved = false;
        for (delta, across) in [(dx, true), (dy, false)] {
            if delta == 0.0 {
                continue;
            }
            for id in &under {
                let Some(extent) = laid.scroll_extent(*id) else {
                    continue;
                };
                let b = self.boxes.entry(*id).or_default();
                let reach = reach_of(extent, across);
                let was = along(b.offset, across);
                let to = (was + delta).clamp(0.0, reach);
                if to != was {
                    set_along(&mut b.offset, across, to);
                    b.glide = None;
                    b.moved_at = Some(now);
                    moved = true;
                    break;
                }
            }
        }
        moved
    }

    /// The box a gesture moves: under the pointer, the innermost that can go the way the fingers
    /// mostly go; where none can (each is at that end), the innermost that scrolls that way at all,
    /// to be pulled past its end.
    fn choose(&self, laid: &Laid, at: Point, dx: f32, dy: f32) -> Option<Id> {
        let across = dx.abs() > dy.abs();
        let delta = if across { dx } else { dy };
        let under: Vec<(Id, Extent)> = laid
            .ids_at(at)
            .into_iter()
            .filter_map(|id| Some((id, laid.scroll_extent(id)?)))
            .filter(|(_, extent)| reach_of(*extent, across) > 0.0)
            .collect();
        let can_go = |(id, extent): &(Id, Extent)| {
            let offset = along(self.offset(*id), across);
            if delta > 0.0 {
                offset < reach_of(*extent, across)
            } else {
                offset > 0.0
            }
        };
        under
            .iter()
            .find(|candidate| can_go(candidate))
            .or_else(|| under.first())
            .map(|(id, _)| *id)
    }

    /// Moves the box `id` by the fingers' `dx` and `dy`: within its reach it scrolls, past an end
    /// it is pulled. Momentum (`coasting`) that runs into an end bounces it once and is spent:
    /// returns whether it was.
    fn pull_by(&mut self, id: Id, dx: f32, dy: f32, coasting: bool, now: f64) -> bool {
        let elastic = self.elastic && !self.reduce_motion;
        let Some(b) = self.boxes.get_mut(&id) else {
            return false;
        };
        let Some(extent) = b.extent else {
            return false;
        };
        b.glide = None;
        if !coasting {
            // The fingers hold the box: one coming back is held where it is.
            b.spring = None;
        }
        let mut spent = false;
        for (delta, across) in [(dx, true), (dy, false)] {
            let reach = reach_of(extent, across);
            if delta == 0.0 || reach <= 0.0 {
                continue;
            }
            let raw = along(b.offset, across) + along(b.pull, across) + delta;
            let offset = raw.clamp(0.0, reach);
            let pull = if elastic { raw - offset } else { 0.0 };
            set_along(&mut b.offset, across, offset);
            set_along(&mut b.pull, across, pull);
            set_along(
                &mut b.stretch,
                across,
                rubber(pull, view_of(extent, across)),
            );
            if coasting && pull != 0.0 {
                spent = true;
            }
        }
        b.moved_at = Some(now);
        if spent {
            let reduce = self.reduce_motion;
            let_go(b, now, reduce);
        }
        spent
    }

    /// Fingers came down over `at`: a box under them that is springing back is held there.
    fn catch(&mut self, laid: &Laid, at: Point, now: f64) {
        for id in laid.ids_at(at) {
            if let Some(b) = self.boxes.get_mut(&id)
                && b.spring.is_some()
                && let Some(extent) = b.extent
            {
                b.spring = None;
                b.pull = Point::new(
                    unrubber(b.stretch.x, extent.view.w),
                    unrubber(b.stretch.y, extent.view.h),
                );
                b.moved_at = Some(now);
                return;
            }
        }
    }

    // --- The bars --------------------------------------------------------------------------

    /// The pointer is at `at` (`None`: it left the page). While a knob is held it moves the box;
    /// otherwise it says which bar the pointer is over. Returns whether a frame is worth drawing.
    pub fn point(&mut self, at: Option<Point>, now: f64) -> bool {
        if let (Some(held), Some(at)) = (self.held, at) {
            let Some(b) = self.boxes.get_mut(&held.id) else {
                return false;
            };
            let Some(extent) = b.extent else {
                return false;
            };
            let (track, knob) = knob_of(extent, b.rect, b.stretch, held.across);
            let room = (track - knob).max(1.0);
            let moved = along(at, held.across) - along(held.from, held.across);
            let reach = reach_of(extent, held.across);
            let to = (held.offset + moved * reach / room).clamp(0.0, reach);
            let was = along(b.offset, held.across);
            set_along(&mut b.offset, held.across, to);
            b.moved_at = Some(now);
            return to != was;
        }
        let over = at.and_then(|at| self.bar_at(at));
        let changed = over != self.over;
        self.over = over;
        changed && self.bars != Bars::Hidden
    }

    /// A button went down at `at`. On a bar's knob it is held (and moves the box as the pointer
    /// does); in its track the box goes a page toward it, or to that place and is held there
    /// ([`Scrolls::set_track_jumps`]). Returns whether a bar took the press: one that did not is
    /// the page's.
    pub fn press(&mut self, at: Point, now: f64) -> bool {
        if self.bars == Bars::Hidden {
            return false;
        }
        let Some((id, across)) = self.bar_at(at) else {
            return false;
        };
        let (jumps, reduce) = (self.track_jumps, self.reduce_motion);
        let Some(b) = self.boxes.get_mut(&id) else {
            return false;
        };
        let Some(extent) = b.extent else {
            return false;
        };
        let (track, knob) = knob_of(extent, b.rect, b.stretch, across);
        let reach = reach_of(extent, across);
        let room = (track - knob).max(1.0);
        let start = along(Point::new(b.rect.x, b.rect.y), across) + INSET;
        let offset = along(b.offset, across);
        let knob_at = start + room * (offset / reach.max(1.0)).clamp(0.0, 1.0);
        let pointer = along(at, across);
        let on_knob = pointer >= knob_at && pointer <= knob_at + knob;
        self.pressed = true;
        if !on_knob && !jumps {
            let page = view_of(extent, across) * 0.9;
            let toward = if pointer < knob_at { -page } else { page };
            let mut target = b.offset;
            set_along(&mut target, across, (offset + toward).clamp(0.0, reach));
            go(b, target, !reduce, now);
            return true;
        }
        if !on_knob {
            // To the place clicked, the knob's middle under the pointer, and held from there.
            let mut target = b.offset;
            set_along(
                &mut target,
                across,
                ((pointer - start - knob / 2.0) / room * reach).clamp(0.0, reach),
            );
            go(b, target, false, now);
        }
        b.glide = None;
        b.moved_at = Some(now);
        self.held = Some(Held {
            id,
            across,
            from: at,
            offset: along(b.offset, across),
        });
        true
    }

    /// The button came up: a knob held is let go. Returns whether a bar took the press it ends
    /// (the release is the bar's then, not the page's).
    pub fn release(&mut self, now: f64) -> bool {
        if let Some(held) = self.held.take()
            && let Some(b) = self.boxes.get_mut(&held.id)
        {
            b.moved_at = Some(now);
        }
        std::mem::take(&mut self.pressed)
    }

    /// Whether a bar's knob is held: the pointer's moves and the release are the bar's.
    pub fn holding(&self) -> bool {
        self.held.is_some()
    }

    /// The bar under `at`: the innermost box that scrolls that way and whose edge `at` is near.
    fn bar_at(&self, at: Point) -> Option<(Id, bool)> {
        let mut found: Option<(Id, bool, f32)> = None;
        for (id, b) in &self.boxes {
            let Some(extent) = b.extent else {
                continue;
            };
            let r = b.rect;
            if !r.contains(at.x, at.y) {
                continue;
            }
            let area = r.w * r.h;
            let near_right = extent.max_offset() > 0.0 && at.x >= r.right() - ZONE;
            let near_bottom = extent.max_offset_x() > 0.0 && at.y >= r.bottom() - ZONE;
            let across = if near_right {
                false
            } else if near_bottom {
                true
            } else {
                continue;
            };
            if found.is_none_or(|(_, _, least)| area < least) {
                found = Some((*id, across, area));
            }
        }
        found.map(|(id, across, _)| (id, across))
    }

    // --- Each frame ------------------------------------------------------------------------

    /// After the page was laid out (or again on the page last laid out, when only a box moved):
    /// the page's asks are done, a box that can no longer go as far as it was comes back within
    /// its reach, what is springing back or going where it was asked is moved on to `now`, every
    /// box is moved where it is, and its bars are put on the page.
    pub fn place(&mut self, laid: &mut Laid, now: f64, reduce_motion: bool) {
        self.reduce_motion = reduce_motion;
        let scale = laid.scale;
        let mut asks = std::mem::take(&mut self.asks);
        let mut placed = Vec::new();
        for index in laid.scrollers() {
            let Some((id, extent, _)) = laid.scroller_at(index) else {
                continue;
            };
            let b = self.boxes.entry(id).or_default();
            b.extent = Some(extent);
            for ask in asks.iter().filter(|ask| ask.id == id) {
                let target = Point::new(
                    ask.x
                        .unwrap_or(b.offset.x)
                        .clamp(0.0, extent.max_offset_x()),
                    ask.y.unwrap_or(b.offset.y).clamp(0.0, extent.max_offset()),
                );
                go(b, target, ask.animate && !reduce_motion, now);
            }
            asks.retain(|ask| ask.id != id);
            step(b, now, reduce_motion);
            b.offset = Point::new(
                b.offset.x.clamp(0.0, extent.max_offset_x()),
                b.offset.y.clamp(0.0, extent.max_offset()),
            );
            let to = (
                snap(b.offset.x + b.stretch.x, scale),
                snap(b.offset.y + b.stretch.y, scale),
            );
            laid.rescroll(index, to);
            placed.push((index, id));
        }
        // An ask for a box that is not on the page yet waits there: it is where the box will be.
        for ask in asks {
            let b = self.boxes.entry(ask.id).or_default();
            b.offset = Point::new(
                ask.x.unwrap_or(b.offset.x).max(0.0),
                ask.y.unwrap_or(b.offset.y).max(0.0),
            );
        }
        // The bars, once every box is where it goes (one inside another moves with it).
        let mut bars = Vec::new();
        for (index, id) in placed {
            let Some((_, extent, rect)) = laid.scroller_at(index) else {
                continue;
            };
            let Some(clip) = laid.shown_at(index) else {
                continue;
            };
            let over = self
                .over
                .filter(|(over, _)| *over == id)
                .map(|(_, across)| across);
            let held = self
                .held
                .filter(|held| held.id == id)
                .map(|held| held.across);
            let Some(b) = self.boxes.get_mut(&id) else {
                continue;
            };
            b.rect = rect;
            let b = *b;
            let quads = self.bars_of(&b, extent, rect, clip, scale, over, held, now);
            if !quads.is_empty() {
                bars.push((index, quads));
            }
        }
        laid.bars = bars;
    }

    /// How much of a box's bars show at `now`: all while it moves, the pointer is over them or
    /// one is held; the overlay ones go a while after.
    fn level(&self, b: &Scroller, active: bool, now: f64) -> f32 {
        match self.bars {
            Bars::Hidden => 0.0,
            Bars::Always => 1.0,
            Bars::Overlay => {
                if active || b.spring.is_some() || b.glide.is_some() {
                    return 1.0;
                }
                let Some(moved) = b.moved_at else {
                    return 0.0;
                };
                let since = now - moved;
                if since < SHOW {
                    1.0
                } else if since < SHOW + FADE {
                    (1.0 - (since - SHOW) / FADE) as f32
                } else {
                    0.0
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn bars_of(
        &self,
        b: &Scroller,
        extent: Extent,
        rect: Rect,
        clip: Rect,
        scale: f32,
        over: Option<bool>,
        held: Option<bool>,
        now: f64,
    ) -> Vec<Quad> {
        let mut quads = Vec::new();
        let level = self.level(b, over.is_some() || held.is_some(), now);
        if level <= 0.0 {
            return quads;
        }
        let px = |r: Rect| Rect::new(r.x * scale, r.y * scale, r.w * scale, r.h * scale);
        for across in [false, true] {
            if reach_of(extent, across) <= 0.0 {
                continue;
            }
            let wide = over == Some(across) || held == Some(across) || self.bars == Bars::Always;
            let thick = if wide { THICK_OVER } else { THICK };
            let (track, knob) = knob_of(extent, rect, b.stretch, across);
            let reach = reach_of(extent, across);
            let share = (along(b.offset, across) / reach).clamp(0.0, 1.0);
            let knob_at = INSET + (track - knob) * share;
            let (track_rect, knob_rect) = if across {
                let y = rect.bottom() - INSET - thick;
                (
                    Rect::new(rect.x + INSET, y, track, thick),
                    Rect::new(rect.x + knob_at, y, knob, thick),
                )
            } else {
                let x = rect.right() - INSET - thick;
                (
                    Rect::new(x, rect.y + INSET, thick, track),
                    Rect::new(x, rect.y + knob_at, thick, knob),
                )
            };
            let round = |r: Rect| [r.w.min(r.h) * scale / 2.0; 4];
            if wide {
                quads.push(bar_quad(
                    px(track_rect),
                    round(track_rect),
                    0.07 * level,
                    clip,
                ));
            }
            let ink = if held == Some(across) { 0.6 } else { 0.42 };
            quads.push(bar_quad(px(knob_rect), round(knob_rect), ink * level, clip));
        }
        quads
    }

    /// Whether anything is on its way (a box springing back, going where it was asked, bars
    /// going): the host draws the next frame too, by [`Scrolls::place`] on the same page.
    pub fn moving(&self, now: f64) -> bool {
        self.boxes.values().any(|b| {
            b.spring.is_some()
                || b.glide.is_some()
                || (self.bars == Bars::Overlay
                    && b.moved_at
                        .is_some_and(|at| now - at >= SHOW && now - at < SHOW + FADE))
        })
    }

    /// When a box's bars start to go, if they are showing and nothing else moves: the host draws
    /// a frame then, and is [`Scrolls::moving`] until they are gone.
    pub fn wake_at(&self, now: f64) -> Option<f64> {
        if self.bars != Bars::Overlay {
            return None;
        }
        self.boxes
            .values()
            .filter_map(|b| b.moved_at.map(|at| at + SHOW))
            .filter(|at| *at > now)
            .min_by(f64::total_cmp)
    }
}

/// The box goes to `target`: over a glide, or at once.
fn go(b: &mut Scroller, target: Point, animate: bool, now: f64) {
    b.spring = None;
    b.pull = Point::new(0.0, 0.0);
    b.stretch = Point::new(0.0, 0.0);
    if animate && target != b.offset {
        b.glide = Some(Glide {
            from: b.offset,
            to: target,
            start: now,
        });
    } else {
        b.glide = None;
        b.offset = target;
    }
    b.moved_at = Some(now);
}

/// The fingers let a box go: pulled past an end, it springs back (at once where motion is
/// reduced).
fn let_go(b: &mut Scroller, now: f64, reduce: bool) {
    b.pull = Point::new(0.0, 0.0);
    if b.stretch == Point::new(0.0, 0.0) {
        return;
    }
    if reduce {
        b.stretch = Point::new(0.0, 0.0);
    } else {
        b.spring = Some(Spring {
            from: b.stretch,
            start: now,
        });
    }
    b.moved_at = Some(now);
}

/// Moves a box springing back or going where it was asked on to `now`.
fn step(b: &mut Scroller, now: f64, reduce: bool) {
    if let Some(glide) = b.glide {
        let t = if reduce {
            1.0
        } else {
            ((now - glide.start) / GLIDE) as f32
        };
        if t >= 1.0 {
            b.offset = glide.to;
            b.glide = None;
        } else {
            let at = if t <= 0.0 { 0.0 } else { Ease::Out.at(t) };
            b.offset = Point::new(
                glide.from.x + (glide.to.x - glide.from.x) * at,
                glide.from.y + (glide.to.y - glide.from.y) * at,
            );
        }
        b.moved_at = Some(now);
    }
    if let Some(spring) = b.spring {
        let t = ((now - spring.start).max(0.0)) as f32;
        let left = (1.0 + SPRING * t) * (-SPRING * t).exp();
        let stretch = Point::new(spring.from.x * left, spring.from.y * left);
        if reduce || (stretch.x.abs() < REST && stretch.y.abs() < REST) {
            b.stretch = Point::new(0.0, 0.0);
            b.spring = None;
        } else {
            b.stretch = stretch;
        }
        b.moved_at = Some(now);
    }
}

/// How far a pull of `pull` points draws the content past the end of a box `view` points long.
fn rubber(pull: f32, view: f32) -> f32 {
    if pull == 0.0 || view <= 0.0 {
        return 0.0;
    }
    let drawn = (1.0 - 1.0 / (pull.abs() * RUBBER / view + 1.0)) * view;
    drawn.copysign(pull)
}

/// The pull that draws the content `stretch` points past the end ([`rubber`] the other way).
fn unrubber(stretch: f32, view: f32) -> f32 {
    if stretch == 0.0 || view <= 0.0 {
        return 0.0;
    }
    let share = (stretch.abs() / view).min(0.99);
    ((1.0 / (1.0 - share) - 1.0) * view / RUBBER).copysign(stretch)
}

fn along(point: Point, across: bool) -> f32 {
    if across { point.x } else { point.y }
}

fn set_along(point: &mut Point, across: bool, value: f32) {
    if across {
        point.x = value;
    } else {
        point.y = value;
    }
}

fn reach_of(extent: Extent, across: bool) -> f32 {
    if across {
        extent.max_offset_x()
    } else {
        extent.max_offset()
    }
}

fn view_of(extent: Extent, across: bool) -> f32 {
    if across { extent.view.w } else { extent.view.h }
}

/// How long a bar's track is and its knob, points: the knob is the share of the content the box
/// shows, no shorter than [`KNOB_MIN`], and shorter while the box is pulled past an end.
fn knob_of(extent: Extent, rect: Rect, stretch: Point, across: bool) -> (f32, f32) {
    let length = if across { rect.w } else { rect.h };
    // Where both bars are, each leaves the corner to the other.
    let corner = if reach_of(extent, !across) > 0.0 {
        THICK_OVER + INSET
    } else {
        0.0
    };
    let track = (length - 2.0 * INSET - corner).max(0.0);
    let content = if across {
        extent.content.w
    } else {
        extent.content.h
    };
    let view = view_of(extent, across);
    let mut knob = (track * view / content.max(1.0)).max(KNOB_MIN.min(track));
    let pulled = along(stretch, across).abs();
    if pulled > 0.0 {
        knob = (knob - pulled).max(KNOB_MIN.min(track) / 2.0);
    }
    (track, knob.min(track))
}

fn bar_quad(rect: Rect, radii: [f32; 4], alpha: f32, clip: Rect) -> Quad {
    Quad {
        rect,
        radii,
        fill: Some(Role::Foreground.solid().faded(alpha)),
        gradient: None,
        border: None,
        shadow: None,
        clip,
    }
}
