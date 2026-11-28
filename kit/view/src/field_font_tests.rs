//! The engine's guards for the font of a text field: the host draws the words of a field, and it
//! sets them in the text of the box the field stands in, as a style sheet's `font: inherit` does.

use crate::{
    Element, Family, FieldKind, Font, Id, Measure, Metrics, Role, Size, ZOOM_TEXT, Zoom, div,
    layout, text,
};

/// A host whose line of a text field is not a share of its size: 16 at 12.5 points, 28.5 at 25 (as
/// the system's fields have lines that do not grow with the size by a rule), and 2 taller in the
/// system's face. It remembers which fonts the layout asked it the line of.
#[derive(Default)]
struct Host {
    asked: Vec<Font>,
}

impl Measure for Host {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().count() as f32 * font.size * 0.5,
            height: font.size * 1.25,
            ascent: font.size,
        }
    }

    fn field_line(&mut self, font: &Font) -> Option<f32> {
        self.asked.push(*font);
        let face = if font.family == Family::System {
            2.0
        } else {
            0.0
        };
        Some(font.size + 3.5 + face)
    }
}

const FIELD: Id = Id::new(1);

fn field() -> Element {
    div().field(FIELD, FieldKind::Line).w(100.0).h(30.0)
}

fn lines(rows: u16) -> Element {
    div()
        .field(FIELD, FieldKind::Lines)
        .field_rows(rows)
        .w(100.0)
}

fn font_of(root: Element, scale: f32) -> Font {
    let laid = layout(root, Size::new(400.0, 400.0), scale, &mut Host::default());
    laid.fields()[0].font
}

fn height_of(root: Element, scale: f32) -> f32 {
    let laid = layout(root, Size::new(400.0, 400.0), scale, &mut Host::default());
    laid.fields()[0].rect.h
}

#[test]
fn a_field_in_a_box_that_says_nothing_of_its_text_is_set_in_the_kits_own() {
    for scale in [1.0, 2.0] {
        assert_eq!(
            font_of(div().child(field()), scale),
            Font {
                family: Family::Terminal,
                size: ZOOM_TEXT * scale,
                weight: 400,
                italic: false,
            }
        );
    }
}

#[test]
fn a_field_is_set_in_the_size_the_box_around_it_says_times_the_scale() {
    let font = font_of(div().text_size(15.0).child(div().child(field())), 2.0);
    assert_eq!(font.size, 30.0);
    // What its own element says is over what it inherits.
    let font = font_of(div().text_size(15.0).child(field().text_size(13.5)), 2.0);
    assert_eq!(font.size, 27.0);
}

#[test]
fn a_field_takes_the_face_the_weight_and_the_italic_of_the_box_it_is_in_too() {
    let font = font_of(
        div()
            .family(Family::System)
            .weight(700)
            .italic()
            .child(field()),
        1.0,
    );
    assert_eq!(
        font,
        Font {
            family: Family::System,
            size: ZOOM_TEXT,
            weight: 700,
            italic: true,
        }
    );
}

#[test]
fn a_field_with_its_hint_in_it_has_the_font_of_a_field_without() {
    let hinted = field().child(text("Search"));
    assert_eq!(
        font_of(div().text_size(15.0).child(hinted), 1.0),
        font_of(div().text_size(15.0).child(field()), 1.0)
    );
}

#[test]
fn the_layout_asks_the_host_the_line_of_the_font_it_tells_the_field_is_in() {
    let root = div().text_size(15.0).family(Family::System).child(lines(3));
    let mut host = Host::default();
    let laid = layout(root, Size::new(400.0, 400.0), 2.0, &mut host);
    assert!(!host.asked.is_empty());
    // The line the host says is for that very font, so that a host that measured the field at the
    // size it draws it in is the one the layout believes.
    assert!(host.asked.iter().all(|font| *font == laid.fields()[0].font));
}

#[test]
fn a_field_of_rows_is_as_tall_as_that_many_lines_of_its_own_font() {
    // The line is 16 at 12.5 points, 23.5 at 20 and 25.5 at 20 in the system's face: not a share of
    // the size, so that the height is the host's line of the font and not the default's line scaled.
    // The box is a whole number of pixels, upward (three lines of 23.5 are 70.5).
    assert_eq!(height_of(div().child(lines(3)), 1.0), 48.0);
    assert_eq!(height_of(div().text_size(20.0).child(lines(3)), 1.0), 71.0);
    assert_eq!(
        height_of(
            div().text_size(20.0).family(Family::System).child(lines(2)),
            1.0
        ),
        51.0
    );
    // At twice the scale the host is asked at twice the size: 43.5 a line, 131 pixels of 130.5.
    assert_eq!(height_of(div().text_size(20.0).child(lines(3)), 2.0), 65.5);
}

#[test]
fn a_field_of_rows_in_text_of_another_size_keeps_its_padding_and_border() {
    let root = div()
        .text_size(20.0)
        .child(lines(2).py(4.0).border(1.0, Role::Border));
    // Two lines of 23.5, the padding above and below and a border of 1 round.
    assert_eq!(height_of(root, 1.0), 2.0 * 23.5 + 8.0 + 2.0);
}

#[test]
fn a_field_in_a_box_that_is_zoomed_has_a_font_and_boxes_that_much_larger() {
    let mut held = div()
        .id(Id::new(2))
        .size(200.0, 100.0)
        .scroll()
        .zoom(Zoom::default())
        .child(field());
    held.style.zoomed = Some(1.5);
    let laid = layout(
        div().child(held),
        Size::new(400.0, 400.0),
        2.0,
        &mut Host::default(),
    );
    let field = &laid.fields()[0];
    // In the page's points as it is drawn: the font and the box (30 points tall) are both 1.5 times as
    // large, so that what the host sets in the font stands in the box it is given.
    assert_eq!(field.font.size / laid.scale(), ZOOM_TEXT * 1.5);
    assert_eq!(field.rect.h, 45.0);
}

#[test]
fn a_hint_that_says_a_size_of_its_own_does_not_change_the_font_of_the_field() {
    // The host draws the hint in the field's font: the text inside is its words, and the size it says
    // is for the box and the baseline the layout makes of it.
    let hinted = field().child(text("Search").text_size(20.0));
    assert_eq!(
        font_of(div().child(hinted), 1.0),
        font_of(div().child(field()), 1.0)
    );
}
