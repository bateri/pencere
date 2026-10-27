//! The engine's guards for the host's scrolling ([`crate::Scrolls`]): a box moved on the page
//! already laid out is where laying it out scrolled puts it; a gesture stays with the box it
//! started on; a box pulled past its end springs back; what the page asks is done; the bars show,
//! go, are drawn where they belong and can be dragged. The host's measuring is a fixed-width
//! stand-in.

use crate::{
    Bars, Element, Font, Id, Laid, Measure, Metrics, Paint, Phase, Point, Role, ScrollTo, Scrolls,
    Size, div, layout, text,
};

struct Fixed;

impl Measure for Fixed {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().count() as f32 * font.size * 0.5,
            height: font.size * 1.25,
        }
    }
}

const LIST: Id = Id::new(1);
const INNER: Id = Id::new(2);
const COVER: Id = Id::new(3);

fn rows(n: u64) -> impl Iterator<Item = Element> {
    (0..n).map(|n| {
        div()
            .h(20.0)
            .no_shrink()
            .press(Id::new(100 + n))
            .child(text("row"))
    })
}

/// A header over a list of 10 rows of 20 points (80 of them shown) on a page of 200 × 100.
fn list() -> Element {
    div()
        .col()
        .size_full()
        .child(div().h(20.0).no_shrink())
        .child(
            div()
                .id(LIST)
                .col()
                .grow(1.0)
                .min_h(0.0)
                .scroll()
                .children(rows(10)),
        )
}

fn lay(root: Element) -> Laid {
    layout(root, Size::new(200.0, 100.0), 1.0, &mut Fixed)
}

/// The page laid out and placed by `scrolls` at `now`.
fn placed(scrolls: &mut Scrolls, root: Element, now: f64) -> Laid {
    let mut root = root;
    scrolls.apply(&mut root);
    let mut laid = lay(root);
    scrolls.place(&mut laid, now, false);
    laid
}

fn ask(id: Id, y: f32, animate: bool) -> ScrollTo {
    ScrollTo {
        id,
        x: None,
        y: Some(y),
        animate,
    }
}

fn row_y(laid: &Laid, n: u64) -> f32 {
    laid.bounds(Id::new(100 + n)).map_or(f32::NAN, |r| r.y)
}

const OVER: Point = Point::new(50.0, 60.0);

#[test]
fn a_box_moved_on_the_page_laid_out_is_where_laying_it_out_scrolled_puts_it() {
    // A list that scrolls, with a box that clips and a list that scrolls inside it.
    let page = |outer: f32, inner: f32| {
        div().size(200.0, 100.0).child(
            div()
                .id(LIST)
                .col()
                .size(200.0, 100.0)
                .scroll()
                .scrolled(0.0, outer)
                .child(div().h(30.0).no_shrink().child(text("head")))
                .child(
                    div().h(25.0).no_shrink().clip().child(
                        div()
                            .h(50.0)
                            .no_shrink()
                            .bg(Role::Accent)
                            .child(text("cut")),
                    ),
                )
                .child(
                    div()
                        .id(INNER)
                        .col()
                        .h(40.0)
                        .no_shrink()
                        .scroll()
                        .scrolled(0.0, inner)
                        .children(rows(6)),
                )
                .children(rows(8)),
        )
    };
    let laid_scrolled = lay(page(35.0, 12.0));
    let mut moved = lay(page(0.0, 0.0));
    let mut scrolls = Scrolls::new();
    scrolls.set_bars(Bars::Hidden);
    scrolls.ask(ask(LIST, 35.0, false));
    scrolls.ask(ask(INNER, 12.0, false));
    scrolls.place(&mut moved, 0.0, false);
    assert_eq!(
        moved.nodes, laid_scrolled.nodes,
        "every box, and what each lets be drawn"
    );
    assert_eq!(moved.paint(&mut Fixed), laid_scrolled.paint(&mut Fixed));
    assert_eq!(moved.scroll_extent(LIST).map(|e| e.offset.y), Some(35.0));
}

#[test]
fn the_page_is_told_where_its_boxes_went_once() {
    let mut scrolls = Scrolls::new();
    let laid = placed(&mut scrolls, list(), 0.0);
    assert!(scrolls.wheel(&laid, OVER, 0.0, 30.0, Phase::Wheel, 0.0));
    assert!(scrolls.wheel(&laid, OVER, 0.0, 10.0, Phase::Wheel, 0.0));
    assert_eq!(scrolls.take_moved(), vec![(LIST, Point::new(0.0, 40.0))]);
    assert!(scrolls.take_moved().is_empty());
    // The rows are moved on the page already laid out, and the page asks of it where they are.
    let mut laid = laid;
    scrolls.place(&mut laid, 0.0, false);
    assert_eq!(row_y(&laid, 2), 20.0 + 40.0 - 40.0);
    assert_eq!(laid.hit(Point::new(5.0, 25.0)), Some(Id::new(102)));
}

/// A list of four rows of 20 in a box of 40, inside a list of the page.
fn nested() -> Element {
    div().col().size_full().child(
        div()
            .id(LIST)
            .col()
            .size(200.0, 100.0)
            .scroll()
            .child(
                div()
                    .id(INNER)
                    .col()
                    .h(40.0)
                    .no_shrink()
                    .scroll()
                    .children(rows(4)),
            )
            .children(rows(8)),
    )
}

#[test]
fn a_gesture_stays_with_the_box_it_started_on_and_the_next_one_goes_on() {
    let mut scrolls = Scrolls::new();
    let laid = placed(&mut scrolls, nested(), 0.0);
    let over_inner = Point::new(50.0, 10.0);
    assert!(scrolls.wheel(&laid, over_inner, 0.0, 0.0, Phase::Begin, 0.0));
    assert!(scrolls.wheel(&laid, over_inner, 0.0, 30.0, Phase::Move, 0.0));
    assert!(scrolls.wheel(&laid, over_inner, 0.0, 30.0, Phase::Move, 0.0));
    assert_eq!(scrolls.offset(INNER).y, 40.0, "the inner list at its end");
    assert_eq!(
        scrolls.offset(LIST).y,
        0.0,
        "and the gesture did not go on to the outer one"
    );
    scrolls.wheel(&laid, over_inner, 0.0, 0.0, Phase::Lift, 0.0);
    // A new gesture down over the inner list, which cannot go further: the outer one moves.
    assert!(scrolls.wheel(&laid, over_inner, 0.0, 0.0, Phase::Begin, 1.0));
    assert!(scrolls.wheel(&laid, over_inner, 0.0, 30.0, Phase::Move, 1.0));
    assert_eq!(scrolls.offset(LIST).y, 30.0);
    assert_eq!(scrolls.offset(INNER).y, 40.0);
}

#[test]
fn a_box_pulled_past_its_end_springs_back_when_the_fingers_leave() {
    let mut scrolls = Scrolls::new();
    scrolls.set_elastic(true);
    let mut laid = placed(&mut scrolls, list(), 0.0);
    scrolls.wheel(&laid, OVER, 0.0, 0.0, Phase::Begin, 0.0);
    assert!(scrolls.wheel(&laid, OVER, 0.0, -100.0, Phase::Move, 0.0));
    assert_eq!(scrolls.offset(LIST).y, 0.0, "still at the start");
    scrolls.place(&mut laid, 0.0, false);
    let pulled = row_y(&laid, 0) - 20.0;
    assert!(
        pulled > 10.0 && pulled < 80.0,
        "drawn down, less than the fingers went: {pulled}"
    );
    assert!(
        scrolls.take_moved().is_empty(),
        "the page is not told of a pull"
    );
    scrolls.wheel(&laid, OVER, 0.0, 0.0, Phase::Lift, 0.0);
    assert!(scrolls.moving(0.0));
    scrolls.place(&mut laid, 0.1, false);
    let coming = row_y(&laid, 0) - 20.0;
    assert!(coming > 0.0 && coming < pulled, "on its way back: {coming}");
    scrolls.place(&mut laid, 1.0, false);
    assert_eq!(row_y(&laid, 0), 20.0, "back at the start");
    assert!(!scrolls.moving(1.0) || scrolls.wake_at(1.0).is_some());
}

#[test]
fn momentum_that_runs_into_an_end_bounces_once_and_is_spent() {
    let mut scrolls = Scrolls::new();
    scrolls.set_elastic(true);
    let mut laid = placed(&mut scrolls, list(), 0.0);
    scrolls.wheel(&laid, OVER, 0.0, 0.0, Phase::Begin, 0.0);
    scrolls.wheel(&laid, OVER, 0.0, 100.0, Phase::Move, 0.0);
    scrolls.wheel(&laid, OVER, 0.0, 0.0, Phase::Lift, 0.0);
    assert!(scrolls.wheel(&laid, OVER, 0.0, 40.0, Phase::Coast, 0.02));
    assert_eq!(
        scrolls.offset(LIST).y,
        120.0,
        "at the end, the rest of it a bounce"
    );
    scrolls.place(&mut laid, 0.02, false);
    assert!(scrolls.moving(0.02), "springing back");
    assert!(
        scrolls.wheel(&laid, OVER, 0.0, -40.0, Phase::Coast, 0.04),
        "what momentum is left is the box's, and moves nothing"
    );
    assert_eq!(scrolls.offset(LIST).y, 120.0);
    scrolls.wheel(&laid, OVER, 0.0, 0.0, Phase::Stop, 0.05);
}

#[test]
fn without_elastic_or_with_motion_reduced_nothing_is_pulled() {
    for (elastic, reduce) in [(false, false), (true, true)] {
        let mut scrolls = Scrolls::new();
        scrolls.set_elastic(elastic);
        let mut laid = lay(list());
        scrolls.place(&mut laid, 0.0, reduce);
        scrolls.wheel(&laid, OVER, 0.0, 0.0, Phase::Begin, 0.0);
        scrolls.wheel(&laid, OVER, 0.0, -100.0, Phase::Move, 0.0);
        scrolls.place(&mut laid, 0.0, reduce);
        assert_eq!(row_y(&laid, 0), 20.0, "elastic {elastic}, reduce {reduce}");
    }
}

#[test]
fn a_page_asks_and_the_box_glides_there_and_past_the_end_is_the_end() {
    let mut scrolls = Scrolls::new();
    let mut laid = placed(&mut scrolls, list(), 0.0);
    scrolls.ask(ask(LIST, 1.0e9, true));
    scrolls.place(&mut laid, 1.0, false);
    assert_eq!(scrolls.offset(LIST).y, 0.0, "it starts where it was");
    assert!(scrolls.moving(1.0));
    scrolls.place(&mut laid, 1.1, false);
    let on_the_way = scrolls.offset(LIST).y;
    assert!(on_the_way > 0.0 && on_the_way < 120.0, "{on_the_way}");
    scrolls.place(&mut laid, 1.5, false);
    assert_eq!(scrolls.offset(LIST).y, 120.0);
    assert_eq!(row_y(&laid, 9), 20.0 + 180.0 - 120.0);
    // With motion reduced, at once.
    scrolls.ask(ask(LIST, 0.0, true));
    scrolls.place(&mut laid, 2.0, true);
    assert_eq!(scrolls.offset(LIST).y, 0.0);
}

#[test]
fn an_ask_for_a_box_that_is_not_on_the_page_yet_waits_for_it() {
    let mut scrolls = Scrolls::new();
    let mut empty = lay(div());
    scrolls.ask(ask(LIST, 60.0, false));
    scrolls.place(&mut empty, 0.0, false);
    let laid = placed(&mut scrolls, list(), 0.1);
    assert_eq!(scrolls.offset(LIST).y, 60.0);
    assert_eq!(row_y(&laid, 3), 20.0);
}

fn knobs(laid: &Laid) -> Vec<crate::Quad> {
    laid.paint(&mut Fixed)
        .items
        .into_iter()
        .filter_map(|item| match item {
            Paint::Quad(quad)
                if quad
                    .fill
                    .is_some_and(|fill| fill.alpha < 0.7 && fill.alpha > 0.0) =>
            {
                Some(quad)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn overlay_bars_show_while_a_box_moves_and_go_a_while_after() {
    let mut scrolls = Scrolls::new();
    let mut laid = placed(&mut scrolls, list(), 0.0);
    assert!(
        knobs(&laid).is_empty(),
        "a box that has not moved shows none"
    );
    scrolls.wheel(&laid, OVER, 0.0, 60.0, Phase::Wheel, 0.0);
    scrolls.place(&mut laid, 0.0, false);
    let shown = knobs(&laid);
    assert_eq!(shown.len(), 1, "the knob down the right edge");
    let knob = shown.first().map(|q| q.rect).unwrap_or_default();
    // 80 of 200 shown: the knob is 2/5 of the track, half way down it at 60 of 120.
    assert!(knob.x > 190.0 && knob.right() <= 200.0, "{knob:?}");
    let track = 80.0 - 4.0;
    assert!((knob.h - track * 0.4).abs() < 0.01, "{knob:?}");
    assert!(
        (knob.y - (20.0 + 2.0 + (track - knob.h) * 0.5)).abs() < 0.01,
        "{knob:?}"
    );
    assert_eq!(
        scrolls.wake_at(0.0),
        Some(0.9),
        "the knob starts to go then"
    );
    scrolls.place(&mut laid, 1.0, false);
    assert!(scrolls.moving(1.0), "going");
    let fading = knobs(&laid);
    assert!(
        fading
            .first()
            .is_some_and(|q| q.fill.is_some_and(|f| f.alpha < 0.42))
    );
    scrolls.place(&mut laid, 2.0, false);
    assert!(knobs(&laid).is_empty());
    assert!(!scrolls.moving(2.0));
    // Always: there without moving.
    scrolls.set_bars(Bars::Always);
    scrolls.place(&mut laid, 3.0, false);
    assert!(!knobs(&laid).is_empty());
}

#[test]
fn a_box_s_bars_are_drawn_over_what_it_holds_and_under_what_covers_it() {
    let mut scrolls = Scrolls::new();
    scrolls.set_bars(Bars::Always);
    let page = || {
        list().child(
            div()
                .id(COVER)
                .overlay()
                .inset(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .bg(Role::Raised(2)),
        )
    };
    let laid = placed(&mut scrolls, page(), 0.0);
    let items = laid.paint(&mut Fixed).items;
    let place_of = |find: &dyn Fn(&Paint) -> bool| items.iter().position(find);
    let knob = place_of(
        &|item| matches!(item, Paint::Quad(q) if q.fill.is_some_and(|f| f.alpha > 0.3 && f.alpha < 0.5)),
    );
    let last_row = place_of(&|item| matches!(item, Paint::Text(run) if run.origin.y >= 180.0));
    let cover = place_of(
        &|item| matches!(item, Paint::Quad(q) if q.fill.is_some_and(|f| f.source == crate::Source::Role(Role::Raised(2)))),
    );
    assert!(knob.is_some() && cover.is_some(), "{items:?}");
    assert!(
        last_row < knob && knob < cover,
        "{last_row:?} {knob:?} {cover:?}"
    );
}

#[test]
fn a_knob_dragged_moves_the_box_and_a_click_in_its_track_goes_a_page() {
    let mut scrolls = Scrolls::new();
    scrolls.set_bars(Bars::Always);
    let mut laid = placed(&mut scrolls, list(), 0.0);
    // The knob: 30.4 points long at the top of a track from 22 to 98.
    let on_knob = Point::new(196.0, 30.0);
    assert!(scrolls.press(on_knob, 0.0));
    assert!(scrolls.holding());
    assert!(scrolls.point(Some(Point::new(196.0, 30.0 + 22.8)), 0.0));
    let room = 76.0 - 76.0 * 0.4;
    assert!((scrolls.offset(LIST).y - 22.8 * 120.0 / room).abs() < 0.01);
    assert!(scrolls.release(0.0));
    assert!(!scrolls.holding());
    // Below the knob: a page down, over a glide.
    let was = scrolls.offset(LIST).y;
    scrolls.place(&mut laid, 0.0, false);
    assert!(scrolls.press(Point::new(196.0, 96.0), 1.0));
    assert!(!scrolls.holding());
    scrolls.place(&mut laid, 2.0, false);
    assert!((scrolls.offset(LIST).y - (was + 72.0).min(120.0)).abs() < 0.01);
    // Away from the bars a press is the page's.
    assert!(!scrolls.press(Point::new(100.0, 50.0), 3.0));
}

#[test]
fn a_click_in_the_track_goes_there_where_the_system_says_so() {
    let mut scrolls = Scrolls::new();
    scrolls.set_bars(Bars::Always);
    scrolls.set_track_jumps(true);
    let _laid = placed(&mut scrolls, list(), 0.0);
    assert!(scrolls.press(Point::new(196.0, 96.0), 0.0));
    assert_eq!(
        scrolls.offset(LIST).y,
        120.0,
        "the knob's middle under the pointer, at the end"
    );
    assert!(scrolls.holding(), "and held from there");
}
