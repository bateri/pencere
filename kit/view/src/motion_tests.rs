//! The guards of motion: a change moves over its time and stops, a row slides to its new place,
//! comes in and goes out; nothing moves under Reduce Motion, on a resize or for scrolling.

use crate::{
    Ease, Element, Font, Id, Laid, Measure, Metrics, Motion, Motions, Role, Shift, Size, div,
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

const LIST: Id = Id::new(1);
const BOX: Id = Id::new(2);
const PAGE: Size = Size::new(200.0, 100.0);
const FRAME: f64 = 1.0 / 60.0;

fn row_id(n: u64) -> Id {
    Id::new(100 + n)
}

fn rows_motion() -> Motion {
    Motion::new(0.2)
        .place()
        .enter(Shift::fade(0.2).by(0.0, 6.0))
        .exit(Shift::fade(0.1))
}

/// A list of the rows `ids`, 20 points each, scrolled `offset` points.
fn list(ids: &[u64], offset: f32) -> Element {
    div()
        .id(LIST)
        .col()
        .h_full()
        .scroll()
        .scrolled(0.0, offset)
        .children(ids.iter().map(|&n| {
            div()
                .id(row_id(n))
                .h(20.0)
                .no_shrink()
                .bg(Role::Accent)
                .motion(rows_motion())
        }))
}

fn lay(motions: &mut Motions, root: Element, now: f64) -> Laid {
    motions.layout(root, PAGE, 1.0, &mut Fixed, now, false)
}

/// Lays `root` out frame by frame from `now` until nothing moves; the frames it took.
fn settle(motions: &mut Motions, root: impl Fn() -> Element, mut now: f64) -> usize {
    let mut frames = 0;
    while motions.moving() {
        now += FRAME;
        lay(motions, root(), now);
        frames += 1;
        assert!(frames < 120, "the move never ends");
    }
    frames
}

#[test]
fn a_change_of_colour_moves_over_its_time_and_then_the_page_rests() {
    let page = |hot: bool| {
        div()
            .id(BOX)
            .w(50.0)
            .h(20.0)
            .bg(if hot {
                Role::Accent.solid()
            } else {
                Role::Accent.alpha(0.0)
            })
            .motion(Motion::new(0.12).color())
    };
    let mut motions = Motions::default();
    lay(&mut motions, page(false), 0.0);
    assert!(!motions.moving(), "a page that has just come does not move");
    lay(&mut motions, page(true), 1.0);
    assert!(motions.moving());
    let halfway = lay(&mut motions, page(true), 1.06);
    let fill = halfway.nodes[0].fill.expect("a fill on the way");
    assert!(fill.alpha > 0.1 && fill.alpha < 1.0, "{fill:?}");
    let frames = settle(&mut motions, || page(true), 1.06);
    assert!((2..=6).contains(&frames), "{frames}");
    let rest = lay(&mut motions, page(true), 2.0);
    assert_eq!(rest.nodes[0].fill, Some(Role::Accent.solid()));
    assert!(!motions.moving());
}

#[test]
fn a_width_in_points_follows_a_change_and_ends_where_it_was_said() {
    let page = |wide: bool| {
        div().row().child(
            div()
                .id(BOX)
                .h(20.0)
                .w(if wide { 150.0 } else { 100.0 })
                .motion(Motion::new(0.2).size()),
        )
    };
    let mut motions = Motions::default();
    lay(&mut motions, page(false), 0.0);
    lay(&mut motions, page(true), 1.0);
    let on_the_way = lay(&mut motions, page(true), 1.05);
    let width = on_the_way.bounds(BOX).unwrap().w;
    assert!(width > 100.0 && width < 150.0, "{width}");
    settle(&mut motions, || page(true), 1.05);
    assert_eq!(
        lay(&mut motions, page(true), 3.0).bounds(BOX).unwrap().w,
        150.0
    );
}

#[test]
fn a_row_moved_up_by_one_that_went_slides_and_the_one_that_went_fades_out() {
    let mut motions = Motions::default();
    lay(&mut motions, list(&[1, 2, 3], 0.0), 0.0);
    // The first row goes: the second is drawn where it was and slides up.
    let laid = lay(&mut motions, list(&[2, 3], 0.0), 1.0);
    assert_eq!(
        laid.bounds(row_id(2)).unwrap().y,
        20.0,
        "it starts where it was"
    );
    let on_the_way = lay(&mut motions, list(&[2, 3], 0.0), 1.05);
    let y = on_the_way.bounds(row_id(2)).unwrap().y;
    assert!(y > 0.0 && y < 20.0, "{y}");
    // The row that went is still drawn, fainter, where it was, and cannot be pressed.
    assert!(on_the_way.bounds(row_id(1)).is_none());
    let ghost = on_the_way.nodes.last().unwrap();
    assert_eq!(ghost.rect.y, 0.0);
    assert!(
        ghost.opacity < 1.0 && ghost.opacity > 0.0,
        "{}",
        ghost.opacity
    );
    assert!(ghost.meta.id.is_none());
    settle(&mut motions, || list(&[2, 3], 0.0), 1.05);
    let rest = lay(&mut motions, list(&[2, 3], 0.0), 3.0);
    assert_eq!(rest.bounds(row_id(2)).unwrap().y, 0.0);
    assert_eq!(
        rest.len(),
        3,
        "the list and its two rows, nothing that went"
    );
}

#[test]
fn a_row_that_comes_fades_in_from_below() {
    let mut motions = Motions::default();
    lay(&mut motions, list(&[1], 0.0), 0.0);
    let laid = lay(&mut motions, list(&[1, 2], 0.0), 1.0);
    let index = laid
        .nodes
        .iter()
        .position(|n| n.meta.id == Some(row_id(2)))
        .unwrap();
    assert_eq!(laid.nodes[index].opacity, 0.0);
    assert_eq!(laid.nodes[index].rect.y, 26.0, "six points below its place");
    settle(&mut motions, || list(&[1, 2], 0.0), 1.0);
    let rest = lay(&mut motions, list(&[1, 2], 0.0), 3.0);
    let index = rest
        .nodes
        .iter()
        .position(|n| n.meta.id == Some(row_id(2)))
        .unwrap();
    assert_eq!(
        (rest.nodes[index].opacity, rest.nodes[index].rect.y),
        (1.0, 20.0)
    );
}

#[test]
fn what_scrolling_brings_in_or_takes_out_does_not_move() {
    let ids: Vec<u64> = (1..=10).collect();
    let mut motions = Motions::default();
    // A list that makes only the rows in view, as a long one does: scrolled by 40, the first two
    // are gone and two more come, and the rows have new places on the page.
    lay(&mut motions, list(&ids[0..6], 0.0), 0.0);
    lay(
        &mut motions,
        div()
            .id(LIST)
            .col()
            .h_full()
            .scroll()
            .scrolled(0.0, 40.0)
            .child(div().h(40.0).no_shrink())
            .children(ids[2..8].iter().map(|&n| {
                div()
                    .id(row_id(n))
                    .h(20.0)
                    .no_shrink()
                    .motion(rows_motion())
            })),
        1.0,
    );
    assert!(!motions.moving());
}

#[test]
fn nothing_moves_when_the_page_changes_size_or_under_reduce_motion() {
    let mut motions = Motions::default();
    lay(&mut motions, list(&[1, 2], 0.0), 0.0);
    motions.layout(
        list(&[2], 0.0),
        Size::new(300.0, 100.0),
        1.0,
        &mut Fixed,
        1.0,
        false,
    );
    assert!(!motions.moving(), "a resize");
    motions.layout(
        list(&[1, 2], 0.0),
        Size::new(300.0, 100.0),
        1.0,
        &mut Fixed,
        2.0,
        true,
    );
    assert!(!motions.moving(), "reduce motion");
}

#[test]
fn the_eases_start_at_nothing_end_at_all_and_only_rise() {
    for ease in [Ease::Linear, Ease::Out, Ease::InOut, Ease::Spring] {
        assert!(ease.at(0.0).abs() < 1e-3, "{ease:?}");
        assert!((ease.at(1.0) - 1.0).abs() < 1e-3, "{ease:?}");
        let mut last = 0.0;
        for step in 1..=20 {
            let now = ease.at(step as f32 / 20.0);
            assert!(now >= last - 1e-4, "{ease:?} {now} < {last}");
            last = now;
        }
    }
    assert!(Ease::Out.at(0.25) > 0.5, "fast at first");
}

#[test]
fn a_tip_is_where_its_element_is_seen() {
    let page = div().child(div().w(40.0).h(10.0).tip("Containers", Some("1".into())));
    let laid = crate::layout(page, PAGE, 2.0, &mut Fixed);
    let tips = laid.tips();
    assert_eq!(tips.len(), 1);
    assert_eq!(tips[0].0, crate::Rect::new(0.0, 0.0, 40.0, 10.0));
    assert_eq!(tips[0].1.words, "Containers");
    assert_eq!(tips[0].1.keys.as_deref(), Some("1"));
}
