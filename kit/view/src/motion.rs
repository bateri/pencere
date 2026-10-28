//! Motion: a change of an element moves instead of jumping, as a style sheet's `transition` and a
//! motion library's layout animation do, and the host does the moving without the page.
//!
//! A page says how an element moves ([`Element::motion`]): which of its properties follow a change
//! over time (its colours, its opacity, its size, its place among the others), how it comes in
//! when it appears and how it goes when it is gone. It says nothing of the frames in between: it
//! builds the page as it is now, and [`Motions::layout`] lays it out as it is on the way, keeping
//! by each element's [`Id`] where it was. A page in a sandbox is not asked again for every frame
//! of a move; the host lays the same tree out again until [`Motions::moving`] says no.
//!
//! **Each move stops**, and under Reduce Motion there is none: every change is a snap and nothing
//! comes in or goes out over time (an accessibility setting never adds an animation). Nothing
//! moves on the frame the page changes size either, or for what scrolling brought into view or
//! took out of it: those are not changes of the page, and a list does not ripple as it scrolls.
//!
//! [`Element::motion`]: crate::Element::motion

use std::collections::{HashMap, HashSet};

use crate::layout::{Laid, Node, layout};
use crate::style::{Length, Style};
use crate::{Color, Element, Id, Measure, Mix, Rect, Size};

/// How a change of an element moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    /// How long a change takes, seconds.
    pub seconds: f32,
    /// How the change goes over that time.
    pub ease: Ease,
    /// The colours of its fill, its border, its shadow and its words follow a change.
    pub color: bool,
    /// Its opacity follows a change.
    pub opacity: bool,
    /// A width or a height given in points follows a change (one that is not given in points, the
    /// content's or a share of the parent's, does not: it snaps).
    pub size: bool,
    /// Moved among the others (a row above it went, the list was sorted again), it slides from
    /// where it was to where it is.
    pub place: bool,
    /// How it comes in when it appears on a page that was there before it.
    pub enter: Option<Shift>,
    /// How it goes when it is no longer on the page: the host keeps drawing it, as it was, while
    /// it goes. It cannot be pressed meanwhile.
    pub exit: Option<Shift>,
    /// It turns round and round, clockwise, a turn in this many seconds, for as long as it is on
    /// the page (a spinner while something loads): its picture or its canvas turns about its
    /// middle, its box and its place stay. It does not turn under Reduce Motion.
    pub spin: Option<f32>,
}

impl Motion {
    /// A motion of `seconds` in which nothing moves yet: add what does.
    #[must_use]
    pub const fn new(seconds: f32) -> Self {
        Self {
            seconds,
            ease: Ease::Out,
            color: false,
            opacity: false,
            size: false,
            place: false,
            enter: None,
            exit: None,
            spin: None,
        }
    }

    #[must_use]
    pub const fn ease(self, ease: Ease) -> Self {
        Self { ease, ..self }
    }

    #[must_use]
    pub const fn color(self) -> Self {
        Self {
            color: true,
            ..self
        }
    }

    #[must_use]
    pub const fn opacity(self) -> Self {
        Self {
            opacity: true,
            ..self
        }
    }

    #[must_use]
    pub const fn size(self) -> Self {
        Self { size: true, ..self }
    }

    #[must_use]
    pub const fn place(self) -> Self {
        Self {
            place: true,
            ..self
        }
    }

    #[must_use]
    pub const fn enter(self, from: Shift) -> Self {
        Self {
            enter: Some(from),
            ..self
        }
    }

    #[must_use]
    pub const fn exit(self, to: Shift) -> Self {
        Self {
            exit: Some(to),
            ..self
        }
    }

    /// It turns round, clockwise, a turn in `seconds` (more than 0), for as long as it is on the
    /// page ([`Motion::spin`]).
    #[must_use]
    pub const fn spin(self, seconds: f32) -> Self {
        Self {
            spin: Some(seconds),
            ..self
        }
    }
}

/// How a move goes over its time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Ease {
    /// At one pace from start to end.
    Linear,
    /// Fast at first, settling at the end: what most changes are.
    #[default]
    Out,
    /// Slow at both ends.
    InOut,
    /// As a spring that does not bounce: quick, and long in coming to rest.
    Spring,
}

impl Ease {
    /// How far a move is at `t` of its time (`0.0..=1.0`).
    pub fn at(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::Linear => t,
            Ease::Out => bezier(0.2, 0.0, 0.0, 1.0, t),
            Ease::InOut => bezier(0.4, 0.0, 0.2, 1.0, t),
            Ease::Spring => {
                if t >= 1.0 {
                    1.0
                } else {
                    1.0 - (1.0 + 10.0 * t) * (-10.0 * t).exp()
                }
            }
        }
    }
}

/// The cubic Bézier curve of a style sheet's `cubic-bezier(x1, y1, x2, y2)` at `x`.
fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let curve = |a: f32, b: f32, s: f32| {
        let u = 1.0 - s;
        3.0 * u * u * s * a + 3.0 * u * s * s * b + s * s * s
    };
    // The `s` whose x is `x`, by halving: the curve's x only rises.
    let (mut low, mut high) = (0.0_f32, 1.0_f32);
    for _ in 0..24 {
        let middle = f32::midpoint(low, high);
        if curve(x1, x2, middle) < x {
            low = middle;
        } else {
            high = middle;
        }
    }
    curve(y1, y2, f32::midpoint(low, high))
}

/// Where an element comes in from or goes to, as a difference from where it is: this opacity (of
/// its own), moved this far (points), over this time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shift {
    pub opacity: f32,
    pub dx: f32,
    pub dy: f32,
    /// Seconds.
    pub seconds: f32,
}

impl Shift {
    /// From nothing to all of it (or back) where it stands, over `seconds`.
    #[must_use]
    pub const fn fade(seconds: f32) -> Self {
        Self {
            opacity: 0.0,
            dx: 0.0,
            dy: 0.0,
            seconds,
        }
    }

    /// The same, moved by `(dx, dy)` points.
    #[must_use]
    pub const fn by(self, dx: f32, dy: f32) -> Self {
        Self { dx, dy, ..self }
    }
}

/// What the host says of an element while the pointer rests on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tip {
    pub words: String,
    /// The keys that do the same, as the page writes them.
    pub keys: Option<String>,
}

/// The properties of an element that follow a change, as the page said them last.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Values {
    fill: Option<Color>,
    border: Option<Color>,
    shadow: Option<Color>,
    words: Option<Color>,
    opacity: Option<f32>,
    width: Option<f32>,
    height: Option<f32>,
}

impl Values {
    fn of(style: &Style, motion: &Motion) -> Self {
        let points = |length: Length| match length {
            Length::Px(points) => Some(points),
            _ => None,
        };
        let mut values = Values::default();
        if motion.color {
            values.fill = style.fill;
            values.border = style.border.map(|border| border.color);
            values.shadow = style.shadow.map(|shadow| shadow.color);
            values.words = style.text.color;
        }
        if motion.opacity {
            values.opacity = Some(style.opacity.unwrap_or(1.0));
        }
        if motion.size {
            values.width = points(style.width);
            values.height = points(style.height);
        }
        values
    }

    /// `self` on the way to `to`, `t` of the way.
    fn toward(self, to: Self, t: f32) -> Self {
        let number = |from: Option<f32>, to: Option<f32>| match (from, to) {
            (Some(from), Some(to)) => Some(from + (to - from) * t),
            // From or to a size that is not in points: no way between, it is the new one.
            _ => to,
        };
        Values {
            fill: blend(self.fill, to.fill, t),
            border: blend(self.border, to.border, t),
            shadow: blend(self.shadow, to.shadow, t),
            words: blend(self.words, to.words, t),
            opacity: number(self.opacity, to.opacity),
            width: number(self.width, to.width),
            height: number(self.height, to.height),
        }
    }

    /// Puts what is set here into `style`.
    fn apply(self, style: &mut Style, motion: &Motion) {
        if motion.color {
            style.fill = self.fill;
            if let (Some(border), Some(color)) = (&mut style.border, self.border) {
                border.color = color;
            }
            if let (Some(shadow), Some(color)) = (&mut style.shadow, self.shadow) {
                shadow.color = color;
            }
            style.text.color = self.words;
        }
        if motion.opacity {
            style.opacity = self.opacity;
        }
        if motion.size {
            if let Some(width) = self.width {
                style.width = Length::Px(width);
            }
            if let Some(height) = self.height {
                style.height = Length::Px(height);
            }
        }
    }
}

/// A colour `t` of the way from `from` to `to`. One that comes from none comes from itself,
/// transparent, and one that goes to none goes to itself, transparent.
fn blend(from: Option<Color>, to: Option<Color>, t: f32) -> Option<Color> {
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    match (from, to) {
        (None, None) => None,
        (None, Some(to)) => Some(to.faded(t)),
        (Some(from), None) => (t < 1.0).then(|| from.faded(1.0 - t)),
        (Some(from), Some(to)) if t >= 1.0 || from == to => Some(to),
        (Some(from), Some(to)) => {
            let alpha = lerp(from.alpha, to.alpha);
            Some(match (from.mix, to.mix) {
                // The same colour, more or less of it.
                (a, b) if from.source == to.source && a == b => Color { alpha, ..to },
                // The same colour taken toward the same other, by more or less.
                (Some(a), Some(b)) if from.source == to.source && a.toward == b.toward => Color {
                    mix: Some(Mix {
                        toward: b.toward,
                        amount: lerp(a.amount, b.amount),
                    }),
                    alpha,
                    ..to
                },
                (None, Some(b)) if from.source == to.source => Color {
                    mix: Some(Mix {
                        toward: b.toward,
                        amount: b.amount * t,
                    }),
                    alpha,
                    ..to
                },
                (Some(a), None) if from.source == to.source => Color {
                    mix: Some(Mix {
                        toward: a.toward,
                        amount: a.amount * (1.0 - t),
                    }),
                    alpha,
                    ..to
                },
                // One colour toward another. (What each was taken toward is let go on the way:
                // a colour is one colour toward one other; the move ends on the colour it went to.)
                _ => Color {
                    source: from.source,
                    mix: Some(Mix {
                        toward: to.source,
                        amount: t,
                    }),
                    alpha,
                },
            })
        }
    }
}

/// A move from one thing to another over a time.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Leg {
    start: f64,
    seconds: f32,
    ease: Ease,
}

impl Leg {
    /// How far it is at `now`, eased.
    fn at(&self, now: f64) -> f32 {
        if self.seconds <= 0.0 {
            return 1.0;
        }
        #[allow(clippy::cast_possible_truncation)]
        let t = ((now - self.start) / f64::from(self.seconds)) as f32;
        self.ease.at(t)
    }

    fn done(&self, now: f64) -> bool {
        now >= self.start + f64::from(self.seconds.max(0.0))
    }
}

/// What is kept of an element that moves.
#[derive(Clone, Debug)]
struct Track {
    /// The properties as the page said them last, and where they were coming from.
    target: Values,
    from: Values,
    change: Option<Leg>,
    /// Where it stood among the others when the page was last laid out (its box against the
    /// content of the box that holds it), pixels; and how far from there it was drawn, which
    /// comes down to nothing over `slide`.
    place: (f32, f32),
    offset: (f32, f32),
    slide: Option<Leg>,
    /// Coming in, from `enter`.
    enter: Option<(Shift, Leg)>,
    /// What it looked like when it was last drawn, for going out: its boxes against the content
    /// of the box that held it, and that box's id.
    look: Option<Look>,
}

impl Track {
    fn values(&self, now: f64) -> Values {
        match self.change {
            Some(leg) => self.from.toward(self.target, leg.at(now)),
            None => self.target,
        }
    }

    fn offset(&self, now: f64) -> (f32, f32) {
        match self.slide {
            Some(leg) => {
                let left = 1.0 - leg.at(now);
                (self.offset.0 * left, self.offset.1 * left)
            }
            None => (0.0, 0.0),
        }
    }
}

/// An element as it was last drawn: its boxes and those inside it, against the content of the
/// box that held it.
#[derive(Clone, Debug)]
struct Look {
    nodes: Vec<Node>,
    holder: Id,
    exit: Shift,
}

/// An element that is no longer on the page and is going.
#[derive(Clone, Debug)]
struct Ghost {
    look: Look,
    leg: Leg,
}

/// What the host keeps between the frames of a page for its motion: where each element that moves
/// was, and the elements that are going.
#[derive(Clone, Debug, Default)]
pub struct Motions {
    tracks: HashMap<Id, Track>,
    ghosts: Vec<Ghost>,
    /// How far each box with an id that scrolls was scrolled, pixels.
    scrolls: HashMap<Id, (f32, f32)>,
    /// The page size and scale of the last layout.
    last: Option<(Size, f32)>,
    moving: bool,
}

impl Motions {
    /// Lays `root` out as [`layout`] does, with each element that moves where it is on its way.
    /// `now` is seconds on the host's clock. Afterwards [`Motions::moving`] says whether to lay
    /// it out again at the next frame.
    pub fn layout(
        &mut self,
        mut root: Element,
        viewport: Size,
        scale: f32,
        measure: &mut dyn Measure,
        now: f64,
        reduce_motion: bool,
    ) -> Laid {
        // A page laid out for the first time, or at another size, does not move: what changed is
        // the room it has, not the page.
        let still = reduce_motion || self.last != Some((viewport, scale));
        let first = self.last.is_none();
        self.last = Some((viewport, scale));
        let mut moving = false;
        let mut seen = HashSet::new();
        let mut fresh = HashSet::new();
        self.before(&mut root, now, still, &mut seen, &mut fresh, &mut moving);
        let mut laid = layout(root, viewport, scale, measure);
        self.after(&mut laid, now, still, first, &seen, &fresh, &mut moving);
        // What spins turns with the clock, whatever else moves, and never under Reduce Motion: a
        // spinner asks for every frame while it is on the page.
        if !reduce_motion {
            for node in &mut laid.nodes {
                let Some(seconds) = node.meta.motion.and_then(|motion| motion.spin) else {
                    continue;
                };
                if seconds > 0.0 && seconds.is_finite() && !node.hidden {
                    let turns = now / f64::from(seconds);
                    #[allow(clippy::cast_possible_truncation)]
                    let part = (turns - turns.floor()) as f32;
                    node.turn = part * 360.0;
                    moving = true;
                }
            }
        }
        self.moving = moving;
        laid
    }

    /// Whether anything is on its way: the host lays the page out again at the next frame while
    /// it is.
    pub fn moving(&self) -> bool {
        self.moving
    }

    /// The properties that follow a change, put where they are on their way, in the tree.
    fn before(
        &mut self,
        element: &mut Element,
        now: f64,
        still: bool,
        seen: &mut HashSet<Id>,
        fresh: &mut HashSet<Id>,
        moving: &mut bool,
    ) {
        if let (Some(id), Some(motion)) = (element.meta.id, element.meta.motion) {
            let target = Values::of(&element.style, &motion);
            seen.insert(id);
            let track = self.tracks.entry(id).or_insert_with(|| {
                fresh.insert(id);
                Track {
                    target,
                    from: target,
                    change: None,
                    place: (0.0, 0.0),
                    offset: (0.0, 0.0),
                    slide: None,
                    enter: None,
                    look: None,
                }
            });
            if track.target != target {
                track.from = track.values(now);
                track.target = target;
                track.change = Some(Leg {
                    start: now,
                    seconds: motion.seconds,
                    ease: motion.ease,
                });
            }
            if still || track.change.is_some_and(|leg| leg.done(now)) {
                track.change = None;
            }
            if track.change.is_some() {
                *moving = true;
            }
            track.values(now).apply(&mut element.style, &motion);
        }
        for child in &mut element.children {
            self.before(child, now, still, seen, fresh, moving);
        }
    }

    /// The places, the comings in and the goings out, on the page laid out.
    #[allow(clippy::too_many_arguments)]
    fn after(
        &mut self,
        laid: &mut Laid,
        now: f64,
        still: bool,
        first: bool,
        seen: &HashSet<Id>,
        fresh: &HashSet<Id>,
        moving: &mut bool,
    ) {
        let scale = laid.scale;
        // The boxes that scroll, and whether each has scrolled since the last frame: what
        // scrolling brings in or takes out does not come or go over time.
        let mut scrolls = HashMap::new();
        let mut scrolled = HashSet::new();
        for node in &laid.nodes {
            if let (Some(id), Some(_)) = (node.meta.id, node.extent) {
                scrolls.insert(id, node.scroll);
                if self.scrolls.get(&id).is_some_and(|was| *was != node.scroll) {
                    scrolled.insert(id);
                }
            }
        }
        let holder_of = |nodes: &[Node], index: usize| {
            ancestors(nodes, index).find_map(|parent| Some((parent, nodes.get(parent)?.meta.id?)))
        };
        let in_scrolled = |nodes: &[Node], index: usize| {
            ancestors(nodes, index).any(|parent| {
                nodes
                    .get(parent)
                    .and_then(|node| node.meta.id)
                    .is_some_and(|id| scrolled.contains(&id))
            })
        };

        let mut index = 0;
        while let Some(node) = laid.nodes.get(index) {
            let (Some(id), Some(motion)) = (node.meta.id, node.meta.motion) else {
                index += 1;
                continue;
            };
            let corner = (node.rect.x, node.rect.y);
            let end = subtree_end(&laid.nodes, index);
            let holder = holder_of(&laid.nodes, index);
            let origin = holder
                .and_then(|(parent, _)| laid.nodes.get(parent))
                .map_or((0.0, 0.0), content_origin);
            let place = (corner.0 - origin.0, corner.1 - origin.1);
            let quiet = still || in_scrolled(&laid.nodes, index);
            let Some(track) = self.tracks.get_mut(&id) else {
                index += 1;
                continue;
            };
            if fresh.contains(&id) {
                track.place = place;
                if let Some(enter) = motion.enter.filter(|_| !quiet && !first) {
                    track.enter = Some((
                        enter,
                        Leg {
                            start: now,
                            seconds: enter.seconds,
                            ease: motion.ease,
                        },
                    ));
                }
            } else if track.place != place {
                if motion.place && !quiet {
                    let (x, y) = track.offset(now);
                    track.offset = (x + track.place.0 - place.0, y + track.place.1 - place.1);
                    track.slide = Some(Leg {
                        start: now,
                        seconds: motion.seconds,
                        ease: motion.ease,
                    });
                }
                track.place = place;
            }
            if still || track.slide.is_some_and(|leg| leg.done(now)) {
                track.slide = None;
            }
            if still || track.enter.is_some_and(|(_, leg)| leg.done(now)) {
                track.enter = None;
            }
            let (mut dx, mut dy) = track.offset(now);
            let mut opacity = 1.0;
            if let Some((enter, leg)) = track.enter {
                let left = 1.0 - leg.at(now);
                dx += enter.dx * scale * left;
                dy += enter.dy * scale * left;
                opacity = enter.opacity + (1.0 - enter.opacity) * (1.0 - left);
            }
            if track.slide.is_some() || track.enter.is_some() {
                *moving = true;
            }
            let Some(boxes) = laid.nodes.get_mut(index..end) else {
                break;
            };
            shift(boxes, dx, dy, opacity);
            track.look = match (motion.exit, holder) {
                (Some(exit), Some((_, holder))) => Some(Look {
                    nodes: boxes
                        .iter()
                        .map(|node| {
                            let mut node = node.clone();
                            // The parents inside it, as places in it; its own is the holder's.
                            node.parent = node.parent.and_then(|parent| parent.checked_sub(index));
                            node.rect.x -= origin.0;
                            node.rect.y -= origin.1;
                            node.clip.x -= origin.0;
                            node.clip.y -= origin.1;
                            node
                        })
                        .collect(),
                    holder,
                    exit,
                }),
                _ => None,
            };
            // What is inside it moves with it; an element inside it that moves of its own is
            // still followed.
            index += 1;
        }

        // The elements that are gone: those that go out over time start going, unless scrolling
        // took them out, the page changed size or nothing moves.
        let gone: Vec<Id> = self
            .tracks
            .keys()
            .filter(|id| !seen.contains(id))
            .copied()
            .collect();
        for id in gone {
            let Some(track) = self.tracks.remove(&id) else {
                continue;
            };
            let Some(look) = track.look else { continue };
            if still || scrolled.contains(&look.holder) {
                continue;
            }
            self.ghosts.push(Ghost {
                leg: Leg {
                    start: now,
                    seconds: look.exit.seconds,
                    ease: Ease::Out,
                },
                look,
            });
        }
        if still {
            self.ghosts.clear();
        }
        self.ghosts.retain(|ghost| !ghost.leg.done(now));
        for ghost in &self.ghosts {
            let Some(holder) = laid
                .nodes
                .iter()
                .position(|node| node.meta.id == Some(ghost.look.holder))
            else {
                continue;
            };
            let Some(origin) = laid.nodes.get(holder).map(content_origin) else {
                continue;
            };
            let t = ghost.leg.at(now);
            let exit = ghost.look.exit;
            let (dx, dy) = (
                origin.0 + exit.dx * scale * t,
                origin.1 + exit.dy * scale * t,
            );
            let opacity = 1.0 + (exit.opacity - 1.0) * t;
            let base = laid.nodes.len();
            for (at, node) in ghost.look.nodes.iter().enumerate() {
                let mut node = node.clone();
                node.rect.x += dx;
                node.rect.y += dy;
                node.clip.x += origin.0;
                node.clip.y += origin.1;
                node.opacity *= opacity;
                // Inside the ghost, the parents are the ghost's own; its first box is held by
                // the box that held it.
                node.parent = if at == 0 {
                    Some(holder)
                } else {
                    node.parent.map(|parent| base + parent)
                };
                // What is going is only seen: it cannot be pressed, focused, read or typed in.
                node.meta = crate::element::Meta::default();
                laid.nodes.push(node);
            }
            *moving = true;
        }
        self.scrolls = scrolls;
    }
}

/// The end of the boxes inside the one at `index` (they follow it, in pre-order).
fn subtree_end(nodes: &[Node], index: usize) -> usize {
    let mut end = index + 1;
    while end < nodes.len() && is_under(nodes, end, index) {
        end += 1;
    }
    end
}

fn is_under(nodes: &[Node], index: usize, outer: usize) -> bool {
    ancestors(nodes, index).any(|parent| parent == outer)
}

/// The boxes that hold the one at `index`, nearest first.
fn ancestors(nodes: &[Node], index: usize) -> impl Iterator<Item = usize> + '_ {
    std::iter::successors(nodes.get(index).and_then(|node| node.parent), |&at| {
        nodes.get(at).and_then(|node| node.parent)
    })
}

/// Where the content of a box starts: its corner, less how far it scrolls.
fn content_origin(node: &Node) -> (f32, f32) {
    (node.rect.x - node.scroll.0, node.rect.y - node.scroll.1)
}

/// Moves the boxes of an element and those inside it by `(dx, dy)` pixels and makes them `opacity`
/// as faint. A clip that comes from outside the element stays where it is.
fn shift(nodes: &mut [Node], dx: f32, dy: f32, opacity: f32) {
    if dx == 0.0 && dy == 0.0 && opacity >= 1.0 {
        return;
    }
    let Some(outer_clip) = nodes.first().map(|node| node.clip) else {
        return;
    };
    for node in nodes {
        node.rect.x += dx;
        node.rect.y += dy;
        if node.clip != outer_clip {
            node.clip = Rect::new(node.clip.x + dx, node.clip.y + dy, node.clip.w, node.clip.h);
        }
        node.opacity *= opacity;
    }
}
