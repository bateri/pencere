//! The pointer over a word with the modifiers of a press that is the page's held (⌘ and a click
//! that is a link), and that press: the page is told the word the pointer came to and left
//! (`Enter`, `Leave`), at most once in `links.delay` and only of the last it came to, and the press
//! (`Press`), where the page's `gestures` say a press is its own. What a word looks like while the
//! modifiers are held is no event: the looks that are `held` are drawn by the editor
//! ([`pencere_view::edit::Look::shows`]), so that the pointer's moving over words does not go to the
//! page at the rate it moves.
//!
//! The modifiers are those the drive knows ([`Editors::set_mods`]); the pointer is where it last
//! was ([`Editors::rest`]).

use pencere_view::edit::{Mark, Pressing, TargetEvent, TargetKind, TargetSource};
use pencere_view::{Button, Id, Laid, Measure, Mods, Point, Rect};

use super::{EditorEvent, Editors, scroll_of};
use crate::view::Word;

/// What the page was told of the pointer's coming to a word, and what waits to be.
#[derive(Debug, Default)]
pub(super) struct Targeting {
    /// The word the page was told the pointer is on.
    told: Option<TargetEvent>,
    /// A word the pointer came to that is not told yet, and when it is told at.
    due: Option<f64>,
    /// When the page was last told of a word.
    last: Option<f64>,
}

/// The modifiers of the press that is the page's (the first of the left button's that is `Target`
/// and is not part of a run), where there is one.
fn link_mods(gestures: &[(pencere_view::edit::Gesture, Pressing)]) -> Option<Mods> {
    gestures
        .iter()
        .find(|(gesture, pressing)| {
            *pressing == Pressing::Target && gesture.button == Button::Left && gesture.count == 1
        })
        .map(|(gesture, _)| gesture.mods)
}

impl Editors {
    /// The modifiers held are `mods` now (what the drive was told of them), at `now`: what is drawn
    /// by them is drawn again, and the pointer's coming to a word that is a link is told or taken
    /// back.
    pub fn set_mods(&mut self, laid: &Laid, mods: Mods, now: f64, measure: &mut dyn Measure) {
        self.now = now;
        if mods == self.mods {
            return;
        }
        self.mods = mods;
        for editor in self.editors.values_mut() {
            editor.view.mods = mods;
            if editor.buffer.gated() {
                self.redraw = true;
            }
            if editor.buffer.has_widgets() {
                // What shows of the widgets (and so how wide the lines are) goes by the modifiers.
                editor.view.counted_again();
                self.relayout = true;
            }
        }
        self.retarget(laid, measure);
    }

    /// The modifiers held, as the editors know them.
    pub fn mods(&self) -> Mods {
        self.mods
    }

    /// What the pointer is over is found again, as the text, the scroll or the modifiers moved it:
    /// the character each editor draws its looks by, and the word of a link told of or taken back.
    pub(super) fn retarget(&mut self, laid: &Laid, measure: &mut dyn Measure) {
        let over = self.pointer.and_then(|at| self.under(laid, at));
        let mut found: Option<(Id, Word)> = None;
        for id in self.order.clone() {
            let Some(editor) = self.editors.get_mut(&id) else {
                continue;
            };
            let point = over.filter(|(over, _)| *over == id).map(|(_, point)| point);
            let char_at = point
                .filter(|_| editor.shown)
                .and_then(|point| editor.view.char_under(&editor.buffer, point, measure));
            if editor.view.pointer_char != char_at {
                editor.view.pointer_char = char_at;
                if editor.buffer.gated() {
                    self.redraw = true;
                }
            }
            let armed = editor.view.options.events.target
                && link_mods(&editor.view.options.gestures)
                    .is_some_and(|wanted| wanted == self.mods);
            if let (true, Some(point)) = (armed, point) {
                found = editor
                    .view
                    .word_under(&editor.buffer, point, measure)
                    .map(|word| (id, word));
            }
        }
        let same = |told: &TargetEvent, word: &(Id, Word)| {
            told.editor == word.0 && (told.from, told.to) == (word.1.from, word.1.to)
        };
        let Some(word) = found else {
            self.targeting.due = None;
            self.leave_target();
            return;
        };
        if self
            .targeting
            .told
            .as_ref()
            .is_some_and(|told| same(told, &word))
        {
            self.targeting.due = None;
            return;
        }
        self.leave_target();
        let delay = self
            .editors
            .get(&word.0)
            .map_or(0.0, |editor| f64::from(editor.view.options.links.delay));
        let due = self.targeting.last.map_or(self.now, |last| last + delay);
        if due <= self.now {
            self.enter_target(laid, word.0, word.1);
        } else {
            self.targeting.due = Some(due);
        }
    }

    /// The time a word waiting to be told of is told at.
    pub(super) fn target_due(&self) -> Option<f64> {
        self.targeting.due
    }

    /// The frame at `now`: a word waited on is told if it is still the one the pointer is on.
    pub(super) fn tick_target(&mut self, laid: &Laid, now: f64, measure: &mut dyn Measure) {
        self.now = now;
        if self.targeting.due.is_some_and(|due| now >= due) {
            self.targeting.due = None;
            self.retarget(laid, measure);
        }
    }

    fn enter_target(&mut self, laid: &Laid, id: Id, word: Word) {
        let Some(event) = self.target_event(laid, id, &word, TargetKind::Enter, TargetSource::Word)
        else {
            return;
        };
        self.targeting.last = Some(self.now);
        self.targeting.told = Some(event.clone());
        self.events.push(EditorEvent::Target(event));
    }

    /// The page is told the pointer left the word it was told of, if it was.
    pub(super) fn leave_target(&mut self) {
        if let Some(told) = self.targeting.told.take() {
            self.events.push(EditorEvent::Target(TargetEvent {
                kind: TargetKind::Leave,
                ..told
            }));
        }
    }

    fn target_event(
        &self,
        laid: &Laid,
        id: Id,
        word: &Word,
        kind: TargetKind,
        source: TargetSource,
    ) -> Option<TargetEvent> {
        let editor = self.editors.get(&id)?;
        let room = laid.editors().into_iter().find(|room| room.id == id)?;
        let s = self.scale.max(0.01);
        let scroll = scroll_of(laid, &room);
        Some(TargetEvent {
            editor: id,
            kind,
            source,
            at: word.at,
            from: word.from,
            to: word.to,
            id: None,
            rect: Rect::new(
                (room.pixels.x + word.rect.x - scroll.x) / s,
                (room.pixels.y + word.rect.y - scroll.y) / s,
                word.rect.w / s,
                word.rect.h / s,
            ),
            mods: self.mods,
            button: Button::Left,
            count: 0,
            version: editor.buffer.version(),
        })
    }

    /// A press on a widget (or a part of one) that has a name, in an editor that hears of presses: told,
    /// with whatever modifiers; no caret moves. Whether it was on one.
    pub(super) fn press_widget(
        &mut self,
        laid: &Laid,
        id: Id,
        point: Point,
        (button, mods, count): (Button, Mods, u32),
    ) -> bool {
        let Some(editor) = self.editors.get(&id) else {
            return false;
        };
        if !editor.view.options.events.target {
            return false;
        }
        let Some((found, part)) = editor.view.widget_under(point) else {
            return false;
        };
        let Some(name) = part
            .and_then(|part| found.placed.widget.parts.get(part))
            .and_then(|part| part.id.clone())
            .or_else(|| found.placed.widget.id.clone())
        else {
            return false;
        };
        let word = Word {
            at: found.placed.at,
            from: found.placed.at,
            to: found.placed.at,
            rect: found.rect,
        };
        if let Some(mut event) =
            self.target_event(laid, id, &word, TargetKind::Press, TargetSource::Widget)
        {
            event.mods = mods;
            event.button = button;
            event.count = count;
            event.id = Some(name);
            self.events.push(EditorEvent::Target(event));
        }
        true
    }

    /// A press that is the page's: told, as of the link it is on where the page drew one that is
    /// held (its range and its `id`), else of the word. Whether there was a place under it.
    pub(super) fn press_target(
        &mut self,
        laid: &Laid,
        id: Id,
        point: Point,
        (button, mods, count): (Button, Mods, u32),
        measure: &mut dyn Measure,
    ) -> bool {
        let Some(editor) = self.editors.get(&id) else {
            return false;
        };
        let Some(char_at) = editor.view.char_under(&editor.buffer, point, measure) else {
            return false;
        };
        // A look that is drawn now, by the modifiers of this press and the pointer being over it, and
        // has a name: the link.
        let link = editor
            .buffer
            .decorations_in(char_at, char_at + 1)
            .into_iter()
            .rev()
            .find_map(|(_, item)| match &item.mark {
                Mark::Style(look)
                    if look.id.is_some()
                        && look.held.is_some()
                        && look.shows(mods, true)
                        && item.from <= char_at
                        && item.to > char_at =>
                {
                    Some((item.from, item.to, look.id.clone()))
                }
                _ => None,
            });
        let word = match &link {
            Some((from, to, _)) => editor
                .view
                .range_box(&editor.buffer, *from, *to, measure)
                .map(|rect| Word {
                    at: char_at,
                    from: *from,
                    to: *to,
                    rect,
                }),
            None => editor.view.word_under(&editor.buffer, point, measure),
        };
        let Some(word) = word else {
            return false;
        };
        let source = if link.is_some() {
            TargetSource::Link
        } else {
            TargetSource::Word
        };
        if let Some(mut event) = self.target_event(laid, id, &word, TargetKind::Press, source) {
            event.mods = mods;
            event.button = button;
            event.count = count;
            event.id = link.and_then(|(_, _, name)| name);
            self.events.push(EditorEvent::Target(event));
        }
        true
    }
}
