//! The engine's guards for images: the room an image makes, how it fills it, and what the draw
//! list says of it. The host's side — the sizes of the images it has decoded — is a table.

use std::collections::HashMap;

use crate::{
    Color, DrawList, Element, Fit, Font, Measure, Metrics, Paint, Picture, Rect, Rgba, Role, Size,
    div, image, layout,
};

/// A host that knows the images in its table, in pixels of the image, and says so each time it
/// is asked (the number of times is what a test counts).
#[derive(Default)]
struct Images {
    known: HashMap<&'static str, Size>,
    asked: Vec<String>,
}

impl Images {
    fn with(src: &'static str, w: f32, h: f32) -> Self {
        let mut images = Self::default();
        images.known.insert(src, Size::new(w, h));
        images
    }
}

impl Measure for Images {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().count() as f32 * font.size * 0.5,
            height: font.size * 1.25,
        }
    }

    fn image(&mut self, src: &str) -> Option<Size> {
        self.asked.push(src.to_owned());
        self.known.get(src).copied()
    }
}

fn laid(root: Element, host: &mut Images, w: f32, h: f32, scale: f32) -> crate::Laid {
    layout(root, Size::new(w * scale, h * scale), scale, host)
}

fn pictures(list: &DrawList) -> Vec<&Picture> {
    list.items
        .iter()
        .filter_map(|item| match item {
            Paint::Image(picture) => Some(picture),
            _ => None,
        })
        .collect()
}

/// The box of the one image in the page, and what it draws.
fn lone(root: Element, host: &mut Images, scale: f32) -> (Rect, Vec<Picture>) {
    let page = laid(root, host, 200.0, 200.0, scale);
    let list = page.paint(host);
    let boxes: Vec<Rect> = page.rects().collect();
    (
        boxes.get(1).copied().expect("the image's box"),
        pictures(&list).into_iter().cloned().collect(),
    )
}

#[test]
fn an_image_that_is_given_no_side_has_its_own_size_a_point_to_a_pixel() {
    let mut host = Images::with("a.png", 40.0, 20.0);
    let root = div().items_start().child(image("a.png", Fit::Contain));
    let (at, drawn) = lone(root.clone(), &mut host, 1.0);
    assert_eq!(at, Rect::new(0.0, 0.0, 40.0, 20.0));
    assert_eq!(drawn.len(), 1);
    // At two pixels to the point it is twice as large: the page is in points, the image is not
    // (an image sent at twice its size in points is sharp).
    let (at, _) = lone(root, &mut host, 2.0);
    assert_eq!(at, Rect::new(0.0, 0.0, 80.0, 40.0));
}

#[test]
fn an_image_given_one_side_takes_the_other_from_its_proportions() {
    let mut host = Images::with("a.png", 40.0, 20.0);
    let by_width = div()
        .items_start()
        .child(image("a.png", Fit::Contain).w(30.0));
    let (at, drawn) = lone(by_width, &mut host, 1.0);
    assert_eq!(at, Rect::new(0.0, 0.0, 30.0, 15.0));
    assert_eq!(drawn[0].rect, at);
    let by_height = div()
        .items_start()
        .child(image("a.png", Fit::Contain).h(10.0));
    let (at, _) = lone(by_height, &mut host, 1.0);
    assert_eq!(at, Rect::new(0.0, 0.0, 20.0, 10.0));
    // Given both, it takes both and the proportions are the fit's business.
    let both = div().child(image("a.png", Fit::Contain).size(50.0, 50.0));
    let (at, _) = lone(both, &mut host, 1.0);
    assert_eq!((at.w, at.h), (50.0, 50.0));
}

#[test]
fn an_image_stretched_across_a_column_keeps_its_proportions_in_its_height() {
    // A column stretches its children across: the width is the column's, and the height is what
    // the proportions make of it, as for any picture that is left to be as wide as it may.
    let mut host = Images::with("a.png", 40.0, 20.0);
    let (at, _) = lone(div().child(image("a.png", Fit::Contain)), &mut host, 1.0);
    assert_eq!((at.w, at.h), (200.0, 100.0));
}

#[test]
fn contain_draws_the_whole_image_as_large_as_the_box_lets_it_be_and_centres_it() {
    let mut host = Images::with("wide.png", 40.0, 20.0);
    let root = div().child(image("wide.png", Fit::Contain).size(100.0, 100.0));
    let (at, drawn) = lone(root, &mut host, 1.0);
    assert_eq!(at, Rect::new(0.0, 0.0, 100.0, 100.0));
    let [picture] = drawn.as_slice() else {
        panic!("one picture, got {drawn:?}");
    };
    assert_eq!(picture.src, "wide.png");
    assert_eq!(picture.rect, Rect::new(0.0, 25.0, 100.0, 50.0));
    assert_eq!(picture.crop, Rect::new(0.0, 0.0, 1.0, 1.0));
    assert_eq!(picture.full(), Size::new(100.0, 50.0));

    // A tall one is held by the height.
    let mut host = Images::with("tall.png", 20.0, 40.0);
    let root = div().child(image("tall.png", Fit::Contain).size(100.0, 60.0));
    let (_, drawn) = lone(root, &mut host, 1.0);
    assert_eq!(drawn[0].rect, Rect::new(35.0, 0.0, 30.0, 60.0));
}

#[test]
fn cover_fills_the_box_and_cuts_the_middle_of_what_does_not_fit() {
    let mut host = Images::with("wide.png", 40.0, 20.0);
    let root = div().child(image("wide.png", Fit::Cover).size(100.0, 100.0));
    let (_, drawn) = lone(root, &mut host, 1.0);
    // The image is five times as large to cover the height, 200 wide: a half of it shows.
    assert_eq!(drawn[0].rect, Rect::new(0.0, 0.0, 100.0, 100.0));
    assert_eq!(drawn[0].crop, Rect::new(0.25, 0.0, 0.5, 1.0));
    assert_eq!(drawn[0].full(), Size::new(200.0, 100.0));

    let mut host = Images::with("tall.png", 20.0, 40.0);
    let root = div().child(image("tall.png", Fit::Cover).size(100.0, 50.0));
    let (_, drawn) = lone(root, &mut host, 1.0);
    assert_eq!(drawn[0].rect, Rect::new(0.0, 0.0, 100.0, 50.0));
    assert_eq!(drawn[0].crop, Rect::new(0.0, 0.375, 1.0, 0.25));
    assert_eq!(drawn[0].full(), Size::new(100.0, 200.0));

    // A box with the image's own proportions shows all of it either way.
    let mut host = Images::with("same.png", 40.0, 20.0);
    let root = div().child(image("same.png", Fit::Cover).size(80.0, 40.0));
    let (_, drawn) = lone(root, &mut host, 1.0);
    assert_eq!(drawn[0].crop, Rect::new(0.0, 0.0, 1.0, 1.0));
}

#[test]
fn what_is_drawn_is_in_pixels_of_the_page_at_any_scale() {
    let mut host = Images::with("wide.png", 40.0, 20.0);
    let root = div().child(image("wide.png", Fit::Contain).size(100.0, 100.0));
    let (at, drawn) = lone(root, &mut host, 2.0);
    assert_eq!(at, Rect::new(0.0, 0.0, 200.0, 200.0));
    assert_eq!(drawn[0].rect, Rect::new(0.0, 50.0, 200.0, 100.0));
    assert_eq!(drawn[0].full(), Size::new(200.0, 100.0));
}

#[test]
fn an_image_the_host_does_not_know_leaves_its_box_empty_and_the_box_keeps_its_fill() {
    let mut host = Images::default();
    let root = div().child(
        image("missing.png", Fit::Contain)
            .size(30.0, 30.0)
            .bg(Role::Raised(1)),
    );
    let (at, drawn) = lone(root, &mut host, 1.0);
    assert_eq!(at, Rect::new(0.0, 0.0, 30.0, 30.0));
    assert!(drawn.is_empty());
    // Its fill is drawn all the same.
    let page = laid(
        div().child(
            image("missing.png", Fit::Contain)
                .size(30.0, 30.0)
                .bg(Role::Raised(1)),
        ),
        &mut host,
        100.0,
        100.0,
        1.0,
    );
    let list = page.paint(&mut host);
    assert_eq!(list.items.len(), 1);
    assert!(matches!(list.items[0], Paint::Quad(_)));

    // And with no size said it takes no room at all, so nothing is laid out around a hole.
    let (at, drawn) = lone(
        div().items_start().child(image("missing.png", Fit::Cover)),
        &mut host,
        1.0,
    );
    assert_eq!((at.w, at.h), (0.0, 0.0));
    assert!(drawn.is_empty());
}

#[test]
fn a_size_that_is_not_one_is_a_size_not_known() {
    for bad in [
        Size::new(0.0, 10.0),
        Size::new(10.0, -1.0),
        Size::new(f32::NAN, 10.0),
        Size::new(10.0, f32::INFINITY),
        Size::new(1.0e9, 10.0),
    ] {
        let mut host = Images::default();
        host.known.insert("bad.png", bad);
        let (at, drawn) = lone(
            div().items_start().child(image("bad.png", Fit::Contain)),
            &mut host,
            1.0,
        );
        assert_eq!((at.w, at.h), (0.0, 0.0), "{bad:?}");
        assert!(drawn.is_empty(), "{bad:?}");
    }
}

#[test]
fn a_page_laid_out_again_when_the_image_is_known_has_the_room_and_the_picture() {
    // What a host does: lays the page out as it is, the image is being decoded; the decoding
    // ends; it lays the same page out again.
    let root = div()
        .row()
        .items_start()
        .gap(4.0)
        .child(image("a.png", Fit::Contain))
        .child(div().size(10.0, 10.0));
    let mut host = Images::default();
    let before = laid(root.clone(), &mut host, 200.0, 100.0, 1.0);
    let boxes: Vec<Rect> = before.rects().collect();
    assert_eq!(boxes[1], Rect::new(0.0, 0.0, 0.0, 0.0));
    assert_eq!(boxes[2], Rect::new(4.0, 0.0, 10.0, 10.0));
    assert!(pictures(&before.paint(&mut host)).is_empty());

    host.known.insert("a.png", Size::new(30.0, 20.0));
    let after = laid(root, &mut host, 200.0, 100.0, 1.0);
    let boxes: Vec<Rect> = after.rects().collect();
    assert_eq!(boxes[1], Rect::new(0.0, 0.0, 30.0, 20.0));
    assert_eq!(boxes[2], Rect::new(34.0, 0.0, 10.0, 10.0));
    assert_eq!(pictures(&after.paint(&mut host)).len(), 1);
    // Asked once for each layout, and not by the painting.
    assert_eq!(host.asked, ["a.png", "a.png"]);
}

#[test]
fn an_image_fills_what_is_inside_its_padding_and_border() {
    let mut host = Images::with("a.png", 40.0, 20.0);
    let root = div().child(
        image("a.png", Fit::Cover)
            .size(60.0, 40.0)
            .p(5.0)
            .border(1.0, Role::Line),
    );
    let (_, drawn) = lone(root, &mut host, 1.0);
    assert_eq!(drawn[0].rect, Rect::new(6.0, 6.0, 48.0, 28.0));
}

#[test]
fn the_tint_and_the_opacity_reach_the_draw_list_as_the_page_said_them() {
    let mut host = Images::with("a.svg", 16.0, 16.0);
    let ink = Rgba::rgb(1, 2, 3).or_dark(Rgba::rgb(250, 251, 252));
    let root = div().items_start().child(
        image("a.svg", Fit::Contain)
            .tint(ink)
            .opacity(0.5)
            .rounded(40.0),
    );
    let (_, drawn) = lone(root, &mut host, 1.0);
    assert_eq!(drawn[0].tint, Some(ink));
    assert_eq!(drawn[0].opacity, 0.5);
    // The corners are taken in to what the drawn room can have, as a box's are.
    assert_eq!(drawn[0].radii, [8.0; 4]);

    let plain = div().items_start().child(image("a.svg", Fit::Contain));
    let (_, drawn) = lone(plain, &mut host, 1.0);
    assert_eq!(drawn[0].tint, None);
    assert_eq!(drawn[0].opacity, 1.0);

    // An opacity that is not one is a whole image.
    let odd = div()
        .items_start()
        .child(image("a.svg", Fit::Contain).opacity(f32::NAN));
    let (_, drawn) = lone(odd, &mut host, 1.0);
    assert_eq!(drawn[0].opacity, 1.0);
    // And one of a role is held, not resolved.
    let toned = div()
        .items_start()
        .child(image("a.svg", Fit::Contain).tint(Role::Accent));
    let (_, drawn) = lone(toned, &mut host, 1.0);
    assert_eq!(
        drawn[0].tint.map(|c| c.source),
        Some(Color::from(Role::Accent).source)
    );
}

#[test]
fn a_hidden_image_asks_for_nothing_to_be_drawn() {
    let mut host = Images::with("a.png", 40.0, 20.0);
    let root = div().child(image("a.png", Fit::Contain).hidden());
    let page = laid(root, &mut host, 100.0, 100.0, 1.0);
    assert!(pictures(&page.paint(&mut host)).is_empty());
}
