//! A box whose size is its laid size, and that moves from the one to the other
//! ([`Motion::auto_size`]).
//!
//! The page is laid out as it is (the sizes the boxes are going to). A box whose size changed from
//! the last frame is on its way, and the page is laid out again with it at the size it has by now.
//! What is inside such a box is not laid out again: every box in it stays as the first layout put
//! it, only moved to where the box is, and the box cuts it where it ends. So what is inside is laid
//! out for the size the box is going to (a line of text does not wrap again as the box goes), and
//! what comes after the box in the page follows its edge.
//!
//! A box inside another that is on its way is laid out for the size the outer one is going to, and is
//! on its way of its own: what is inside the outer box is taken from a layout in which the inner one
//! is at the size it has by now, so that what comes after it follows it.

use super::{Ease, Leg, Motions, Pace, ancestors, content_origin, shift, subtree_end};
use crate::layout::{Laid, Node, layout};
use crate::style::{Length, Overflow};
use crate::{Element, Id, Measure, Rect, Size};

/// What is kept of a box that moves with the size it is laid out at.
#[derive(Clone, Copy, Debug)]
pub(super) struct Fit {
    /// The size it is laid out at, as the page last was: points.
    pub target: (f32, f32),
    /// Where it came from, and the move.
    pub from: (f32, f32),
    pub leg: Option<Leg>,
}

impl Fit {
    pub fn at(size: (f32, f32)) -> Self {
        Self {
            target: size,
            from: size,
            leg: None,
        }
    }

    /// The size it is drawn at `now`, points.
    pub fn size(&self, now: f64) -> (f32, f32) {
        let Some(leg) = self.leg else {
            return self.target;
        };
        (
            (self.from.0 + (self.target.0 - self.from.0) * leg.progress(now, 0)).max(0.0),
            (self.from.1 + (self.target.1 - self.from.1) * leg.progress(now, 1)).max(0.0),
        )
    }

    /// How fast the size goes at `now`, points a second.
    fn speed(&self, now: f64) -> (f32, f32) {
        let Some(leg) = self.leg else {
            return (0.0, 0.0);
        };
        (
            (self.target.0 - self.from.0) * leg.speed(now, 0),
            (self.target.1 - self.from.1) * leg.speed(now, 1),
        )
    }

    /// It is sent to `size` from where it is at `now`: the move begins there, and a spring keeps the
    /// speed it had.
    fn send(&mut self, size: (f32, f32), now: f64, seconds: f32, ease: Ease, bounce: f32) {
        let here = self.size(now);
        let speed = self.speed(now);
        let mut leg = Leg::new(now, seconds, ease, bounce);
        if ease == Ease::Spring {
            leg.rates[0] = rate(speed.0, size.0 - here.0);
            leg.rates[1] = rate(speed.1, size.1 - here.1);
        }
        self.from = here;
        self.target = size;
        self.leg = Some(leg);
    }
}

/// How fast a move of `distance` begins, as a share of the distance a second, to go at `speed`.
fn rate(speed: f32, distance: f32) -> f32 {
    if distance.abs() > 0.01 && speed.is_finite() {
        speed / distance
    } else {
        0.0
    }
}

fn differs(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() > 0.01 || (a.1 - b.1).abs() > 0.01
}

/// A box on its way, as the first layout has it.
struct Going {
    id: Id,
    index: usize,
    /// The size it is drawn at, points.
    size: (f32, f32),
    /// The box on its way that holds it, as its place in the list of these.
    outer: Option<usize>,
}

impl Motions {
    /// The page laid out as it is (`laid`), laid out again for the boxes that are on their way.
    /// `spare` is the tree it was laid out from.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn fit(
        &mut self,
        laid: Laid,
        spare: &Element,
        viewport: Size,
        measure: &mut dyn Measure,
        now: f64,
        still: bool,
        pace: &mut Pace,
    ) -> Laid {
        let scale = laid.scale;
        let mut going: Vec<Going> = Vec::new();
        for (index, node) in laid.nodes.iter().enumerate() {
            let (Some(id), Some(motion)) = (node.meta.id, node.meta.motion) else {
                continue;
            };
            let Some(track) = self.tracks.get_mut(&id) else {
                continue;
            };
            if !motion.auto_size {
                track.fit = None;
                continue;
            }
            let size = (node.rect.w / scale, node.rect.h / scale);
            let fit = track.fit.get_or_insert_with(|| Fit::at(size));
            if still {
                *fit = Fit::at(size);
            } else if differs(fit.target, size) {
                fit.send(size, now, motion.seconds, motion.ease, motion.bounce);
            }
            if fit.leg.is_some_and(|leg| leg.done(now)) {
                *fit = Fit::at(fit.target);
            }
            if fit.leg.is_some() {
                going.push(Going {
                    id,
                    index,
                    size: fit.size(now),
                    outer: None,
                });
            }
        }
        if going.is_empty() {
            return laid;
        }
        pace.finite = true;
        pace.lays = true;
        for at in 0..going.len() {
            let outer = going.get(at).and_then(|inner| {
                ancestors(&laid.nodes, inner.index)
                    .find_map(|parent| going.iter().position(|other| other.index == parent))
            });
            if let Some(inner) = going.get_mut(at) {
                inner.outer = outer;
            }
        }

        // The page with the outermost of the boxes at the size they have; and, for each box that
        // has boxes on their way inside it, what is inside it with those at the size they have.
        let nodes = laid.nodes.len();
        let mut pinned = |held: &dyn Fn(&Going) -> bool| {
            let sizes: Vec<(Id, (f32, f32))> = going
                .iter()
                .filter(|inner| held(inner))
                .map(|inner| (inner.id, inner.size))
                .collect();
            let mut tree = spare.clone();
            pin(&mut tree, &sizes);
            layout(tree, viewport, scale, measure)
        };
        let mut page = pinned(&|inner| inner.outer.is_none());
        let mut sources: Vec<Option<Laid>> = Vec::new();
        for at in 0..going.len() {
            let holds = going.iter().any(|inner| inner.outer == Some(at));
            sources.push(holds.then(|| pinned(&|inner| inner.outer == Some(at))));
        }
        let same = |other: &Laid| other.nodes.len() == nodes;
        if !same(&page) || !sources.iter().flatten().all(same) {
            return laid;
        }

        // What is inside each box is what the layout of the sizes they are going to made of it,
        // moved to where the box is, and cut where it ends.
        let mut inside = vec![false; nodes];
        for (at, inner) in going.iter().enumerate() {
            let source = sources.get(at).and_then(Option::as_ref).unwrap_or(&laid);
            let end = subtree_end(&page.nodes, inner.index);
            let (Some(there), Some(here)) = (
                source.nodes.get(inner.index).cloned(),
                page.nodes.get(inner.index).cloned(),
            ) else {
                continue;
            };
            let (dx, dy) = (here.rect.x - there.rect.x, here.rect.y - there.rect.y);
            let kept = children_clip(&there);
            let cut = here.clip.intersect(here.rect);
            for index in inner.index + 1..end {
                let (Some(node), Some(slot)) = (source.nodes.get(index), page.nodes.get_mut(index))
                else {
                    continue;
                };
                *slot = node.clone();
                slot.rect = slot.rect.translate(dx, dy);
                slot.clip = if slot.clip == kept {
                    cut
                } else {
                    slot.clip.translate(dx, dy).intersect(cut)
                };
                if let Some(held) = inside.get_mut(index) {
                    *held = true;
                }
            }
            if let Some(node) = page.nodes.get_mut(inner.index) {
                node.clips = true;
                if let Some((content, _)) = there.extent {
                    node.extent = Some((content, Size::new(here.rect.w, here.rect.h)));
                }
            }
            let held = inner.index + 1..end;
            page.carets.retain(|(index, ..)| !held.contains(index));
            page.ranges.retain(|(index, ..)| !held.contains(index));
            page.carets.extend(
                source
                    .carets
                    .iter()
                    .filter(|(index, ..)| held.contains(index))
                    .cloned(),
            );
            page.ranges.extend(
                source
                    .ranges
                    .iter()
                    .filter(|(index, ..)| held.contains(index))
                    .cloned(),
            );
        }
        page.carets.sort_by_key(|(index, ..)| *index);
        page.ranges.sort_by_key(|(index, ..)| *index);

        // What slides of its own goes where the page says (against the box that holds it, as the
        // slide is measured), and slides there.
        for index in 0..nodes {
            let Some(node) = page.nodes.get(index) else {
                break;
            };
            if inside.get(index).copied().unwrap_or(false)
                || !node.meta.motion.is_some_and(|motion| motion.place)
                || node.meta.id.is_none()
            {
                continue;
            }
            let Some(target) = laid.nodes.get(index) else {
                continue;
            };
            let held = ancestors(&page.nodes, index).find(|&holder| {
                page.nodes
                    .get(holder)
                    .is_some_and(|node| node.meta.id.is_some())
            });
            let origin_of = |nodes: &[Node]| {
                held.and_then(|holder| nodes.get(holder))
                    .map_or((0.0, 0.0), content_origin)
            };
            let (here, there) = (origin_of(&page.nodes), origin_of(&laid.nodes));
            let (dx, dy) = (
                (target.rect.x - there.0) - (node.rect.x - here.0),
                (target.rect.y - there.1) - (node.rect.y - here.1),
            );
            let end = subtree_end(&page.nodes, index);
            if let Some(boxes) = page.nodes.get_mut(index..end) {
                shift(boxes, dx, dy, 1.0);
            }
        }
        page
    }
}

/// What a box lets of itself be drawn in the boxes inside it.
fn children_clip(node: &Node) -> Rect {
    if node.clips {
        node.clip.intersect(node.rect)
    } else {
        node.clip
    }
}

/// The boxes `sizes` name are as large as they say, and cut what is in them.
fn pin(element: &mut Element, sizes: &[(Id, (f32, f32))]) {
    if let Some(id) = element.meta.id
        && let Some((_, (width, height))) = sizes.iter().find(|(held, _)| *held == id)
    {
        let style = &mut element.style;
        style.width = Length::Px(*width);
        style.height = Length::Px(*height);
        style.min_width = Length::Auto;
        style.min_height = Length::Auto;
        style.max_width = Length::Auto;
        style.max_height = Length::Auto;
        style.grow = 0.0;
        style.shrink = Some(0.0);
        if style.overflow == Overflow::Visible {
            style.overflow = Overflow::Hidden;
        }
    }
    for child in &mut element.children {
        pin(child, sizes);
    }
}
