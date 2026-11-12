//! The engine's guards for what a person can drag out of the view ([`crate::Element::carry`]): the
//! element a drag picks up at a point, and what lies over it that answers the pointer instead.

use crate::{
    Carry, Element, Font, Id, Laid, Measure, Metrics, Point, Rect, Size, div, layout, text,
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

const ROW: Id = Id::new(1);
const BUTTON: Id = Id::new(2);
const FILE: Id = Id::new(3);

fn lay(root: Element) -> Laid {
    layout(root, Size::new(200.0, 100.0), 1.0, &mut Fixed)
}

/// A row of 200 × 20 that carries a file, with a word in it and a button at its end, and below it
/// a row that carries nothing.
fn page() -> Element {
    div()
        .col()
        .size(200.0, 100.0)
        .child(
            div()
                .id(ROW)
                .row()
                .h(20.0)
                .no_shrink()
                .carry(Carry::path("/tmp/a b.txt").url("https://example.com/a"))
                .child(div().grow(1.0).child(text("a b.txt")))
                .child(div().w(30.0).press(BUTTON)),
        )
        .child(div().h(20.0).no_shrink().child(text("nothing")))
}

#[test]
fn a_drag_picks_up_what_the_element_under_it_carries_with_its_box_on_the_page() {
    let laid = lay(page());
    let found = laid.carry_at(Point::new(10.0, 10.0)).expect("a carry");
    assert_eq!(found.id, ROW);
    assert_eq!(found.carry.words(), "/tmp/a b.txt");
    assert_eq!(found.carry.url.as_deref(), Some("https://example.com/a"));
    assert_eq!(found.rect, Rect::new(0.0, 0.0, 200.0, 20.0));
    assert!(!found.draggable);
    // The row below carries nothing.
    assert_eq!(laid.carry_at(Point::new(10.0, 30.0)), None);
    // Nor does the page past its elements.
    assert_eq!(laid.carry_at(Point::new(10.0, 99.0)), None);
}

#[test]
fn what_answers_the_pointer_over_a_carrying_element_is_not_a_drag_of_it() {
    let laid = lay(page());
    // The button at the end of the row is the button's.
    assert_eq!(laid.carry_at(Point::new(190.0, 10.0)), None);
    assert_eq!(laid.hit(Point::new(190.0, 10.0)), Some(BUTTON));
}

#[test]
fn a_text_field_or_an_editor_in_a_carrying_element_keeps_the_pointer_for_its_selection() {
    let field = div().col().size(200.0, 100.0).child(
        div()
            .id(ROW)
            .row()
            .h(20.0)
            .no_shrink()
            .carry(Carry::text("a line"))
            .child(div().id(Id::new(7)).w(100.0).text_field(Id::new(7))),
    );
    let laid = lay(field);
    assert_eq!(
        laid.carry_at(Point::new(150.0, 10.0)).map(|c| c.id),
        Some(ROW)
    );
    assert_eq!(laid.carry_at(Point::new(50.0, 10.0)), None);
    // An editor that carries itself is the editor's.
    let editor = div().col().size(200.0, 100.0).child(
        div()
            .id(Id::new(8))
            .h(20.0)
            .no_shrink()
            .editor(Id::new(8), crate::edit::EditorOptions::default())
            .carry(Carry::text("a line")),
    );
    assert_eq!(lay(editor).carry_at(Point::new(50.0, 10.0)), None);
}

#[test]
fn an_element_with_no_name_or_one_that_is_off_does_not_carry() {
    // No name: nothing to say the drag is over to.
    let nameless = div()
        .size(200.0, 100.0)
        .child(div().h(20.0).carry(Carry::text("x")));
    assert_eq!(lay(nameless).carry_at(Point::new(10.0, 10.0)), None);
    // Hidden, and disabled.
    for off in [
        div().id(FILE).h(20.0).carry(Carry::text("x")).hidden(),
        div()
            .id(FILE)
            .h(20.0)
            .carry(Carry::text("x"))
            .disabled(true),
    ] {
        let laid = lay(div().size(200.0, 100.0).child(off));
        assert_eq!(laid.carry_at(Point::new(10.0, 10.0)), None);
    }
}

#[test]
fn an_element_that_is_also_the_pages_to_drag_says_so() {
    let laid = lay(div()
        .size(200.0, 100.0)
        .child(div().h(20.0).draggable(FILE).carry(Carry::text("tab"))));
    let found = laid.carry_at(Point::new(10.0, 10.0)).expect("a carry");
    assert!(found.draggable);
    assert_eq!(found.id, FILE);
}

#[test]
fn the_point_is_the_pages_and_a_scale_does_not_move_it() {
    let laid = layout(page(), Size::new(400.0, 200.0), 2.0, &mut Fixed);
    assert_eq!(
        laid.carry_at(Point::new(10.0, 10.0)).map(|c| c.id),
        Some(ROW)
    );
    assert_eq!(
        laid.carry_at(Point::new(10.0, 10.0)).map(|c| c.rect),
        Some(Rect::new(0.0, 0.0, 200.0, 20.0))
    );
}

#[test]
fn a_page_that_pressed_what_was_carried_away_lets_it_go_when_the_carry_is_over() {
    use crate::Ui;
    let laid = lay(div()
        .size(200.0, 100.0)
        .child(div().h(20.0).draggable(FILE).carry(Carry::text("tab"))));
    let mut ui = Ui::default();
    let at = Point::new(10.0, 10.0);
    ui.press(&laid, at);
    ui.point(&laid, Some(Point::new(40.0, 10.0)));
    assert!(ui.dragging().is_some());
    // The pointer is the system's now, and the page never hears the release.
    assert!(ui.carried());
    assert!(ui.dragging().is_none());
    assert_eq!(ui.pressed(), None);
    assert!(ui.take_drop().is_none());
    assert!(!ui.carried(), "nothing is left to let go");
}
