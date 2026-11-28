//! Which rows of a frame are rows of the frame before it, a little way down or up: what a page that
//! scrolled did, found from the two draw lists, before a pixel of the new one is drawn.
//!
//! A row of a picture is what the items that reach it draw on it, in order ([`reach`]). A row of one
//! frame is, to the pixel, a row of another where the items that reach the two are the same, one for
//! one and in the same order, and each pair draws the same on the two rows:
//!
//! - the words of a line, whose drawing is set on a pixel row by one rounding of where its baseline
//!   is: a line that moved by a part of a pixel moved by a whole number of them, whichever way its
//!   baseline rounds, and two lines that are the same but for where they stand draw the same on the
//!   rows that are as far from their baselines;
//! - a box (no shadow) that is the same, on the rows that are well inside it: a row of it that is not
//!   near its top or its bottom is one fill across, wherever its edges are (a box moved by a part of
//!   a pixel has other edges, and no other middle).
//!
//! Nothing else is the same on two rows but where it did not move at all, and a row that has one
//! of those has no answer here: it is drawn. Rows are found only where the drawing's own rules are
//! known and the cut of a clip is clear of them (a row by the edge of a clip is drawn); what an item
//! reaches past what it draws is the same on both, since the pair has to be the same.

use std::collections::HashMap;

use pencere_view::{DrawList, Font, Paint, Quad, Rect, Run};

use super::{Fonts, intersect};

/// A frame: its draw list, and where each of its items reaches ([`super::reach`], pixels).
#[derive(Clone, Copy)]
pub struct Side<'a> {
    pub list: &'a DrawList,
    pub reach: &'a [Option<(f32, f32)>],
}

/// The rows of a picture that are rows of the one before it.
#[derive(Debug)]
pub struct Slide {
    /// The columns, from and to, in which they are: where the lines that moved are cut.
    pub columns: (u32, u32),
    /// For each row of the picture, how many rows further down the row of the frame before it is
    /// that this is, in the columns, to the pixel; none where that is not known.
    pub rows: Vec<Option<i32>>,
}

/// How far across an item draws, at the most: where it is cut, and the box it is.
#[must_use]
pub fn spread(item: &Paint) -> (f32, f32) {
    match item {
        Paint::Quad(quad) if quad.shadow.is_none() => (
            quad.rect.x.max(quad.clip.rect.x),
            quad.rect.right().min(quad.clip.rect.right()),
        ),
        Paint::Quad(_) | Paint::Editor(_) | Paint::Browser(_) => (f32::NEG_INFINITY, f32::INFINITY),
        Paint::Text(run) => (run.clip.rect.x, run.clip.rect.right()),
        Paint::Image(picture) if picture.turn == 0.0 => (
            picture.rect.x.max(picture.clip.rect.x),
            picture.rect.right().min(picture.clip.rect.right()),
        ),
        Paint::Image(picture) => (picture.clip.rect.x, picture.clip.rect.right()),
        Paint::Figure(figure) => (
            figure.rect.x.max(figure.clip.rect.x),
            figure.rect.right().min(figure.clip.rect.right()),
        ),
    }
}

/// The rows of the picture `size` (`rows`: the ranges of rows that are asked for, first and how
/// many, from the top down) of the frame `new` that are rows of the frame `old`, and in what
/// columns: the lines of text that stand in `new` where they stood in `old`, a little higher or
/// lower, say how far it was (the most of them), and what stands in their columns is compared row by
/// row. None where no line says it, or no row is.
#[must_use]
pub fn slide(
    old: Side<'_>,
    new: Side<'_>,
    fonts: &Fonts,
    size: (u32, u32),
    rows: &[(u32, u32)],
) -> Option<Slide> {
    let mut ascents: HashMap<Font, Option<f32>> = HashMap::new();
    let baselines =
        |side: Side<'_>, ascents: &mut HashMap<Font, Option<f32>>| -> Vec<Option<i64>> {
            side.list
                .items
                .iter()
                .map(|item| match item {
                    Paint::Text(run) => baseline_row(run, fonts, ascents),
                    _ => None,
                })
                .collect()
        };
    let old_rows = baselines(old, &mut ascents);
    let new_rows = baselines(new, &mut ascents);
    let (shifts, columns) = votes(old, new, &old_rows, &new_rows, size, rows)?;
    let columns = (
        columns.0.floor().max(0.0) as u32,
        columns.1.ceil().min(size.0 as f32).max(0.0) as u32,
    );
    if columns.1 <= columns.0 {
        return None;
    }
    let (from, to) = (columns.0 as f32, columns.1 as f32);
    let within = |item: &Paint| {
        let (left, right) = spread(item);
        left < to && right > from
    };
    let members = |side: Side<'_>| -> Vec<(i64, i64, u32)> {
        let mut members: Vec<(i64, i64, u32)> = side
            .list
            .items
            .iter()
            .enumerate()
            .filter_map(|(n, item)| {
                let (top, bottom) = (*side.reach.get(n)?)?;
                if !within(item) {
                    return None;
                }
                let (first, last) = (top.floor() as i64, bottom.ceil() as i64 - 1);
                (last >= first).then_some((first, last, u32::try_from(n).ok()?))
            })
            .collect();
        members.sort_unstable();
        members
    };
    let (old_members, new_members) = (members(old), members(new));
    let height = size.1 as i64;
    let mut found: Vec<Option<i32>> = vec![None; size.1 as usize];
    let mut now = Sweep::new(&new_members);
    let mut before: Vec<Sweep<'_>> = shifts.iter().map(|_| Sweep::new(&old_members)).collect();
    let (mut standing, mut then) = (Vec::new(), Vec::new());
    let mut any = false;
    let mut sticky = 0;
    for &(top, count) in rows {
        for row in i64::from(top)..(i64::from(top) + i64::from(count)).min(height) {
            now.at(row, &mut standing);
            for step in 0..shifts.len() {
                let which = (step + sticky) % shifts.len();
                let Some(&shift) = shifts.get(which) else {
                    continue;
                };
                let there = row + i64::from(shift);
                if there < 0 || there >= height {
                    continue;
                }
                let Some(sweep) = before.get_mut(which) else {
                    continue;
                };
                sweep.at(there, &mut then);
                if standing.len() != then.len() {
                    continue;
                }
                let same = standing.iter().zip(&then).all(|(&n, &o)| {
                    let (Some(a), Some(b)) = (
                        new.list.items.get(n as usize),
                        old.list.items.get(o as usize),
                    ) else {
                        return false;
                    };
                    match (a, b) {
                        (Paint::Quad(a), Paint::Quad(b)) => quad_slides(a, b, row, there),
                        (Paint::Text(a), Paint::Text(b)) => text_slides(
                            (a, new_rows.get(n as usize).copied().flatten()),
                            (b, old_rows.get(o as usize).copied().flatten()),
                            (row, there),
                        ),
                        _ => false,
                    }
                });
                if same {
                    if let Some(slot) = found.get_mut(row as usize) {
                        *slot = Some(shift);
                    }
                    any = true;
                    sticky = which;
                    break;
                }
            }
        }
    }
    any.then_some(Slide {
        columns,
        rows: found,
    })
}

/// The row of pixels a line's baseline is rounded to when it is drawn (the rows of its letters are
/// at fixed distances from it), and none where the line is drawn another way: the lines under and
/// through the words round otherwise, a baseline half way between two rows may be rounded either
/// way by the arithmetic of the rows around it, and a font none has a face of is drawn as boxes.
fn baseline_row(run: &Run, fonts: &Fonts, ascents: &mut HashMap<Font, Option<f32>>) -> Option<i64> {
    if run.underline || run.strike {
        return None;
    }
    let ascent = *ascents.entry(run.font).or_insert_with(|| {
        fonts
            .covers(&run.font, 'a')
            .map(|_| fonts.line(&run.font).0)
    });
    let baseline = run.origin.y + ascent?;
    let part = baseline - baseline.floor();
    if (part - 0.5).abs() < 1e-3 || !baseline.is_finite() {
        return None;
    }
    Some(baseline.round() as i64)
}

/// A key to find lines that are the same but for where they stand: what they say, across, and how.
fn key(run: &Run) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut mix = |word: u64| {
        hash = (hash ^ word).wrapping_mul(0x0100_0000_01b3);
    };
    for byte in run.text.bytes() {
        mix(u64::from(byte));
    }
    mix(u64::from(run.origin.x.to_bits()));
    mix(u64::from(run.font.size.to_bits()));
    mix(u64::from(run.font.weight));
    mix(u64::from(run.font.italic));
    hash
}

/// Whether two lines are the same but for how high they stand.
fn text_alike(a: &Run, b: &Run) -> bool {
    a.origin.x == b.origin.x
        && a.text == b.text
        && a.font == b.font
        && a.color == b.color
        && a.opacity == b.opacity
        && a.grounds == b.grounds
        && a.clip == b.clip
        && a.underline == b.underline
        && a.strike == b.strike
}

/// How far the lines of `new` are from lines of `old` that are the same: the shifts (the likeliest
/// first, and the one either side of it, a line's rows being rounded), and the columns the lines
/// that moved by them are cut to. None where no shift is that of two lines or more.
fn votes(
    old: Side<'_>,
    new: Side<'_>,
    old_rows: &[Option<i64>],
    new_rows: &[Option<i64>],
    size: (u32, u32),
    rows: &[(u32, u32)],
) -> Option<(Vec<i32>, (f32, f32))> {
    let mut table: Vec<(u64, u32)> = Vec::new();
    for (n, item) in old.list.items.iter().enumerate() {
        if let Paint::Text(run) = item
            && old_rows.get(n).copied().flatten().is_some()
            && !run.text.trim().is_empty()
            && let Ok(n) = u32::try_from(n)
        {
            table.push((key(run), n));
        }
    }
    table.sort_unstable();
    let asked = |top: f32, bottom: f32| {
        rows.iter()
            .any(|&(first, count)| top < (first + count) as f32 && bottom > first as f32)
    };
    // How many lines moved by each distance, and where they are cut.
    let mut moved: HashMap<i64, (u32, f32, f32)> = HashMap::new();
    for (n, item) in new.list.items.iter().enumerate() {
        let Paint::Text(now) = item else {
            continue;
        };
        let Some(here) = new_rows.get(n).copied().flatten() else {
            continue;
        };
        if now.text.trim().is_empty() {
            continue;
        }
        let Some((top, bottom)) = new.reach.get(n).copied().flatten() else {
            continue;
        };
        if !asked(top, bottom) {
            continue;
        }
        let wanted = key(now);
        let first = table.partition_point(|(hash, _)| *hash < wanted);
        for &(hash, o) in table.get(first..).unwrap_or(&[]) {
            if hash != wanted {
                break;
            }
            let (Some(Paint::Text(was)), Some(there)) = (
                old.list.items.get(o as usize),
                old_rows.get(o as usize).copied().flatten(),
            ) else {
                continue;
            };
            let by = there - here;
            if by == 0 || by.abs() >= i64::from(size.1) || !text_alike(was, now) {
                continue;
            }
            let entry = moved
                .entry(by)
                .or_insert((0, f32::INFINITY, f32::NEG_INFINITY));
            entry.0 += 1;
            entry.1 = entry.1.min(now.clip.rect.x);
            entry.2 = entry.2.max(now.clip.rect.right());
        }
    }
    let (&best, &(count, ..)) = moved
        .iter()
        .max_by_key(|(by, (count, ..))| (*count, std::cmp::Reverse(by.abs())))?;
    if count < 2 {
        return None;
    }
    let mut shifts = Vec::new();
    let mut columns = (f32::INFINITY, f32::NEG_INFINITY);
    for near in [best, best - 1, best + 1] {
        if near == 0 || near.signum() != best.signum() {
            continue;
        }
        let Some(&(count, left, right)) = moved.get(&near) else {
            // No line was seen at this one, but a line may be (it is tried by the rows).
            if let Ok(near) = i32::try_from(near) {
                shifts.push(near);
            }
            continue;
        };
        if count > 0 {
            columns = (columns.0.min(left), columns.1.max(right));
        }
        if let Ok(near) = i32::try_from(near) {
            shifts.push(near);
        }
    }
    (columns.0 < columns.1).then_some((shifts, columns))
}

/// Whether a row `row` is clear of the edges of `clip` (by a pixel): where a clip cuts an item the
/// row of it is not the same as one a little further down or up.
fn clear_of(clip: Rect, row: i64) -> bool {
    let row = row as f32;
    row >= clip.y + 1.0 && row + 1.0 <= clip.y + clip.h - 1.0
}

/// Whether two lines draw the same on the rows `rows` (new, old): they are the same but for where
/// they stand, their baselines are rounded as far apart as the rows are, and the rows are clear of
/// the clips.
fn text_slides(
    (new, here): (&Run, Option<i64>),
    (old, there): (&Run, Option<i64>),
    rows: (i64, i64),
) -> bool {
    let (Some(here), Some(there)) = (here, there) else {
        return false;
    };
    there - here == rows.1 - rows.0
        && text_alike(new, old)
        && clear_of(new.clip.rect, rows.0)
        && clear_of(old.clip.rect, rows.1)
}

/// Whether the row `row` is well inside the box `quad`: a fill across, whatever its corners are,
/// and the stroke of its border (the corners' radii and the border are cleared).
fn well_inside(quad: &Quad, row: i64) -> bool {
    let near = quad.radii.iter().copied().fold(0.0_f32, f32::max)
        + quad.border.map_or(0.0, |border| border.width)
        + 2.0;
    let row = row as f32;
    row >= quad.rect.y + near
        && row + 1.0 <= quad.rect.bottom() - near
        && clear_of(quad.clip.rect, row as i64)
}

/// Whether two boxes draw the same on the rows `rows` (new, old): they are the same but for where they
/// stand (and are cut the same way: filled with a mask or not), and both rows are well inside them.
fn quad_slides(new: &Quad, old: &Quad, here: i64, there: i64) -> bool {
    // A box with a gradient is not one fill across (the drawing may change how it is filled).
    new.shadow.is_none()
        && old.shadow.is_none()
        && new.gradient.is_none()
        && old.gradient.is_none()
        && new.rect.x == old.rect.x
        && new.rect.w == old.rect.w
        && new.rect.h == old.rect.h
        && new.radii == old.radii
        && new.fill == old.fill
        && new.border == old.border
        && new.clip == old.clip
        && whole(new) == whole(old)
        && well_inside(new, here)
        && well_inside(old, there)
}

/// Whether a box is drawn whole, with no mask of its clip (as the drawing does it).
fn whole(quad: &Quad) -> bool {
    intersect(quad.rect, quad.clip.rect) == Some(quad.rect)
        && quad.clip.round.is_none()
        && quad.clip.fade.is_none()
}

/// The items that reach the rows, as a window moved down them: for a row, which are standing
/// there, in the order they are drawn.
struct Sweep<'a> {
    /// The items' first row, last row and number, by first row.
    members: &'a [(i64, i64, u32)],
    next: usize,
    /// The standing ones: number and last row, by number.
    standing: Vec<(u32, i64)>,
}

impl<'a> Sweep<'a> {
    fn new(members: &'a [(i64, i64, u32)]) -> Self {
        Self {
            members,
            next: 0,
            standing: Vec::new(),
        }
    }

    /// The numbers of the items that reach `row`, in `out`, in the order they are drawn. The rows asked
    /// for do not go back up.
    fn at(&mut self, row: i64, out: &mut Vec<u32>) {
        while let Some(&(first, last, n)) = self.members.get(self.next) {
            if first > row {
                break;
            }
            self.next += 1;
            if last < row {
                continue;
            }
            let at = self.standing.partition_point(|(have, _)| *have < n);
            self.standing.insert(at, (n, last));
        }
        self.standing.retain(|(_, last)| *last >= row);
        out.clear();
        out.extend(self.standing.iter().map(|(n, _)| *n));
    }
}
