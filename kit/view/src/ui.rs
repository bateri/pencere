//! What a page remembers between builds: what the pointer is over and has pressed, what is being
//! dragged, where the keyboard is, how far each list is scrolled, how far each highlight has faded.
//!
//! The elements are made afresh for every frame, so none of this can be in them; it is kept by
//! [`Id`] and the page reads it when it builds. Every method that takes the pointer or the
//! wheel answers whether anything it keeps *changed* — a pointer moving inside the row it
//! was already over changes nothing, and the host draws no frame for it.

use std::collections::HashMap;

use crate::{Fades, Id, Point, Queries};

/// How far the pointer moves with a button held on something that can be dragged before it is
/// dragged, points: less is a click with an unsteady hand.
pub const DRAG_START: f32 = 4.0;

/// Something being dragged: what it is, where the pointer is now, and what it would be put down on
/// there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub source: Id,
    /// Where the button went down.
    pub from: Point,
    /// Where the pointer is.
    pub at: Point,
    /// The drop target under the pointer, if there is one and it is not the thing dragged.
    pub target: Option<Id>,
    /// The pointer has moved far enough for it to be a drag ([`DRAG_START`]); until then it may
    /// still be a click.
    pub moving: bool,
}

/// Something that was dragged and let go: what, on what (nothing, where it was let go over no drop
/// target), and where.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dropped {
    pub source: Id,
    pub target: Option<Id>,
    pub at: Point,
}

/// The remembered state of a page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ui {
    hover: Option<Id>,
    pressed: Option<Id>,
    drag: Option<Drag>,
    dropped: Option<Dropped>,
    focus: Option<Id>,
    /// How far each scrolling box is scrolled down, points.
    scroll: HashMap<Id, f32>,
    /// And across.
    scroll_x: HashMap<Id, f32>,
    /// The highlights coming and going.
    pub fades: Fades,
}

impl Ui {
    /// The pressable element the pointer is over.
    pub fn hover(&self) -> Option<Id> {
        self.hover
    }

    /// The pressable element a button is held on.
    pub fn pressed(&self) -> Option<Id> {
        self.pressed
    }

    /// The element the keyboard is on.
    pub fn focus(&self) -> Option<Id> {
        self.focus
    }

    // --- The pointer ---------------------------------------------------------------------

    /// The pointer is at `at` (`None`: it left the page). Returns whether the element it is
    /// over changed, or — while something is dragged — whether the drag moved.
    pub fn point(&mut self, laid: &dyn Queries, at: Option<Point>) -> bool {
        let mut changed = false;
        if let (Some(drag), Some(at)) = (&mut self.drag, at) {
            let (dx, dy) = (at.x - drag.from.x, at.y - drag.from.y);
            if !drag.moving && dx * dx + dy * dy >= DRAG_START * DRAG_START {
                drag.moving = true;
                changed = true;
            }
            if drag.moving {
                let target = laid.drop_target(at).filter(|target| *target != drag.source);
                changed |= drag.at != at || drag.target != target;
                drag.at = at;
                drag.target = target;
            }
        }
        let over = at.and_then(|at| laid.hit(at));
        changed | (std::mem::replace(&mut self.hover, over) != over)
    }

    /// A button went down at `at`: the element under it is the pressed one (and, where it can be
    /// picked up, perhaps the one dragged), and the keyboard moves to the nearest element around it
    /// that takes it (`focusable`). Returns the element pressed.
    pub fn press(&mut self, laid: &dyn Queries, at: Point) -> Option<Id> {
        let hit = laid.hit(at);
        self.pressed = hit;
        self.dropped = None;
        self.drag = laid.drag_source(at).map(|source| Drag {
            source,
            from: at,
            at,
            target: None,
            moving: false,
        });
        let under = laid.ids_at(at);
        if let Some(focus) = under.into_iter().find(|id| self.takes_focus(laid, *id)) {
            self.focus = Some(focus);
        }
        hit
    }

    /// The button came up at `at`. A click is a release over the element the press was on, when
    /// it was not dragged: returns it. What was dragged is put down ([`Ui::take_drop`]).
    pub fn release(&mut self, laid: &dyn Queries, at: Point) -> Option<Id> {
        if let Some(drag) = self.drag.take()
            && drag.moving
        {
            self.pressed = None;
            self.dropped = Some(Dropped {
                source: drag.source,
                target: laid.drop_target(at).filter(|target| *target != drag.source),
                at,
            });
            return None;
        }
        let pressed = self.pressed.take()?;
        (laid.hit(at) == Some(pressed)).then_some(pressed)
    }

    // --- Dragging ------------------------------------------------------------------------

    /// What is being dragged, once the pointer has moved far enough for it to be a drag: the page
    /// draws it where the pointer is (an overlay at `at` is the usual way) and marks the target.
    pub fn dragging(&self) -> Option<Drag> {
        self.drag.filter(|drag| drag.moving)
    }

    /// What was dragged and let go since the last ask: the page moves it where it went. Asked
    /// once, it is no longer kept.
    pub fn take_drop(&mut self) -> Option<Dropped> {
        self.dropped.take()
    }

    /// Gives up the drag under way, if there is one (the escape key): nothing is dropped, and the
    /// release that follows is no click. Returns whether there was one.
    pub fn cancel_drag(&mut self) -> bool {
        let was = self.dragging().is_some();
        if was {
            self.drag = None;
            self.pressed = None;
        }
        was
    }

    // --- The wheel -----------------------------------------------------------------------

    /// How far the scrolling box named `id` is scrolled, points.
    pub fn scroll_offset(&self, id: Id) -> f32 {
        self.scroll.get(&id).copied().unwrap_or(0.0)
    }

    /// Scrolls the box named `id` to `offset`, kept within the box's reach on the page `laid`
    /// (or at least 0 where the box is not known yet). Returns whether it moved.
    pub fn scroll_to(&mut self, laid: &dyn Queries, id: Id, offset: f32) -> bool {
        let reach = laid.scroll_extent(id).map_or(f32::MAX, |e| e.max_offset());
        let offset = offset.clamp(0.0, reach);
        let was = self.scroll.insert(id, offset).unwrap_or(0.0);
        was != offset
    }

    /// Sets how far the box named `id` is scrolled, without knowing yet how far it can go: the
    /// next [`Ui::settle`] brings it within its reach. For a page that has just changed what
    /// the box holds.
    pub fn set_scroll(&mut self, id: Id, offset: f32) {
        self.scroll.insert(id, offset.max(0.0));
    }

    /// How far the scrolling box named `id` is scrolled across, points.
    pub fn scroll_offset_x(&self, id: Id) -> f32 {
        self.scroll_x.get(&id).copied().unwrap_or(0.0)
    }

    /// Sets how far the box named `id` is scrolled across, as [`Ui::set_scroll`] does down.
    pub fn set_scroll_x(&mut self, id: Id, offset: f32) {
        self.scroll_x.insert(id, offset.max(0.0));
    }

    /// The wheel turned over `at` by `dx` and `dy` points (positive: across and down the page).
    /// For each way, the innermost scrolling box under the pointer that has room to go that way
    /// moves; one already at its end passes the turn on to the box around it. Returns whether
    /// anything moved.
    pub fn scroll_by(&mut self, laid: &dyn Queries, at: Point, dx: f32, dy: f32) -> bool {
        let under = laid.ids_at(at);
        let mut moved = false;
        for (delta, across) in [(dx, true), (dy, false)] {
            if delta == 0.0 {
                continue;
            }
            for id in &under {
                let Some(extent) = laid.scroll_extent(*id) else {
                    continue;
                };
                let (offsets, reach) = if across {
                    (&mut self.scroll_x, extent.max_offset_x())
                } else {
                    (&mut self.scroll, extent.max_offset())
                };
                let was = offsets.get(id).copied().unwrap_or(0.0);
                let to = (was + delta).clamp(0.0, reach);
                if to != was {
                    offsets.insert(*id, to);
                    moved = true;
                    break;
                }
            }
        }
        moved
    }

    /// Brings every scrolled box back within its reach on the page `laid` — a list that got
    /// shorter (a filter) or a window that got taller. Returns whether any moved: the page
    /// is laid out again then, as its rows depend on the offset.
    pub fn settle(&mut self, laid: &dyn Queries) -> bool {
        let mut moved = false;
        self.scroll.retain(|id, offset| {
            let Some(extent) = laid.scroll_extent(*id) else {
                return *offset > 0.0;
            };
            let to = offset.clamp(0.0, extent.max_offset());
            moved |= to != *offset;
            *offset = to;
            to > 0.0
        });
        self.scroll_x.retain(|id, offset| {
            let Some(extent) = laid.scroll_extent(*id) else {
                return *offset > 0.0;
            };
            let to = offset.clamp(0.0, extent.max_offset_x());
            moved |= to != *offset;
            *offset = to;
            to > 0.0
        });
        moved
    }

    // --- The keyboard --------------------------------------------------------------------

    fn takes_focus(&self, laid: &dyn Queries, id: Id) -> bool {
        laid.focus_order().contains(&id)
    }

    /// Puts the keyboard on `id`; returns whether it moved.
    pub fn focus_on(&mut self, id: Option<Id>) -> bool {
        std::mem::replace(&mut self.focus, id) != id
    }

    /// Moves the keyboard to the next element that takes it (the previous one when `back`),
    /// around from the last to the first. From nowhere it goes to the first (the last, back).
    /// Returns the element the keyboard is on now.
    pub fn focus_step(&mut self, laid: &dyn Queries, back: bool) -> Option<Id> {
        let order = laid.focus_order();
        if order.is_empty() {
            self.focus = None;
            return None;
        }
        let at = self
            .focus
            .and_then(|focus| order.iter().position(|id| *id == focus));
        let next = match (at, back) {
            (None, false) => 0,
            (None, true) => order.len() - 1,
            (Some(at), false) => (at + 1) % order.len(),
            (Some(at), true) => (at + order.len() - 1) % order.len(),
        };
        self.focus = order.get(next).copied();
        self.focus
    }

    /// Where a key goes: the element the keyboard is on, then the named elements around it,
    /// the innermost first. The page offers the key to each in turn and the first that
    /// takes it ends the walk.
    pub fn key_path(&self, laid: &dyn Queries) -> Vec<Id> {
        self.focus
            .map_or_else(Vec::new, |focus| laid.path_of(focus))
    }
}
