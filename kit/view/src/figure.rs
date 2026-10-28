//! Drawing in lines and shapes: a box that holds paths ([`canvas`]) — a chart's bars and line, a
//! ring that fills as a task goes on, the line that joins two things — in the points of its own box.
//!
//! The engine draws no pixels: the paths go into the draw list as they are ([`crate::Figure`]), in
//! the box's pixels, and the host turns them into pixels. The sandbox crate has the one rasteriser
//! every host uses (`pencere_sandbox::figure`), so that a figure is the same on every terminal.

use crate::{Color, Point};

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

/// How the line along a path is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    /// Points.
    pub width: f32,
    pub color: Color,
}

/// One path of a canvas: its steps, the colour inside it and the line along it. One with neither
/// draws nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub segments: Vec<Segment>,
    pub fill: Option<Color>,
    pub stroke: Option<Stroke>,
}

/// A path with no steps yet.
pub fn path() -> Path {
    Path {
        segments: Vec::new(),
        fill: None,
        stroke: None,
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
        self.fill = Some(color.into());
        self
    }

    /// The line along the path, `width` points, round where it turns and ends.
    #[must_use]
    pub fn stroke(mut self, width: f32, color: impl Into<Color>) -> Self {
        self.stroke = Some(Stroke {
            width,
            color: color.into(),
        });
        self
    }

    /// The same path turned `degrees` clockwise about `center`, its own points.
    #[must_use]
    pub(crate) fn turned(mut self, degrees: f32, center: Point) -> Self {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let turn = |at: Point| {
            let (x, y) = (at.x - center.x, at.y - center.y);
            Point::new(center.x + x * cos - y * sin, center.y + x * sin + y * cos)
        };
        for segment in &mut self.segments {
            *segment = match *segment {
                Segment::Move(at) => Segment::Move(turn(at)),
                Segment::Line(at) => Segment::Line(turn(at)),
                Segment::Quad { control, to } => Segment::Quad {
                    control: turn(control),
                    to: turn(to),
                },
                Segment::Cubic { a, b, to } => Segment::Cubic {
                    a: turn(a),
                    b: turn(b),
                    to: turn(to),
                },
                Segment::Close => Segment::Close,
            };
        }
        self
    }

    /// The same path with every colour `opacity` as faint.
    #[must_use]
    pub(crate) fn faded(&self, opacity: f32) -> Self {
        Self {
            segments: self.segments.clone(),
            fill: self.fill.map(|color| color.faded(opacity)),
            stroke: self.stroke.map(|stroke| Stroke {
                color: stroke.color.faded(opacity),
                ..stroke
            }),
        }
    }
}
