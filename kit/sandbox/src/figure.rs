//! A canvas in pixels: what every host draws the paths of a [`Figure`] with, so that a figure is
//! the same on every terminal, as an image is ([`crate::Images`]).
//!
//! The colours are the host's to say (a role in its theme, a value's form for it); everything else
//! — how a curve is followed, how wide a line is and how its ends and turns are drawn, which parts
//! of a path that crosses itself are inside — is this.

use std::hash::{Hash, Hasher};

use pencere_view::{Color, Figure, Segment};
use resvg::tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform,
};

use crate::images::Bitmap;

/// The most pixels a side of a figure is drawn in: a canvas larger than any screen is drawn no
/// larger, and the rest of it is not seen.
const MOST_SIDE: u32 = 8192;

/// The ids of figures have the top bit set and those of images ([`crate::Images`]) never do: a host
/// that keeps the textures of both by id keeps them apart.
const FIGURE_ID: u64 = 1 << 63;

/// The paths of `figure` drawn into pixels the size of its box (whole pixels, rounded up), each
/// colour as `resolve` says it is — sRGB with its opacity, each `0.0..=1.0`, as
/// [`Color::resolve`] gives it. `None` for a box with no room, or one with nothing that is drawn.
///
/// The bitmap's `id` is the same for the same figure in the same colours: a host that drew it
/// once need not upload it again.
pub fn draw_figure(figure: &Figure, mut resolve: impl FnMut(Color) -> [f32; 4]) -> Option<Bitmap> {
    let side = |length: f32| {
        if length.is_finite() && length > 0.0 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            Some((length.ceil() as u32).min(MOST_SIDE))
        } else {
            None
        }
    };
    let (width, height) = (side(figure.rect.w)?, side(figure.rect.h)?);
    let scale = if figure.scale.is_finite() && figure.scale > 0.0 {
        figure.scale
    } else {
        1.0
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (width, height, scale.to_bits()).hash(&mut hasher);

    let mut pixmap = Pixmap::new(width, height)?;
    let transform = Transform::from_scale(scale, scale);
    let mut drew = false;
    for path in &figure.paths {
        let Some(shape) = shape(&path.segments, &mut hasher) else {
            continue;
        };
        if let Some(fill) = path.fill {
            let rgba = resolve(fill);
            hash_rgba(rgba, &mut hasher);
            pixmap.fill_path(&shape, &paint(rgba), FillRule::EvenOdd, transform, None);
            drew = true;
        }
        if let Some(line) = path.stroke {
            let width = if line.width.is_finite() {
                line.width
            } else {
                0.0
            };
            if width > 0.0 {
                let rgba = resolve(line.color);
                hash_rgba(rgba, &mut hasher);
                width.to_bits().hash(&mut hasher);
                let stroke = Stroke {
                    width,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    ..Stroke::default()
                };
                pixmap.stroke_path(&shape, &paint(rgba), &stroke, transform, None);
                drew = true;
            }
        }
    }
    drew.then(|| Bitmap::new(hasher.finish() | FIGURE_ID, width, height, pixmap.take()))
}

/// The steps of a path as a shape to draw, in the canvas's points; `None` for one with nothing to
/// draw. A step that is not a number is passed over.
fn shape(segments: &[Segment], hasher: &mut impl Hasher) -> Option<resvg::tiny_skia::Path> {
    let mut builder = PathBuilder::new();
    let finite = |p: pencere_view::Point| p.x.is_finite() && p.y.is_finite();
    for segment in segments {
        match *segment {
            Segment::Move(to) if finite(to) => {
                hash_points(&[to], 0, hasher);
                builder.move_to(to.x, to.y);
            }
            Segment::Line(to) if finite(to) => {
                hash_points(&[to], 1, hasher);
                builder.line_to(to.x, to.y);
            }
            Segment::Quad { control, to } if finite(control) && finite(to) => {
                hash_points(&[control, to], 2, hasher);
                builder.quad_to(control.x, control.y, to.x, to.y);
            }
            Segment::Cubic { a, b, to } if finite(a) && finite(b) && finite(to) => {
                hash_points(&[a, b, to], 3, hasher);
                builder.cubic_to(a.x, a.y, b.x, b.y, to.x, to.y);
            }
            Segment::Close => {
                4_u8.hash(hasher);
                builder.close();
            }
            _ => {}
        }
    }
    builder.finish()
}

fn hash_points(points: &[pencere_view::Point], step: u8, hasher: &mut impl Hasher) {
    step.hash(hasher);
    for point in points {
        (point.x.to_bits(), point.y.to_bits()).hash(hasher);
    }
}

fn hash_rgba(rgba: [f32; 4], hasher: &mut impl Hasher) {
    for channel in rgba {
        channel.to_bits().hash(hasher);
    }
}

fn paint(rgba: [f32; 4]) -> Paint<'static> {
    let clamp = |value: f32| {
        if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(
        channel(clamp(rgba[0])),
        channel(clamp(rgba[1])),
        channel(clamp(rgba[2])),
        channel(clamp(rgba[3])),
    );
    paint.anti_alias = true;
    paint
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn channel(value: f32) -> u8 {
    (value * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use pencere_view::{Rect, Role, path};

    fn figure(paths: Vec<pencere_view::Path>, scale: f32) -> Figure {
        Figure {
            rect: Rect::new(0.0, 0.0, 20.0 * scale, 20.0 * scale),
            scale,
            paths,
            clip: Rect::new(0.0, 0.0, 1000.0, 1000.0),
        }
    }

    fn solid(_: Color) -> [f32; 4] {
        [1.0, 0.0, 0.0, 1.0]
    }

    fn pixel(bitmap: &Bitmap, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * bitmap.width + x) * 4) as usize;
        let p = bitmap.pixels();
        [p[at], p[at + 1], p[at + 2], p[at + 3]]
    }

    #[test]
    fn a_filled_square_is_its_colour_inside_and_nothing_outside_at_the_scale_of_the_page() {
        let square = path().rect(5.0, 5.0, 10.0, 10.0).fill(Role::Accent);
        let bitmap = draw_figure(&figure(vec![square], 2.0), solid).expect("drawn");
        assert_eq!((bitmap.width, bitmap.height), (40, 40));
        assert_eq!(pixel(&bitmap, 20, 20), [255, 0, 0, 255]);
        assert_eq!(pixel(&bitmap, 5, 5), [0, 0, 0, 0]);
        assert_eq!(pixel(&bitmap, 35, 35), [0, 0, 0, 0]);
        assert!(bitmap.id & FIGURE_ID != 0);
    }

    #[test]
    fn a_line_is_as_wide_as_it_says_and_half_its_colour_is_half_its_opacity_premultiplied() {
        let line = path()
            .move_to(0.0, 10.0)
            .line_to(20.0, 10.0)
            .stroke(4.0, Role::Accent);
        let half = |_: Color| [1.0, 1.0, 1.0, 0.5];
        let bitmap = draw_figure(&figure(vec![line], 1.0), half).expect("drawn");
        assert_eq!(pixel(&bitmap, 10, 10), [128, 128, 128, 128]);
        assert_eq!(pixel(&bitmap, 10, 2), [0, 0, 0, 0]);
    }

    #[test]
    fn the_same_figure_in_the_same_colours_is_the_same_bitmap_and_another_is_not() {
        let ring = || path().circle(10.0, 10.0, 6.0).stroke(2.0, Role::Accent);
        let a = draw_figure(&figure(vec![ring()], 1.0), solid).expect("drawn");
        let b = draw_figure(&figure(vec![ring()], 1.0), solid).expect("drawn");
        assert_eq!(a.id, b.id);
        let other =
            draw_figure(&figure(vec![ring()], 1.0), |_| [0.0, 1.0, 0.0, 1.0]).expect("drawn");
        assert_ne!(a.id, other.id);
        let moved = path().circle(11.0, 10.0, 6.0).stroke(2.0, Role::Accent);
        let c = draw_figure(&figure(vec![moved], 1.0), solid).expect("drawn");
        assert_ne!(a.id, c.id);
    }

    #[test]
    fn a_figure_with_no_room_or_nothing_drawn_is_no_bitmap() {
        assert!(draw_figure(&figure(vec![path().rect(0.0, 0.0, 5.0, 5.0)], 1.0), solid).is_none());
        let mut empty = figure(
            vec![path().rect(0.0, 0.0, 5.0, 5.0).fill(Role::Accent)],
            1.0,
        );
        empty.rect.w = 0.0;
        assert!(draw_figure(&empty, solid).is_none());
        let odd = path()
            .move_to(f32::NAN, 0.0)
            .line_to(f32::INFINITY, 3.0)
            .fill(Role::Accent);
        assert!(draw_figure(&figure(vec![odd], 1.0), solid).is_none());
    }
}
