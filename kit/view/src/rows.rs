//! A long list's arithmetic: where each row stands, which rows a window of the list shows, and
//! how far to scroll to see one.
//!
//! A page that shows a list builds only the rows in view — the elements of ten thousand
//! rows are not made to show thirty — and keeps the rest of the list's length with a spacer
//! above and one below. The rows' heights are known without laying them out (a list row is
//! the same height every time), so their places are a sum. Which rows to build as the list
//! scrolls, and when to build them again, is [`Built`]'s; a page keeps a [`List`] for each long
//! list it shows, which does all of it.

use std::cell::RefCell;
use std::ops::Range;

use crate::element::{Element, div};
use crate::style::{Direction, Display};
use crate::{Id, Point, Queries, ScrollTo};

/// The heights of a list's rows, and the sums of them.
#[derive(Clone, Debug, PartialEq)]
pub struct Rows {
    places: Places,
    /// The room kept above the first row and below the last.
    lead: f32,
    trail: f32,
}

/// Where the rows start, from the first.
#[derive(Clone, Debug, PartialEq)]
enum Places {
    /// `tops[i]` is where row `i` starts; the last is where the last row ends. Never empty.
    Tops(Vec<f32>),
    /// `count` rows of one `height`: a long list of one height has no table of where each starts.
    Even { count: usize, height: f32 },
}

impl Default for Rows {
    fn default() -> Self {
        Self::from_tops(Vec::new())
    }
}

impl Rows {
    /// A list whose rows are `heights` high, points.
    pub fn new(heights: impl IntoIterator<Item = f32>) -> Self {
        let mut tops = vec![0.0];
        let mut sum = 0.0;
        for height in heights {
            sum += height.max(0.0);
            tops.push(sum);
        }
        Self::from_tops(tops)
    }

    /// A list whose rows start where `tops` says: `tops[i]` is where row `i` starts, from 0, and
    /// the last is where the last row ends — the sums a page keeps of a list that grows, given as
    /// they are.
    pub fn from_tops(mut tops: Vec<f32>) -> Self {
        if tops.is_empty() {
            tops.push(0.0);
        }
        Self {
            places: Places::Tops(tops),
            lead: 0.0,
            trail: 0.0,
        }
    }

    /// A list of `count` rows each `height` high, points: as long as any list, as small as none.
    pub fn even(count: usize, height: f32) -> Self {
        Self {
            places: Places::Even {
                count,
                height: height.max(0.0),
            },
            lead: 0.0,
            trail: 0.0,
        }
    }

    /// The same list with `lead` points of room before the first row and `trail` after the last.
    #[must_use]
    pub fn padded(mut self, lead: f32, trail: f32) -> Self {
        self.lead = lead;
        self.trail = trail;
        self
    }

    /// How many rows.
    pub fn len(&self) -> usize {
        match &self.places {
            Places::Tops(tops) => tops.len() - 1,
            Places::Even { count, .. } => *count,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The length of the whole list, with the room around the rows, points.
    pub fn total(&self) -> f32 {
        self.top(self.len()) + self.trail
    }

    /// Where row `index` starts (the list's length for a row past the end).
    pub fn top(&self, index: usize) -> f32 {
        let index = index.min(self.len());
        let from_first = match &self.places {
            Places::Tops(tops) => tops.get(index).copied().unwrap_or(0.0),
            Places::Even { height, .. } => index as f32 * height,
        };
        self.lead + from_first
    }

    /// The height of row `index`.
    pub fn height(&self, index: usize) -> f32 {
        self.top(index + 1) - self.top(index)
    }

    /// How many of the rows' tops (the end of the last one among them) are before `y`, or not past
    /// it where `inclusive`.
    fn count_before(&self, y: f32, inclusive: bool) -> usize {
        let (mut low, mut high) = (0, self.len() + 1);
        while low < high {
            let middle = (low + high) / 2;
            let top = self.top(middle);
            if top < y || (inclusive && top == y) {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low
    }

    /// The row at `y` from the list's start (the last row for a point past the end).
    pub fn at(&self, y: f32) -> Option<usize> {
        if self.is_empty() {
            return None;
        }
        // The first top greater than `y` is the start of the row after the one at `y`.
        let after = self.count_before(y, true);
        Some(after.saturating_sub(1).min(self.len() - 1))
    }

    /// The rows that touch the window `offset..offset + view`, `overscan` points more at each
    /// end (rows that are not in view yet, so a scroll finds them built).
    pub fn window(&self, offset: f32, view: f32, overscan: f32) -> Range<usize> {
        if self.is_empty() || view <= 0.0 {
            return 0..0;
        }
        let from = (offset - overscan).max(0.0);
        let to = offset + view + overscan;
        if from >= self.total() {
            return self.len()..self.len();
        }
        let start = self.at(from).unwrap_or(0);
        // The first row that starts at or after `to` is the first one out of the window.
        let end = self.count_before(to, false).min(self.len());
        start..end.max(start)
    }

    /// The offset that shows all of row `index` in a window of `view` points that is at
    /// `offset` now: `offset` itself when the row is in view, the nearest edge otherwise. The
    /// first row is shown from the start of the list and the last to its end, room around
    /// them and all.
    pub fn reveal(&self, index: usize, offset: f32, view: f32) -> f32 {
        let top = if index == 0 { 0.0 } else { self.top(index) };
        let bottom = if index + 1 >= self.len() {
            self.total()
        } else {
            self.top(index + 1)
        };
        if top < offset {
            top
        } else if bottom > offset + view {
            (bottom - view).max(0.0).min(top)
        } else {
            offset
        }
    }
}

/// How much of a list is built beyond what is in view, in views: more the way it goes than behind
/// it, for the host scrolls what is built at once and the page builds what comes into view only a
/// frame or two later.
const AHEAD: f32 = 2.0;
const BEHIND: f32 = 0.5;
/// How near the end of what is built, the way the list goes, what is in view comes before the page
/// is built again, in views: as much as comes into view while the page builds, at the most a
/// person scrolls.
const NEAR: f32 = 1.0;

/// Which rows of a long list a page built, and which way the list goes: kept between frames, so
/// that the page builds two views of rows ahead of what is in view and half a view behind it, and
/// is built again only when what is in view comes within a view of the end of what was built, the
/// way the list goes — not at every turn of the wheel, and never short of what scrolling brings
/// into view while it keeps up.
///
/// The page asks [`Built::span`] which rows to build, and [`Built::stale`] whether the list's
/// being scrolled to an offset (`Event::Scrolled`) means it is to be built again.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Built {
    /// The rows built last, and how many the list had then.
    built: Option<(Range<usize>, usize)>,
    /// Whether the list last went up, and where it was then: the way is changed only by a move,
    /// so that the build a move asked for, at the same place, finds the way it went.
    up: bool,
    seen: f32,
}

impl Built {
    pub fn new() -> Self {
        Self::default()
    }

    fn went(&mut self, offset: f32) {
        if offset != self.seen {
            self.up = offset < self.seen;
            self.seen = offset;
        }
    }

    /// Whether, with the list `rows` at `offset` in a window of `view` points, the page is to be
    /// built again: what is in view is past what was built, or near its end the way the list
    /// goes, or the list is not as long as it was.
    pub fn stale(&mut self, rows: &Rows, offset: f32, view: f32) -> bool {
        self.went(offset);
        let Some((built, len)) = &self.built else {
            return true;
        };
        if *len != rows.len() {
            return true;
        }
        let (top, bottom) = (rows.top(built.start), rows.top(built.end));
        let (more_above, more_below) = (built.start > 0, built.end < rows.len());
        if (more_above && offset < top) || (more_below && offset + view > bottom) {
            return true;
        }
        let near = view * NEAR;
        if self.up {
            more_above && offset - near < top
        } else {
            more_below && offset + view + near > bottom
        }
    }

    /// The rows to build with the list `rows` at `offset` in a window of `view` points: those
    /// built last while they still hold what is in view with room, else a new span around it,
    /// longer the way the list goes (and all of it the other way at an end of the list).
    pub fn span(&mut self, rows: &Rows, offset: f32, view: f32) -> Range<usize> {
        if !self.stale(rows, offset, view)
            && let Some((built, _)) = &self.built
        {
            return built.clone();
        }
        // Before anything is built there is no way the list goes yet: as much is built both ways.
        let (before, after) = match (&self.built, self.up) {
            (None, _) => (AHEAD, AHEAD),
            (Some(_), true) => (AHEAD, BEHIND),
            (Some(_), false) => (BEHIND, AHEAD),
        };
        // What would be built past an end of the list is built at the other instead: a list at its
        // end can only go back.
        let (start, end) = (rows.top(0), rows.total());
        let (mut from, mut to) = (offset - view * before, offset + view * (1.0 + after));
        if from < start {
            to += start - from;
            from = start;
        }
        if to > end {
            from = (from - (to - end)).max(start);
            to = end;
        }
        let span = rows.window(from, to - from, 0.0);
        self.built = Some((span.clone(), rows.len()));
        span
    }
}

/// A long list in a box the host scrolls, as a page keeps it between frames: its name, which rows
/// it made and which way it goes ([`Built`]), where its rows stood, the window it was laid out in
/// and how far the host has it scrolled.
///
/// The page builds the list's box with [`List::build`] as it builds the rest of the page (it takes
/// `&self`: what it made is the list's memory of the build, kept inside it), and tells it what the
/// host said: the page laid out ([`List::settle`]) and the boxes scrolled ([`List::scrolled`]).
/// Each answers whether the page is to be built again for the list. The host scrolls the rows made
/// at once, so it is built again only when what is in view comes near the end of them, or the box
/// was laid out taller than they fill.
#[derive(Clone, Debug)]
pub struct List {
    id: Id,
    state: RefCell<State>,
}

#[derive(Clone, Debug)]
struct State {
    /// Whether the list is on the page: built since it was last forgotten.
    shown: bool,
    built: Built,
    rows: Rows,
    made: Range<usize>,
    /// The window the box was laid out in, and the one taken before it was (the page's height).
    view: Option<f32>,
    before_laid: f32,
    /// How far the host has the box scrolled: what the page was last told (or asked for) and,
    /// before that, as it was laid out.
    told: Option<f32>,
    laid: f32,
}

impl State {
    fn view(&self) -> f32 {
        self.view.unwrap_or(self.before_laid)
    }

    fn offset(&self) -> f32 {
        self.told.unwrap_or(self.laid)
    }
}

/// How far down a list of `rows` in a window `view` tall is built for `offset`: held to the end of
/// these rows (an ask past the end is the end of the rows given, which may be further than the box
/// was when it was last laid out: a log that grew is asked for at its new end and built there).
fn within(offset: f32, rows: &Rows, view: f32) -> f32 {
    offset.min(rows.total() - view).max(0.0)
}

impl List {
    pub fn new(id: Id) -> Self {
        Self {
            id,
            state: RefCell::new(State {
                shown: false,
                built: Built::new(),
                rows: Rows::default(),
                made: 0..0,
                view: None,
                before_laid: 0.0,
                told: None,
                laid: 0.0,
            }),
        }
    }

    /// The name of the list's box.
    pub fn id(&self) -> Id {
        self.id
    }

    /// How far the host has the list scrolled down, points: what the page was last told or asked
    /// for, else as it was laid out.
    pub fn offset(&self) -> f32 {
        self.state.borrow().offset()
    }

    /// How tall the list's window is, points: as it was laid out, or the page before it was.
    pub fn view(&self) -> f32 {
        self.state.borrow().view()
    }

    /// The rows the last build made.
    pub fn made(&self) -> Range<usize> {
        self.state.borrow().made.clone()
    }

    /// The list's box: `body`, named and scrolling, with a spacer, the rows of `rows` near what is
    /// in view (`make(i)` makes row i) and a spacer. `page_h` is the window taken before the box
    /// is laid out: the page's height. The box is a column with no padding above and no gap
    /// between its children; room above the rows is [`Rows::padded`].
    pub fn build(
        &self,
        body: Element,
        rows: Rows,
        page_h: f32,
        mut make: impl FnMut(usize) -> Element,
    ) -> Element {
        debug_assert!(
            body.style.padding[0] == 0.0 && body.style.gap.1 == 0.0,
            "a long list's box has no padding above and no gap: room above the rows is Rows::padded"
        );
        debug_assert!(
            body.style.display != Display::Grid && body.style.direction == Direction::Column,
            "a long list's box is a column: its rows stand one under another"
        );
        let made = {
            let mut state = self.state.borrow_mut();
            state.before_laid = page_h;
            let view = state.view();
            let offset = within(state.offset(), &rows, view);
            state.built.span(&rows, offset, view)
        };
        // The rows are made with nothing of the list held: a row may ask it how far it is scrolled.
        let spacer = |height: f32| div().h(height.max(0.0)).no_shrink();
        let mut body = body
            .id(self.id)
            .scroll()
            .child(spacer(rows.top(made.start)));
        for index in made.clone() {
            body = body.child(make(index));
        }
        body = body.child(spacer(rows.total() - rows.top(made.end)));
        let mut state = self.state.borrow_mut();
        state.shown = true;
        state.rows = rows;
        state.made = made;
        body
    }

    /// The page was laid out as `laid`: whether the list's window is now one the rows made do not
    /// fill, and the page is to be built again. A list not on the page asks for nothing.
    pub fn settle(&mut self, laid: &dyn Queries) -> bool {
        let Some(extent) = laid.scroll_extent(self.id) else {
            return false;
        };
        let state = self.state.get_mut();
        state.view = Some(extent.view.h);
        state.laid = extent.offset.y;
        let offset = within(state.offset(), &state.rows, extent.view.h);
        state.shown && state.built.stale(&state.rows, offset, extent.view.h)
    }

    /// The host scrolled `boxes`: whether the list is among them and is to be built again at its
    /// new place.
    pub fn scrolled(&mut self, boxes: &[(Id, Point)]) -> bool {
        let state = self.state.get_mut();
        let mut again = false;
        for (id, at) in boxes {
            if *id == self.id {
                state.told = Some(at.y);
                let view = state.view();
                again |= state.shown && state.built.stale(&state.rows, at.y, view);
            }
        }
        again
    }

    /// Asks the host to scroll the list to `y` down: what the page hands the host.
    pub fn scroll_to(&mut self, y: f32) -> ScrollTo {
        self.state.get_mut().told = Some(y);
        ScrollTo {
            id: self.id,
            x: None,
            y: Some(y),
            animate: false,
        }
    }

    /// Scrolls the least that shows all of row `index` of `rows`, if the list is not there
    /// already ([`Rows::reveal`]).
    pub fn reveal(&mut self, rows: &Rows, index: usize) -> Option<ScrollTo> {
        let (offset, view) = (self.offset(), self.view());
        let to = rows.reveal(index, offset, view);
        (to != offset).then(|| self.scroll_to(to))
    }

    /// The list is not on the page: what was built of it, its rows and its window are forgotten,
    /// and it is built afresh when it comes back. How far it is scrolled stands.
    pub fn forget(&self) {
        let mut state = self.state.borrow_mut();
        state.shown = false;
        state.built = Built::new();
        state.rows = Rows::default();
        state.made = 0..0;
        state.view = None;
        state.before_laid = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(count: usize) -> Rows {
        Rows::new(std::iter::repeat_n(32.0, count))
    }

    #[test]
    fn rows_stand_one_after_another_and_the_list_is_their_sum() {
        let rows = Rows::new([38.0, 32.0, 32.0]);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows.top(0), 0.0);
        assert_eq!(rows.top(1), 38.0);
        assert_eq!(rows.top(2), 70.0);
        assert_eq!(rows.total(), 102.0);
        assert_eq!(rows.height(0), 38.0);
        assert_eq!(rows.top(9), 102.0, "past the end is the end");
    }

    #[test]
    fn a_point_is_in_the_row_that_stands_there() {
        let rows = Rows::new([38.0, 32.0, 32.0]);
        assert_eq!(rows.at(0.0), Some(0));
        assert_eq!(rows.at(37.9), Some(0));
        assert_eq!(rows.at(38.0), Some(1));
        assert_eq!(rows.at(500.0), Some(2));
        assert_eq!(Rows::default().at(3.0), None);
    }

    #[test]
    fn a_window_holds_the_rows_that_touch_it_and_no_others() {
        let rows = flat(10_000);
        // 600 points from the top: rows 0..19 touch it (19 × 32 = 608).
        assert_eq!(rows.window(0.0, 600.0, 0.0), 0..19);
        // Half a row down: the first row is still touched by its bottom half.
        assert_eq!(rows.window(16.0, 64.0, 0.0), 0..3);
        assert_eq!(rows.window(32.0, 64.0, 0.0), 1..3);
        // Far down the list the window is as small as at the top.
        let far = rows.window(160_000.0, 600.0, 0.0);
        assert_eq!(far, 5000..5019);
        // The overscan takes rows on both sides.
        assert_eq!(rows.window(160_000.0, 600.0, 64.0), 4998..5021);
        // Past the end, and nothing to show.
        assert_eq!(rows.window(1.0e9, 600.0, 0.0).len(), 0);
        assert_eq!(flat(0).window(0.0, 600.0, 0.0), 0..0);
        assert_eq!(rows.window(0.0, 0.0, 0.0), 0..0);
    }

    #[test]
    fn the_spacers_and_the_rows_in_the_window_make_up_the_whole_list() {
        let rows = Rows::new((0..500).map(|i| if i % 7 == 0 { 38.0 } else { 32.0 }));
        for offset in [0.0, 13.0, 4000.0, 9999.0, 15_000.0] {
            let window = rows.window(offset, 500.0, 32.0);
            let built: f32 = window.clone().map(|i| rows.height(i)).sum();
            let before = rows.top(window.start);
            let after = rows.total() - rows.top(window.end);
            assert!(
                (before + built + after - rows.total()).abs() < 0.01,
                "offset {offset}"
            );
        }
    }

    #[test]
    fn scrolling_to_a_row_moves_the_least_that_shows_it() {
        let rows = flat(100);
        // In view: stays where it is.
        assert_eq!(rows.reveal(3, 0.0, 320.0), 0.0);
        // Below the window: its bottom edge goes to the window's.
        assert_eq!(rows.reveal(15, 0.0, 320.0), 16.0 * 32.0 - 320.0);
        // Above it: its top edge goes to the window's.
        assert_eq!(rows.reveal(2, 640.0, 320.0), 64.0);
        // A window shorter than the row shows the row's start.
        assert_eq!(rows.reveal(5, 0.0, 10.0), 160.0);
    }

    #[test]
    fn room_around_the_rows_is_in_every_sum_and_the_ends_show_it() {
        let rows = Rows::new([32.0, 32.0, 32.0]).padded(6.0, 8.0);
        assert_eq!(rows.top(0), 6.0);
        assert_eq!(rows.top(1), 38.0);
        assert_eq!(rows.total(), 6.0 + 96.0 + 8.0);
        assert_eq!(
            rows.at(2.0),
            Some(0),
            "the room before the first row is the first row's"
        );
        // The first row is revealed from the very start, the last to the very end.
        assert_eq!(rows.reveal(0, 50.0, 40.0), 0.0);
        assert_eq!(rows.reveal(2, 0.0, 40.0), rows.total() - 40.0);
        // The spacers and the rows still make the whole list.
        let window = rows.window(30.0, 40.0, 0.0);
        let built: f32 = window.clone().map(|i| rows.height(i)).sum();
        assert_eq!(
            rows.top(window.start) + built + (rows.total() - rows.top(window.end)),
            rows.total()
        );
    }

    #[test]
    fn rows_of_one_height_stand_where_a_table_of_their_heights_puts_them() {
        let table = flat(500).padded(6.0, 8.0);
        let even = Rows::even(500, 32.0).padded(6.0, 8.0);
        assert_eq!(even.len(), 500);
        assert_eq!(even.total(), table.total());
        for index in [0, 1, 250, 499, 500, 900] {
            assert_eq!(even.top(index), table.top(index), "top of {index}");
        }
        for y in [0.0, 5.9, 6.0, 37.9, 38.0, 8000.0, 16_005.0, 1.0e6] {
            assert_eq!(even.at(y), table.at(y), "row at {y}");
        }
        for offset in [0.0, 13.0, 4000.0, 9999.0, 15_000.0, 1.0e9] {
            assert_eq!(
                even.window(offset, 500.0, 32.0),
                table.window(offset, 500.0, 32.0),
                "window at {offset}"
            );
        }
        assert_eq!(even.reveal(0, 50.0, 40.0), table.reveal(0, 50.0, 40.0));
        assert_eq!(even.reveal(499, 0.0, 40.0), table.reveal(499, 0.0, 40.0));
        assert!(Rows::even(0, 32.0).is_empty());
        assert_eq!(Rows::even(0, 32.0).window(0.0, 600.0, 0.0), 0..0);
    }

    #[test]
    fn a_list_is_where_its_tops_say_it_is() {
        let rows = Rows::from_tops(vec![0.0, 38.0, 70.0, 102.0]);
        assert_eq!(rows, Rows::new([38.0, 32.0, 32.0]));
        assert_eq!(Rows::from_tops(Vec::new()), Rows::default());
        assert!(Rows::from_tops(vec![0.0]).is_empty());
    }

    /// The page as the host has it: the list goes by `speed` points a frame, the host tells the
    /// page where it went, and the page builds again where [`Built`] says to; the rows the host
    /// shows next are the ones the page built a frame before.
    fn scroll_by_frames(rows: &Rows, view: f32, from: f32, speed: f32, frames: usize) -> usize {
        let mut built = Built::new();
        let end = (rows.total() - view).max(0.0);
        let mut offset = from.clamp(0.0, end);
        let mut span = built.span(rows, offset, view);
        let mut builds = 1;
        for frame in 0..frames {
            offset = (offset + speed).clamp(0.0, end);
            let shown = rows.window(offset, view, 0.0);
            assert!(
                span.start <= shown.start && shown.end <= span.end,
                "frame {frame} at {offset}: {shown:?} shown, {span:?} built"
            );
            if built.stale(rows, offset, view) {
                span = built.span(rows, offset, view);
                builds += 1;
            }
        }
        builds
    }

    #[test]
    fn a_list_that_goes_by_a_view_a_frame_finds_its_rows_built() {
        let rows = flat(20_000);
        let view = 600.0;
        for speed in [1.0, 10.0, 120.0, 300.0, 600.0] {
            scroll_by_frames(&rows, view, 0.0, speed, 2000);
            scroll_by_frames(&rows, view, rows.total(), -speed, 2000);
        }
        // Uneven rows are found built as well.
        let uneven = Rows::new((0..5000).map(|i| if i % 9 == 0 { 64.0 } else { 26.0 }));
        scroll_by_frames(&uneven, view, 0.0, 550.0, 1000);
        scroll_by_frames(&uneven, view, uneven.total(), -550.0, 1000);
    }

    #[test]
    fn a_list_is_built_again_once_in_a_view_and_not_at_every_turn() {
        let rows = flat(20_000);
        let view = 600.0;
        // 20 000 points down, ten at a time: a build in about every 600 points, not 2 000.
        let builds = scroll_by_frames(&rows, view, 0.0, 10.0, 2000);
        assert!((30..=38).contains(&builds), "{builds} builds");
        let up = scroll_by_frames(&rows, view, 640_000.0, -10.0, 2000);
        assert!(
            up.abs_diff(builds) <= 1,
            "as often up ({up}) as down ({builds})"
        );
    }

    #[test]
    fn more_is_built_the_way_the_list_goes() {
        let rows = flat(10_000);
        let view = 640.0;
        let mut built = Built::new();
        // Before it moves, as much is built above what is in view as below it.
        let first = built.span(&rows, 99_000.0, view);
        let in_view = rows.window(99_000.0, view, 0.0);
        assert_eq!(first.end - in_view.end, in_view.start - first.start);
        // It goes down.
        assert!(built.stale(&rows, 100_000.0, view));
        let down = built.span(&rows, 100_000.0, view);
        let in_view = rows.window(100_000.0, view, 0.0);
        assert!(down.end - in_view.end > 2 * (in_view.start - down.start));
        assert!(!built.stale(&rows, 100_500.0, view));
        // It turns: what is built is the same until what is in view comes near its top.
        assert!(!built.stale(&rows, 100_490.0, view));
        assert_eq!(built.span(&rows, 100_490.0, view), down);
        assert!(built.stale(&rows, 100_300.0, view));
        let up = built.span(&rows, 100_300.0, view);
        let in_view = rows.window(100_300.0, view, 0.0);
        assert!(in_view.start - up.start > 2 * (up.end - in_view.end));
        // Asked again where it is, it still goes up.
        assert_eq!(built.span(&rows, 100_300.0, view), up);
    }

    #[test]
    fn at_an_end_of_the_list_all_that_is_built_is_the_other_way() {
        let rows = flat(10_000);
        let view = 640.0;
        let end = rows.total() - view;
        let mut built = Built::new();
        // Gone down to the end, the list can only go back: what is built is two views and a half
        // above it.
        built.span(&rows, end - 5000.0, view);
        let at_end = built.span(&rows, end, view);
        assert_eq!(at_end.end, rows.len());
        assert_eq!(at_end.start, rows.window(end - 2.5 * view, view, 0.0).start);
        // And it goes back up a view and a half before it is built again.
        assert!(!built.stale(&rows, end - 1.4 * view, view));
        assert!(built.stale(&rows, end - 1.6 * view, view));
        // At the top, before it has moved, four views are built below.
        let mut built = Built::new();
        let top = built.span(&rows, 0.0, view);
        assert_eq!(top, rows.window(0.0, 5.0 * view, 0.0));
    }

    #[test]
    fn a_list_that_changed_its_length_is_built_again() {
        let mut built = Built::new();
        assert!(built.stale(&flat(100), 0.0, 600.0), "nothing is built yet");
        let span = built.span(&flat(100), 0.0, 600.0);
        assert!(!built.stale(&flat(100), 0.0, 600.0));
        assert!(built.stale(&flat(90), 0.0, 600.0));
        assert_ne!(built.span(&flat(90), 0.0, 600.0), span);
        assert!(!built.stale(&flat(90), 0.0, 600.0));
        // All of a short list is built, and is never built again for scrolling.
        assert_eq!(built.span(&flat(10), 0.0, 600.0), 0..10);
        assert!(!built.stale(&flat(10), 0.0, 600.0));
        assert_eq!(built.span(&Rows::default(), 0.0, 600.0), 0..0);
        assert!(!built.stale(&Rows::default(), 0.0, 600.0));
    }

    /// A page laid out with the list's box `view` tall, scrolled `offset` down.
    struct Laid {
        id: Id,
        view: f32,
        offset: f32,
    }

    impl Queries for Laid {
        fn bounds(&self, _id: Id) -> Option<crate::Rect> {
            None
        }
        fn ids_at(&self, _at: Point) -> Vec<Id> {
            Vec::new()
        }
        fn hit(&self, _at: Point) -> Option<Id> {
            None
        }
        fn scroll_extent(&self, id: Id) -> Option<crate::Extent> {
            (id == self.id).then(|| crate::Extent {
                content: crate::Size::new(400.0, 32.0 * 10_000.0),
                view: crate::Size::new(400.0, self.view),
                offset: Point::new(0.0, self.offset),
            })
        }
        fn focus_order(&self) -> Vec<Id> {
            Vec::new()
        }
        fn path_of(&self, _id: Id) -> Vec<Id> {
            Vec::new()
        }
        fn drag_source(&self, _at: Point) -> Option<Id> {
            None
        }
        fn drop_target(&self, _at: Point) -> Option<Id> {
            None
        }
    }

    const LIST: Id = Id::named("list");

    fn row(index: usize) -> Element {
        div().h(32.0).id(Id::new(index as u64))
    }

    fn height(element: &Element) -> f32 {
        match element.style.height {
            crate::style::Length::Px(points) => points,
            _ => panic!("a spacer has a height of points"),
        }
    }

    #[test]
    fn a_list_is_a_spacer_the_rows_near_what_is_in_view_and_a_spacer() {
        let list = List::new(LIST);
        let rows = flat(10_000).padded(6.0, 8.0);
        let body = list.build(div(), rows.clone(), 640.0, row);
        assert_eq!(body.meta.id, Some(LIST), "the box is named");
        assert_eq!(
            body.style.overflow,
            crate::style::Overflow::Scroll,
            "and scrolls"
        );
        let made = list.made();
        assert_eq!(
            made,
            rows.window(0.0, 5.0 * 640.0, 0.0),
            "before it moves, four views below"
        );
        assert_eq!(body.children.len(), made.len() + 2);
        let above = height(&body.children[0]);
        let below = height(body.children.last().unwrap());
        assert_eq!(
            above,
            rows.top(made.start),
            "the room above is in the spacer"
        );
        assert_eq!(above + 32.0 * made.len() as f32 + below, rows.total());
        assert_eq!(list.offset(), 0.0);
        assert_eq!(list.view(), 640.0, "not laid out yet: the page");
    }

    #[test]
    fn a_list_is_built_again_only_when_it_is_scrolled_near_the_end_of_its_rows() {
        let mut list = List::new(LIST);
        list.build(div(), flat(10_000), 640.0, row);
        assert!(!list.settle(&Laid {
            id: LIST,
            view: 640.0,
            offset: 0.0
        }));
        let at = |y: f32| [(LIST, Point::new(0.0, y))];
        assert!(!list.scrolled(&at(320.0)), "within the rows made");
        assert_eq!(list.offset(), 320.0, "the page was told");
        assert!(!list.scrolled(&[(Id::named("other"), Point::new(0.0, 9e6))]));
        assert!(list.scrolled(&at(5000.0)), "near their end");
        list.build(div(), flat(10_000), 640.0, row);
        assert!(list.made().contains(&(5000 / 32)));
        assert!(!list.scrolled(&at(5100.0)));
    }

    #[test]
    fn a_list_laid_out_taller_than_its_rows_fill_is_built_again() {
        let mut list = List::new(LIST);
        // Built for a short page, laid out in a tall window: what was made does not fill it.
        list.build(div(), flat(10_000), 100.0, row);
        assert!(list.settle(&Laid {
            id: LIST,
            view: 2000.0,
            offset: 0.0
        }));
        list.build(div(), flat(10_000), 100.0, row);
        assert!(!list.settle(&Laid {
            id: LIST,
            view: 2000.0,
            offset: 0.0
        }));
        assert_eq!(list.view(), 2000.0);
        // Forgotten, it is built afresh: as much above as below what is in view.
        list.scrolled(&[(LIST, Point::new(0.0, 100_000.0))]);
        list.forget();
        assert_eq!(list.view(), 0.0);
        assert_eq!(list.offset(), 100_000.0, "how far it is scrolled stands");
        // Not on the page, it asks for nothing.
        assert!(!list.settle(&Laid {
            id: LIST,
            view: 2000.0,
            offset: 0.0
        }));
        assert!(!list.scrolled(&[(LIST, Point::new(0.0, 100_000.0))]));
        list.build(div(), flat(10_000), 640.0, row);
        let in_view = flat(10_000).window(100_000.0, list.view(), 0.0);
        assert_eq!(
            list.made().end - in_view.end,
            in_view.start - list.made().start
        );
        // A page without the box says nothing of it.
        assert!(!list.settle(&Laid {
            id: Id::named("other"),
            view: 1.0,
            offset: 0.0
        }));
    }

    #[test]
    fn what_the_page_was_told_or_asked_stands_over_where_it_was_laid_out() {
        let mut list = List::new(LIST);
        let rows = flat(100);
        list.build(div(), rows.clone(), 320.0, row);
        list.settle(&Laid {
            id: LIST,
            view: 320.0,
            offset: 64.0,
        });
        assert_eq!(list.offset(), 64.0, "before it is told, as it was laid out");
        let ask = list.scroll_to(200.0);
        assert_eq!(
            ask,
            ScrollTo {
                id: LIST,
                x: None,
                y: Some(200.0),
                animate: false
            }
        );
        list.settle(&Laid {
            id: LIST,
            view: 320.0,
            offset: 64.0,
        });
        assert_eq!(
            list.offset(),
            200.0,
            "what it asked stands until the host says otherwise"
        );
        list.scrolled(&[(LIST, Point::new(0.0, 96.0))]);
        assert_eq!(list.offset(), 96.0);
        // A row in view is not scrolled to; one below is, the least that shows it.
        assert_eq!(list.reveal(&rows, 5), None);
        let ask = list.reveal(&rows, 20).expect("row 20 is below the window");
        assert_eq!(ask.y, Some(21.0 * 32.0 - 320.0));
        assert_eq!(list.offset(), 21.0 * 32.0 - 320.0);
    }

    #[test]
    fn a_list_asked_past_its_end_is_built_at_the_end_of_the_rows_it_is_given() {
        let mut list = List::new(LIST);
        list.scroll_to(1.0e9);
        list.build(div(), flat(100), 640.0, row);
        assert!(!list.settle(&Laid {
            id: LIST,
            view: 200.0,
            offset: 100.0 * 32.0 - 200.0,
        }));
        // Nine hundred rows more, and asked to the end again: the end it has now is built.
        list.scroll_to(1.0e9);
        list.build(div(), flat(1000), 640.0, row);
        assert_eq!(list.made().end, 1000);
    }
}
