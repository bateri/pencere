//! The engine's guards for the zoom of a box that scrolls ([`crate::Element::zoom`]): a pinch keeps
//! the point under the fingers where it is; a scale stays in the range the page gave and is pulled
//! past it only where the host is elastic, and comes back; the wheel with the command key held
//! zooms and the plain wheel scrolls; a page asks for a scale and gets it, over a moment or at
//! once; what is inside is laid out at the scale (text, pictures, what a click lands on); the page
//! is told the scale. The host's measuring is a fixed-width stand-in.

use std::collections::HashMap;

use crate::{
    Element, Fit, Font, GesturePhase, Id, Laid, Measure, Metrics, Paint, Phase, Point, Rect,
    ScrollTo, Scrolls, Size, Zoom, div, image, layout, text,
};

struct Fixed {
    pictures: HashMap<&'static str, Size>,
}

impl Fixed {
    fn new() -> Self {
        Self {
            pictures: HashMap::new(),
        }
    }
}

impl Measure for Fixed {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().count() as f32 * font.size * 0.5,
            height: font.size * 1.25,
        }
    }

    fn image(&mut self, src: &str, _page: u32) -> Option<Size> {
        self.pictures.get(src).copied()
    }
}

const BOX: Id = Id::new(1);
const WORDS: Id = Id::new(2);
const PLAIN: Id = Id::new(3);

/// 20 rows of 20 points (400 of them) in a box of 200 × 100 that may be zoomed.
fn page(rule: Zoom) -> Element {
    div().size(200.0, 100.0).child(
        div()
            .id(BOX)
            .col()
            .size(200.0, 100.0)
            .scroll()
            .zoom(rule)
            .child(
                div()
                    .h(20.0)
                    .no_shrink()
                    .items_start()
                    .child(text("row").id(WORDS)),
            )
            .children((1..20).map(|n| div().h(20.0).no_shrink().press(Id::new(100 + n)))),
    )
}

fn lay(root: Element, host: &mut Fixed) -> Laid {
    layout(root, Size::new(200.0, 100.0), 1.0, host)
}

/// The page laid out and placed by `scrolls` at `now`.
fn placed(scrolls: &mut Scrolls, root: Element, now: f64) -> Laid {
    let mut root = root;
    scrolls.apply(&mut root);
    let mut laid = lay(root, &mut Fixed::new());
    scrolls.place(&mut laid, now, false);
    laid
}

/// What `scrolls` have of the page after a frame at `now`: the way the drive does it.
fn frame(scrolls: &mut Scrolls, rule: Zoom, now: f64) -> Laid {
    scrolls.advance_zoom(now);
    placed(scrolls, page(rule), now)
}

fn row_y(laid: &Laid, n: u64) -> f32 {
    laid.bounds(Id::new(100 + n)).map_or(f32::NAN, |r| r.y)
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

const OVER: Point = Point::new(50.0, 60.0);

fn pinch(scrolls: &mut Scrolls, laid: &Laid, at: Point, by: &[f32]) {
    assert!(scrolls.magnify(laid, at, 0.0, GesturePhase::Begin, 0.0));
    for by in by {
        assert!(scrolls.magnify(laid, at, *by, GesturePhase::Move, 0.0));
    }
}

#[test]
fn a_pinch_keeps_the_point_under_the_fingers_where_it_is() {
    let rule = Zoom::default();
    let mut scrolls = Scrolls::new();
    let laid = placed(&mut scrolls, page(rule), 0.0);
    // The top of the fourth row is under the fingers.
    assert!(near(row_y(&laid, 3), 60.0));
    pinch(&mut scrolls, &laid, OVER, &[1.0]);
    scrolls.magnify(&laid, OVER, 0.0, GesturePhase::End, 0.0);
    let laid = frame(&mut scrolls, rule, 0.0);
    assert!(near(scrolls.zoom_of(BOX), 2.0));
    // Twice as large, and the same edge where the fingers are; the rows are twice as tall.
    assert!(near(row_y(&laid, 3), 60.0), "{}", row_y(&laid, 3));
    assert!(near(row_y(&laid, 4), 100.0), "{}", row_y(&laid, 4));
    // The pinch goes on from there: the fingers apart again by half is three times what it was.
    pinch(&mut scrolls, &laid, Point::new(50.0, 90.0), &[0.5]);
    let laid = frame(&mut scrolls, rule, 0.0);
    assert!(near(scrolls.zoom_of(BOX), 3.0));
    // The edge of the fifth row was 10 points under the fingers' own and is 15 now.
    assert!(near(row_y(&laid, 4), 105.0), "{}", row_y(&laid, 4));
}

#[test]
fn a_click_lands_on_what_is_under_it_at_the_scale() {
    let rule = Zoom::default();
    let mut scrolls = Scrolls::new();
    let laid = placed(&mut scrolls, page(rule), 0.0);
    pinch(&mut scrolls, &laid, OVER, &[1.0]);
    let laid = frame(&mut scrolls, rule, 0.0);
    let under = |y: f32| laid.ids_at(Point::new(50.0, y));
    // The third row was at 40 to 60 and is at 20 to 60, the fourth at 60 to 100.
    assert!(under(30.0).contains(&Id::new(102)));
    assert!(!under(30.0).contains(&Id::new(101)));
    assert!(under(61.0).contains(&Id::new(103)));
    assert!(under(99.0).contains(&Id::new(103)));
}

#[test]
fn what_is_inside_is_laid_out_at_the_scale() {
    let rule = Zoom::default();
    let mut scrolls = Scrolls::new();
    let laid = placed(&mut scrolls, page(rule), 0.0);
    let at_one = laid.bounds(WORDS).expect("the words").w;
    pinch(&mut scrolls, &laid, OVER, &[1.0]);
    let laid = frame(&mut scrolls, rule, 0.0);
    assert!(near(laid.bounds(WORDS).expect("the words").w, 2.0 * at_one));
    // The box itself stays the size it was given.
    assert_eq!(laid.bounds(BOX), Some(Rect::new(0.0, 0.0, 200.0, 100.0)));
    let extent = laid.scroll_extent(BOX).expect("a box that scrolls");
    assert!(near(extent.content.h, 800.0), "{:?}", extent);
    assert!(near(extent.zoom, 2.0));
}

#[test]
fn a_picture_is_asked_at_the_size_it_is_shown() {
    let mut host = Fixed::new();
    host.pictures.insert("a.png", Size::new(40.0, 20.0));
    let rule = Zoom::default();
    let root = || {
        div().size(200.0, 100.0).child(
            div()
                .id(BOX)
                .size(200.0, 100.0)
                .scroll()
                .zoom(rule)
                .child(image("a.png", Fit::Contain).size(100.0, 50.0).no_shrink()),
        )
    };
    let mut scrolls = Scrolls::new();
    let mut at_one = root();
    scrolls.apply(&mut at_one);
    let mut laid = layout(at_one, Size::new(200.0, 100.0), 1.0, &mut host);
    scrolls.place(&mut laid, 0.0, false);
    let shown = |laid: &Laid, host: &mut Fixed| {
        laid.paint(host)
            .items
            .iter()
            .find_map(|item| match item {
                Paint::Image(picture) => Some(picture.rect.w),
                _ => None,
            })
            .unwrap_or(f32::NAN)
    };
    assert!(near(shown(&laid, &mut host), 100.0));
    pinch(&mut scrolls, &laid, Point::new(10.0, 10.0), &[1.0]);
    let mut root = root();
    scrolls.apply(&mut root);
    let mut laid = layout(root, Size::new(200.0, 100.0), 1.0, &mut host);
    scrolls.place(&mut laid, 0.0, false);
    assert!(near(shown(&laid, &mut host), 200.0));
}

#[test]
fn a_scale_stays_in_the_range_the_page_gave() {
    let rule = Zoom {
        min: 0.5,
        max: 3.0,
        ..Zoom::default()
    };
    let mut scrolls = Scrolls::new();
    let laid = placed(&mut scrolls, page(rule), 0.0);
    pinch(&mut scrolls, &laid, OVER, &[9.0]);
    let laid = frame(&mut scrolls, rule, 0.0);
    assert!(near(scrolls.zoom_of(BOX), 3.0), "{}", scrolls.zoom_of(BOX));
    // Not elastic: the content is not drawn past it either.
    assert!(near(row_y(&laid, 2) - row_y(&laid, 1), 60.0));
    pinch(&mut scrolls, &laid, OVER, &[-0.99]);
    frame(&mut scrolls, rule, 0.0);
    assert!(near(scrolls.zoom_of(BOX), 0.5), "{}", scrolls.zoom_of(BOX));
}

#[test]
fn a_scale_pulled_past_its_range_is_drawn_past_it_and_comes_back() {
    let rule = Zoom {
        min: 1.0,
        max: 2.0,
        ..Zoom::default()
    };
    let mut scrolls = Scrolls::new();
    scrolls.set_elastic(true);
    let laid = placed(&mut scrolls, page(rule), 0.0);
    pinch(&mut scrolls, &laid, OVER, &[1.0, 0.5]);
    let drawn = frame(&mut scrolls, rule, 0.0);
    // The page is told the end of the range; what is drawn goes past it, by less than the fingers.
    assert!(near(scrolls.zoom_of(BOX), 2.0));
    let height = row_y(&drawn, 2) - row_y(&drawn, 1);
    assert!(height > 40.0 && height < 60.0, "{height}");
    // Let go: it springs back to the end of the range, and stops.
    assert!(scrolls.magnify(&drawn, OVER, 0.0, GesturePhase::End, 0.0));
    assert!(scrolls.zooming());
    let mut now = 0.0;
    let mut frames = 0;
    let mut last = frame(&mut scrolls, rule, now);
    while scrolls.moving(now) {
        now += 1.0 / 60.0;
        last = frame(&mut scrolls, rule, now);
        frames += 1;
        assert!(frames < 600, "it does not come to rest");
    }
    assert!(near(row_y(&last, 2) - row_y(&last, 1), 40.0));
    assert!(!scrolls.zooming());
    assert!(near(scrolls.zoom_of(BOX), 2.0));
}

#[test]
fn the_wheel_with_the_command_key_zooms_and_the_plain_wheel_scrolls() {
    let rule = Zoom::default();
    let mut scrolls = Scrolls::new();
    let laid = placed(&mut scrolls, page(rule), 0.0);
    // The turn that moves the content up makes it smaller: past the size the page gave there is
    // nothing smaller, so the box stays where it is; down makes it larger.
    assert!(scrolls.zoom_wheel(&laid, OVER, -50.0, 0.0));
    let laid = frame(&mut scrolls, rule, 0.0);
    assert!(near(scrolls.zoom_of(BOX), (50.0f32 * 0.008).exp()));
    // Outside the box nothing zooms.
    assert!(!scrolls.zoom_wheel(&laid, Point::new(500.0, 500.0), -50.0, 0.0));
    // The plain wheel scrolls the box, and does not zoom it.
    let before = scrolls.zoom_of(BOX);
    let wheeled = scrolls.wheel(&laid, OVER, 0.0, -30.0, Phase::Wheel, 0.0);
    assert!(wheeled.took);
    assert!(near(scrolls.zoom_of(BOX), before));
}

#[test]
fn a_box_that_does_not_zoom_is_not_zoomed_by_a_pinch_or_a_turn() {
    let mut scrolls = Scrolls::new();
    let plain = || {
        div().size(200.0, 100.0).child(
            div()
                .id(PLAIN)
                .size(200.0, 100.0)
                .scroll()
                .child(div().h(400.0).no_shrink()),
        )
    };
    let laid = placed(&mut scrolls, plain(), 0.0);
    assert!(!scrolls.magnify(&laid, OVER, 0.5, GesturePhase::Begin, 0.0));
    assert!(!scrolls.magnify(&laid, OVER, 0.5, GesturePhase::Move, 0.0));
    assert!(!scrolls.smart_magnify(&laid, OVER, 0.0));
    assert!(!scrolls.zoom_wheel(&laid, OVER, -50.0, 0.0));
    assert!(!scrolls.zooming());
    assert!(near(scrolls.zoom_of(PLAIN), 1.0));
}

#[test]
fn a_double_tap_goes_to_the_size_the_page_says_and_back() {
    let rule = Zoom {
        smart: 2.5,
        ..Zoom::default()
    };
    let mut scrolls = Scrolls::new();
    let laid = placed(&mut scrolls, page(rule), 0.0);
    assert!(scrolls.smart_magnify(&laid, OVER, 0.0));
    // It glides: where it is a moment in is between, and after the glide it is there.
    let laid = frame(&mut scrolls, rule, 0.05);
    let between = scrolls.zoom_of(BOX);
    assert!(between > 1.0 && between < 2.5, "{between}");
    assert!(scrolls.zooming());
    let laid = {
        let _ = laid;
        frame(&mut scrolls, rule, 5.0)
    };
    assert!(near(scrolls.zoom_of(BOX), 2.5));
    // The point that was tapped stays where it was while it went.
    assert!(near(row_y(&laid, 3), 60.0), "{}", row_y(&laid, 3));
    assert!(scrolls.smart_magnify(&laid, OVER, 6.0));
    frame(&mut scrolls, rule, 12.0);
    assert!(near(scrolls.zoom_of(BOX), 1.0));
}

#[test]
fn a_double_tap_goes_at_once_where_motion_is_reduced() {
    let rule = Zoom::default();
    let mut scrolls = Scrolls::new();
    let mut root = page(rule);
    scrolls.apply(&mut root);
    let mut laid = lay(root, &mut Fixed::new());
    scrolls.place(&mut laid, 0.0, true);
    assert!(scrolls.smart_magnify(&laid, OVER, 0.0));
    scrolls.advance_zoom(0.0);
    assert!(near(scrolls.zoom_of(BOX), 2.0));
    assert!(!scrolls.moving(0.0));
}

fn ask(scale: f32, at: Option<Point>, animate: bool) -> ScrollTo {
    ScrollTo {
        id: BOX,
        x: None,
        y: None,
        animate,
        zoom: Some(scale),
        at,
    }
}

#[test]
fn a_page_asks_for_a_scale_and_gets_it_at_once_or_over_a_moment() {
    let rule = Zoom::default();
    let mut scrolls = Scrolls::new();
    let laid = placed(&mut scrolls, page(rule), 0.0);
    // At once, about a point of the page.
    scrolls.ask(ask(3.0, Some(OVER), false));
    assert!(scrolls.zooming());
    let laid2 = frame(&mut scrolls, rule, 0.0);
    assert!(near(scrolls.zoom_of(BOX), 3.0));
    assert!(near(row_y(&laid2, 3), 60.0), "{}", row_y(&laid2, 3));
    // Past the range is the end of it.
    scrolls.ask(ask(40.0, None, false));
    frame(&mut scrolls, rule, 1.0);
    assert!(near(scrolls.zoom_of(BOX), 4.0));
    // Over a moment: not there at the first frame, there at the last.
    scrolls.ask(ask(1.0, None, true));
    frame(&mut scrolls, rule, 2.0);
    assert!(scrolls.zoom_of(BOX) > 1.5);
    assert!(scrolls.moving(2.0));
    frame(&mut scrolls, rule, 8.0);
    assert!(near(scrolls.zoom_of(BOX), 1.0));
    assert!(!scrolls.moving(8.0));
    let _ = laid;
}

#[test]
fn an_ask_for_a_scale_of_a_box_not_on_the_page_waits_for_it() {
    let rule = Zoom::default();
    let mut scrolls = Scrolls::new();
    scrolls.ask(ask(2.0, None, false));
    scrolls.advance_zoom(0.0);
    assert!(near(scrolls.zoom_of(BOX), 1.0));
    // It comes: the ask is done.
    let _ = placed(&mut scrolls, page(rule), 0.0);
    frame(&mut scrolls, rule, 0.0);
    assert!(near(scrolls.zoom_of(BOX), 2.0));
}

#[test]
fn the_page_is_told_the_scale_once_with_how_far_the_box_is_scrolled() {
    let rule = Zoom::default();
    let mut scrolls = Scrolls::new();
    let laid = placed(&mut scrolls, page(rule), 0.0);
    assert!(scrolls.take_moved().is_empty());
    pinch(&mut scrolls, &laid, OVER, &[1.0]);
    let moved = scrolls.take_moved();
    assert_eq!(moved.len(), 1);
    assert_eq!(moved[0].id, BOX);
    assert!(near(moved[0].zoom, 2.0), "{}", moved[0].zoom);
    assert!(near(moved[0].offset.y, 60.0), "{:?}", moved[0].offset);
    assert!(scrolls.take_moved().is_empty());
}

const OTHER: Id = Id::new(4);

/// The box that zooms and a box that scrolls and does not, one above the other.
fn two(rule: Zoom) -> Element {
    div()
        .col()
        .size(200.0, 200.0)
        .child(
            div()
                .id(BOX)
                .col()
                .size(200.0, 100.0)
                .scroll()
                .zoom(rule)
                .child(div().h(400.0).no_shrink()),
        )
        .child(
            div()
                .id(OTHER)
                .col()
                .size(200.0, 100.0)
                .scroll()
                .child(div().h(400.0).no_shrink()),
        )
}

#[test]
fn an_ask_past_the_range_is_answered_with_where_the_box_is() {
    let rule = Zoom {
        max: 3.0,
        ..Zoom::default()
    };
    let mut scrolls = Scrolls::new();
    let laid = placed(&mut scrolls, page(rule), 0.0);
    pinch(&mut scrolls, &laid, OVER, &[9.0]);
    scrolls.magnify(&laid, OVER, 0.0, GesturePhase::End, 0.0);
    frame(&mut scrolls, rule, 0.0);
    assert!(near(scrolls.zoom_of(BOX), 3.0));
    assert_eq!(scrolls.take_moved().len(), 1);
    // At the end of its range already: the scale does not change, and the page that asked for 40
    // is told it is at 3 all the same.
    scrolls.ask(ask(40.0, None, false));
    frame(&mut scrolls, rule, 1.0);
    let told = scrolls.take_moved();
    assert_eq!(told.len(), 1);
    assert!(near(told[0].zoom, 3.0), "{}", told[0].zoom);
    assert!(scrolls.take_moved().is_empty());
}

#[test]
fn an_ask_of_a_box_that_does_not_zoom_is_answered_and_costs_no_frame() {
    let rule = Zoom::default();
    let mut scrolls = Scrolls::new();
    placed(&mut scrolls, two(rule), 0.0);
    assert!(scrolls.take_moved().is_empty());
    scrolls.ask(ScrollTo {
        id: OTHER,
        ..ask(2.0, None, true)
    });
    // Nothing can be done of it: it asks for no frame.
    assert!(!scrolls.zooming());
    assert!(!scrolls.moving(0.0));
    scrolls.advance_zoom(0.0);
    assert!(!scrolls.zooming());
    let told = scrolls.take_moved();
    assert_eq!(told.len(), 1);
    assert_eq!((told[0].id, told[0].zoom), (OTHER, 1.0));
    // And it is gone: nothing is asked of the next frame either.
    scrolls.advance_zoom(1.0);
    assert!(scrolls.take_moved().is_empty());
    assert!(!scrolls.zooming());
}

#[test]
fn an_ask_of_a_box_not_yet_on_the_page_waits_without_a_frame_and_is_done_when_it_comes() {
    let rule = Zoom::default();
    let mut scrolls = Scrolls::new();
    scrolls.ask(ask(2.0, None, false));
    assert!(!scrolls.zooming() && !scrolls.moving(0.0));
    // The first layout of the box: the next frame is asked for, which does it.
    placed(&mut scrolls, page(rule), 0.0);
    assert!(scrolls.zooming() && scrolls.moving(0.0));
    frame(&mut scrolls, rule, 0.0);
    assert!(near(scrolls.zoom_of(BOX), 2.0));
    assert!(!scrolls.zooming());
}

#[test]
fn an_ask_of_a_box_that_says_no_zoom_is_answered_where_it_is_first_laid_out() {
    let rule = Zoom::default();
    let mut scrolls = Scrolls::new();
    scrolls.ask(ScrollTo {
        id: OTHER,
        ..ask(2.0, None, false)
    });
    placed(&mut scrolls, two(rule), 0.0);
    assert!(!scrolls.zooming() && !scrolls.moving(0.0));
    let told = scrolls.take_moved();
    assert_eq!(told.len(), 1);
    assert_eq!((told[0].id, told[0].zoom), (OTHER, 1.0));
}

const OUTER: Id = Id::new(5);

/// A box that may be zoomed under 60 points of a list that scrolls: the list is 100 high, the box 100
/// with 20 rows of 20 points.
fn under_a_list(rule: Zoom) -> Element {
    div()
        .id(OUTER)
        .col()
        .size(200.0, 100.0)
        .scroll()
        .child(div().h(60.0).no_shrink())
        .child(
            div()
                .id(BOX)
                .col()
                .size(200.0, 100.0)
                .no_shrink()
                .scroll()
                .zoom(rule)
                .children((0..20).map(|n| div().h(20.0).no_shrink().press(Id::new(100 + n)))),
        )
}

#[test]
fn a_pinch_in_a_box_that_the_list_it_is_in_moved_since_the_page_was_laid_out_holds_where_it_is_now()
{
    let rule = Zoom::default();
    let mut scrolls = Scrolls::new();
    let mut laid = placed(&mut scrolls, under_a_list(rule), 0.0);
    // The list is turned over its first 60 points, so that the box is 40 up, and the page is placed
    // again without being laid out.
    scrolls.wheel(&laid, Point::new(50.0, 30.0), 0.0, 40.0, Phase::Wheel, 0.1);
    scrolls.place(&mut laid, 0.1, false);
    assert!(near(laid.bounds(BOX).map_or(f32::NAN, |r| r.y), 20.0));
    // The third row's top is under the fingers (40 into the box).
    pinch(&mut scrolls, &laid, Point::new(50.0, 60.0), &[1.0]);
    scrolls.magnify(&laid, Point::new(50.0, 60.0), 0.0, GesturePhase::End, 0.1);
    scrolls.advance_zoom(0.2);
    let laid = placed(&mut scrolls, under_a_list(rule), 0.2);
    assert!(near(scrolls.zoom_of(BOX), 2.0));
    assert!(near(row_y(&laid, 2), 60.0), "{}", row_y(&laid, 2));
}

/// Three rows of 20 points with a gap of 10 between them, in a box that may be zoomed.
fn gapped(rule: Option<Zoom>) -> Element {
    let mut inner = div().id(BOX).col().size(200.0, 100.0).scroll().gap(10.0);
    if let Some(rule) = rule {
        inner = inner.zoom(rule);
    }
    div()
        .size(200.0, 100.0)
        .child(inner.children((0..3).map(|n| div().h(20.0).no_shrink().press(Id::new(100 + n)))))
}

#[test]
fn the_gap_between_what_a_zoomed_box_holds_is_as_large_as_what_it_holds() {
    let rule = Zoom::default();
    let mut scrolls = Scrolls::new();
    let laid = placed(&mut scrolls, gapped(Some(rule)), 0.0);
    assert!(near(row_y(&laid, 2) - row_y(&laid, 1), 30.0));
    pinch(&mut scrolls, &laid, OVER, &[1.0]);
    scrolls.advance_zoom(0.0);
    let laid = placed(&mut scrolls, gapped(Some(rule)), 0.0);
    // Twice as large: the rows 40 tall and the gap 20.
    assert!(near(scrolls.zoom_of(BOX), 2.0));
    assert!(
        near(row_y(&laid, 2) - row_y(&laid, 1), 60.0),
        "{}",
        row_y(&laid, 2) - row_y(&laid, 1)
    );
}

#[test]
fn a_range_that_is_not_where_the_box_is_brings_it_in_and_the_page_is_told() {
    // A least of two: the box is first laid out at one, and comes up to its least.
    let rule = Zoom {
        min: 2.0,
        max: 4.0,
        ..Zoom::default()
    };
    let mut scrolls = Scrolls::new();
    placed(&mut scrolls, page(rule), 0.0);
    assert!(near(scrolls.zoom_of(BOX), 2.0), "{}", scrolls.zoom_of(BOX));
    assert!(scrolls.zooming(), "laid out again at the scale it came to");
    let moved = scrolls.take_moved();
    assert_eq!(moved.len(), 1, "{moved:?}");
    assert!(near(moved[0].zoom, 2.0));
    let laid = frame(&mut scrolls, rule, 0.0);
    assert!(near(row_y(&laid, 2) - row_y(&laid, 1), 40.0));
    // The most comes down to a smaller one the same way.
    let narrower = Zoom {
        min: 1.0,
        max: 1.5,
        ..Zoom::default()
    };
    frame(&mut scrolls, narrower, 0.0);
    assert!(near(scrolls.zoom_of(BOX), 1.5), "{}", scrolls.zoom_of(BOX));
}

#[test]
fn a_box_that_stops_saying_a_zoom_is_at_its_size_and_the_page_is_told() {
    let rule = Zoom::default();
    let mut scrolls = Scrolls::new();
    let laid = placed(&mut scrolls, page(rule), 0.0);
    pinch(&mut scrolls, &laid, OVER, &[1.0]);
    frame(&mut scrolls, rule, 0.0);
    scrolls.take_moved();
    assert!(near(scrolls.zoom_of(BOX), 2.0));
    let mut plain = page(rule);
    plain.children[0].style.zoom = None;
    scrolls.advance_zoom(0.0);
    placed(&mut scrolls, plain, 0.0);
    assert!(near(scrolls.zoom_of(BOX), 1.0));
    let moved = scrolls.take_moved();
    assert_eq!(moved.len(), 1, "{moved:?}");
    assert!(near(moved[0].zoom, 1.0));
}

#[test]
fn a_wheel_that_does_not_turn_up_or_down_with_the_command_key_zooms_nothing() {
    let rule = Zoom::default();
    let mut scrolls = Scrolls::new();
    let laid = placed(&mut scrolls, page(rule), 0.0);
    assert!(!scrolls.zoom_wheel(&laid, OVER, 0.0, 0.0));
}

#[test]
fn a_pinch_that_began_where_no_box_zooms_is_the_pages_to_its_end() {
    let rule = Zoom::default();
    let mut scrolls = Scrolls::new();
    let laid = placed(&mut scrolls, page(rule), 0.0);
    let outside = Point::new(500.0, 500.0);
    assert!(!scrolls.magnify(&laid, outside, 0.0, GesturePhase::Begin, 0.0));
    // The fingers came over the box that zooms: the pinch is still the page's.
    assert!(!scrolls.magnify(&laid, OVER, 0.5, GesturePhase::Move, 0.0));
    assert!(!scrolls.magnify(&laid, OVER, 0.0, GesturePhase::End, 0.0));
    assert!(near(scrolls.zoom_of(BOX), 1.0));
    // The next one is the box's.
    assert!(scrolls.magnify(&laid, OVER, 0.0, GesturePhase::Begin, 0.0));
}
