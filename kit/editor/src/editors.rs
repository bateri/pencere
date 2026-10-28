//! The editors of a page, as a host keeps them: one for each editor on the page ([`Laid::editors`]),
//! its document kept for as long as an editor of its name is there, hidden or not; what the person
//! does sent to the one it is for, in the order a key is for the page's commands, the editor's own
//! and the text; what the page asks done; and each drawn in its place.
//!
//! A host keeps one `Editors` for a page, and for every frame:
//!
//! 1. lays the page out measuring with it ([`Editors::measure`]), so that each editor's room scrolls
//!    over its content and what is anchored to its caret stands by it;
//! 2. [`Editors::sync`] with the page laid out: editors come and go with their rooms, and an
//!    editor whose document changed since it last searched searches again, once a frame however
//!    many keys came between two frames (until then what it found is carried over the changes;
//!    what the page's asks change after it is searched at the next frame);
//! 3. paints the page and [`Editors::paint`] on the list: each editor's place is filled.
//!
//! What it has for the page ([`Editors::take_events`]) and for the host's scrolling
//! ([`Editors::take_scrolls`]) is taken after each thing the person does; where it says the page is
//! to be laid out again ([`Editors::wants_layout`]: a caret that an overlay stands by moved), the
//! host lays out the tree it has, asking nothing of the page.

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

use pencere_view::edit::{
    EditCommand, Edited, EditorAsk, Found, GutterEvent, GutterHit, GutterKind, HoverEvent, Keys,
    Movement, Range, Refusal, RevealAlign, Selection, limits,
};
use pencere_view::{
    Button, Cursor, DrawList, EditorMetrics, EditorRoom, Font, Id, Key, Laid, Measure, Metrics,
    Mods, Paint, Point, Rect, ScrollTo, Size,
};

use crate::buffer::{Buffer, Note};
use crate::text::Snapshot;
use crate::view::{Cell, Kind, Rows, View};

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
    /// What its search finds now: after every search asked for, and whenever the count, or the
    /// match the main selection is on, changed.
    Found { id: Id, found: Found },
    /// The editor was made for a room the page laid out (shown or hidden): what the page asks of
    /// it is done from now on. Told before anything else of it.
    Ready { id: Id, version: u64 },
    /// The pointer did something in a gutter column the page hears of.
    Gutter(GutterEvent),
    /// The pointer rested over a word the page hears of, or left it.
    Hover(HoverEvent),
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
    /// A place to be brought into view, and where in the room.
    reveal: Option<(usize, RevealAlign)>,
    /// The gaps kept open for the page, as last told to the layout.
    zones: Vec<(Id, Rect)>,
    /// The ranges the page named, by their names ([`Id::anchor`]) and their boxes in the content, as
    /// last told to the layout.
    anchors: Vec<(Id, Rect)>,
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
    /// Something only drawn changed (the gutter mark the pointer is over).
    redraw: bool,
    /// Where the pointer last was, points of the page: what it is over is found again when the
    /// page is laid out (a scroll, an edit and a fold move what is under it).
    pointer: Option<Point>,
    /// What the pointer rests on ([`Editors::rest`]).
    hovering: hover::Hovering,
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

    fn zones(&mut self, id: Id) -> Vec<(Id, Rect)> {
        self.editors.zones(id)
    }

    fn anchors(&mut self, id: Id) -> Vec<(Id, Rect)> {
        self.editors.anchors(id)
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

    /// The gaps the editor named `id` keeps open for the page, each by its name and its box in the
    /// content, pixels ([`Measure::zones`]).
    pub fn zones(&self, id: Id) -> Vec<(Id, Rect)> {
        self.editors
            .get(&id)
            .filter(|editor| editor.shown)
            .map(|editor| editor.view.zone_boxes())
            .unwrap_or_default()
    }

    /// The editor named `id`'s document, where there is one.
    /// The ranges the page named in editor `id`, each by its name and its box in the content (pixels;
    /// [`Measure::anchors`]).
    pub fn anchors(&self, id: Id) -> Vec<(Id, Rect)> {
        self.editors
            .get(&id)
            .filter(|editor| editor.shown)
            .map(|editor| editor.anchors.clone())
            .unwrap_or_default()
    }

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

    /// Whether something only drawn changed since this was last asked (the mark the pointer is
    /// over): the host draws a frame, the page is told nothing. Asking says it once.
    pub fn wants_redraw(&mut self) -> bool {
        std::mem::take(&mut self.redraw)
    }

    /// What the markers of the gutters in view say of themselves (their `label`s): each with its box
    /// in the page's points, for a screen reader and for the host's tooltips.
    pub fn labels(&self, laid: &Laid) -> Vec<(Rect, String)> {
        let s = self.scale.max(0.01);
        let mut out = Vec::new();
        for room in laid.editors() {
            let Some(editor) = self
                .editors
                .get(&room.id)
                .filter(|e| e.shown && !room.hidden)
            else {
                continue;
            };
            let scroll = scroll_of(laid, &room);
            let (top, bottom) = (room.pixels.y, room.pixels.bottom());
            for (col, from, to, label) in editor.view.labels(&editor.buffer) {
                let Some(col) = editor.view.cols.get(col) else {
                    continue;
                };
                let y = room.pixels.y + editor.view.line_top(from) - scroll.y;
                let end = room.pixels.y + editor.view.line_bottom(to) - scroll.y;
                if end <= top || y >= bottom || end <= y {
                    continue;
                }
                let (y, end) = (y.max(top), end.min(bottom));
                out.push((
                    Rect::new((room.pixels.x + col.x) / s, y / s, col.w / s, (end - y) / s),
                    label,
                ));
            }
        }
        out
    }

    /// The pointer is at `at` (points of the page), or gone (`None`): the cell of a gutter it is over
    /// is drawn as it is when the pointer is over it (a marker lit, a bar wider, the column's preview
    /// in an empty cell), and the page is told of the pointer's coming to a cell and going from it
    /// where the column says it hears that.
    pub fn hover(&mut self, laid: &Laid, at: Option<Point>) {
        self.pointer = at;
        let found = at.and_then(|at| {
            let (id, point) = self.under(laid, at)?;
            let editor = self.editors.get(&id)?;
            let x = point.x - editor.view.scroll.x;
            Some((id, editor.view.cell_at(&editor.buffer, x, point.y)?))
        });
        let ids = self.order.clone();
        let mut told = Vec::new();
        for id in ids {
            let now = found
                .as_ref()
                .filter(|(over, _)| *over == id)
                .map(|(_, cell)| cell.clone());
            let Some(editor) = self.editors.get_mut(&id) else {
                continue;
            };
            if editor.view.hover == now {
                continue;
            }
            let before = std::mem::replace(&mut editor.view.hover, now.clone());
            self.redraw = true;
            let hears = |cell: &Cell| {
                editor
                    .view
                    .cols
                    .get(cell.col)
                    .is_some_and(|col| col.heard.hover)
            };
            if let Some(cell) = before.filter(hears) {
                told.push((id, cell, GutterKind::Leave));
            }
            if let Some(cell) = now.filter(hears) {
                told.push((id, cell, GutterKind::Enter));
            }
        }
        for (id, cell, kind) in told {
            if let Some(event) =
                self.gutter_event(laid, id, &cell, kind, (Button::Left, 0, Mods::default()))
            {
                self.events.push(EditorEvent::Gutter(event));
            }
        }
    }

    /// The cell of column `column` at line `line` of the editor named `id`, in the page's points,
    /// where it is in view.
    pub fn cell_rect(&self, laid: &Laid, id: Id, column: &str, line: usize) -> Option<Rect> {
        let editor = self.editors.get(&id).filter(|editor| editor.shown)?;
        let room = laid
            .editors()
            .into_iter()
            .find(|room| room.id == id && !room.hidden)?;
        let col = editor.view.cols.iter().find(|col| col.name == column)?;
        let s = self.scale.max(0.01);
        let y = room.pixels.y + editor.view.line_top(line) - scroll_of(laid, &room).y;
        if y + editor.view.row <= room.pixels.y || y >= room.pixels.bottom() {
            return None;
        }
        Some(Rect::new(
            (room.pixels.x + col.x) / s,
            y / s,
            col.w / s,
            editor.view.row / s,
        ))
    }

    /// What the pointer at `at` (points of the page) is to be: over the text of an editor, the
    /// I-beam; over its gutter the arrow, and the hand where something there answers a press (a
    /// button that folds, a column that takes presses, a marker whose shape the page chose). `None`
    /// where the pointer is not over an editor's room, or over something that stands on it.
    pub fn cursor_at(&self, laid: &Laid, at: Point) -> Option<Cursor> {
        let (id, point) = self.under(laid, at)?;
        let editor = self.editors.get(&id)?;
        let x = point.x - editor.view.scroll.x;
        if editor.view.gutter_shown && x >= 0.0 && x < editor.view.gutter {
            let Some(cell) = editor.view.cell_at(&editor.buffer, x, point.y) else {
                return Some(Cursor::Arrow);
            };
            let col = editor.view.cols.get(cell.col)?;
            return Some(
                if let Some(cursor) = cell.marker.as_ref().and_then(|m| m.cursor) {
                    cursor
                } else if col.heard.press
                    || col.heard.menu
                    || (col.kind == Kind::Folds
                        && editor
                            .buffer
                            .fold_lines(cell.line, cell.line)
                            .contains(&cell.line))
                {
                    Cursor::Pointer
                } else {
                    Cursor::Arrow
                },
            );
        }
        Some(Cursor::Text)
    }

    /// What the pointer did in `cell` of the gutter of editor `id`, as the page is told it: the cell's
    /// box in the page's points and the marker there.
    fn gutter_event(
        &self,
        laid: &Laid,
        id: Id,
        cell: &Cell,
        kind: GutterKind,
        (button, count, mods): (Button, u32, Mods),
    ) -> Option<GutterEvent> {
        let editor = self.editors.get(&id)?;
        let col = editor.view.cols.get(cell.col)?;
        let room = laid.editors().into_iter().find(|room| room.id == id)?;
        let s = self.scale.max(0.01);
        let top = editor.view.line_top(cell.line);
        let scroll = scroll_of(laid, &room);
        let rect = Rect::new(
            (room.pixels.x + col.x) / s,
            (room.pixels.y + top - scroll.y) / s,
            col.w / s,
            editor.view.row / s,
        );
        let marker = cell.marker.as_ref().map(|held| GutterHit {
            id: held.id.clone(),
            layer: editor
                .buffer
                .layer_names()
                .get(held.layer)
                .cloned()
                .unwrap_or_default(),
            from: held.from,
            to: held.to,
        });
        Some(GutterEvent {
            editor: id,
            column: col.name.clone(),
            line: cell.line,
            kind,
            button,
            count,
            mods,
            marker,
            rect,
        })
    }

    /// A press of `button` that is not the main one's at `at` (points of the page): in a gutter
    /// column that hears menus it is told to the page; nothing else of an editor answers it. Whether
    /// it was one.
    pub fn press_other(&mut self, laid: &Laid, at: Point, button: Button, mods: Mods) -> bool {
        self.end_rest();
        let Some((id, point)) = self.under(laid, at) else {
            return false;
        };
        let Some(editor) = self.editors.get(&id) else {
            return false;
        };
        let x = point.x - editor.view.scroll.x;
        let Some(cell) = editor.view.cell_at(&editor.buffer, x, point.y) else {
            return false;
        };
        let heard = editor
            .view
            .cols
            .get(cell.col)
            .is_some_and(|col| col.heard.menu);
        if heard
            && let Some(event) =
                self.gutter_event(laid, id, &cell, GutterKind::Press, (button, 1, mods))
        {
            self.events.push(EditorEvent::Gutter(event));
        }
        heard
    }

    // --- Each frame ------------------------------------------------------------------------

    /// After the page was laid out: an editor for each room there, new ones empty, those whose room
    /// went gone with their documents; each laid out where its room is scrolled.
    pub fn sync(&mut self, laid: &Laid, measure: &mut dyn Measure) {
        self.sync_editors(laid, measure);
        // What is under the pointer may not be what was: the text scrolled, grew or folded.
        if self.pointer.is_some() {
            self.hover(laid, self.pointer);
        }
        self.check_rest();
    }

    /// What the pointer is over, found again where it last was: the shape it has there.
    pub fn cursor_here(&self, laid: &Laid) -> Option<Cursor> {
        self.pointer.and_then(|at| self.cursor_at(laid, at))
    }

    fn sync_editors(&mut self, laid: &Laid, measure: &mut dyn Measure) {
        let rooms = laid.editors();
        self.scale = laid.scale();
        self.editors
            .retain(|id, _| rooms.iter().any(|room| room.id == *id));
        if self.focus.is_some_and(|id| !self.editors.contains_key(&id)) {
            self.focus = None;
        }
        self.order = rooms.iter().map(|room| room.id).collect();
        for room in &rooms {
            let editor = match self.editors.entry(room.id) {
                Entry::Occupied(kept) => kept.into_mut(),
                Entry::Vacant(room_of) => {
                    let made = Editor::default();
                    self.events.push(EditorEvent::Ready {
                        id: room.id,
                        version: made.buffer.version(),
                    });
                    room_of.insert(made)
                }
            };
            editor.buffer.set_options(room.options.clone());
            editor.view.set_room(room, self.scale, measure);
            editor.view.size_gutter(editor.buffer.len_lines());
            editor.shown = !room.hidden;
            editor.buffer.search_again();
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
                    let head = selection.ranges.get(selection.main).map_or(0, |r| r.head);
                    editor.reveal = Some((head, RevealAlign::Nearest));
                    EditorEvent::Selection { id, selection }
                }
                Note::Refused(refusal) => EditorEvent::Refused { id, refusal },
                Note::Found(found) => EditorEvent::Found { id, found },
            });
        }
        if !editor.shown {
            return;
        }
        editor.view.size_gutter(editor.buffer.len_lines());
        editor.view.count(&editor.buffer);
        // A gap that came, went or moved is where an element of the page is to be put: the page is laid
        // out again with it.
        let zones = editor.view.zone_boxes();
        if zones != editor.zones {
            editor.zones = zones;
            self.relayout = true;
        }
        let anchors: Vec<(Id, Rect)> = editor
            .buffer
            .anchors()
            .into_iter()
            .filter_map(|(name, from, to)| {
                Some((
                    id.anchor(&name),
                    editor.view.range_box(&editor.buffer, from, to, measure)?,
                ))
            })
            .collect();
        if anchors != editor.anchors {
            // A range that moved, came or went is where an overlay stands by it is to be put again, and
            // what the layout knows of it (`Laid::bounds`) is to be found again.
            self.relayout = true;
            editor.anchors = anchors;
        }
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
        if let Some((at, align)) = editor.reveal.take()
            && let Some(caret) = editor.view.caret_of(&editor.buffer, at, measure)
            && let Some(ask) = keep_in_view(&editor.view, id, caret, scale, align)
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
            out.items.extend(
                editor
                    .view
                    .paint(&editor.buffer, &slot, focused, caret_on, measure),
            );
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
        self.end_rest();
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
            // ⌥ is how a Mac's keyboards type what is not on a key of its own (`@`, `[`, `{`, `|`,
            // `~` on many of them): a character with it types, as one with ⇧ does.
            return match key {
                Key::Character(_) if !mods.command && !mods.control => Route::Type,
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
        self.end_rest();
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
        if let Some(cell) = editor.view.cell_at(&editor.buffer, room_x, point.y) {
            let Some(col) = editor.view.cols.get(cell.col) else {
                return true;
            };
            // A column of the page's that hears this press: the page's, and nothing of the editor's.
            let heard = if mods.control {
                col.heard.menu
            } else {
                col.heard.press
            };
            if heard {
                let event = self.gutter_event(
                    laid,
                    id,
                    &cell,
                    GutterKind::Press,
                    (Button::Left, count, mods),
                );
                if let Some(event) = event {
                    self.events.push(EditorEvent::Gutter(event));
                }
                if let Some(editor) = self.editors.get_mut(&id) {
                    editor.view.pressed = Some(cell);
                }
                self.redraw = true;
                return true;
            }
            let (line, folds) = (cell.line, col.kind == Kind::Folds);
            self.with(id, laid, measure, |buffer, _| {
                if folds && buffer.fold_at(line, None) {
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
        for editor in self.editors.values_mut() {
            if editor.view.pressed.take().is_some() {
                self.redraw = true;
            }
        }
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
        if let EditorAsk::Reveal { version, at, align } = ask {
            match editor.buffer.carry(version, at) {
                Ok(at) => editor.reveal = Some((at, align)),
                Err(refusal) => self.events.push(EditorEvent::Refused { id, refusal }),
            }
            self.refresh(id, Some(laid), measure);
            return;
        }
        self.with(id, laid, measure, |buffer, rows| {
            let _ = match ask {
                EditorAsk::Load { text, more } => buffer.load(&text, more),
                EditorAsk::Change {
                    base,
                    changes,
                    history,
                    tag,
                } => buffer.change_as(base, changes, now, history, tag),
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

/// Where to scroll the editor `id` so that `caret` (pixels of the content) shows: where it does not
/// (`Nearest`), or with its row at the top, the middle or the bottom of the room whatever shows
/// now (the host takes a place past the end to the end).
fn keep_in_view(
    view: &View,
    id: Id,
    caret: Rect,
    scale: f32,
    align: RevealAlign,
) -> Option<ScrollTo> {
    let s = scale.max(0.01);
    let (top, height) = (view.scroll.y, view.room.h);
    // The caret's whole row is kept in view, not only its caret, and as many rows about it as the
    // page asks (at most half of those in view).
    let index = ((caret.y - view.pad) / view.row).floor().max(0.0);
    let (above, below) = (
        view.pad + index * view.row,
        view.pad + (index + 1.0) * view.row,
    );
    let in_view = (height / view.row).floor().max(1.0);
    let margin = f32::from(view.options.scroll_margin).min(((in_view - 1.0) / 2.0).floor())
        * view.row
        + view.pad;
    let y = match align {
        RevealAlign::Start => Some((above - view.pad).max(0.0)),
        RevealAlign::Center => Some(((above + below - height) / 2.0).max(0.0)),
        RevealAlign::End => Some((below + view.pad - height).max(0.0)),
        RevealAlign::Nearest if above - margin < top => Some((above - margin).max(0.0)),
        RevealAlign::Nearest if below + margin > top + height => Some(below + margin - height),
        RevealAlign::Nearest => None,
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

impl From<EditorEvent> for pencere_view::Event {
    /// What the page is told of it.
    fn from(event: EditorEvent) -> Self {
        use pencere_view::Event;
        match event {
            EditorEvent::Edited { id, edited } => Event::Edit { editor: id, edited },
            EditorEvent::Selection { id, selection } => Event::Selection {
                editor: id,
                selection,
            },
            EditorEvent::Refused { id, refusal } => Event::EditorRefused {
                editor: id,
                refusal,
            },
            EditorEvent::Viewport { id, first, last } => Event::Viewport {
                editor: id,
                first,
                last,
            },
            EditorEvent::Focus(id) => Event::Focus(id),
            EditorEvent::Found { id, found } => Event::Found { editor: id, found },
            EditorEvent::Ready { id, version } => Event::EditorReady {
                editor: id,
                version,
            },
            EditorEvent::Gutter(event) => Event::Gutter(event),
            EditorEvent::Hover(event) => Event::Hover(event),
        }
    }
}

mod hover;
