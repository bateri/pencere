//! The guards of motion: a change moves over its time and stops, a row slides to its new place,
//! comes in and goes out; nothing moves under Reduce Motion, on a resize or for scrolling.

use crate::{
    Ease, Element, Font, Id, Laid, Measure, Metrics, Motion, Motions, Paint, Role, Shift, Size,
    div, text,
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
fn the_words_of_a_row_that_goes_fade_out_with_it_in_their_own_colour() {
    let rows = |ids: &[u64]| {
        div()
            .id(LIST)
            .col()
            .h_full()
            .scroll()
            .children(ids.iter().map(|&n| {
                div()
                    .id(row_id(n))
                    .h(20.0)
                    .no_shrink()
                    .motion(rows_motion())
                    .child(text(format!("row {n}")).color(Role::Dim))
            }))
    };
    let mut motions = Motions::default();
    lay(&mut motions, rows(&[1, 2]), 0.0);
    lay(&mut motions, rows(&[2]), 1.0);
    let list = lay(&mut motions, rows(&[2]), 1.05).paint(&mut Fixed);
    let Some(Paint::Text(run)) = list
        .items
        .iter()
        .find(|item| matches!(item, Paint::Text(run) if run.text == "row 1"))
    else {
        panic!("the row that went is drawn");
    };
    assert_eq!(
        run.color,
        Role::Dim.solid(),
        "its words are in their colour"
    );
    assert!(
        run.opacity > 0.0 && run.opacity < 1.0,
        "and as faint as the row: {}",
        run.opacity
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

/// A list of rows that move in every way that only paints: a colour and an opacity that follow a
/// change, a slide, a coming in and a going out, a child inside a row that moves of its own, and a
/// spinner beside it.
fn painted_page(ids: &[u64], hot: bool) -> Element {
    let look = |hot: bool| {
        if hot {
            Role::Accent.solid()
        } else {
            Role::Accent.alpha(0.0)
        }
    };
    div()
        .row()
        .child(
            div()
                .id(LIST)
                .col()
                .w(100.0)
                .h_full()
                .children(ids.iter().map(|&n| {
                    div()
                        .id(row_id(n))
                        .h(20.0)
                        .no_shrink()
                        .bg(look(hot))
                        .opacity(if hot { 1.0 } else { 0.5 })
                        .motion(
                            Motion::new(0.2)
                                .color()
                                .opacity()
                                .place()
                                .enter(Shift::fade(0.2).by(0.0, 6.0))
                                .exit(Shift::fade(0.1).by(4.0, 0.0)),
                        )
                        .child(
                            div()
                                .id(Id::new(200 + n))
                                .size(10.0, 10.0)
                                .bg(look(hot))
                                .opacity(if hot { 0.8 } else { 1.0 })
                                .motion(Motion::new(0.3).color().opacity()),
                        )
                })),
        )
        .child(
            div()
                .size(20.0, 20.0)
                .bg(Role::Accent)
                .motion(Motion::new(0.1).spin(0.5)),
        )
}

#[test]
fn what_only_paints_is_drawn_from_the_layout_there_is_as_a_layout_would_draw_it() {
    let mut repainted = Motions::default();
    let mut laid_again = Motions::default();
    lay(&mut repainted, painted_page(&[1, 2, 3], false), 0.0);
    lay(&mut laid_again, painted_page(&[1, 2, 3], false), 0.0);
    // The first row goes, a fourth comes, and every row changes its colour and its opacity.
    let first = lay(&mut repainted, painted_page(&[2, 3, 4], true), 1.0);
    assert_eq!(
        first,
        lay(&mut laid_again, painted_page(&[2, 3, 4], true), 1.0)
    );
    let mut repaints = 0;
    for frame in 1..40 {
        let now = 1.0 + f64::from(frame) * FRAME;
        let again = lay(&mut laid_again, painted_page(&[2, 3, 4], true), now);
        assert!(!repainted.lays(), "nothing of it takes room");
        let drawn = repainted
            .repaint(now, false)
            .expect("a page that moves is drawn from its layout");
        repaints += 1;
        assert_eq!(drawn, again, "frame {frame}");
        match frame {
            // The row that went is still drawn at the third frame, and gone at the twentieth.
            2 => assert_eq!(drawn.len(), first.len(), "it is going"),
            20 => assert!(drawn.len() < first.len(), "it is gone"),
            _ => {}
        }
        if frame == 5 {
            assert_ne!(drawn, first, "the frames differ");
        }
        assert_eq!(repainted.moving(), laid_again.moving(), "frame {frame}");
        assert_eq!(repainted.ends(), laid_again.ends(), "frame {frame}");
    }
    assert_eq!(repaints, 39);
    assert!(
        repainted.moving() && !repainted.ends(),
        "only the spin goes on, which has no end"
    );
}

#[test]
fn what_takes_room_is_laid_out_again_and_what_is_not_there_to_draw_from_is_not_repainted() {
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
    assert!(motions.repaint(0.0, false).is_none(), "no page yet");
    lay(&mut motions, page(false), 0.0);
    assert!(!motions.moving() && !motions.lays());
    lay(&mut motions, page(true), 1.0);
    assert!(motions.moving() && motions.ends() && motions.lays());
    assert!(motions.repaint(1.05, false).is_none(), "it takes room");
    settle(&mut motions, || page(true), 1.0);
    assert!(!motions.moving() && !motions.lays());
    // A page with nothing on it that moves keeps nothing to draw from.
    let mut plain = Motions::default();
    lay(&mut plain, div().child(div().h(10.0)), 0.0);
    assert!(plain.repaint(0.1, false).is_none());
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
