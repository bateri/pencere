//! Laying a page out: the element tree becomes a Taffy tree (flexbox and grid, in pixels),
//! the host measures the lines of text, and what comes back is every element's box in the
//! page's pixels, ready to be painted ([`Laid::paint`]).

use std::collections::HashMap;

use taffy::geometry::{Line, Point as TPoint, Rect as TRect, Size as TSize};
use taffy::style::{
    AlignItems, AvailableSpace, Dimension, Display as TDisplay, FlexDirection, FlexWrap,
    GridPlacement, GridTemplateComponent, JustifyContent, LengthPercentage, LengthPercentageAuto,
    MaxTrackSizingFunction, MinTrackSizingFunction, Overflow as TOverflow, Position,
    Style as TStyle,
};
use taffy::{NodeId, TaffyTree};

use crate::element::{FieldKind, Fit, Kind, Meta, Spark};
use crate::style::{
    Border, Direction, Display, Items, Justify, Length, Overflow, Shadow, Style, TextStyle, Track,
};
use crate::text::wrap_lines;
use crate::{Align, Color, Element, Family, Font, Id, Measure, Metrics, Point, Rect, Role, Size};

/// The text a page is written in until a box says otherwise: the terminal's face at 12.5
/// points, regular, in the foreground, at the start of its box.
const DEFAULT_TEXT: TextStyle = TextStyle {
    family: Some(Family::Terminal),
    size: Some(12.5),
    weight: Some(400),
    italic: Some(false),
    underline: Some(false),
    strike: Some(false),
    color: Some(Role::Foreground.solid()),
    align: Some(Align::Start),
};

/// A line of text laid out (or, `wrap`, the lines its words take in its box).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextLine {
    pub text: String,
    pub font: Font,
    pub color: Color,
    pub align: Align,
    pub truncate: bool,
    pub wrap: bool,
    pub underline: bool,
    pub strike: bool,
}

/// An image laid out: what the host said of its size when the page was laid out, so that what
/// is drawn is what the room was made for.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Placed {
    pub src: String,
    pub fit: Fit,
    pub tint: Option<Color>,
    /// The size of the image itself, pixels (a point to a pixel, times the scale); `None` while
    /// the host did not know it.
    pub natural: Option<Size>,
    /// What the element keeps between its box and the image: padding and border, pixels (top,
    /// right, bottom, left).
    pub inset: [f32; 4],
}

/// What a laid-out element draws besides its box.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Content {
    None,
    Text(TextLine),
    Spark(Spark),
    Image(Placed),
}

/// An element after layout: its box on the page, in pixels, and how it is drawn.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Node {
    /// The box, in the page's pixels.
    pub rect: Rect,
    /// What the elements around it let be drawn.
    pub clip: Rect,
    /// A box nothing of which is drawn (it, or an ancestor, is `hidden`).
    pub hidden: bool,
    pub content: Content,
    pub fill: Option<Color>,
    /// In pixels.
    pub border: Option<Border>,
    pub radii: [f32; 4],
    pub shadow: Option<Shadow>,
    /// How much of what it draws shows: its own opacity times those of the boxes that hold it.
    pub opacity: f32,
    /// The element that holds this one (its place in the paint order).
    pub parent: Option<usize>,
    pub meta: Meta,
    /// For a box that scrolls: how large its content is and how large the box is, pixels.
    pub extent: Option<(Size, Size)>,
}

/// A page laid out: every element's box, in the order they are painted.
#[derive(Clone, Debug, PartialEq)]
pub struct Laid {
    pub(crate) nodes: Vec<Node>,
    /// The page's size, pixels.
    pub size: Size,
    /// Pixels per point.
    pub scale: f32,
}

/// What a Taffy leaf needs to be measured.
enum Leaf {
    Text {
        text: String,
        font: Font,
        /// Its words go on in new lines where the box is too narrow for them.
        wrap: bool,
    },
    /// An image, with its size in pixels where the host knew it, and what the box keeps for its
    /// padding and border across and down: a side that is given is the box's, and the picture
    /// has what is left of it.
    Image {
        natural: Option<Size>,
        inset: (f32, f32),
    },
}

/// An element between the walk down (the tree being made) and the walk back (the boxes
/// being read).
struct Pending {
    id: NodeId,
    parent: Option<usize>,
    meta: Meta,
    overflow: Overflow,
    /// Pixels.
    scroll: (f32, f32),
    /// The room inside the box below and to the right of its content, pixels.
    pad_bottom: f32,
    pad_right: f32,
    hidden: bool,
    /// Its own opacity, `0.0..=1.0`.
    opacity: f32,
    content: Content,
    fill: Option<Color>,
    border: Option<Border>,
    radii: [f32; 4],
    shadow: Option<Shadow>,
}

/// Lays `root` out on a page of `viewport` pixels. A page is written in points: `scale` is
/// the pixels to a point. `measure` is asked for every line of text.
pub fn layout(mut root: Element, viewport: Size, scale: f32, measure: &mut dyn Measure) -> Laid {
    // The root is exactly the page: the percentages below it are the page's.
    root.style.width = Length::Px(viewport.w / scale);
    root.style.height = Length::Px(viewport.h / scale);
    let mut tree: TaffyTree<Leaf> = TaffyTree::new();
    let mut pending = Vec::new();
    let (_, root_id) = build(
        &mut tree,
        &mut pending,
        root,
        None,
        DEFAULT_TEXT,
        scale,
        measure,
    );

    let mut cache: HashMap<(Font, String), Metrics> = HashMap::new();
    let space = TSize {
        width: AvailableSpace::Definite(viewport.w),
        height: AvailableSpace::Definite(viewport.h),
    };
    let measured = tree.compute_layout_with_measure(
        root_id,
        space,
        |known, available, _node, leaf, _style| {
            let natural = match leaf {
                Some(Leaf::Text {
                    text,
                    font,
                    wrap: true,
                }) => {
                    // As wide as the box it is given, or as the room there is (none at all when
                    // the layout asks how narrow it can be: the longest word), and as tall as the
                    // lines it then takes.
                    let budget = known.width.unwrap_or(match available.width {
                        AvailableSpace::Definite(room) => room,
                        AvailableSpace::MinContent => 0.0,
                        AvailableSpace::MaxContent => f32::INFINITY,
                    });
                    let mut metrics = |run: &str| {
                        *cache
                            .entry((*font, run.to_owned()))
                            .or_insert_with(|| measure.measure(font, run))
                    };
                    let line_height = metrics(text).height;
                    let lines = wrap_lines(text, budget, &mut |run| metrics(run).width);
                    let widest = lines
                        .iter()
                        .map(|line| metrics(line).width)
                        .fold(0.0, f32::max);
                    #[allow(clippy::cast_precision_loss)]
                    let height = line_height * lines.len() as f32;
                    Metrics {
                        width: widest,
                        height,
                    }
                }
                Some(Leaf::Text { text, font, .. }) => *cache
                    .entry((*font, text.clone()))
                    .or_insert_with(|| measure.measure(font, text)),
                Some(Leaf::Image { natural, inset }) => {
                    // The sides given are the box's; the room measured is the picture's, inside
                    // the padding and the border that the box adds to it.
                    let (width, height) = image_room(
                        known.width.map(|side| (side - inset.0).max(0.0)),
                        known.height.map(|side| (side - inset.1).max(0.0)),
                        *natural,
                    );
                    return TSize { width, height };
                }
                None => Metrics::default(),
            };
            TSize {
                width: known.width.unwrap_or(natural.width),
                height: known.height.unwrap_or(natural.height),
            }
        },
    );
    if measured.is_err() {
        return Laid {
            nodes: Vec::new(),
            size: viewport,
            scale,
        };
    }

    let page = Rect::new(0.0, 0.0, viewport.w, viewport.h);
    let mut nodes: Vec<Node> = Vec::with_capacity(pending.len());
    // Where each element's children are measured from, and the clip they inherit.
    let mut origins: Vec<(f32, f32, Rect, bool)> = Vec::with_capacity(pending.len());
    // How far down and across each box's children reach, where the box does not scroll them:
    // the size of what a scrolling box holds.
    let mut reach: Vec<(f32, f32)> = Vec::with_capacity(pending.len());
    // What each box scrolls its children by (across, down), and the room it keeps to the right
    // of and below them, pixels.
    let mut inside: Vec<[f32; 4]> = Vec::with_capacity(pending.len());
    for entry in pending {
        let Ok(laid) = tree.layout(entry.id) else {
            // A node Taffy does not know: nothing of it (cannot happen — its own ids).
            nodes.push(empty(page));
            origins.push((0.0, 0.0, page, true));
            reach.push((0.0, 0.0));
            inside.push([0.0; 4]);
            continue;
        };
        let (ox, oy, outer_clip, outer_hidden) = entry
            .parent
            .and_then(|parent| origins.get(parent).copied())
            .unwrap_or((0.0, 0.0, page, false));
        let rect = Rect::new(
            ox + laid.location.x,
            oy + laid.location.y,
            laid.size.width,
            laid.size.height,
        );
        let hidden = outer_hidden || entry.hidden;
        let own_clip = if entry.overflow == Overflow::Visible {
            outer_clip
        } else {
            outer_clip.intersect(rect)
        };
        origins.push((
            rect.x - entry.scroll.0,
            rect.y - entry.scroll.1,
            own_clip,
            hidden,
        ));
        if let Some(parent) = entry.parent {
            let scrolled = inside.get(parent).copied().unwrap_or([0.0; 4]);
            let held = (rect.right() + scrolled[0], rect.bottom() + scrolled[1]);
            if let Some(reach) = reach.get_mut(parent) {
                *reach = (reach.0.max(held.0), reach.1.max(held.1));
            }
        }
        reach.push((0.0, 0.0));
        inside.push([
            entry.scroll.0,
            entry.scroll.1,
            entry.pad_right,
            entry.pad_bottom,
        ]);
        let held_opacity = entry
            .parent
            .and_then(|parent| nodes.get(parent))
            .map_or(1.0, |parent| parent.opacity);
        nodes.push(Node {
            rect,
            clip: outer_clip,
            hidden,
            content: entry.content,
            fill: entry.fill,
            border: entry.border,
            radii: entry.radii,
            shadow: entry.shadow,
            opacity: entry.opacity * held_opacity,
            parent: entry.parent,
            meta: entry.meta,
            extent: (entry.overflow == Overflow::Scroll)
                .then_some((Size::new(0.0, 0.0), Size::new(rect.w, rect.h))),
        });
    }
    // `nodes`, `reach` and `inside` get one entry for every element: they are the same length.
    for ((node, reach), inside) in nodes.iter_mut().zip(&reach).zip(&inside) {
        if let Some((content, _)) = &mut node.extent {
            *content = Size::new(
                (reach.0 - node.rect.x).max(0.0) + inside[2],
                (reach.1 - node.rect.y).max(0.0) + inside[3],
            );
        }
    }
    Laid {
        nodes,
        size: viewport,
        scale,
    }
}

/// The room an image takes: the sides its style gives it, and for one that is not given the
/// other's with the proportions of the image; both of them when it gives neither (its own size);
/// nothing of a side while the image is not known and the style has not said it.
fn image_room(width: Option<f32>, height: Option<f32>, natural: Option<Size>) -> (f32, f32) {
    let Some(natural) = natural else {
        return (width.unwrap_or(0.0), height.unwrap_or(0.0));
    };
    match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, w * natural.h / natural.w),
        (None, Some(h)) => (h * natural.w / natural.h, h),
        (None, None) => (natural.w, natural.h),
    }
}

/// The size an image is laid out with, from what the host said of it: nothing unless it is a size
/// (more than nothing, no bigger than a number a layout can be trusted with).
fn natural_size(said: Option<Size>, scale: f32) -> Option<Size> {
    /// More than the images a view may hold are wide (`ImageLimits`), by far; a size past it is
    /// not one.
    const MOST: f32 = 65_536.0;
    let said = said?;
    let fits = |side: f32| side.is_finite() && side > 0.0 && side <= MOST;
    (fits(said.w) && fits(said.h)).then(|| Size::new(said.w * scale, said.h * scale))
}

fn empty(page: Rect) -> Node {
    Node {
        rect: Rect::default(),
        clip: page,
        hidden: true,
        content: Content::None,
        fill: None,
        border: None,
        radii: [0.0; 4],
        shadow: None,
        opacity: 1.0,
        parent: None,
        meta: Meta::default(),
        extent: None,
    }
}

/// Makes the Taffy node of `element` and of what is inside it; returns the element's place in
/// `pending` (the pre-order the boxes are painted in) and its node.
fn build(
    tree: &mut TaffyTree<Leaf>,
    pending: &mut Vec<Pending>,
    element: Element,
    parent: Option<usize>,
    outer: TextStyle,
    scale: f32,
    measure: &mut dyn Measure,
) -> (usize, NodeId) {
    let Element {
        kind,
        style,
        children,
        meta,
    } = element;
    let text = style.text.over(outer);
    let font = Font {
        family: text.family.unwrap_or(Family::Terminal),
        size: text.size.unwrap_or(12.5) * scale,
        weight: text.weight.unwrap_or(400),
        italic: text.italic.unwrap_or(false),
    };
    let color = text.color.unwrap_or(Role::Foreground.solid());

    let index = pending.len();
    let px = |points: f32| points * scale;
    pending.push(Pending {
        id: NodeId::new(0),
        parent,
        meta,
        overflow: style.overflow,
        scroll: (px(style.scroll.0), px(style.scroll.1)),
        pad_bottom: px(style.padding[2]),
        pad_right: px(style.padding[1]),
        hidden: style.display == Display::None,
        opacity: style
            .opacity
            .filter(|opacity| opacity.is_finite())
            .map_or(1.0, |opacity| opacity.clamp(0.0, 1.0)),
        content: Content::None,
        fill: style.fill,
        border: style.border.map(|b| Border {
            width: (b.width * scale).round().max(1.0),
            color: b.color,
        }),
        radii: style.radii.map(px),
        shadow: style.shadow.map(|s| Shadow {
            dx: px(s.dx),
            dy: px(s.dy),
            blur: px(s.blur),
            color: s.color,
        }),
    });

    let mut taffy_style = convert(&style, scale, matches!(kind, Kind::Text(_)));
    let id = match kind {
        Kind::Text(content) => {
            let leaf = Leaf::Text {
                text: content.clone(),
                font,
                wrap: style.wrap,
            };
            if let Some(entry) = pending.get_mut(index) {
                entry.content = Content::Text(TextLine {
                    text: content,
                    font,
                    color,
                    align: text.align.unwrap_or(Align::Start),
                    truncate: style.truncate,
                    wrap: style.wrap,
                    underline: text.underline.unwrap_or(false),
                    strike: text.strike.unwrap_or(false),
                });
            }
            tree.new_leaf_with_context(taffy_style, leaf)
        }
        Kind::Spark(spark) => {
            if let Some(entry) = pending.get_mut(index) {
                entry.content = Content::Spark(spark);
            }
            tree.new_leaf(taffy_style)
        }
        Kind::Image(image) => {
            let natural = natural_size(measure.image(&image.src), scale);
            let [top, right, bottom, left] = style.padding;
            let border = style
                .border
                .map_or(0.0, |b| (b.width * scale).round().max(1.0));
            // The picture is drawn inside the padding and the border, so they are part of the
            // box and the room it leaves is what the picture is measured for: a box that takes
            // its size from the image is the image's, and the border besides. (The border of any
            // other element is drawn over its box and takes no room.)
            let border_room = LengthPercentage::length(border);
            taffy_style.border = TRect {
                top: border_room,
                right: border_room,
                bottom: border_room,
                left: border_room,
            };
            // And the proportions are the box's whatever limits one side: an image held to a
            // largest width keeps its height in proportion. (A ratio is the box's, border and
            // padding included, so it is the picture's only where there are none; with some, the
            // sides the style gives are measured as ever and a limit on one is not followed.)
            let bare = border == 0.0 && [top, right, bottom, left].iter().all(|edge| *edge == 0.0);
            if let (Some(natural), true) = (natural, bare) {
                taffy_style.aspect_ratio = Some(natural.w / natural.h);
            }
            if let Some(entry) = pending.get_mut(index) {
                entry.content = Content::Image(Placed {
                    src: image.src,
                    fit: image.fit,
                    tint: image.tint,
                    natural,
                    inset: [top, right, bottom, left].map(|edge| edge * scale + border),
                });
            }
            let inset = (
                (left + right) * scale + 2.0 * border,
                (top + bottom) * scale + 2.0 * border,
            );
            tree.new_leaf_with_context(taffy_style, Leaf::Image { natural, inset })
        }
        Kind::Box => {
            let ids: Vec<NodeId> = children
                .into_iter()
                .map(|child| build(tree, pending, child, Some(index), text, scale, measure).1)
                .collect();
            tree.new_with_children(taffy_style, &ids)
        }
    };
    // Taffy cannot refuse a node it is given a valid style for; if it ever does, the
    // placeholder id stands and the element lays out as nothing.
    let id = id.unwrap_or(NodeId::new(0));
    if let Some(entry) = pending.get_mut(index) {
        entry.id = id;
    }
    (index, id)
}

fn dimension(length: Length, scale: f32) -> Dimension {
    match length {
        Length::Auto => Dimension::auto(),
        Length::Px(points) => Dimension::length(points * scale),
        Length::Percent(percent) => Dimension::percent(percent / 100.0),
    }
}

fn convert(style: &Style, scale: f32, is_text: bool) -> TStyle {
    let px = |points: f32| points * scale;
    let edge = |points: f32| LengthPercentage::length(points * scale);
    let margin = |points: f32| LengthPercentageAuto::length(points * scale);
    let inset = |value: Option<f32>| value.map_or_else(LengthPercentageAuto::auto, margin);
    let [top, right, bottom, left] = style.padding;
    let [m_top, m_right, m_bottom, m_left] = style.margin;
    let [i_top, i_right, i_bottom, i_left] = style.inset;
    let mut min_size = TSize {
        width: dimension(style.min_width, scale),
        height: dimension(style.min_height, scale),
    };
    // A line that may be cut gives its room away down to nothing, as a style sheet's
    // `min-width: 0` on an ellipsis would.
    if is_text && style.truncate && style.min_width == Length::Auto {
        min_size.width = Dimension::length(0.0);
    }
    let overflow = match style.overflow {
        Overflow::Visible => TOverflow::Visible,
        Overflow::Hidden => TOverflow::Hidden,
        Overflow::Scroll => TOverflow::Scroll,
    };
    TStyle {
        display: match style.display {
            Display::Flex => TDisplay::Flex,
            Display::Grid => TDisplay::Grid,
            Display::None => TDisplay::None,
        },
        flex_direction: match style.direction {
            Direction::Row => FlexDirection::Row,
            Direction::Column => FlexDirection::Column,
        },
        flex_wrap: if style.wrap {
            FlexWrap::Wrap
        } else {
            FlexWrap::NoWrap
        },
        position: if style.absolute {
            Position::Absolute
        } else {
            Position::Relative
        },
        inset: TRect {
            top: inset(i_top),
            right: inset(i_right),
            bottom: inset(i_bottom),
            left: inset(i_left),
        },
        size: TSize {
            width: dimension(style.width, scale),
            height: dimension(style.height, scale),
        },
        min_size,
        max_size: TSize {
            width: dimension(style.max_width, scale),
            height: dimension(style.max_height, scale),
        },
        margin: TRect {
            top: margin(m_top),
            right: margin(m_right),
            bottom: margin(m_bottom),
            left: margin(m_left),
        },
        padding: TRect {
            top: edge(top),
            right: edge(right),
            bottom: edge(bottom),
            left: edge(left),
        },
        gap: TSize {
            width: edge(style.gap.0),
            height: edge(style.gap.1),
        },
        flex_grow: style.grow,
        flex_shrink: style.shrink.unwrap_or(1.0),
        align_items: style.items.map(|items| match items {
            Items::Start => AlignItems::FlexStart,
            Items::Center => AlignItems::Center,
            Items::End => AlignItems::FlexEnd,
            Items::Stretch => AlignItems::Stretch,
        }),
        justify_content: style.justify.map(|justify| match justify {
            Justify::Start => JustifyContent::FlexStart,
            Justify::Center => JustifyContent::Center,
            Justify::End => JustifyContent::FlexEnd,
            Justify::Between => JustifyContent::SpaceBetween,
        }),
        grid_template_columns: style.columns.iter().map(|t| track(*t, px)).collect(),
        grid_template_rows: style.rows.iter().map(|t| track(*t, px)).collect(),
        grid_column: Line {
            start: GridPlacement::Auto,
            end: if style.column_span > 1 {
                GridPlacement::Span(style.column_span)
            } else {
                GridPlacement::Auto
            },
        },
        overflow: TPoint {
            x: overflow,
            y: overflow,
        },
        scrollbar_width: 0.0,
        ..TStyle::default()
    }
}

fn track(track: Track, px: impl Fn(f32) -> f32) -> GridTemplateComponent<String> {
    let (min, max) = match track {
        Track::Px(points) => (
            MinTrackSizingFunction::length(px(points)),
            MaxTrackSizingFunction::length(px(points)),
        ),
        Track::Fr(share) => (
            MinTrackSizingFunction::auto(),
            MaxTrackSizingFunction::fr(share),
        ),
        Track::Range { min, fr } => (
            MinTrackSizingFunction::length(px(min)),
            MaxTrackSizingFunction::fr(fr),
        ),
        Track::Auto => (
            MinTrackSizingFunction::auto(),
            MaxTrackSizingFunction::auto(),
        ),
    };
    GridTemplateComponent::Single(taffy::MinMax { min, max })
}

impl Laid {
    /// The number of elements on the page.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The boxes of the elements in paint order, pixels — what a test or a host that has to
    /// know where things stand reads.
    pub fn rects(&self) -> impl Iterator<Item = Rect> + '_ {
        self.nodes.iter().map(|node| node.rect)
    }

    // --- Asking the page where things are ------------------------------------------------
    //
    // Everything below speaks in points, the page's own unit: a pointer's place goes in and
    // a box comes out the way the page was written.

    /// Pixels to a point.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The part of `node` that shows, pixels: its box where the boxes around it let it be seen.
    fn shown(node: &Node) -> Rect {
        node.rect.intersect(node.clip)
    }

    fn named(&self, id: Id) -> Option<&Node> {
        self.nodes.iter().find(|node| node.meta.id == Some(id))
    }

    /// The box of the element named `id`, points (where it is on the page now, whatever of it
    /// is cut off by a box around it).
    pub fn bounds(&self, id: Id) -> Option<Rect> {
        let node = self.named(id)?;
        let s = self.scale;
        Some(Rect::new(
            node.rect.x / s,
            node.rect.y / s,
            node.rect.w / s,
            node.rect.h / s,
        ))
    }

    /// The named elements whose visible part holds `at`, the innermost first. An element that is
    /// `hidden`, or whose place is cut off by a box around it, is not there.
    pub fn ids_at(&self, at: Point) -> Vec<Id> {
        let (x, y) = (at.x * self.scale, at.y * self.scale);
        self.nodes
            .iter()
            .rev()
            .filter(|node| !node.hidden && Self::shown(node).contains(x, y))
            .filter_map(|node| node.meta.id)
            .collect()
    }

    /// The element under `at` that answers the pointer ([`crate::Element::press`]): the topmost.
    pub fn hit(&self, at: Point) -> Option<Id> {
        let (x, y) = (at.x * self.scale, at.y * self.scale);
        self.nodes
            .iter()
            .rev()
            .find(|node| node.meta.pressable && !node.hidden && Self::shown(node).contains(x, y))
            .and_then(|node| node.meta.id)
    }

    /// How large the content of the scrolling box named `id` is and how large the box is.
    pub fn scroll_extent(&self, id: Id) -> Option<Extent> {
        let (content, view) = self.named(id)?.extent?;
        let s = self.scale;
        Some(Extent {
            content: Size::new(content.w / s, content.h / s),
            view: Size::new(view.w / s, view.h / s),
        })
    }

    /// The elements the keyboard can be on, in the order they are written.
    pub fn focus_order(&self) -> Vec<Id> {
        self.nodes
            .iter()
            .filter(|node| node.meta.focusable && !node.hidden)
            .filter_map(|node| node.meta.id)
            .collect()
    }

    /// The text fields the host puts over the page ([`crate::Element::field`]), in the order they
    /// are written: each named, of its kind, where it stands (points), what it says while it is
    /// empty, and what it is to a screen reader. An element that says it is a field and has no
    /// name, or is hidden, is not one.
    pub fn fields(&self) -> Vec<Field> {
        let s = self.scale;
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| !node.hidden)
            .filter_map(|(index, node)| {
                let (Some(id), Some(kind)) = (node.meta.id, node.meta.field) else {
                    return None;
                };
                Some(Field {
                    id,
                    kind,
                    rect: Rect::new(
                        node.rect.x / s,
                        node.rect.y / s,
                        node.rect.w / s,
                        node.rect.h / s,
                    ),
                    hint: self.hint_of(index).map(str::to_owned),
                    label: node.meta.label.clone(),
                })
            })
            .collect()
    }

    /// What the page is to a screen reader, in the order it is written: each element that answers
    /// the pointer is a button, said by its label or else by the words inside it (which are not
    /// said again apart); each text field is a field, said by its label or else by its hint; each
    /// line of text that is in neither is said as it is, and so is an element that is neither but
    /// has a label (an icon that says what it shows), whose words inside are not said again. What
    /// is hidden, has no room or is cut off altogether by the boxes around it is not there.
    /// Boxes are points.
    pub fn accessible(&self) -> Vec<Accessible> {
        let s = self.scale;
        let points = |rect: Rect| Rect::new(rect.x / s, rect.y / s, rect.w / s, rect.h / s);
        let shown = |node: &Node| {
            !node.hidden && node.rect.w > 0.0 && node.rect.h > 0.0 && {
                let seen = Self::shown(node);
                seen.w > 0.0 && seen.h > 0.0
            }
        };
        let mut items = Vec::new();
        // The element whose words are said for it (a button, a field, a labelled box): what is
        // inside it is not said apart.
        let mut said_by: Option<usize> = None;
        for (index, node) in self.nodes.iter().enumerate() {
            if said_by.is_some_and(|outer| self.is_inside(index, outer)) {
                continue;
            }
            said_by = None;
            if !shown(node) {
                continue;
            }
            let field = node.meta.field.filter(|_| node.meta.id.is_some());
            let role = if let Some(kind) = field {
                Some(AccessibleRole::Field(kind))
            } else if node.meta.pressable {
                Some(AccessibleRole::Button)
            } else if node.meta.label.is_some() {
                Some(AccessibleRole::Text)
            } else {
                None
            };
            if let Some(role) = role {
                said_by = Some(index);
                let words = node.meta.label.clone().unwrap_or_else(|| match role {
                    AccessibleRole::Field(_) => self.hint_of(index).unwrap_or_default().to_owned(),
                    _ => self.words_in(index),
                });
                items.push(Accessible {
                    role,
                    id: node.meta.id,
                    rect: points(node.rect),
                    words,
                });
                continue;
            }
            if let Content::Text(line) = &node.content
                && !line.text.is_empty()
            {
                items.push(Accessible {
                    role: AccessibleRole::Text,
                    id: node.meta.id,
                    rect: points(node.rect),
                    words: line.text.clone(),
                });
            }
        }
        items
    }

    /// The words of the lines of text inside the element at `outer`, in order, a space between.
    fn words_in(&self, outer: usize) -> String {
        let mut words: Vec<&str> = Vec::new();
        for (index, node) in self.nodes.iter().enumerate().skip(outer) {
            if !self.is_inside(index, outer) {
                if index > outer {
                    break;
                }
                continue;
            }
            if let Content::Text(line) = &node.content
                && !node.hidden
                && !line.text.is_empty()
            {
                words.push(&line.text);
            }
        }
        words.join(" ")
    }

    /// What the field at `index` says while it is empty: the words of the first text inside it
    /// (`div().text_field(id).child(text("Filter by name…"))`). The page does not draw them; the
    /// host's field does.
    fn hint_of(&self, field: usize) -> Option<&str> {
        self.nodes
            .iter()
            .enumerate()
            .skip(field + 1)
            .filter(|(index, _)| self.is_inside(*index, field))
            .find_map(|(_, node)| match &node.content {
                Content::Text(line) => Some(line.text.as_str()),
                _ => None,
            })
    }

    /// Whether the element at `index` is inside (or is) the one at `outer`.
    pub(crate) fn is_inside(&self, index: usize, outer: usize) -> bool {
        let mut at = Some(index);
        while let Some(here) = at {
            if here == outer {
                return true;
            }
            at = self.nodes.get(here).and_then(|node| node.parent);
        }
        false
    }

    /// Whether the element at `index` is in the room of a text field of the host's, which the
    /// host draws over: what is there is the field's, not the page's.
    pub(crate) fn in_text_field(&self, index: usize) -> bool {
        let mut at = Some(index);
        while let Some(here) = at {
            let Some(node) = self.nodes.get(here) else {
                return false;
            };
            if node.meta.field.is_some() && node.meta.id.is_some() {
                return true;
            }
            at = node.parent;
        }
        false
    }

    /// `id` and the named elements that hold it, innermost first: where a key goes, from the
    /// element the keyboard is on out to the boxes around it.
    pub fn path_of(&self, id: Id) -> Vec<Id> {
        let mut path = Vec::new();
        let mut at = self.nodes.iter().position(|node| node.meta.id == Some(id));
        while let Some(index) = at {
            let Some(node) = self.nodes.get(index) else {
                break;
            };
            if let Some(named) = node.meta.id {
                path.push(named);
            }
            at = node.parent;
        }
        path
    }
}

/// What a page asks of itself laid out: where things stand, what is under the pointer, how far
/// a box can scroll. [`Laid`] answers it, and so does anything that can reach one — a page kept
/// elsewhere than the host that laid it out (a module in a sandbox, asking across the border)
/// is written against this and not against [`Laid`].
pub trait Queries {
    /// The box of the element named `id`, points.
    fn bounds(&self, id: Id) -> Option<Rect>;
    /// The named elements whose visible part holds `at`, innermost first.
    fn ids_at(&self, at: Point) -> Vec<Id>;
    /// The element under `at` that answers the pointer: the topmost.
    fn hit(&self, at: Point) -> Option<Id>;
    /// How large the content of the scrolling box named `id` is and how large the box is.
    fn scroll_extent(&self, id: Id) -> Option<Extent>;
    /// The elements the keyboard can be on, in the order they are written.
    fn focus_order(&self) -> Vec<Id>;
    /// `id` and the named elements that hold it, innermost first.
    fn path_of(&self, id: Id) -> Vec<Id>;
}

impl Queries for Laid {
    fn bounds(&self, id: Id) -> Option<Rect> {
        Laid::bounds(self, id)
    }
    fn ids_at(&self, at: Point) -> Vec<Id> {
        Laid::ids_at(self, at)
    }
    fn hit(&self, at: Point) -> Option<Id> {
        Laid::hit(self, at)
    }
    fn scroll_extent(&self, id: Id) -> Option<Extent> {
        Laid::scroll_extent(self, id)
    }
    fn focus_order(&self) -> Vec<Id> {
        Laid::focus_order(self)
    }
    fn path_of(&self, id: Id) -> Vec<Id> {
        Laid::path_of(self, id)
    }
}

/// The size of what a scrolling box holds against the box's own, points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Extent {
    pub content: Size,
    pub view: Size,
}

impl Extent {
    /// The furthest the content can be scrolled down: 0 when it fits.
    pub fn max_offset(self) -> f32 {
        (self.content.h - self.view.h).max(0.0)
    }

    /// The furthest it can be scrolled across: 0 when it fits.
    pub fn max_offset_x(self) -> f32 {
        (self.content.w - self.view.w).max(0.0)
    }
}

/// What an element is to a screen reader ([`Laid::accessible`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessibleRole {
    /// It answers the pointer: pressing it does what it says.
    Button,
    /// A text field of the host's, of its kind.
    Field(FieldKind),
    /// Words to read.
    Text,
}

/// One thing a screen reader says of the page ([`Laid::accessible`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Accessible {
    pub role: AccessibleRole,
    /// The element's name, where it has one (a button's is what a press on it names).
    pub id: Option<Id>,
    /// Where it stands, points.
    pub rect: Rect,
    /// What is said.
    pub words: String,
}

/// A text field the host puts over the page ([`Laid::fields`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    /// Its name: the id of the element that is its room.
    pub id: Id,
    pub kind: FieldKind,
    /// Where it stands, points.
    pub rect: Rect,
    /// What it says while it is empty: the first text inside its room.
    pub hint: Option<String>,
    /// What it is to a screen reader, where the page said ([`crate::Element::label`]).
    pub label: Option<String>,
}
