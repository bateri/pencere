//! The engine's guards for `outline`: a ring drawn over a box and what is inside it, that takes no
//! room and answers no pointer, cut by the boxes around the box and not by the box itself.

use crate::{
    Color, DrawList, Font, Id, Measure, Metrics, Paint, Quad, Rect, Role, Size, div, layout, text,
};

struct Fixed;

impl Measure for Fixed {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().count() as f32 * font.size * 0.5,
            height: font.size * 1.25,
            ascent: font.size,
        }
    }
}

const BOX: Id = Id::new(1);
const PAGE: Rect = Rect::new(0.0, 0.0, 200.0, 100.0);

fn lay(root: crate::Element) -> crate::Laid {
    layout(root, Size::new(200.0, 100.0), 1.0, &mut Fixed)
}

fn drawn(root: crate::Element) -> DrawList {
    lay(root).paint(&mut Fixed)
}

/// The quads that are a ring alone: a border and no fill.
fn rings(list: &DrawList) -> Vec<&Quad> {
    list.items
        .iter()
        .filter_map(|item| match item {
            Paint::Quad(quad) if quad.fill.is_none() && quad.border.is_some() => Some(quad),
            _ => None,
        })
        .collect()
}

#[test]
fn an_outline_takes_no_room_and_moves_nothing() {
    let page = |ringed: bool| {
        let boxed = div().id(BOX).size(40.0, 20.0).rounded(4.0);
        let boxed = if ringed {
            boxed.outline(2.0, Role::Accent, 1.0)
        } else {
            boxed
        };
        div()
            .p(10.0)
            .gap(6.0)
            .child(boxed)
            .child(div().size(40.0, 20.0))
    };
    let with: Vec<Rect> = lay(page(true)).rects().collect();
    let without: Vec<Rect> = lay(page(false)).rects().collect();
    assert_eq!(with, without);
}

#[test]
fn the_ring_is_drawn_after_what_is_inside_the_box_and_stands_off_it_by_the_offset_and_its_width() {
    let list = drawn(
        div().p(10.0).child(
            div()
                .id(BOX)
                .size(60.0, 30.0)
                .bg(Role::Background)
                .outline(2.0, Role::Accent, 1.0)
                .child(text("inside")),
        ),
    );
    let ring = rings(&list);
    assert_eq!(ring.len(), 1);
    // The box is at 10, 10; the ring is its border 3 points out (1 of offset and 2 of width).
    assert_eq!(ring[0].rect, Rect::new(7.0, 7.0, 66.0, 36.0));
    assert_eq!(ring[0].border.map(|border| border.width), Some(2.0));
    // After the words in the box, the last thing drawn.
    let last = list.items.last();
    assert!(matches!(last, Some(Paint::Quad(quad)) if quad.fill.is_none()));
}

#[test]
fn the_corners_of_a_ring_follow_the_box_and_a_box_with_square_corners_has_a_square_ring() {
    let ring = |boxed: crate::Element| {
        let list = drawn(div().p(10.0).child(boxed.id(BOX).size(60.0, 30.0)));
        rings(&list)[0].radii
    };
    // The radius of the box and what the ring reaches out (1 of offset, 2 of width): the inside of
    // the ring is as round as the box is, 1 point off it.
    assert_eq!(
        ring(div().rounded(4.0).outline(2.0, Role::Accent, 1.0)),
        [7.0; 4]
    );
    assert_eq!(
        ring(
            div()
                .rounded_corners([4.0, 0.0, 8.0, 0.0])
                .outline(2.0, Role::Accent, 1.0)
        ),
        [7.0, 0.0, 11.0, 0.0]
    );
    assert_eq!(ring(div().outline(2.0, Role::Accent, 1.0)), [0.0; 4]);
}

#[test]
fn an_offset_less_than_nothing_draws_the_ring_inside_the_box() {
    let list =
        drawn(
            div()
                .p(10.0)
                .child(div().id(BOX).size(60.0, 30.0).rounded(6.0).outline(
                    2.0,
                    Role::Accent,
                    -2.0,
                )),
        );
    let ring = rings(&list)[0];
    // Its outer edge is the box's: the box's own rectangle and corners.
    assert_eq!(ring.rect, Rect::new(10.0, 10.0, 60.0, 30.0));
    assert_eq!(ring.radii, [6.0; 4]);
    // A ring that reaches past the middle of a box that is small is not drawn.
    let none = drawn(div().child(div().size(4.0, 4.0).outline(2.0, Role::Accent, -4.0)));
    assert!(rings(&none).is_empty());
}

#[test]
fn a_ring_is_cut_by_the_box_around_the_box_and_not_by_the_box_that_has_it() {
    // The box clips what is in it, and the ring is outside its edge: the page around it lets all of
    // it be seen.
    let own = drawn(
        div().p(10.0).child(
            div()
                .id(BOX)
                .size(60.0, 30.0)
                .clip()
                .outline(2.0, Role::Accent, 1.0)
                .child(text("inside")),
        ),
    );
    assert_eq!(rings(&own)[0].clip.rect, PAGE);
    // In a box that clips, the ring is cut at that box's padding box.
    let held =
        drawn(
            div().child(
                div().size(50.0, 50.0).p(10.0).clip().child(
                    div()
                        .id(BOX)
                        .size(30.0, 20.0)
                        .outline(2.0, Role::Accent, 1.0),
                ),
            ),
        );
    assert_eq!(rings(&held)[0].clip.rect, Rect::new(0.0, 0.0, 50.0, 50.0));
}

#[test]
fn a_ring_that_is_clear_is_not_drawn_and_one_of_no_width_is_none() {
    let clear: Color = Color::from(Role::Accent).faded(0.0);
    let list = drawn(div().child(div().size(40.0, 20.0).outline(2.0, clear, 1.0)));
    assert!(rings(&list).is_empty());
    let thin = drawn(div().child(div().size(40.0, 20.0).outline(0.0, Role::Accent, 1.0)));
    assert!(rings(&thin).is_empty());
    let odd = drawn(div().child(div().size(40.0, 20.0).outline(2.0, Role::Accent, f32::NAN)));
    assert!(rings(&odd).is_empty());
}

#[test]
fn a_ring_shows_as_much_as_the_box_that_has_it_and_goes_with_it() {
    let list = drawn(
        div().child(
            div()
                .size(40.0, 20.0)
                .opacity(0.5)
                .outline(2.0, Role::Accent, 1.0),
        ),
    );
    assert_eq!(
        rings(&list)[0].border.map(|border| border.color.alpha),
        Some(0.5)
    );
    let hidden = drawn(
        div().child(
            div()
                .size(40.0, 20.0)
                .hidden()
                .outline(2.0, Role::Accent, 1.0),
        ),
    );
    assert!(rings(&hidden).is_empty());
}

#[test]
fn a_ring_is_drawn_under_an_overlay_written_in_its_box_as_all_else_is() {
    // The box is the last of the page, so that the overlay that follows it in the paint order is
    // written inside it: the ring is not drawn over what drops from the box.
    let list = drawn(
        div().child(
            div()
                .id(BOX)
                .size(40.0, 20.0)
                .outline(2.0, Role::Accent, 1.0)
                .child(div().overlay().size(30.0, 10.0).bg(Role::Foreground)),
        ),
    );
    let at = |wanted: bool| {
        list.items.iter().position(|item| match item {
            Paint::Quad(quad) => (quad.fill.is_none() && quad.border.is_some()) == wanted,
            _ => false,
        })
    };
    let (ring, overlay) = (at(true).expect("a ring"), at(false).expect("an overlay"));
    assert!(
        ring < overlay,
        "the ring at {ring}, the overlay at {overlay}"
    );
}

#[test]
fn a_ring_is_over_the_boxes_that_follow_its_box_in_the_page_as_a_style_sheets_is() {
    // The box under the ringed one has a fill that reaches the ring's outer 3 points.
    let list = drawn(
        div()
            .p(10.0)
            .child(
                div()
                    .id(BOX)
                    .size(40.0, 20.0)
                    .outline(2.0, Role::Accent, 1.0),
            )
            .child(div().size(40.0, 20.0).bg(Role::Foreground)),
    );
    let at = |wanted: bool| {
        list.items.iter().position(|item| match item {
            Paint::Quad(quad) => (quad.fill.is_none() && quad.border.is_some()) == wanted,
            _ => false,
        })
    };
    let (ring, fill) = (at(true).expect("a ring"), at(false).expect("a fill"));
    assert!(fill < ring, "the fill at {fill}, the ring at {ring}");
}

#[test]
fn the_ring_of_a_box_in_a_box_with_a_ring_is_drawn_before_it() {
    let list = drawn(
        div().p(20.0).child(
            div()
                .size(80.0, 40.0)
                .p(10.0)
                .outline(2.0, Role::Foreground, 1.0)
                .child(
                    div()
                        .id(BOX)
                        .size(40.0, 10.0)
                        .outline(2.0, Role::Accent, 1.0),
                ),
        ),
    );
    let rings = rings(&list);
    assert_eq!(rings.len(), 2);
    // The inner one first, and the outer over it.
    assert!(rings[0].rect.w < rings[1].rect.w);
}
