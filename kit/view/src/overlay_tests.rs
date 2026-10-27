//! The engine's guards for overlays: an element taken out of the boxes around it, placed against
//! the page or beside another element, drawn over the rest and found first by the pointer. The
//! host's measuring is a fixed-width stand-in.

use crate::{
    Align, DrawList, Font, Id, Laid, Measure, Metrics, Paint, Point, Rect, Role, Side, Size, div,
    layout, text,
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

const MENU: Id = Id::new(1);
const BUTTON: Id = Id::new(2);
const COVER: Id = Id::new(3);
const LIST: Id = Id::new(4);

fn lay(root: crate::Element, w: f32, h: f32) -> Laid {
    layout(root, Size::new(w, h), 1.0, &mut Fixed)
}

fn bounds(laid: &Laid, id: Id) -> Rect {
    laid.bounds(id).expect("the element is on the page")
}

fn fills(list: &DrawList) -> Vec<(Rect, Rect, Role)> {
    list.items
        .iter()
        .filter_map(|item| match item {
            Paint::Quad(quad) => Some((quad.rect, quad.clip, quad.fill?.role()?)),
            _ => None,
        })
        .collect()
}

/// A button 30 points down a list that clips, with a menu written inside the list (as a menu is
/// written beside the button it drops from) and a box after the list that covers the same room.
fn page(menu: crate::Element) -> crate::Element {
    div()
        .child(
            div()
                .id(LIST)
                .h(60.0)
                .clip()
                .pt(30.0)
                .pl(20.0)
                .child(div().press(BUTTON).size(40.0, 10.0).bg(Role::Accent))
                .child(menu),
        )
        .child(div().press(COVER).h(100.0).mt(-30.0).bg(Role::Raised(1)))
}

fn menu() -> crate::Element {
    div()
        .press(MENU)
        .size(50.0, 40.0)
        .bg(Role::Raised(2))
        .child(text("Open"))
}

#[test]
fn an_overlay_stands_against_the_page_and_not_the_box_it_is_written_in() {
    let laid = lay(
        page(menu().overlay().inset(Some(5.0), None, None, Some(7.0))),
        200.0,
        200.0,
    );
    assert_eq!(bounds(&laid, MENU), Rect::new(7.0, 5.0, 50.0, 40.0));
    // It takes no room where it is written: the button is where it was.
    assert_eq!(bounds(&laid, BUTTON), Rect::new(20.0, 30.0, 40.0, 10.0));
}

#[test]
fn an_overlay_is_drawn_over_everything_and_cut_by_nothing_that_holds_it() {
    let laid = lay(
        page(menu().anchor(BUTTON, Side::Below).anchor_gap(2.0)),
        200.0,
        200.0,
    );
    let list = laid.paint(&mut Fixed);
    let drawn = fills(&list);
    let (rect, clip, role) = *drawn.last().expect("something is drawn");
    // Last of the boxes, though written before the box that covers it.
    assert_eq!(role, Role::Raised(2));
    // Below the button, two points away, past the bottom of the list that clips it.
    assert_eq!(rect, Rect::new(20.0, 42.0, 50.0, 40.0));
    assert_eq!(clip, Rect::new(0.0, 0.0, 200.0, 200.0));
    // And its words with it, after it.
    assert!(matches!(list.items.last(), Some(Paint::Text(run)) if run.text == "Open"));
}

#[test]
fn the_pointer_finds_an_overlay_before_what_is_under_it() {
    let laid = lay(page(menu().anchor(BUTTON, Side::Below)), 200.0, 200.0);
    assert_eq!(laid.hit(Point::new(25.0, 70.0)), Some(MENU));
    assert_eq!(laid.ids_at(Point::new(25.0, 70.0)).first(), Some(&MENU));
    // Beside it, the box that covers the list.
    assert_eq!(laid.hit(Point::new(150.0, 70.0)), Some(COVER));
}

#[test]
fn an_overlay_with_no_room_below_goes_above_and_stays_on_the_page_across() {
    let root = div()
        .child(div().h(150.0))
        .child(
            div()
                .row()
                .child(div().w(170.0))
                .child(div().press(BUTTON).size(20.0, 10.0)),
        )
        .child(
            div()
                .press(MENU)
                .size(50.0, 40.0)
                .anchor(BUTTON, Side::Below),
        );
    let laid = lay(root, 200.0, 180.0);
    // Below the button (160) would end at 200, past the page (180): above it, and moved left so
    // that it ends at the page's right edge.
    assert_eq!(bounds(&laid, MENU), Rect::new(150.0, 110.0, 50.0, 40.0));
}

#[test]
fn an_overlay_lines_up_with_the_middle_or_the_end_of_its_element() {
    let at = |align| {
        let root = div()
            .p(50.0)
            .child(div().press(BUTTON).size(40.0, 10.0))
            .child(
                div()
                    .press(MENU)
                    .size(20.0, 10.0)
                    .anchor(BUTTON, Side::Right)
                    .anchor_align(align)
                    .anchor_gap(4.0),
            );
        bounds(&lay(root, 300.0, 300.0), MENU)
    };
    assert_eq!(at(Align::Start), Rect::new(94.0, 50.0, 20.0, 10.0));
    let root = div()
        .p(50.0)
        .child(div().press(BUTTON).size(40.0, 30.0))
        .child(
            div()
                .press(MENU)
                .size(60.0, 10.0)
                .anchor(BUTTON, Side::Below)
                .anchor_align(Align::Center),
        );
    assert_eq!(
        bounds(&lay(root, 300.0, 300.0), MENU),
        Rect::new(40.0, 80.0, 60.0, 10.0)
    );
    let root = div()
        .p(50.0)
        .child(div().press(BUTTON).size(40.0, 30.0))
        .child(
            div()
                .press(MENU)
                .size(60.0, 10.0)
                .anchor(BUTTON, Side::Below)
                .anchor_align(Align::End),
        );
    assert_eq!(
        bounds(&lay(root, 300.0, 300.0), MENU),
        Rect::new(30.0, 80.0, 60.0, 10.0)
    );
}

#[test]
fn an_overlay_is_hidden_and_faded_with_what_holds_it_and_does_not_make_it_scroll() {
    let root = div().child(
        div()
            .id(LIST)
            .h(50.0)
            .scroll()
            .opacity(0.5)
            .child(div().h(20.0))
            .child(menu().overlay().inset(Some(0.0), None, None, Some(0.0))),
    );
    let laid = lay(root.clone(), 200.0, 200.0);
    let extent = laid.scroll_extent(LIST).expect("the list scrolls");
    assert_eq!(extent.content.h, 20.0);
    let list = laid.paint(&mut Fixed);
    let menu_fill = list
        .items
        .iter()
        .find_map(|item| match item {
            Paint::Quad(quad) if quad.rect.w == 50.0 => quad.fill,
            _ => None,
        })
        .expect("the menu is drawn");
    assert_eq!(menu_fill.alpha, 0.5);

    let hidden = div().child(div().hidden().child(menu().overlay()));
    let laid = lay(hidden, 200.0, 200.0);
    assert_eq!(laid.hit(Point::new(5.0, 5.0)), None);
    assert!(laid.paint(&mut Fixed).items.is_empty());
}

#[test]
fn an_overlay_whose_element_is_not_on_the_page_stands_where_its_inset_puts_it() {
    let root = div().child(menu().anchor(Id::new(99), Side::Below).inset(
        Some(3.0),
        None,
        None,
        Some(4.0),
    ));
    assert_eq!(
        bounds(&lay(root, 200.0, 200.0), MENU),
        Rect::new(4.0, 3.0, 50.0, 40.0)
    );
}

#[test]
fn an_overlay_covers_the_page_and_the_keys_of_what_is_in_it_go_out_through_what_holds_it() {
    let root = div().id(LIST).child(
        div()
            .id(COVER)
            .overlay()
            .inset(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
            .child(div().press(MENU).size(10.0, 10.0)),
    );
    let laid = lay(root, 120.0, 80.0);
    assert_eq!(bounds(&laid, COVER), Rect::new(0.0, 0.0, 120.0, 80.0));
    assert_eq!(laid.path_of(MENU), vec![MENU, COVER, LIST]);
}

#[test]
fn the_wheel_over_an_overlay_does_not_reach_what_it_covers() {
    // A long list that scrolls, and over it, as a palette is, an overlay with a short list of its
    // own that has nothing to scroll.
    let rows = (0..20).map(|n| div().h(20.0).no_shrink().press(Id::new(100 + n)));
    let palette = div()
        .id(COVER)
        .overlay()
        .inset(Some(10.0), None, None, Some(10.0))
        .size(100.0, 50.0)
        .child(
            div()
                .id(MENU)
                .col()
                .h(40.0)
                .scroll()
                .child(div().h(20.0).press(BUTTON).child(text("Copy"))),
        );
    let root = div()
        .col()
        .size_full()
        .child(
            div()
                .id(LIST)
                .col()
                .grow(1.0)
                .min_h(0.0)
                .scroll()
                .children(rows),
        )
        .child(palette);
    let laid = lay(root, 200.0, 100.0);
    let over = Point::new(30.0, 20.0);
    assert_eq!(laid.ids_at(over), vec![BUTTON, MENU, COVER]);
    let mut scrolls = crate::Scrolls::new();
    let wheel = |scrolls: &mut crate::Scrolls, at| {
        scrolls
            .wheel(&laid, at, 0.0, 30.0, crate::Phase::Wheel, 0.0)
            .took
    };
    assert!(
        !wheel(&mut scrolls, over),
        "the palette's list has no room, and the list under it is not under the pointer"
    );
    assert_eq!(scrolls.offset(LIST).y, 0.0);
    // Beside the palette the list is what the pointer is over.
    let beside = Point::new(150.0, 80.0);
    assert_eq!(laid.ids_at(beside), vec![Id::new(104), LIST]);
    assert!(wheel(&mut scrolls, beside));
    assert_eq!(scrolls.offset(LIST).y, 30.0);
}
