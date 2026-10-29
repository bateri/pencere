//! What the pointer rests on: a word of an editor's text it stays over for the delay the page set
//! ([`pencere_view::edit::HoverOptions`]) is told to the page (`Dwell`), and its going from it
//! (`Leave`), where the editor hears of it ([`pencere_view::edit::EditorEvents::hover`]). The
//! editor has no clock: the drive reads the time the rest is due at ([`Editors::rest_due`]) and
//! gives it back at the frame ([`Editors::tick`]).
//!
//! The rest is only the pointer's and the word's: what a scroll, a key, a press, an edit or the
//! window's losing the keyboard does ends it (the page is told it left, where it was told of it)
//! and none begins again until the pointer moves.

use pencere_view::edit::{HoverEvent, HoverHit, HoverKind};
use pencere_view::{Id, Laid, Measure, Mods, Point, Rect};

use super::{EditorEvent, Editors, scroll_of};

/// What the pointer rests on, and what the page was told of it.
#[derive(Debug, Default)]
pub(super) struct Hovering {
    rest: Option<Rest>,
    /// A scroll, a key, a press, an edit or the loss of the keyboard ended the rest: nothing is
    /// told until the pointer moves.
    quiet: bool,
    /// The `Dwell` the page was told of, from which its `Leave` is made.
    told: Option<HoverEvent>,
    /// The modifiers held as the drive last knew.
    mods: Mods,
}

/// The word the pointer is over, and since when.
#[derive(Clone, Debug, PartialEq)]
struct Rest {
    editor: Id,
    at: usize,
    from: usize,
    to: usize,
    /// Pixels of the content.
    rect: Rect,
    since: f64,
    /// What it was found at: a scroll or an edit changes what is under the pointer.
    scroll: Point,
    version: u64,
}

impl Editors {
    /// The pointer is at `at` (points of the page), or gone (`None`), at `now`, with `mods` held and a
    /// button held or not: what it rests on is found, and a rest that ended is told. Called at every
    /// move of the pointer.
    pub fn rest(
        &mut self,
        laid: &Laid,
        at: Option<Point>,
        mods: Mods,
        now: f64,
        measure: &mut dyn Measure,
    ) {
        self.hovering.mods = mods;
        self.now = now;
        self.pointer = at;
        let was_quiet = std::mem::take(&mut self.hovering.quiet);
        let found = if self.drag.is_some() {
            None
        } else {
            at.and_then(|at| {
                let (id, point) = self.under(laid, at)?;
                let editor = self.editors.get(&id).filter(|editor| editor.shown)?;
                let word = editor.view.word_under(&editor.buffer, point, measure)?;
                Some(Rest {
                    editor: id,
                    at: word.at,
                    from: word.from,
                    to: word.to,
                    rect: word.rect,
                    since: now,
                    scroll: editor.view.scroll,
                    version: editor.buffer.version(),
                })
            })
        };
        match (&mut self.hovering.rest, found) {
            (Some(rest), Some(found))
                if rest.editor == found.editor
                    && (rest.from, rest.to) == (found.from, found.to) =>
            {
                rest.at = found.at;
                if was_quiet {
                    rest.since = now;
                }
            }
            (_, found) => {
                self.leave_rest();
                self.hovering.rest = found;
            }
        }
        self.retarget(laid, measure);
    }

    /// The time the pointer's rest is next to be told at, on the frames' clock; `None` where it is
    /// not resting on a word the page hears of, or has been told.
    pub fn rest_due(&self) -> Option<f64> {
        [self.rest_due_hover(), self.target_due(), self.chord_due()]
            .into_iter()
            .flatten()
            .reduce(f64::min)
    }

    fn rest_due_hover(&self) -> Option<f64> {
        let rest = self.hovering.rest.as_ref()?;
        if self.hovering.quiet || self.hovering.told.is_some() {
            return None;
        }
        let editor = self.editors.get(&rest.editor)?;
        editor
            .view
            .options
            .events
            .hover
            .then(|| rest.since + f64::from(editor.view.options.hover.delay))
    }

    /// The frame at `now`, `laid` the page: a rest whose time has come is told.
    pub fn tick(&mut self, laid: &Laid, now: f64, measure: &mut dyn Measure) {
        self.now = now;
        self.tick_chord(now);
        self.tick_target(laid, now, measure);
        if self.rest_due_hover().is_none_or(|due| now < due) {
            return;
        }
        let Some(rest) = self.hovering.rest.clone() else {
            return;
        };
        let Some(editor) = self.editors.get(&rest.editor) else {
            return;
        };
        let Some(room) = laid
            .editors()
            .into_iter()
            .find(|room| room.id == rest.editor)
        else {
            return;
        };
        let s = self.scale.max(0.01);
        let scroll = scroll_of(laid, &room);
        let names = editor.buffer.layer_names();
        let hits = editor
            .buffer
            .decorations_in(rest.at, rest.at + 1)
            .into_iter()
            .filter_map(|(layer, item)| {
                let pencere_view::edit::Mark::Style(look) = &item.mark else {
                    return None;
                };
                Some(HoverHit {
                    layer: names.get(layer).cloned().unwrap_or_default(),
                    id: look.id.clone()?,
                    from: item.from,
                    to: item.to,
                })
            })
            .collect();
        let event = HoverEvent {
            editor: rest.editor,
            kind: HoverKind::Dwell,
            at: rest.at,
            from: rest.from,
            to: rest.to,
            line: editor.buffer.line_of(rest.at),
            hits,
            rect: Rect::new(
                (room.pixels.x + rest.rect.x - scroll.x) / s,
                (room.pixels.y + rest.rect.y - scroll.y) / s,
                rest.rect.w / s,
                rest.rect.h / s,
            ),
            mods: self.hovering.mods,
            version: editor.buffer.version(),
        };
        self.hovering.told = Some(event.clone());
        self.events.push(EditorEvent::Hover(event));
    }

    /// What ends the pointer's rest (a key, a press, the window's losing the keyboard): where the
    /// page was told of it, it is told it left, and nothing is told until the pointer moves.
    pub fn end_rest(&mut self) {
        self.leave_rest();
        self.hovering.quiet = true;
    }

    /// What the pointer rests on is found again after the page was laid out: the text scrolled or
    /// changed under it, which ends the rest ([`Editors::end_rest`]); an editor that went takes it.
    pub(super) fn check_rest(&mut self) {
        let Some(rest) = &self.hovering.rest else {
            return;
        };
        let moved = self
            .editors
            .get(&rest.editor)
            .filter(|editor| editor.shown)
            .is_none_or(|editor| {
                editor.view.scroll != rest.scroll || editor.buffer.version() != rest.version
            });
        if moved {
            if let Some(rest) = &mut self.hovering.rest
                && let Some(editor) = self.editors.get(&rest.editor)
            {
                rest.scroll = editor.view.scroll;
                rest.version = editor.buffer.version();
            }
            self.end_rest();
        }
    }

    /// The page is told the pointer left the word it was told of, if it was.
    fn leave_rest(&mut self) {
        if let Some(told) = self.hovering.told.take() {
            self.events.push(EditorEvent::Hover(HoverEvent {
                kind: HoverKind::Leave,
                ..told
            }));
        }
    }

    /// The range of the word at `at` in the editor named `id`, by the rule a double click has;
    /// `None` for an editor that is not there or a place that is not in its text.
    pub fn word_at(&self, id: Id, at: usize) -> Option<(usize, usize)> {
        let editor = self.editors.get(&id)?;
        (at <= editor.buffer.len_chars()).then(|| editor.buffer.word_range(at))
    }
}
