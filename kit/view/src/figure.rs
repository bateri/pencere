//! Drawing in lines and shapes: a box that holds paths ([`canvas`]) — a chart's bars and line, a
//! ring that fills as a task goes on, the line that joins two things — in the points of its own box.
//!
//! The engine draws no pixels: the paths go into the draw list as they are ([`crate::Figure`]), in
//! the box's pixels, and the host turns them into pixels. The sandbox crate has the one rasteriser
//! every host uses (`pencere_sandbox::figure`), so that a figure is the same on every terminal.

use crate::{Color, Point, Role, Shadow};

/// A step of a path, in the points of the canvas's box (its top-left corner is the origin).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    /// Starts a new part of the path at the point, drawing nothing.
    Move(Point),
    /// A straight line to the point.
    Line(Point),
    /// A curve to `to`, bent toward `control` (a quadratic Bézier).
    Quad { control: Point, to: Point },
    /// A curve to `to`, leaving toward `a` and arriving from `b` (a cubic Bézier).
    Cubic { a: Point, b: Point, to: Point },
    /// A straight line back to where the part began, closing it.
    Close,
}

/// One colour of a gradient and where along it the colour is: `offset` is `0.0` at the gradient's
/// start and `1.0` at its end. An offset outside that, or one that is not a number, is taken at the
/// nearest end, and one that is less than the offset of the stop before it is taken at that.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    pub offset: f32,
    pub color: Color,
}

impl Stop {
    pub fn new(offset: f32, color: impl Into<Color>) -> Self {
        Self {
            offset,
            color: color.into(),
        }
    }
}

/// What a fill or a line is painted with: a colour, or a gradient from one colour to others. The
/// points of a gradient are the path's own, so that they go where the path's transform takes it.
///
/// A gradient with one stop is that colour, and one that cannot be drawn (its two points are the
/// same, its radius is nothing, a point of it is not a number) is its last stop's colour all over.
/// One with no stop paints nothing.
#[derive(Clone, Debug, PartialEq)]
pub enum Brush {
    Color(Color),
    /// From `from` (offset `0.0`) to `to` (offset `1.0`); before and after them the first and the
    /// last colour go on.
    Linear {
        from: Point,
        to: Point,
        stops: Vec<Stop>,
    },
    /// From `center` (offset `0.0`) out to `radius` from it (offset `1.0`); past it the last colour
    /// goes on.
    Radial {
        center: Point,
        radius: f32,
        stops: Vec<Stop>,
    },
}

impl Brush {
    /// A gradient along the line from `(x0, y0)` to `(x1, y1)`.
    pub fn linear(
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        stops: impl IntoIterator<Item = Stop>,
    ) -> Self {
        Self::Linear {
            from: Point::new(x0, y0),
            to: Point::new(x1, y1),
            stops: stops.into_iter().collect(),
        }
    }

    /// A gradient out from `(cx, cy)` to `radius` from it.
    pub fn radial(cx: f32, cy: f32, radius: f32, stops: impl IntoIterator<Item = Stop>) -> Self {
        Self::Radial {
            center: Point::new(cx, cy),
            radius,
            stops: stops.into_iter().collect(),
        }
    }

    /// The colour, when the brush is one and not a gradient.
    pub fn color(&self) -> Option<Color> {
        match self {
            Self::Color(color) => Some(*color),
            Self::Linear { .. } | Self::Radial { .. } => None,
        }
    }

    /// The same brush with every colour `opacity` as faint.
    #[must_use]
    pub(crate) fn faded(&self, opacity: f32) -> Self {
        let fade = |stops: &[Stop]| {
            stops
                .iter()
                .map(|stop| Stop {
                    color: stop.color.faded(opacity),
                    ..*stop
                })
                .collect()
        };
        match self {
            Self::Color(color) => Self::Color(color.faded(opacity)),
            Self::Linear { from, to, stops } => Self::Linear {
                from: *from,
                to: *to,
                stops: fade(stops),
            },
            Self::Radial {
                center,
                radius,
                stops,
            } => Self::Radial {
                center: *center,
                radius: *radius,
                stops: fade(stops),
            },
        }
    }
}

impl From<Color> for Brush {
    fn from(color: Color) -> Self {
        Self::Color(color)
    }
}

/// How the ends of a line are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cap {
    /// Half a circle past the end, as wide as the line.
    #[default]
    Round,
    /// Cut square at the end.
    Butt,
    /// Cut square half the line's width past the end.
    Square,
}

/// How a line is drawn where it turns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Join {
    #[default]
    Round,
    /// The two edges run on to meet in a point; a turn so sharp that the point would be longer than
    /// four times half the line's width (a style sheet's default limit) is bevelled instead.
    Miter,
    /// Cut square across the turn.
    Bevel,
}

/// How a path is drawn over what is under it in its canvas. Where nothing is under it (the canvas's
/// own pixels are transparent) every one of them is the path's colour as it is, and nothing but
/// what the same canvas drew earlier is mixed with: not the page behind the canvas.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Blend {
    /// Laid over.
    #[default]
    Normal,
    /// Added: each channel is the sum of the two, to its most. Lights that overlap grow brighter.
    Add,
    /// Each channel is as if two lights were shone one on the other: never darker than either.
    Screen,
    /// Each channel is the product of the two, as with two filters one behind the other: never
    /// lighter than either.
    Multiply,
}

/// How the line along a path is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    /// Points.
    pub width: f32,
    pub brush: Brush,
    /// The lengths, points, of the dashes and the gaps between them, in turn, from a dash. A list of
    /// an odd number of lengths is read twice over (a style sheet's `stroke-dasharray`); one that is
    /// empty, whose lengths are all nothing, or one of which is not a number or less than nothing,
    /// is a line with no gaps.
    pub dash: Vec<f32>,
    /// How far into the dashes the line starts, points.
    pub dash_offset: f32,
    pub cap: Cap,
    pub join: Join,
}

impl Stroke {
    /// A line `width` points wide in `brush`, round where it turns and ends, with no gaps.
    pub fn new(width: f32, brush: impl Into<Brush>) -> Self {
        Self {
            width,
            brush: brush.into(),
            dash: Vec::new(),
            dash_offset: 0.0,
            cap: Cap::Round,
            join: Join::Round,
        }
    }
}

/// A path's own transform: where each of its points goes, and its gradients, and how wide its line
/// is, in the canvas's points. It is the six numbers of a style sheet's `matrix(a b c d e f)`: the
/// point `(x, y)` goes to `(a x + c y + e, b x + d y + f)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Matrix {
    /// Every point where it is.
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// What a page says: scaled, then turned `rotate` degrees clockwise (the way `arc` and a spin
    /// go), both about `origin`, then moved by `translate`. The one place the order is written.
    #[must_use]
    pub fn from_parts(
        scale: (f32, f32),
        rotate: f32,
        translate: (f32, f32),
        origin: Point,
    ) -> Self {
        let scaling = Self {
            a: scale.0,
            d: scale.1,
            ..Self::IDENTITY
        };
        Self::moving(-origin.x, -origin.y)
            .then(scaling)
            .then(Self::turning(rotate, Point::new(0.0, 0.0)))
            .then(Self::moving(origin.x + translate.0, origin.y + translate.1))
    }

    /// A turn of `degrees` clockwise about `center`.
    #[must_use]
    pub fn turning(degrees: f32, center: Point) -> Self {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: center.x - (cos * center.x - sin * center.y),
            f: center.y - (sin * center.x + cos * center.y),
        }
    }

    fn moving(x: f32, y: f32) -> Self {
        Self {
            e: x,
            f: y,
            ..Self::IDENTITY
        }
    }

    /// This transform, and then `next`.
    #[must_use]
    pub fn then(self, next: Self) -> Self {
        Self {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            e: next.a * self.e + next.c * self.f + next.e,
            f: next.b * self.e + next.d * self.f + next.f,
        }
    }

    /// Where `at` goes.
    pub fn apply(self, at: Point) -> Point {
        Point::new(
            self.a * at.x + self.c * at.y + self.e,
            self.b * at.x + self.d * at.y + self.f,
        )
    }
}

impl Default for Matrix {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// One path of a canvas: its steps, the paint inside it and the line along it. One with neither
/// draws nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub segments: Vec<Segment>,
    pub fill: Option<Brush>,
    pub stroke: Option<Stroke>,
    /// Drawn with no smoothing: a pixel is the path's where its middle is inside it (or under its
    /// line), and is otherwise not touched. A point of the page is seldom a whole number of pixels
    /// (a host draws a page as large as its own text is to the kit's), so the edges of shapes laid
    /// on a grid of points fall between pixels: smoothed, each such edge is a blurred pixel and two
    /// shapes that touch leave a faint seam; crisp, they are whole pixels, some a pixel wider than
    /// others. A line thinner than a pixel may not be drawn at all.
    pub crisp: bool,
    pub blend: Blend,
    /// What the path drawn (its fill and its line) leaves behind it: moved by `(dx, dy)` and blurred
    /// over `blur`, in the canvas's points **after** the transform (a turned shape's glow stays
    /// round), drawn under the path in its blend. With no offset and a bright colour it is a glow.
    /// The canvas's own pixels are all there is: a shadow past the box is cut.
    pub shadow: Option<Shadow>,
    /// Where the path is drawn: its points, its gradients and the width of its line all go by it.
    pub transform: Matrix,
}

/// A path with no steps yet.
pub fn path() -> Path {
    Path {
        segments: Vec::new(),
        fill: None,
        stroke: None,
        crisp: false,
        blend: Blend::Normal,
        shadow: None,
        transform: Matrix::IDENTITY,
    }
}

/// How many cubic curves a whole turn of a circle is drawn with.
const QUARTERS: usize = 4;

impl Path {
    #[must_use]
    pub fn move_to(mut self, x: f32, y: f32) -> Self {
        self.segments.push(Segment::Move(Point::new(x, y)));
        self
    }

    #[must_use]
    pub fn line_to(mut self, x: f32, y: f32) -> Self {
        self.segments.push(Segment::Line(Point::new(x, y)));
        self
    }

    #[must_use]
    pub fn quad_to(mut self, cx: f32, cy: f32, x: f32, y: f32) -> Self {
        self.segments.push(Segment::Quad {
            control: Point::new(cx, cy),
            to: Point::new(x, y),
        });
        self
    }

    #[must_use]
    pub fn cubic_to(mut self, ax: f32, ay: f32, bx: f32, by: f32, x: f32, y: f32) -> Self {
        self.segments.push(Segment::Cubic {
            a: Point::new(ax, ay),
            b: Point::new(bx, by),
            to: Point::new(x, y),
        });
        self
    }

    #[must_use]
    pub fn close(mut self) -> Self {
        self.segments.push(Segment::Close);
        self
    }

    /// A rectangle, as a closed part of the path.
    #[must_use]
    pub fn rect(self, x: f32, y: f32, w: f32, h: f32) -> Self {
        self.move_to(x, y)
            .line_to(x + w, y)
            .line_to(x + w, y + h)
            .line_to(x, y + h)
            .close()
    }

    /// An arc of the circle about `(cx, cy)` of radius `r`, from the angle `start` to `end`, in
    /// degrees clockwise from the top (`0` to `360` is the whole circle, from twelve o'clock round):
    /// how a ring that fills is drawn. It begins a new part of the path where it starts.
    #[must_use]
    pub fn arc(mut self, cx: f32, cy: f32, r: f32, start: f32, end: f32) -> Self {
        let at = |degrees: f32| {
            let radians = (degrees - 90.0).to_radians();
            Point::new(cx + r * radians.cos(), cy + r * radians.sin())
        };
        let sweep = (end - start).clamp(-360.0, 360.0);
        if !sweep.is_finite() || !r.is_finite() || sweep == 0.0 {
            return self;
        }
        self.segments.push(Segment::Move(at(start)));
        // Each piece is at most a quarter turn, drawn as the cubic that follows a circle best.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let pieces = ((sweep.abs() / 90.0).ceil() as usize).clamp(1, QUARTERS);
        #[allow(clippy::cast_precision_loss)]
        let step = sweep / pieces as f32;
        let handle = 4.0 / 3.0 * (step.to_radians() / 4.0).tan() * r;
        for piece in 0..pieces {
            #[allow(clippy::cast_precision_loss)]
            let from = start + step * piece as f32;
            let to = from + step;
            let (p0, p3) = (at(from), at(to));
            let tangent = |degrees: f32| {
                let radians = (degrees - 90.0).to_radians();
                (-radians.sin(), radians.cos())
            };
            let (t0, t3) = (tangent(from), tangent(to));
            self.segments.push(Segment::Cubic {
                a: Point::new(p0.x + t0.0 * handle, p0.y + t0.1 * handle),
                b: Point::new(p3.x - t3.0 * handle, p3.y - t3.1 * handle),
                to: p3,
            });
        }
        self
    }

    /// The whole circle about `(cx, cy)` of radius `r`, closed.
    #[must_use]
    pub fn circle(self, cx: f32, cy: f32, r: f32) -> Self {
        self.arc(cx, cy, r, 0.0, 360.0).close()
    }

    /// The colour inside the path (its parts that cross themselves are inside where a line out of
    /// them crosses the path an odd number of times).
    #[must_use]
    pub fn fill(mut self, color: impl Into<Color>) -> Self {
        self.fill = Some(Brush::Color(color.into()));
        self
    }

    /// The inside of the path painted with `brush`: a gradient, say.
    #[must_use]
    pub fn fill_brush(mut self, brush: Brush) -> Self {
        self.fill = Some(brush);
        self
    }

    /// The line along the path, `width` points, round where it turns and ends unless [`Path::cap`] and
    /// [`Path::join`] say otherwise; the dashes it was given stay.
    #[must_use]
    pub fn stroke(self, width: f32, color: impl Into<Color>) -> Self {
        self.stroke_brush(width, Brush::Color(color.into()))
    }

    /// The line along the path painted with `brush`.
    #[must_use]
    pub fn stroke_brush(mut self, width: f32, brush: Brush) -> Self {
        let stroke = self.line();
        stroke.width = width;
        stroke.brush = brush;
        self
    }

    /// The line broken into dashes `pattern` long and the gaps between them, in turn, starting
    /// `offset` into it ([`Stroke::dash`]). Whichever of this and [`Path::stroke`] comes first.
    #[must_use]
    pub fn dash(mut self, pattern: Vec<f32>, offset: f32) -> Self {
        let stroke = self.line();
        stroke.dash = pattern;
        stroke.dash_offset = offset;
        self
    }

    /// How the ends of the line are drawn. Whichever of this and [`Path::stroke`] comes first.
    #[must_use]
    pub fn cap(mut self, cap: Cap) -> Self {
        self.line().cap = cap;
        self
    }

    /// How the line is drawn where it turns. Whichever of this and [`Path::stroke`] comes first.
    #[must_use]
    pub fn join(mut self, join: Join) -> Self {
        self.line().join = join;
        self
    }

    /// The line the options go to: one with no width until [`Path::stroke`] gives it one, which
    /// draws nothing.
    fn line(&mut self) -> &mut Stroke {
        self.stroke
            .get_or_insert_with(|| Stroke::new(0.0, Color::from(Role::Foreground)))
    }

    /// Whether the path is drawn with no smoothing, its pixels whole ([`Path::crisp`]): dots of a
    /// dithered fill, pixel art, the bars of a chart that are to meet without a seam.
    #[must_use]
    pub fn crisp(mut self, on: bool) -> Self {
        self.crisp = on;
        self
    }

    /// How the path is drawn over what the canvas has under it ([`Blend`]).
    #[must_use]
    pub fn blend(mut self, blend: Blend) -> Self {
        self.blend = blend;
        self
    }

    /// What the path leaves behind it ([`Path::shadow`]): moved `(dx, dy)` points and blurred over
    /// `blur` (a deviation of half of it, as a box's shadow is), in `color`. With no offset and a
    /// bright colour, a glow.
    #[must_use]
    pub fn shadow(mut self, dx: f32, dy: f32, blur: f32, color: impl Into<Color>) -> Self {
        self.shadow = Some(Shadow {
            dx,
            dy,
            blur,
            color: color.into(),
        });
        self
    }

    /// Where the path is drawn ([`Path::transform`]).
    #[must_use]
    pub fn transform(mut self, transform: Matrix) -> Self {
        self.transform = transform;
        self
    }

    /// The same path turned `degrees` clockwise about `center`, a point of the canvas: its transform
    /// is its own and then the turn, the points are as they were, and a shadow's offset turns with
    /// the picture.
    #[must_use]
    pub(crate) fn turned(self, degrees: f32, center: Point) -> Self {
        self.posed(degrees, 1.0, center)
    }

    /// The same path drawn `scale` times as large about the origin of the canvas and then turned
    /// `degrees` clockwise about `center`: what a canvas that is scaled and turned does, with its box
    /// grown to be the size it is drawn at. A shadow's offset goes by it as the shadow does.
    #[must_use]
    pub(crate) fn posed(mut self, degrees: f32, scale: f32, center: Point) -> Self {
        let pose = Matrix {
            a: scale,
            d: scale,
            ..Matrix::IDENTITY
        }
        .then(Matrix::turning(degrees, center));
        self.transform = self.transform.then(pose);
        if let Some(shadow) = &mut self.shadow {
            let at = Matrix::turning(degrees, Point::new(0.0, 0.0))
                .apply(Point::new(shadow.dx * scale, shadow.dy * scale));
            shadow.dx = at.x;
            shadow.dy = at.y;
        }
        self
    }

    /// The same path with every colour `opacity` as faint.
    #[must_use]
    pub(crate) fn faded(&self, opacity: f32) -> Self {
        Self {
            segments: self.segments.clone(),
            fill: self.fill.as_ref().map(|brush| brush.faded(opacity)),
            stroke: self.stroke.as_ref().map(|stroke| Stroke {
                width: stroke.width,
                brush: stroke.brush.faded(opacity),
                dash: stroke.dash.clone(),
                dash_offset: stroke.dash_offset,
                cap: stroke.cap,
                join: stroke.join,
            }),
            crisp: self.crisp,
            blend: self.blend,
            shadow: self.shadow.map(|shadow| Shadow {
                color: shadow.color.faded(opacity),
                ..shadow
            }),
            transform: self.transform,
        }
    }
}
