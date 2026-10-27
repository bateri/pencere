//! A long list's arithmetic: where each row stands, which rows a window of the list shows, and
//! how far to scroll to see one.
//!
//! A page that shows a list builds only the rows in view — the elements of ten thousand
//! rows are not made to show thirty — and keeps the rest of the list's length with a spacer
//! above and one below. The rows' heights are known without laying them out (a list row is
//! the same height every time), so their places are a sum. Which rows to build as the list
//! scrolls, and when to build them again, is [`Built`]'s.

use std::ops::Range;

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
const AHEAD: f32 = 1.5;
const BEHIND: f32 = 0.5;
/// How near the end of what is built, the way the list goes, what is in view comes before the page
/// is built again, in views.
const NEAR: f32 = 0.75;

/// Which rows of a long list a page built, and which way the list goes: kept between frames, so
/// that the page builds a view and a half of rows ahead of what is in view and half a view behind
/// it, and is built again only when what is in view comes within three quarters of a view of the
/// end of what was built, the way the list goes — not at every turn of the wheel, and never short
/// of what scrolling brings into view while it keeps up.
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
    fn a_list_that_goes_by_three_quarters_of_a_view_a_frame_finds_its_rows_built() {
        let rows = flat(20_000);
        let view = 600.0;
        for speed in [1.0, 10.0, 120.0, 300.0, 450.0] {
            scroll_by_frames(&rows, view, 0.0, speed, 2000);
            scroll_by_frames(&rows, view, rows.total(), -speed, 2000);
        }
        // Uneven rows are found built as well.
        let uneven = Rows::new((0..5000).map(|i| if i % 9 == 0 { 64.0 } else { 26.0 }));
        scroll_by_frames(&uneven, view, 0.0, 400.0, 1000);
        scroll_by_frames(&uneven, view, uneven.total(), -400.0, 1000);
    }

    #[test]
    fn a_list_is_built_again_once_in_three_quarters_of_a_view_and_not_at_every_turn() {
        let rows = flat(20_000);
        let view = 600.0;
        // 20 000 points down, ten at a time: a build in about every 450 points, not 2 000.
        let builds = scroll_by_frames(&rows, view, 0.0, 10.0, 2000);
        assert!((40..=50).contains(&builds), "{builds} builds");
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
        assert!(!built.stale(&rows, 100_300.0, view));
        // It turns: what is built is the same until what is in view comes near its top.
        assert!(!built.stale(&rows, 100_290.0, view));
        assert_eq!(built.span(&rows, 100_290.0, view), down);
        assert!(built.stale(&rows, 100_100.0, view));
        let up = built.span(&rows, 100_100.0, view);
        let in_view = rows.window(100_100.0, view, 0.0);
        assert!(in_view.start - up.start > 2 * (up.end - in_view.end));
        // Asked again where it is, it still goes up.
        assert_eq!(built.span(&rows, 100_100.0, view), up);
    }

    #[test]
    fn at_an_end_of_the_list_all_that_is_built_is_the_other_way() {
        let rows = flat(10_000);
        let view = 640.0;
        let end = rows.total() - view;
        let mut built = Built::new();
        // Gone down to the end, the list can only go back: what is built is two views above it.
        built.span(&rows, end - 5000.0, view);
        let at_end = built.span(&rows, end, view);
        assert_eq!(at_end.end, rows.len());
        assert_eq!(at_end.start, rows.window(end - 2.0 * view, view, 0.0).start);
        // And it goes back up a view and a quarter before it is built again.
        assert!(!built.stale(&rows, end - 1.2 * view, view));
        assert!(built.stale(&rows, end - 1.3 * view, view));
        // At the top, before it has moved, three views are built below.
        let mut built = Built::new();
        let top = built.span(&rows, 0.0, view);
        assert_eq!(top, rows.window(0.0, 4.0 * view, 0.0));
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
}
