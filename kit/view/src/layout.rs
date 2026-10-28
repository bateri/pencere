//! Laying a page out: the element tree becomes a Taffy tree (flexbox and grid, in pixels),
//! the host measures the lines of text, and what comes back is every element's box in the
//! page's pixels, ready to be painted ([`Laid::paint`]).

use std::collections::{HashMap, HashSet, VecDeque};

use taffy::geometry::{Line, Point as TPoint, Rect as TRect, Size as TSize};
use taffy::style::{
    AlignItems, AvailableSpace, Dimension, Display as TDisplay, FlexDirection, FlexWrap,
    GridPlacement, GridTemplateComponent, JustifyContent, LengthPercentage, LengthPercentageAuto,
    MaxTrackSizingFunction, MinTrackSizingFunction, Overflow as TOverflow, Position,
    Style as TStyle,
};
use taffy::{NodeId, TaffyTree, compute_leaf_layout};

use crate::edit::EditorOptions;
use crate::element::{FieldKind, Fit, Kind, Meta, Spark, State};
use crate::paint::Quad;
use crate::rich::{Look, RichLine, cut_line, lay_lines};
use crate::style::{
    Anchor, Border, Direction, Display, Gradient, Items, Justify, Length, Overflow, Shadow, Side,
    Style, TextStyle, Track,
};
use crate::text::wrap_lines;
use crate::{
    Align, Color, Command, Element, Family, Font, Id, Key, Measure, Metrics, Mods, Path, Point,
    Rect, Role, Size, Span,
};

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

/// A text of many looks laid out: its parts, the look each comes to over the element's, and the
/// lines they take in the element's box (pieces placed against its left edge, pixels).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Rich {
    /// The element's own look: what a part does not say.
    pub base: TextLine,
    pub spans: Vec<Span>,
    pub looks: Vec<Look>,
    /// The height of each line, pixels.
    pub line_height: f32,
    pub lines: Vec<RichLine>,
}

/// The text an editor's room is in: the face, the size and the colour of the boxes around it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EditorLook {
    pub font: Font,
    pub color: Color,
}

/// What a laid-out element draws besides its box.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Content {
    None,
    /// The room of an editor of the host's: the host draws in it.
    Editor(EditorLook),
    Text(TextLine),
    Rich(Rich),
    Spark(Spark),
    Image(Placed),
    Canvas(Vec<Path>),
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
    pub gradient: Option<Gradient>,
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
    /// How far the box moves its children up and left, pixels.
    pub scroll: (f32, f32),
    /// It is an overlay: placed against the page and drawn over the rest ([`Element::overlay`]).
    pub overlay: bool,
    /// Nothing inside it is drawn outside it (it clips, or it scrolls).
    pub clips: bool,
    /// How far its picture or its canvas is turned, degrees clockwise about its middle: where it
    /// spins ([`crate::Motion::spin`]).
    pub turn: f32,
}

/// A page laid out: every element's box, in the order they are painted.
#[derive(Clone, Debug, PartialEq)]
pub struct Laid {
    pub(crate) nodes: Vec<Node>,
    /// The scroll bars of the boxes that scroll, drawn after what is inside each box (and so under
    /// what covers it): the place of the box in `nodes` and its bars, pixels
    /// ([`crate::Scrolls::place`]).
    pub(crate) bars: Vec<(usize, Vec<Quad>)>,
    /// The page's size, pixels.
    pub size: Size,
    /// Pixels per point.
    pub scale: f32,
    /// The carets of the editors on the page: the editor's place in `nodes`, the caret's name and
    /// its box from the top-left of the editor's content as it is when nothing is scrolled, pixels
    /// ([`crate::Measure::editor`]).
    pub(crate) carets: Vec<(usize, Id, Rect)>,
    /// What the overlays of the page are anchored to.
    pub(crate) anchored: Vec<Id>,
    /// An editor with an overlay anchored to its caret was scrolled since the page was laid out:
    /// the overlay stands where the caret was ([`Laid::wants_layout`]).
    pub(crate) stale: bool,
}

/// What a Taffy leaf needs to be measured.
enum Leaf {
    Text {
        text: String,
        font: Font,
        /// Its words go on in new lines where the box is too narrow for them.
        wrap: bool,
    },
    /// A text of many looks: its parts, the fonts they are in, and whether it wraps.
    Rich {
        spans: Vec<Span>,
        looks: Vec<Look>,
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
    gradient: Option<Gradient>,
    border: Option<Border>,
    radii: [f32; 4],
    shadow: Option<Shadow>,
    /// It is an overlay: a child of the page's box in the layout, whatever holds it.
    overlay: bool,
    anchor: Option<Anchor>,
}

/// An overlay met on the walk down, laid out after the rest as a child of the page: the element,
/// the text it inherits where it is written, and the element that holds it there.
struct Lifted {
    element: Element,
    text: TextStyle,
    parent: usize,
}

/// Lays `root` out on a page of `viewport` pixels. A page is written in points: `scale` is
/// the pixels to a point. `measure` is asked for every line of text.
pub fn layout(mut root: Element, viewport: Size, scale: f32, measure: &mut dyn Measure) -> Laid {
    // The root is exactly the page: the percentages below it are the page's.
    root.style.width = Length::Px(viewport.w / scale);
    root.style.height = Length::Px(viewport.h / scale);
    let mut tree: TaffyTree<Leaf> = TaffyTree::new();
    let mut pending = Vec::new();
    let mut lifted = VecDeque::new();
    let (_, root_id) = build(
        &mut tree,
        &mut pending,
        &mut lifted,
        root,
        None,
        DEFAULT_TEXT,
        scale,
        measure,
    );
    // The overlays, after everything else (so drawn over it), each a child of the page's box: its
    // insets are the page's. An overlay inside an overlay is lifted in its turn.
    while let Some(Lifted {
        element,
        text,
        parent,
    }) = lifted.pop_front()
    {
        let (index, id) = build(
            &mut tree,
            &mut pending,
            &mut lifted,
            element,
            Some(parent),
            text,
            scale,
            measure,
        );
        if let Some(entry) = pending.get_mut(index) {
            entry.overlay = true;
        }
        let _ = tree.add_child(root_id, id);
    }

    let mut cache: HashMap<(Font, String), Metrics> = HashMap::new();
    let space = TSize {
        width: AvailableSpace::Definite(viewport.w),
        height: AvailableSpace::Definite(viewport.h),
    };
    let measured = tree.compute_layout_with_measure(root_id, space, |input, _node, leaf, style| {
        // taffy places a leaf (its padding, border and the sizes its style gives) around the room
        // its content measures, which is all that is the engine's to say. No style of the kit's
        // is a `calc()`, so there is none to resolve.
        compute_leaf_layout(
            input,
            style,
            |_, _| 0.0,
            |known, available| {
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
                    Some(Leaf::Rich { spans, looks, wrap }) => {
                        let budget = if *wrap {
                            known.width.unwrap_or(match available.width {
                                AvailableSpace::Definite(room) => room,
                                AvailableSpace::MinContent => 0.0,
                                AvailableSpace::MaxContent => f32::INFINITY,
                            })
                        } else {
                            f32::INFINITY
                        };
                        let mut width = |font: &Font, run: &str| {
                            cache
                                .entry((*font, run.to_owned()))
                                .or_insert_with(|| measure.measure(font, run))
                                .width
                        };
                        let lines = lay_lines(spans, looks, budget, *wrap, &mut width);
                        let widest = lines.iter().map(|line| line.width).fold(0.0, f32::max);
                        let line_height = rich_line_height(spans, looks, &mut |font, run| {
                            *cache
                                .entry((*font, run.to_owned()))
                                .or_insert_with(|| measure.measure(font, run))
                        });
                        #[allow(clippy::cast_precision_loss)]
                        let height = line_height * lines.len() as f32;
                        Metrics {
                            width: widest,
                            height,
                        }
                    }
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
        )
    });
    if measured.is_err() {
        return Laid {
            nodes: Vec::new(),
            bars: Vec::new(),
            size: viewport,
            scale,
            carets: Vec::new(),
            anchored: Vec::new(),
            stale: false,
        };
    }

    let page = Rect::new(0.0, 0.0, viewport.w, viewport.h);
    let mut nodes: Vec<Node> = Vec::with_capacity(pending.len());
    // The overlays that stand beside an element, by their place in `nodes`.
    let mut anchors: Vec<(usize, Anchor)> = Vec::new();
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
            .map(|(ox, oy, clip, hidden)| {
                // An overlay stands against the page and is cut by nothing that holds it; it is
                // hidden with it all the same.
                if entry.overlay {
                    (0.0, 0.0, page, hidden)
                } else {
                    (ox, oy, clip, hidden)
                }
            })
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
        if let Some(parent) = entry.parent.filter(|_| !entry.overlay) {
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
        if let Some(anchor) = entry.anchor.filter(|_| entry.overlay) {
            anchors.push((nodes.len(), anchor));
        }
        nodes.push(Node {
            rect,
            clip: outer_clip,
            hidden,
            content: entry.content,
            fill: entry.fill,
            gradient: entry.gradient,
            border: entry.border,
            radii: entry.radii,
            shadow: entry.shadow,
            opacity: entry.opacity * held_opacity,
            parent: entry.parent,
            meta: entry.meta,
            extent: (entry.overflow == Overflow::Scroll)
                .then_some((Size::new(0.0, 0.0), Size::new(rect.w, rect.h))),
            scroll: entry.scroll,
            overlay: entry.overlay,
            clips: entry.overflow != Overflow::Visible,
            turn: 0.0,
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
    // An editor's room scrolls over what the host's editor laid out, and its caret is where the
    // editor last put it.
    let mut carets = Vec::new();
    for (index, node) in nodes.iter_mut().enumerate() {
        let (Some(id), Content::Editor(_)) = (node.meta.id, &node.content) else {
            continue;
        };
        let Some(said) = measure.editor(id) else {
            continue;
        };
        if let Some((content, view)) = &mut node.extent {
            let fits = |side: f32| side.is_finite() && side >= 0.0;
            *content = Size::new(
                if fits(said.content.w) {
                    said.content.w
                } else {
                    view.w
                },
                if fits(said.content.h) {
                    said.content.h
                } else {
                    view.h
                },
            );
        }
        if let Some(caret) = said.caret.filter(|r| r.x.is_finite() && r.y.is_finite()) {
            carets.push((index, id.caret(), caret));
        }
    }
    let anchored = anchors.iter().map(|(_, anchor)| anchor.to).collect();
    for (index, anchor) in anchors {
        place_beside(&mut nodes, &carets, index, anchor, page, scale);
    }
    for node in &mut nodes {
        if let Content::Rich(rich) = &mut node.content {
            lay_rich(rich, node.rect.w, measure);
        }
    }
    Laid {
        nodes,
        bars: Vec::new(),
        size: viewport,
        scale,
        carets,
        anchored,
        stale: false,
    }
}

/// Where the caret of an editor is on the page, pixels: its box in the content, moved as the room
/// is scrolled.
fn caret_on_page(nodes: &[Node], (index, _, caret): &(usize, Id, Rect)) -> Option<Rect> {
    let room = nodes.get(*index)?;
    Some(caret.translate(room.rect.x - room.scroll.0, room.rect.y - room.scroll.1))
}

/// The height of a line of a text of many looks: the tallest of its parts' fonts.
fn rich_line_height(
    spans: &[Span],
    looks: &[Look],
    metrics: &mut dyn FnMut(&Font, &str) -> Metrics,
) -> f32 {
    spans
        .iter()
        .zip(looks)
        .map(|(span, look)| {
            let sample = if span.text.trim().is_empty() {
                "x"
            } else {
                span.text.as_str()
            };
            metrics(&look.font, sample).height
        })
        .fold(0.0, f32::max)
}

/// The lines of a text of many looks in a box `width` pixels wide: as many as its words take where
/// it wraps, and where it does not, one for each of its line breaks, cut to fit with "…" where it
/// may be.
fn lay_rich(rich: &mut Rich, width: f32, measure: &mut dyn Measure) {
    let mut cache: HashMap<(Font, String), Metrics> = HashMap::new();
    let mut metrics = |font: &Font, run: &str| {
        *cache
            .entry((*font, run.to_owned()))
            .or_insert_with(|| measure.measure(font, run))
    };
    rich.line_height = rich_line_height(&rich.spans, &rich.looks, &mut metrics);
    let budget = if rich.base.wrap {
        width + 1.0
    } else {
        f32::INFINITY
    };
    let mut lines = lay_lines(
        &rich.spans,
        &rich.looks,
        budget,
        rich.base.wrap,
        &mut |font, run| metrics(font, run).width,
    );
    if rich.base.truncate && !rich.base.wrap {
        for line in &mut lines {
            if line.width > width + 1.0 {
                *line = cut_line(line, &rich.looks, width, measure);
            }
        }
    }
    rich.lines = lines;
}

/// Moves the overlay at `index`, and what is inside it, beside the element its anchor names: on
/// the side asked where the page has the room, on the other where it has not (and there it has),
/// and along that side as far as it takes to stay on the page.
fn place_beside(
    nodes: &mut [Node],
    carets: &[(usize, Id, Rect)],
    index: usize,
    anchor: Anchor,
    page: Rect,
    scale: f32,
) {
    let Some(me) = nodes.get(index).map(|node| node.rect) else {
        return;
    };
    // The element it stands beside: the first by that name that is not inside the overlay itself;
    // or the caret of an editor, where its room is shown.
    let element = nodes
        .iter()
        .enumerate()
        .find(|(at, node)| {
            node.meta.id == Some(anchor.to) && !node.hidden && !is_inside_of(nodes, *at, index)
        })
        .map(|(_, node)| node.rect);
    let caret = || {
        let found = carets.iter().find(|(_, id, _)| *id == anchor.to)?;
        let room = nodes.get(found.0)?;
        if room.hidden {
            return None;
        }
        caret_on_page(nodes, found)
    };
    let Some(to) = element.or_else(caret) else {
        return;
    };
    let gap = anchor.gap * scale;
    // Where it starts along the side of the element, kept on the page.
    let along = |start: f32, length: f32, mine: f32, room: f32| {
        let at = match anchor.align {
            Align::Start => start,
            Align::Center => start + (length - mine) / 2.0,
            Align::End => start + length - mine,
        };
        at.min(room - mine).max(0.0)
    };
    // `before` is where it starts on the side before the element (above it, or to its left) and
    // `after` on the side after it; `wanted_after` is the side asked for.
    let side = |before: f32, after: f32, mine: f32, room: f32, wanted_after: bool| {
        let fits_before = before >= 0.0;
        let fits_after = after + mine <= room;
        if wanted_after {
            if fits_after || !fits_before {
                after
            } else {
                before
            }
        } else if fits_before || !fits_after {
            before
        } else {
            after
        }
    };
    let (x, y) = match anchor.side {
        Side::Below | Side::Above => (
            along(to.x, to.w, me.w, page.w),
            side(
                to.y - gap - me.h,
                to.bottom() + gap,
                me.h,
                page.h,
                anchor.side == Side::Below,
            ),
        ),
        Side::Right | Side::Left => (
            side(
                to.x - gap - me.w,
                to.right() + gap,
                me.w,
                page.w,
                anchor.side == Side::Right,
            ),
            along(to.y, to.h, me.h, page.h),
        ),
    };
    let (dx, dy) = (x - me.x, y - me.y);
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    let mut end = index + 1;
    while end < nodes.len() && is_inside_of(nodes, end, index) {
        end += 1;
    }
    let Some(boxes) = nodes.get_mut(index..end) else {
        return;
    };
    let Some(outer_clip) = boxes.first().map(|node| node.clip) else {
        return;
    };
    for node in boxes {
        node.rect = node.rect.translate(dx, dy);
        // A clip of a box inside it moves with it; the page's does not.
        if node.clip != outer_clip {
            node.clip = node.clip.translate(dx, dy);
        }
    }
}

/// Whether the node at `index` is inside (or is) the one at `outer`.
fn is_inside_of(nodes: &[Node], index: usize, outer: usize) -> bool {
    let mut at = Some(index);
    while let Some(here) = at {
        if here == outer {
            return true;
        }
        at = nodes.get(here).and_then(|node| node.parent);
    }
    false
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
        gradient: None,
        border: None,
        radii: [0.0; 4],
        shadow: None,
        opacity: 1.0,
        parent: None,
        meta: Meta::default(),
        extent: None,
        scroll: (0.0, 0.0),
        overlay: false,
        clips: false,
        turn: 0.0,
    }
}

/// Makes the Taffy node of `element` and of what is inside it; returns the element's place in
/// `pending` (the pre-order the boxes are painted in) and its node.
#[allow(clippy::too_many_arguments)]
fn build(
    tree: &mut TaffyTree<Leaf>,
    pending: &mut Vec<Pending>,
    lifted: &mut VecDeque<Lifted>,
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
    let px = |points: f32| snap(points, scale);
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
        gradient: style.gradient,
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
        overlay: false,
        anchor: style.anchor,
    });

    let mut taffy_style = convert(&style, scale, matches!(kind, Kind::Text(_) | Kind::Rich(_)));
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
        Kind::Rich(spans) => {
            let base = TextLine {
                text: String::new(),
                font,
                color,
                align: text.align.unwrap_or(Align::Start),
                truncate: style.truncate,
                wrap: style.wrap,
                underline: text.underline.unwrap_or(false),
                strike: text.strike.unwrap_or(false),
            };
            let looks: Vec<Look> = spans.iter().map(|span| span.over(&base)).collect();
            let leaf = Leaf::Rich {
                spans: spans.clone(),
                looks: looks.clone(),
                wrap: style.wrap,
            };
            if let Some(entry) = pending.get_mut(index) {
                entry.content = Content::Rich(Rich {
                    base,
                    spans,
                    looks,
                    line_height: 0.0,
                    lines: Vec::new(),
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
        Kind::Canvas(paths) => {
            if let Some(entry) = pending.get_mut(index) {
                entry.content = Content::Canvas(paths);
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
            if let Some(entry) = pending.get_mut(index)
                && entry.meta.editor.is_some()
            {
                entry.content = Content::Editor(EditorLook { font, color });
            }
            let mut ids: Vec<NodeId> = Vec::with_capacity(children.len());
            for child in children {
                if child.style.overlay {
                    // Laid out after the rest, as a child of the page.
                    lifted.push_back(Lifted {
                        element: child,
                        text,
                        parent: index,
                    });
                    continue;
                }
                ids.push(
                    build(
                        tree,
                        pending,
                        lifted,
                        child,
                        Some(index),
                        text,
                        scale,
                        measure,
                    )
                    .1,
                );
            }
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

/// `points` as whole pixels. A page's lengths are points and a pixel is not a whole number of
/// them where the page is drawn larger than its points (a terminal's zoom); a length that is a
/// fraction of a pixel is rounded on its own, so that a box's padding is as many pixels on each
/// side and the boxes inside it stand in its middle, instead of the edges each being rounded
/// where the fractions add up to (two pixels above, three below).
pub(crate) fn snap(points: f32, scale: f32) -> f32 {
    (points * scale).round()
}

fn dimension(length: Length, scale: f32) -> Dimension {
    match length {
        Length::Auto => Dimension::auto(),
        Length::Px(points) => Dimension::length(snap(points, scale)),
        Length::Percent(percent) => Dimension::percent(percent / 100.0),
    }
}

/// A least or a most size, which taffy takes as a length, a share or nothing (`auto`).
fn bound(length: Length, scale: f32) -> LengthPercentageAuto {
    match length {
        Length::Auto => LengthPercentageAuto::auto(),
        Length::Px(points) => LengthPercentageAuto::length(snap(points, scale)),
        Length::Percent(percent) => LengthPercentageAuto::percent(percent / 100.0),
    }
}

fn convert(style: &Style, scale: f32, is_text: bool) -> TStyle {
    let px = |points: f32| snap(points, scale);
    let edge = |points: f32| LengthPercentage::length(snap(points, scale));
    let margin = |points: f32| LengthPercentageAuto::length(snap(points, scale));
    let inset = |value: Option<f32>| value.map_or_else(LengthPercentageAuto::auto, margin);
    let [top, right, bottom, left] = style.padding;
    let [m_top, m_right, m_bottom, m_left] = style.margin;
    let [i_top, i_right, i_bottom, i_left] = style.inset;
    let mut min_size = TSize {
        width: bound(style.min_width, scale),
        height: bound(style.min_height, scale),
    };
    // A line that may be cut gives its room away down to nothing, as a style sheet's
    // `min-width: 0` on an ellipsis would.
    if is_text && style.truncate && style.min_width == Length::Auto {
        min_size.width = LengthPercentageAuto::length(0.0);
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
        position: if style.absolute || style.overlay {
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
            width: bound(style.max_width, scale),
            height: bound(style.max_height, scale),
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
        // Not said is the box's `normal`: stretched across, packed from the start along.
        align_items: style.items.map_or(AlignItems::NORMAL, |items| match items {
            Items::Start => AlignItems::FLEX_START,
            Items::Center => AlignItems::CENTER,
            Items::End => AlignItems::FLEX_END,
            Items::Stretch => AlignItems::STRETCH,
        }),
        justify_content: style
            .justify
            .map_or(JustifyContent::NORMAL, |justify| match justify {
                Justify::Start => JustifyContent::FLEX_START,
                Justify::Center => JustifyContent::CENTER,
                Justify::End => JustifyContent::FLEX_END,
                Justify::Between => JustifyContent::SPACE_BETWEEN,
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
        let s = self.scale;
        let rect = match self.named(id) {
            Some(node) => node.rect,
            None => {
                let caret = self.carets.iter().find(|(_, caret, _)| *caret == id)?;
                caret_on_page(&self.nodes, caret)?
            }
        };
        Some(Rect::new(rect.x / s, rect.y / s, rect.w / s, rect.h / s))
    }

    /// Whether an overlay of the page stands beside `id` (an element, or an editor's caret).
    pub fn anchored_to(&self, id: Id) -> bool {
        self.anchored.contains(&id)
    }

    /// Whether the page is to be laid out again, as it is (its tree, not a new one asked of the
    /// page): an editor was scrolled ([`crate::Scrolls::place`]) that has an overlay anchored to its
    /// caret, which still stands where the caret was.
    pub fn wants_layout(&self) -> bool {
        self.stale
    }

    /// The named elements under `at` that hold one another: the topmost there, then what holds
    /// it, out to the page — where a wheel or a click goes, as an event bubbles in a browser. What
    /// lies under the topmost without holding it (the list an overlay covers) is not there, nor an
    /// element that is `hidden`, or whose place at `at` is cut off by a box around it.
    pub fn ids_at(&self, at: Point) -> Vec<Id> {
        let (x, y) = (at.x * self.scale, at.y * self.scale);
        let under = |node: &Node| !node.hidden && Self::shown(node).contains(x, y);
        let mut held = self.nodes.iter().rposition(under);
        let mut ids = Vec::new();
        while let Some(node) = held.and_then(|index| self.nodes.get(index)) {
            if let Some(id) = node.meta.id.filter(|_| under(node)) {
                ids.push(id);
            }
            held = node.parent;
        }
        ids
    }

    /// The element under `at` that answers the pointer ([`crate::Element::press`],
    /// [`crate::Element::draggable`], a link of a text of many looks): the topmost. One that is
    /// [`crate::Element::disabled`] does not answer, and what is under it does.
    pub fn hit(&self, at: Point) -> Option<Id> {
        let (x, y) = (at.x * self.scale, at.y * self.scale);
        self.nodes.iter().rev().find_map(|node| {
            if node.hidden || node.meta.state.disabled || !Self::shown(node).contains(x, y) {
                return None;
            }
            if let Some(link) = self
                .links_of(node)
                .into_iter()
                .find(|(rect, _)| rect.contains(x, y))
            {
                return Some(link.1);
            }
            (node.meta.pressable || node.meta.draggable)
                .then_some(node.meta.id)
                .flatten()
        })
    }

    /// The element under `at` that can be picked up and moved ([`crate::Element::draggable`]):
    /// the topmost, when nothing that answers the pointer lies over it.
    pub fn drag_source(&self, at: Point) -> Option<Id> {
        let hit = self.hit(at)?;
        self.nodes
            .iter()
            .any(|node| node.meta.id == Some(hit) && node.meta.draggable && !node.hidden)
            .then_some(hit)
    }

    /// The element under `at` that what is dragged can be put down on
    /// ([`crate::Element::drop_target`]): the topmost.
    pub fn drop_target(&self, at: Point) -> Option<Id> {
        let (x, y) = (at.x * self.scale, at.y * self.scale);
        self.nodes
            .iter()
            .rev()
            .find(|node| {
                node.meta.drop_target
                    && !node.hidden
                    && !node.meta.state.disabled
                    && Self::shown(node).contains(x, y)
            })
            .and_then(|node| node.meta.id)
    }

    /// The links of a text of many looks, each with the box of its words (pixels), in order.
    fn links_of(&self, node: &Node) -> Vec<(Rect, Id)> {
        let Content::Rich(rich) = &node.content else {
            return Vec::new();
        };
        rich_boxes(node, rich)
            .into_iter()
            .filter_map(|(rect, piece)| {
                let link = rich.looks.get(piece.span)?.link?;
                Some((rect, link))
            })
            .collect()
    }

    /// What the page can be told to do now ([`crate::Element::command`]): the commands of the
    /// element the keyboard is on ([`crate::Element::focused`]) and of each box around it, out to
    /// the page's own, the nearest first. A command already listed by a nearer box is not listed
    /// again, and keys a nearer one has are not a farther one's.
    pub fn commands(&self) -> Vec<Command> {
        let focused = self
            .nodes
            .iter()
            .position(|node| node.meta.focused && !node.hidden);
        let mut path = Vec::new();
        let mut at = focused.or(if self.nodes.is_empty() { None } else { Some(0) });
        while let Some(here) = at {
            path.push(here);
            at = self.nodes.get(here).and_then(|node| node.parent);
        }
        let mut ids = HashSet::new();
        let mut taken: Vec<crate::Shortcut> = Vec::new();
        let mut out = Vec::new();
        for index in path {
            let Some(node) = self.nodes.get(index) else {
                continue;
            };
            for command in &node.meta.commands {
                if !ids.insert(command.id) {
                    continue;
                }
                let mut command = command.clone();
                // Keys a nearer command has are that one's: this one keeps the rest of its own.
                command
                    .keys
                    .retain(|keys| !taken.iter().any(|k| k.matches(keys.key, keys.mods)));
                taken.extend(command.keys.iter().copied());
                out.push(command);
            }
        }
        out
    }

    /// The command that a key pressed with `mods` does now, if one does and can be done: what the
    /// host tells the page in place of the key ([`crate::Event::Command`]).
    pub fn command_for(&self, key: Key, mods: Mods) -> Option<Id> {
        self.commands()
            .into_iter()
            .find(|command| command.done_by(key, mods))
            .filter(|command| command.enabled)
            .map(|command| command.id)
    }

    /// How large the content of the scrolling box named `id` is, how large the box is, and how far
    /// it is scrolled.
    pub fn scroll_extent(&self, id: Id) -> Option<Extent> {
        self.named(id).and_then(|node| self.extent_of(node))
    }

    fn extent_of(&self, node: &Node) -> Option<Extent> {
        let (content, view) = node.extent?;
        let s = self.scale;
        let mut extent = Extent {
            content: Size::new(content.w / s, content.h / s),
            view: Size::new(view.w / s, view.h / s),
            offset: Point::new(0.0, 0.0),
        };
        // Where the box is pulled past an end, it is at that end.
        extent.offset = Point::new(
            (node.scroll.0 / s).clamp(0.0, extent.max_offset_x()),
            (node.scroll.1 / s).clamp(0.0, extent.max_offset()),
        );
        Some(extent)
    }

    /// The places in the paint order of the boxes that scroll, have a name and are not hidden.
    pub(crate) fn scrollers(&self) -> Vec<usize> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| !node.hidden && node.meta.id.is_some() && node.extent.is_some())
            .map(|(index, _)| index)
            .collect()
    }

    /// The box that scrolls at `index` in the paint order: its name, its extent, and the part of
    /// the page it shows, points.
    pub(crate) fn scroller_at(&self, index: usize) -> Option<(Id, Extent, Rect)> {
        let node = self.nodes.get(index)?;
        let s = self.scale;
        let shown = self.shown_at(index)?;
        Some((
            node.meta.id?,
            self.extent_of(node)?,
            Rect::new(shown.x / s, shown.y / s, shown.w / s, shown.h / s),
        ))
    }

    /// What is drawn over the box that scrolls at `index` where it shows, points: what comes after
    /// it in the paint order and is not inside it (an overlay, wherever it is written, comes after
    /// everything). Its bars answer the pointer only where none of it is, as its elements do
    /// ([`Laid::ids_at`]).
    pub(crate) fn covering(&self, index: usize) -> Vec<Rect> {
        let Some(node) = self.nodes.get(index) else {
            return Vec::new();
        };
        let shown = Self::shown(node);
        let inside = |mut at: usize| loop {
            if at == index {
                return true;
            }
            match self.nodes.get(at).and_then(|node| node.parent) {
                Some(parent) => at = parent,
                None => return false,
            }
        };
        // What is inside it follows it; an overlay written inside it is lifted to the end, a child
        // of the page's box, so all that comes after the first that is not inside it is over it.
        let after = (index + 1..self.nodes.len())
            .find(|at| !inside(*at))
            .unwrap_or(self.nodes.len());
        let s = self.scale;
        self.nodes
            .iter()
            .skip(after)
            .filter(|other| !other.hidden)
            .map(|other| Self::shown(other).intersect(shown))
            .filter(|cut| cut.w > 0.0 && cut.h > 0.0)
            .map(|cut| Rect::new(cut.x / s, cut.y / s, cut.w / s, cut.h / s))
            .collect()
    }

    /// The part of the page the box at `index` shows, pixels: its box, cut by those around it.
    pub(crate) fn shown_at(&self, index: usize) -> Option<Rect> {
        self.nodes.get(index).map(Self::shown)
    }

    /// Scrolls the box at `index` in the paint order to `to` pixels across and down (past an end,
    /// where it is pulled there), without laying the page out again: what is inside it moves, and
    /// what each box inside it lets be drawn is cut again. An overlay written inside it stands
    /// where it was put until the page is laid out again.
    pub(crate) fn rescroll(&mut self, index: usize, to: (f32, f32)) {
        let Some(node) = self.nodes.get_mut(index) else {
            return;
        };
        let (dx, dy) = (node.scroll.0 - to.0, node.scroll.1 - to.1);
        node.scroll = to;
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        let editor = match (&node.content, node.meta.id) {
            (Content::Editor(_), Some(id)) => Some(id),
            _ => None,
        };
        if editor.is_some_and(|id| self.anchored.contains(&id.caret())) {
            self.stale = true;
        }
        // What is inside a box follows it in the paint order, the overlays aside (they come after
        // everything else).
        for at in index + 1..self.subtree_end(index) {
            let parent = self.nodes.get(at).and_then(|node| node.parent);
            let held = parent
                .and_then(|parent| self.nodes.get(parent))
                .map(|parent| {
                    if parent.clips {
                        parent.clip.intersect(parent.rect)
                    } else {
                        parent.clip
                    }
                });
            if let Some(node) = self.nodes.get_mut(at) {
                node.rect.x += dx;
                node.rect.y += dy;
                if let Some(held) = held {
                    node.clip = held;
                }
            }
        }
    }

    /// The place in the paint order just after the box at `index` and what is inside it.
    pub(crate) fn subtree_end(&self, index: usize) -> usize {
        let mut end = index + 1;
        while end < self.nodes.len() && self.is_inside(end, index) {
            end += 1;
        }
        end
    }

    /// The elements the keyboard can be on, in the order they are written (one that is
    /// [`crate::Element::disabled`] is not one of them).
    pub fn focus_order(&self) -> Vec<Id> {
        self.nodes
            .iter()
            .filter(|node| node.meta.focusable && !node.hidden && !node.meta.state.disabled)
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

    /// The editors the host puts on the page ([`crate::Element::editor`]), in the order they are
    /// written: each named, with its options, where its room stands, the text it is in, what it says
    /// while it is empty and what it is to a screen reader — the hidden ones too (a hidden editor
    /// keeps its document: a page with tabs hides the editors of the files not in front). Of two by
    /// the same name, the first is the editor. One with no name is none.
    pub fn editors(&self) -> Vec<EditorRoom> {
        let s = self.scale;
        let mut seen = HashSet::new();
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                let (Some(id), Some(options), Content::Editor(look)) =
                    (node.meta.id, node.meta.editor.as_ref(), &node.content)
                else {
                    return None;
                };
                if !seen.insert(id) {
                    return None;
                }
                Some(EditorRoom {
                    id,
                    options: options.clone(),
                    rect: Rect::new(
                        node.rect.x / s,
                        node.rect.y / s,
                        node.rect.w / s,
                        node.rect.h / s,
                    ),
                    pixels: node.rect,
                    clip: Self::shown(node),
                    hidden: node.hidden,
                    font: look.font,
                    color: look.color,
                    hint: options
                        .hint
                        .clone()
                        .or_else(|| self.hint_of(index).map(str::to_owned)),
                    label: node.meta.label.clone(),
                })
            })
            .collect()
    }

    /// The elements that say something while the pointer rests on them ([`crate::Element::tip`]),
    /// with the part of each that is seen, in points; the innermost last, so a host that looks
    /// for the tip under the pointer takes the last that holds it.
    pub fn tips(&self) -> Vec<(Rect, crate::Tip)> {
        let s = self.scale;
        self.nodes
            .iter()
            .filter(|node| !node.hidden)
            .filter_map(|node| {
                let tip = node.meta.tip.clone()?;
                let seen = Self::shown(node);
                (seen.w > 0.0 && seen.h > 0.0).then(|| {
                    (
                        Rect::new(seen.x / s, seen.y / s, seen.w / s, seen.h / s),
                        tip,
                    )
                })
            })
            .collect()
    }

    /// What the page is to a screen reader, in the order it is written: each element that says what
    /// it is ([`crate::Element::role`]) is that, and each other that answers the pointer is a
    /// button, said by its label or else by the words inside it (which are not said again apart);
    /// each text field is a field, said by its label or else by its hint; each line of text that is
    /// in none of them is said as it is (and each link in it, apart, as a link), and so is an
    /// element that is none of them but has a label (an icon that says what it shows), whose words
    /// inside are not said again. What is hidden, has no room or is cut off altogether by the boxes
    /// around it is not there. Boxes are points.
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
            let editor = node.meta.id.is_some() && matches!(node.content, Content::Editor(_));
            let role = if let Some(kind) = field {
                Some(AccessibleRole::Field(kind))
            } else if editor {
                Some(AccessibleRole::Editor)
            } else if let Some(role) = node.meta.role {
                Some(role)
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
                    AccessibleRole::Editor => node
                        .meta
                        .editor
                        .as_ref()
                        .and_then(|options| options.hint.clone())
                        .or_else(|| self.hint_of(index).map(str::to_owned))
                        .unwrap_or_default(),
                    _ => self.words_in(index),
                });
                items.push(Accessible {
                    role,
                    id: node.meta.id,
                    rect: points(node.rect),
                    words,
                    state: node.meta.state.clone(),
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
                    state: node.meta.state.clone(),
                });
            }
            if let Content::Rich(rich) = &node.content {
                let words = crate::rich::words(&rich.spans);
                if !words.trim().is_empty() {
                    items.push(Accessible {
                        role: AccessibleRole::Text,
                        id: node.meta.id,
                        rect: points(node.rect),
                        words,
                        state: node.meta.state.clone(),
                    });
                }
                // Each link, apart, as a link: the words of all its pieces, and the box of the
                // first.
                let mut said: Vec<Id> = Vec::new();
                for (rect, link) in self.links_of(node) {
                    if said.contains(&link) {
                        continue;
                    }
                    said.push(link);
                    let words = rich
                        .spans
                        .iter()
                        .filter(|span| span.link == Some(link))
                        .map(|span| span.text.as_str())
                        .collect::<String>();
                    items.push(Accessible {
                        role: AccessibleRole::Link,
                        id: Some(link),
                        rect: points(rect),
                        words,
                        state: State::default(),
                    });
                }
            }
        }
        items
    }

    /// The words of the lines of text inside the element at `outer`, in order, a space between.
    fn words_in(&self, outer: usize) -> String {
        let mut words: Vec<String> = Vec::new();
        for (index, node) in self.nodes.iter().enumerate().skip(outer) {
            if !self.is_inside(index, outer) {
                if index > outer {
                    break;
                }
                continue;
            }
            if !node.hidden {
                match &node.content {
                    Content::Text(line) if !line.text.is_empty() => words.push(line.text.clone()),
                    Content::Rich(rich) => words.push(crate::rich::words(&rich.spans)),
                    _ => {}
                }
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
            .filter(|(index, _)| self.is_inside(*index, field) && !self.lifted_from(*index, field))
            .find_map(|(_, node)| match &node.content {
                Content::Text(line) => Some(line.text.as_str()),
                _ => None,
            })
    }

    /// Whether the element at `index`, inside the one at `outer`, is in an overlay written inside
    /// it (and so stands apart from it).
    fn lifted_from(&self, index: usize, outer: usize) -> bool {
        let mut at = Some(index);
        while let Some(here) = at.filter(|here| *here != outer) {
            let Some(node) = self.nodes.get(here) else {
                return false;
            };
            if node.overlay {
                return true;
            }
            at = node.parent;
        }
        false
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

    /// Whether the element at `index` is in the room of a text field or an editor of the host's,
    /// which the host draws over: what is there is the host's, not the page's.
    pub(crate) fn in_text_field(&self, index: usize) -> bool {
        let mut at = Some(index);
        while let Some(here) = at {
            let Some(node) = self.nodes.get(here) else {
                return false;
            };
            if (node.meta.field.is_some() || matches!(node.content, Content::Editor(_)))
                && node.meta.id.is_some()
            {
                return true;
            }
            // An overlay is drawn over the page, not in the room it is written in.
            if node.overlay {
                return false;
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
    /// The element under `at` that can be picked up and moved: the topmost.
    fn drag_source(&self, at: Point) -> Option<Id>;
    /// The element under `at` that what is dragged can be put down on: the topmost.
    fn drop_target(&self, at: Point) -> Option<Id>;

    // The text of the page's editors ([`crate::Element::editor`]): what the host posted of each,
    // of the version the last [`crate::Event::Edit`] said. A page that lives in the host that
    // shows it reads its editors there, and these say none.

    /// The version of the text of the editor named `editor` that is read.
    fn editor_version(&self, _editor: Id) -> Option<u64> {
        None
    }
    /// How many characters it has.
    fn editor_length(&self, _editor: Id) -> Option<usize> {
        None
    }
    /// How many lines it has.
    fn editor_lines(&self, _editor: Id) -> Option<usize> {
        None
    }
    /// The characters `from..to`: none for a range it has not, or of more than a query answers
    /// with ([`crate::edit::limits::REPLY_TEXT_BYTES`]).
    fn editor_text(&self, _editor: Id, _from: usize, _to: usize) -> Option<String> {
        None
    }
    /// Where line `line` starts.
    fn line_start(&self, _editor: Id, _line: usize) -> Option<usize> {
        None
    }
    /// The line the place `at` is on.
    fn line_of(&self, _editor: Id, _at: usize) -> Option<usize> {
        None
    }
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
    fn drag_source(&self, at: Point) -> Option<Id> {
        Laid::drag_source(self, at)
    }
    fn drop_target(&self, at: Point) -> Option<Id> {
        Laid::drop_target(self, at)
    }
}

/// The size of what a scrolling box holds against the box's own, and how far it is scrolled
/// across and down (within its reach), points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Extent {
    pub content: Size,
    pub view: Size,
    pub offset: Point,
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

/// What an element is to a screen reader ([`Laid::accessible`], [`crate::Element::role`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessibleRole {
    /// It answers the pointer: pressing it does what it says.
    Button,
    /// A text field of the host's, of its kind.
    Field(FieldKind),
    /// An editor of text of the host's ([`crate::Element::editor`]).
    Editor,
    /// Words to read.
    Text,
    /// It goes somewhere: a link of a text, or an element that is one.
    Link,
    /// It is checked or not ([`crate::Element::checked`]).
    Checkbox,
    /// It is on or off ([`crate::Element::checked`]): a switch, or a button that stays down.
    Switch,
    /// One of a set of which one is chosen ([`crate::Element::checked`]).
    Radio,
    /// A tab of a set of them; the one in front is [`crate::Element::selected`].
    Tab,
    /// An entry of a menu or a list that is picked from.
    MenuItem,
    /// The title of what follows, of a level from 1 (the page's title) down.
    Heading(u8),
    /// A picture, said by its label.
    Image,
    /// How far something has come ([`crate::Element::value`]).
    Progress,
    /// A value that is set by moving something along a line ([`crate::Element::value`]).
    Slider,
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
    /// Whether it is checked, chosen, open or of no use, and the value it shows.
    pub state: State,
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

/// An editor the host puts on the page ([`Laid::editors`]).
#[derive(Clone, Debug, PartialEq)]
pub struct EditorRoom {
    /// Its name: the id of the element that is its room.
    pub id: Id,
    pub options: EditorOptions,
    /// Where it stands, points.
    pub rect: Rect,
    /// The same, pixels.
    pub pixels: Rect,
    /// The part of the room that shows, pixels: its box, cut by the boxes around it.
    pub clip: Rect,
    /// It, or a box that holds it, is hidden: it keeps its document and draws nothing.
    pub hidden: bool,
    /// The face its text is in (its size in pixels) and the colour, from the boxes around it.
    pub font: Font,
    pub color: Color,
    /// What it says while its document is empty: its options' hint, or the first text inside it.
    pub hint: Option<String>,
    /// What it is to a screen reader, where the page said ([`crate::Element::label`]).
    pub label: Option<String>,
}

/// The pieces of a text of many looks with the box each takes on the page, pixels, in order: the
/// lines one under the other as the lines of a text that wraps are (one in the middle of its box),
/// each where its element's alignment puts it.
pub(crate) fn rich_boxes<'a>(node: &Node, rich: &'a Rich) -> Vec<(Rect, &'a crate::rich::Piece)> {
    #[allow(clippy::cast_precision_loss)]
    let total = rich.line_height * rich.lines.len() as f32;
    let room = (node.rect.h - total) / 2.0;
    let top = node.rect.y + if rich.base.wrap { room.max(0.0) } else { room };
    let mut out = Vec::new();
    for (index, line) in rich.lines.iter().enumerate() {
        let x = match rich.base.align {
            Align::Start => node.rect.x,
            Align::Center => node.rect.x + (node.rect.w - line.width) / 2.0,
            Align::End => node.rect.right() - line.width,
        };
        #[allow(clippy::cast_precision_loss)]
        let y = top + rich.line_height * index as f32;
        for piece in &line.pieces {
            out.push((
                Rect::new(x + piece.x, y, piece.width, rich.line_height),
                piece,
            ));
        }
    }
    out
}
