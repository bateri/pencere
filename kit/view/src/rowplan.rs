//! How a box's height is shared out in rows where a page is laid out on a grid of cells.
//!
//! A page's lengths are points and a row is not a whole number of them: rounded one by one, the
//! parts of a box are not as many rows as the box (a card of nine rows whose parts come to eight
//! leaves a row at its foot, and a gap of six points, a third of a row, is none). So a box that is
//! written a height, and is a column of boxes that are written theirs, is given its rows as a whole:
//!
//! - the box's rows are those of its height;
//! - its padding above and below, the gaps between what is in it and the heights of those are
//!   shared out over the rows by the points each is (the largest remainders take what is left);
//! - a part that has a line of text in it is at least as many rows as it has lines;
//! - parts that are as many points as one another (the paddings, the gaps, a pair of equal
//!   boxes) are as many rows as one another: they take a row more together or none, and the rows
//!   that are too few for a group to have one more are not given, so the box is that many rows
//!   shorter (ten equal rows are ten rows, whatever their points);
//! - a part takes a row above its points only where it is as good as half a row above them,
//!   which is what rounding it on its own would do, so that a few points of room are no row (a box
//!   that is given its rows by the box it is in has them all to give out, and its parts take
//!   them by the largest remainders alone);
//! - a box that is given its rows by the box it is in has them, and takes none away.
//!
//! A box that centres what is in it down the page is the nearest number of rows to its height that
//! has the same oddness as what is in it ([`with_middle`]), so that a line in it has as many rows
//! above as below. A box that is not written a height, or whose parts are not all written one, is as
//! it was: the sum of what is in it, a length rounded on its own.
//!
//! The rows come out in the layout ([`crate::layout`]): what is drawn, what is hit, how far a box
//! scrolls and what a page is told are all of the same numbers. A page that is not laid out on a
//! grid has no plan at all.

use std::collections::BTreeMap;

use crate::element::{Element, Kind};
use crate::layout::Units;
use crate::style::{Direction, Display, Items, Justify, Length, Style};

/// What a box's own sizes down the page come to in rows, and the rows of what is in it that the box
/// gives. A size that is `None` is not planned: it is its length in points, rounded on its own.
#[derive(Clone, Debug, Default)]
pub(crate) struct Plan {
    pub(crate) height: Option<f32>,
    pub(crate) top: Option<f32>,
    pub(crate) bottom: Option<f32>,
    pub(crate) gap: Option<f32>,
    /// The height of each of the children, by their places among them.
    pub(crate) children: Vec<Option<f32>>,
}

/// Where a box takes room down the page.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Slot {
    Top,
    Bottom,
    Gap,
    Child(usize),
    /// The points the box has and what is in it does not take.
    Free,
}

/// One part of a column.
#[derive(Clone, Copy, Debug)]
struct Part {
    slot: Slot,
    points: f32,
    /// The rows it is at least, for the lines of text in it.
    least: f32,
    /// The rows its points come to, whole and not.
    share: f32,
    rows: f32,
}

/// Why a box was not shared out: it is then as it was, its heights each as many rows as they come
/// nearest to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Refused {
    NotAColumn,
    Bounds,
    NotWritten,
    Grows,
    Margin,
    Overfull,
    Nothing,
}

/// The plan of a box on a grid: `given` is the rows the box it is in gives it.
pub(crate) fn plan(
    style: &Style,
    children: &[Element],
    given: Option<f32>,
    scale: f32,
    unit: Units,
) -> Plan {
    if !unit.grid {
        return Plan::default();
    }
    let mut plan = Plan {
        height: given,
        ..Plan::default()
    };
    let Length::Px(points) = style.height else {
        return plan;
    };
    if points <= 0.0 {
        return plan;
    }
    if children.iter().any(in_flow)
        && let Ok(shared) = share(style, children, points, given, scale, unit)
    {
        return shared;
    }
    if given.is_none() {
        plan.height = Some(alone(style, children, points, scale, unit));
    }
    plan
}

/// The rows a box written `points` high is where no box gives it any: where it centres what is in
/// it down the page the nearest that has a middle, else the nearest, and one at least.
fn alone(style: &Style, children: &[Element], points: f32, scale: f32, unit: Units) -> f32 {
    if centres(style) {
        let inside = held(style, children, scale, unit);
        if inside >= 1.0 {
            return with_middle(points * scale / unit.y, inside);
        }
    }
    unit.high(points, scale)
}

/// The nearest number of rows to `wanted` that is odd where `inside` is and even where it is even,
/// the smaller where two are as near, and `inside` at least.
pub(crate) fn with_middle(wanted: f32, inside: f32) -> f32 {
    let below = inside + 2.0 * ((wanted - inside) / 2.0).floor();
    let above = below + 2.0;
    let near = if wanted - below <= above - wanted {
        below
    } else {
        above
    };
    near.max(inside)
}

/// Whether a box centres what is in it down the page: a row that centres its items (`align-items`)
/// or a column that centres them along it (`justify-content`).
pub(crate) fn centres(style: &Style) -> bool {
    style.display == Display::Flex
        && match style.direction {
            Direction::Row => style.items == Some(Items::Center),
            Direction::Column => style.justify == Some(Justify::Center),
        }
}

/// The rows of what is in a box that centres: the tallest of a row's items, the sum of a column's.
fn held(style: &Style, children: &[Element], scale: f32, unit: Units) -> f32 {
    let [top, _, bottom, _] = style.padding;
    let around = unit.down(top, scale) + unit.down(bottom, scale);
    around + inside(style, children, scale, unit)
}

/// The rows of the items of a box, with the gaps between them.
fn inside(style: &Style, children: &[Element], scale: f32, unit: Units) -> f32 {
    let rows = children
        .iter()
        .filter(|child| in_flow(child))
        .map(|child| rows_of(child, scale, unit));
    if style.display == Display::Flex && style.direction == Direction::Column {
        let count = children.iter().filter(|child| in_flow(child)).count();
        let gaps = count.saturating_sub(1) as f32 * unit.down(style.gap.1, scale);
        rows.sum::<f32>() + gaps
    } else {
        rows.fold(0.0, f32::max)
    }
}

/// The rows an element is where nothing gives it any: its own height where it is written one, else
/// what is in it.
fn rows_of(element: &Element, scale: f32, unit: Units) -> f32 {
    let style = &element.style;
    if let Length::Px(points) = style.height
        && points > 0.0
    {
        return plan(style, &element.children, None, scale, unit)
            .height
            .unwrap_or(0.0);
    }
    match &element.kind {
        Kind::Box => held(style, &element.children, scale, unit),
        Kind::Text(_) | Kind::Rich(_) => needs(element),
        Kind::Spark(_) | Kind::Image(_) | Kind::Canvas(_) => 0.0,
    }
}

/// Whether an element takes room in the box it is in (an overlay, a box laid over the rest and a box
/// that is not shown do not).
fn in_flow(element: &Element) -> bool {
    let style = &element.style;
    !style.overlay && !style.absolute && style.display != Display::None
}

/// The rows an element is at least, for the lines of text in it: a line is a row, and what is in a
/// column is one under another.
pub(crate) fn needs(element: &Element) -> f32 {
    let style = &element.style;
    if style.display == Display::None {
        return 0.0;
    }
    match &element.kind {
        Kind::Text(words) => {
            if words.is_empty() {
                0.0
            } else {
                1.0
            }
        }
        Kind::Rich(_) => 1.0,
        Kind::Box => {
            let lines = element
                .children
                .iter()
                .filter(|child| in_flow(child))
                .map(needs);
            if style.display == Display::Flex && style.direction == Direction::Column {
                lines.sum()
            } else {
                lines.fold(0.0, f32::max)
            }
        }
        Kind::Spark(_) | Kind::Image(_) | Kind::Canvas(_) => 0.0,
    }
}

/// A column written `height` points, shared out over its rows.
fn share(
    style: &Style,
    children: &[Element],
    height: f32,
    given: Option<f32>,
    scale: f32,
    unit: Units,
) -> Result<Plan, Refused> {
    if style.display != Display::Flex || style.direction != Direction::Column || style.wrap {
        return Err(Refused::NotAColumn);
    }
    if style.min_height != Length::Auto || style.max_height != Length::Auto {
        return Err(Refused::Bounds);
    }
    let mut parts: Vec<Part> = Vec::new();
    let [top, _, bottom, _] = style.padding;
    if top > 0.0 {
        parts.push(part(Slot::Top, top, 0.0));
    }
    let mut first = true;
    for (index, child) in children
        .iter()
        .enumerate()
        .filter(|(_, child)| in_flow(child))
    {
        if !first && style.gap.1 > 0.0 {
            parts.push(part(Slot::Gap, style.gap.1, 0.0));
        }
        first = false;
        let own = &child.style;
        let Length::Px(points) = own.height else {
            return Err(Refused::NotWritten);
        };
        if own.grow > 0.0 {
            return Err(Refused::Grows);
        }
        if own.min_height != Length::Auto || own.max_height != Length::Auto {
            return Err(Refused::Bounds);
        }
        if own.margin[0] != 0.0 || own.margin[2] != 0.0 {
            return Err(Refused::Margin);
        }
        parts.push(part(Slot::Child(index), points.max(0.0), needs(child)));
    }
    if bottom > 0.0 {
        parts.push(part(Slot::Bottom, bottom, 0.0));
    }
    let taken: f32 = parts.iter().map(|part| part.points).sum();
    if taken > height + 0.5 {
        return Err(Refused::Overfull);
    }
    // What the box has and what is in it does not take stands where the box puts it.
    let free = height - taken;
    if free >= 0.5 {
        match style.justify {
            Some(Justify::Center) => {
                parts.insert(0, part(Slot::Free, free / 2.0, 0.0));
                parts.push(part(Slot::Free, free / 2.0, 0.0));
            }
            Some(Justify::End) => parts.insert(0, part(Slot::Free, free, 0.0)),
            _ => parts.push(part(Slot::Free, free, 0.0)),
        }
    }
    let total = given.unwrap_or_else(|| (height * scale / unit.y).round().max(1.0));
    for part in &mut parts {
        part.share = part.points * total / height;
        part.rows = (part.share + EPSILON).floor().max(part.least);
    }
    let mut left = total - parts.iter().map(|part| part.rows).sum::<f32>();
    // Groups of parts of one size, each of those that are not at their share yet.
    let mut groups: BTreeMap<(bool, i64), Vec<usize>> = BTreeMap::new();
    for (place, part) in parts.iter().enumerate() {
        if part.share - part.rows > EPSILON {
            let size = (part.points * 100.0).round() as i64;
            groups
                .entry((part.slot == Slot::Free, size))
                .or_default()
                .push(place);
        }
    }
    let mut order: Vec<(f32, usize, Vec<usize>)> = groups
        .into_values()
        .filter_map(|members| {
            let place = *members.first()?;
            let part = parts.get(place)?;
            Some((part.share - part.rows, place, members))
        })
        .collect();
    order.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    // A box that is given its rows has them all to give out; one that is not takes a row for a part
    // only where it is at least half of one, and is the rows shorter for those that are left.
    let least = if given.is_some() {
        EPSILON
    } else {
        0.5 - EPSILON
    };
    for (remainder, _, members) in order {
        if left <= 0.0 {
            break;
        }
        if remainder < least || members.len() as f32 > left {
            continue;
        }
        for place in &members {
            if let Some(part) = parts.get_mut(*place) {
                part.rows += 1.0;
            }
        }
        left -= members.len() as f32;
    }
    let rows: f32 = parts.iter().map(|part| part.rows).sum();
    if rows <= 0.0 {
        return Err(Refused::Nothing);
    }
    let mut plan = Plan {
        height: Some(given.unwrap_or(rows)),
        children: vec![None; children.len()],
        ..Plan::default()
    };
    for part in &parts {
        match part.slot {
            Slot::Top => plan.top = Some(part.rows),
            Slot::Bottom => plan.bottom = Some(part.rows),
            Slot::Gap => plan.gap = Some(part.rows),
            Slot::Child(index) => {
                if let Some(entry) = plan.children.get_mut(index) {
                    *entry = Some(part.rows);
                }
            }
            Slot::Free => {}
        }
    }
    Ok(plan)
}

/// What one rounds a share to a whole row within.
const EPSILON: f32 = 1e-3;

fn part(slot: Slot, points: f32, least: f32) -> Part {
    Part {
        slot,
        points,
        least,
        share: 0.0,
        rows: 0.0,
    }
}
