//! Motion: a change of an element moves instead of jumping, as a style sheet's `transition` and a
//! motion library's layout animation do, and the host does the moving without the page.
//!
//! A page says how an element moves ([`Element::motion`]): which of its properties follow a change
//! over time (its colours, its opacity, its size, its place among the others), how it comes in
//! when it appears and how it goes when it is gone. It says nothing of the frames in between: it
//! builds the page as it is now, and [`Motions::layout`] lays it out as it is on the way, keeping
//! by each element's [`Id`] where it was. A page in a sandbox is not asked again for every frame
//! of a move; the host draws the same tree again until [`Motions::moving`] says no, and lays it out
//! again only while what is on its way takes room ([`Motions::lays`]): a colour, an opacity, a turn
//! or a slide is written on the boxes there are ([`Motions::repaint`]).
//!
//! **Each move stops**, and under Reduce Motion there is none: every change is a snap and nothing
//! comes in or goes out over time (an accessibility setting never adds an animation). Nothing
//! moves on the frame the page changes size either, or for what scrolling brought into view or
//! took out of it: those are not changes of the page, and a list does not ripple as it scrolls.
//!
//! [`Element::motion`]: crate::Element::motion

mod fit;

use std::collections::{HashMap, HashSet};

use crate::layout::{DEFAULT_TEXT, Laid, Node, layout, snap};
use crate::spring::spring_step;
use crate::style::{Border, Length, Shadow, Style};
use crate::{Color, Element, Id, Measure, Mix, Rect, Size};
use fit::Fit;

/// How a change of an element moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    /// How long a change takes, seconds.
    pub seconds: f32,
    /// How the change goes over that time.
    pub ease: Ease,
    /// With [`Ease::Spring`]: how much it bounces, `0.0..=0.9`. 0 comes to rest with no overshoot;
    /// more swings past where it goes and back, the longer the more.
    pub bounce: f32,
    /// The colours of its fill, its border, its shadow and its words follow a change.
    pub color: bool,
    /// Its opacity follows a change.
    pub opacity: bool,
    /// A width or a height given in points follows a change (one that is not given in points, the
    /// content's or a share of the parent's, does not: it snaps).
    pub size: bool,
    /// The width and the height it is laid out at follow a change, whatever its style gives them
    /// (in points, the content's, a share of the parent's): a box that opens to its content or
    /// closes. What is inside is laid out as it is at the size the box is going to, and the box
    /// shows what fits while it is on its way; it cuts what is inside only then. Said with `size`,
    /// it is this that moves the box.
    pub auto_size: bool,
    /// Moved among the others (a row above it went, the list was sorted again), it slides from
    /// where it was to where it is.
    pub place: bool,
    /// Its corners' radii (all four) follow a change.
    pub rounded: bool,
    /// The thickness of its border, and the border's colour, follow a change. It takes room.
    pub border: bool,
    /// What its shadow is: how far it falls, how blurred, and its colour, follow a change.
    pub shadow: bool,
    /// Its padding (all four sides) follows a change. It takes room.
    pub padding: bool,
    /// The size of its words (and of those inside it that have no size of their own) follows a
    /// change. It takes room.
    pub text_size: bool,
    /// The turn and the scale its picture or its canvas has ([`crate::Style::turn`],
    /// [`crate::Style::scale`]) follow a change.
    pub transform: bool,
    /// How it comes in when it appears on a page that was there before it.
    pub enter: Option<Shift>,
    /// How it goes when it is no longer on the page: the host keeps drawing it, as it was, while
    /// it goes. It cannot be pressed meanwhile.
    pub exit: Option<Shift>,
    /// It turns round and round, clockwise, a turn in this many seconds, for as long as it is on
    /// the page (a spinner while something loads): its picture or its canvas turns about its
    /// middle, its box and its place stay. It does not turn under Reduce Motion. It is the
    /// [`Cycle`] that turns 360 degrees, linearly, one way.
    pub spin: Option<f32>,
    /// It goes to a shift and back, over and over, for as long as it is on the page.
    pub cycle: Option<Cycle>,
}

impl Motion {
    /// A motion of `seconds` in which nothing moves yet: add what does.
    #[must_use]
    pub const fn new(seconds: f32) -> Self {
        Self {
            seconds,
            ease: Ease::Out,
            bounce: 0.0,
            color: false,
            opacity: false,
            size: false,
            auto_size: false,
            place: false,
            rounded: false,
            border: false,
            shadow: false,
            padding: false,
            text_size: false,
            transform: false,
            enter: None,
            exit: None,
            spin: None,
            cycle: None,
        }
    }

    #[must_use]
    pub const fn ease(self, ease: Ease) -> Self {
        Self { ease, ..self }
    }

    /// How much a spring bounces, `0.0..=0.9` (a number outside is the nearest).
    #[must_use]
    pub const fn bounce(self, bounce: f32) -> Self {
        let bounce = if bounce > 0.9 { 0.9 } else { bounce };
        let bounce = if bounce > 0.0 { bounce } else { 0.0 };
        Self { bounce, ..self }
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

    /// The width and the height it is laid out at follow a change ([`Motion::auto_size`]).
    #[must_use]
    pub const fn auto_size(self) -> Self {
        Self {
            auto_size: true,
            ..self
        }
    }

    #[must_use]
    pub const fn place(self) -> Self {
        Self {
            place: true,
            ..self
        }
    }

    #[must_use]
    pub const fn rounded(self) -> Self {
        Self {
            rounded: true,
            ..self
        }
    }

    #[must_use]
    pub const fn border(self) -> Self {
        Self {
            border: true,
            ..self
        }
    }

    #[must_use]
    pub const fn shadow(self) -> Self {
        Self {
            shadow: true,
            ..self
        }
    }

    #[must_use]
    pub const fn padding(self) -> Self {
        Self {
            padding: true,
            ..self
        }
    }

    #[must_use]
    pub const fn text_size(self) -> Self {
        Self {
            text_size: true,
            ..self
        }
    }

    #[must_use]
    pub const fn transform(self) -> Self {
        Self {
            transform: true,
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

    /// It goes to `cycle` and back, for as long as it is on the page ([`Motion::cycle`]).
    #[must_use]
    pub const fn cycle(self, cycle: Cycle) -> Self {
        Self {
            cycle: Some(cycle),
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
    /// A spring: quick, and long in coming to rest. [`Ease::at`] is the one that does not bounce;
    /// a move of a motion with a `bounce` swings past where it goes and back, and one sent
    /// elsewhere in flight keeps its speed.
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
/// its own), moved this far (points), turned this many degrees and this many times as large (a
/// picture and a canvas), over this time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shift {
    pub opacity: f32,
    pub dx: f32,
    pub dy: f32,
    /// Degrees clockwise.
    pub rotate: f32,
    /// How many times as large (1: as it is).
    pub scale: f32,
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
            rotate: 0.0,
            scale: 1.0,
            seconds,
        }
    }

    /// The same, moved by `(dx, dy)` points.
    #[must_use]
    pub const fn by(self, dx: f32, dy: f32) -> Self {
        Self { dx, dy, ..self }
    }

    /// The same, turned `degrees` clockwise (a picture or a canvas).
    #[must_use]
    pub const fn turned(self, degrees: f32) -> Self {
        Self {
            rotate: degrees,
            ..self
        }
    }

    /// The same, `scale` times as large (a picture or a canvas).
    #[must_use]
    pub const fn scaled(self, scale: f32) -> Self {
        Self { scale, ..self }
    }
}

/// What an element does over and over, for as long as it is on the page: it goes from where it is
/// to this shift and back (or, with [`Cycle::once`], from where it is to it, and again from the
/// start), a time of `seconds` for the whole way. Its time is the host's clock alone: every cycle
/// of the same seconds is at the same place of it at the same time. Not under Reduce Motion, and
/// not while the element is hidden.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cycle {
    /// A turn of the cycle, seconds.
    pub seconds: f32,
    /// How the way goes: [`Ease::Linear`], [`Ease::Out`] or [`Ease::InOut`] (the others are
    /// [`Ease::InOut`]).
    pub ease: Ease,
    /// There and back; not, it goes one way and starts again.
    pub alternate: bool,
    /// The shift it goes to. A cycle that fades (less than 1) takes at least
    /// [`Cycle::FASTEST_FADE`] for a turn.
    pub opacity: f32,
    pub dx: f32,
    pub dy: f32,
    pub rotate: f32,
    pub scale: f32,
}

impl Cycle {
    /// The least a turn of a cycle that fades takes, seconds: nothing that loops blinks more than
    /// three times a second, which is what a person may be harmed by, however deep it goes.
    pub const FASTEST_FADE: f32 = 0.5;

    /// A cycle of `seconds` that goes nowhere yet: add where.
    #[must_use]
    pub const fn new(seconds: f32) -> Self {
        Self {
            seconds,
            ease: Ease::InOut,
            alternate: true,
            opacity: 1.0,
            dx: 0.0,
            dy: 0.0,
            rotate: 0.0,
            scale: 1.0,
        }
    }

    /// The cycle a spin of `seconds` is: round, one way, at one pace.
    #[must_use]
    pub const fn spin(seconds: f32) -> Self {
        Self {
            ease: Ease::Linear,
            alternate: false,
            rotate: 360.0,
            ..Self::new(seconds)
        }
    }

    #[must_use]
    pub const fn ease(self, ease: Ease) -> Self {
        Self { ease, ..self }
    }

    /// Not there and back: one way, and again.
    #[must_use]
    pub const fn once(self) -> Self {
        Self {
            alternate: false,
            ..self
        }
    }

    /// As faint as `opacity` (`0.0..=1.0`, a number outside is the nearest). The cycle then takes
    /// at least [`Cycle::FASTEST_FADE`] for a turn.
    #[must_use]
    pub const fn fade(self, opacity: f32) -> Self {
        let opacity = if opacity.is_nan() || opacity > 1.0 {
            1.0
        } else if opacity < 0.0 {
            0.0
        } else {
            opacity
        };
        let seconds = if opacity < 1.0 && self.seconds < Self::FASTEST_FADE {
            Self::FASTEST_FADE
        } else {
            self.seconds
        };
        Self {
            opacity,
            seconds,
            ..self
        }
    }

    /// How long a turn takes: the seconds it was given, and where it fades not less than
    /// [`Cycle::FASTEST_FADE`], however it was made.
    fn turn(&self) -> f32 {
        if self.opacity < 1.0 {
            self.seconds.max(Self::FASTEST_FADE)
        } else {
            self.seconds
        }
    }

    #[must_use]
    pub const fn by(self, dx: f32, dy: f32) -> Self {
        Self { dx, dy, ..self }
    }

    #[must_use]
    pub const fn turned(self, degrees: f32) -> Self {
        Self {
            rotate: degrees,
            ..self
        }
    }

    #[must_use]
    pub const fn scaled(self, scale: f32) -> Self {
        Self { scale, ..self }
    }

    /// Where the cycle is at `now`, `0.0..=1.0` of the way from where the element is to its shift.
    fn at(&self, now: f64) -> f32 {
        let turns = now / f64::from(self.turn());
        #[allow(clippy::cast_possible_truncation)]
        let part = (turns - turns.floor()) as f32;
        let along = if self.alternate {
            1.0 - (2.0 * part - 1.0).abs()
        } else {
            part
        };
        let ease = match self.ease {
            Ease::Linear | Ease::Out => self.ease,
            _ => Ease::InOut,
        };
        ease.at(along)
    }
}

/// What the host says of an element while the pointer rests on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tip {
    pub words: String,
    /// The keys that do the same, as the page writes them.
    pub keys: Option<String>,
}

/// Where each of the colours that follow a change is in [`Values::colors`].
const FILL: usize = 0;
const EDGE_COLOR: usize = 1;
const SHADOW_COLOR: usize = 2;
const WORDS: usize = 3;
const COLORS: usize = 4;

/// Where each of the numbers that follow a change is in [`Values::numbers`].
const OPACITY: usize = 0;
const WIDTH: usize = 1;
const HEIGHT: usize = 2;
/// The four radii, from the top left clockwise.
const RADII: usize = 3;
/// The thickness of the border.
const EDGE: usize = 7;
const SHADOW_X: usize = 8;
const SHADOW_Y: usize = 9;
const SHADOW_BLUR: usize = 10;
/// The four sides, top, right, bottom, left.
const PADDING: usize = 11;
const TEXT: usize = 15;
const ROTATE: usize = 16;
const SCALE: usize = 17;
const NUMBERS: usize = 18;

/// What a move goes through, a place for each colour and then each number: its progress, and how
/// fast it was going.
const CHANNELS: usize = COLORS + NUMBERS;

/// The numbers that take room: where they change, the page is laid out again.
const ROOM: [usize; 8] = [
    WIDTH,
    HEIGHT,
    EDGE,
    PADDING,
    PADDING + 1,
    PADDING + 2,
    PADDING + 3,
    TEXT,
];

/// The properties of an element that follow a change, as the page said them last.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Values {
    colors: [Option<Color>; COLORS],
    numbers: [Option<f32>; NUMBERS],
}

impl Values {
    /// What the page says of the properties `motion` follows. `outer` is the size of the words the
    /// element inherits, points.
    fn of(style: &Style, motion: &Motion, outer: f32) -> Self {
        let points = |length: Length| match length {
            Length::Px(points) => Some(points),
            _ => None,
        };
        let mut values = Values::default();
        if motion.color {
            values.colors[FILL] = style.fill;
            values.colors[WORDS] = style.text.color;
        }
        if motion.color || motion.border {
            values.colors[EDGE_COLOR] = style.border.map(|border| border.color);
        }
        if motion.color || motion.shadow {
            values.colors[SHADOW_COLOR] = style.shadow.map(|shadow| shadow.color);
        }
        let numbers = &mut values.numbers;
        if motion.opacity {
            numbers[OPACITY] = Some(style.opacity.unwrap_or(1.0));
        }
        if motion.size {
            numbers[WIDTH] = points(style.width);
            numbers[HEIGHT] = points(style.height);
        }
        if motion.rounded {
            put_four(numbers, RADII, style.radii);
        }
        if motion.border {
            numbers[EDGE] = Some(style.border.map_or(0.0, |border| border.width));
        }
        if motion.shadow {
            let shadow = style.shadow;
            numbers[SHADOW_X] = Some(shadow.map_or(0.0, |shadow| shadow.dx));
            numbers[SHADOW_Y] = Some(shadow.map_or(0.0, |shadow| shadow.dy));
            numbers[SHADOW_BLUR] = Some(shadow.map_or(0.0, |shadow| shadow.blur));
        }
        if motion.padding {
            put_four(numbers, PADDING, style.padding);
        }
        if motion.text_size {
            numbers[TEXT] = Some(style.text.size.unwrap_or(outer));
        }
        if motion.transform {
            numbers[ROTATE] = Some(style.turn);
            numbers[SCALE] = Some(style.scale.unwrap_or(1.0));
        }
        values
    }

    /// `self` on the way to `to`, each colour and number as far as `progress` says. A move that
    /// goes past (a spring that bounces) is held to what a colour, an opacity and a size can be.
    fn toward(self, to: Self, progress: &[f32; CHANNELS]) -> Self {
        let mut values = Values::default();
        let shares = progress.iter().copied();
        for (((slot, from), to), t) in values
            .colors
            .iter_mut()
            .zip(self.colors)
            .zip(to.colors)
            .zip(shares.clone())
        {
            *slot = blend(from, to, t.clamp(0.0, 1.0));
        }
        for (at, (((slot, from), to), t)) in values
            .numbers
            .iter_mut()
            .zip(self.numbers)
            .zip(to.numbers)
            .zip(shares.skip(COLORS))
            .enumerate()
        {
            *slot = match (from, to) {
                (Some(from), Some(to)) => Some(held(at, from + (to - from) * t)),
                // From or to a size that is not in points: no way between, it is the new one.
                _ => to,
            };
        }
        values
    }

    /// Whether going from `self` to `to` takes room: the layout of the page changes on the way, not
    /// only how it is painted. (The colour of the words is laid out with them, where they inherit it.)
    fn takes_room(&self, to: &Self) -> bool {
        self.colors[WORDS] != to.colors[WORDS]
            || ROOM
                .iter()
                .any(|&at| self.numbers.get(at) != to.numbers.get(at))
    }

    /// Puts what takes room, and what is laid out with it, into `style`; what only paints is
    /// written on the boxes ([`Values::paint`]) after the layout.
    fn apply(self, style: &mut Style, motion: &Motion) {
        if motion.color {
            style.text.color = self.colors[WORDS];
        }
        if motion.opacity {
            // The opacity is multiplied into the boxes after the layout, and not twice.
            style.opacity = None;
        }
        if motion.size {
            if let Some(width) = self.numbers[WIDTH] {
                style.width = Length::Px(width);
            }
            if let Some(height) = self.numbers[HEIGHT] {
                style.height = Length::Px(height);
            }
        }
        if motion.border {
            // A border with no thickness is none; one that is going (the page's has none) has
            // the colour it fades in.
            style.border = match (self.numbers[EDGE], self.colors[EDGE_COLOR]) {
                (Some(width), Some(color)) if width > 0.0 => Some(Border { width, color }),
                _ => None,
            };
        }
        if motion.padding {
            for (side, points) in style
                .padding
                .iter_mut()
                .zip(self.numbers.iter().skip(PADDING))
            {
                if let Some(points) = points {
                    *side = *points;
                }
            }
        }
        if motion.text_size {
            // In half points, as the sizes a page says are held (`world.rs`): each new size opens a
            // font and keeps it, and a size that is a new one at each frame would open fonts without end.
            style.text.size =
                self.numbers[TEXT].map(|size| (size.clamp(1.0, 256.0) * 2.0).round() / 2.0);
        }
    }

    /// Puts what only paints into the box laid out, `scale` pixels to a point.
    fn paint(self, node: &mut Node, motion: &Motion, scale: f32) {
        if motion.color {
            node.fill = self.colors[FILL];
        }
        if let (true, Some(border), Some(color)) = (
            motion.color || motion.border,
            &mut node.border,
            self.colors[EDGE_COLOR],
        ) {
            border.color = color;
        }
        if motion.shadow {
            node.shadow = self.colors[SHADOW_COLOR].map(|color| {
                let at = |place: usize| {
                    snap(
                        self.numbers.get(place).copied().flatten().unwrap_or(0.0),
                        scale,
                    )
                };
                Shadow {
                    dx: at(SHADOW_X),
                    dy: at(SHADOW_Y),
                    blur: at(SHADOW_BLUR),
                    color,
                }
            });
        } else if let (true, Some(shadow), Some(color)) =
            (motion.color, &mut node.shadow, self.colors[SHADOW_COLOR])
        {
            shadow.color = color;
        }
        if motion.rounded {
            for (radius, number) in node.radii.iter_mut().zip(self.numbers.iter().skip(RADII)) {
                *radius = snap(number.unwrap_or(0.0), scale);
            }
        }
        if motion.transform {
            node.turn = self.numbers[ROTATE].unwrap_or(0.0);
            node.scaled = self.numbers[SCALE].unwrap_or(1.0);
        }
    }

    /// How much of what it draws shows, of the element's own: what is multiplied into its boxes.
    fn shows(self, motion: &Motion) -> f32 {
        match self.numbers[OPACITY].filter(|_| motion.opacity) {
            Some(opacity) if opacity.is_finite() => opacity.clamp(0.0, 1.0),
            _ => 1.0,
        }
    }
}

/// Four numbers (the corners, the sides) put from `first` on.
fn put_four(numbers: &mut [Option<f32>; NUMBERS], first: usize, values: [f32; 4]) {
    if let Some(slots) = numbers.get_mut(first..first + 4) {
        for (slot, value) in slots.iter_mut().zip(values) {
            *slot = Some(value);
        }
    }
}

/// A number on the way, held to what it can be: an opacity is `0.0..=1.0`, a turn and how far a
/// shadow falls are any, the rest is not less than nothing.
fn held(number: usize, value: f32) -> f32 {
    match number {
        OPACITY => value.clamp(0.0, 1.0),
        ROTATE | SHADOW_X | SHADOW_Y => value,
        _ => value.max(0.0),
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
    /// How much a spring bounces.
    bounce: f32,
    /// How fast each channel was going when the move began, as a share of its way, a second: where
    /// a spring is sent elsewhere in flight, it keeps its speed.
    rates: [f32; CHANNELS],
}

impl Leg {
    fn new(start: f64, seconds: f32, ease: Ease, bounce: f32) -> Self {
        Self {
            start,
            seconds,
            ease,
            bounce,
            rates: [0.0; CHANNELS],
        }
    }

    /// How damped the spring is: 1 comes to rest with no overshoot.
    fn zeta(&self) -> f32 {
        (1.0 - self.bounce).clamp(0.1, 1.0)
    }

    /// How long it lasts. A spring is let the longer for the bounce it has: it is at rest, to the
    /// eye, by then, and put where it goes.
    fn lasts(&self) -> f32 {
        match self.ease {
            Ease::Spring => self.seconds * (0.7 / self.zeta()).max(1.0),
            _ => self.seconds,
        }
    }

    /// How far channel `channel` is at `now`, eased: 0 to 1 (a spring that bounces goes past 1).
    fn progress(&self, now: f64, channel: usize) -> f32 {
        if self.seconds <= 0.0 {
            return 1.0;
        }
        #[allow(clippy::cast_possible_truncation)]
        let elapsed = (now - self.start).max(0.0) as f32;
        match self.ease {
            Ease::Spring if elapsed >= self.lasts() => 1.0,
            Ease::Spring => {
                let rate = self.rates.get(channel).copied().unwrap_or(0.0);
                let (error, _) = spring_step(-1.0, rate, 10.0 / self.seconds, self.zeta(), elapsed);
                1.0 + error
            }
            ease => ease.at(elapsed / self.seconds),
        }
    }

    /// How far it is at `now`, for what goes by no channel.
    fn at(&self, now: f64) -> f32 {
        self.progress(now, 0)
    }

    /// How fast channel `channel` goes at `now`, shares of its way a second.
    fn speed(&self, now: f64, channel: usize) -> f32 {
        if self.seconds <= 0.0 || self.done(now) {
            return 0.0;
        }
        #[allow(clippy::cast_possible_truncation)]
        let elapsed = (now - self.start).max(0.0) as f32;
        match self.ease {
            Ease::Spring => {
                let rate = self.rates.get(channel).copied().unwrap_or(0.0);
                spring_step(-1.0, rate, 10.0 / self.seconds, self.zeta(), elapsed).1
            }
            ease => {
                let step = 1.0e-3 * self.seconds;
                let before = (elapsed - step).max(0.0);
                (ease.at((elapsed + step) / self.seconds) - ease.at(before / self.seconds))
                    / (elapsed + step - before)
            }
        }
    }

    fn done(&self, now: f64) -> bool {
        now >= self.start + f64::from(self.lasts().max(0.0))
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
    /// The size it is laid out at, where it follows that ([`Motion::auto_size`]).
    fit: Option<Fit>,
    /// What it looked like when it was last drawn, for going out: its boxes against the content
    /// of the box that held it, and that box's id.
    look: Option<Look>,
}

impl Track {
    fn values(&self, now: f64) -> Values {
        match self.change {
            Some(leg) => {
                let progress = if leg.ease == Ease::Spring {
                    // Each goes at a speed of its own where it was sent elsewhere in flight.
                    std::array::from_fn(|channel| leg.progress(now, channel))
                } else {
                    [leg.at(now); CHANNELS]
                };
                self.from.toward(self.target, &progress)
            }
            None => self.target,
        }
    }

    /// How fast each channel goes at `now`, as a share of the way from `here` to `to`, where it is
    /// sent there from the move it is on: a spring keeps its speed through it, whichever way it
    /// goes now.
    fn carry(&self, now: f64, here: &Values, to: &Values) -> [f32; CHANNELS] {
        let mut rates = [0.0; CHANNELS];
        let Some(leg) = self.change else {
            return rates;
        };
        for (channel, rate) in rates.iter_mut().enumerate().take(COLORS) {
            *rate = leg.speed(now, channel);
        }
        let numbers = (self.from.numbers.into_iter())
            .zip(self.target.numbers)
            .zip(here.numbers)
            .zip(to.numbers);
        for (at, (((then, was), now_at), next)) in numbers.enumerate() {
            let (Some(then), Some(was), Some(now_at), Some(next)) = (then, was, now_at, next)
            else {
                continue;
            };
            let units = (was - then) * leg.speed(now, COLORS + at);
            let distance = next - now_at;
            if distance.abs() > 0.01
                && units.is_finite()
                && let Some(rate) = rates.get_mut(COLORS + at)
            {
                *rate = units / distance;
            }
        }
        rates
    }

    /// How far from its place it is drawn at `now`, pixels: each way has a channel of its own (0 and
    /// 1) in the move, as a spring that was sent elsewhere in flight keeps the speed of each.
    fn offset(&self, now: f64) -> (f32, f32) {
        match self.slide {
            Some(leg) => (
                self.offset.0 * (1.0 - leg.progress(now, 0)),
                self.offset.1 * (1.0 - leg.progress(now, 1)),
            ),
            None => (0.0, 0.0),
        }
    }

    /// How fast that offset goes at `now`, pixels a second.
    fn offset_speed(&self, now: f64) -> (f32, f32) {
        match self.slide {
            Some(leg) => (
                -self.offset.0 * leg.speed(now, 0),
                -self.offset.1 * leg.speed(now, 1),
            ),
            None => (0.0, 0.0),
        }
    }
}

/// The speed a move of a distance (from where it is drawn to its place) must start with, as a share
/// of the distance a second, to go at `speed` (pixels a second) away from where it was.
fn carried(speed: f32, distance: f32) -> f32 {
    if distance.abs() > 0.01 && speed.is_finite() {
        -speed / distance
    } else {
        0.0
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
    /// What moved at the last frame.
    pace: Pace,
    /// The last page laid out, as the layout made it, before what moves was put on it: what
    /// [`Motions::repaint`] draws the frames that follow from.
    base: Option<Laid>,
}

/// What moves at a frame.
#[derive(Clone, Copy, Debug, Default)]
struct Pace {
    /// Something moves that comes to an end: a change on its way, a slide, a coming in, a going out.
    finite: bool,
    /// Something moves for as long as it is on the page: what spins.
    endless: bool,
    /// What is on its way takes room, so that the next frame is laid out again.
    lays: bool,
    /// A box follows the size it is laid out at ([`Motion::auto_size`]): the tree is kept for a
    /// second layout.
    fits: bool,
}

impl Motions {
    /// Lays `root` out as [`layout`] does, with each element that moves where it is on its way.
    /// `now` is seconds on the host's clock. Afterwards [`Motions::moving`] says whether to draw
    /// the page again at the next frame, and [`Motions::lays`] whether to lay it out again for it.
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
        let mut pace = Pace::default();
        let mut seen = HashSet::new();
        let mut fresh = HashSet::new();
        let outer = DEFAULT_TEXT.size.unwrap_or(12.5);
        self.before(
            &mut root, now, still, &mut seen, &mut fresh, &mut pace, outer,
        );
        let spare = pace.fits.then(|| root.clone());
        let mut laid = layout(root, viewport, scale, measure);
        if let Some(spare) = &spare {
            laid = self.fit(laid, spare, viewport, measure, now, still, &mut pace);
        }
        // What the frames that follow are drawn from, while something on the page can move.
        self.base = laid
            .nodes
            .iter()
            .any(|node| {
                node.meta.motion.is_some_and(|motion| {
                    motion.spin.is_some() || motion.cycle.is_some() || node.meta.id.is_some()
                })
            })
            .then(|| laid.clone());
        self.after(&mut laid, now, still, first, &seen, &fresh, &mut pace);
        cycles(&mut laid, now, reduce_motion, &mut pace);
        self.pace = pace;
        laid
    }

    /// The page as [`Motions::layout`] last laid it out, drawn again at `now`: what is on its way
    /// (a colour, an opacity, a slide, a coming in or going out, a turn) put on its boxes as it is
    /// then. It is the page `layout` would give for the same tree and time, with no layout done: the
    /// frames of what only paints. `None` where there is none to draw from (nothing on the page
    /// moves, or what moves takes room: [`Motions::lays`]).
    pub fn repaint(&mut self, now: f64, reduce_motion: bool) -> Option<Laid> {
        if self.pace.lays {
            return None;
        }
        let mut laid = self.base.clone()?;
        let mut pace = Pace::default();
        for track in self.tracks.values_mut() {
            if reduce_motion || track.change.is_some_and(|leg| leg.done(now)) {
                track.change = None;
            }
            pace.finite |= track.change.is_some();
        }
        let seen = self.tracks.keys().copied().collect();
        self.after(
            &mut laid,
            now,
            reduce_motion,
            false,
            &seen,
            &HashSet::new(),
            &mut pace,
        );
        cycles(&mut laid, now, reduce_motion, &mut pace);
        self.pace = pace;
        Some(laid)
    }

    /// Whether anything is on its way: the host draws the page again at the next frame while it is.
    pub fn moving(&self) -> bool {
        self.pace.finite || self.pace.endless
    }

    /// Whether something is on its way that comes to an end (a change, a slide, a coming in or a
    /// going out), and not only what moves for as long as it is on the page (a spin).
    pub fn ends(&self) -> bool {
        self.pace.finite
    }

    /// Whether what is on its way takes room (a width in points, the colour of words that are laid
    /// out with them): the page is laid out again at the next frame. Where it is not, the page is
    /// drawn again from the layout it has ([`Motions::repaint`]).
    pub fn lays(&self) -> bool {
        self.pace.lays
    }

    /// The properties that follow a change, put where they are on their way, in the tree. `outer`
    /// is the size of the words the element inherits, as the page says it.
    #[allow(clippy::too_many_arguments)]
    fn before(
        &mut self,
        element: &mut Element,
        now: f64,
        still: bool,
        seen: &mut HashSet<Id>,
        fresh: &mut HashSet<Id>,
        pace: &mut Pace,
        outer: f32,
    ) {
        let said = element.style.text.size.unwrap_or(outer);
        if let (Some(id), Some(motion)) = (element.meta.id, element.meta.motion) {
            // A box that follows its laid size is moved by that, whatever its style says.
            pace.fits |= motion.auto_size;
            let motion = Motion {
                size: motion.size && !motion.auto_size,
                ..motion
            };
            let target = Values::of(&element.style, &motion, outer);
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
                    fit: None,
                    look: None,
                }
            });
            if track.target != target {
                let here = track.values(now);
                let mut leg = Leg::new(now, motion.seconds, motion.ease, motion.bounce);
                if motion.ease == Ease::Spring {
                    leg.rates = track.carry(now, &here, &target);
                }
                track.from = here;
                track.target = target;
                track.change = Some(leg);
            }
            if still || track.change.is_some_and(|leg| leg.done(now)) {
                track.change = None;
            }
            if track.change.is_some() {
                pace.finite = true;
                pace.lays |= track.from.takes_room(&track.target);
            }
            track.values(now).apply(&mut element.style, &motion);
        }
        for child in &mut element.children {
            self.before(child, now, still, seen, fresh, pace, said);
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
        pace: &mut Pace,
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
                        Leg::new(now, enter.seconds, motion.ease, motion.bounce),
                    ));
                }
            } else if track.place != place {
                if motion.place && !quiet {
                    let (x, y) = track.offset(now);
                    let (vx, vy) = track.offset_speed(now);
                    track.offset = (x + track.place.0 - place.0, y + track.place.1 - place.1);
                    let mut leg = Leg::new(now, motion.seconds, motion.ease, motion.bounce);
                    if motion.ease == Ease::Spring {
                        // Sent elsewhere in flight, a spring goes on at the speed it had.
                        leg.rates[0] = carried(vx, track.offset.0);
                        leg.rates[1] = carried(vy, track.offset.1);
                    }
                    track.slide = Some(leg);
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
            let (mut opacity, mut turn, mut size) = (1.0, 0.0, 1.0);
            if let Some((enter, leg)) = track.enter {
                let left = 1.0 - leg.at(now);
                dx += enter.dx * scale * left;
                dy += enter.dy * scale * left;
                opacity = (enter.opacity + (1.0 - enter.opacity) * (1.0 - left)).clamp(0.0, 1.0);
                turn = enter.rotate * left;
                size = (1.0 + (enter.scale - 1.0) * left).max(0.0);
            }
            if track.slide.is_some() || track.enter.is_some() {
                pace.finite = true;
            }
            // What only paints is written on the boxes: the colours on this one, the opacity on it
            // and what is inside it.
            let values = track.values(now);
            let Some(boxes) = laid.nodes.get_mut(index..end) else {
                break;
            };
            if let Some(node) = boxes.first_mut() {
                values.paint(node, &motion, scale);
                node.turn += turn;
                node.scaled *= size;
            }
            shift(boxes, dx, dy, opacity * values.shows(&motion));
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
                leg: Leg::new(now, look.exit.seconds, Ease::Out, 0.0),
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
            let Some((origin, cut)) = laid.nodes.get(holder).map(|node| {
                // What the box that held it cuts now, not as it did where it went from. A box that
                // follows its size cuts what went from it at rest too: that is not the page's.
                let cut = if node.clips || node.meta.motion.is_some_and(|motion| motion.auto_size) {
                    node.clip.intersect(node.rect)
                } else {
                    node.clip
                };
                (content_origin(node), cut)
            }) else {
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
                node.clip = node.clip.intersect(cut);
                node.opacity *= opacity;
                if at == 0 {
                    node.turn += exit.rotate * t;
                    node.scaled *= (1.0 + (exit.scale - 1.0) * t).max(0.0);
                }
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
            pace.finite = true;
        }
        self.scrolls = scrolls;
    }
}

/// What spins or cycles goes by the clock, whatever else moves, and never under Reduce Motion or
/// while it is hidden: a spinner asks for every frame while it is on the page. A box that has both
/// a spin and a cycle has what each does, and what the others do of its own (a turn it has, a
/// slide, a coming in) goes with them: turns are added, the sizes and the opacities multiplied.
fn cycles(laid: &mut Laid, now: f64, reduce_motion: bool, pace: &mut Pace) {
    if reduce_motion {
        return;
    }
    let scale = laid.scale;
    for index in 0..laid.nodes.len() {
        let Some(node) = laid.nodes.get(index) else {
            break;
        };
        let Some(motion) = node.meta.motion.filter(|_| !node.hidden) else {
            continue;
        };
        let (mut dx, mut dy, mut opacity, mut turn, mut size) = (0.0, 0.0, 1.0, 0.0, 1.0);
        let mut any = false;
        let spin = motion.spin.map(Cycle::spin);
        for cycle in spin.into_iter().chain(motion.cycle) {
            if !(cycle.seconds > 0.0 && cycle.seconds.is_finite()) {
                continue;
            }
            let along = cycle.at(now);
            dx += cycle.dx * scale * along;
            dy += cycle.dy * scale * along;
            let faint = if cycle.opacity.is_finite() {
                cycle.opacity.clamp(0.0, 1.0)
            } else {
                1.0
            };
            opacity *= 1.0 + (faint - 1.0) * along;
            turn += cycle.rotate * along;
            size *= (1.0 + (cycle.scale - 1.0) * along).max(0.0);
            any = true;
        }
        if !any {
            continue;
        }
        pace.endless = true;
        let end = subtree_end(&laid.nodes, index);
        if let Some(boxes) = laid.nodes.get_mut(index..end) {
            if let Some(node) = boxes.first_mut() {
                node.turn += turn;
                node.scaled *= size;
            }
            shift(boxes, dx, dy, opacity);
            for node in boxes.iter_mut() {
                node.cycled.0 += dx;
                node.cycled.1 += dy;
            }
        }
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
