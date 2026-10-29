//! The editors of a page, the host's half of them: the kit's editor (`pencere-editor`, the same in
//! every host) in each editor's room, its document kept for as long as an editor of its name is on
//! the page, what the person does done to the one with the keyboard, what the page asks done, each
//! drawn in its place, its caret blinked on the frames' clock.
//!
//! A frame lays the page out measuring with the editors (`Editors::measure`), so that a room
//! scrolls over its editor's content and what is anchored to a caret stands by it; then the editors
//! are made for the rooms there and what the page's replies asked of them is done
//! ([`Editing::settle`]); where a caret an overlay stands by moved, the same tree is laid out again,
//! the page asked nothing. After anything is done to them — in a frame or between frames, where the
//! host's keys, input method and pointer reach them — what they have for the page is told it, its
//! texts first ([`Editing::told`]).

use std::sync::Arc;

use pencere_editor::{EditorEvent, Editors};
use pencere_view::edit::{EditorAsk, TextSnapshot};
use pencere_view::{Event, Id, Keyboard, Laid, Measure, Point, Reply, ScrollTo};

/// How long after the caret last moved it stops blinking, lit: a terminal cursor's rule, so that a
/// page at rest goes back to drawing no frames.
const IDLE_STOP: f64 = 15.0;

/// What the drive keeps of a page's editors.
pub(crate) struct Editing {
    pub(crate) editors: Editors,
    /// What the page's replies asked of its editors: done in the next frame, once the editors are
    /// made for the page it laid out (a reply may come with the page that has the editor).
    asks: Vec<(Id, EditorAsk)>,
    /// The editor a reply sent the keyboard to that was not there yet: given it once the editors
    /// are made.
    focus: Option<Id>,
    pub(crate) blink: Blink,
}

/// What came of something done to the editors, for the page and for the host.
#[derive(Default)]
pub(crate) struct Told {
    /// The text of each editor: posted to the page before the events, so that it reads the version
    /// they tell of. Empty where there are no events.
    pub texts: Vec<(Id, Arc<dyn TextSnapshot>)>,
    pub events: Vec<Event>,
    /// Where an editor asks to be scrolled, to keep its caret in view.
    pub scrolls: Vec<ScrollTo>,
    /// The page is to be laid out again as it is: an editor's content grew or shrank, or a caret
    /// an overlay stands by moved.
    pub relayout: bool,
    /// Something only drawn changed (the gutter mark under the pointer): a frame, and nothing told.
    pub redraw: bool,
}

impl Told {
    /// Whether there is anything to tell or to do.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.scrolls.is_empty() && !self.relayout && !self.redraw
    }
}

impl Editing {
    /// The editors of a page whose carets blink `half_period` seconds lit, then as long dark.
    pub(crate) fn new(half_period: f64) -> Self {
        Self {
            editors: Editors::new(),
            asks: Vec::new(),
            focus: None,
            blink: Blink::new(half_period),
        }
    }

    /// Takes what `reply` asks of the editors out of it (a copy of it, where the reply is to be
    /// `kept` whole), to be done at the next frame, and gives the keyboard where it says
    /// (`Keyboard::Field` of an editor; a field or the page is none of them). Whether there was
    /// anything for the editors.
    pub(crate) fn take_reply(&mut self, reply: &mut Reply, kept: bool) -> bool {
        let asks = if kept {
            reply.editor.clone()
        } else {
            std::mem::take(&mut reply.editor)
        };
        let any = !asks.is_empty();
        self.asks.extend(asks);
        match reply.keyboard {
            Some(Keyboard::Field(id)) => {
                self.editors.set_focus(Some(id));
                self.focus = (self.editors.focused() != Some(id)).then_some(id);
                self.blink.moved();
                true
            }
            Some(Keyboard::Page) => {
                self.editors.set_focus(None);
                self.focus = None;
                true
            }
            None => any,
        }
    }

    /// The keyboard went where the person put it (a field, the page, an editor): a reply's that
    /// was waiting for its editor is not given any more.
    pub(crate) fn person_moved_keyboard(&mut self) {
        self.focus = None;
    }

    /// After the page was laid out (`laid`): an editor for each room there, the keyboard where a
    /// reply sent it, and what the replies asked done, at `now`.
    pub(crate) fn settle(&mut self, laid: &Laid, measure: &mut dyn Measure, now: f64) {
        self.editors.sync(laid, measure);
        self.editors.tick(laid, now, measure);
        if let Some(id) = self.focus.take() {
            self.editors.set_focus(Some(id));
        }
        for (id, ask) in std::mem::take(&mut self.asks) {
            self.editors.ask(laid, id, ask, measure, now);
        }
    }

    /// What the editors have for the page and the host now. What moved a caret puts it back lit.
    pub(crate) fn told(&mut self) -> Told {
        let scrolls = self.editors.take_scrolls();
        let relayout = self.editors.wants_layout();
        let redraw = self.editors.wants_redraw();
        let events = self.editors.take_events();
        if events.iter().any(|event| {
            matches!(
                event,
                EditorEvent::Edited { .. }
                    | EditorEvent::Selection { .. }
                    | EditorEvent::Focus(Some(_))
            )
        }) {
            self.blink.moved();
        }
        let texts = if events.is_empty() {
            Vec::new()
        } else {
            self.editors
                .snapshots()
                .into_iter()
                .map(|(id, text)| (id, Arc::new(text) as Arc<dyn TextSnapshot>))
                .collect()
        };
        Told {
            texts,
            events: events.into_iter().map(Event::from).collect(),
            scrolls,
            relayout,
            redraw,
        }
    }
}

/// Whether `at` (points of the page) is in the room of an editor that shows: a press there is not
/// one that takes the keyboard back to the page, though what is over the room (an overlay by the
/// caret) is not the editor's.
pub(crate) fn in_a_room(laid: &Laid, at: Point) -> bool {
    laid.editors().iter().any(|room| {
        let r = room.rect;
        !room.hidden && at.x >= r.x && at.y >= r.y && at.x < r.x + r.w && at.y < r.y + r.h
    })
}

/// The caret's blink: lit for a half period, dark for one, from the frame after it last moved (a
/// caret that moved is lit), lit and still after [`IDLE_STOP`] or with Reduce Motion on, and not drawn
/// while the host's keyboard is not in the page of the person's window. The times are the frames'.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Blink {
    /// How long the caret stays lit, then dark (none: it is lit and still).
    half: f64,
    /// The frame the caret was last put back lit at.
    since: Option<f64>,
    /// It moved since: the next frame puts it back lit.
    moved: bool,
    /// The host's keyboard is in the page, in the person's window.
    active: bool,
}

impl Blink {
    pub(crate) fn new(half: f64) -> Self {
        Self {
            half,
            since: None,
            moved: true,
            active: true,
        }
    }

    /// Something moved the caret, or the editor took the keyboard.
    pub(crate) fn moved(&mut self) {
        self.moved = true;
    }

    /// Whether the host's keyboard is in the page in the person's window. Coming back puts the
    /// caret back lit.
    pub(crate) fn set_active(&mut self, active: bool) {
        if active && !self.active {
            self.moved = true;
        }
        self.active = active;
    }

    /// Whether the caret is drawn in the frame at `now`, where an editor has the keyboard
    /// (`shown`), and when that is next to change: `None` until something changes it.
    pub(crate) fn at(&mut self, now: f64, still: bool, shown: bool) -> (bool, Option<f64>) {
        if !shown || !self.active {
            self.moved = true;
            return (false, None);
        }
        let since = match self.since {
            Some(since) if !self.moved && now >= since => since,
            _ => {
                self.moved = false;
                self.since = Some(now);
                now
            }
        };
        let lit_for = now - since;
        // A period that is no time is a caret that does not blink.
        if still || lit_for >= IDLE_STOP || !(self.half > 0.0 && self.half.is_finite()) {
            return (true, None);
        }
        let phase = (lit_for / self.half).floor();
        let lit = phase.rem_euclid(2.0) < 1.0;
        let next = since + (phase + 1.0) * self.half;
        (lit, Some(next.min(since + IDLE_STOP)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_caret_that_moved_is_lit_then_blinks_on_the_frames_clock_and_stops_lit() {
        let mut blink = Blink::new(0.5);
        assert_eq!(blink.at(10.0, false, true), (true, Some(10.5)));
        assert_eq!(blink.at(10.5, false, true), (false, Some(11.0)));
        assert_eq!(blink.at(10.7, false, true), (false, Some(11.0)));
        assert_eq!(blink.at(11.0, false, true), (true, Some(11.5)));
        // A key: lit again, from the frame after it.
        blink.moved();
        assert_eq!(blink.at(11.25, false, true), (true, Some(11.75)));
        // Still after a while without a change: lit, and nothing more to wake for.
        assert_eq!(blink.at(25.75, false, true), (false, Some(26.25)));
        assert_eq!(blink.at(26.25, false, true), (true, None));
        assert_eq!(blink.at(40.0, false, true), (true, None));
    }

    #[test]
    fn a_caret_is_not_drawn_away_from_the_keyboard_and_is_still_with_reduce_motion() {
        let mut blink = Blink::new(0.5);
        assert_eq!(
            blink.at(1.0, false, false),
            (false, None),
            "no editor has it"
        );
        blink.set_active(false);
        assert_eq!(
            blink.at(2.0, false, true),
            (false, None),
            "another view has it"
        );
        blink.set_active(true);
        assert_eq!(
            blink.at(3.5, false, true),
            (true, Some(4.0)),
            "back, and lit"
        );
        blink.moved();
        assert_eq!(blink.at(4.0, true, true), (true, None), "Reduce Motion");
    }

    #[test]
    fn a_caret_whose_period_is_no_time_is_lit_and_still() {
        for half in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let mut blink = Blink::new(half);
            assert_eq!(blink.at(1.0, false, true), (true, None), "{half}");
        }
    }
}
