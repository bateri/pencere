//! Sets of changes: checking one a page sent, carrying a place over one ([`map`]), making one of two
//! made in turn ([`compose`]) and the one that undoes it ([`invert`]).
//!
//! A set is a list of [`Change`]s in the places of the text it is made against, in order and apart
//! (one may end where the next starts). It is done from the last to the first, so that each is done
//! in the places it was written in.

use pencere_view::edit::{Change, Refusal};

/// What a change did, for carrying places over it: the range it replaced and how many characters
/// came in its place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Step {
    pub from: usize,
    pub to: usize,
    pub len: usize,
}

/// A change as it was done: what it took out as well as what it put in, so that it can be undone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Done {
    pub from: usize,
    pub to: usize,
    pub removed: String,
    pub insert: String,
}

/// Which way a place goes where text comes in exactly at it, or where a range around it is replaced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Assoc {
    /// It stays before what comes in (at the start of a range replaced around it).
    Before,
    /// It goes after it (to the end of what replaced the range around it).
    After,
}

/// Whether `changes` is a set that can be made against a text of `len` characters: each range the
/// right way round, in the text, and after the one before.
pub(crate) fn check(changes: &[Change], len: usize) -> Result<(), Refusal> {
    let mut end = 0;
    for change in changes {
        if change.from > change.to {
            return Err(Refusal::Reversed);
        }
        if change.to > len {
            return Err(Refusal::OutOfDocument);
        }
        if change.from < end {
            return Err(Refusal::Overlapping);
        }
        end = change.to;
    }
    Ok(())
}

/// The steps of a set of changes.
pub(crate) fn steps(changes: &[Change]) -> Vec<Step> {
    changes
        .iter()
        .map(|change| Step {
            from: change.from,
            to: change.to,
            len: change.insert.chars().count(),
        })
        .collect()
}

/// The steps of a set as it was done.
pub(crate) fn steps_done(done: &[Done]) -> Vec<Step> {
    done.iter()
        .map(|change| Step {
            from: change.from,
            to: change.to,
            len: change.insert.chars().count(),
        })
        .collect()
}

/// Where the place `at` is after the steps of one set: past a range replaced before it by what
/// came in its place; inside one, at its start or its end (`assoc`); at a place text came in, before
/// or after it (`assoc`).
pub(crate) fn map(steps: &[Step], at: usize, assoc: Assoc) -> usize {
    // How far the steps before `at` moved it, as a signed number: the characters that came in less
    // those that went.
    let mut gained: i128 = 0;
    for step in steps {
        let removed = step.to - step.from;
        let moved = |gained: i128, base: usize| -> usize {
            usize::try_from(base as i128 + gained).unwrap_or(0)
        };
        if at < step.from {
            break;
        }
        if step.from == step.to {
            // Text came in at `step.from`, and only there.
            if at == step.from {
                match assoc {
                    Assoc::Before => break,
                    Assoc::After => {
                        gained += step.len as i128;
                        continue;
                    }
                }
            }
            gained += step.len as i128;
            continue;
        }
        if at == step.from {
            break;
        }
        if at < step.to {
            return match assoc {
                Assoc::Before => moved(gained, step.from),
                Assoc::After => moved(gained + step.len as i128, step.from),
            };
        }
        gained += step.len as i128 - removed as i128;
    }
    usize::try_from(at as i128 + gained).unwrap_or(0)
}

/// Where `at` is after the steps of several sets, done in turn.
pub(crate) fn map_through<'a>(
    sets: impl IntoIterator<Item = &'a Vec<Step>>,
    at: usize,
    assoc: Assoc,
) -> usize {
    sets.into_iter().fold(at, |at, steps| map(steps, at, assoc))
}

/// A set of changes made against an older text, carried over what was done since (`sets`, in
/// turn): each range's start goes after what came in at it, its end before, so that it takes in
/// none of what others typed at its edges; a range that was taken out altogether is the place it
/// was.
pub(crate) fn rebase<'a>(
    changes: Vec<Change>,
    sets: impl IntoIterator<Item = &'a Vec<Step>> + Clone,
) -> Vec<Change> {
    let mut rebased: Vec<Change> = Vec::with_capacity(changes.len());
    for change in changes {
        let from = map_through(sets.clone(), change.from, Assoc::After);
        let to = map_through(sets.clone(), change.to, Assoc::Before).max(from);
        // A set stays in order whatever is carried over it; two changes that came to touch keep it.
        let from = rebased.last().map_or(from, |last| from.max(last.to));
        let to = to.max(from);
        rebased.push(Change {
            from,
            to,
            insert: change.insert,
        });
    }
    rebased
}

/// The set that undoes `done`: in the places of the text after it, each range that came in put
/// back as what it replaced.
pub(crate) fn invert(done: &[Done]) -> Vec<Change> {
    let mut gained: i128 = 0;
    let mut out = Vec::with_capacity(done.len());
    for change in done {
        let len = change.insert.chars().count();
        let removed = change.to - change.from;
        let from = usize::try_from(change.from as i128 + gained).unwrap_or(0);
        out.push(Change {
            from,
            to: from + len,
            insert: change.removed.clone(),
        });
        gained += len as i128 - removed as i128;
    }
    out
}

/// One part of a set of changes read from the start of the text to its end.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Op {
    /// Characters kept as they were.
    Keep(usize),
    /// Characters taken out: these.
    Remove(Vec<char>),
    /// Characters put in: these.
    Put(Vec<char>),
}

/// The parts of `done`, over a text of `len` characters.
fn ops(done: &[Done], len: usize) -> Vec<Op> {
    let mut out = Vec::new();
    let mut at = 0;
    for change in done {
        if change.from > at {
            out.push(Op::Keep(change.from - at));
        }
        if change.to > change.from {
            out.push(Op::Remove(change.removed.chars().collect()));
        }
        if !change.insert.is_empty() {
            out.push(Op::Put(change.insert.chars().collect()));
        }
        at = change.to;
    }
    if len > at {
        out.push(Op::Keep(len - at));
    }
    out
}

/// The set that does what `first` and then `second` did, over the text `first` was made against
/// (`len` characters): what `first` put in and `second` took out is neither put in nor taken out.
pub(crate) fn compose(first: &[Done], len: usize, second: &[Done]) -> Vec<Done> {
    let middle = len as i128
        + first
            .iter()
            .map(|c| c.insert.chars().count() as i128 - (c.to - c.from) as i128)
            .sum::<i128>();
    let a = ops(first, len);
    let b = ops(second, usize::try_from(middle).unwrap_or(0));
    let mut a = a.into_iter();
    let mut b = b.into_iter();
    let mut next_a = a.next();
    let mut next_b = b.next();
    let mut out: Vec<Op> = Vec::new();
    loop {
        match (next_a.take(), next_b.take()) {
            (None, None) => break,
            // What the first took out, the second never saw.
            (Some(Op::Remove(chars)), held_b) => {
                out.push(Op::Remove(chars));
                next_a = a.next();
                next_b = held_b;
            }
            // What the second put in is the first's text at none of its places.
            (held_a, Some(Op::Put(chars))) => {
                out.push(Op::Put(chars));
                next_a = held_a;
                next_b = b.next();
            }
            (Some(op_a), Some(op_b)) => {
                let (left_a, left_b) = meet(op_a, op_b, &mut out);
                next_a = left_a.or_else(|| a.next());
                next_b = left_b.or_else(|| b.next());
            }
            // One ran out before the other: the texts were not of the lengths said; what is left
            // is what it is.
            (Some(op_a), None) => {
                out.push(op_a);
                next_a = a.next();
            }
            (None, Some(op_b)) => {
                if let Op::Remove(chars) = op_b {
                    out.push(Op::Remove(chars));
                }
                next_b = b.next();
            }
        }
    }
    to_done(out)
}

/// A part of the first set (a kept or a put run, which the second reads) against a part of the
/// second (a kept or a removed run): what the two come to over as much as both cover, and what is
/// left of each.
fn meet(a: Op, b: Op, out: &mut Vec<Op>) -> (Option<Op>, Option<Op>) {
    let len_a = match &a {
        Op::Keep(n) => *n,
        Op::Put(chars) | Op::Remove(chars) => chars.len(),
    };
    let len_b = match &b {
        Op::Keep(n) => *n,
        Op::Put(chars) | Op::Remove(chars) => chars.len(),
    };
    let both = len_a.min(len_b);
    let split = |op: Op, at: usize| -> (Op, Option<Op>) {
        match op {
            Op::Keep(n) => (Op::Keep(at), (n > at).then(|| Op::Keep(n - at))),
            Op::Put(mut chars) => {
                let rest = chars.split_off(at.min(chars.len()));
                (Op::Put(chars), (!rest.is_empty()).then_some(Op::Put(rest)))
            }
            Op::Remove(mut chars) => {
                let rest = chars.split_off(at.min(chars.len()));
                (
                    Op::Remove(chars),
                    (!rest.is_empty()).then_some(Op::Remove(rest)),
                )
            }
        }
    };
    let (a, left_a) = split(a, both);
    let (b, left_b) = split(b, both);
    match (a, b) {
        (Op::Keep(n), Op::Keep(_)) => out.push(Op::Keep(n)),
        (Op::Put(chars), Op::Keep(_)) => out.push(Op::Put(chars)),
        // The second took out text of the first's: what it took out is the original's.
        (Op::Keep(_), Op::Remove(chars)) => out.push(Op::Remove(chars)),
        // The second took out what the first put in: neither is left.
        (Op::Put(_), Op::Remove(_)) => {}
        // Not reached: removes of the first and puts of the second are taken before.
        (a, _) => out.push(a),
    }
    (left_a, left_b)
}

/// The changes the parts say, as places in the original text.
fn to_done(ops: Vec<Op>) -> Vec<Done> {
    let mut out: Vec<Done> = Vec::new();
    let mut at = 0;
    let mut open: Option<Done> = None;
    for op in ops {
        match op {
            Op::Keep(n) => {
                if n == 0 {
                    continue;
                }
                if let Some(done) = open.take() {
                    out.push(done);
                }
                at += n;
            }
            Op::Remove(chars) => {
                let done = open.get_or_insert_with(|| Done {
                    from: at,
                    to: at,
                    ..Done::default()
                });
                done.to += chars.len();
                at += chars.len();
                done.removed.extend(chars);
            }
            Op::Put(chars) => {
                let done = open.get_or_insert_with(|| Done {
                    from: at,
                    to: at,
                    ..Done::default()
                });
                done.insert.extend(chars);
            }
        }
    }
    if let Some(done) = open {
        out.push(done);
    }
    // Text put back as it was is no change.
    out.retain(|done| done.removed != done.insert);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done(from: usize, to: usize, removed: &str, insert: &str) -> Done {
        Done {
            from,
            to,
            removed: removed.into(),
            insert: insert.into(),
        }
    }

    #[test]
    fn a_place_after_a_change_moves_by_what_it_added() {
        let steps = [Step {
            from: 2,
            to: 4,
            len: 5,
        }];
        assert_eq!(map(&steps, 1, Assoc::Before), 1);
        assert_eq!(map(&steps, 2, Assoc::After), 2);
        assert_eq!(map(&steps, 3, Assoc::Before), 2);
        assert_eq!(map(&steps, 3, Assoc::After), 7);
        assert_eq!(map(&steps, 4, Assoc::Before), 7);
        assert_eq!(map(&steps, 9, Assoc::Before), 12);
    }

    #[test]
    fn a_place_where_text_came_in_goes_before_or_after_it() {
        let steps = [Step {
            from: 3,
            to: 3,
            len: 2,
        }];
        assert_eq!(map(&steps, 3, Assoc::Before), 3);
        assert_eq!(map(&steps, 3, Assoc::After), 5);
    }

    #[test]
    fn two_sets_in_turn_compose_into_one() {
        // "abcdef" → "aXYcdef" (b to XY) → "aXcdZef" (Y out, Z in at 4 of the middle text).
        let first = [done(1, 2, "b", "XY")];
        let second = [done(2, 3, "Y", ""), done(5, 5, "", "Z")];
        let both = compose(&first, 6, &second);
        assert_eq!(both, vec![done(1, 2, "b", "X"), done(4, 4, "", "Z")]);
    }

    #[test]
    fn what_the_first_put_in_and_the_second_took_out_is_nothing() {
        let first = [done(3, 3, "", "abc")];
        let second = [done(3, 6, "abc", "")];
        assert_eq!(compose(&first, 5, &second), Vec::<Done>::new());
    }

    #[test]
    fn the_inverse_puts_back_what_was_replaced() {
        let set = [done(1, 2, "b", "XY"), done(4, 6, "ef", "")];
        assert_eq!(
            invert(&set),
            vec![Change::new(1, 3, "b"), Change::new(5, 5, "ef")]
        );
    }
}
