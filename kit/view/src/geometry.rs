//! Points, sizes and rectangles. The engine's numbers are `f32`: authored in points,
//! laid out and drawn in pixels.

/// A rectangle: the top-left corner and the size, with `y` growing downward.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// A rectangle that clips nothing.
    pub const UNBOUNDED: Self = Self {
        x: -1.0e6,
        y: -1.0e6,
        w: 2.0e6,
        h: 2.0e6,
    };

    pub fn right(self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(self) -> f32 {
        self.y + self.h
    }

    /// Where the two overlap; a rectangle with no area (at the nearer corner) when they do not.
    #[must_use]
    pub fn intersect(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        Self {
            x,
            y,
            w: (right - x).max(0.0),
            h: (bottom - y).max(0.0),
        }
    }

    #[must_use]
    pub fn translate(self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
    }

    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
}

/// A width and a height.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub w: f32,
    pub h: f32,
}

impl Size {
    pub const fn new(w: f32, h: f32) -> Self {
        Self { w, h }
    }
}

/// A point.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// What a box lets be drawn of what is in it: a rectangle, and, where the innermost box that clips
/// has rounded corners, their shape (`round`). The rectangle is where every box that clips around
/// it ends; the corners are the innermost one's alone, so that a box that clips and is square does
/// not square the corners of the one around it, and of two rounded ones the outer cuts as a
/// rectangle does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clip {
    pub rect: Rect,
    pub round: Option<Round>,
}

/// The shape of a rounded clip: the box that is cut (the padding box of the one that clips) and the
/// radius of its four corners, the border's width less, as CSS's `overflow` cuts at the inner edge
/// of a border: top-left, top-right, bottom-right, bottom-left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Round {
    pub rect: Rect,
    pub radii: [f32; 4],
}

impl Clip {
    /// A rectangle's clip: square.
    pub const fn new(rect: Rect) -> Self {
        Self { rect, round: None }
    }

    /// A clip that cuts nothing.
    pub const UNBOUNDED: Self = Self::new(Rect::UNBOUNDED);

    /// The same clip cut further at `rect`: the corners stay as they were.
    #[must_use]
    pub fn intersect(self, rect: Rect) -> Self {
        Self {
            rect: self.rect.intersect(rect),
            round: self.round,
        }
    }

    /// This clip, of what was in a box that cut at `was`, carried by (`dx`, `dy`) into the box as it cuts
    /// at `now`: cut at both rectangles, with `now`'s corners where this clip had `was`'s (the box's
    /// own, which may be rounded otherwise now) and its own where a box inside it rounds them.
    #[must_use]
    pub fn carried_into(self, was: Self, now: Self, dx: f32, dy: f32) -> Self {
        let moved = self.translate(dx, dy);
        Self {
            rect: moved.rect.intersect(now.rect),
            round: if self.round == was.round {
                now.round
            } else {
                moved.round
            },
        }
    }

    /// What a box that clips at `inside` (its padding box) lets be drawn, in a clip that is this: the
    /// rectangle cut at `inside`, and the corners that `radii` say where one is rounded (a square one
    /// leaves the corners of the box around it).
    #[must_use]
    pub fn within(self, inside: Rect, radii: [f32; 4]) -> Self {
        let rect = self.rect.intersect(inside);
        let round = if radii.iter().any(|radius| *radius > 0.0) {
            Some(Round {
                rect: inside,
                radii,
            })
        } else {
            self.round
        };
        Self { rect, round }
    }

    /// The same clip moved.
    #[must_use]
    pub fn translate(self, dx: f32, dy: f32) -> Self {
        Self {
            rect: self.rect.translate(dx, dy),
            round: self.round.map(|round| Round {
                rect: round.rect.translate(dx, dy),
                radii: round.radii,
            }),
        }
    }

    /// Whether the point is where drawing is let: in the rectangle and inside the corners.
    pub fn contains(self, x: f32, y: f32) -> bool {
        self.rect.contains(x, y) && self.round.is_none_or(|round| round.contains(x, y))
    }
}

impl From<Rect> for Clip {
    fn from(rect: Rect) -> Self {
        Self::new(rect)
    }
}

/// A clip with square corners is its rectangle.
impl PartialEq<Rect> for Clip {
    fn eq(&self, other: &Rect) -> bool {
        self.round.is_none() && self.rect == *other
    }
}

impl Round {
    /// How far the point is from the shape's edge: negative inside it, positive outside, zero on it.
    pub fn distance(self, x: f32, y: f32) -> f32 {
        let Rect {
            x: left,
            y: top,
            w,
            h,
        } = self.rect;
        let (half_w, half_h) = (w.max(0.0) / 2.0, h.max(0.0) / 2.0);
        let limit = half_w.min(half_h);
        let [tl, tr, br, bl] = self.radii.map(|radius| radius.clamp(0.0, limit));
        let (px, py) = (x - (left + half_w), y - (top + half_h));
        let radius = match (py < 0.0, px < 0.0) {
            (true, true) => tl,
            (true, false) => tr,
            (false, false) => br,
            (false, true) => bl,
        };
        let (qx, qy) = (px.abs() - (half_w - radius), py.abs() - (half_h - radius));
        qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
    }

    /// Whether the point is inside the shape.
    pub fn contains(self, x: f32, y: f32) -> bool {
        self.distance(x, y) <= 0.0
    }

    /// How much of a pixel centred at the point the shape holds, `0.0..=1.0`: its edge is a pixel wide, as
    /// a box's is drawn.
    pub fn cover(self, x: f32, y: f32) -> f32 {
        (0.5 - self.distance(x, y)).clamp(0.0, 1.0)
    }
}
