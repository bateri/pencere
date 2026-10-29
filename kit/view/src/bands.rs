//! The shape of a selection that runs over several lines: a band of ground for each line, joined
//! where the lines meet and rounded where they do not (the selection of a terminal, of Zed's
//! `rounded_selection`, of VS Code's `roundedSelection`).
//!
//! The engine draws no pixels and this is geometry only: it says which boxes to fill and with which
//! corners round, and the caller draws them as it draws any box. A terminal's grid of cells and an
//! editor's lines of any width go through the same rule, so a selection looks the same in both.
//!
//! A band is a line's box of the selection: `top..bottom` down and `left..right` across (half open:
//! the first column at `left` is in, the one at `right` is not). Bands are given in the order of the
//! lines, at most one a line, and two are neighbours where the first one's `bottom` is the second
//! one's `top`: a line with no band between them, or a gap kept open, splits the shape there into
//! two that are rounded each on its own.
//!
//! Each corner of a band looks at the neighbour on its edge and at whether that neighbour covers
//! the corner's column:
//!
//! - it does not (no neighbour, or the neighbour ends before the column, so that two bands that
//!   touch only at a point stay two) and the corner is **open**: it is rounded;
//! - it does, and the two edges are one (within [`LEVEL`]): the corner is **flush** and the shape
//!   goes straight on;
//! - it does and the neighbour goes further out than this band's edge: the corner is a **step**, the
//!   band's own corner square and the inside of the turn rounded by a fillet, a square of the
//!   radius beside the band that is filled outside a quarter of a circle. The fillet is always the
//!   narrower band's: the wider one's edge on that side is open, so every step is filled once.

use crate::Rect;

/// Two edges closer than this (points or pixels) are one: the lines of a proportional face do not
/// put the same column at the same place to the last bit, and a step of less than half a pixel is
/// no step.
pub const LEVEL: f32 = 0.5;

/// One line's box of a selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

impl Band {
    pub const fn new(top: f32, bottom: f32, left: f32, right: f32) -> Self {
        Self {
            top,
            bottom,
            left,
            right,
        }
    }

    /// Whether it has a box to fill: finite, and wider and taller than nothing.
    fn solid(&self) -> bool {
        [self.top, self.bottom, self.left, self.right]
            .iter()
            .all(|edge| edge.is_finite())
            && self.right > self.left
            && self.bottom > self.top
    }
}

/// A corner of a box, in the order a [`Part::Body`]'s radii are: top left, top right, bottom right,
/// bottom left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

/// One box to fill, as [`rounded`] says.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Part {
    /// The band `band` (its index in the list given): a box with the radius of each corner, top
    /// left, top right, bottom right, bottom left; 0 where it is flush or a step.
    Body {
        band: usize,
        rect: Rect,
        radii: [f32; 4],
    },
    /// The inside of a turn, beside the band `band` at its corner `corner`: a square, filled
    /// everywhere but in a quarter of a circle about one of its corners, so that it rounds the
    /// turn the shape makes there. The circle's centre is the square's bottom left corner for a
    /// top left `corner`, its bottom right for a top right one, its top right for a bottom right
    /// one and its top left for a bottom left one.
    Fillet {
        band: usize,
        rect: Rect,
        corner: Corner,
    },
}

/// How a band joins its neighbour at one corner.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Join {
    /// Rounded, by at most this much: the neighbour's edge is that far inside this one's (no
    /// neighbour, or one that does not reach under the corner, is as far as can be).
    Open(f32),
    Flush,
    /// The neighbour goes further out by this much.
    Step(f32),
}

/// The pieces of a selection of `bands`, the corners that are open rounded by `radius` (points or
/// pixels, as the bands are). A radius of 0 or less (or one that is not a number) gives the bands
/// as they are, square. Each radius is held to what the shape can take: half of the band's width
/// and of its height, and of a fillet, the step it fills and half of the band's height.
pub fn rounded(bands: &[Band], radius: f32) -> Vec<Part> {
    let radius = if radius.is_finite() {
        radius.max(0.0)
    } else {
        0.0
    };
    let mut parts = Vec::with_capacity(bands.len());
    for (index, band) in bands.iter().enumerate() {
        if !band.solid() {
            continue;
        }
        let rect = Rect::new(
            band.left,
            band.top,
            band.right - band.left,
            band.bottom - band.top,
        );
        let neighbour = |other: Option<usize>, touches: fn(&Band, &Band) -> bool| {
            other
                .and_then(|other| bands.get(other))
                .filter(|other| other.solid() && touches(band, other))
        };
        let above = neighbour(index.checked_sub(1), |this, above| {
            (above.bottom - this.top).abs() <= LEVEL
        });
        let below = neighbour(index.checked_add(1), |this, below| {
            (below.top - this.bottom).abs() <= LEVEL
        });
        let held = radius.min(rect.w / 2.0).min(rect.h / 2.0);
        let joins = [
            left_join(band, above),
            right_join(band, above),
            right_join(band, below),
            left_join(band, below),
        ];
        let radii = joins.map(|join| match join {
            Join::Open(reach) => held.min(reach),
            _ => 0.0,
        });
        parts.push(Part::Body {
            band: index,
            rect,
            radii,
        });
        if radius <= 0.0 {
            continue;
        }
        let corners = [
            Corner::TopLeft,
            Corner::TopRight,
            Corner::BottomRight,
            Corner::BottomLeft,
        ];
        for (corner, join) in corners.into_iter().zip(joins) {
            let Join::Step(step) = join else {
                continue;
            };
            let size = radius.min(step).min(rect.h / 2.0);
            if size <= 0.0 {
                continue;
            }
            let (x, y) = match corner {
                Corner::TopLeft => (band.left - size, band.top),
                Corner::TopRight => (band.right, band.top),
                Corner::BottomRight => (band.right, band.bottom - size),
                Corner::BottomLeft => (band.left - size, band.bottom - size),
            };
            parts.push(Part::Fillet {
                band: index,
                rect: Rect::new(x, y, size, size),
                corner,
            });
        }
    }
    parts
}

/// How `this`'s left corner joins the neighbour on its edge: the corner's column is the first
/// one, at `left`.
fn left_join(this: &Band, neighbour: Option<&Band>) -> Join {
    match neighbour {
        Some(other) if other.left <= this.left + LEVEL && this.left < other.right - LEVEL => {
            if other.left < this.left - LEVEL {
                Join::Step(this.left - other.left)
            } else {
                Join::Flush
            }
        }
        // The neighbour starts inside this band: the round corner stays inside the overhang.
        Some(other) if other.left > this.left + LEVEL && other.left < this.right - LEVEL => {
            Join::Open(other.left - this.left)
        }
        _ => Join::Open(f32::INFINITY),
    }
}

/// The same for the right corner: its column is the last one, just inside `right`.
fn right_join(this: &Band, neighbour: Option<&Band>) -> Join {
    match neighbour {
        Some(other) if other.left < this.right - LEVEL && this.right <= other.right + LEVEL => {
            if other.right > this.right + LEVEL {
                Join::Step(other.right - this.right)
            } else {
                Join::Flush
            }
        }
        Some(other) if other.right < this.right - LEVEL && other.right > this.left + LEVEL => {
            Join::Open(this.right - other.right)
        }
        _ => Join::Open(f32::INFINITY),
    }
}
