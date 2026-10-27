//! What a laid-out page draws, as a flat list in painter's order: later items cover earlier
//! ones. Everything is in **pixels** of the page, top-left origin, and colours are still
//! what the page said ([`Color`]): the host resolves them in the pane's theme and turns the items
//! into whatever its renderer draws.

use crate::element::Fit;
use crate::layout::{Content, Laid, Node, Placed, Rich, TextLine, rich_boxes};
use crate::style::{Border, Gradient, Shadow};
use crate::text::wrap_lines;
use crate::{Align, Color, Font, Measure, Path, Point, Rect, Size};

/// A box: an optional fill, a fill that runs from one colour to another over it, a border (drawn
/// inside, over the fills) and a soft shadow (never under the box), cut at `clip`. `radii` are
/// top-left, top-right, bottom-right, bottom-left.
#[derive(Clone, Debug, PartialEq)]
pub struct Quad {
    pub rect: Rect,
    pub radii: [f32; 4],
    pub fill: Option<Color>,
    /// From `from` to `to` along a line at `angle` degrees through the middle of the box, long
    /// enough that its ends are the box's corners, as a style sheet's `linear-gradient`.
    pub gradient: Option<Gradient>,
    pub border: Option<Border>,
    pub shadow: Option<Shadow>,
    pub clip: Rect,
}

/// One line of text: `origin` is the top-left of its line box. Already cut to fit its box
/// (ending in "…" where it did not) — what the host shapes is what to draw.
///
/// `grounds` is what the line is read against: the fills under it, **nearest first** — its own
/// box's, then each box that holds it, outward — up to and including the first that is
/// opaque ([`Color::is_opaque`]), or all of them where none is. Where the last of them is
/// translucent, or there is none, the page's background is under it (the host knows it:
/// `Role::Background` in its theme). They are what the page said, unresolved: a host that keeps
/// text readable resolves them, lays them over each other from the last to the first, and
/// holds `color` to its lowest contrast against that. A fill the line does not stand on (the
/// box that holds it ends before the line does) is still named; the engine does not
/// intersect boxes.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub origin: Point,
    pub text: String,
    pub font: Font,
    pub color: Color,
    pub grounds: Vec<Color>,
    pub clip: Rect,
    /// A line under the words, which the host draws where its font puts one, in `color`.
    pub underline: bool,
    /// A line through the words, likewise.
    pub strike: bool,
}

/// An image: the file of the view's bundle at `src`, drawn into `rect`, the part of it `crop` says.
///
/// The host asks for the pixels of the image by its path and the size it is drawn at
/// ([`Picture::full`]), and draws them as a textured, rounded rectangle: `crop` is what the
/// rectangle shows of the image, `tint` and `opacity` how it is coloured, `radii` how it is
/// rounded. It is cut at `clip`. A box whose image the host did not know when the page was laid
/// out (it was still being decoded, or it is not an image) has a quad if it has a fill and no
/// picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    /// The path of the image in the view's bundle.
    pub src: String,
    /// Where the image is drawn, pixels: the box it is in for `cover`; for `contain` the part of
    /// the box the whole image takes, centred.
    pub rect: Rect,
    /// What `rect` shows of the image, as shares of it: `(0, 0, 1, 1)` is all of it, and `cover`
    /// is the middle of it where the proportions of the box are not its own.
    pub crop: Rect,
    /// Where there is one, the image is a shape and this is its colour (unresolved: the host
    /// resolves it in the pane's theme, as it does any colour): every pixel of the image is this
    /// colour with the image's opacity at that pixel.
    pub tint: Option<Color>,
    /// How much of the image shows, `0.0..=1.0`, on top of its own opacity and the tint's: the
    /// opacity of its element and the boxes that hold it.
    pub opacity: f32,
    /// The corners of `rect`, pixels: top-left, top-right, bottom-right, bottom-left. The corners
    /// the element asked for, taken in where the room that is drawn is smaller than its box.
    pub radii: [f32; 4],
    pub clip: Rect,
}

impl Picture {
    /// The size in pixels the **whole** image comes to when it is drawn as `rect` and `crop` say
    /// (for `cover`, more than `rect`): what a host rasterises a vector image at, and rounds up
    /// to whole pixels. The image is never smaller than a pixel.
    pub fn full(&self) -> Size {
        Size::new(
            (self.rect.w / self.crop.w.max(f32::EPSILON)).max(1.0),
            (self.rect.h / self.crop.h.max(f32::EPSILON)).max(1.0),
        )
    }
}

/// Paths to draw in a box: what a canvas has ([`crate::canvas`]). The paths are in **points** of
/// the box, from its top-left corner; `rect` is the box on the page, pixels, and `scale` the pixels
/// to a point, so a host draws a path's point `(x, y)` at `(rect.x + x * scale, rect.y + y * scale)`
/// and a line `width` points wide `width * scale` pixels wide. The colours are unresolved, with the
/// element's opacity on them. The sandbox crate turns a figure into pixels for every host
/// (`pencere_sandbox::figure`).
#[derive(Clone, Debug, PartialEq)]
pub struct Figure {
    pub rect: Rect,
    pub scale: f32,
    pub paths: Vec<Path>,
    pub clip: Rect,
}

/// One thing the page draws.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Quad(Quad),
    Text(Run),
    Image(Picture),
    Figure(Figure),
}

/// A page's draw list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DrawList {
    pub items: Vec<Paint>,
}

/// How much of a sparkline's colour the area under it is.
const SPARK_AREA: f32 = 0.18;
/// A sparkline holds this many samples across its width: older ones are cut, a shorter
/// history leaves the left of the box empty.
const SPARK_SLOTS: usize = 30;

impl Laid {
    /// The page's draw list. `measure` is asked again for a line that has to be cut to fit.
    pub fn paint(&self, measure: &mut dyn Measure) -> DrawList {
        let mut list = DrawList::default();
        // The bars of a box that scrolls go after what is inside it: by where that ends.
        let mut bars: Vec<(usize, &Vec<Quad>)> = self
            .bars
            .iter()
            .map(|(index, quads)| (self.subtree_end(*index), quads))
            .collect();
        bars.sort_by_key(|(end, _)| std::cmp::Reverse(*end));
        for (index, node) in self.nodes.iter().enumerate() {
            while let Some((_, quads)) = bars.pop_if(|(end, _)| *end <= index) {
                list.items.extend(quads.iter().cloned().map(Paint::Quad));
            }
            if node.hidden || node.rect.w <= 0.0 || node.rect.h <= 0.0 {
                continue;
            }
            // An element that does not show at all draws nothing (and what is inside it shows no
            // more than it does).
            let opacity = node.opacity;
            if opacity <= 0.0 {
                continue;
            }
            let faded = |color: Color| color.faded(opacity);
            if node.fill.is_some()
                || node.gradient.is_some()
                || node.border.is_some()
                || node.shadow.is_some()
            {
                list.items.push(Paint::Quad(Quad {
                    rect: node.rect,
                    radii: node.radii,
                    fill: node.fill.map(faded),
                    gradient: node.gradient.map(|gradient| Gradient {
                        from: faded(gradient.from),
                        to: faded(gradient.to),
                        ..gradient
                    }),
                    border: node.border.map(|border| Border {
                        color: faded(border.color),
                        ..border
                    }),
                    shadow: node.shadow.map(|shadow| Shadow {
                        color: faded(shadow.color),
                        ..shadow
                    }),
                    clip: node.clip,
                }));
            }
            match &node.content {
                Content::None => {}
                // The words in the room of the host's field are what it says while it is empty:
                // the host's field draws them (`Laid::text_field_hint`).
                Content::Text(_) if self.in_text_field(index) => {}
                Content::Text(line) => {
                    for mut run in place_text(node, line, measure) {
                        run.color = faded(run.color);
                        run.grounds = self.grounds_under(index);
                        list.items.push(Paint::Text(run));
                    }
                }
                Content::Rich(_) if self.in_text_field(index) => {}
                Content::Rich(rich) => {
                    let grounds = self.grounds_under(index);
                    paint_rich(&mut list, node, rich, &grounds);
                }
                Content::Image(image) => {
                    if let Some(picture) = place_image(node, image) {
                        list.items.push(Paint::Image(picture));
                    }
                }
                Content::Canvas(paths) => {
                    if !paths.is_empty() {
                        list.items.push(Paint::Figure(Figure {
                            rect: node.rect,
                            scale: self.scale,
                            paths: paths.iter().map(|path| path.faded(opacity)).collect(),
                            clip: node.clip.intersect(node.rect),
                        }));
                    }
                }
                Content::Spark(spark) => {
                    let clip = node.clip.intersect(node.rect);
                    paint_spark(
                        &mut list,
                        node.rect,
                        clip,
                        &spark.values,
                        faded(spark.color),
                        spark.peak,
                        self.scale,
                    );
                }
            }
        }
        while let Some((_, quads)) = bars.pop() {
            list.items.extend(quads.iter().cloned().map(Paint::Quad));
        }
        list
    }

    /// The fills under the element at `index`, nearest first: its own, then those of the boxes
    /// that hold it, until one is opaque (see [`Run::grounds`]); only fills that are drawn.
    fn grounds_under(&self, index: usize) -> Vec<Color> {
        let mut grounds = Vec::new();
        let mut at = Some(index);
        while let Some(node) = at.and_then(|at| self.nodes.get(at)) {
            // A box with no room is not drawn, and so is not a ground.
            let drawn = !node.hidden && node.rect.w > 0.0 && node.rect.h > 0.0;
            // A fill that runs from one colour to another is read as its middle, over the plain
            // fill under it.
            if let Some(gradient) = node.gradient.filter(|_| drawn) {
                let middle = gradient.middle().faded(node.opacity);
                grounds.push(middle);
                if middle.is_opaque() {
                    break;
                }
            }
            if let Some(fill) = node.fill.filter(|_| drawn) {
                let fill = fill.faded(node.opacity);
                grounds.push(fill);
                if fill.is_opaque() {
                    break;
                }
            }
            at = node.parent;
        }
        grounds
    }
}

/// The lines of a text of many looks: for each piece, the ground of its part where it has one and
/// its words, in the look of its part, read against that ground and then `grounds`.
fn paint_rich(list: &mut DrawList, node: &Node, rich: &Rich, grounds: &[Color]) {
    let opacity = node.opacity;
    let clip = if rich.base.truncate {
        node.clip.intersect(node.rect)
    } else {
        node.clip
    };
    for (rect, piece) in rich_boxes(node, rich) {
        let Some(look) = rich.looks.get(piece.span) else {
            continue;
        };
        let mut under = grounds.to_vec();
        if let Some(background) = look.background {
            let background = background.faded(opacity);
            list.items.push(Paint::Quad(Quad {
                rect,
                radii: [0.0; 4],
                fill: Some(background),
                gradient: None,
                border: None,
                shadow: None,
                clip,
            }));
            under.insert(0, background);
        }
        if piece.text.trim().is_empty() {
            continue;
        }
        list.items.push(Paint::Text(Run {
            origin: Point::new(rect.x, rect.y),
            text: piece.text.clone(),
            font: look.font,
            color: look.color.faded(opacity),
            grounds: under,
            clip,
            underline: look.underline,
            strike: look.strike,
        }));
    }
}

/// Where an image stands in its box, and what of it shows; nothing for an image the host did not
/// know, or a box with no room left inside its padding and border.
fn place_image(node: &Node, image: &Placed) -> Option<Picture> {
    let natural = image.natural?;
    let [top, right, bottom, left] = image.inset;
    let room = Rect::new(
        node.rect.x + left,
        node.rect.y + top,
        node.rect.w - left - right,
        node.rect.h - top - bottom,
    );
    if room.w <= 0.0 || room.h <= 0.0 {
        return None;
    }
    let (rect, crop) = match image.fit {
        Fit::Contain => {
            let along = (room.w / natural.w).min(room.h / natural.h);
            let (w, h) = (natural.w * along, natural.h * along);
            (
                Rect::new(
                    room.x + (room.w - w) / 2.0,
                    room.y + (room.h - h) / 2.0,
                    w,
                    h,
                ),
                Rect::new(0.0, 0.0, 1.0, 1.0),
            )
        }
        Fit::Cover => {
            let along = (room.w / natural.w).max(room.h / natural.h);
            let (shown_w, shown_h) = (room.w / (natural.w * along), room.h / (natural.h * along));
            (
                room,
                Rect::new(
                    (1.0 - shown_w) / 2.0,
                    (1.0 - shown_h) / 2.0,
                    shown_w,
                    shown_h,
                ),
            )
        }
    };
    let most = rect.w.min(rect.h) / 2.0;
    Some(Picture {
        src: image.src.clone(),
        rect,
        crop,
        tint: image.tint,
        opacity: node.opacity,
        radii: node.radii.map(|radius| radius.clamp(0.0, most)),
        clip: node.clip,
    })
}

/// Where a line stands in its box, cut where it does not fit and is allowed to be cut; or, for
/// text that wraps, its lines one under the other, in the middle of the box as one line is.
fn place_text(node: &Node, line: &TextLine, measure: &mut dyn Measure) -> Vec<Run> {
    if line.text.is_empty() {
        return Vec::new();
    }
    let whole = measure.measure(&line.font, &line.text);
    if whole.width <= 0.0 {
        // The host draws no words (no proportional text): nothing to place.
        return Vec::new();
    }
    let box_width = node.rect.w;
    let lines: Vec<(String, f32)> = if line.wrap {
        let mut width = |run: &str| measure.measure(&line.font, run).width;
        wrap_lines(&line.text, box_width + 1.0, &mut width)
            .into_iter()
            .map(|text| {
                let width = width(&text);
                (text, width)
            })
            .collect()
    } else if line.truncate && whole.width > box_width + 1.0 {
        match cut(&line.text, &line.font, box_width, measure) {
            Some(cut) => vec![cut],
            None => return Vec::new(),
        }
    } else {
        vec![(line.text.clone(), whole.width)]
    };
    #[allow(clippy::cast_precision_loss)]
    let total = whole.height * lines.len() as f32;
    // One line stands in the middle of its box, whatever its height; lines that do not all fit
    // start at the top, so that the first is the one seen.
    let room = (node.rect.h - total) / 2.0;
    let top = node.rect.y + if line.wrap { room.max(0.0) } else { room };
    // A line that may be cut stays in its box; the rest is the boxes around it.
    let clip = if line.truncate {
        node.clip.intersect(node.rect)
    } else {
        node.clip
    };
    lines
        .into_iter()
        .enumerate()
        .filter(|(_, (text, _))| !text.is_empty())
        .map(|(index, (text, width))| {
            let x = match line.align {
                Align::Start => node.rect.x,
                Align::Center => node.rect.x + (box_width - width) / 2.0,
                Align::End => node.rect.right() - width,
            };
            #[allow(clippy::cast_precision_loss)]
            let y = top + whole.height * index as f32;
            Run {
                origin: Point::new(x, y),
                text,
                font: line.font,
                color: line.color,
                grounds: Vec::new(),
                clip,
                underline: line.underline,
                strike: line.strike,
            }
        })
        .collect()
}

/// The longest start of `text` that fits `budget` pixels with a "…" after it; `None` when not
/// even the "…" fits.
fn cut(text: &str, font: &Font, budget: f32, measure: &mut dyn Measure) -> Option<(String, f32)> {
    const ELLIPSIS: char = '\u{2026}';
    let ends: Vec<usize> = text
        .char_indices()
        .map(|(at, c)| at + c.len_utf8())
        .collect();
    let with = |count: usize| {
        let start = count
            .checked_sub(1)
            .and_then(|last| ends.get(last))
            .copied()
            .unwrap_or(0);
        let mut cut = text.get(..start).unwrap_or_default().trim_end().to_owned();
        cut.push(ELLIPSIS);
        cut
    };
    // `fits` characters are known to fit (0: only the ellipsis, checked last), `over` is a
    // count known not to.
    let (mut fits, mut over) = (0_usize, ends.len());
    let mut best: Option<(String, f32)> = None;
    while fits + 1 < over {
        let middle = fits + (over - fits) / 2;
        let candidate = with(middle);
        let width = measure.measure(font, &candidate).width;
        if width <= budget {
            fits = middle;
            best = Some((candidate, width));
        } else {
            over = middle;
        }
    }
    if best.is_none() {
        let candidate = with(0);
        let width = measure.measure(font, &candidate).width;
        if width <= budget {
            best = Some((candidate, width));
        }
    }
    best
}

/// A sparkline in `rect`: the area under the line softly filled and the line a thin stroke on
/// top, in boxes one point wide — the kit draws boxes, and a line read at every point of its
/// width and drawn as a box per point (as tall as the line climbs or falls there) is the line
/// to the eye at a sparkline's size.
fn paint_spark(
    list: &mut DrawList,
    rect: Rect,
    clip: Rect,
    values: &[f32],
    color: Color,
    peak: Option<f32>,
    scale: f32,
) {
    let kept = values
        .get(values.len().saturating_sub(SPARK_SLOTS)..)
        .unwrap_or_default();
    let Some(last) = kept.len().checked_sub(1) else {
        return;
    };
    let peak = peak
        .unwrap_or_else(|| kept.iter().copied().fold(0.0_f32, f32::max).max(1e-9) * 1.15)
        .max(1e-9);
    let offset = (SPARK_SLOTS - kept.len()) as f32;
    let stroke = (1.1 * scale).round().max(1.0);
    let margin = scale.max(1.0);
    let height = (rect.h - 2.0 * margin).max(0.0);
    let y_at = |v: f32| rect.y + margin + height * (1.0 - v.clamp(0.0, peak) / peak);
    // Where the line is `x` pixels from the box's left: between two samples, a straight line;
    // nothing before the first.
    let line_at = |x: f32| -> Option<f32> {
        let at = x / rect.w * (SPARK_SLOTS - 1) as f32 - offset;
        if at < 0.0 {
            return None;
        }
        let at = at.min(last as f32);
        let (i, along) = (at.floor() as usize, at.fract());
        let (Some(&a), Some(&b)) = (kept.get(i), kept.get((i + 1).min(last))) else {
            return None;
        };
        Some(y_at(a + (b - a) * along))
    };
    let solid = |rect: Rect, color: Color| {
        Paint::Quad(Quad {
            rect,
            radii: [0.0; 4],
            fill: Some(color),
            gradient: None,
            border: None,
            shadow: None,
            clip,
        })
    };
    let area = Color {
        alpha: color.alpha * SPARK_AREA,
        ..color
    };
    let step = scale.round().max(1.0);
    let columns = (rect.w / step).floor() as usize;
    for column in 0..columns {
        let x = column as f32 * step;
        let (here, next) = (line_at(x), line_at(x + step));
        let (y0, y1) = match (here, next) {
            (Some(y0), Some(y1)) => (y0, y1),
            (None, Some(y1)) => (y1, y1),
            _ => continue,
        };
        let left = rect.x + x;
        let mid = (y0 + y1) / 2.0;
        let floor = rect.bottom();
        if floor > mid {
            list.items
                .push(solid(Rect::new(left, mid, step, floor - mid), area));
        }
        // The line: the rise or fall across this column, at least a stroke thick.
        let top = y0.min(y1) - stroke / 2.0;
        let bottom = y0.max(y1) + stroke / 2.0;
        list.items
            .push(solid(Rect::new(left, top, step, bottom - top), color));
    }
}
