//! One editor's document and what is kept with it: its selections, versions, history, decorations,
//! what an input method composes and what a search found. Nothing a page sends breaks it: what is
//! not something it can do is refused, and the page is told why ([`Note::Refused`]).

use pencere_view::edit::{
    Change, Decorate, Decoration, Edited, EditorOptions, Origin, Range, Refusal, Selection, limits,
};

use crate::changes::{Assoc, Done, Step, check, invert, map, rebase, steps, steps_done};
use crate::decorations::Decorations;
use crate::find::Found;
use crate::history::{Group, Kind, Log, Undo};
use crate::segments::{at_column, column_of};
use crate::text::{Snapshot, Text};

/// What an editor has to tell its page, in the order it happened.
#[derive(Clone, Debug, PartialEq)]
pub enum Note {
    /// The document changed.
    Edited(Edited),
    /// The selections are elsewhere, or an input method began or stopped composing.
    Selection(Selection),
    /// What the page asked was not done.
    Refused(Refusal),
}

/// What an input method is composing: the range of the document its text is in, the part of that
/// it says is selected (characters of the text), and what the range held before it began.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Marked {
    pub from: usize,
    pub to: usize,
    pub original: String,
}

/// Where the rows of the text are, for a move up and down: the editor asks it where across a place
/// is, and which place is nearest a point across a number of rows away. What a row is, is the
/// placement's: a line, or the part of a line that wraps, with the folded lines left out.
pub trait Placement {
    /// Where across `at` is, in the placement's units.
    fn x_of(&self, buffer: &Buffer, at: usize) -> f32;
    /// The place nearest `x` across on the row `rows` rows below `at`'s (above it, negative);
    /// `None` where there is no such row.
    fn row_from(&self, buffer: &Buffer, at: usize, rows: isize, x: f32) -> Option<usize>;
    /// How many rows a page up or down goes.
    fn page_rows(&self) -> usize;
}

/// The placement of a text with no room: a row is a line that is not folded away, and across is
/// the column (a grapheme each, a tab to its stop).
#[derive(Clone, Copy, Debug, Default)]
pub struct Columns;

impl Placement for Columns {
    fn x_of(&self, buffer: &Buffer, at: usize) -> f32 {
        let line = buffer.text.line_of(at);
        let start = buffer.text.line_start(line);
        column_of(
            &buffer.text.line(line),
            at - start.min(at),
            buffer.tab_columns(),
        ) as f32
    }

    fn row_from(&self, buffer: &Buffer, at: usize, rows: isize, x: f32) -> Option<usize> {
        let mut line = buffer.text.line_of(at);
        let hidden = buffer.hidden_lines();
        let is_hidden = |line: usize| hidden.iter().any(|(a, b)| line >= *a && line <= *b);
        for _ in 0..rows.unsigned_abs() {
            loop {
                line = if rows < 0 {
                    line.checked_sub(1)?
                } else {
                    let next = line + 1;
                    if next >= buffer.text.len_lines() {
                        return None;
                    }
                    next
                };
                if !is_hidden(line) {
                    break;
                }
            }
        }
        let column = x.max(0.0).round() as usize;
        Some(
            buffer.text.line_start(line)
                + at_column(&buffer.text.line(line), column, buffer.tab_columns()),
        )
    }

    fn page_rows(&self) -> usize {
        20
    }
}

/// An editor's document and its state.
#[derive(Clone, Debug)]
pub struct Buffer {
    pub(crate) text: Text,
    pub(crate) version: u64,
    pub(crate) log: Log,
    pub(crate) ranges: Vec<Range>,
    pub(crate) main: usize,
    /// For each range, where across a move up or down keeps going back to.
    pub(crate) goals: Vec<Option<f32>>,
    pub(crate) undo: Undo,
    pub(crate) decorations: Decorations,
    pub(crate) options: EditorOptions,
    pub(crate) marked: Option<Marked>,
    pub(crate) found: Option<Found>,
    notes: Vec<Note>,
    /// The selections changed since the page was last told.
    selection_moved: bool,
    /// A load of several pieces is under way: the person cannot change the text until it ends.
    pub(crate) loading: bool,
    /// The most bytes the document may come to now.
    room: usize,
}

impl Default for Buffer {
    fn default() -> Self {
        Self {
            text: Text::default(),
            version: 0,
            log: Log::new(0),
            ranges: vec![Range::caret(0)],
            main: 0,
            goals: vec![None],
            undo: Undo::default(),
            decorations: Decorations::default(),
            options: EditorOptions::default(),
            marked: None,
            found: None,
            notes: Vec::new(),
            selection_moved: false,
            loading: false,
            room: limits::DOCUMENT_BYTES,
        }
    }
}

impl Buffer {
    /// An editor holding `text`, at version 0, a caret at its start; refused where the text is
    /// larger than a document may be.
    pub fn new(text: &str) -> Result<Self, Refusal> {
        if text.len() > limits::DOCUMENT_BYTES {
            return Err(Refusal::TooLarge);
        }
        Ok(Self {
            text: Text::new(text),
            ..Self::default()
        })
    }

    // --- What is there ---------------------------------------------------------------------

    /// The whole text.
    pub fn text(&self) -> String {
        self.text.string()
    }

    /// The characters `from..to`.
    pub fn slice(&self, from: usize, to: usize) -> Option<String> {
        self.text.slice(from, to)
    }

    pub fn len_chars(&self) -> usize {
        self.text.len_chars()
    }

    pub fn len_bytes(&self) -> usize {
        self.text.len_bytes()
    }

    pub fn len_lines(&self) -> usize {
        self.text.len_lines()
    }

    /// The text of line `line`, without its line break.
    pub fn line(&self, line: usize) -> String {
        self.text.line(line)
    }

    /// Where line `line` starts.
    pub fn line_start(&self, line: usize) -> usize {
        self.text.line_start(line)
    }

    /// Where line `line` ends (the place of its line break).
    pub fn line_end(&self, line: usize) -> usize {
        self.text.line_end(line)
    }

    /// The line the place `at` is on.
    pub fn line_of(&self, at: usize) -> usize {
        self.text.line_of(at)
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn options(&self) -> &EditorOptions {
        &self.options
    }

    /// How the editor is set up: what the page says each frame.
    pub fn set_options(&mut self, options: EditorOptions) {
        self.options = options;
    }

    /// The most bytes the document may come to (an editor's own limit, or less where the page's
    /// other editors hold the rest of what a page may).
    pub fn set_room(&mut self, bytes: usize) {
        self.room = bytes.min(limits::DOCUMENT_BYTES);
    }

    /// The text as it is now, for the page to read.
    pub fn snapshot(&self) -> Snapshot {
        self.text.snapshot(self.version)
    }

    /// The selections.
    pub fn selection(&self) -> Selection {
        Selection {
            ranges: self.ranges.clone(),
            main: self.main,
            version: self.version,
            composing: self.marked.is_some(),
        }
    }

    pub fn ranges(&self) -> &[Range] {
        &self.ranges
    }

    /// The main range.
    pub fn main(&self) -> Range {
        self.ranges.get(self.main).copied().unwrap_or_default()
    }

    /// The range an input method is composing in, where one is.
    pub fn marked_range(&self) -> Option<(usize, usize)> {
        self.marked.as_ref().map(|marked| (marked.from, marked.to))
    }

    pub fn composing(&self) -> bool {
        self.marked.is_some()
    }

    /// What the search last asked for found, in order.
    pub fn found(&self) -> &[(usize, usize)] {
        self.found
            .as_ref()
            .map_or(&[], |found| found.ranges.as_slice())
    }

    /// The decorations of every layer that overlap `from..to`, each with the place of its layer.
    pub fn decorations_in(&self, from: usize, to: usize) -> Vec<(usize, &Decoration)> {
        self.decorations.in_range(from, to)
    }

    /// The names of the layers of decorations, in the order they came: what
    /// [`Buffer::decorations_in`] says a decoration's layer is by.
    pub fn layer_names(&self) -> Vec<String> {
        self.decorations
            .layers
            .iter()
            .map(|layer| layer.name.clone())
            .collect()
    }

    /// The regions that fold, and the ones folded.
    pub fn folds(&self) -> Vec<(usize, usize)> {
        self.decorations.folds()
    }

    pub fn folded(&self) -> &[(usize, usize)] {
        &self.decorations.folded
    }

    /// The lines folded away: for each folded region, its first hidden line and its last.
    pub fn hidden_lines(&self) -> Vec<(usize, usize)> {
        self.decorations
            .folded
            .iter()
            .filter_map(|(from, to)| {
                let first = self.text.line_of(*from);
                let last = self.text.line_of(*to);
                (last > first).then_some((first + 1, last))
            })
            .collect()
    }

    /// The columns a tab takes.
    pub(crate) fn tab_columns(&self) -> usize {
        usize::from(self.options.tab_width.max(1))
    }

    /// The place `at` in units of UTF-16, where the text has it.
    pub fn utf16_of(&self, at: usize) -> Option<usize> {
        self.text.utf16_of(at)
    }

    /// The place `units` of UTF-16 into the text.
    pub fn place_of_utf16(&self, units: usize) -> Option<usize> {
        self.text.place_of_utf16(units)
    }

    /// What there is to tell the page, oldest first; the selections last, where they moved.
    pub fn take_notes(&mut self) -> Vec<Note> {
        let mut notes = std::mem::take(&mut self.notes);
        if std::mem::take(&mut self.selection_moved) {
            notes.push(Note::Selection(self.selection()));
        }
        notes
    }

    pub(crate) fn refuse(&mut self, refusal: Refusal) -> Refusal {
        self.notes.push(Note::Refused(refusal));
        refusal
    }

    // --- Changing the text ------------------------------------------------------------------

    /// Does `changes` (a set against the document as it is), as `origin` made it: the text, the
    /// version, the log, the decorations, what was found and what is composed. The selections are
    /// the caller's to put. Returns what was done; nothing, for a set that changes nothing.
    pub(crate) fn apply(
        &mut self,
        changes: Vec<Change>,
        origin: Origin,
    ) -> Result<Vec<Done>, Refusal> {
        let len = self.text.len_chars();
        check(&changes, len)?;
        let changes: Vec<Change> = changes
            .into_iter()
            .filter(|change| change.from != change.to || !change.insert.is_empty())
            .collect();
        if changes.is_empty() {
            return Ok(Vec::new());
        }
        let mut bytes = self.text.len_bytes();
        for change in &changes {
            bytes -= self.text.bytes_of(change.from, change.to).unwrap_or(0);
            bytes += change.insert.len();
        }
        if bytes > self.room {
            return Err(Refusal::TooLarge);
        }
        self.unfold_touched(&changes);
        let done: Vec<Done> = changes
            .iter()
            .map(|change| Done {
                from: change.from,
                to: change.to,
                removed: self.text.slice(change.from, change.to).unwrap_or_default(),
                insert: change.insert.clone(),
            })
            .collect();
        for change in changes.iter().rev() {
            // Checked above: every range is the text's.
            self.text.replace(change.from, change.to, &change.insert);
        }
        let steps = steps(&changes);
        self.log.push(steps.clone(), len);
        self.version += 1;
        self.decorations.map(&steps);
        if let Some(found) = &mut self.found {
            found.map(&steps);
        }
        if origin != Origin::Compose
            && let Some(marked) = &mut self.marked
        {
            marked.from = map(&steps, marked.from, Assoc::Before);
            marked.to = map(&steps, marked.to, Assoc::After).max(marked.from);
        }
        self.notes.push(Note::Edited(Edited {
            version: self.version,
            changes,
            origin,
        }));
        Ok(done)
    }

    /// Does `changes` as the person or the page made it, remembered to be undone (`kind` says what
    /// it may be undone with), and puts the selections at `after` — or, where none is given, where
    /// they were, carried over the changes.
    pub(crate) fn edit(
        &mut self,
        changes: Vec<Change>,
        origin: Origin,
        kind: Kind,
        now: f64,
        after: Option<(Vec<Range>, usize)>,
    ) -> Result<(), Refusal> {
        let before = (self.ranges.clone(), self.main);
        let len_before = self.text.len_chars();
        let start = self.version;
        let done = self.apply(changes, origin)?;
        if done.is_empty() {
            if let Some((ranges, main)) = after {
                self.set_ranges(ranges, main);
            }
            return Ok(());
        }
        self.undo.record(Group {
            done: done.clone(),
            len_before,
            start,
            end: self.version,
            before: before.0,
            main_before: before.1,
            at: now,
            kind,
            open: true,
        });
        match after {
            Some((ranges, main)) => self.set_ranges(ranges, main),
            None => {
                let steps = steps_done(&done);
                self.map_ranges(&steps, Assoc::After);
            }
        }
        Ok(())
    }

    /// The regions folded whose hidden lines `changes` touch, unfolded.
    fn unfold_touched(&mut self, changes: &[Change]) {
        let text = &self.text;
        self.decorations.folded.retain(|(from, to)| {
            let hidden = text.line_end(text.line_of(*from));
            !changes.iter().any(|change| {
                (change.from < *to && change.to > hidden)
                    || (change.from == change.to && change.from > hidden && change.from <= *to)
            })
        });
    }

    // --- The selections ---------------------------------------------------------------------

    /// Puts the selections at `ranges`, the main one at `main`: inside the text, in order, and
    /// those that overlap made one.
    pub(crate) fn set_ranges(&mut self, ranges: Vec<Range>, main: usize) {
        let len = self.text.len_chars();
        let mut ranges: Vec<(Range, bool)> = ranges
            .into_iter()
            .enumerate()
            .map(|(index, range)| {
                (
                    Range::new(range.anchor.min(len), range.head.min(len)),
                    index == main,
                )
            })
            .collect();
        if ranges.is_empty() {
            ranges.push((Range::caret(0), true));
        }
        ranges.sort_by_key(|(range, _)| (range.start(), range.end()));
        let mut merged: Vec<(Range, bool)> = Vec::with_capacity(ranges.len());
        for (range, is_main) in ranges {
            if let Some((last, last_main)) = merged.last_mut() {
                let touches = range.start() == last.start()
                    || range.start() < last.end()
                    || (range.start() == last.end() && (range.is_empty() || last.is_empty()));
                if touches {
                    let (start, end) = (last.start(), last.end().max(range.end()));
                    *last = if last.anchor <= last.head {
                        Range::new(start, end)
                    } else {
                        Range::new(end, start)
                    };
                    *last_main |= is_main;
                    continue;
                }
            }
            merged.push((range, is_main));
        }
        let main = merged.iter().position(|(_, is_main)| *is_main).unwrap_or(0);
        let ranges: Vec<Range> = merged.into_iter().map(|(range, _)| range).collect();
        if ranges != self.ranges || main != self.main {
            self.selection_moved = true;
        }
        self.goals = vec![None; ranges.len()];
        self.ranges = ranges;
        self.main = main;
        self.reveal();
    }

    /// Every range carried over `steps`.
    fn map_ranges(&mut self, steps: &[Step], assoc: Assoc) {
        let ranges: Vec<Range> = self
            .ranges
            .iter()
            .map(|range| {
                Range::new(
                    map(steps, range.anchor, assoc),
                    map(steps, range.head, assoc),
                )
            })
            .collect();
        let main = self.main;
        self.set_ranges(ranges, main);
    }

    /// A folded region a caret or an end of a range went into, unfolded.
    fn reveal(&mut self) {
        let text = &self.text;
        let ranges = &self.ranges;
        self.decorations.folded.retain(|(from, to)| {
            let hidden = text.line_end(text.line_of(*from));
            !ranges.iter().any(|range| {
                [range.anchor, range.head]
                    .iter()
                    .any(|at| *at > hidden && *at <= *to)
            })
        });
    }

    /// The person put the selections here (a click, a drag): what they type next is undone apart
    /// from what they typed before.
    pub fn select(&mut self, ranges: Vec<Range>, main: usize) {
        self.undo.close();
        self.set_ranges(ranges, main);
    }

    // --- What the page asks -----------------------------------------------------------------

    /// The page's changes, against the document at `base`: carried over what was done since and
    /// done, to be undone as one. The selections are carried over them.
    pub fn change(&mut self, base: u64, changes: Vec<Change>, now: f64) -> Result<(), Refusal> {
        let result = self.page_change(base, changes, now);
        result.map_err(|refusal| self.refuse(refusal))
    }

    fn page_change(&mut self, base: u64, changes: Vec<Change>, now: f64) -> Result<(), Refusal> {
        let len = self.log.len_at(base, self.text.len_chars())?;
        check(&changes, len)?;
        let since = self.log.since(base)?;
        let changes = rebase(changes, since.iter().copied());
        self.undo.close();
        self.edit(changes, Origin::Page, Kind::Other, now, None)?;
        self.undo.close();
        Ok(())
    }

    /// The page's selections, in the places of `version`.
    pub fn select_at(
        &mut self,
        version: u64,
        ranges: Vec<Range>,
        main: usize,
    ) -> Result<(), Refusal> {
        let result = self.page_select(version, ranges, main);
        result.map_err(|refusal| self.refuse(refusal))
    }

    fn page_select(
        &mut self,
        version: u64,
        ranges: Vec<Range>,
        main: usize,
    ) -> Result<(), Refusal> {
        let len = self.log.len_at(version, self.text.len_chars())?;
        if ranges
            .iter()
            .any(|range| range.anchor > len || range.head > len)
        {
            return Err(Refusal::OutOfDocument);
        }
        let since = self.log.since(version)?;
        let carry = |at: usize| {
            since
                .iter()
                .fold(at, |at, steps| map(steps, at, Assoc::After))
        };
        let ranges = ranges
            .into_iter()
            .map(|range| Range::new(carry(range.anchor), carry(range.head)))
            .collect();
        self.select(ranges, main);
        Ok(())
    }

    /// The page's decorations of a layer for a range of the document.
    pub fn decorate(&mut self, decorate: Decorate) -> Result<(), Refusal> {
        let result = self.page_decorate(decorate);
        result.map_err(|refusal| self.refuse(refusal))
    }

    fn page_decorate(&mut self, decorate: Decorate) -> Result<(), Refusal> {
        let Decorate {
            layer,
            version,
            from,
            to,
            decorations,
        } = decorate;
        let len = self.log.len_at(version, self.text.len_chars())?;
        if from > to {
            return Err(Refusal::Reversed);
        }
        if to > len {
            return Err(Refusal::OutOfDocument);
        }
        if decorations.len() > limits::REPLY_DECORATIONS {
            return Err(Refusal::TooManyDecorations);
        }
        for item in &decorations {
            if item.from > item.to {
                return Err(Refusal::Reversed);
            }
            if item.from < from || item.to > to {
                return Err(Refusal::OutsideRange);
            }
        }
        let since = self.log.since(version)?;
        let carry =
            |at: usize, assoc: Assoc| since.iter().fold(at, |at, steps| map(steps, at, assoc));
        let new_from = carry(from, Assoc::After);
        let new_to = carry(to, Assoc::Before).max(new_from);
        let items: Vec<Decoration> = decorations
            .into_iter()
            .filter_map(|mut item| {
                if item.from == item.to {
                    let at = carry(item.from, Assoc::Before);
                    item.from = at;
                    item.to = at;
                    return Some(item);
                }
                item.from = carry(item.from, Assoc::After);
                item.to = carry(item.to, Assoc::Before);
                (item.from < item.to).then_some(item)
            })
            .collect();
        self.decorations.replace(&layer, new_from, new_to, items)?;
        let text = &self.text;
        self.decorations.keep_folded(&|at| text.line_of(at));
        Ok(())
    }

    /// A piece of the document the page loads: the first takes the place of the whole text, each
    /// after it (`more` said of the one before) goes on its end. Nothing of it can be undone, and
    /// what could be undone before cannot be any more.
    pub fn load(&mut self, text: &str, more: bool) -> Result<(), Refusal> {
        let result = self.page_load(text, more);
        result.map_err(|refusal| self.refuse(refusal))
    }

    fn page_load(&mut self, text: &str, more: bool) -> Result<(), Refusal> {
        let len = self.text.len_chars();
        let first = !self.loading;
        let change = if first {
            Change::new(0, len, text)
        } else {
            Change::new(len, len, text)
        };
        if first {
            self.marked = None;
        }
        self.apply(vec![change], Origin::Load)?;
        self.undo.clear();
        self.loading = more;
        if first {
            self.set_ranges(vec![Range::caret(0)], 0);
        }
        Ok(())
    }

    // --- Undoing ----------------------------------------------------------------------------

    /// Takes back the last group of changes: carried over what was done after it, where something
    /// was. The selections go back to where they were before it, where nothing was.
    pub fn undo(&mut self, _now: f64) {
        if self.options.read_only || self.marked.is_some() {
            return;
        }
        let Some(group) = self.undo.undo.pop() else {
            return;
        };
        let Ok(since) = self.log.since(group.end) else {
            // Too far back to carry over: nothing before it can be undone either.
            self.undo.clear();
            return;
        };
        let alone = since.is_empty();
        let changes = rebase(invert(&group.done), since.iter().copied());
        let len_before = self.text.len_chars();
        let start = self.version;
        let selection = (self.ranges.clone(), self.main);
        let Ok(done) = self.apply(changes, Origin::Undo) else {
            return;
        };
        if alone {
            self.set_ranges(group.before.clone(), group.main_before);
            Undo::same_as(&mut self.undo.undo, &group, self.version);
        } else {
            let ends = done_ends(&done);
            self.set_ranges(ends, 0);
        }
        self.undo.redo.push(Group {
            done,
            len_before,
            start,
            end: self.version,
            before: selection.0,
            main_before: selection.1,
            at: group.at,
            kind: Kind::Other,
            open: false,
        });
    }

    /// Does again the last group undone.
    pub fn redo(&mut self, now: f64) {
        if self.options.read_only || self.marked.is_some() {
            return;
        }
        let Some(group) = self.undo.redo.pop() else {
            return;
        };
        let Ok(since) = self.log.since(group.end) else {
            self.undo.redo.clear();
            return;
        };
        let alone = since.is_empty();
        let changes = rebase(invert(&group.done), since.iter().copied());
        let len_before = self.text.len_chars();
        let start = self.version;
        let selection = (self.ranges.clone(), self.main);
        let Ok(done) = self.apply(changes, Origin::Redo) else {
            return;
        };
        let ends = done_ends(&done);
        self.set_ranges(ends, 0);
        if alone {
            Undo::same_as(&mut self.undo.redo, &group, self.version);
        }
        self.undo.close();
        self.undo.undo.push(Group {
            done,
            len_before,
            start,
            end: self.version,
            before: selection.0,
            main_before: selection.1,
            at: now,
            kind: Kind::Other,
            open: false,
        });
    }

    // --- An input method --------------------------------------------------------------------

    /// An input method composes `text` (what it has so far), in place of what it composed before,
    /// or of `replace` (a range it names), or of the main selection; `selected` is the part of the
    /// text it says is selected, characters into it. An empty text ends the composing, with nothing
    /// in its place.
    pub fn set_marked(
        &mut self,
        text: &str,
        selected: (usize, usize),
        replace: Option<(usize, usize)>,
        now: f64,
    ) {
        if self.options.read_only || self.loading || (self.marked.is_none() && text.is_empty()) {
            return;
        }
        let main = self.main();
        let (from, to, original) = match &self.marked {
            Some(marked) => (marked.from, marked.to, marked.original.clone()),
            None => {
                let (from, to) = replace.unwrap_or((main.start(), main.end()));
                let original = self.text.slice(from, to).unwrap_or_default();
                (from, to, original)
            }
        };
        if from > to || to > self.text.len_chars() {
            return;
        }
        let len = text.chars().count();
        let caret = (from + selected.0.min(len), from + selected.1.min(len));
        // The other ranges stay where they were, carried over the change.
        let others: Vec<Range> = self
            .ranges
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != self.main)
            .map(|(_, range)| *range)
            .collect();
        let steps = [Step { from, to, len }];
        let mut ranges: Vec<Range> = others
            .iter()
            .map(|range| {
                Range::new(
                    map(&steps, range.anchor, Assoc::After),
                    map(&steps, range.head, Assoc::After),
                )
            })
            .collect();
        ranges.push(Range::new(caret.0, caret.1));
        let main = ranges.len() - 1;
        if self.marked.is_none() {
            self.undo.close();
        }
        let result = self.edit(
            vec![Change::new(from, to, text)],
            Origin::Compose,
            Kind::Compose,
            now,
            Some((ranges, main)),
        );
        if result.is_err() {
            return;
        }
        self.marked = (len > 0).then(|| Marked {
            from,
            to: from + len,
            original,
        });
        if self.marked.is_none() {
            self.end_composing();
        }
        self.selection_moved = true;
    }

    /// The input method is done: what it composed is what the person typed.
    pub fn unmark(&mut self) {
        if self.marked.take().is_some() {
            self.end_composing();
            self.selection_moved = true;
        }
    }

    /// The input method gave up: the range goes back to what it held before it began.
    pub fn abort_marked(&mut self, now: f64) {
        let Some(marked) = self.marked.take() else {
            return;
        };
        let back = marked.from + marked.original.chars().count();
        let _ = self.edit(
            vec![Change::new(marked.from, marked.to, marked.original)],
            Origin::Compose,
            Kind::Compose,
            now,
            Some((vec![Range::caret(back)], 0)),
        );
        // Composed and taken back, there is nothing to undo.
        if self
            .undo
            .undo
            .last()
            .is_some_and(|top| top.kind == Kind::Compose && top.done.is_empty())
        {
            self.undo.undo.pop();
        }
        self.end_composing();
        self.selection_moved = true;
    }

    /// What was composed is undone as the person's typing, apart from what comes after it.
    fn end_composing(&mut self) {
        if let Some(top) = self.undo.undo.last_mut()
            && top.kind == Kind::Compose
        {
            top.kind = Kind::Typing;
            top.open = false;
        }
    }

    /// The input method puts `text` in for good: in place of what it composed, or of `replace`, or
    /// as the person typing it.
    pub fn insert(&mut self, text: &str, replace: Option<(usize, usize)>, now: f64) {
        if self.options.read_only || self.loading {
            return;
        }
        if let Some(marked) = self.marked.clone() {
            let end = marked.from + text.chars().count();
            let _ = self.edit(
                vec![Change::new(marked.from, marked.to, text)],
                Origin::Person,
                Kind::Compose,
                now,
                Some((vec![Range::caret(end)], 0)),
            );
            self.marked = None;
            self.end_composing();
            self.selection_moved = true;
            return;
        }
        if let Some((from, to)) = replace.filter(|(from, to)| from <= to) {
            let end = from + text.chars().count();
            let _ = self.edit(
                vec![Change::new(from, to, text)],
                Origin::Person,
                Kind::Typing,
                now,
                Some((vec![Range::caret(end)], 0)),
            );
            return;
        }
        self.type_text(text, now);
    }
}

/// A caret at the end of each change done, in the places after them.
fn done_ends(done: &[Done]) -> Vec<Range> {
    let mut gained: i128 = 0;
    let mut out = Vec::with_capacity(done.len());
    for change in done {
        let len = change.insert.chars().count();
        let end = usize::try_from(change.from as i128 + gained).unwrap_or(0) + len;
        out.push(Range::caret(end));
        gained += len as i128 - (change.to - change.from) as i128;
    }
    if out.is_empty() {
        out.push(Range::caret(0));
    }
    out
}
