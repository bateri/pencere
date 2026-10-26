//! The engine's guards for margins that are less than nothing: an element that gives back room on
//! its sides (a line that is wider to the pointer than it is drawn) takes from its row only the room
//! it is left with, as a style sheet's flex box does, and the elements beside it shrink by no more
//! and no less than the row needs.

use crate::{Font, Id, Measure, Metrics, Size, div, layout};

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
const EDGE: Id = Id::new(2);
const PANEL: Id = Id::new(3);

/// A list wider than the page that may shrink, and after it a panel that may not, with a line
/// between them seven points wide that gives three back on each side.
fn split(page: f32) -> (f32, f32, f32) {
    let root = div()
        .row()
        .w(page)
        .h(100.0)
        .child(
            div()
                .id(LIST)
                .col()
                .grow(1.0)
                .min_w(0.0)
                .child(div().w(1300.0).h(20.0)),
        )
        .child(
            div()
                .row()
                .no_shrink()
                .child(div().id(EDGE).w(7.0).ml(-3.0).mr(-3.0).no_shrink())
                .child(div().id(PANEL).w(352.0).no_shrink()),
        );
    let laid = layout(root, Size::new(page, 100.0), 1.0, &mut Fixed);
    let list = laid.bounds(LIST).expect("the list");
    let edge = laid.bounds(EDGE).expect("the line");
    let panel = laid.bounds(PANEL).expect("the panel");
    assert_eq!(
        edge.x,
        list.x + list.w - 3.0,
        "the line starts three points into the list"
    );
    (list.w, panel.x, panel.x + panel.w)
}

#[test]
fn a_line_that_gives_back_its_sides_takes_one_point_of_the_row() {
    for page in [1100.0, 1400.0, 1800.0] {
        let (list, panel_x, panel_end) = split(page);
        assert_eq!(list, page - 1.0 - 352.0, "the list at {page}");
        assert_eq!(panel_x, page - 352.0, "the panel at {page}");
        assert_eq!(panel_end, page, "the panel ends on the page at {page}");
    }
}
