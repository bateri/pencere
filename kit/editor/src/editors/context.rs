//! A press that opens a menu ([`pencere_view::edit::Pressing::Context`]), and the page's asks to
//! copy and cut (`copy`, `cut`).
//!
//! The caret goes as a text view puts it for a press that opens a menu (VS Code's rule): where the
//! press is outside every selection, to one caret there; inside one (at its ends too, and at a
//! caret), the selections stay. The editor has the keyboard. Nothing is told for it: the page is
//! told the press as it is any, and `Place::inside` says which it was.

use pencere_view::edit::{EditCommand, Range};
use pencere_view::{Id, Laid, Measure, Point};

use super::{EditorEvent, Editors};

impl Editors {
    /// Whether the place at `at` (points of the page) in an editor's text is in a selection or at a
    /// caret: a press that opens a menu keeps the selections where it is.
    pub fn selection_holds(&self, laid: &Laid, at: Point, measure: &mut dyn Measure) -> bool {
        let Some((id, point)) = self.under(laid, at) else {
            return false;
        };
        let Some(editor) = self.editors.get(&id) else {
            return false;
        };
        let place = editor.view.place_at(&editor.buffer, point, measure);
        editor
            .buffer
            .ranges()
            .iter()
            .any(|range| range.start() <= place && place <= range.end())
    }

    /// A press that opens a menu in the editor `id`, `point` in its room: the caret put where the
    /// press is unless that is in a selection, and the keyboard its.
    pub(super) fn press_context(
        &mut self,
        laid: &Laid,
        id: Id,
        point: Point,
        measure: &mut dyn Measure,
    ) {
        if self.focus != Some(id) {
            self.focus = Some(id);
            self.events.push(EditorEvent::Focus(Some(id)));
        }
        let Some(editor) = self.editors.get(&id) else {
            return;
        };
        let place = editor.view.place_at(&editor.buffer, point, measure);
        let inside = editor
            .buffer
            .ranges()
            .iter()
            .any(|range| range.start() <= place && place <= range.end());
        if !inside {
            self.with(id, laid, measure, |buffer, _| {
                buffer.select(vec![Range::caret(place)], 0);
            });
        }
    }

    /// The page asked the editor `id` to copy or cut: the text is for the host's pasteboard
    /// ([`Editors::take_copied`]), what a key's copy or cut gives.
    pub(super) fn copy_command(
        &mut self,
        id: Id,
        command: &EditCommand,
        laid: &Laid,
        measure: &mut dyn Measure,
        now: f64,
    ) {
        let text = if *command == EditCommand::Cut {
            self.with(id, laid, measure, |buffer, _| buffer.cut(now))
                .flatten()
        } else {
            self.editors
                .get(&id)
                .and_then(|editor| editor.buffer.copy_text())
        };
        if let Some(text) = text {
            self.copied.push(text);
        }
    }
}
