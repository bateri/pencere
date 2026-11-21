//! A canvas in pixels: what every host draws the paths of a [`Figure`] with, so that a figure is
//! the same on every terminal, as an image is ([`crate::Images`]).
//!
//! The colours are the host's to say (a role in its theme, a value's form for it); everything else
//! — how a curve is followed, how wide a line is and how its ends and turns are drawn, where it is
//! broken into dashes, how a gradient runs, how a path mixes with what is under it, where a path
//! has been moved to, what it leaves behind as a shadow, which parts of a path that crosses itself
//! are inside, whether its edges are smoothed — is this.

use std::hash::{Hash, Hasher};

use pencere_view::{Blend, Brush, Cap, Color, Figure, Join, Segment, Stop};
use resvg::tiny_skia::{
    self, BlendMode, FillRule, GradientStop, LineCap, LineJoin, LinearGradient, Paint, PathBuilder,
    Pixmap, PixmapPaint, RadialGradient, Shader, SpreadMode, Stroke, StrokeDash, Transform,
};

use crate::blur::Blur;
use crate::images::Bitmap;

/// The most pixels a side of a figure is drawn in: a canvas larger than any screen is drawn no
/// larger, and the rest of it is not seen.
const MOST_SIDE: u32 = 8192;

/// The most a shadow is blurred over, points: a deviation of half of it.
const MOST_BLUR: f32 = 64.0;

/// A dash list whose lengths come to less than this, points, is a line with no gaps: it would be
/// drawn as a million dashes, which a person cannot tell from a line.
const LEAST_DASH: f32 = 0.1;

/// The ids of figures have the top bit set and those of images ([`crate::Images`]) never do: a host
/// that keeps the textures of both by id keeps them apart.
const FIGURE_ID: u64 = 1 << 63;

/// What a figure is known by without drawing it, so that a host that still has its pixels need not
/// draw it again: a canvas is laid out anew with each frame, and drawing it costs what its pixels do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FigureKey {
    /// The `id` of the bitmap [`draw_figure`] gives: the same for the same figure in the same
    /// colours, wherever it stands.
    pub id: u64,
    /// The bitmap's size, pixels: the figure's box, whole pixels rounded up.
    pub width: u32,
    pub height: u32,
}

/// [`draw_figure`]'s answer but for the pixels: `None` where it draws nothing.
pub fn figure_key(figure: &Figure, resolve: impl FnMut(Color) -> [f32; 4]) -> Option<FigureKey> {
    planned(figure, resolve).map(|plan| plan.key)
}

/// The paths of `figure` drawn into pixels the size of its box (whole pixels, rounded up), each
/// colour as `resolve` says it is — sRGB with its opacity, each `0.0..=1.0`, as
/// [`Color::resolve`] gives it. `None` for a box with no room, or one with nothing that is drawn.
///
/// The bitmap's `id` is the same for the same figure in the same colours: a host that drew it
/// once need not upload it again, nor draw it ([`figure_key`]).
pub fn draw_figure(figure: &Figure, resolve: impl FnMut(Color) -> [f32; 4]) -> Option<Bitmap> {
    let Plan { key, scale, parts } = planned(figure, resolve)?;
    let mut pixmap = Pixmap::new(key.width, key.height)?;
    let mut room = Room::default();
    for part in &parts {
        let transform = part.matrix.post_scale(scale, scale);
        if let Some(shade) = &part.shadow {
            cast(&mut pixmap, part, shade, (transform, scale), &mut room);
        }
        if let Some(ink) = &part.fill {
            let paint = painted(ink, part.crisp, part.blend);
            pixmap.fill_path(&part.shape, &paint, FillRule::EvenOdd, transform, None);
        }
        if let Some(line) = &part.line {
            let paint = painted(&line.ink, part.crisp, part.blend);
            pixmap.stroke_path(&part.shape, &paint, &stroked(line), transform, None);
        }
    }
    Some(Bitmap::new(key.id, key.width, key.height, pixmap.take()))
}

/// A figure made ready to draw: what it is known by, and what is drawn of each of its paths.
struct Plan {
    key: FigureKey,
    scale: f32,
    parts: Vec<Part>,
}

struct Part {
    shape: tiny_skia::Path,
    fill: Option<Ink>,
    line: Option<Line>,
    crisp: bool,
    blend: BlendMode,
    shadow: Option<Shade>,
    /// The path's own transform, in the canvas's points (the page's scale not in it).
    matrix: Transform,
}

/// What a fill or a line is painted with, its colours the host's.
enum Ink {
    Solid([f32; 4]),
    /// A gradient is `(offset, colour)` stops, the offsets in `0.0..=1.0` and each no less than the one
    /// before.
    Linear {
        from: (f32, f32),
        to: (f32, f32),
        stops: Vec<(f32, [f32; 4])>,
    },
    Radial {
        center: (f32, f32),
        radius: f32,
        stops: Vec<(f32, [f32; 4])>,
    },
}

struct Line {
    ink: Ink,
    width: f32,
    cap: LineCap,
    join: LineJoin,
    /// The lengths of the dashes and the gaps, an even number of them, and where it starts.
    dash: Option<(Vec<f32>, f32)>,
}

/// What the shadows of one figure are made in, one after another: the coverage of a shadow and the room
/// its blur works in.
#[derive(Default)]
struct Room {
    grid: Vec<f32>,
    blur: Blur,
}

/// A shadow: its offset and its blur in points, and its colour.
struct Shade {
    dx: f32,
    dy: f32,
    blur: f32,
    rgba: [f32; 4],
}

fn planned(figure: &Figure, mut resolve: impl FnMut(Color) -> [f32; 4]) -> Option<Plan> {
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

    let mut parts = Vec::new();
    for path in &figure.paths {
        let Some(matrix) = matrix_of(path.transform) else {
            continue;
        };
        let Some(shape) = shape(&path.segments, &mut hasher) else {
            continue;
        };
        let fill = path
            .fill
            .as_ref()
            .and_then(|brush| inked(brush, &mut resolve))
            .inspect(|ink| hash_ink(ink, &mut hasher));
        let line = path.stroke.as_ref().and_then(|line| {
            let width = if line.width.is_finite() {
                line.width
            } else {
                0.0
            };
            let ink = if width > 0.0 {
                inked(&line.brush, &mut resolve)?
            } else {
                return None;
            };
            hash_ink(&ink, &mut hasher);
            width.to_bits().hash(&mut hasher);
            let dash = dash_of(&line.dash, line.dash_offset);
            hash_dash(&dash, &mut hasher);
            (line.cap as u8, line.join as u8).hash(&mut hasher);
            Some(Line {
                ink,
                width,
                cap: match line.cap {
                    Cap::Round => LineCap::Round,
                    Cap::Butt => LineCap::Butt,
                    Cap::Square => LineCap::Square,
                },
                join: match line.join {
                    Join::Round => LineJoin::Round,
                    Join::Miter => LineJoin::Miter,
                    Join::Bevel => LineJoin::Bevel,
                },
                dash,
            })
        });
        if fill.is_some() || line.is_some() {
            // The same steps drawn crisp are other pixels: a host holding the smoothed ones draws
            // again.
            path.crisp.hash(&mut hasher);
            let blend = match path.blend {
                Blend::Normal => BlendMode::SourceOver,
                Blend::Add => BlendMode::Plus,
                Blend::Screen => BlendMode::Screen,
                Blend::Multiply => BlendMode::Multiply,
            };
            (path.blend as u8).hash(&mut hasher);
            for number in [
                path.transform.a,
                path.transform.b,
                path.transform.c,
                path.transform.d,
                path.transform.e,
                path.transform.f,
            ] {
                number.to_bits().hash(&mut hasher);
            }
            let shadow = path.shadow.and_then(|shadow| {
                let rgba = resolve(shadow.color);
                shade(shadow.dx, shadow.dy, shadow.blur, rgba)
            });
            match &shadow {
                Some(shade) => {
                    1_u8.hash(&mut hasher);
                    for number in [shade.dx, shade.dy, shade.blur] {
                        number.to_bits().hash(&mut hasher);
                    }
                    hash_rgba(shade.rgba, &mut hasher);
                }
                None => 0_u8.hash(&mut hasher),
            }
            parts.push(Part {
                shape,
                fill,
                line,
                crisp: path.crisp,
                blend,
                shadow,
                matrix,
            });
        }
    }
    (!parts.is_empty()).then(|| Plan {
        key: FigureKey {
            id: hasher.finish() | FIGURE_ID,
            width,
            height,
        },
        scale,
        parts,
    })
}
/// The transform of a path as tiny-skia has it; `None` where the path has nowhere to be: a number of
/// it is not a number, or it folds the plane flat.
fn matrix_of(matrix: pencere_view::Matrix) -> Option<Transform> {
    let numbers = [matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f];
    let flat = matrix.a * matrix.d - matrix.b * matrix.c;
    (numbers.iter().all(|number| number.is_finite()) && flat.is_finite() && flat != 0.0)
        .then(|| Transform::from_row(matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f))
}

/// A brush with its colours the host's. A gradient with one stop is that colour and one that cannot
/// be drawn (a point of it is not a number, its two points are one, its radius is nothing) is its
/// last stop's colour; one with no stop is `None`: it paints nothing.
fn inked(brush: &Brush, resolve: &mut impl FnMut(Color) -> [f32; 4]) -> Option<Ink> {
    /// Shorter than this a gradient is not one that can be drawn, points.
    const LEAST: f32 = 1e-4;
    let finite = |at: pencere_view::Point| at.x.is_finite() && at.y.is_finite();
    match brush {
        Brush::Color(color) => Some(Ink::Solid(resolve(*color))),
        Brush::Linear { from, to, stops } => {
            let stops = stops_of(stops, resolve);
            let last = stops.last()?.1;
            let length = (to.x - from.x).hypot(to.y - from.y);
            if stops.len() == 1 || !finite(*from) || !finite(*to) || length < LEAST {
                return Some(Ink::Solid(last));
            }
            Some(Ink::Linear {
                from: (from.x, from.y),
                to: (to.x, to.y),
                stops,
            })
        }
        Brush::Radial {
            center,
            radius,
            stops,
        } => {
            let stops = stops_of(stops, resolve);
            let last = stops.last()?.1;
            if stops.len() == 1 || !finite(*center) || !radius.is_finite() || *radius < LEAST {
                return Some(Ink::Solid(last));
            }
            Some(Ink::Radial {
                center: (center.x, center.y),
                radius: *radius,
                stops,
            })
        }
    }
}

/// The stops of a gradient with their offsets put right (in `0.0..=1.0`, none before the one
/// before it; one that is not a number at the start) and their colours the host's.
fn stops_of(stops: &[Stop], resolve: &mut impl FnMut(Color) -> [f32; 4]) -> Vec<(f32, [f32; 4])> {
    let mut before = 0.0_f32;
    stops
        .iter()
        .map(|stop| {
            let offset = if stop.offset.is_finite() {
                stop.offset.clamp(0.0, 1.0)
            } else {
                0.0
            };
            before = offset.max(before);
            (before, resolve(stop.color))
        })
        .collect()
}

/// The dashes and gaps of a line as it is drawn: an odd number of lengths read twice; `None`, no
/// gaps, for no lengths, ones that come to nothing (or to less than [`LEAST_DASH`]) and ones that
/// are not numbers or are less than nothing.
fn dash_of(list: &[f32], offset: f32) -> Option<(Vec<f32>, f32)> {
    let total: f32 = list.iter().sum();
    if list
        .iter()
        .any(|length| !length.is_finite() || *length < 0.0)
        || !total.is_finite()
        || total < LEAST_DASH
    {
        return None;
    }
    let mut lengths = list.to_vec();
    if lengths.len() % 2 == 1 {
        lengths.extend_from_slice(list);
    }
    let offset = if offset.is_finite() { offset } else { 0.0 };
    Some((lengths, offset))
}

/// A shadow as it is drawn; `None` for one that has nothing to draw or a number that is not one.
fn shade(dx: f32, dy: f32, blur: f32, rgba: [f32; 4]) -> Option<Shade> {
    let numbers = [dx, dy, blur, rgba[0], rgba[1], rgba[2], rgba[3]];
    (numbers.iter().all(|number| number.is_finite()) && rgba[3] > 0.0).then(|| Shade {
        dx,
        dy,
        blur: blur.clamp(0.0, MOST_BLUR),
        rgba,
    })
}

/// The steps of a path as a shape to draw, in the canvas's points; `None` for one with nothing to
/// draw. A step that is not a number is passed over.
fn shape(segments: &[Segment], hasher: &mut impl Hasher) -> Option<tiny_skia::Path> {
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

fn hash_ink(ink: &Ink, hasher: &mut impl Hasher) {
    let hash_stops = |stops: &[(f32, [f32; 4])], hasher: &mut _| {
        stops.len().hash(hasher);
        for (offset, rgba) in stops {
            offset.to_bits().hash(hasher);
            hash_rgba(*rgba, hasher);
        }
    };
    match ink {
        Ink::Solid(rgba) => {
            0_u8.hash(hasher);
            hash_rgba(*rgba, hasher);
        }
        Ink::Linear { from, to, stops } => {
            1_u8.hash(hasher);
            for number in [from.0, from.1, to.0, to.1] {
                number.to_bits().hash(hasher);
            }
            hash_stops(stops, hasher);
        }
        Ink::Radial {
            center,
            radius,
            stops,
        } => {
            2_u8.hash(hasher);
            for number in [center.0, center.1, *radius] {
                number.to_bits().hash(hasher);
            }
            hash_stops(stops, hasher);
        }
    }
}

fn hash_dash(dash: &Option<(Vec<f32>, f32)>, hasher: &mut impl Hasher) {
    match dash {
        Some((lengths, offset)) => {
            1_u8.hash(hasher);
            lengths.len().hash(hasher);
            for number in lengths.iter().chain(std::iter::once(offset)) {
                number.to_bits().hash(hasher);
            }
        }
        None => 0_u8.hash(hasher),
    }
}

/// The paint of an ink: smoothed at its edges, or, `crisp`, each pixel the path's where its middle
/// is inside it and untouched where it is not; laid on what is under it as `blend` says.
fn painted(ink: &Ink, crisp: bool, blend: BlendMode) -> Paint<'static> {
    let shader = match ink {
        Ink::Solid(rgba) => solid(*rgba),
        Ink::Linear { from, to, stops } => LinearGradient::new(
            tiny_skia::Point::from_xy(from.0, from.1),
            tiny_skia::Point::from_xy(to.0, to.1),
            gradient(stops),
            SpreadMode::Pad,
            Transform::identity(),
        )
        .unwrap_or_else(|| solid(last_of(stops))),
        Ink::Radial {
            center,
            radius,
            stops,
        } => {
            let middle = tiny_skia::Point::from_xy(center.0, center.1);
            RadialGradient::new(
                middle,
                0.0,
                middle,
                *radius,
                gradient(stops),
                SpreadMode::Pad,
                Transform::identity(),
            )
            .unwrap_or_else(|| solid(last_of(stops)))
        }
    };
    Paint {
        shader,
        anti_alias: !crisp,
        blend_mode: blend,
        ..Paint::default()
    }
}

fn last_of(stops: &[(f32, [f32; 4])]) -> [f32; 4] {
    stops.last().map_or([0.0; 4], |(_, rgba)| *rgba)
}

fn clamped(rgba: [f32; 4]) -> [f32; 4] {
    rgba.map(|value| {
        if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            0.0
        }
    })
}

fn solid(rgba: [f32; 4]) -> Shader<'static> {
    let [r, g, b, a] = clamped(rgba);
    Shader::SolidColor(tiny_skia::Color::from_rgba8(
        channel(r),
        channel(g),
        channel(b),
        channel(a),
    ))
}

fn gradient(stops: &[(f32, [f32; 4])]) -> Vec<GradientStop> {
    stops
        .iter()
        .map(|(offset, rgba)| {
            let [r, g, b, a] = clamped(*rgba);
            let color = tiny_skia::Color::from_rgba(r, g, b, a).unwrap_or(tiny_skia::Color::BLACK);
            GradientStop::new(*offset, color)
        })
        .collect()
}

/// The longest a turn of a line comes to, as a multiple of half the line's width: a style sheet's
/// default limit, past which a turn is bevelled.
const MITER_LIMIT: f32 = 4.0;

fn stroked(line: &Line) -> Stroke {
    Stroke {
        width: line.width,
        miter_limit: MITER_LIMIT,
        line_cap: line.cap,
        line_join: line.join,
        dash: line
            .dash
            .as_ref()
            .and_then(|(lengths, offset)| StrokeDash::new(lengths.clone(), *offset)),
    }
}

/// The shadow of `part` drawn into `pixmap`: what the part covers, moved and blurred as the shade
/// says, in the colour of the shade, laid as the part is. Worked out in the room it reaches and no
/// more (the part's box moved, and the blur past it), cut at the picture's edges and the blur's reach
/// past them: a shape half off the picture casts as much of a shadow on it as a whole one.
fn cast(
    pixmap: &mut Pixmap,
    part: &Part,
    shade: &Shade,
    (transform, scale): (Transform, f32),
    room: &mut Room,
) {
    let sigma = shade.blur * scale * 0.5;
    let reach = (sigma * 3.0).ceil() + 1.0;
    let (dx, dy) = (shade.dx * scale, shade.dy * scale);
    let Some(bounds) = part.shape.bounds().transform(transform) else {
        return;
    };
    // A line reaches past the shape by half its width, which the transform may stretch, and at a
    // turn or an end by more.
    let grown = part.line.as_ref().map_or(1.0, |line| {
        let stretch = (transform.sx * transform.sx
            + transform.kx * transform.kx
            + transform.ky * transform.ky
            + transform.sy * transform.sy)
            .sqrt();
        let beyond = if line.join == LineJoin::Miter {
            MITER_LIMIT
        } else if line.cap == LineCap::Square {
            std::f32::consts::SQRT_2
        } else {
            1.0
        };
        line.width * 0.5 * beyond * stretch + 1.0
    });
    #[allow(clippy::cast_precision_loss)]
    let (wide, high) = (pixmap.width() as f32, pixmap.height() as f32);
    let left = (bounds.left() - grown + dx - reach).max(-reach).floor();
    let top = (bounds.top() - grown + dy - reach).max(-reach).floor();
    let right = (bounds.right() + grown + dx + reach)
        .min(wide + reach)
        .ceil();
    let bottom = (bounds.bottom() + grown + dy + reach)
        .min(high + reach)
        .ceil();
    if !(right > left && bottom > top) {
        return;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (across, down) = ((right - left) as u32, (bottom - top) as u32);
    let Some(mut layer) = Pixmap::new(across, down) else {
        return;
    };
    // The coverage: the part in black, where the shadow lies.
    let to_layer = transform.post_translate(dx - left, dy - top);
    let mut black = Paint::default();
    black.set_color_rgba8(0, 0, 0, 255);
    black.anti_alias = !part.crisp;
    if part.fill.is_some() {
        layer.fill_path(&part.shape, &black, FillRule::EvenOdd, to_layer, None);
    }
    if let Some(line) = &part.line {
        layer.stroke_path(&part.shape, &black, &stroked(line), to_layer, None);
    }
    let Room { grid, blur } = room;
    grid.clear();
    grid.extend(
        layer
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| f32::from(pixel[3]) / 255.0),
    );
    if sigma > 0.0 {
        blur.coverage(grid, (across as usize, down as usize), sigma);
    }
    let [r, g, b, a] = clamped(shade.rgba);
    for (pixel, coverage) in layer
        .data_mut()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(grid.iter())
    {
        let alpha = (coverage * a).clamp(0.0, 1.0);
        for (byte, value) in pixel
            .iter_mut()
            .zip([r * alpha, g * alpha, b * alpha, alpha])
        {
            *byte = channel(value);
        }
    }
    #[allow(clippy::cast_possible_truncation)]
    pixmap.draw_pixmap(
        left as i32,
        top as i32,
        layer.as_ref(),
        &PixmapPaint {
            blend_mode: part.blend,
            ..PixmapPaint::default()
        },
        Transform::identity(),
        None,
    );
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn channel(value: f32) -> u8 {
    (value * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use pencere_view::{Blend, Brush, Cap, Join, Matrix, Point, Rect, Role, Source, Stop, path};

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

    /// A host decides by the key whether to draw: it says what the bitmap will be, whatever is drawn,
    /// and is no key where no bitmap comes.
    #[test]
    fn the_key_of_a_figure_is_what_its_bitmap_is_known_by() {
        let ring = path().circle(10.0, 10.0, 6.0).stroke(2.0, Role::Accent);
        let filled = path().rect(1.0, 1.0, 5.0, 5.0).fill(Role::Accent);
        let drawn = figure(vec![ring, filled], 2.0);
        let key = figure_key(&drawn, solid).expect("a key");
        let bitmap = draw_figure(&drawn, solid).expect("drawn");
        assert_eq!(
            (key.id, key.width, key.height),
            (bitmap.id, bitmap.width, bitmap.height)
        );
        // Where it stands is not what it is.
        let mut moved = drawn.clone();
        moved.rect.x += 100.5;
        moved.rect.y += 7.25;
        assert_eq!(figure_key(&moved, solid), Some(key));
        // Nothing drawn is no key, as it is no bitmap.
        let nothing = figure(vec![path().rect(0.0, 0.0, 5.0, 5.0)], 1.0);
        assert_eq!(figure_key(&nothing, solid), None);
        assert!(draw_figure(&nothing, solid).is_none());
        let mut no_room = drawn;
        no_room.rect.h = 0.0;
        assert_eq!(figure_key(&no_room, solid), None);
    }

    /// At a scale where a point is not a whole number of pixels, the edges of a square fall between
    /// pixels: smoothed, those pixels are part colour; crisp, every pixel is the colour or nothing,
    /// and squares that touch leave no seam between them.
    #[test]
    fn a_crisp_path_is_whole_pixels_where_a_smooth_one_is_blurred_at_its_edges() {
        let scale = 2.08;
        let dots = |crisp: bool| {
            (0..4)
                .map(|at| {
                    #[allow(clippy::cast_precision_loss)]
                    let x = 1.0 + 2.0 * at as f32;
                    path()
                        .rect(x, 1.0, 2.0, 2.0)
                        .fill(Role::Accent)
                        .crisp(crisp)
                })
                .collect::<Vec<_>>()
        };
        let partial = |bitmap: &Bitmap| {
            bitmap
                .pixels()
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] != 0 && p[3] != 255)
                .count()
        };
        let smooth = draw_figure(&figure(dots(false), scale), solid).expect("drawn");
        let crisp = draw_figure(&figure(dots(true), scale), solid).expect("drawn");
        assert!(partial(&smooth) > 0, "the smooth edges are blurred");
        assert_eq!(partial(&crisp), 0, "every crisp pixel is whole");
        // The four dots meet: along the row through their middle, from the first's left edge to the
        // last's right edge, no pixel is empty.
        let row = (2.0 * scale) as u32;
        let (from, to) = ((1.0 * scale).ceil() as u32, (9.0 * scale).floor() as u32);
        for x in from..to {
            assert_eq!(pixel(&crisp, x, row), [255, 0, 0, 255], "pixel {x}");
        }
        // Crisp is another bitmap, so that a host holding the smooth one draws again.
        assert_ne!(smooth.id, crisp.id);
        assert_ne!(
            figure_key(&figure(dots(false), scale), solid),
            figure_key(&figure(dots(true), scale), solid)
        );
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

    // --- the effects of a path: gradients, blends, a line's options, a transform, a shadow ---

    /// A colour of each role the tests use is told apart: accent red, success green, foreground blue.
    fn palette(color: Color) -> [f32; 4] {
        color.resolve(|source| match source {
            Source::Role(Role::Accent) => [1.0, 0.0, 0.0, 1.0],
            Source::Role(Role::Success) => [0.0, 1.0, 0.0, 1.0],
            Source::Role(Role::Foreground) => [0.0, 0.0, 1.0, 1.0],
            _ => [1.0, 1.0, 1.0, 1.0],
        })
    }

    fn square(x: f32, y: f32, side: f32) -> pencere_view::Path {
        path().rect(x, y, side, side)
    }

    fn drawn(paths: Vec<pencere_view::Path>, scale: f32) -> Bitmap {
        draw_figure(&figure(paths, scale), palette).expect("drawn")
    }

    fn key(path: pencere_view::Path) -> FigureKey {
        figure_key(&figure(vec![path], 1.0), palette).expect("a key")
    }

    fn red_to_green() -> Vec<Stop> {
        vec![Stop::new(0.0, Role::Accent), Stop::new(1.0, Role::Success)]
    }

    #[test]
    fn a_linear_gradient_is_the_first_colour_at_its_start_and_the_last_at_its_end() {
        let fill = Brush::linear(0.0, 0.0, 20.0, 0.0, red_to_green());
        let bitmap = drawn(vec![square(0.0, 0.0, 20.0).fill_brush(fill)], 1.0);
        let [r, g, ..] = pixel(&bitmap, 0, 10);
        assert!(r > 240 && g < 20, "{r} {g}");
        let [r, g, ..] = pixel(&bitmap, 19, 10);
        assert!(r < 20 && g > 240, "{r} {g}");
        let [r, g, ..] = pixel(&bitmap, 10, 10);
        assert!(
            (100..160).contains(&r) && (100..160).contains(&g),
            "{r} {g}"
        );
    }

    #[test]
    fn a_radial_gradient_is_the_first_colour_at_its_middle_and_the_last_from_its_radius_on() {
        let fill = Brush::radial(10.0, 10.0, 8.0, red_to_green());
        let bitmap = drawn(vec![square(0.0, 0.0, 20.0).fill_brush(fill)], 1.0);
        let [r, g, ..] = pixel(&bitmap, 10, 10);
        assert!(r > 225 && g < 30, "{r} {g}");
        let [r, g, ..] = pixel(&bitmap, 0, 0);
        assert!(r < 5 && g > 250, "past the radius: {r} {g}");
    }

    #[test]
    fn a_gradient_goes_by_the_scale_of_the_page_as_the_path_does() {
        // At scale 2 the middle of the gradient is the pixel (20, 20), and its radius 16 pixels.
        let fill = Brush::radial(10.0, 10.0, 8.0, red_to_green());
        let bitmap = drawn(vec![square(0.0, 0.0, 20.0).fill_brush(fill)], 2.0);
        let [r, g, ..] = pixel(&bitmap, 20, 20);
        assert!(r > 225 && g < 30, "{r} {g}");
        let [r, g, ..] = pixel(&bitmap, 35, 20);
        assert!(r < 20 && g > 235, "{r} {g}");
    }

    #[test]
    fn a_line_is_painted_with_a_gradient_too() {
        let line = path()
            .move_to(0.0, 10.0)
            .line_to(20.0, 10.0)
            .stroke_brush(6.0, Brush::linear(0.0, 0.0, 20.0, 0.0, red_to_green()))
            .cap(Cap::Butt);
        let bitmap = drawn(vec![line], 1.0);
        let [r, g, ..] = pixel(&bitmap, 1, 10);
        assert!(r > 230 && g < 30, "{r} {g}");
        let [r, g, ..] = pixel(&bitmap, 18, 10);
        assert!(r < 30 && g > 230, "{r} {g}");
    }

    #[test]
    fn a_gradient_that_cannot_be_drawn_is_its_last_colour_and_one_with_no_stop_is_nothing() {
        let green = [0, 255, 0, 255];
        let same_points = Brush::linear(5.0, 5.0, 5.0, 5.0, red_to_green());
        let no_radius = Brush::radial(10.0, 10.0, 0.0, red_to_green());
        let not_a_number = Brush::linear(f32::NAN, 0.0, 20.0, 0.0, red_to_green());
        let one_stop = Brush::linear(0.0, 0.0, 20.0, 0.0, [Stop::new(0.3, Role::Success)]);
        for brush in [same_points, no_radius, not_a_number, one_stop] {
            let bitmap = drawn(vec![square(0.0, 0.0, 20.0).fill_brush(brush.clone())], 1.0);
            assert_eq!(pixel(&bitmap, 3, 3), green, "{brush:?}");
        }
        let empty = Brush::linear(0.0, 0.0, 20.0, 0.0, []);
        let nothing = figure(vec![square(0.0, 0.0, 20.0).fill_brush(empty)], 1.0);
        assert!(draw_figure(&nothing, palette).is_none());
    }

    #[test]
    fn stops_out_of_order_or_out_of_range_are_taken_at_the_nearest_end() {
        let stops = vec![
            Stop::new(-4.0, Role::Accent),
            Stop::new(0.5, Role::Success),
            Stop::new(0.2, Role::Foreground),
        ];
        let fill = Brush::linear(0.0, 0.0, 20.0, 0.0, stops);
        let bitmap = drawn(vec![square(0.0, 0.0, 20.0).fill_brush(fill)], 1.0);
        // The third is taken at the second's place, so past it the colour is blue; at the start red.
        let [r, _, b, _] = pixel(&bitmap, 0, 10);
        assert!(r > 240 && b < 20, "{r} {b}");
        assert_eq!(pixel(&bitmap, 18, 10), [0, 0, 255, 255]);
    }

    #[test]
    fn where_two_paths_overlap_add_is_brighter_and_multiply_darker_than_normal() {
        let red = || square(0.0, 0.0, 12.0).fill(Role::Accent);
        let green = |blend: Blend| square(6.0, 6.0, 12.0).fill(Role::Success).blend(blend);
        let at = |blend: Blend| pixel(&drawn(vec![red(), green(blend)], 1.0), 9, 9);
        assert_eq!(at(Blend::Normal), [0, 255, 0, 255]);
        assert_eq!(at(Blend::Add), [255, 255, 0, 255]);
        assert_eq!(at(Blend::Screen), [255, 255, 0, 255]);
        assert_eq!(at(Blend::Multiply), [0, 0, 0, 255]);
    }

    #[test]
    fn a_path_with_nothing_under_it_is_its_colour_in_every_blend() {
        for blend in [Blend::Normal, Blend::Add, Blend::Screen, Blend::Multiply] {
            let bitmap = drawn(
                vec![square(5.0, 5.0, 10.0).fill(Role::Success).blend(blend)],
                1.0,
            );
            assert_eq!(pixel(&bitmap, 10, 10), [0, 255, 0, 255], "{blend:?}");
        }
    }

    #[test]
    fn a_dashed_line_has_gaps_and_a_start_that_moves_them() {
        let line = |offset: f32| {
            path()
                .move_to(0.0, 10.0)
                .line_to(20.0, 10.0)
                .stroke(4.0, Role::Accent)
                .cap(Cap::Butt)
                .dash(vec![4.0, 4.0], offset)
        };
        let bitmap = drawn(vec![line(0.0)], 1.0);
        assert_eq!(pixel(&bitmap, 2, 10), [255, 0, 0, 255]);
        assert_eq!(pixel(&bitmap, 6, 10), [0, 0, 0, 0], "the gap");
        assert_eq!(pixel(&bitmap, 10, 10), [255, 0, 0, 255]);
        let moved = drawn(vec![line(4.0)], 1.0);
        assert_eq!(pixel(&moved, 2, 10), [0, 0, 0, 0], "started in the gap");
        assert_eq!(pixel(&moved, 6, 10), [255, 0, 0, 255]);
    }

    #[test]
    fn a_dash_of_an_odd_number_of_lengths_is_read_twice_and_a_useless_one_is_no_gaps() {
        let line = |pattern: Vec<f32>| {
            path()
                .move_to(0.0, 10.0)
                .line_to(20.0, 10.0)
                .stroke(4.0, Role::Accent)
                .cap(Cap::Butt)
                .dash(pattern, 0.0)
        };
        // 4, 4, 4 is 4 on, 4 off, 4 on, then 4 off, 4 on, 4 off.
        let odd = drawn(vec![line(vec![4.0, 4.0, 4.0])], 1.0);
        assert_eq!(pixel(&odd, 2, 10), [255, 0, 0, 255]);
        assert_eq!(pixel(&odd, 6, 10), [0, 0, 0, 0]);
        assert_eq!(pixel(&odd, 10, 10), [255, 0, 0, 255]);
        assert_eq!(
            pixel(&odd, 14, 10),
            [0, 0, 0, 0],
            "the second round starts with a gap"
        );
        for pattern in [
            vec![],
            vec![0.0, 0.0],
            vec![4.0, -4.0],
            vec![4.0, f32::NAN],
            vec![f32::INFINITY, 4.0],
        ] {
            let solid = drawn(vec![line(pattern.clone())], 1.0);
            assert_eq!(pixel(&solid, 6, 10), [255, 0, 0, 255], "{pattern:?}");
        }
    }

    #[test]
    fn a_butt_end_stops_where_the_line_does_and_a_round_one_goes_on_past_it() {
        let line = |cap: Cap| {
            path()
                .move_to(5.0, 10.0)
                .line_to(15.0, 10.0)
                .stroke(6.0, Role::Accent)
                .cap(cap)
        };
        let at = |cap: Cap| pixel(&drawn(vec![line(cap)], 1.0), 3, 10);
        assert_eq!(at(Cap::Butt), [0, 0, 0, 0]);
        assert_eq!(at(Cap::Round), [255, 0, 0, 255]);
        assert_eq!(at(Cap::Square), [255, 0, 0, 255]);
        // Where only the corner of a square end is: a round one is not.
        let corner = |cap: Cap| pixel(&drawn(vec![line(cap)], 1.0), 2, 7)[3];
        assert_eq!(corner(Cap::Square), 255);
        assert!(
            corner(Cap::Round) < 40,
            "only the edge of the circle is in it"
        );
    }

    #[test]
    fn a_miter_turn_is_sharper_than_a_round_one_and_a_very_sharp_one_is_cut() {
        let corner = |join: Join| {
            path()
                .move_to(4.0, 16.0)
                .line_to(4.0, 4.0)
                .line_to(16.0, 4.0)
                .stroke(6.0, Role::Accent)
                .cap(Cap::Butt)
                .join(join)
        };
        let at = |join: Join| pixel(&drawn(vec![corner(join)], 1.0), 1, 1)[3];
        assert_eq!(at(Join::Miter), 255);
        assert!(at(Join::Round) < 40, "only the edge of the circle is in it");
        assert!(at(Join::Bevel) < 40);
        // A turn that sharp has a point longer than four half widths: it is bevelled instead.
        let sharp = path()
            .move_to(2.0, 10.0)
            .line_to(18.0, 10.0)
            .line_to(2.0, 12.0)
            .stroke(2.0, Role::Accent)
            .cap(Cap::Butt)
            .join(Join::Miter);
        assert_eq!(pixel(&drawn(vec![sharp], 1.0), 19, 10), [0, 0, 0, 0]);
    }

    #[test]
    fn a_transform_moves_the_pixels_and_carries_the_width_of_the_line_with_it() {
        let at = Point::new(0.0, 0.0);
        let moved = square(0.0, 0.0, 4.0)
            .fill(Role::Accent)
            .transform(Matrix::from_parts((1.0, 1.0), 0.0, (10.0, 6.0), at));
        let bitmap = drawn(vec![moved], 1.0);
        assert_eq!(pixel(&bitmap, 12, 8), [255, 0, 0, 255]);
        assert_eq!(pixel(&bitmap, 2, 2), [0, 0, 0, 0]);
        let grown = square(0.0, 0.0, 4.0)
            .fill(Role::Accent)
            .transform(Matrix::from_parts((2.0, 2.0), 0.0, (0.0, 0.0), at));
        assert_eq!(pixel(&drawn(vec![grown], 1.0), 6, 6), [255, 0, 0, 255]);
        // A line 2 points wide, three times taller: 6 points wide where it runs along.
        let line = path()
            .move_to(0.0, 10.0)
            .line_to(20.0, 10.0)
            .stroke(2.0, Role::Accent)
            .cap(Cap::Butt);
        let flat = drawn(vec![line.clone()], 1.0);
        assert_eq!(pixel(&flat, 10, 7), [0, 0, 0, 0]);
        let tall = line.transform(Matrix::from_parts(
            (1.0, 3.0),
            0.0,
            (0.0, 0.0),
            Point::new(0.0, 10.0),
        ));
        assert_eq!(pixel(&drawn(vec![tall], 1.0), 10, 7), [255, 0, 0, 255]);
    }

    #[test]
    fn a_path_turned_by_its_transform_is_where_the_turn_puts_it_and_its_gradient_with_it() {
        // A bar along the top, turned a quarter clockwise about the middle: it is at the right.
        let bar = square(0.0, 0.0, 20.0)
            .transform(Matrix::IDENTITY)
            .fill_brush(Brush::linear(0.0, 0.0, 20.0, 0.0, red_to_green()));
        let turned = |path: pencere_view::Path| {
            path.transform(Matrix::from_parts(
                (1.0, 1.0),
                90.0,
                (0.0, 0.0),
                Point::new(10.0, 10.0),
            ))
        };
        let bitmap = drawn(vec![turned(bar)], 1.0);
        // The gradient ran from left to right; turned, it runs from the top down.
        let [r, g, ..] = pixel(&bitmap, 10, 0);
        assert!(r > 240 && g < 20, "{r} {g}");
        let [r, g, ..] = pixel(&bitmap, 10, 19);
        assert!(r < 20 && g > 240, "{r} {g}");
    }

    #[test]
    fn a_shadow_is_behind_the_path_in_its_colour_moved_the_way_it_says() {
        let dot = |dx: f32, blur: f32| {
            square(8.0, 8.0, 4.0)
                .fill(Role::Accent)
                .shadow(dx, 0.0, blur, Role::Success)
        };
        let hard = drawn(vec![dot(5.0, 0.0)], 1.0);
        assert_eq!(
            pixel(&hard, 10, 10),
            [255, 0, 0, 255],
            "the path is over its shadow"
        );
        assert_eq!(
            pixel(&hard, 15, 10),
            [0, 255, 0, 255],
            "the shadow, moved to the right"
        );
        assert_eq!(pixel(&hard, 4, 10), [0, 0, 0, 0], "and not to the left");
        // Blurred, it is soft: partly there past where a hard one ends.
        let soft = drawn(vec![dot(5.0, 8.0)], 1.0);
        let [_, g, _, a] = pixel(&soft, 15, 10);
        assert!(g > 0 && a > 0 && a < 255, "{g} {a}");
        let [_, _, _, left] = pixel(&soft, 4, 10);
        assert!(left > 0, "the blur goes round the path too");
    }

    #[test]
    fn a_shadow_with_no_offset_is_a_glow_all_round_the_path() {
        let glow = square(8.0, 8.0, 4.0)
            .fill(Role::Accent)
            .shadow(0.0, 0.0, 8.0, Role::Success);
        let bitmap = drawn(vec![glow], 1.0);
        for (x, y) in [(5, 10), (14, 10), (10, 5), (10, 14)] {
            let [_, g, _, a] = pixel(&bitmap, x, y);
            assert!(g > 0 && a > 0, "({x}, {y}): {g} {a}");
        }
        // Farther off it is fainter.
        let [.., near] = pixel(&bitmap, 6, 10);
        let [.., far] = pixel(&bitmap, 1, 10);
        assert!(near > far, "{near} {far}");
    }

    #[test]
    fn a_shadow_is_drawn_in_the_blend_of_its_path_and_one_of_a_shape_off_the_box_is_not_dimmer() {
        let under = square(0.0, 0.0, 20.0).fill(Role::Accent);
        let lit = square(2.0, 8.0, 4.0)
            .fill(Role::Foreground)
            .shadow(12.0, 0.0, 0.0, Role::Success)
            .blend(Blend::Add);
        let bitmap = drawn(vec![under, lit], 1.0);
        assert_eq!(
            pixel(&bitmap, 16, 10),
            [255, 255, 0, 255],
            "green added to red"
        );
        // A shape half off the box: the shadow of its part that is in is as strong as of a whole one.
        let half_off =
            square(-4.0, 8.0, 8.0)
                .fill(Role::Accent)
                .shadow(0.0, 0.0, 10.0, Role::Success);
        let whole = square(6.0, 8.0, 8.0)
            .fill(Role::Accent)
            .shadow(0.0, 0.0, 10.0, Role::Success);
        let [_, _, _, edge] = pixel(&drawn(vec![half_off], 1.0), 6, 12);
        let [_, _, _, inner] = pixel(&drawn(vec![whole], 1.0), 16, 12);
        assert!(edge.abs_diff(inner) <= 2, "{edge} {inner}");
    }

    #[test]
    fn a_shadow_is_in_points_after_the_transform_at_the_scale_of_the_page() {
        let dot = square(8.0, 8.0, 4.0)
            .fill(Role::Accent)
            .shadow(5.0, 0.0, 0.0, Role::Success);
        let bitmap = drawn(vec![dot.clone()], 2.0);
        // 5 points is 10 pixels: the path is the pixels 16..24 and the shadow 26..34.
        assert_eq!(pixel(&bitmap, 30, 20), [0, 255, 0, 255]);
        // Turned a quarter about its middle the shape turns and the offset does not.
        let turned = dot.transform(Matrix::from_parts(
            (1.0, 1.0),
            90.0,
            (0.0, 0.0),
            Point::new(10.0, 10.0),
        ));
        let bitmap = drawn(vec![turned], 1.0);
        assert_eq!(
            pixel(&bitmap, 15, 10),
            [0, 255, 0, 255],
            "still to the right"
        );
    }

    #[test]
    fn what_is_not_a_number_in_a_transform_or_a_shadow_draws_no_more_than_it_can() {
        let mut flat = Matrix::IDENTITY;
        flat.a = f32::NAN;
        let lost = square(5.0, 5.0, 10.0).fill(Role::Accent).transform(flat);
        assert!(draw_figure(&figure(vec![lost], 1.0), palette).is_none());
        let odd = square(5.0, 5.0, 10.0).fill(Role::Accent).shadow(
            f32::NAN,
            f32::INFINITY,
            f32::NAN,
            Role::Success,
        );
        let bitmap = drawn(vec![odd], 1.0);
        assert_eq!(
            pixel(&bitmap, 10, 10),
            [255, 0, 0, 255],
            "the path is drawn without it"
        );
    }

    /// Every field of a path is in what a figure is known by: where one differs, a host that holds
    /// the other's pixels draws again.
    #[test]
    fn every_field_the_effects_add_changes_the_key_of_a_figure() {
        let base = || square(2.0, 2.0, 10.0).fill(Role::Accent);
        let line = || {
            path()
                .move_to(0.0, 10.0)
                .line_to(20.0, 10.0)
                .stroke(2.0, Role::Accent)
        };
        let gradient =
            |stops: Vec<Stop>| base().fill_brush(Brush::linear(0.0, 0.0, 20.0, 0.0, stops));
        let moved = |dx: f32| base().shadow(dx, 0.0, 4.0, Role::Success);
        let keys = [
            key(base()),
            key(gradient(red_to_green())),
            key(gradient(vec![
                Stop::new(0.0, Role::Accent),
                Stop::new(0.5, Role::Success),
            ])),
            key(gradient(vec![
                Stop::new(0.0, Role::Success),
                Stop::new(1.0, Role::Accent),
            ])),
            key(base().fill_brush(Brush::linear(0.0, 0.0, 20.0, 5.0, red_to_green()))),
            key(base().fill_brush(Brush::radial(10.0, 10.0, 8.0, red_to_green()))),
            key(base().fill_brush(Brush::radial(10.0, 10.0, 9.0, red_to_green()))),
            key(base().blend(Blend::Add)),
            key(base().blend(Blend::Screen)),
            key(base().blend(Blend::Multiply)),
            key(moved(1.0)),
            key(moved(2.0)),
            key(base().shadow(1.0, 0.0, 6.0, Role::Success)),
            key(base().shadow(1.0, 0.0, 4.0, Role::Foreground)),
            key(base().shadow(0.0, 1.0, 4.0, Role::Success)),
            key(base().transform(Matrix::from_parts(
                (1.0, 1.0),
                0.0,
                (1.0, 0.0),
                Point::new(0.0, 0.0),
            ))),
            key(base().transform(Matrix::from_parts(
                (1.0, 1.0),
                10.0,
                (0.0, 0.0),
                Point::new(0.0, 0.0),
            ))),
            key(line()),
            key(line().stroke_brush(2.0, Brush::linear(0.0, 0.0, 20.0, 0.0, red_to_green()))),
            key(line().dash(vec![2.0, 2.0], 0.0)),
            key(line().dash(vec![2.0, 3.0], 0.0)),
            key(line().dash(vec![2.0, 2.0], 1.0)),
            key(line().cap(Cap::Butt)),
            key(line().cap(Cap::Square)),
            key(line().join(Join::Miter)),
            key(line().join(Join::Bevel)),
        ];
        for (at, one) in keys.iter().enumerate() {
            for (to, other) in keys.iter().enumerate().skip(at + 1) {
                assert_ne!(one.id, other.id, "{at} and {to} are the same figure");
            }
        }
    }
}
