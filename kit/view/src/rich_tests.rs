//! The engine's guards for text of many looks: how its parts run from line to line, how each is
//! drawn, and what a link in it answers. The host's measuring is a fixed-width stand-in: every
//! character is half the size wide, a bold one too.

use crate::{
    AccessibleRole, DrawList, Family, Font, Id, Laid, Measure, Metrics, Paint, Point, Rect, Role,
    Run, Size, div, layout, rich, span,
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

const LINK: Id = Id::new(7);

fn lay(root: crate::Element, w: f32, h: f32) -> Laid {
    layout(root, Size::new(w, h), 1.0, &mut Fixed)
}

fn runs(list: &DrawList) -> Vec<&Run> {
    list.items
        .iter()
        .filter_map(|item| match item {
            Paint::Text(run) => Some(run),
            _ => None,
        })
        .collect()
}

/// Text of size 10: a character is 5 pixels wide and a line 12.5 tall.
fn paragraph() -> crate::Element {
    rich([
        span("one "),
        span("two").weight(700).color(Role::Accent),
        span(", three four "),
        span("five").link(LINK),
    ])
    .text_size(10.0)
}

#[test]
fn the_parts_of_a_text_run_on_from_line_to_line_each_in_its_own_look() {
    // 80 pixels: sixteen characters to a line.
    let laid = lay(
        div().child(div().w(80.0).child(paragraph().wrap())),
        200.0,
        200.0,
    );
    let list = laid.paint(&mut Fixed);
    let drawn: Vec<(&str, f32, f32, u16)> = runs(&list)
        .iter()
        .map(|run| {
            (
                run.text.as_str(),
                run.origin.x,
                run.origin.y,
                run.font.weight,
            )
        })
        .collect();
    assert_eq!(
        drawn,
        vec![
            ("one ", 0.0, 0.0, 400),
            ("two", 20.0, 0.0, 700),
            // "two," is one word of two parts: the line does not break between them.
            (", three", 35.0, 0.0, 400),
            ("four ", 0.0, 12.5, 400),
            ("five", 25.0, 12.5, 400),
        ]
    );
    let accent = runs(&list)[1].color.role();
    assert_eq!(accent, Some(Role::Accent));
    // The link is underlined; the rest is not.
    assert!(runs(&list)[4].underline && !runs(&list)[0].underline);
    // Two lines tall (as whole pixels).
    assert_eq!(laid.rects().nth(2).map(|r| r.h), Some(25.0));
}

#[test]
fn a_word_of_two_parts_goes_to_the_next_line_whole() {
    let text = rich([span("aaaa b"), span("bb").weight(700)])
        .wrap()
        .text_size(10.0);
    // 30 pixels: six characters to a line, and "bbb" does not fit after "aaaa ".
    let laid = lay(div().child(div().w(30.0).child(text)), 200.0, 200.0);
    let list = laid.paint(&mut Fixed);
    let drawn: Vec<(&str, f32, f32)> = runs(&list)
        .iter()
        .map(|run| (run.text.as_str(), run.origin.x, run.origin.y))
        .collect();
    assert_eq!(
        drawn,
        vec![("aaaa", 0.0, 0.0), ("b", 0.0, 12.5), ("bb", 5.0, 12.5)]
    );
}

#[test]
fn a_text_of_many_looks_that_does_not_wrap_is_one_line_cut_to_fit_where_it_may_be() {
    let laid = lay(
        div().child(div().w(50.0).child(paragraph().truncate())),
        200.0,
        200.0,
    );
    let list = laid.paint(&mut Fixed);
    let words: String = runs(&list).iter().map(|run| run.text.as_str()).collect();
    // Three characters are left where the third part starts: "," and the "…".
    assert_eq!(words, "one two,\u{2026}");
    assert!(
        runs(&list)
            .iter()
            .all(|run| run.origin.x + run.text.chars().count() as f32 * 5.0 <= 50.0)
    );
}

#[test]
fn a_link_answers_the_pointer_by_its_name_and_is_read_as_a_link() {
    let laid = lay(
        div().child(div().w(80.0).child(paragraph().wrap())),
        200.0,
        200.0,
    );
    // "five" is on the second line, 25 to 45 pixels across.
    assert_eq!(laid.hit(Point::new(30.0, 15.0)), Some(LINK));
    assert_eq!(laid.hit(Point::new(10.0, 15.0)), None);
    let read = laid.accessible();
    assert_eq!(read[0].role, AccessibleRole::Text);
    assert_eq!(read[0].words, "one two, three four five");
    assert_eq!(read[1].role, AccessibleRole::Link);
    assert_eq!(read[1].id, Some(LINK));
    assert_eq!(read[1].words, "five");
    assert_eq!(read[1].rect, Rect::new(25.0, 12.5, 20.0, 12.5));
}

#[test]
fn a_part_with_a_ground_is_drawn_on_it_and_read_against_it() {
    // Size 8: a character is 4 pixels wide and a line 10 tall.
    let text = rich([
        span("run "),
        span("make").family(Family::Terminal).bg(Role::Raised(2)),
    ])
    .text_size(8.0);
    let laid = lay(div().bg(Role::Background).child(text), 200.0, 200.0);
    let list = laid.paint(&mut Fixed);
    let ground = list.items.iter().find_map(|item| match item {
        Paint::Quad(quad) if quad.fill.and_then(|fill| fill.role()) == Some(Role::Raised(2)) => {
            Some(quad.rect)
        }
        _ => None,
    });
    assert_eq!(ground, Some(Rect::new(16.0, 0.0, 16.0, 10.0)));
    let make = runs(&list)
        .into_iter()
        .find(|run| run.text == "make")
        .expect("the part is drawn");
    let grounds: Vec<Option<Role>> = make.grounds.iter().map(|ground| ground.role()).collect();
    assert_eq!(grounds, vec![Some(Role::Raised(2)), Some(Role::Background)]);
}

#[test]
fn a_text_of_many_looks_in_a_row_gives_its_room_away_down_to_its_longest_word() {
    let root = div().child(
        div()
            .row()
            .w(40.0)
            .child(rich([span("aa "), span("bbbb")]).wrap().text_size(10.0))
            .child(div().w(30.0).h(5.0)),
    );
    let laid = lay(root, 200.0, 200.0);
    let text = laid.rects().nth(2).expect("the text is laid out");
    // "bbbb" is 20 pixels: no narrower, though the row has 10 to give.
    assert!(text.w >= 20.0, "{text:?}");
    let list = laid.paint(&mut Fixed);
    assert_eq!(runs(&list).len(), 2);
}

#[test]
fn line_breaks_in_a_text_of_many_looks_start_new_lines() {
    let text = rich([span("a\n"), span("b").weight(700)]).text_size(8.0);
    let laid = lay(div().child(text), 200.0, 200.0);
    let list = laid.paint(&mut Fixed);
    let drawn: Vec<(&str, f32)> = runs(&list)
        .iter()
        .map(|run| (run.text.as_str(), run.origin.y))
        .collect();
    assert_eq!(drawn, vec![("a", 0.0), ("b", 10.0)]);
}
