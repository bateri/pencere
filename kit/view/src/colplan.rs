//! How a box's width is shared out in columns where a page is laid out on a grid of cells.
//!
//! A page's lengths are points and a column is not a whole number of them: rounded one by one, what
//! is in a box is not as many columns as the box has (a card of 26 columns whose padding is 1.75 of
//! one on each side has 22 inside, and a graph written the card's width less that padding, 22.5
//! columns, rounds to 23 and stands a column past the words above it). So a box that is written a
//! width gives what is in it its columns:
//!
//! - the columns inside the box are those of its width less its padding on each side, each as
//!   many columns as it rounds to on its own (as they were);
//! - in a column, a box written as wide as the inside of the box it is in (within half a point)
//!   is as many columns as that inside; one written narrower is not more columns than the inside;
//!   one written wider is as it was (what a box that scrolls across holds);
//! - in a row whose boxes are all written a width and do not grow, and that are written no wider
//!   than the row together but, each rounded on its own, come to more columns than it has, the
//!   columns inside are shared out over the boxes and the gaps between them by the points each is
//!   (the largest remainders take what is left), and those that are as many points as one another
//!   are as many columns as one another: they take a column more together or none. What the row
//!   has and they do not take stays where the row puts it. A row whose boxes fit as they round, and
//!   one whose boxes are written wider than it, are as they were.
//!
//! A box that is not written a width, one that is a grid, a row that wraps, and a box inside one
//! of these that has a margin beside it, a least or a most width, or grows, is as it was: a length
//! rounded on its own. The columns come out in the layout ([`crate::layout`]); a page that is not
//! laid out on a grid has no plan at all.

use std::collections::BTreeMap;

use crate::element::Element;
use crate::layout::Units;
use crate::style::{Direction, Display, Length, Style};

/// The columns a box gives each of the boxes in it, by their places among them; `None` where it
/// gives none (the box's width is then its points, rounded on its own).
#[derive(Clone, Debug, Default)]
pub(crate) struct Plan {
    pub(crate) children: Vec<Option<f32>>,
    /// The columns of a gap of a row that was shared out.
    pub(crate) gap: Option<f32>,
}

/// As near as two lengths are when a page wrote the one from the other, points.
const SAME: f32 = 0.5;

/// What one rounds a share to a whole column within.
const EPSILON: f32 = 1e-3;

/// The plan of a box on a grid: `given` is the columns the box it is in gives it.
pub(crate) fn plan(
    style: &Style,
    children: &[Element],
    given: Option<f32>,
    scale: f32,
    unit: Units,
) -> Plan {
    let none = Plan::default;
    if !unit.grid || style.display != Display::Flex || style.wrap {
        return none();
    }
    let Length::Px(width) = style.width else {
        return none();
    };
    let [_, right, _, left] = style.padding;
    let columns = given.unwrap_or_else(|| unit.wide(width, scale));
    let inside = columns - unit.across(left, scale) - unit.across(right, scale);
    let points = width - left - right;
    if inside < 1.0 || points <= 0.0 {
        return none();
    }
    match style.direction {
        Direction::Column => Plan {
            children: children
                .iter()
                .map(|child| across(child, points, inside, scale, unit))
                .collect(),
            gap: None,
        },
        Direction::Row => along(style, children, points, inside, scale, unit).unwrap_or_default(),
    }
}

/// Whether an element takes room in the box it is in.
fn in_flow(element: &Element) -> bool {
    let style = &element.style;
    !style.overlay && !style.absolute && style.display != Display::None
}

/// The points an element is written wide, where the box it is in may give it its columns: it is
/// written a width, has no least and no most, and no margin beside it.
fn written(element: &Element) -> Option<f32> {
    let style = &element.style;
    let Length::Px(points) = style.width else {
        return None;
    };
    let free = style.min_width == Length::Auto
        && style.max_width == Length::Auto
        && style.margin[1] == 0.0
        && style.margin[3] == 0.0;
    (points > 0.0 && free).then_some(points)
}

/// The columns a box of a column is given: those inside the box it is in where it is written as
/// wide as that, no more than them where it is written narrower.
fn across(child: &Element, points: f32, inside: f32, scale: f32, unit: Units) -> Option<f32> {
    if !in_flow(child) {
        return None;
    }
    let own = written(child)?;
    if (own - points).abs() <= SAME {
        return Some(inside);
    }
    (own < points && unit.wide(own, scale) > inside).then_some(inside)
}

/// One part of a row: a box or the gaps between the boxes.
#[derive(Clone, Copy, Debug)]
struct Part {
    /// The box's place among the children; `None` for a gap.
    child: Option<usize>,
    points: f32,
    share: f32,
    columns: f32,
}

/// A row written `points` wide inside, shared out over its `inside` columns; `None` where it is
/// not shared (it is then as it was): where what is in it, each rounded on its own, is no more
/// columns than the row has, nothing is to be done.
fn along(
    style: &Style,
    children: &[Element],
    points: f32,
    inside: f32,
    scale: f32,
    unit: Units,
) -> Option<Plan> {
    let mut parts: Vec<Part> = Vec::new();
    for (index, child) in children
        .iter()
        .enumerate()
        .filter(|(_, child)| in_flow(child))
    {
        if child.style.grow > 0.0 {
            return None;
        }
        if !parts.is_empty() && style.gap.0 > 0.0 {
            parts.push(part(None, style.gap.0));
        }
        parts.push(part(Some(index), written(child)?));
    }
    let taken: f32 = parts.iter().map(|part| part.points).sum();
    if parts.is_empty() || taken > points + SAME {
        return None;
    }
    let alone: f32 = parts
        .iter()
        .map(|part| match part.child {
            Some(_) => unit.wide(part.points, scale),
            None => unit.across(part.points, scale),
        })
        .sum();
    if alone <= inside {
        return None;
    }
    for part in &mut parts {
        part.share = part.points * inside / points;
        part.columns = (part.share + EPSILON).floor();
    }
    // What the boxes and the gaps come to in columns, whole and not: what is past that is the room
    // the row has and they do not take, and is nobody's to take.
    let mut left = (taken * inside / points + EPSILON).floor()
        - parts.iter().map(|part| part.columns).sum::<f32>();
    // Groups of parts of one size, each of those that are not at their share yet: the gaps are one
    // group of their own (a row has one gap), the boxes go by their points.
    let mut groups: BTreeMap<(bool, i64), Vec<usize>> = BTreeMap::new();
    for (place, part) in parts.iter().enumerate() {
        if part.share - part.columns > EPSILON {
            #[allow(clippy::cast_possible_truncation)]
            let size = (part.points * 100.0).round() as i64;
            groups
                .entry((part.child.is_none(), size))
                .or_default()
                .push(place);
        }
    }
    let mut order: Vec<(f32, usize, Vec<usize>)> = groups
        .into_values()
        .filter_map(|members| {
            let place = *members.first()?;
            let part = parts.get(place)?;
            Some((part.share - part.columns, place, members))
        })
        .collect();
    order.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    for (_, _, members) in order {
        #[allow(clippy::cast_precision_loss)]
        let wanted = members.len() as f32;
        if left <= 0.0 {
            break;
        }
        if wanted > left {
            continue;
        }
        for place in &members {
            if let Some(part) = parts.get_mut(*place) {
                part.columns += 1.0;
            }
        }
        left -= wanted;
    }
    let mut plan = Plan {
        children: vec![None; children.len()],
        gap: None,
    };
    for part in &parts {
        match part.child {
            // A box that was written is not lost between the cells.
            Some(index) => {
                if let Some(entry) = plan.children.get_mut(index) {
                    *entry = Some(part.columns.max(1.0));
                }
            }
            None => plan.gap = Some(part.columns),
        }
    }
    // The boxes that were kept from vanishing may have taken a column the row has not.
    let gaps = parts.iter().filter(|part| part.child.is_none()).count();
    #[allow(clippy::cast_precision_loss)]
    let all = plan.children.iter().flatten().sum::<f32>() + plan.gap.unwrap_or(0.0) * gaps as f32;
    (all <= inside).then_some(plan)
}

fn part(child: Option<usize>, points: f32) -> Part {
    Part {
        child,
        points,
        share: 0.0,
        columns: 0.0,
    }
}
