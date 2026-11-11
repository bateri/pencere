//! Rows on a grid of cells: a box that is written a height and is a column of boxes that are written
//! theirs has the rows of its height shared out among them, so that its parts are the box; a box that
//! centres what is in it is a number of rows that has a middle. Off a grid nothing changes.

use crate::{Element, Font, Grid, Id, Measure, Metrics, Rect, Size, div, layout, text};

/// A grid of cells eight points across and eighteen down, which text fills a column a letter of.
struct Cells;

impl Measure for Cells {
    fn measure(&mut self, _font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().count() as f32 * 8.0,
            height: 18.0,
        }
    }

    fn grid(&self) -> Option<Grid> {
        Grid::new(8.0, 18.0)
    }
}

/// The same measure, with no grid: the page is laid out in points.
struct Points;

impl Measure for Points {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        Cells.measure(font, text)
    }
}

const BOX: Id = Id::new(1);
const LINE: Id = Id::new(2);
const HEAD: Id = Id::new(10);
const CAPTION: Id = Id::new(11);
const BIG: Id = Id::new(12);
const FIGURES: Id = Id::new(13);
const GRAPH: Id = Id::new(14);
const STRIP: Id = Id::new(15);
const FOOT: Id = Id::new(16);

/// The page `bar` is the box of, laid out on the grid (`on`) or in points.
fn page(bar: Element, on: bool) -> crate::Laid {
    // A row of its own, that packs at the start, so that the box under test is not given rows by a
    // page that is a column of boxes written a height.
    let root = div()
        .row()
        .items_start()
        .w(400.0)
        .h(600.0)
        .child(bar.id(BOX));
    let size = Size::new(400.0, 600.0);
    if on {
        layout(root, size, 1.0, &mut Cells)
    } else {
        layout(root, size, 1.0, &mut Points)
    }
}

/// The height and the line's place in the box of `bar`, laid out on the grid (`on`) or in points.
fn laid(bar: Element, on: bool) -> (f32, f32) {
    let laid = page(bar, on);
    let bar = laid.bounds(BOX).expect("the box");
    let line = laid.bounds(LINE).map_or(0.0, |line| line.y - bar.y);
    (bar.h, line)
}

/// A row that centres one line, `points` tall.
fn centring(points: f32) -> Element {
    div()
        .row()
        .items_center()
        .h(points)
        .child(text("line").id(LINE))
}

/// A card as a page of toppen draws one: padding of 14 and gaps of 6 points around a head of 18, the
/// caption of 14 where it has one, a value of 34, its figures of 32 (two lines, one over the other) and
/// a graph that is what is left, then a line of 16 for a card that is short; `points` tall.
fn card(points: f32) -> Element {
    let compact = points < 150.0;
    let mut used = 28.0 + 18.0 + 6.0;
    let mut card = div()
        .w(300.0)
        .h(points)
        .no_shrink()
        .col()
        .gap(6.0)
        .p(14.0)
        .child(
            div()
                .row()
                .items_center()
                .gap(8.0)
                .h(18.0)
                .no_shrink()
                .child(text("CPU"))
                .id(HEAD),
        );
    if compact {
        used += 16.0 + 6.0;
    } else {
        if points >= 196.0 {
            card = card.child(
                div()
                    .h(14.0)
                    .no_shrink()
                    .child(text("Now, on 8 threads"))
                    .id(CAPTION),
            );
            used += 14.0 + 6.0;
        }
        card = card
            .child(
                div()
                    .row()
                    .items_center()
                    .h(34.0)
                    .no_shrink()
                    .child(text("25%"))
                    .id(BIG),
            )
            .child(
                div()
                    .row()
                    .gap(8.0)
                    .h(32.0)
                    .no_shrink()
                    .child(
                        div()
                            .col()
                            .gap(2.0)
                            .grow(1.0)
                            .child(text("Load"))
                            .child(text("1.19")),
                    )
                    .child(
                        div()
                            .col()
                            .gap(2.0)
                            .grow(1.0)
                            .child(text("Top"))
                            .child(text("39%")),
                    )
                    .id(FIGURES),
            );
        used += 34.0 + 32.0 + 12.0;
    }
    card = card.child(div().h(points - used).id(GRAPH));
    if compact {
        card = card.child(
            div()
                .row()
                .items_center()
                .h(16.0)
                .no_shrink()
                .child(text("load 1.19"))
                .id(FOOT),
        );
    }
    card
}

/// The rows of each of `ids`, and where the first starts and the last ends in the box, in rows.
fn rows(laid: &crate::Laid, ids: &[Id]) -> Vec<f32> {
    ids.iter()
        .map(|id| laid.bounds(*id).expect("a part").h / 18.0)
        .collect()
}

fn of(laid: &crate::Laid, id: Id) -> Rect {
    laid.bounds(id).expect("a part")
}

#[test]
fn a_box_that_centres_its_line_is_an_odd_number_of_rows() {
    // (points, rows): the nearest odd number, the smaller where two are as near, and one at least.
    for (points, rows) in [
        (44.0, 3.0),
        (34.0, 1.0),
        (32.0, 1.0),
        (30.0, 1.0),
        (28.0, 1.0),
        (26.0, 1.0),
        (24.0, 1.0),
        (54.0, 3.0),
        (36.0, 1.0),
        (72.0, 3.0),
        (90.0, 5.0),
        (6.0, 1.0),
    ] {
        let (height, line) = laid(centring(points), true);
        assert_eq!(
            height,
            rows * 18.0,
            "a row of {points} points is {rows} rows"
        );
        assert_eq!(
            line,
            (rows - 1.0) / 2.0 * 18.0,
            "its line has as many rows above as below ({points} points)"
        );
    }
}

#[test]
fn a_box_that_centres_two_lines_is_even() {
    // What is in it has two rows: the box is the nearest even number.
    for (points, rows) in [(44.0, 2.0), (54.0, 2.0), (30.0, 2.0), (80.0, 4.0)] {
        let bar = div()
            .row()
            .items_center()
            .h(points)
            .child(div().col().child(text("one")).child(text("two")));
        assert_eq!(laid(bar, true).0, rows * 18.0, "{points} points");
    }
}

#[test]
fn a_column_that_centres_along_itself_is_the_same() {
    for (points, rows) in [(44.0, 3.0), (34.0, 1.0), (54.0, 3.0)] {
        let bar = div()
            .col()
            .justify_center()
            .h(points)
            .child(text("line").id(LINE));
        let (height, line) = laid(bar, true);
        assert_eq!(height, rows * 18.0, "a column of {points} points");
        assert_eq!(line, (rows - 1.0) / 2.0 * 18.0, "its line is in the middle");
    }
}

#[test]
fn a_box_that_does_not_centre_down_is_the_nearest_number_of_rows() {
    for (points, rows) in [
        (44.0, 2.0),
        (34.0, 2.0),
        (28.0, 2.0),
        (26.0, 1.0),
        (54.0, 3.0),
    ] {
        // A row that centres nothing, a row that packs at the end, a column that centres across
        // (which moves its items sideways), and a box that centres and has nothing in it.
        let plain = div().row().h(points).child(text("line").id(LINE));
        let across = div()
            .col()
            .items_center()
            .h(points)
            .child(text("line").id(LINE));
        let packed = div()
            .row()
            .items_end()
            .h(points)
            .child(text("line").id(LINE));
        for (name, bar) in [("plain", plain), ("across", across), ("end", packed)] {
            assert_eq!(
                laid(bar, true).0,
                rows * 18.0,
                "{name} box of {points} points"
            );
        }
        let empty = div().row().items_center().h(points);
        assert_eq!(
            laid(empty, true).0,
            rows * 18.0,
            "an empty box of {points} points"
        );
    }
}

#[test]
fn the_least_and_the_most_of_a_box_are_as_many_rows_as_they_come_nearest() {
    let least = div()
        .row()
        .items_center()
        .min_h(44.0)
        .child(text("line").id(LINE));
    assert_eq!(laid(least, true).0, 2.0 * 18.0, "at least 44 points");
    let most = div()
        .row()
        .items_center()
        .h(100.0)
        .max_h(34.0)
        .child(text("line").id(LINE));
    assert_eq!(laid(most, true).0, 2.0 * 18.0, "at most 34 points");
    // A least of nothing is nothing, and a share of the parent is the share.
    let nothing = div()
        .row()
        .items_center()
        .min_h(0.0)
        .child(text("line").id(LINE));
    assert_eq!(laid(nothing, true).0, 18.0, "a line of text is a row");
    let share = div()
        .row()
        .items_center()
        .h_full()
        .child(text("line").id(LINE));
    // The page is 33 rows (600 points is 33.3): a share of it is as many rows as it has.
    assert_eq!(laid(share, true).0, 33.0 * 18.0, "a share is not rounded");
}

#[test]
fn a_card_has_the_rows_of_its_height_and_its_parts_are_all_of_them() {
    // (points, the rows of the card, of its head, caption, value, figures, graph and foot): what the
    // rows of each come to by its points, with a line of text no less than a row.
    let cases: [(f32, f32, [f32; 7]); 3] = [
        (124.0, 7.0, [1.0, 0.0, 0.0, 0.0, 3.0, 1.0, 0.0]),
        (164.0, 9.0, [1.0, 0.0, 2.0, 2.0, 2.0, 0.0, 0.0]),
        (214.0, 12.0, [1.0, 1.0, 2.0, 2.0, 4.0, 0.0, 0.0]),
    ];
    for (points, height, parts) in cases {
        let laid = page(card(points), true);
        let outer = of(&laid, BOX);
        assert_eq!(outer.h / 18.0, height, "the card of {points} points");
        let found: Vec<f32> = [HEAD, CAPTION, BIG, FIGURES, GRAPH, FOOT, STRIP]
            .iter()
            .map(|id| laid.bounds(*id).map_or(0.0, |rect| rect.h / 18.0))
            .collect();
        // [head, caption, value, figures, graph, foot, strip]
        assert_eq!(
            [
                found[0], found[1], found[2], found[3], found[4], found[5], found[6]
            ],
            parts,
            "the parts of the card of {points} points"
        );
        // One row of room above and one below, and what is in it fills the rest: no row is left at
        // the foot and none is cut.
        let first = of(&laid, HEAD);
        assert_eq!(first.y - outer.y, 18.0, "a row above ({points} points)");
        let last = [FOOT, GRAPH]
            .iter()
            .filter_map(|id| laid.bounds(*id))
            .map(|rect| rect.y + rect.h)
            .fold(0.0, f32::max);
        assert_eq!(
            outer.y + outer.h - last,
            18.0,
            "a row below ({points} points)"
        );
        // Each part starts where the one before ends: the gaps between them are as many rows as one
        // another (none, where six points are a third of a row).
        let mut ends = Vec::new();
        for id in [HEAD, CAPTION, BIG, FIGURES, GRAPH, FOOT] {
            if let Some(rect) = laid.bounds(id)
                && rect.h > 0.0
            {
                ends.push((rect.y, rect.y + rect.h));
            }
        }
        for pair in ends.windows(2) {
            assert_eq!(
                pair[0].1, pair[1].0,
                "no gap in the card of {points} points"
            );
        }
    }
}

#[test]
fn a_filter_bar_is_three_rows_and_the_head_of_a_table_one() {
    assert_eq!(laid(centring(44.0), true).0, 3.0 * 18.0, "the filter bar");
    assert_eq!(laid(centring(30.0), true).0, 18.0, "the head of a table");
}

#[test]
fn a_head_that_says_two_lines_is_two_rows_and_one_line_is_one() {
    // The head of a table: a row of 30 points that centres its cells, which are as tall as it is and
    // have a title (and under it the type of the column, a second line).
    let cell = |lines: u32| {
        let mut words = div().col().child(text("title"));
        if lines > 1 {
            words = words.child(text("int"));
        }
        div().row().items_center().h_full().px(8.0).child(words)
    };
    for (lines, rows) in [(1, 1.0), (2, 2.0)] {
        let head = div()
            .row()
            .items_center()
            .h(30.0)
            .child(cell(lines))
            .child(cell(1));
        assert_eq!(laid(head, true).0, rows * 18.0, "{lines} line(s) in a cell");
    }
}

/// A list of `count` rows of 32 points with 4 around them, as a palette makes it: a box of the rows'
/// points that scrolls, with the room above and below as spacers.
fn list(count: usize) -> Element {
    let mut list = div()
        .col()
        .scroll()
        .h(count as f32 * 32.0 + 8.0)
        .child(div().h(4.0).no_shrink());
    for index in 0..count {
        list = list.child(
            div()
                .row()
                .items_center()
                .h(32.0)
                .no_shrink()
                .child(text("row"))
                .id(Id::new(100 + index as u64)),
        );
    }
    list.child(div().h(4.0).no_shrink())
}

#[test]
fn ten_rows_of_a_palette_are_ten_rows() {
    // 328 points are 18 rows, and ten rows of 32 are 1.8 of them: the rows do not grow, and the
    // eight that are left are not a row each, so the list is as many rows as it has.
    let laid = page(list(10), true);
    assert_eq!(of(&laid, BOX).h, 10.0 * 18.0, "ten rows");
    for index in 0..10 {
        let row = of(&laid, Id::new(100 + index));
        assert_eq!(row.h, 18.0, "row {index} is a row");
        assert_eq!(row.y, of(&laid, BOX).y + index as f32 * 18.0, "row {index}");
    }
}

#[test]
fn a_few_rows_of_a_palette_are_as_many_rows_each() {
    // Three rows of 32 and 4 around are 104 points, 6 rows: two each, which fit together.
    let laid = page(list(3), true);
    assert_eq!(of(&laid, BOX).h, 6.0 * 18.0, "three rows");
    for index in 0..3 {
        assert_eq!(of(&laid, Id::new(100 + index)).h, 36.0, "row {index}");
    }
}

#[test]
fn parts_that_are_as_many_points_are_as_many_rows() {
    // Five boxes of 20 points with gaps of 10: whatever the rows come to, they are as many for each
    // of the five and for each of the gaps.
    for count in 2..8 {
        let mut bar = div().col().gap(10.0).p(7.0).no_shrink();
        let high = 14.0 + count as f32 * 20.0 + (count - 1) as f32 * 10.0;
        bar = bar.h(high);
        for index in 0..count {
            bar = bar.child(
                div()
                    .h(20.0)
                    .no_shrink()
                    .child(text("part"))
                    .id(Id::new(200 + index as u64)),
            );
        }
        let laid = page(bar, true);
        let heights: Vec<f32> = (0..count)
            .map(|index| of(&laid, Id::new(200 + index as u64)).h)
            .collect();
        assert!(
            heights.windows(2).all(|pair| pair[0] == pair[1]),
            "{count} equal parts are as many rows as one another: {heights:?}"
        );
        let tops: Vec<f32> = (0..count)
            .map(|index| of(&laid, Id::new(200 + index as u64)).y)
            .collect();
        let steps: Vec<f32> = tops.windows(2).map(|pair| pair[1] - pair[0]).collect();
        assert!(
            steps.windows(2).all(|pair| pair[0] == pair[1]),
            "and the gaps between them are: {steps:?}"
        );
    }
}

#[test]
fn what_is_in_a_box_is_all_of_it() {
    // Whatever the points, a column's parts are its rows: what is above the first part is what is
    // below the last (the paddings are as many points on each side), and the parts stand end to end
    // with the same room between them, so that no row is left over and none cut.
    let mut seed: u32 = 7;
    let mut next = |count: u32| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) % count
    };
    for round in 0..200 {
        let parts = 1 + next(5) as usize;
        let pad = (next(3) * 6) as f32;
        let gap = (next(4) * 3) as f32;
        let heights: Vec<f32> = (0..parts).map(|_| 14.0 + next(7) as f32 * 6.0).collect();
        let high = 2.0 * pad + gap * (parts - 1) as f32 + heights.iter().sum::<f32>();
        let mut bar = div().col().gap(gap).py(pad).h(high).no_shrink();
        for (index, height) in heights.iter().enumerate() {
            bar = bar.child(
                div()
                    .h(*height)
                    .no_shrink()
                    .child(text("part"))
                    .id(Id::new(300 + index as u64)),
            );
        }
        let laid = page(bar, true);
        let outer = of(&laid, BOX);
        let first = of(&laid, Id::new(300));
        let last = of(&laid, Id::new(300 + parts as u64 - 1));
        assert_eq!(
            first.y - outer.y,
            outer.y + outer.h - (last.y + last.h),
            "round {round}: as much above as below ({heights:?}, pad {pad}, gap {gap})"
        );
        let mut at = first.y;
        let mut step = None;
        for index in 0..parts {
            let part = of(&laid, Id::new(300 + index as u64));
            assert!(
                part.h >= 18.0,
                "round {round}: a line of text is a row ({heights:?})"
            );
            if index > 0 {
                let room = part.y - at;
                assert_eq!(
                    *step.get_or_insert(room),
                    room,
                    "round {round}: the same room between the parts ({heights:?}, gap {gap})"
                );
            }
            at = part.y + part.h;
        }
        assert_eq!(outer.h % 18.0, 0.0, "round {round}: a whole number of rows");
    }
}

#[test]
fn a_column_that_centres_boxes_of_its_own_leaves_as_much_above_as_below() {
    // 100 points are 6 rows, and the box in it is 2: two rows above and two below.
    let bar = div()
        .col()
        .justify_center()
        .h(100.0)
        .child(div().h(36.0).no_shrink().child(text("line")).id(LINE));
    let laid = page(bar, true);
    let outer = of(&laid, BOX);
    let inner = of(&laid, LINE);
    assert_eq!(outer.h, 6.0 * 18.0);
    assert_eq!(inner.y - outer.y, outer.y + outer.h - (inner.y + inner.h));
}

#[test]
fn a_box_given_its_rows_has_them() {
    // The graph is not written a height of its own that could be kept: the card has the rows that it
    // has, and a line of text in a part of it is a row.
    let laid = page(card(164.0), true);
    assert_eq!(
        rows(&laid, &[HEAD, BIG, FIGURES, GRAPH]),
        [1.0, 2.0, 2.0, 2.0]
    );
}

#[test]
fn a_page_with_no_grid_is_laid_out_as_it_was() {
    for points in [44.0, 34.0, 32.0, 28.0, 26.0, 24.0, 54.0, 36.0] {
        let (height, _) = laid(centring(points), false);
        assert_eq!(height, points, "a row of {points} points off a grid");
    }
    // A card is its points, and what is in it is its own.
    for points in [124.0, 164.0, 214.0] {
        let laid = page(card(points), false);
        assert_eq!(of(&laid, BOX).h, points, "a card of {points} points");
        assert_eq!(of(&laid, HEAD).h, 18.0);
        assert_eq!(
            of(&laid, HEAD).y - of(&laid, BOX).y,
            14.0,
            "padding of 14 points"
        );
        if points > 150.0 {
            assert_eq!(of(&laid, BIG).h, 34.0);
            assert_eq!(of(&laid, FIGURES).h, 32.0);
            let before = if points >= 196.0 { CAPTION } else { HEAD };
            let ends = of(&laid, before).y + of(&laid, before).h;
            assert_eq!(of(&laid, BIG).y - ends, 6.0, "a gap of 6 points");
        }
    }
    // A list of rows in points.
    let laid = page(list(10), false);
    assert_eq!(of(&laid, BOX).h, 328.0);
    assert_eq!(of(&laid, Id::new(100)).h, 32.0);
}
