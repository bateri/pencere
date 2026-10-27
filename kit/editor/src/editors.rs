//! The editors of a page, as a host keeps them: one for each editor on the page ([`Laid::editors`]),
//! its document kept for as long as an editor of its name is there, hidden or not; what the person
//! does sent to the one it is for, in the order a key is for the page's commands, the editor's own
//! and the text; what the page asks done; and each drawn in its place.
//!
//! A host keeps one `Editors` for a page, and for every frame:
//!
//! 1. lays the page out measuring with it ([`Editors::measure`]), so that each editor's room scrolls
//!    over its content and what is anchored to its caret stands by it;
//! 2. [`Editors::sync`] with the page laid out: editors come and go with their rooms;
//! 3. paints the page and [`Editors::paint`] on the list: each editor's place is filled.
//!
//! What it has for the page ([`Editors::take_events`]) and for the host's scrolling
//! ([`Editors::take_scrolls`]) is taken after each thing the person does; where it says the page is
//! to be laid out again ([`Editors::wants_layout`]: a caret that an overlay stands by moved), the
//! host lays out the tree it has, asking nothing of the page.

use std::cell::RefCell;
use std::collections::HashMap;

use pencere_view::edit::{
    EditCommand, Edited, EditorAsk, Keys, Movement, Range, Refusal, Selection, limits,
};
use pencere_view::{
    DrawList, EditorMetrics, EditorRoom, Font, Id, Key, Laid, Measure, Metrics, Mods, Paint, Point,
    Rect, ScrollTo, Size,
};

use crate::buffer::{Buffer, Note};
use crate::text::Snapshot;
use crate::view::{Rows, View};

/// What an editor has to tell its page.
#[derive(Clone, Debug, PartialEq)]
pub enum EditorEvent {
    /// The document changed.
    Edited { id: Id, edited: Edited },
    /// The selections moved, or an input method began or stopped composing.
    Selection { id: Id, selection: Selection },
    /// What the page asked was not done.
    Refused { id: Id, refusal: Refusal },
    /// The lines in view are these now, the first and the last: what a page that colours only
    /// what is seen colours.
    Viewport { id: Id, first: usize, last: usize },
    /// The person clicked into an editor that did not have the keyboard: it has it now.
    Focus(Option<Id>),
}

/// Where a key goes ([`Editors::key`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    /// No editor has the keyboard: the key is the host's to do with as it does.
    NotEditor,
    /// It does the page's command of this name (the page's commands come first).
    Command(Id),
    /// It is the page's to read, as a key ([`pencere_view::Input::Key`]): no command of the page's
    /// and none of the editor's is it, and it types nothing, or the editor gives its keys to the page
    /// ([`Keys::Page`]).
    Page,
    /// The editor did it.
    Done,
    /// It types: the host hands it to its system's handling of text (an input method, which then
    /// says what is typed and composed: [`Editors::type_text`], [`Editors::set_marked`]). A host
    /// with none types the key's character.
    Type,
    /// The host puts this on its pasteboard: a copy.
    Copy(String),
    /// A cut: put on the pasteboard, it is out of the text.
    Cut(String),
    /// A paste: the host reads its pasteboard and hands the text over ([`Editors::paste`]).
    Paste,
}

/// What a press in an editor takes in as the pointer is dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Unit {
    Character,
    Word,
    Line,
    /// A column (⌥): the graphemes between two places across, on each line between.
    Column,
}

/// A press in an editor, while the button is held.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    id: Id,
    unit: Unit,
    /// What the press took: its range.
    anchor: (usize, usize),
    /// Where the press was, pixels of the content (for a column).
    from: Point,
    /// The ranges there were apart from the one being made (⌘ adds one).
    others: usize,
}

/// One editor: its document, its layout, and what was last said of it.
#[derive(Debug, Default)]
struct Editor {
    buffer: Buffer,
    view: View,
    shown: bool,
    metrics: EditorMetrics,
    /// The lines in view the page was last told.
    told: Option<(usize, usize)>,
    /// The main caret is to be brought into view.
    reveal: Option<usize>,
}

/// The editors of a page.
#[derive(Debug, Default)]
pub struct Editors {
    editors: HashMap<Id, Editor>,
    /// Their names in the order they are written.
    order: Vec<Id>,
    focus: Option<Id>,
    events: Vec<EditorEvent>,
    scrolls: Vec<ScrollTo>,
    relayout: bool,
    drag: Option<Drag>,
    scale: f32,
}

/// A host's measure, with what its editors laid out ([`Measure::editor`]).
pub struct Measured<'a> {
    measure: &'a mut dyn Measure,
    editors: &'a Editors,
}

impl Measure for Measured<'_> {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        self.measure.measure(font, text)
    }

    fn image(&mut self, src: &str) -> Option<Size> {
        self.measure.image(src)
    }

    fn offsets(&mut self, font: &Font, text: &str, at: &[usize]) -> Vec<f32> {
        self.measure.offsets(font, text, at)
    }

    fn editor(&mut self, id: Id) -> Option<EditorMetrics> {
        self.editors.metrics(id)
    }
}

impl Editors {
    pub fn new() -> Self {
        Self {
            scale: 1.0,
            ..Self::default()
        }
    }

    /// `measure`, saying what the editors laid out: what the page is laid out with.
    pub fn measure<'a>(&'a self, measure: &'a mut dyn Measure) -> Measured<'a> {
        Measured {
            measure,
            editors: self,
        }
    }

    /// What the editor named `id` laid out: where its caret is, how large its content is.
    pub fn metrics(&self, id: Id) -> Option<EditorMetrics> {
        self.editors
            .get(&id)
            .filter(|editor| editor.shown)
            .map(|editor| editor.metrics)
    }

    /// The editor named `id`'s document, where there is one.
    pub fn buffer(&self, id: Id) -> Option<&Buffer> {
        self.editors.get(&id).map(|editor| &editor.buffer)
    }

    /// The names of the editors, in the order they are written.
    pub fn ids(&self) -> &[Id] {
        &self.order
    }

    /// A copy of each editor's text, for the page to read.
    pub fn snapshots(&self) -> Vec<(Id, Snapshot)> {
        self.order
            .iter()
            .filter_map(|id| Some((*id, self.editors.get(id)?.buffer.snapshot())))
            .collect()
    }

    /// The editor that has the keyboard.
    pub fn focused(&self) -> Option<Id> {
        self.focus
    }

    /// The keyboard goes to the editor named `id` (the page said so: `Keyboard::Field(id)`), or to
    /// none of them. A name that is not an editor's is none.
    pub fn set_focus(&mut self, id: Option<Id>) {
        self.focus = id.filter(|id| self.editors.contains_key(id));
    }

    /// What there is to tell the page, oldest first.
    pub fn take_events(&mut self) -> Vec<EditorEvent> {
        std::mem::take(&mut self.events)
    }

    /// Where the editors ask to be scrolled (to keep a caret in view), for the host's scrolling.
    pub fn take_scrolls(&mut self) -> Vec<ScrollTo> {
        std::mem::take(&mut self.scrolls)
    }

    /// Whether the page is to be laid out again as it is: a caret that an overlay stands by moved,
    /// or the content of an editor grew or shrank, since it was laid out. Asking says it once.
    pub fn wants_layout(&mut self) -> bool {
        std::mem::take(&mut self.relayout)
    }

    // --- Each frame ------------------------------------------------------------------------

    /// After the page was laid out: an editor for each room there, new ones empty, those whose room
    /// went gone with their documents; each laid out where its room is scrolled.
    pub fn sync(&mut self, laid: &Laid, measure: &mut dyn Measure) {
        let rooms = laid.editors();
        self.scale = laid.scale();
        self.editors
            .retain(|id, _| rooms.iter().any(|room| room.id == *id));
        if self.focus.is_some_and(|id| !self.editors.contains_key(&id)) {
            self.focus = None;
        }
        self.order = rooms.iter().map(|room| room.id).collect();
        for room in &rooms {
            let editor = self.editors.entry(room.id).or_default();
            editor.buffer.set_options(room.options.clone());
            editor.view.set_room(room, self.scale, measure);
            editor.view.size_gutter(editor.buffer.len_lines());
            editor.shown = !room.hidden;
            let scroll = scroll_of(laid, room);
            if editor.shown {
                editor.view.lay_out(&editor.buffer, scroll, measure);
            }
            self.refresh(room.id, Some(laid), measure);
        }
        self.budget();
    }

    /// Each document's room: an editor's own, or what the page's others leave of what a page may.
    fn budget(&mut self) {
        let total: usize = self.editors.values().map(|e| e.buffer.len_bytes()).sum();
        for editor in self.editors.values_mut() {
            let others = total - editor.buffer.len_bytes();
            editor
                .buffer
                .set_room(limits::PAGE_DOCUMENTS_BYTES.saturating_sub(others));
        }
    }

    /// What is told of editor `id` after anything was done to it: its events, where its caret and
    /// its content are now (and whether the page is to be laid out again for them), where it is to
    /// be scrolled to keep its caret in view, and the lines in view.
    fn refresh(&mut self, id: Id, laid: Option<&Laid>, measure: &mut dyn Measure) {
        let scale = self.scale;
        let Some(editor) = self.editors.get_mut(&id) else {
            return;
        };
        for note in editor.buffer.take_notes() {
            self.events.push(match note {
                Note::Edited(edited) => EditorEvent::Edited { id, edited },
                Note::Selection(selection) => {
                    editor.reveal =
                        Some(selection.ranges.get(selection.main).map_or(0, |r| r.head));
                    EditorEvent::Selection { id, selection }
                }
                Note::Refused(refusal) => EditorEvent::Refused { id, refusal },
            });
        }
        if !editor.shown {
            return;
        }
        editor.view.size_gutter(editor.buffer.len_lines());
        editor.view.count(&editor.buffer);
        let main = editor.buffer.main().head;
        let metrics = EditorMetrics {
            caret: editor.view.caret_of(&editor.buffer, main, measure),
            content: editor.view.content(),
        };
        if metrics != editor.metrics {
            let anchored = laid.is_none_or(|laid| laid.anchored_to(id.caret()));
            if metrics.content != editor.metrics.content || anchored {
                self.relayout = true;
            }
            editor.metrics = metrics;
        }
        if let Some(at) = editor.reveal.take()
            && let Some(caret) = editor.view.caret_of(&editor.buffer, at, measure)
            && let Some(ask) = keep_in_view(&editor.view, id, caret, scale)
        {
            self.scrolls.push(ask);
        }
        let lines = editor.view.lines_in_view();
        if editor.told != Some(lines) {
            editor.told = Some(lines);
            self.events.push(EditorEvent::Viewport {
                id,
                first: lines.0,
                last: lines.1,
            });
        }
    }

    /// The page's draw list with each editor's place filled with what it draws: the carets where
    /// `caret_on` (the host blinks them).
    pub fn paint(&mut self, list: DrawList, measure: &mut dyn Measure, caret_on: bool) -> DrawList {
        let mut out = DrawList::default();
        for item in list.items {
            let Paint::Editor(slot) = item else {
                out.items.push(item);
                continue;
            };
            let focused = self.focus == Some(slot.id);
            let Some(editor) = self.editors.get_mut(&slot.id).filter(|e| e.shown) else {
                continue;
            };
            if editor.view.scroll != slot.scroll || editor.view.rows.is_empty() {
                editor.view.lay_out(&editor.buffer, slot.scroll, measure);
            }
            out.items
                .extend(editor.view.paint(&editor.buffer, &slot, focused, caret_on));
        }
        out
    }

    // --- The keyboard ----------------------------------------------------------------------

    /// A key pressed with `mods`, while the editor that has the keyboard is on the page `laid`:
    /// where it goes — the page's command for it first (while an input method is not composing), then
    /// the editor's own, then the text; or, where the editor gives its keys to the page, every one
    /// of them to the page.
    pub fn key(
        &mut self,
        laid: &Laid,
        key: Key,
        mods: Mods,
        measure: &mut dyn Measure,
        now: f64,
    ) -> Route {
        let Some(id) = self.focus else {
            return Route::NotEditor;
        };
        let Some(editor) = self.editors.get(&id).filter(|editor| editor.shown) else {
            return Route::NotEditor;
        };
        if editor.buffer.composing() {
            return Route::Type;
        }
        if let Some(command) = laid.command_for(key, mods) {
            return Route::Command(command);
        }
        if editor.buffer.options().keys == Keys::Page {
            return Route::Page;
        }
        let Some(action) = builtin(key, mods) else {
            return match key {
                Key::Character(_) if !mods.any_command() => Route::Type,
                _ => Route::Page,
            };
        };
        match action {
            Action::Copy => editor.buffer.copy_text().map_or(Route::Done, Route::Copy),
            Action::Cut => {
                let cut = self.with(id, laid, measure, |buffer, _| buffer.cut(now));
                cut.flatten().map_or(Route::Done, Route::Cut)
            }
            Action::Paste => Route::Paste,
            Action::Escape => {
                if editor.buffer.ranges().len() > 1 {
                    self.with(id, laid, measure, |buffer, _| {
                        buffer.command(EditCommand::SingleSelection, now);
                    });
                    Route::Done
                } else {
                    Route::Page
                }
            }
            Action::Tab => {
                self.with(id, laid, measure, |buffer, _| buffer.tab(now));
                Route::Done
            }
            Action::ToLineStart => {
                self.with(id, laid, measure, |buffer, rows| {
                    // Back to where the line starts, past its indent too.
                    let ranges = buffer
                        .ranges()
                        .iter()
                        .map(|range| {
                            let start = buffer.line_start(buffer.line_of(range.head));
                            if range.is_empty() {
                                Range::new(range.head, start)
                            } else {
                                *range
                            }
                        })
                        .collect();
                    let main = buffer.selection().main;
                    buffer.select(ranges, main);
                    buffer.command_in(EditCommand::DeleteBackward, now, rows);
                });
                Route::Done
            }
            Action::ToLineEnd => {
                self.with(id, laid, measure, |buffer, rows| {
                    let ranges = buffer
                        .ranges()
                        .iter()
                        .map(|range| {
                            let end = buffer.line_end(buffer.line_of(range.head));
                            if range.is_empty() {
                                Range::new(range.head, end)
                            } else {
                                *range
                            }
                        })
                        .collect();
                    let main = buffer.selection().main;
                    buffer.select(ranges, main);
                    buffer.command_in(EditCommand::DeleteForward, now, rows);
                });
                Route::Done
            }
            Action::Command(command) => {
                self.with(id, laid, measure, |buffer, rows| {
                    buffer.command_in(command, now, rows);
                });
                Route::Done
            }
        }
    }

    /// Does `change` to editor `id`'s document, its rows those of its layout, and tells what came of
    /// it.
    fn with<T>(
        &mut self,
        id: Id,
        laid: &Laid,
        measure: &mut dyn Measure,
        change: impl FnOnce(&mut Buffer, &Rows<'_>) -> T,
    ) -> Option<T> {
        let editor = self.editors.get_mut(&id)?;
        let Editor { buffer, view, .. } = editor;
        let rows = Rows {
            view,
            measure: RefCell::new(&mut *measure),
        };
        let out = change(buffer, &rows);
        if editor.shown {
            let scroll = editor.view.scroll;
            editor.view.lay_out(&editor.buffer, scroll, measure);
        }
        self.refresh(id, Some(laid), measure);
        self.budget();
        Some(out)
    }

    /// Text the person typed, from the host's handling of text, into the editor that has the
    /// keyboard: in place of what an input method composed, where it did.
    pub fn type_text(&mut self, laid: &Laid, text: &str, measure: &mut dyn Measure, now: f64) {
        if let Some(id) = self.focus {
            self.with(id, laid, measure, |buffer, _| {
                buffer.insert(text, None, now)
            });
        }
    }

    /// Text pasted into the editor that has the keyboard.
    pub fn paste(&mut self, laid: &Laid, text: &str, measure: &mut dyn Measure, now: f64) {
        if let Some(id) = self.focus {
            self.with(id, laid, measure, |buffer, _| buffer.paste(text, now));
        }
    }

    /// An input method composes `text` in the editor that has the keyboard (see
    /// [`Buffer::set_marked`]).
    pub fn set_marked(
        &mut self,
        laid: &Laid,
        text: &str,
        selected: (usize, usize),
        replace: Option<(usize, usize)>,
        measure: &mut dyn Measure,
        now: f64,
    ) {
        if let Some(id) = self.focus {
            self.with(id, laid, measure, |buffer, _| {
                buffer.set_marked(text, selected, replace, now);
            });
        }
    }

    /// The input method is done composing.
    pub fn unmark(&mut self, laid: &Laid, measure: &mut dyn Measure) {
        if let Some(id) = self.focus {
            self.with(id, laid, measure, |buffer, _| buffer.unmark());
        }
    }

    /// The input method gave up.
    pub fn abort_marked(&mut self, laid: &Laid, measure: &mut dyn Measure, now: f64) {
        if let Some(id) = self.focus {
            self.with(id, laid, measure, |buffer, _| buffer.abort_marked(now));
        }
    }

    // --- An input method's questions, in units of UTF-16 --------------------------------------

    /// The main selection of the editor that has the keyboard, as an input method of macOS asks
    /// for it: where it starts and how long it is, units of UTF-16.
    pub fn selected_utf16(&self) -> Option<(usize, usize)> {
        let buffer = self.focused_buffer()?;
        let main = buffer.main();
        let start = buffer.utf16_of(main.start())?;
        let end = buffer.utf16_of(main.end())?;
        Some((start, end - start))
    }

    /// What is composed, likewise.
    pub fn marked_utf16(&self) -> Option<(usize, usize)> {
        let buffer = self.focused_buffer()?;
        let (from, to) = buffer.marked_range()?;
        let start = buffer.utf16_of(from)?;
        Some((start, buffer.utf16_of(to)? - start))
    }

    /// The text of a range given in units of UTF-16 (cut to the document).
    pub fn text_utf16(&self, start: usize, len: usize) -> Option<String> {
        let buffer = self.focused_buffer()?;
        let total = buffer.utf16_of(buffer.len_chars())?;
        let from = buffer.place_of_utf16(start.min(total))?;
        let to = buffer.place_of_utf16((start + len).min(total))?;
        buffer.slice(from, to)
    }

    /// The places of a range given in units of UTF-16 (cut to the document).
    pub fn places_of_utf16(&self, start: usize, len: usize) -> Option<(usize, usize)> {
        let buffer = self.focused_buffer()?;
        let total = buffer.utf16_of(buffer.len_chars())?;
        Some((
            buffer.place_of_utf16(start.min(total))?,
            buffer.place_of_utf16((start + len).min(total))?,
        ))
    }

    /// The box, points of the page, of the place `units` of UTF-16 into the text of the editor that
    /// has the keyboard: where an input method opens its window.
    pub fn rect_utf16(&self, laid: &Laid, units: usize, measure: &mut dyn Measure) -> Option<Rect> {
        let id = self.focus?;
        let editor = self.editors.get(&id)?;
        let at = editor.buffer.place_of_utf16(units)?;
        let caret = editor.view.caret_of(&editor.buffer, at, measure)?;
        let room = laid.editors().into_iter().find(|room| room.id == id)?;
        let scroll = scroll_of(laid, &room);
        let s = self.scale.max(0.01);
        Some(Rect::new(
            (room.pixels.x + caret.x - scroll.x) / s,
            (room.pixels.y + caret.y - scroll.y) / s,
            caret.w / s,
            caret.h / s,
        ))
    }

    /// Text given by an input method with its ranges in units of UTF-16: what it composes, the part
    /// of it selected, and the range of the document it replaces.
    #[allow(clippy::too_many_arguments)]
    pub fn set_marked_utf16(
        &mut self,
        laid: &Laid,
        text: &str,
        selected: (usize, usize),
        replace: Option<(usize, usize)>,
        measure: &mut dyn Measure,
        now: f64,
    ) {
        let in_text = |units: usize| -> usize {
            let mut count = 0;
            for (chars, c) in text.chars().enumerate() {
                if count >= units {
                    return chars;
                }
                count += c.len_utf16();
            }
            text.chars().count()
        };
        let selected = (in_text(selected.0), in_text(selected.0 + selected.1));
        let replace = replace.and_then(|(start, len)| self.places_of_utf16(start, len));
        self.set_marked(laid, text, selected, replace, measure, now);
    }

    fn focused_buffer(&self) -> Option<&Buffer> {
        self.editors.get(&self.focus?).map(|editor| &editor.buffer)
    }

    // --- The pointer -----------------------------------------------------------------------

    /// The editor under `at` (points of the page) and the place in its document there: what a
    /// press over it says of where it is.
    pub fn place_at(
        &self,
        laid: &Laid,
        at: Point,
        measure: &mut dyn Measure,
    ) -> Option<(Id, usize)> {
        let (id, point) = self.under(laid, at)?;
        let editor = self.editors.get(&id)?;
        Some((id, editor.view.place_at(&editor.buffer, point, measure)))
    }

    /// The editor whose room is the topmost under `at`, and where `at` is in its content (pixels).
    fn under(&self, laid: &Laid, at: Point) -> Option<(Id, Point)> {
        let under = laid.ids_at(at);
        let room = laid
            .editors()
            .into_iter()
            .find(|room| !room.hidden && under.first() == Some(&room.id))?;
        let scroll = scroll_of(laid, &room);
        let s = laid.scale();
        Some((
            room.id,
            Point::new(
                at.x * s - room.pixels.x + scroll.x,
                at.y * s - room.pixels.y + scroll.y,
            ),
        ))
    }

    /// A press of the left button at `at` (points of the page), the `count`th of a quick run: in an
    /// editor's room it is the editor's (returns `true`) and it has the keyboard. A press puts the
    /// caret; with shift it selects to it, with ⌘ it adds a caret, with ⌥ it starts a column; two
    /// select a word, three a line. In the gutter, a press on a fold button folds or unfolds, on a
    /// number selects its line.
    pub fn press(
        &mut self,
        laid: &Laid,
        at: Point,
        mods: Mods,
        count: u32,
        measure: &mut dyn Measure,
    ) -> bool {
        let Some((id, point)) = self.under(laid, at) else {
            return false;
        };
        if self.focus != Some(id) {
            self.focus = Some(id);
            self.events.push(EditorEvent::Focus(Some(id)));
        }
        let Some(editor) = self.editors.get(&id) else {
            return true;
        };
        let room_x = point.x - editor.view.scroll.x;
        if let Some((line, button)) = editor.view.gutter_at(room_x, point.y) {
            self.with(id, laid, measure, |buffer, _| {
                if button && buffer.fold_at(line, None) {
                    return;
                }
                let (from, to) = (buffer.line_start(line), buffer.line_start(line + 1));
                buffer.select(vec![Range::new(from, to)], 0);
            });
            return true;
        }
        let place = editor.view.place_at(&editor.buffer, point, measure);
        let others = editor.buffer.ranges().len();
        let (unit, anchor) = match count {
            2 => (Unit::Word, editor.buffer.word_range(place)),
            n if n >= 3 => {
                let line = editor.buffer.line_of(place);
                (
                    Unit::Line,
                    (
                        editor.buffer.line_start(line),
                        editor.buffer.line_start(line + 1),
                    ),
                )
            }
            _ if mods.alt => (Unit::Column, (place, place)),
            _ => (Unit::Character, (place, place)),
        };
        let shift = mods.shift && count == 1;
        let add = mods.command;
        self.drag = Some(Drag {
            id,
            unit,
            anchor,
            from: point,
            others: if add { others } else { 0 },
        });
        self.with(id, laid, measure, |buffer, _| {
            let made = if shift {
                Range::new(buffer.main().anchor, place)
            } else {
                Range::new(anchor.0, anchor.1)
            };
            if add {
                let mut ranges = buffer.ranges().to_vec();
                ranges.push(made);
                let main = ranges.len() - 1;
                buffer.select(ranges, main);
            } else {
                buffer.select(vec![made], 0);
            }
        });
        true
    }

    /// The pointer moved to `at` with the button held: what a press in an editor took grows to it;
    /// past the room, the editor scrolls toward it.
    pub fn drag_to(&mut self, laid: &Laid, at: Point, measure: &mut dyn Measure) -> bool {
        let Some(drag) = self.drag else {
            return false;
        };
        let Some(room) = laid.editors().into_iter().find(|room| room.id == drag.id) else {
            return false;
        };
        let scroll = scroll_of(laid, &room);
        let s = laid.scale();
        let point = Point::new(
            at.x * s - room.pixels.x + scroll.x,
            at.y * s - room.pixels.y + scroll.y,
        );
        // Past the top or the bottom, the room scrolls by as far as the pointer is past it.
        let (above, below) = (at.y * s - room.pixels.y, at.y * s - room.pixels.bottom());
        let past = if above < 0.0 {
            above
        } else if below > 0.0 {
            below
        } else {
            0.0
        };
        if past != 0.0 {
            self.scrolls.push(ScrollTo {
                id: drag.id,
                x: None,
                y: Some(((scroll.y + past) / s).max(0.0)),
                animate: false,
            });
        }
        let Some(editor) = self.editors.get(&drag.id) else {
            return false;
        };
        let place = editor.view.place_at(&editor.buffer, point, measure);
        let column =
            (drag.unit == Unit::Column).then(|| column_ranges(editor, drag.from, point, measure));
        let others = drag.others;
        self.with(drag.id, laid, measure, |buffer, _| {
            let made: Vec<Range> = match drag.unit {
                Unit::Character => vec![Range::new(drag.anchor.0, place)],
                Unit::Word => {
                    let (from, to) = buffer.word_range(place);
                    vec![span(drag.anchor, (from, to), place)]
                }
                Unit::Line => {
                    let line = buffer.line_of(place);
                    let here = (buffer.line_start(line), buffer.line_start(line + 1));
                    vec![span(drag.anchor, here, place)]
                }
                Unit::Column => column.unwrap_or_default(),
            };
            let mut ranges: Vec<Range> = buffer.ranges().iter().take(others).copied().collect();
            ranges.extend(made);
            let main = ranges.len().saturating_sub(1);
            buffer.select(ranges, main);
        });
        true
    }

    /// The button came up: a press an editor took ends. Returns whether it was one.
    pub fn release(&mut self) -> bool {
        self.drag.take().is_some()
    }

    // --- What the page asks ----------------------------------------------------------------

    /// What the page asks of the editor named `id` in a reply.
    pub fn ask(
        &mut self,
        laid: &Laid,
        id: Id,
        ask: EditorAsk,
        measure: &mut dyn Measure,
        now: f64,
    ) {
        let Some(editor) = self.editors.get_mut(&id) else {
            self.events.push(EditorEvent::Refused {
                id,
                refusal: Refusal::NoEditor,
            });
            return;
        };
        if let EditorAsk::Reveal { version, at } = ask {
            match editor.buffer.carry(version, at) {
                Ok(at) => editor.reveal = Some(at),
                Err(refusal) => self.events.push(EditorEvent::Refused { id, refusal }),
            }
            self.refresh(id, Some(laid), measure);
            return;
        }
        self.with(id, laid, measure, |buffer, rows| {
            let _ = match ask {
                EditorAsk::Load { text, more } => buffer.load(&text, more),
                EditorAsk::Change { base, changes } => buffer.change(base, changes, now),
                EditorAsk::Select {
                    version,
                    ranges,
                    main,
                } => buffer.select_at(version, ranges, main),
                EditorAsk::Command(command) => {
                    buffer.command_in(command, now, rows);
                    Ok(())
                }
                EditorAsk::Decorate(decorate) => buffer.decorate(decorate),
                EditorAsk::Reveal { .. } => Ok(()),
            };
        });
    }
}

/// How far the room is scrolled, pixels.
fn scroll_of(laid: &Laid, room: &EditorRoom) -> Point {
    let s = laid.scale();
    laid.scroll_extent(room.id)
        .map_or(Point::default(), |extent| {
            Point::new(extent.offset.x * s, extent.offset.y * s)
        })
}

/// The range from what a press took (`anchor`) to what is under the pointer now (`here`), the two
/// together, its head the end toward `place`.
fn span(anchor: (usize, usize), here: (usize, usize), place: usize) -> Range {
    let (from, to) = (anchor.0.min(here.0), anchor.1.max(here.1));
    if place < anchor.0 {
        Range::new(to, from)
    } else {
        Range::new(from, to)
    }
}

/// The ranges of a column from `from` to `to` (pixels of the content): on each line between them
/// that shows, from the boundary nearest the one across to the boundary nearest the other.
fn column_ranges(editor: &Editor, from: Point, to: Point, measure: &mut dyn Measure) -> Vec<Range> {
    let view = &editor.view;
    let buffer = &editor.buffer;
    let first = buffer.line_of(view.place_at(buffer, Point::new(from.x, from.y), measure));
    let last = buffer.line_of(view.place_at(buffer, Point::new(to.x, to.y), measure));
    let (top, bottom) = (first.min(last), first.max(last));
    let hidden = buffer.hidden_lines();
    (top..=bottom)
        .filter(|line| !hidden.iter().any(|(a, b)| line >= a && line <= b))
        .filter_map(|line| {
            let y = view.point_of(buffer, buffer.line_start(line), measure)?.y;
            let anchor = view.place_at(buffer, Point::new(from.x, y), measure);
            let head = view.place_at(buffer, Point::new(to.x, y), measure);
            Some(Range::new(anchor, head))
        })
        .collect()
}

/// Where to scroll the editor `id` so that `caret` (pixels of the content) shows, where it does not.
fn keep_in_view(view: &View, id: Id, caret: Rect, scale: f32) -> Option<ScrollTo> {
    let s = scale.max(0.01);
    let (top, height) = (view.scroll.y, view.room.h);
    let y = if caret.y - view.pad < top {
        Some((caret.y - view.pad).max(0.0))
    } else if caret.bottom() + view.pad > top + height {
        Some(caret.bottom() + view.pad - height)
    } else {
        None
    };
    let x = if view.wrap {
        None
    } else {
        let left = view.scroll.x + view.text_x();
        let right = view.scroll.x + view.room.w;
        let margin = view.space * 4.0;
        if caret.x < left {
            Some((caret.x - view.text_x() - margin).max(0.0))
        } else if caret.right() + view.pad > right {
            Some(caret.right() + margin - view.room.w)
        } else {
            None
        }
    };
    (x.is_some() || y.is_some()).then(|| ScrollTo {
        id,
        x: x.map(|x| x / s),
        y: y.map(|y| y / s),
        animate: false,
    })
}

/// What a key does in an editor, apart from typing.
#[derive(Clone, Debug, PartialEq)]
enum Action {
    Command(EditCommand),
    Tab,
    Copy,
    Cut,
    Paste,
    Escape,
    ToLineStart,
    ToLineEnd,
}

/// The editor's own keys, as a Mac's text views have them (⌘ the command key, ⌥ the option key),
/// with those of editors for code besides.
fn builtin(key: Key, mods: Mods) -> Option<Action> {
    let (shift, alt, command, control) = (mods.shift, mods.alt, mods.command, mods.control);
    let go = |movement: Movement| {
        Some(Action::Command(EditCommand::Move {
            movement,
            extend: shift,
        }))
    };
    let does = |command: EditCommand| Some(Action::Command(command));
    match key {
        Key::Left if command => go(Movement::LineStart),
        Key::Left if alt => go(Movement::WordLeft),
        Key::Left => go(Movement::Left),
        Key::Right if command => go(Movement::LineEnd),
        Key::Right if alt => go(Movement::WordRight),
        Key::Right => go(Movement::Right),
        Key::Up if alt && command => does(EditCommand::AddCaretAbove),
        Key::Up if alt && shift => does(EditCommand::DuplicateLines),
        Key::Up if alt => does(EditCommand::MoveLinesUp),
        Key::Up if command => go(Movement::DocumentStart),
        Key::Up => go(Movement::Up),
        Key::Down if alt && command => does(EditCommand::AddCaretBelow),
        Key::Down if alt && shift => does(EditCommand::DuplicateLines),
        Key::Down if alt => does(EditCommand::MoveLinesDown),
        Key::Down if command => go(Movement::DocumentEnd),
        Key::Down => go(Movement::Down),
        Key::PageUp => go(Movement::PageUp),
        Key::PageDown => go(Movement::PageDown),
        Key::Home if command => go(Movement::DocumentStart),
        Key::Home => go(Movement::LineStart),
        Key::End if command => go(Movement::DocumentEnd),
        Key::End => go(Movement::LineEnd),
        Key::Backspace if command => Some(Action::ToLineStart),
        Key::Backspace if alt => does(EditCommand::DeleteWordBackward),
        Key::Backspace => does(EditCommand::DeleteBackward),
        Key::Delete if alt => does(EditCommand::DeleteWordForward),
        Key::Delete => does(EditCommand::DeleteForward),
        Key::Enter if !command && !alt && !control => does(EditCommand::Newline),
        Key::Tab if shift => does(EditCommand::Outdent),
        Key::Tab if !mods.any_command() => Some(Action::Tab),
        Key::Escape if !mods.any_command() && !shift => Some(Action::Escape),
        Key::Character(c) if command => match c.to_ascii_lowercase() {
            'z' if shift => does(EditCommand::Redo),
            'z' => does(EditCommand::Undo),
            'a' => does(EditCommand::SelectAll),
            'd' => does(EditCommand::SelectNext),
            '/' => does(EditCommand::ToggleComment),
            ']' if alt => does(EditCommand::Unfold),
            '[' if alt => does(EditCommand::Fold),
            ']' => does(EditCommand::Indent),
            '[' => does(EditCommand::Outdent),
            'g' if shift => does(EditCommand::FindPrevious),
            'g' => does(EditCommand::FindNext),
            'c' => Some(Action::Copy),
            'x' => Some(Action::Cut),
            'v' => Some(Action::Paste),
            _ => None,
        },
        Key::Character(c) if control && !alt => match c.to_ascii_lowercase() {
            'a' => go(Movement::LineStart),
            'e' => go(Movement::LineEnd),
            'k' => Some(Action::ToLineEnd),
            _ => None,
        },
        _ => None,
    }
}
