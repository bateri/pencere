//! What a page laid out says of each of its elements, whether the page named them or not: where it
//! stands in the page's tree, its box, its room inside, what the page wrote of how it is placed, and
//! which of the things a screen reader says it is. A reader of the page that is not the person at
//! the screen (a test, an agent, a panel that shows the boxes) asks [`Laid::inspect`]; what is
//! under a point of the page is [`Laid::inspect_at`].
//!
//! **It reads and keeps nothing.** An element's place is found from what the walk down wrote on
//! each box (its place among the children of the box that holds it), so a page laid out carries two
//! more small numbers per box and nothing is built until it is asked. **No text of the page is in
//! it**: the words an element says are the screen reader's ([`Laid::accessible`]), which a secret
//! field's value is never in.

use std::cmp::Ordering;

use crate::element::{Element, FieldKind};
use crate::layout::{Content, Node};
use crate::style::{Border, Direction, Display, Items, Justify, Length, Overflow, Style, Track};
use crate::{Id, Laid, Point, Rect, Size};

/// What an element is, by what it draws in its box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A box that holds others, or holds nothing.
    Box,
    Text,
    /// A text of many looks.
    Rich,
    Spark,
    Image,
    Canvas,
    /// The room of an editor of the host's.
    Editor,
    /// The room of a browser of the host's.
    Browser,
}

impl Shape {
    /// The word for it that a reading of the page uses.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Box => "box",
            Self::Text => "text",
            Self::Rich => "rich",
            Self::Spark => "spark",
            Self::Image => "image",
            Self::Canvas => "canvas",
            Self::Editor => "editor",
            Self::Browser => "browser",
        }
    }
}

/// What the page wrote of how an element is placed, as it wrote it (before the layout, in points):
/// the half of its style that decides where its box stands and how large it is.
#[derive(Clone, Debug, PartialEq)]
pub struct Placing {
    pub display: Display,
    pub direction: Direction,
    /// A row that is too long goes on in a new line below.
    pub wrap: bool,
    /// Taken out of the flow and placed by `inset` against the box that holds it.
    pub absolute: bool,
    /// Taken out of the flow and out of every box around it ([`crate::Element::overlay`]).
    pub overlay: bool,
    /// Top, right, bottom, left; `None` leaves an edge free.
    pub inset: [Option<f32>; 4],
    pub width: Length,
    pub height: Length,
    pub min_width: Length,
    pub min_height: Length,
    pub max_width: Length,
    pub max_height: Length,
    /// Top, right, bottom, left.
    pub padding: [f32; 4],
    /// Top, right, bottom, left.
    pub margin: [f32; 4],
    /// Between columns, between rows.
    pub gap: (f32, f32),
    pub grow: f32,
    /// `None` is the usual 1.
    pub shrink: Option<f32>,
    pub items: Option<Items>,
    pub justify: Option<Justify>,
    pub columns: Vec<Track>,
    pub rows: Vec<Track>,
    /// How many grid columns it takes (`0` or `1`: one).
    pub column_span: u16,
    pub overflow: Overflow,
}

impl Placing {
    fn of(style: &Style) -> Self {
        Self {
            display: style.display,
            direction: style.direction,
            wrap: style.wrap,
            absolute: style.absolute,
            overlay: style.overlay,
            inset: style.inset,
            width: style.width,
            height: style.height,
            min_width: style.min_width,
            min_height: style.min_height,
            max_width: style.max_width,
            max_height: style.max_height,
            padding: style.padding,
            margin: style.margin,
            gap: style.gap,
            grow: style.grow,
            shrink: style.shrink,
            items: style.items,
            justify: style.justify,
            columns: style.columns.clone(),
            rows: style.rows.clone(),
            column_span: style.column_span,
            overflow: style.overflow,
        }
    }
}

/// An element of the page as it was last laid out ([`Laid::inspect`]). Every length is in points.
#[derive(Clone, Debug, PartialEq)]
pub struct Inspected {
    /// Where it stands in the tree the page wrote: which child it is of the box that holds it, and
    /// of that box in the one that holds it, from the page's root down (the root's own is empty).
    /// An overlay has the place it was written in, not the page's. `None` for what is going out:
    /// it is no part of the tree.
    pub path: Option<Vec<usize>>,
    pub id: Option<Id>,
    pub shape: Shape,
    /// The kind of field the host puts over its box, where it does.
    pub field: Option<FieldKind>,
    /// Its box on the page.
    pub rect: Rect,
    /// What the elements around it let be drawn: what shows of it is `rect` within this.
    pub clip: Rect,
    /// The room inside the box around what is in it: top, right, bottom, left.
    pub padding: [f32; 4],
    /// The border drawn inside the box.
    pub border: Option<Border>,
    /// For a box that scrolls: how far it moves its content up and left.
    pub scroll: (f32, f32),
    /// For a box that scrolls: how large its content is.
    pub content: Option<Size>,
    /// Nothing of it is drawn (it, or an element that holds it, is not shown).
    pub hidden: bool,
    /// It is placed against the page and drawn over the rest.
    pub overlay: bool,
    /// It is going out ([`crate::Motion::exit`]): drawn where it was, fainter, and it has no place
    /// in the tree, no name and no style. The element that holds it is `parent`.
    pub exiting: bool,
    /// The element that holds it, as a place in the list [`Laid::inspect`] gave.
    pub parent: Option<usize>,
    /// What the page wrote of how it is placed; `None` for what is going out, and where the tree
    /// given has no element at its place.
    pub placing: Option<Placing>,
    /// The things a screen reader says that are said for this element, as places in
    /// [`Laid::accessible`]: none for what is not read.
    pub accessible: Vec<usize>,
}

impl Laid {
    /// Every element of the page as it was last laid out, named or not, with the box it stands in
    /// and what the page wrote of how it is placed. They come in the order of the tree the page
    /// wrote (an element, then what is in it, overlays at the place they were written), and what is
    /// going out last. `page` is the tree this was laid out from: where it has no element at a
    /// place, that element's `placing` is `None`.
    pub fn inspect(&self, page: &Element) -> Vec<Inspected> {
        let s = self.scale;
        let points = |rect: Rect| Rect::new(rect.x / s, rect.y / s, rect.w / s, rect.h / s);
        let mut said: Vec<Vec<usize>> = vec![Vec::new(); self.nodes.len()];
        for (item, owner) in self.accessible_owned().1.into_iter().enumerate() {
            if let Some(items) = said.get_mut(owner) {
                items.push(item);
            }
        }
        let paths: Vec<Option<Vec<usize>>> =
            (0..self.nodes.len()).map(|at| self.path_to(at)).collect();
        let mut order: Vec<usize> = (0..self.nodes.len()).collect();
        order.sort_by(|&a, &b| match (paths.get(a), paths.get(b)) {
            (Some(Some(one)), Some(Some(other))) => one.cmp(other),
            (Some(Some(_)), _) => Ordering::Less,
            (_, Some(Some(_))) => Ordering::Greater,
            _ => a.cmp(&b),
        });
        let mut place = vec![0; self.nodes.len()];
        for (listed, &at) in order.iter().enumerate() {
            if let Some(slot) = place.get_mut(at) {
                *slot = listed;
            }
        }
        order
            .into_iter()
            .filter_map(|at| {
                let node = self.nodes.get(at)?;
                let path = paths.get(at).cloned().flatten();
                let placing = path.as_deref().and_then(|path| placing_at(page, path));
                let [left, top, right, bottom] = node.pad;
                Some(Inspected {
                    id: node.meta.id,
                    shape: shape_of(node),
                    field: node.meta.field,
                    rect: points(node.rect),
                    clip: points(node.clip),
                    padding: [top / s, right / s, bottom / s, left / s],
                    border: node.border.map(|border| Border {
                        width: border.width / s,
                        color: border.color,
                    }),
                    scroll: (node.scroll.0 / s, node.scroll.1 / s),
                    content: node
                        .extent
                        .map(|(content, _)| Size::new(content.w / s, content.h / s)),
                    hidden: node.hidden,
                    overlay: node.overlay,
                    exiting: node.exiting,
                    parent: node.parent.and_then(|parent| place.get(parent).copied()),
                    placing,
                    accessible: said.get_mut(at).map(std::mem::take).unwrap_or_default(),
                    path,
                })
            })
            .collect()
    }

    /// The place in the page's tree ([`Inspected::path`]) of the element at `at`, in points: the
    /// topmost one that shows there, named or not. A layer is over what is under it, what a box
    /// that cuts its content shows is where it shows, and a box that is not shown is not found
    /// (what [`Laid::ids_at`] goes by, which names only the ones with a name). `None` where
    /// nothing shows.
    pub fn inspect_at(&self, at: Point) -> Option<Vec<usize>> {
        let (x, y) = (at.x * self.scale, at.y * self.scale);
        let under = |node: &Node| !node.exiting && !node.hidden && Self::shown(node).contains(x, y);
        let top = self.nodes.iter().rposition(under)?;
        self.path_to(top)
    }

    /// The place in the page's tree of the element at `index` of the paint order: found by going up
    /// the elements that hold it, each saying which of its holder's children it is. `None` for
    /// what is going out.
    fn path_to(&self, index: usize) -> Option<Vec<usize>> {
        let mut path = Vec::new();
        let mut at = index;
        // An element is held by one before it in the paint order (an overlay's too): a climb that
        // takes more steps than there are elements is not one.
        for _ in 0..=self.nodes.len() {
            let node = self.nodes.get(at)?;
            if node.exiting {
                return None;
            }
            let Some(holder) = node.parent else {
                path.reverse();
                return Some(path);
            };
            path.push(usize::try_from(node.slot).unwrap_or(usize::MAX));
            at = holder;
        }
        None
    }
}

fn shape_of(node: &Node) -> Shape {
    match node.content {
        Content::None => Shape::Box,
        Content::Editor(_) => Shape::Editor,
        Content::Browser => Shape::Browser,
        Content::Text(_) => Shape::Text,
        Content::Rich(_) => Shape::Rich,
        Content::Spark(_) => Shape::Spark,
        Content::Image(_) => Shape::Image,
        Content::Canvas(_) => Shape::Canvas,
    }
}

/// What the element at `path` of `page` wrote of how it is placed.
fn placing_at(page: &Element, path: &[usize]) -> Option<Placing> {
    let mut element = page;
    for &slot in path {
        element = element.children.get(slot)?;
    }
    Some(Placing::of(&element.style))
}
