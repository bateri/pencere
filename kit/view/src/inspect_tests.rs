//! The guards of the inspection of a page laid out: every element's place in the tree it was
//! written in, its box and room, the style the page gave it, and what is under a point.

use crate::{
    Display, Element, Font, Id, Inspected, Laid, Length, Measure, Metrics, Motion, Motions, Point,
    Role, Shape, Shift, Size, div, layout, text,
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

const PAGE: Size = Size::new(200.0, 100.0);

fn lay(root: &Element, scale: f32) -> Laid {
    layout(
        root.clone(),
        Size::new(PAGE.w * scale, PAGE.h * scale),
        scale,
        &mut Fixed,
    )
}

fn at<'a>(items: &'a [Inspected], path: &[usize]) -> &'a Inspected {
    items
        .iter()
        .find(|item| item.path.as_deref() == Some(path))
        .unwrap_or_else(|| panic!("no element at {path:?}"))
}

/// A page of boxes none of which is named: a column in a padding, two boxes in it, and one in the
/// first.
fn plain() -> Element {
    div().col().p(10.0).children([
        div()
            .h(30.0)
            .mt(4.0)
            .p(2.0)
            .child(div().w(10.0).h(10.0).border(1.0, Role::Accent)),
        div().h(20.0),
    ])
}

#[test]
fn an_element_with_no_name_has_its_place_box_room_and_style() {
    let page = plain();
    let items = lay(&page, 1.0).inspect(&page);
    assert_eq!(items.len(), 4);
    let places: Vec<_> = items.iter().map(|item| item.path.clone()).collect();
    assert_eq!(
        places,
        [Some(vec![]), Some(vec![0]), Some(vec![0, 0]), Some(vec![1])],
        "the tree's order"
    );
    assert!(items.iter().all(|item| item.id.is_none()));

    let root = at(&items, &[]);
    assert_eq!((root.rect.w, root.rect.h), (200.0, 100.0));
    assert_eq!(root.padding, [10.0; 4]);
    assert_eq!(root.parent, None);

    let first = at(&items, &[0]);
    assert_eq!(
        (first.rect.x, first.rect.y),
        (10.0, 14.0),
        "past the padding and its margin"
    );
    assert_eq!((first.rect.w, first.rect.h), (180.0, 30.0));
    assert_eq!(first.padding, [2.0; 4]);
    assert_eq!(first.parent, Some(0));
    let placing = first.placing.as_ref().expect("it wrote its style");
    assert_eq!(
        placing.margin,
        [4.0, 0.0, 0.0, 0.0],
        "the margin is the style's: no box has it"
    );
    assert_eq!(placing.height, Length::Px(30.0));
    assert_eq!(placing.display, Display::Flex);

    let inside = at(&items, &[0, 0]);
    assert_eq!(
        (inside.rect.x, inside.rect.y, inside.rect.w, inside.rect.h),
        (12.0, 16.0, 10.0, 10.0),
        "inside the room of the box that holds it"
    );
    assert_eq!(inside.border.expect("a border").width, 1.0);
    assert_eq!(inside.parent, Some(1));
    assert_eq!(inside.shape, Shape::Box);

    let second = at(&items, &[1]);
    assert_eq!(second.rect.y, 44.0);
    assert!(!second.hidden && !second.overlay && !second.exiting);
}

#[test]
fn what_is_said_is_in_points_whatever_the_scale() {
    let page = plain();
    let one = lay(&page, 1.0).inspect(&page);
    let two = lay(&page, 2.0).inspect(&page);
    assert_eq!(one, two);
}

#[test]
fn a_text_says_it_is_one_and_not_what_it_says() {
    let page = div().child(text("a secret"));
    let items = lay(&page, 1.0).inspect(&page);
    assert_eq!(at(&items, &[0]).shape, Shape::Text);
    assert!(!format!("{items:?}").contains("secret"));
}

const BASE: Id = Id::new(1);
const HOLDER: Id = Id::new(2);
const SHEET: Id = Id::new(3);

/// A page with a layer in the box at its second place, written before the box that follows it.
fn layered() -> Element {
    div().col().children([
        div().id(BASE).h(40.0),
        div().id(HOLDER).h(40.0).children([
            div().h(10.0),
            div()
                .id(SHEET)
                .overlay()
                .inset(Some(10.0), None, None, Some(10.0))
                .w(60.0)
                .h(30.0)
                .child(div().w(20.0).h(10.0).mt(3.0)),
        ]),
        div().h(20.0),
    ])
}

#[test]
fn a_layer_and_what_is_in_it_have_the_place_they_were_written_in() {
    let page = layered();
    let laid = lay(&page, 1.0);
    let items = laid.inspect(&page);
    let places: Vec<_> = items.iter().filter_map(|item| item.path.clone()).collect();
    assert_eq!(
        places,
        [
            vec![],
            vec![0],
            vec![1],
            vec![1, 0],
            vec![1, 1],
            vec![1, 1, 0],
            vec![2]
        ],
        "the layer is where it was written, not at the end of what is painted"
    );

    let sheet = at(&items, &[1, 1]);
    assert_eq!(sheet.id, Some(SHEET));
    assert!(sheet.overlay);
    assert_eq!(
        (sheet.rect.x, sheet.rect.y),
        (10.0, 10.0),
        "against the page"
    );
    assert_eq!(
        items
            .get(sheet.parent.expect("held"))
            .and_then(|held| held.id),
        Some(HOLDER)
    );
    let placing = sheet.placing.as_ref().expect("its style");
    assert!(placing.overlay);
    assert_eq!(placing.inset, [Some(10.0), None, None, Some(10.0)]);

    let inside = at(&items, &[1, 1, 0]);
    assert_eq!((inside.rect.x, inside.rect.y), (10.0, 13.0));
    assert_eq!(
        inside.placing.as_ref().map(|placing| placing.margin),
        Some([3.0, 0.0, 0.0, 0.0]),
        "the style of that element and not of the one beside it in the paint order"
    );
    assert_eq!(
        at(&items, &[2])
            .placing
            .as_ref()
            .map(|placing| placing.height),
        Some(Length::Px(20.0))
    );
    assert_eq!(
        items
            .get(inside.parent.expect("held"))
            .and_then(|held| held.id),
        Some(SHEET)
    );
}

#[test]
fn what_is_over_the_rest_is_found_at_a_point_first_whatever_the_order_it_was_written_in() {
    let page = div().col().children([
        div()
            .overlay()
            .inset(Some(10.0), None, None, Some(10.0))
            .w(40.0)
            .h(40.0),
        div().h(50.0),
    ]);
    let laid = lay(&page, 1.0);
    assert_eq!(laid.inspect_at(Point::new(20.0, 20.0)), Some(vec![0]));
    assert_eq!(laid.inspect_at(Point::new(100.0, 20.0)), Some(vec![1]));
    assert_eq!(
        laid.inspect_at(Point::new(100.0, 80.0)),
        Some(vec![]),
        "the page, where nothing else is"
    );
    assert_eq!(laid.inspect_at(Point::new(-5.0, 20.0)), None);
}

#[test]
fn what_a_box_cuts_off_is_not_found_where_it_is_cut_and_a_box_not_shown_is_not_found_at_all() {
    // The box that cuts is after the one before it in the paint order, so what it holds is over
    // that box where it reaches past its own: only the cut says what is found there.
    let page = div().col().children([
        div().h(30.0),
        div()
            .h(20.0)
            .clip()
            .child(div().h(50.0).no_shrink().child(div().h(50.0).no_shrink())),
        div().hidden().child(div().h(10.0)),
    ]);
    let laid = lay(&page, 1.0);
    assert_eq!(laid.inspect_at(Point::new(5.0, 10.0)), Some(vec![0]));
    assert_eq!(
        laid.inspect_at(Point::new(5.0, 40.0)),
        Some(vec![1, 0, 0]),
        "inside the box that cuts"
    );
    assert_eq!(
        laid.inspect_at(Point::new(5.0, 60.0)),
        Some(vec![]),
        "what it holds reaches here and is cut: what is here is the page"
    );
    let items = laid.inspect(&page);
    assert!(at(&items, &[2]).hidden);
    assert!(
        at(&items, &[2, 0]).hidden,
        "what it holds is not shown with it"
    );
    let cut = at(&items, &[1, 0]);
    assert_eq!(cut.rect.h, 50.0);
    assert_eq!(cut.clip.h, 20.0, "what shows of it is within this");
}

#[test]
fn the_element_at_a_point_is_the_one_that_has_the_name_ids_at_says_first() {
    let page = layered();
    let laid = lay(&page, 1.0);
    let items = laid.inspect(&page);
    let mut named = 0;
    for x in (0..200).step_by(7) {
        for y in (0..100).step_by(5) {
            let point = Point::new(x as f32, y as f32);
            let Some(path) = laid.inspect_at(point) else {
                assert!(laid.ids_at(point).is_empty(), "{point:?}");
                continue;
            };
            let Some(id) = at(&items, &path).id else {
                continue;
            };
            named += 1;
            assert_eq!(laid.ids_at(point).first(), Some(&id), "{point:?}");
        }
    }
    assert!(named > 10, "the guard looked at {named} points with a name");
}

#[test]
fn a_box_that_scrolls_has_a_content_and_one_that_does_not_has_none() {
    let page = div().col().children([
        div()
            .id(BASE)
            .h(40.0)
            .scroll()
            .child(div().h(100.0).no_shrink()),
        div().h(10.0),
    ]);
    let items = lay(&page, 1.0).inspect(&page);
    assert!(at(&items, &[0]).content.is_some());
    assert_eq!(at(&items, &[0]).scroll, (0.0, 0.0));
    assert!(at(&items, &[1]).content.is_none());
}

const LIST: Id = Id::new(10);

fn row(n: u64) -> Id {
    Id::new(100 + n)
}

fn rows(ids: &[u64]) -> Element {
    div().id(LIST).col().h_full().children(ids.iter().map(|&n| {
        div()
            .id(row(n))
            .h(20.0)
            .no_shrink()
            .bg(Role::Accent)
            .motion(Motion::new(0.2).exit(Shift::fade(0.1)))
    }))
}

#[test]
fn what_is_going_out_has_no_place_and_the_places_of_the_rest_do_not_move() {
    let mut motions = Motions::default();
    motions.layout(rows(&[1, 2, 3]), PAGE, 1.0, &mut Fixed, 0.0, false);
    let page = rows(&[2, 3]);
    let laid = motions.layout(page.clone(), PAGE, 1.0, &mut Fixed, 1.0, false);
    let items = laid.inspect(&page);
    let going: Vec<_> = items.iter().filter(|item| item.exiting).collect();
    assert_eq!(going.len(), 1, "the row that went");
    let ghost = going[0];
    assert!(ghost.path.is_none() && ghost.placing.is_none() && ghost.id.is_none());
    assert_eq!(ghost.rect.y, 0.0, "where it was");
    assert_eq!(
        items
            .get(ghost.parent.expect("held"))
            .and_then(|held| held.id),
        Some(LIST),
        "the box that held it"
    );
    assert_eq!(items.last(), Some(ghost), "what is going is the last");
    assert_eq!(
        laid.inspect_at(Point::new(5.0, 5.0)),
        Some(vec![0]),
        "it cannot be found at a point"
    );

    // Whether it is on the way or gone, the rest are where the page wrote them.
    let kept: Vec<_> = items
        .iter()
        .filter(|item| !item.exiting)
        .map(|item| (item.id, item.path.clone()))
        .collect();
    let gone = Motions::default().layout(page.clone(), PAGE, 1.0, &mut Fixed, 5.0, false);
    let bare: Vec<_> = gone
        .inspect(&page)
        .iter()
        .map(|item| (item.id, item.path.clone()))
        .collect();
    assert_eq!(kept, bare);
}

#[test]
fn what_a_screen_reader_says_for_an_element_is_found_by_the_element() {
    let page = div().col().children([
        div().h(10.0),
        div().press(BASE).label("Save").h(20.0).child(text("Save")),
    ]);
    let laid = lay(&page, 1.0);
    let items = laid.inspect(&page);
    let said = laid.accessible();
    let button = at(&items, &[1]);
    assert!(!button.accessible.is_empty());
    for &item in &button.accessible {
        assert_eq!(said.get(item).and_then(|said| said.id), Some(BASE));
    }
    assert!(at(&items, &[0]).accessible.is_empty());
}
