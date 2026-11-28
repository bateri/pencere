//! The engine's guards for `items_baseline`: the children of a row stand on the baseline of their
//! first line of words, which the layout is told by the same place the words are drawn from.

use crate::{
    DrawList, FieldKind, Fit, Font, Id, Measure, Metrics, Paint, Rect, Role, Size, div, image,
    layout, rich, span, text,
};

/// A character is `0.5 × size` wide, a line `1.25 × size` tall and its baseline `1 × size` down
/// the line: round numbers (the line has a quarter of its size below the baseline).
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

const FIELD: Id = Id::new(1);

fn rects(root: crate::Element) -> Vec<Rect> {
    layout(root, Size::new(400.0, 200.0), 1.0, &mut Fixed)
        .rects()
        .collect()
}

fn drawn(root: crate::Element) -> DrawList {
    layout(root, Size::new(400.0, 200.0), 1.0, &mut Fixed).paint(&mut Fixed)
}

/// How far apart two baselines may be and be one: a box is laid out in whole pixels, so a line of
/// words that is not as tall as one stands at most half a pixel off where the layout put it.
const HALF: f32 = 0.5;

fn same(baselines: &[f32]) -> bool {
    baselines
        .windows(2)
        .all(|pair| (pair[0] - pair[1]).abs() <= HALF)
}

/// Where each run of words is drawn on, from the top of the page: its top and the ascent of its
/// font (what the host draws a glyph below the top).
fn baselines(list: &DrawList) -> Vec<f32> {
    list.items
        .iter()
        .filter_map(|item| match item {
            Paint::Text(run) => Some(run.origin.y + run.font.size),
            _ => None,
        })
        .collect()
}

#[test]
fn words_of_two_sizes_stand_on_one_baseline_in_a_row_that_says_so() {
    let row = |container: crate::Element| {
        container
            .row()
            .child(text("Title").text_size(15.0))
            .child(text("Button").text_size(11.0))
    };
    let on_baseline = baselines(&drawn(row(div()).items_baseline()));
    assert_eq!(on_baseline.len(), 2);
    assert!(same(&on_baseline), "{on_baseline:?}");
    // Centred, the lines are, and their baselines are not.
    let centred = baselines(&drawn(row(div()).items_center()));
    assert!(!same(&centred), "{centred:?}");
    // The shorter line is put down by the difference of what is above their baselines, and the row
    // is as tall as the taller line.
    let boxes = rects(row(div()).items_baseline());
    assert_eq!(boxes[1].y, 0.0);
    assert_eq!(boxes[1].h, 19.0);
    assert_eq!(boxes[2].y, 4.0);
}

#[test]
fn the_baseline_is_where_the_words_are_drawn_whatever_their_box_is_around_them() {
    // A line in a box taller than it is, a box with padding and border, and a text that wraps in a
    // box that cuts it: the baseline the layout aligned is the one the words are drawn on.
    let root = div()
        .row()
        .items_baseline()
        .child(text("tall").text_size(15.0).h(40.0))
        .child(
            text("padded")
                .text_size(11.0)
                .pt(3.0)
                .border(2.0, Role::Border),
        )
        .child(
            text("one two three four five six")
                .text_size(10.0)
                .wrap()
                .w(40.0)
                .h(20.0),
        );
    let list = drawn(root);
    // The first line of each text: the first run of the first two, and of the third (which wraps in
    // a box that is shorter than its lines, so that its first line is the one seen) after them.
    let drawn_on = baselines(&list);
    assert_eq!(drawn_on.len(), 6, "{drawn_on:?}");
    let firsts = [drawn_on[0], drawn_on[1], drawn_on[2]];
    assert!(same(&firsts), "baselines {firsts:?}");
}

#[test]
fn a_text_of_many_looks_stands_on_the_baseline_of_its_first_piece() {
    let root = div()
        .row()
        .items_baseline()
        .child(rich([span("Big "), span("bold").weight(700)]).text_size(16.0))
        .child(text("beside").text_size(10.0));
    let drawn_on = baselines(&drawn(root));
    // The two pieces of the rich text and the text beside it, all on one baseline.
    assert_eq!(drawn_on.len(), 3);
    assert!(same(&drawn_on), "{drawn_on:?}");
}

#[test]
fn a_picture_has_its_bottom_edge_on_the_baseline() {
    let root = div()
        .row()
        .items_baseline()
        .child(image("logo.png", Fit::Contain).size(30.0, 20.0))
        .child(text("beside").text_size(10.0));
    let boxes = rects(root.clone());
    let list = drawn(root);
    // Nothing in the picture is words: its bottom edge is its baseline, as in a style sheet.
    assert!((boxes[1].bottom() - baselines(&list)[0]).abs() <= HALF);
}

#[test]
fn a_text_field_tells_a_baseline_for_its_first_line() {
    let field = || {
        div()
            .field(FIELD, FieldKind::Line)
            .w(100.0)
            .h(30.0)
            .p(4.0)
            .border(1.0, Role::Border)
    };
    let root = div()
        .row()
        .items_baseline()
        .child(text("Name").text_size(11.0))
        .child(field());
    let boxes = rects(root.clone());
    let list = drawn(root);
    // The kit measures the field in its own face (12.5 points, a line of 15.625 and an ascent as tall
    // as the size here); the host draws a line of one in the middle of the text box, which is the
    // field less its border (1) and its padding (4) on each side.
    let field_baseline = boxes[2].y + 1.0 + 4.0 + (20.0 - 15.625) / 2.0 + 12.5;
    assert!(
        (field_baseline - baselines(&list)[0]).abs() <= HALF,
        "{field_baseline} and {:?}",
        baselines(&list)
    );
}

#[test]
fn a_text_field_stands_on_the_baseline_of_the_text_of_its_box_and_not_of_the_kits_default() {
    let field = || {
        div()
            .field(FIELD, FieldKind::Line)
            .w(100.0)
            .h(30.0)
            .p(4.0)
            .border(1.0, Role::Border)
    };
    // The box says 20 points: the field is set in it, a line of 25 and an ascent of 20 here.
    let root = div()
        .row()
        .items_baseline()
        .text_size(20.0)
        .child(text("Name").text_size(11.0))
        .child(field());
    let boxes = rects(root.clone());
    let list = drawn(root);
    let field_baseline = boxes[2].y + 1.0 + 4.0 + (20.0 - 25.0) / 2.0 + 20.0;
    assert!(
        (field_baseline - baselines(&list)[0]).abs() <= HALF,
        "{field_baseline} and {:?}",
        baselines(&list)
    );
}

/// A host that says where the words of its field stand in the line of it: a line of 16 whatever
/// the font, its baseline 15 down it, which is not where the kit would put it.
struct Says;

impl Measure for Says {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        Fixed.measure(font, text)
    }

    fn field_line(&mut self, _font: &Font) -> Option<f32> {
        Some(16.0)
    }

    fn field_baseline(&mut self, _font: &Font) -> Option<f32> {
        Some(15.0)
    }
}

#[test]
fn a_label_and_a_field_stand_on_one_baseline_where_the_host_says_it_is() {
    let field = |kind: FieldKind| {
        div()
            .field(FIELD, kind)
            .w(100.0)
            .h(if kind == FieldKind::Lines { 60.0 } else { 30.0 })
            .p(4.0)
            .border(1.0, Role::Border)
    };
    for kind in [FieldKind::Line, FieldKind::Lines] {
        let root = div()
            .row()
            .items_baseline()
            .child(text("Name").text_size(11.0))
            .child(field(kind));
        let laid = layout(root, Size::new(400.0, 200.0), 1.0, &mut Says);
        let boxes: Vec<Rect> = laid.rects().collect();
        let list = laid.paint(&mut Says);
        // The words of a field of one line stand in the middle of its text box (a line of 16 in the
        // 20 left of the field by its border and padding) and a field of lines at the top of it: the
        // baseline is the host's 15 below the top of the line.
        let middle = if kind == FieldKind::Line {
            (20.0 - 16.0) / 2.0
        } else {
            0.0
        };
        let field_baseline = boxes[2].y + 1.0 + 4.0 + middle + 15.0;
        let label = baselines(&list)[0];
        assert!(
            (field_baseline - label).abs() <= HALF,
            "{kind:?}: the field's {field_baseline}, the label's {label}"
        );
    }
}

#[test]
fn a_field_of_lines_stands_its_first_line_at_the_top_of_its_text_box() {
    let root = div()
        .row()
        .items_baseline()
        .child(text("Notes").text_size(11.0))
        .child(
            div()
                .field(FIELD, FieldKind::Lines)
                .w(100.0)
                .h(60.0)
                .p(4.0)
                .border(1.0, Role::Border),
        );
    let boxes = rects(root.clone());
    let list = drawn(root);
    // At the top of the text box, and not in the middle of it.
    let field_baseline = boxes[2].y + 1.0 + 4.0 + 12.5;
    assert!(
        (field_baseline - baselines(&list)[0]).abs() <= HALF,
        "{field_baseline} and {:?}",
        baselines(&list)
    );
}

#[test]
fn in_a_column_the_children_are_not_stretched_nor_aligned_by_baseline() {
    let root = div()
        .col()
        .items_baseline()
        .w(200.0)
        .child(text("one").text_size(15.0))
        .child(text("two").text_size(11.0));
    let boxes = rects(root);
    // As `start`: at the start of the cross axis, as wide as their words.
    assert_eq!((boxes[1].x, boxes[1].w), (0.0, 23.0));
    assert_eq!((boxes[2].x, boxes[2].w), (0.0, 17.0));
    assert_eq!(boxes[2].y, boxes[1].bottom());
}
