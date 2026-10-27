//! What the editor remembers of what was done: the steps of the last versions, to carry over them
//! what a page sends late ([`Log`]), and what undoing and doing again take back and put back
//! ([`Undo`]).

use std::collections::VecDeque;

use pencere_view::edit::{Range, Refusal, limits};

use crate::changes::{Done, Step, compose};

/// The steps of the last [`limits::VERSIONS`] versions of a document.
#[derive(Clone, Debug, Default)]
pub(crate) struct Log {
    /// The oldest version a set here was made against.
    oldest: u64,
    /// The steps that took `oldest + i` to `oldest + i + 1`.
    sets: VecDeque<Vec<Step>>,
    /// The length of the text at `oldest + i`.
    lens: VecDeque<usize>,
}

impl Log {
    pub fn new(version: u64) -> Self {
        Self {
            oldest: version,
            ..Self::default()
        }
    }

    /// The version the last set took the document to.
    pub fn current(&self) -> u64 {
        self.oldest + self.sets.len() as u64
    }

    /// One more version: the steps that made it and the length of the text before them.
    pub fn push(&mut self, steps: Vec<Step>, len_before: usize) {
        self.sets.push_back(steps);
        self.lens.push_back(len_before);
        while self.sets.len() as u64 > limits::VERSIONS {
            self.sets.pop_front();
            self.lens.pop_front();
            self.oldest += 1;
        }
    }

    /// The sets done since `version`, in turn; refused where it is older than the oldest known or
    /// newer than the document.
    pub fn since(&self, version: u64) -> Result<Vec<&Vec<Step>>, Refusal> {
        if version > self.current() {
            return Err(Refusal::FutureVersion);
        }
        let Some(skip) = version.checked_sub(self.oldest) else {
            return Err(Refusal::StaleVersion);
        };
        let skip = usize::try_from(skip).map_err(|_| Refusal::StaleVersion)?;
        Ok(self.sets.iter().skip(skip).collect())
    }

    /// How long the text was at `version` (`now` is its length at the current one).
    pub fn len_at(&self, version: u64, now: usize) -> Result<usize, Refusal> {
        if version == self.current() {
            return Ok(now);
        }
        if version > self.current() {
            return Err(Refusal::FutureVersion);
        }
        let back = version
            .checked_sub(self.oldest)
            .and_then(|at| usize::try_from(at).ok())
            .ok_or(Refusal::StaleVersion)?;
        self.lens.get(back).copied().ok_or(Refusal::StaleVersion)
    }
}

/// The most groups of changes undoing goes back through.
pub const UNDO_DEPTH: usize = 512;
/// Typing that pauses longer than this, seconds, is undone apart from what came before.
pub const UNDO_PAUSE: f64 = 1.0;

/// What kind of change a group is: typing goes with typing and deleting with deleting, while it
/// does not pause; the rest is a group of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Typing,
    Deleting,
    /// What an input method composes: one group, however long it takes.
    Compose,
    Other,
}

/// Changes undone together.
#[derive(Clone, Debug)]
pub(crate) struct Group {
    /// What they did, as one set over the text before them.
    pub done: Vec<Done>,
    /// The length of the text before them.
    pub len_before: usize,
    /// The version of the document before them.
    pub start: u64,
    /// The version they took the document to: the version their changes are in the places of,
    /// to be undone (a later version of the same text, where what came after was undone).
    pub end: u64,
    /// The selection before them, put back when they are undone.
    pub before: Vec<Range>,
    pub main_before: usize,
    /// When the last of them was made.
    pub at: f64,
    pub kind: Kind,
    /// More may join it.
    pub open: bool,
}

/// What can be undone and done again.
#[derive(Clone, Debug, Default)]
pub(crate) struct Undo {
    pub undo: Vec<Group>,
    pub redo: Vec<Group>,
}

impl Undo {
    /// A change the person or the page made: it joins the group before it where it is of the same
    /// kind, follows it at once and comes soon enough; what was undone cannot be done again after.
    pub fn record(&mut self, group: Group) {
        self.redo.clear();
        if let Some(top) = self.undo.last_mut()
            && top.open
            && top.kind == group.kind
            && group.kind != Kind::Other
            && top.end + 1 == group.end
            && (group.kind == Kind::Compose || group.at - top.at <= UNDO_PAUSE)
        {
            top.done = compose(&top.done, top.len_before, &group.done);
            top.end = group.end;
            top.at = group.at;
            return;
        }
        self.close();
        self.undo.push(group);
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
    }

    /// Nothing joins the last group: the person did something else in between.
    pub fn close(&mut self) {
        if let Some(top) = self.undo.last_mut() {
            top.open = false;
        }
    }

    /// `group` was taken back (or done again) with nothing done after it: the document is as it
    /// was at the version before it, so the group that came before it on `stack` is made against
    /// the version it is at now, and nothing need be carried over for it.
    pub fn same_as(stack: &mut [Group], group: &Group, now: u64) {
        if let Some(before) = stack.last_mut()
            && before.end == group.start
        {
            before.end = now;
        }
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}
