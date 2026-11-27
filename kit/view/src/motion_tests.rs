//! The guards of motion: a change moves over its time and stops, a row slides to its new place,
//! comes in and goes out; nothing moves under Reduce Motion, on a resize or for scrolling.

use crate::layout::Node;
use crate::{
    Cycle, Ease, Element, Fit, Font, Id, Laid, Measure, Metrics, Motion, Motions, Paint, Rect,
    Role, Shift, Size, div, image, text,
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
const PAGE_TALL: Size = Size::new(200.0, 400.0);
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

/// A row with a box of `width` points, as `motion` makes it move.
fn spring_page(width: f32, motion: Motion) -> Element {
    div()
        .row()
        .child(div().id(BOX).h(20.0).w(width).no_shrink().motion(motion))
}

fn width_at(motions: &mut Motions, width: f32, motion: Motion, now: f64) -> f32 {
    lay(motions, spring_page(width, motion), now)
        .bounds(BOX)
        .unwrap()
        .w
}

#[test]
fn a_spring_that_does_not_bounce_is_the_curve_it_was_and_comes_to_rest() {
    let motion = Motion::new(0.5).size().ease(Ease::Spring);
    let mut motions = Motions::default();
    width_at(&mut motions, 100.0, motion, 0.0);
    width_at(&mut motions, 150.0, motion, 1.0);
    for tick in [0.05_f64, 0.1, 0.25, 0.4] {
        let width = width_at(&mut motions, 150.0, motion, 1.0 + tick);
        // Critically damped, from rest: ω is ten over the seconds.
        let x = 20.0 * tick;
        let expected = 100.0 + 50.0 * (1.0 - (1.0 + x) * (-x).exp());
        assert!(
            (f64::from(width) - expected).abs() < 0.6,
            "{tick}: {width} {expected}"
        );
    }
    assert!(motions.moving());
    assert_eq!(width_at(&mut motions, 150.0, motion, 1.5), 150.0);
    assert!(!motions.moving(), "it comes to rest in its seconds");
}

#[test]
fn a_spring_sent_elsewhere_in_flight_keeps_its_place_and_its_speed() {
    let motion = Motion::new(0.5).size().ease(Ease::Spring);
    let mut motions = Motions::default();
    width_at(&mut motions, 20.0, motion, 0.0);
    width_at(&mut motions, 180.0, motion, 1.0);
    let on_the_way = width_at(&mut motions, 180.0, motion, 1.1);
    assert!(on_the_way > 100.0 && on_the_way < 140.0, "{on_the_way}");
    // Two pages that are the same to here: one goes on to where it was going, the other is sent
    // below where it is, at this very time.
    let (mut goes_on, mut sent) = (motions.clone(), motions);
    let on = width_at(&mut goes_on, 180.0, motion, 1.1);
    let at = width_at(&mut sent, 60.0, motion, 1.1);
    assert_eq!(on, on_the_way);
    assert!(
        (at - on).abs() <= 1.0,
        "it does not jump: {at} against {on}"
    );
    // It was going up at some hundreds of points a second: sent down, it still goes up first, and
    // turns back for the new place, and stays at it.
    let later = width_at(&mut sent, 60.0, motion, 1.11);
    assert!(later > at + 3.0, "its speed is kept: {at} then {later}");
    let mut highest = later;
    let mut now = 1.11;
    while sent.moving() {
        now += FRAME;
        highest = highest.max(width_at(&mut sent, 60.0, motion, now));
        assert!(now < 4.0, "it never comes to rest");
    }
    assert!(highest > at + 3.0);
    assert_eq!(width_at(&mut sent, 60.0, motion, 5.0), 60.0);
}

#[test]
fn a_spring_that_bounces_goes_past_where_it_goes_and_comes_to_rest_in_its_time() {
    let run = |bounce: f32| {
        let motion = Motion::new(0.3).size().ease(Ease::Spring).bounce(bounce);
        let mut motions = Motions::default();
        width_at(&mut motions, 100.0, motion, 0.0);
        width_at(&mut motions, 150.0, motion, 1.0);
        let (mut widest, mut frames) = (0.0_f32, 0);
        while motions.moving() {
            frames += 1;
            assert!(frames < 60, "it never comes to rest");
            let width = width_at(&mut motions, 150.0, motion, 1.0 + f64::from(frames) * FRAME);
            widest = widest.max(width);
        }
        assert_eq!(width_at(&mut motions, 150.0, motion, 5.0), 150.0);
        (widest, frames)
    };
    let (still, _) = run(0.0);
    assert!(still <= 150.0, "no bounce, no more: {still}");
    let (bounced, frames) = run(0.6);
    assert!(bounced > 155.0, "it goes past: {bounced}");
    // A little longer for the bounce, and not more than the ceiling it has (the seconds, over
    // the damping that is left, with a frame to come to rest in).
    assert!(frames > 18 && frames <= 33, "{frames}");
}

#[test]
fn a_spring_is_held_to_what_a_colour_an_opacity_and_a_size_can_be() {
    let page = |on: bool| {
        div().row().child(
            div()
                .id(BOX)
                .h(20.0)
                .w(if on { 100.0 } else { 10.0 })
                .no_shrink()
                .bg(if on {
                    Role::Accent.solid()
                } else {
                    Role::Accent.alpha(0.0)
                })
                .opacity(if on { 1.0 } else { 0.3 })
                .motion(
                    Motion::new(0.3)
                        .color()
                        .opacity()
                        .size()
                        .ease(Ease::Spring)
                        .bounce(0.8),
                ),
        )
    };
    let mut motions = Motions::default();
    lay(&mut motions, page(false), 0.0);
    lay(&mut motions, page(true), 1.0);
    let mut frames = 0;
    let (mut most, mut least) = (0.0_f32, 1.0_f32);
    while motions.moving() {
        frames += 1;
        assert!(frames < 120);
        let laid = lay(&mut motions, page(true), 1.0 + f64::from(frames) * FRAME);
        let node = &laid.nodes[1];
        let fill = node.fill.expect("a fill");
        assert!((0.0..=1.0).contains(&fill.alpha), "{fill:?}");
        assert!((0.0..=1.0).contains(&node.opacity), "{}", node.opacity);
        assert!(node.rect.w >= 0.0);
        most = most.max(node.rect.w);
        least = least.min(node.opacity);
    }
    assert!(most > 100.0, "the width went past: {most}");
    assert!(least < 1.0);
}

#[test]
fn a_spring_is_laid_out_and_drawn_as_a_layout_would_for_what_only_paints() {
    let page = |on: bool| {
        div().col().child(
            div()
                .id(BOX)
                .w(120.0)
                .h(60.0)
                .rounded(if on { 12.0 } else { 0.0 })
                .bg(if on {
                    Role::Accent.solid()
                } else {
                    Role::Accent.alpha(0.2)
                })
                .motion(
                    Motion::new(0.2)
                        .rounded()
                        .color()
                        .ease(Ease::Spring)
                        .bounce(0.5),
                ),
        )
    };
    let mut repainted = Motions::default();
    let mut laid_again = Motions::default();
    lay(&mut repainted, page(false), 0.0);
    lay(&mut laid_again, page(false), 0.0);
    lay(&mut repainted, page(true), 1.0);
    lay(&mut laid_again, page(true), 1.0);
    for frame in 1..40 {
        let now = 1.0 + f64::from(frame) * FRAME;
        let again = lay(&mut laid_again, page(true), now);
        if !repainted.moving() {
            assert!(!laid_again.moving());
            break;
        }
        assert!(!repainted.lays(), "frame {frame}");
        let drawn = repainted
            .repaint(now, false)
            .expect("drawn from its layout");
        assert_eq!(drawn, again, "frame {frame}");
    }
}

#[test]
fn a_spring_snaps_under_reduce_motion_and_a_page_that_has_just_come_does_not_move() {
    let motion = Motion::new(0.5).size().ease(Ease::Spring).bounce(0.5);
    let mut motions = Motions::default();
    assert_eq!(width_at(&mut motions, 100.0, motion, 0.0), 100.0);
    assert!(!motions.moving());
    let snap = |motions: &mut Motions, width: f32, now: f64| {
        lay(motions, spring_page(width, motion), now);
        motions
            .layout(spring_page(width, motion), PAGE, 1.0, &mut Fixed, now, true)
            .bounds(BOX)
            .unwrap()
            .w
    };
    assert_eq!(snap(&mut motions, 150.0, 1.0), 150.0);
    assert!(
        !motions.moving(),
        "under Reduce Motion nothing is on its way"
    );
}

/// A card at rest as the page says it, with what it looks like turned on or off.
fn card(on: bool) -> Element {
    let mut card = div()
        .id(BOX)
        .w(120.0)
        .h(60.0)
        .row()
        .p(if on { 20.0 } else { 0.0 })
        .rounded(if on { 12.0 } else { 0.0 })
        .motion(Motion::new(0.2).padding().border().rounded().shadow())
        .child(div().id(Id::new(3)).size(10.0, 10.0));
    if on {
        card = card
            .border(6.0, Role::Accent)
            .shadow(0.0, 8.0, 10.0, Role::Accent);
    }
    div().col().child(card)
}

#[test]
fn the_numbers_of_how_an_element_looks_follow_a_change_and_the_ones_that_take_room_lay_out() {
    let mut motions = Motions::default();
    let kid = |laid: &Laid| laid.bounds(Id::new(3)).unwrap().x;
    let at_rest = lay(&mut motions, card(false), 0.0);
    assert_eq!(kid(&at_rest), 0.0);
    lay(&mut motions, card(true), 1.0);
    assert!(
        motions.moving() && motions.lays(),
        "padding and a border take room"
    );
    let halfway = lay(&mut motions, card(true), 1.05);
    let inset = kid(&halfway);
    assert!(inset > 0.0 && inset < 26.0, "{inset}");
    let node = &halfway.nodes[1];
    let radius = node.radii[0];
    assert!(radius > 0.0 && radius < 12.0, "{radius}");
    let shadow = node.shadow.expect("a shadow on the way");
    assert!(shadow.blur > 0.0 && shadow.blur < 10.0, "{shadow:?}");
    let border = node.border.expect("a border on the way");
    assert!(border.width > 0.0 && border.width < 6.0, "{border:?}");
    settle(&mut motions, || card(true), 1.05);
    let rest = lay(&mut motions, card(true), 3.0);
    // The border takes room in the box as the padding does: 6 of it and 20 of padding.
    assert_eq!(kid(&rest), 26.0);
    assert_eq!(rest.nodes[1].radii, [12.0; 4]);
    assert_eq!(rest.nodes[1].border.unwrap().width, 6.0);
    assert_eq!(rest.nodes[1].shadow.unwrap().blur, 10.0);
    // A border that goes is one while it goes, and none at the end.
    lay(&mut motions, card(false), 4.0);
    let going = lay(&mut motions, card(false), 4.05);
    assert!(going.nodes[1].border.is_some());
    settle(&mut motions, || card(false), 4.05);
    let gone = lay(&mut motions, card(false), 6.0);
    assert!(gone.nodes[1].border.is_none() && gone.nodes[1].shadow.is_none());
    assert_eq!(kid(&gone), 0.0);
}

#[test]
fn what_only_paints_of_the_looks_is_not_laid_out_for() {
    let page = |on: bool| {
        div().col().child(
            div()
                .id(BOX)
                .w(120.0)
                .h(60.0)
                .rounded(if on { 12.0 } else { 0.0 })
                .shadow(
                    0.0,
                    if on { 8.0 } else { -2.0 },
                    if on { 10.0 } else { 0.0 },
                    Role::Accent,
                )
                .motion(Motion::new(0.2).rounded().shadow()),
        )
    };
    let mut repainted = Motions::default();
    let mut laid_again = Motions::default();
    lay(&mut repainted, page(false), 0.0);
    lay(&mut laid_again, page(false), 0.0);
    lay(&mut repainted, page(true), 1.0);
    lay(&mut laid_again, page(true), 1.0);
    let mut moved = false;
    for frame in 1..10 {
        let now = 1.0 + f64::from(frame) * FRAME;
        let again = lay(&mut laid_again, page(true), now);
        assert!(!repainted.lays());
        let drawn = repainted
            .repaint(now, false)
            .expect("drawn from its layout");
        assert_eq!(drawn, again, "frame {frame}");
        if frame == 3 {
            let shadow = drawn.nodes[1].shadow.expect("a shadow");
            // From -2 to 8: it was above its box, and is on its way down.
            assert!(shadow.dy > -2.0 && shadow.dy < 8.0, "{shadow:?}");
            moved = true;
        }
    }
    assert!(moved);
}

#[test]
fn the_size_of_words_follows_a_change_from_the_size_they_were_inherited_at() {
    let page = |size: Option<f32>| {
        let mut chip = div()
            .id(BOX)
            .row()
            .motion(Motion::new(0.2).text_size())
            .child(text("hello").id(Id::new(3)));
        if let Some(size) = size {
            chip = chip.text_size(size);
        }
        div().col().text_size(20.0).child(chip)
    };
    let width = |laid: &Laid| laid.bounds(Id::new(3)).unwrap().w;
    let mut motions = Motions::default();
    // Five letters, half the size each.
    assert_eq!(width(&lay(&mut motions, page(None), 0.0)), 50.0);
    lay(&mut motions, page(Some(30.0)), 1.0);
    assert!(motions.lays());
    let halfway = width(&lay(&mut motions, page(Some(30.0)), 1.05));
    assert!(halfway > 50.0 && halfway < 75.0, "{halfway}");
    settle(&mut motions, || page(Some(30.0)), 1.05);
    assert_eq!(width(&lay(&mut motions, page(Some(30.0)), 3.0)), 75.0);
}

/// A picture of 16 points that loops as `cycle` says, at `now`: its box.
fn looped(motions: &mut Motions, cycle: Cycle, now: f64, reduce_motion: bool) -> Laid {
    motions.layout(
        div().child(
            image("a.png", Fit::Contain)
                .id(BOX)
                .size(16.0, 16.0)
                .motion(Motion::new(0.2).cycle(cycle)),
        ),
        PAGE,
        1.0,
        &mut Fixed,
        now,
        reduce_motion,
    )
}

#[test]
fn a_loop_goes_by_the_clock_alone_and_two_of_the_same_seconds_are_in_the_same_phase() {
    let cycle = Cycle::new(2.0).ease(Ease::Linear).by(20.0, 0.0);
    // Two pages that came at different times.
    let (mut early, mut late) = (Motions::default(), Motions::default());
    looped(&mut early, cycle, 0.1, false);
    looped(&mut late, cycle, 0.7, false);
    for now in [0.9, 1.0, 1.3, 1.9] {
        let (a, b) = (
            looped(&mut early, cycle, now, false),
            looped(&mut late, cycle, now, false),
        );
        assert_eq!(a.bounds(BOX), b.bounds(BOX), "at {now}");
        assert_eq!(a, b, "at {now}");
    }
    // There and back: nothing at the start, all of it at half, and nothing again at the whole.
    let x = |now: f64| {
        looped(&mut Motions::default(), cycle, now, false)
            .bounds(BOX)
            .unwrap()
            .x
    };
    assert_eq!(x(0.0), 0.0);
    assert_eq!(x(0.5), 10.0);
    assert_eq!(x(1.0), 20.0);
    assert_eq!(x(1.5), 10.0);
    assert_eq!(x(2.0), 0.0);
    // One way, and again from the start.
    let once = cycle.once();
    let x = |now: f64| {
        looped(&mut Motions::default(), once, now, false)
            .bounds(BOX)
            .unwrap()
            .x
    };
    assert_eq!(x(1.0), 10.0);
    assert_eq!(x(1.9), 19.0);
    assert_eq!(x(2.0), 0.0);
    // It goes on, and a page with one is never at rest.
    let mut motions = Motions::default();
    looped(&mut motions, cycle, 0.0, false);
    assert!(motions.moving() && !motions.ends());
}

#[test]
fn a_loop_that_fades_turns_in_half_a_second_at_the_least_and_one_that_only_turns_is_as_fast_as_it_says()
 {
    assert_eq!(Cycle::new(1.0).fade(0.0).opacity, 0.0);
    assert_eq!(Cycle::new(1.0).fade(0.7).opacity, 0.7);
    assert_eq!(Cycle::new(1.0).fade(3.0).opacity, 1.0);
    // A turn of one that fades takes no less than the least; one that does not fade has none.
    assert_eq!(Cycle::new(0.2).fade(0.3).seconds, Cycle::FASTEST_FADE);
    assert_eq!(Cycle::new(2.0).fade(0.3).seconds, 2.0);
    assert_eq!(Cycle::new(0.2).fade(1.0).seconds, 0.2);
    assert_eq!(
        Cycle::new(0.2)
            .turned(90.0)
            .by(5.0, 5.0)
            .scaled(2.0)
            .seconds,
        0.2
    );
    // Even one that was not made with `fade`: a turn of 0.2 seconds is one of half a second, in
    // which it is as faint as it goes at a quarter of a second, and at a tenth of it is not.
    let wild = Cycle {
        opacity: 0.0,
        ..Cycle::new(0.2).ease(Ease::Linear)
    };
    let opacity = |cycle: Cycle, now: f64| {
        looped(&mut Motions::default(), cycle, now, false).nodes[1].opacity
    };
    assert_eq!(opacity(wild, 0.25), 0.0);
    assert!(
        (opacity(wild, 0.1) - 0.6).abs() < 0.001,
        "{}",
        opacity(wild, 0.1)
    );
    assert_eq!(opacity(wild, 0.0), 1.0);
    // The same seconds that only turn go as fast as they were given.
    let turns = Cycle::new(0.2).ease(Ease::Linear).turned(90.0);
    assert_eq!(opacity(turns, 0.1), 1.0);
    let turn = |now: f64| looped(&mut Motions::default(), turns, now, false).nodes[1].turn;
    assert_eq!(turn(0.1), 90.0);
    assert_eq!(turn(0.05), 45.0);
}

#[test]
fn a_loop_and_a_spin_are_not_run_under_reduce_motion_and_the_element_stands_as_it_is() {
    let cycle = Cycle::new(2.0).ease(Ease::Linear).by(20.0, 0.0).fade(0.5);
    let mut motions = Motions::default();
    let moving = looped(&mut motions, cycle, 1.0, false);
    assert_ne!(moving.bounds(BOX).unwrap().x, 0.0);
    let still = looped(&mut motions, cycle, 1.0, true);
    let node = &still.nodes[1];
    assert_eq!(still.bounds(BOX).unwrap().x, 0.0);
    assert_eq!((node.opacity, node.turn, node.scaled), (1.0, 0.0, 1.0));
    assert!(!motions.moving());
}

#[test]
fn a_spin_is_the_loop_that_turns_a_whole_way_at_one_pace_and_again() {
    let spun = |motion: Motion, now: f64| {
        Motions::default()
            .layout(
                div().child(
                    image("a.png", Fit::Contain)
                        .id(BOX)
                        .size(16.0, 16.0)
                        .motion(motion),
                ),
                PAGE,
                1.0,
                &mut Fixed,
                now,
                false,
            )
            .nodes[1]
            .turn
    };
    let spin = Motion::new(0.2).spin(2.0);
    let same = Motion::new(0.2).cycle(Cycle::new(2.0).ease(Ease::Linear).once().turned(360.0));
    for now in [0.0, 0.25, 0.5, 1.0, 1.7, 2.0, 3.1] {
        assert_eq!(spun(spin, now), spun(same, now), "at {now}");
    }
    assert_eq!(spun(spin, 0.5), 90.0);
}

#[test]
fn the_turn_the_page_gave_a_change_that_moves_a_loop_and_a_spin_come_together() {
    // A picture turned 30 and drawn twice as large, its turn and its size followed by a change; a
    // loop of 40 degrees and one and a half times; and a spin.
    let motion = Motion::new(0.2)
        .transform()
        .spin(4.0)
        .cycle(Cycle::new(2.0).ease(Ease::Linear).turned(40.0).scaled(1.5));
    let page = |turn: f32| {
        div().child(
            image("a.png", Fit::Contain)
                .id(BOX)
                .size(16.0, 16.0)
                .turned(turn)
                .scaled(2.0)
                .motion(motion),
        )
    };
    let mut motions = Motions::default();
    let at = |motions: &mut Motions, turn: f32, now: f64| {
        let laid = motions.layout(page(turn), PAGE, 1.0, &mut Fixed, now, false);
        (laid.nodes[1].turn, laid.nodes[1].scaled)
    };
    // At 1.0 the loop is all the way there (alternating, linear) and the spin a quarter round.
    at(&mut motions, 30.0, 0.0);
    let (turn, scaled) = at(&mut motions, 30.0, 1.0);
    assert!((turn - (30.0 + 40.0 + 90.0)).abs() < 0.01, "{turn}");
    assert!((scaled - 2.0 * 1.5).abs() < 0.001, "{scaled}");
    // The page turns it to 90 at 5.0 (the loop and the spin are at the same place again: 5.0 is
    // two turns and a half of the loop, and a turn and a quarter of the spin): the turn is on its
    // way to it and the others go on over it.
    let (turn, _) = at(&mut motions, 90.0, 5.0);
    assert!(
        (turn - (30.0 + 40.0 + 90.0)).abs() < 0.01,
        "the change has not begun: {turn}"
    );
    let (turn, scaled) = at(&mut motions, 90.0, 5.1);
    let spin = 360.0 * 5.1 / 4.0 % 360.0;
    let loops = 40.0 * (1.0 - (2.0 * (5.1_f64 / 2.0 % 1.0) - 1.0).abs());
    assert!(
        turn > 30.0 + (loops + spin) as f32,
        "it is turning to 90: {turn}"
    );
    assert!(turn < 90.0 + (loops + spin) as f32 + 0.01, "{turn}");
    assert!(scaled > 2.0, "{scaled}");
}

/// Three boxes of 100 points in a column of a tall page, in the order `ids`, that slide as a spring
/// when they are moved among the others, and the one that comes is a spring that comes in from below.
fn column(ids: &[u64]) -> Element {
    div().col().children(ids.iter().map(|&n| {
        div().id(row_id(n)).h(100.0).no_shrink().motion(
            Motion::new(0.5)
                .place()
                .ease(Ease::Spring)
                .enter(Shift::fade(0.5).by(0.0, 60.0)),
        )
    }))
}

fn lay_tall(motions: &mut Motions, root: Element, now: f64) -> Laid {
    motions.layout(root, Size::new(200.0, 400.0), 1.0, &mut Fixed, now, false)
}

fn top_of(laid: &Laid, n: u64) -> f32 {
    laid.bounds(row_id(n)).unwrap().y
}

#[test]
fn a_box_that_slides_as_a_spring_and_is_moved_again_on_its_way_keeps_its_place_and_its_speed() {
    let mut motions = Motions::default();
    lay_tall(&mut motions, column(&[1, 2, 3]), 0.0);
    // The first goes to the end of the column: 200 points down.
    lay_tall(&mut motions, column(&[2, 3, 1]), 1.0);
    let on_the_way = top_of(&lay_tall(&mut motions, column(&[2, 3, 1]), 1.1), 1);
    assert!(on_the_way > 60.0 && on_the_way < 160.0, "{on_the_way}");
    // Two pages the same to here: one goes on, one puts the box back at the top at this very time.
    let (mut goes_on, mut sent) = (motions.clone(), motions);
    let on = top_of(&lay_tall(&mut goes_on, column(&[2, 3, 1]), 1.1), 1);
    let back = top_of(&lay_tall(&mut sent, column(&[1, 2, 3]), 1.1), 1);
    assert_eq!(on, on_the_way);
    assert!(
        (back - on).abs() <= 1.0,
        "it does not jump: {back} against {on}"
    );
    // It was going down at some hundreds of points a second: sent up, it goes down a little first
    // and then turns, as a spring does, and a box with no speed kept would go up at once.
    let later = top_of(&lay_tall(&mut sent, column(&[1, 2, 3]), 1.115), 1);
    assert!(later > back + 3.0, "its speed is kept: {back} then {later}");
    let mut now = 1.115;
    while sent.moving() {
        now += FRAME;
        lay_tall(&mut sent, column(&[1, 2, 3]), now);
        assert!(now < 5.0, "it never comes to rest");
    }
    assert_eq!(
        top_of(&lay_tall(&mut sent, column(&[1, 2, 3]), 9.0), 1),
        0.0
    );
}

#[test]
fn a_box_that_comes_in_as_a_spring_while_the_ones_before_it_go_is_where_it_was_and_goes_on() {
    let mut motions = Motions::default();
    lay_tall(&mut motions, column(&[1, 2]), 0.0);
    // A third comes in from 60 points below its place (the place is the third row's: 200).
    lay_tall(&mut motions, column(&[1, 2, 3]), 1.0);
    let before = top_of(&lay_tall(&mut motions, column(&[1, 2, 3]), 1.1), 3);
    assert!(before > 200.0 && before < 260.0, "{before}");
    let mut control = motions.clone();
    let steady = top_of(&lay_tall(&mut control, column(&[1, 2, 3]), 1.115), 3);
    // The first goes while it comes in: the third's place is 100 points higher.
    let after = top_of(&lay_tall(&mut motions, column(&[2, 3]), 1.1), 3);
    assert!(
        (after - before).abs() <= 1.0,
        "it does not jump: {before} then {after}"
    );
    // Its coming in is not changed by it: the same 15 milliseconds later it has come in as far as it
    // would have, and the move to its new place goes on from where it was.
    let later = top_of(&lay_tall(&mut motions, column(&[2, 3]), 1.115), 3);
    let came = steady - before;
    assert!(
        (later - after) < came + 1.0 && (later - after) > came - 12.0,
        "{before} {steady}; {after} {later}"
    );
}

const HEAD: Id = Id::new(40);
const BODY: Id = Id::new(41);
const FOOT: Id = Id::new(42);
const LINES: u64 = 43;

fn line(n: u64) -> Id {
    Id::new(LINES + n)
}

/// A head of 20 points, a body of two lines of 40 (when it is open) that follows the size it is
/// laid out at, and a foot of 20. With `parts`, the lines go out as they went.
fn accordion(open: bool, body: Motion, parts: bool) -> Element {
    let lines: Vec<Element> = if open {
        (0..2)
            .map(|n| {
                let line = div().id(line(n)).h(40.0).no_shrink();
                if parts {
                    line.motion(Motion::new(0.2).exit(Shift {
                        opacity: 1.0,
                        ..Shift::fade(0.2)
                    }))
                } else {
                    line
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    div().col().children(vec![
        div().id(HEAD).h(20.0).no_shrink(),
        div().id(BODY).col().motion(body).children(lines),
        div().id(FOOT).h(20.0).no_shrink(),
    ])
}

fn body_motion() -> Motion {
    Motion::new(0.2).auto_size()
}

fn at(laid: &Laid, id: Id) -> Rect {
    laid.bounds(id)
        .unwrap_or_else(|| panic!("{id:?} is on the page"))
}

fn node_of(laid: &Laid, id: Id) -> &Node {
    laid.nodes
        .iter()
        .find(|node| node.meta.id == Some(id))
        .unwrap_or_else(|| panic!("{id:?} is on the page"))
}

#[test]
fn a_box_that_opens_to_its_content_shows_what_fits_and_nothing_inside_it_moves() {
    let mut motions = Motions::default();
    lay_tall(&mut motions, accordion(false, body_motion(), false), 0.0);
    let first = lay_tall(&mut motions, accordion(true, body_motion(), false), 1.0);
    assert_eq!(at(&first, BODY).h, 0.0, "it starts where it was");
    let lines = [at(&first, line(0)), at(&first, line(1))];
    assert_eq!(
        (lines[0].y, lines[0].h, lines[1].y, lines[1].h),
        (20.0, 40.0, 60.0, 40.0)
    );
    let mut was = 0.0;
    let mut now = 1.0;
    let mut mid = false;
    while motions.moving() {
        now += FRAME;
        let laid = lay_tall(&mut motions, accordion(true, body_motion(), false), now);
        let body = at(&laid, BODY);
        assert!(
            body.h >= was && body.h <= 80.0,
            "it grows to its size: {was} then {}",
            body.h
        );
        was = body.h;
        mid |= body.h > 5.0 && body.h < 75.0;
        // Each line is where the first layout had it, whatever the box shows of it.
        assert_eq!([at(&laid, line(0)), at(&laid, line(1))], lines, "at {now}");
        // What is under the box follows its edge; what is in it is cut where it ends.
        assert!((at(&laid, FOOT).y - (20.0 + body.h)).abs() <= 1.0);
        let cut = node_of(&laid, line(1)).clip.rect;
        assert!(
            body.h >= 80.0 || cut.y + cut.h <= body.y + body.h + 0.5,
            "cut at {} of {}",
            cut.y + cut.h,
            body.h
        );
        assert!(node_of(&laid, BODY).clips || body.h >= 80.0);
    }
    assert!(mid, "it was seen on its way");
    let rest = lay_tall(
        &mut motions,
        accordion(true, body_motion(), false),
        now + 1.0,
    );
    assert_eq!(at(&rest, BODY).h, 80.0);
    assert_eq!(at(&rest, FOOT).y, 100.0);
    assert!(
        !node_of(&rest, BODY).clips,
        "at rest the page's own clip stands"
    );
    assert!(node_of(&rest, line(1)).clip.rect.h > 300.0);
}

#[test]
fn a_box_that_closes_cuts_the_lines_that_went_with_the_clip_it_has_now() {
    let mut motions = Motions::default();
    lay_tall(&mut motions, accordion(true, body_motion(), true), 0.0);
    let page = lay_tall(&mut motions, accordion(true, body_motion(), true), 0.5);
    let (open, count) = (at(&page, line(1)), page.nodes.len());
    let first = lay_tall(&mut motions, accordion(false, body_motion(), true), 1.0);
    assert_eq!(at(&first, BODY).h, 80.0, "it starts where it was");
    assert_eq!(
        first.nodes.len(),
        count,
        "the lines are still drawn: they go as they went"
    );
    let mut now = 1.0;
    let mut cut_once = false;
    while motions.moving() {
        now += FRAME;
        let laid = lay_tall(&mut motions, accordion(false, body_motion(), true), now);
        let body = at(&laid, BODY);
        for ghost in laid.nodes.iter().skip(4) {
            assert!(ghost.meta.id.is_none());
            assert_eq!(ghost.rect.y, open.y.min(ghost.rect.y), "it does not move");
            assert!(
                ghost.clip.rect.y + ghost.clip.rect.h <= body.y + body.h + 0.5,
                "at {now}: cut at {} of {}",
                ghost.clip.rect.y + ghost.clip.rect.h,
                body.h
            );
            cut_once |= body.h < 70.0;
        }
        assert!((at(&laid, FOOT).y - (20.0 + body.h)).abs() <= 1.0);
    }
    assert!(cut_once);
    let rest = lay_tall(
        &mut motions,
        accordion(false, body_motion(), true),
        now + 1.0,
    );
    assert_eq!(
        (at(&rest, BODY).h, rest.nodes.len()),
        (0.0, 4),
        "nothing is left of it"
    );
}

#[test]
fn a_box_inside_a_box_that_opens_with_it_is_followed_by_what_comes_after_it() {
    const OUT: Id = Id::new(60);
    const IN: Id = Id::new(61);
    const TAIL: Id = Id::new(62);
    let page = |open: bool| {
        let lines: Vec<Element> = if open {
            (0..2)
                .map(|n| div().id(line(n)).h(40.0).no_shrink())
                .collect()
        } else {
            Vec::new()
        };
        div().col().children(vec![
            div().id(HEAD).h(20.0).no_shrink(),
            div().id(OUT).col().motion(body_motion()).children(vec![
                div().h(20.0).no_shrink(),
                div().id(IN).col().motion(body_motion()).children(lines),
                div().id(TAIL).h(20.0).no_shrink(),
            ]),
            div().id(FOOT).h(20.0).no_shrink(),
        ])
    };
    let mut motions = Motions::default();
    lay_tall(&mut motions, page(false), 0.0);
    lay_tall(&mut motions, page(true), 1.0);
    let mut now = 1.0;
    let mut mid = false;
    while motions.moving() {
        now += FRAME;
        let laid = lay_tall(&mut motions, page(true), now);
        let (out, inner, tail) = (at(&laid, OUT), at(&laid, IN), at(&laid, TAIL));
        assert!(
            inner.y + inner.h <= out.y + out.h + 1.0,
            "the inner is cut by the outer"
        );
        assert!(
            (tail.y - (inner.y + inner.h)).abs() <= 1.0,
            "it follows the inner at {now}"
        );
        assert!(
            (at(&laid, FOOT).y - (out.y + out.h)).abs() <= 1.0,
            "and the foot the outer"
        );
        mid |= inner.h > 5.0 && inner.h < 75.0;
    }
    assert!(mid);
    let rest = lay_tall(&mut motions, page(true), now + 1.0);
    assert_eq!(
        (at(&rest, IN).h, at(&rest, OUT).h, at(&rest, FOOT).y),
        (80.0, 120.0, 140.0)
    );
}

#[test]
fn what_says_a_size_in_points_with_auto_size_moves_as_it_does_alone() {
    let by = |motion: Motion| {
        let mut motions = Motions::default();
        let page = |open: bool| {
            div().col().children(vec![
                div().id(HEAD).h(20.0).no_shrink(),
                div()
                    .id(BODY)
                    .h(if open { 80.0 } else { 0.0 })
                    .motion(motion),
                div().id(FOOT).h(20.0).no_shrink(),
            ])
        };
        lay_tall(&mut motions, page(false), 0.0);
        let mut heights = vec![at(&lay_tall(&mut motions, page(true), 1.0), BODY).h];
        let mut now = 1.0;
        while motions.moving() {
            now += FRAME;
            heights.push(at(&lay_tall(&mut motions, page(true), now), BODY).h);
        }
        heights
    };
    let alone = by(body_motion());
    assert_eq!(by(body_motion().size()), alone);
    assert!(alone.iter().any(|h| *h > 5.0 && *h < 75.0), "{alone:?}");
}

#[test]
fn a_box_that_is_there_when_the_page_first_is_laid_out_does_not_move_and_nor_does_one_under_reduce_motion()
 {
    let mut motions = Motions::default();
    let first = lay_tall(&mut motions, accordion(true, body_motion(), false), 0.0);
    assert_eq!(at(&first, BODY).h, 80.0);
    assert!(!motions.moving());
    let mut still = Motions::default();
    still.layout(
        accordion(false, body_motion(), false),
        PAGE_TALL,
        1.0,
        &mut Fixed,
        0.0,
        true,
    );
    let laid = still.layout(
        accordion(true, body_motion(), false),
        PAGE_TALL,
        1.0,
        &mut Fixed,
        1.0,
        true,
    );
    assert_eq!(at(&laid, BODY).h, 80.0);
    assert!(!still.moving() && !still.lays());
    // Nor on a resize: that is not a change of the page.
    let mut resized = Motions::default();
    lay_tall(&mut resized, accordion(false, body_motion(), false), 0.0);
    let wider = resized.layout(
        accordion(true, body_motion(), false),
        Size::new(300.0, 400.0),
        1.0,
        &mut Fixed,
        1.0,
        false,
    );
    assert_eq!(at(&wider, BODY).h, 80.0);
    assert!(!resized.moving());
}

#[test]
fn a_box_that_has_room_to_grow_is_the_size_it_is_on_its_way_and_not_what_is_left() {
    const PANEL: Id = Id::new(70);
    let page = |room: f32| {
        div().col().child(div().col().h(room).children(vec![
            div().id(HEAD).h(20.0).no_shrink(),
            div().id(PANEL).grow(1.0).motion(body_motion()),
        ]))
    };
    let mut motions = Motions::default();
    lay_tall(&mut motions, page(200.0), 0.0);
    assert_eq!(
        at(&lay_tall(&mut motions, page(200.0), 0.1), PANEL).h,
        180.0
    );
    let first = at(&lay_tall(&mut motions, page(100.0), 1.0), PANEL).h;
    assert_eq!(first, 180.0, "it starts where it was");
    let (mut was, mut now, mut mid) = (first, 1.0, false);
    while motions.moving() {
        now += FRAME;
        let h = at(&lay_tall(&mut motions, page(100.0), now), PANEL).h;
        assert!(h <= was && h >= 80.0, "{was} then {h}");
        mid |= h > 100.0 && h < 160.0;
        was = h;
    }
    assert!(mid);
    assert_eq!(
        at(&lay_tall(&mut motions, page(100.0), now + 1.0), PANEL).h,
        80.0
    );
}

#[test]
fn a_box_that_goes_after_what_changes_inside_it_each_frame_comes_to_rest() {
    const PAD: Id = Id::new(80);
    let page = |padded: bool| {
        div().col().children(vec![
            div().id(BODY).col().motion(body_motion()).children(vec![
                div()
                    .id(PAD)
                    .p(if padded { 30.0 } else { 0.0 })
                    .child(div().h(20.0).no_shrink())
                    .motion(Motion::new(0.2).padding()),
            ]),
            div().id(FOOT).h(20.0).no_shrink(),
        ])
    };
    let mut motions = Motions::default();
    lay_tall(&mut motions, page(false), 0.0);
    let (mut was, mut now) = (at(&lay_tall(&mut motions, page(true), 1.0), BODY).h, 1.0);
    assert_eq!(was, 20.0);
    while motions.moving() {
        now += FRAME;
        let h = at(&lay_tall(&mut motions, page(true), now), BODY).h;
        assert!(h >= was && h <= 80.0, "{was} then {h}");
        assert!((at(&lay_tall(&mut motions, page(true), now), FOOT).y - h).abs() <= 1.0);
        was = h;
        assert!(now < 3.0, "it never comes to rest");
    }
    assert!(
        now < 1.0 + 0.2 + 0.2 + 0.1,
        "it is at rest within the two moves: {now}"
    );
    assert_eq!(
        at(&lay_tall(&mut motions, page(true), now + 1.0), BODY).h,
        80.0
    );
}

#[test]
fn a_box_that_opens_inside_one_that_scrolls_makes_the_room_of_it_grow_as_it_goes() {
    const SCROLLER: Id = Id::new(90);
    let page = |open: bool| {
        div().id(SCROLLER).col().h(100.0).scroll().children(vec![
            div().id(HEAD).h(20.0).no_shrink(),
            div()
                .id(BODY)
                .col()
                .motion(body_motion())
                .children(if open {
                    (0..2)
                        .map(|n| div().id(line(n)).h(40.0).no_shrink())
                        .collect()
                } else {
                    Vec::new()
                }),
            div().id(FOOT).h(20.0).no_shrink(),
        ])
    };
    let mut motions = Motions::default();
    lay_tall(&mut motions, page(false), 0.0);
    let content = |laid: &Laid| {
        laid.scroll_extent(SCROLLER)
            .expect("a box that scrolls")
            .content
            .h
    };
    assert_eq!(content(&lay_tall(&mut motions, page(true), 1.0)), 40.0);
    let (mut was, mut now, mut mid) = (40.0, 1.0, false);
    while motions.moving() {
        now += FRAME;
        let h = content(&lay_tall(&mut motions, page(true), now));
        assert!(h >= was && h <= 120.0, "{was} then {h}");
        mid |= h > 60.0 && h < 100.0;
        was = h;
    }
    assert!(mid);
    assert_eq!(
        content(&lay_tall(&mut motions, page(true), now + 1.0)),
        120.0
    );
}

#[test]
fn laying_the_page_out_again_for_the_same_time_gives_the_same_boxes_and_a_spring_goes_on_at_its_speed()
 {
    let spring = || body_motion().ease(Ease::Spring);
    let mut motions = Motions::default();
    lay_tall(&mut motions, accordion(false, spring(), false), 0.0);
    lay_tall(&mut motions, accordion(true, spring(), false), 1.0);
    let once = at(
        &lay_tall(&mut motions, accordion(true, spring(), false), 1.02),
        BODY,
    );
    let twice = at(
        &lay_tall(&mut motions, accordion(true, spring(), false), 1.02),
        BODY,
    );
    assert_eq!(once, twice);
    // Sent back where it came from in flight, it does not jump, and it goes on up for a while as a
    // spring does.
    let (mut goes_on, mut sent) = (motions.clone(), motions);
    let on = at(
        &lay_tall(&mut goes_on, accordion(true, spring(), false), 1.02),
        BODY,
    )
    .h;
    let back = at(
        &lay_tall(&mut sent, accordion(false, spring(), false), 1.02),
        BODY,
    )
    .h;
    assert!(
        (back - on).abs() <= 1.0,
        "it does not jump: {back} against {on}"
    );
    let later = at(
        &lay_tall(&mut sent, accordion(false, spring(), false), 1.035),
        BODY,
    )
    .h;
    assert!(later > back + 3.0, "its speed is kept: {back} then {later}");
    let mut now = 1.035;
    while sent.moving() {
        now += FRAME;
        lay_tall(&mut sent, accordion(false, spring(), false), now);
        assert!(now < 6.0);
    }
    assert_eq!(
        at(
            &lay_tall(&mut sent, accordion(false, spring(), false), 9.0),
            BODY
        )
        .h,
        0.0
    );
}

#[test]
fn a_row_of_a_long_list_that_opens_goes_over_its_time_and_the_list_is_not_built_again_for_it() {
    use crate::rows::{List, Rows};
    const ROWS: Id = Id::new(95);
    const OPENS: Id = Id::new(1003);
    let mut list = List::new(ROWS);
    let page = |list: &List, open: bool| {
        let rows = Rows::new((0..1000).map(|n| if n == 3 && open { 80.0 } else { 20.0 }));
        div()
            .col()
            .child(list.build(div().h(100.0), rows, 100.0, |n| {
                let id = Id::new(1000 + n as u64);
                if n == 3 {
                    div()
                        .id(id)
                        .col()
                        .motion(body_motion())
                        .child(div().h(if open { 80.0 } else { 20.0 }).no_shrink())
                } else {
                    div().id(id).h(20.0).no_shrink()
                }
            }))
    };
    let mut motions = Motions::default();
    let laid = lay_tall(&mut motions, page(&list, false), 0.0);
    assert!(!list.settle(&laid));
    lay_tall(&mut motions, page(&list, true), 1.0);
    let (mut was, mut now, mut mid) = (20.0, 1.0, false);
    while motions.moving() {
        now += FRAME;
        let laid = lay_tall(&mut motions, page(&list, true), now);
        let h = at(&laid, OPENS).h;
        assert!(h >= was && h <= 80.0, "{was} then {h}");
        assert!(!list.settle(&laid), "the list is not built again at {now}");
        mid |= h > 30.0 && h < 70.0;
        was = h;
    }
    assert!(mid);
    let rest = lay_tall(&mut motions, page(&list, true), now + 1.0);
    assert_eq!(at(&rest, OPENS).h, 80.0);
    assert_eq!(
        rest.scroll_extent(ROWS).expect("a list").content.h,
        20.0 * 999.0 + 80.0
    );
}

#[test]
fn a_box_that_narrows_has_what_stretches_in_it_laid_out_for_the_width_it_goes_to() {
    const SIDE: Id = Id::new(96);
    const WORDS: Id = Id::new(97);
    let page = |wide: bool| {
        div().row().child(
            div()
                .id(SIDE)
                .w(if wide { 160.0 } else { 40.0 })
                .motion(body_motion())
                .child(div().id(WORDS).h(10.0).no_shrink()),
        )
    };
    let mut motions = Motions::default();
    let before = at(&lay_tall(&mut motions, page(true), 0.0), WORDS);
    let first = lay_tall(&mut motions, page(false), 1.0);
    let target = at(&first, WORDS);
    assert_eq!((before.w, target.w), (160.0, 40.0));
    let (mut now, mut mid) = (1.0, false);
    while motions.moving() {
        now += FRAME;
        let laid = lay_tall(&mut motions, page(false), now);
        assert_eq!(at(&laid, WORDS), target, "at {now}");
        let side = at(&laid, SIDE).w;
        mid |= side > 60.0 && side < 140.0;
    }
    assert!(mid);
    assert_eq!(
        at(&lay_tall(&mut motions, page(false), now + 1.0), SIDE).w,
        40.0
    );
}

#[test]
fn a_row_that_slides_of_its_own_is_where_it_was_against_its_box_while_a_box_above_it_opens() {
    const LISTED: Id = Id::new(98);
    const ROW: Id = Id::new(99);
    let rows = Motion::new(0.2).place();
    // The row is in a box of its own that is below the one that opens: it follows that box, and
    // there is no slide to it (nothing moved it against what holds it).
    let held = |open: bool| {
        div().col().children(vec![
            div()
                .id(BODY)
                .col()
                .motion(body_motion())
                .children(if open {
                    vec![div().h(80.0).no_shrink()]
                } else {
                    Vec::new()
                }),
            div()
                .id(LISTED)
                .col()
                .child(div().id(ROW).h(20.0).no_shrink().motion(rows)),
        ])
    };
    let mut motions = Motions::default();
    lay_tall(&mut motions, held(false), 0.0);
    lay_tall(&mut motions, held(true), 1.0);
    let (mut now, mut mid) = (1.0, false);
    while motions.moving() {
        now += FRAME;
        let laid = lay_tall(&mut motions, held(true), now);
        let (list, row) = (at(&laid, LISTED), at(&laid, ROW));
        assert_eq!(row.y, list.y, "at {now}");
        assert!((list.y - at(&laid, BODY).h).abs() <= 1.0);
        mid |= list.y > 5.0 && list.y < 75.0;
    }
    assert!(mid);
    // The row is in the same box as the one that opens: it is moved, and it slides there once.
    let sibling = |open: bool| {
        div().col().id(LISTED).children(vec![
            div()
                .id(BODY)
                .col()
                .motion(body_motion())
                .children(if open {
                    vec![div().h(80.0).no_shrink()]
                } else {
                    Vec::new()
                }),
            div().id(ROW).h(20.0).no_shrink().motion(rows),
        ])
    };
    let mut motions = Motions::default();
    lay_tall(&mut motions, sibling(false), 0.0);
    assert_eq!(at(&lay_tall(&mut motions, sibling(true), 1.0), ROW).y, 0.0);
    let (mut now, mut was, mut mid) = (1.0, 0.0, false);
    while motions.moving() {
        now += FRAME;
        let y = at(&lay_tall(&mut motions, sibling(true), now), ROW).y;
        assert!(y >= was && y <= 80.0, "{was} then {y}");
        mid |= y > 5.0 && y < 75.0;
        was = y;
    }
    assert!(mid);
    assert_eq!(
        at(&lay_tall(&mut motions, sibling(true), now + 1.0), ROW).y,
        80.0
    );
}

#[test]
fn a_box_that_scrolls_and_opens_is_as_large_as_it_is_on_its_way_and_holds_what_it_will_hold() {
    let page = |open: bool| {
        div().col().children(vec![
            div()
                .id(BODY)
                .col()
                .scroll()
                .motion(body_motion())
                .children(if open {
                    vec![
                        div().id(line(0)).h(60.0).no_shrink(),
                        div().id(line(1)).h(60.0).no_shrink(),
                    ]
                } else {
                    Vec::new()
                }),
            div().id(FOOT).h(20.0).no_shrink(),
        ])
    };
    let mut motions = Motions::default();
    lay_tall(&mut motions, page(false), 0.0);
    lay_tall(&mut motions, page(true), 1.0);
    let (mut now, mut mid) = (1.0, false);
    while motions.moving() {
        now += FRAME;
        let laid = lay_tall(&mut motions, page(true), now);
        let (body, extent) = (
            at(&laid, BODY),
            laid.scroll_extent(BODY).expect("a box that scrolls"),
        );
        assert_eq!(
            extent.content.h, 120.0,
            "it holds what it will hold at {now}"
        );
        assert!(
            (extent.view.h - body.h).abs() <= 1.0,
            "and is as large as it is"
        );
        assert!((at(&laid, FOOT).y - body.h).abs() <= 1.0);
        mid |= body.h > 20.0 && body.h < 100.0;
    }
    assert!(mid);
}

#[test]
fn what_went_from_a_box_that_follows_its_size_is_cut_by_it_at_rest_too() {
    let page = |open: bool| {
        let lines: Vec<Element> = if open {
            (0..2)
                .map(|n| {
                    div()
                        .id(line(n))
                        .h(40.0)
                        .no_shrink()
                        .motion(Motion::new(0.2).exit(Shift {
                            opacity: 1.0,
                            ..Shift::fade(0.6)
                        }))
                })
                .collect()
        } else {
            Vec::new()
        };
        div().col().children(vec![
            div().id(HEAD).h(20.0).no_shrink(),
            div().id(BODY).col().motion(body_motion()).children(lines),
            div().id(FOOT).h(20.0).no_shrink(),
        ])
    };
    let mut motions = Motions::default();
    lay_tall(&mut motions, page(true), 0.0);
    lay_tall(&mut motions, page(false), 1.0);
    // The box is at rest after 0.2 seconds; what went is drawn for 0.6.
    let laid = lay_tall(&mut motions, page(false), 1.4);
    assert_eq!(at(&laid, BODY).h, 0.0);
    assert!(!node_of(&laid, BODY).clips, "it is at rest");
    assert!(laid.nodes.len() > 4, "what went is still drawn");
    for ghost in laid.nodes.iter().skip(4) {
        assert!(
            ghost.clip.rect.h <= 0.5,
            "cut by the closed box: {:?}",
            ghost.clip
        );
    }
    assert_eq!(lay_tall(&mut motions, page(false), 1.8).nodes.len(), 4);
}

#[test]
fn the_size_of_words_on_its_way_is_in_half_points_so_that_no_font_is_opened_for_each_frame() {
    let page = |size: f32| {
        div().col().child(
            div()
                .id(BOX)
                .row()
                .text_size(size)
                .motion(Motion::new(0.3).text_size())
                .child(text("hellowor").id(Id::new(3))),
        )
    };
    let mut motions = Motions::default();
    lay(&mut motions, page(20.0), 0.0);
    lay(&mut motions, page(30.0), 1.0);
    let (mut now, mut sizes) = (1.0, Vec::new());
    while motions.moving() {
        now += FRAME / 3.0;
        let laid = lay(&mut motions, page(30.0), now);
        // Eight letters, half the size each: a size in half points is a width in 2 (laid out in
        // whole pixels, a size that is not would give an odd width as often as an even one).
        sizes.push(laid.bounds(Id::new(3)).unwrap().w / 2.0);
        assert!(now < 3.0);
    }
    assert!(sizes.len() > 20, "{}", sizes.len());
    for size in &sizes {
        assert!(
            (size.round() - size).abs() < 1e-3,
            "{size} is not whole half points"
        );
    }
    assert!(
        sizes.iter().any(|size| *size > 40.0 && *size < 58.0),
        "it was seen on its way"
    );
}

#[test]
fn what_the_host_reads_of_a_page_is_where_the_page_put_a_box_and_not_where_a_loop_has_it() {
    let page =
        || {
            div().col().child(
                div().id(BOX).w(20.0).h(20.0).motion(
                    Motion::new(0.2).cycle(Cycle::new(1.0).ease(Ease::Linear).by(0.0, 10.0)),
                ),
            )
        };
    let mut motions = Motions::default();
    assert!(lay(&mut motions, page(), 0.0).without_loops().is_none());
    let moved = lay(&mut motions, page(), 0.25);
    assert!(moved.bounds(BOX).unwrap().y > 1.0, "the loop moved it");
    let rest = moved.without_loops().expect("a loop moved it");
    assert_eq!(rest.bounds(BOX).unwrap().y, 0.0);
    assert!(rest.without_loops().is_none());
}

#[test]
fn a_box_that_stops_following_its_laid_size_and_follows_it_again_goes_on_from_where_it_is() {
    let mut motions = Motions::default();
    lay_tall(&mut motions, accordion(true, body_motion(), false), 0.0);
    // It stops following (the body is 80 points while it does, and nothing while it does not),
    // and is not on the way anywhere.
    let apart = lay_tall(&mut motions, accordion(false, Motion::new(0.2), false), 1.0);
    assert_eq!(at(&apart, BODY).h, 0.0);
    assert!(!motions.moving());
    // Following again, it is at the size it is at: what it followed before is not a size it
    // walks from.
    let again = lay_tall(&mut motions, accordion(false, body_motion(), false), 2.0);
    assert_eq!(
        at(&again, BODY).h,
        0.0,
        "it does not go back to what it was"
    );
    assert!(!motions.moving());
    let later = lay_tall(&mut motions, accordion(false, body_motion(), false), 2.05);
    assert_eq!(at(&later, BODY).h, 0.0);
    assert_eq!(at(&later, FOOT).y, 20.0);
}

#[test]
fn what_goes_out_while_it_spins_and_loops_fades_as_turned_and_as_moved_as_it_left() {
    let motion = Motion::new(0.2)
        .spin(2.0)
        .cycle(Cycle::new(2.0).ease(Ease::Linear).by(20.0, 0.0))
        .exit(Shift::fade(0.5));
    let page = |here: bool| {
        let holder = div().id(LIST);
        if here {
            holder.child(
                image("a.png", Fit::Contain)
                    .id(BOX)
                    .size(16.0, 16.0)
                    .motion(motion),
            )
        } else {
            holder
        }
    };
    let mut motions = Motions::default();
    lay(&mut motions, page(true), 0.0);
    // A quarter of the spin, and half of the way of the loop (it goes there and back).
    let there = lay(&mut motions, page(true), 0.5);
    assert_eq!(there.nodes[1].turn, 90.0);
    assert_eq!(there.nodes[1].rect.x, 10.0);
    for now in [0.5, 0.55, 0.6, 0.7] {
        let laid = lay(&mut motions, page(false), now);
        let ghost = laid.nodes.last().expect("it is going");
        assert_eq!(laid.nodes.len(), 2, "at {now}");
        assert_eq!(ghost.turn, 90.0, "at {now}: it goes as turned as it left");
        assert_eq!(ghost.rect.x, 10.0, "at {now}: and where it was");
        assert!(
            laid.without_loops().is_none(),
            "at {now}: what has gone does not loop on"
        );
    }
}

#[test]
fn what_goes_out_of_what_goes_out_leaves_the_ones_that_went_before_it_where_they_were() {
    let fade = Motion::new(0.2).exit(Shift::fade(0.5));
    let page = |outer: bool, inner: bool| {
        let holder = div().id(LIST);
        if !outer {
            return holder;
        }
        let first = div().id(BODY).motion(fade);
        holder.child(if inner {
            first.child(div().id(HEAD).h(10.0).motion(fade))
        } else {
            first
        })
    };
    let mut motions = Motions::default();
    lay(&mut motions, page(true, true), 0.0);
    // The inner one goes: its going is a part of the page for now.
    let inner = lay(&mut motions, page(true, false), 1.0);
    assert_eq!(inner.nodes.len(), 3);
    // The outer one goes after it: it is what it was when it left, and not with what went before.
    let outer = lay(&mut motions, page(false, false), 1.05);
    assert_eq!(outer.nodes.len(), 2);
}

/// A box of 40 by 20 points, its fill as `on` says (solid or none at all) and a spring to follow.
fn tint(on: bool) -> Element {
    div().row().child(
        div()
            .id(BOX)
            .w(40.0)
            .h(20.0)
            .no_shrink()
            .bg(if on {
                Role::Accent.solid()
            } else {
                Role::Accent.alpha(0.0)
            })
            .motion(Motion::new(0.5).color().ease(Ease::Spring).bounce(0.6)),
    )
}

/// `tint` as faint as `level` says.
fn tint_to(level: f32) -> Element {
    div().row().child(
        div()
            .id(BOX)
            .w(40.0)
            .h(20.0)
            .no_shrink()
            .bg(Role::Accent.alpha(level))
            .motion(Motion::new(0.5).color().ease(Ease::Spring).bounce(0.6)),
    )
}

fn alpha_of(laid: &Laid) -> f32 {
    node_of(laid, BOX).fill.expect("a fill").alpha
}

#[test]
fn a_spring_colour_sent_back_in_flight_goes_on_at_its_speed_and_does_not_leave_what_it_was_between()
{
    let mut motions = Motions::default();
    lay(&mut motions, tint(false), 0.0);
    lay(&mut motions, tint(true), 1.0);
    let on_the_way = alpha_of(&lay(&mut motions, tint(true), 1.06));
    assert!(on_the_way > 0.2 && on_the_way < 0.8, "{on_the_way}");
    // Sent back where it came from, at this very time: it was going to solid, and it does not
    // turn at once: it stays where it is, as a colour cannot be past it, till the spring has
    // turned, and it is not more of the colour than it was.
    let mut sent = motions.clone();
    let at = alpha_of(&lay(&mut sent, tint(false), 1.06));
    assert!((at - on_the_way).abs() < 0.01, "{on_the_way} then {at}");
    let mut now = 1.06;
    while sent.moving() {
        now += FRAME;
        assert!(now < 5.0, "it never comes to rest");
        let alpha = alpha_of(&lay(&mut sent, tint(false), now));
        assert!((0.0..=at + 0.005).contains(&alpha), "{alpha} at {now}");
        // The first frames after it was sent: it has not dropped to where it goes.
        if now < 1.06 + 4.0 * FRAME {
            assert!(
                alpha > at - 0.15,
                "its speed is kept: {at} then {alpha} at {now}"
            );
        }
    }
    assert_eq!(alpha_of(&lay(&mut sent, tint(false), 9.0)), 0.0);
}

#[test]
fn a_spring_colour_sent_on_in_flight_goes_at_the_speed_it_had_whatever_the_way_it_has_now() {
    // Going to 0.2 of the colour, and sent on to 1.0 of it (a longer way) or to 0.7 (a shorter
    // one) at the same time: the colour changes at the pace it did, and not as many of the way.
    for (first, then) in [(0.2_f32, 1.0_f32), (1.0, 0.7)] {
        let mut motions = Motions::default();
        lay(&mut motions, tint_to(0.0), 0.0);
        lay(&mut motions, tint_to(first), 1.0);
        let by = |motions: &mut Motions, level: f32, now: f64| {
            alpha_of(&lay(motions, tint_to(level), now))
        };
        let (mut goes_on, mut sent) = (motions.clone(), motions);
        let at = by(&mut goes_on, first, 1.06);
        let step = by(&mut goes_on, first, 1.062) - at;
        assert!(step > 0.0005, "it is on the way: {step}");
        assert!((by(&mut sent, then, 1.06) - at).abs() < 0.001);
        let after = by(&mut sent, then, 1.062) - at;
        assert!(
            (after / step - 1.0).abs() < 0.25,
            "{first} then {then}: it goes by {after} where it went by {step}"
        );
    }
}

#[test]
fn a_turn_and_a_scale_that_are_not_what_they_can_be_are_held_as_the_layout_holds_them() {
    let page = |turn: f32, scale: f32| {
        div().child(
            image("a.png", Fit::Contain)
                .id(BOX)
                .size(16.0, 16.0)
                .turned(turn)
                .scaled(scale)
                .motion(Motion::new(0.2).transform()),
        )
    };
    let sees = |turn: f32, scale: f32| {
        let laid = Motions::default().layout(page(turn, scale), PAGE, 1.0, &mut Fixed, 0.0, false);
        (laid.nodes[1].turn, laid.nodes[1].scaled)
    };
    assert_eq!(sees(30.0, -1.0), (30.0, 0.0), "no less than nothing");
    assert_eq!(sees(f32::NAN, f32::NAN), (0.0, 1.0));
    assert_eq!(sees(f32::INFINITY, f32::NEG_INFINITY), (0.0, 1.0));
    // On its way to one it cannot be, it is not on the way to it for ever, and is not drawn
    // as less than nothing.
    let mut motions = Motions::default();
    lay(&mut motions, page(0.0, 1.0), 0.0);
    lay(&mut motions, page(0.0, -3.0), 1.0);
    let mut now = 1.0;
    while motions.moving() {
        now += FRAME;
        let laid = lay(&mut motions, page(0.0, -3.0), now);
        let scaled = laid.nodes[1].scaled;
        assert!((0.0..=1.0).contains(&scaled), "{scaled} at {now}");
        assert!(now < 3.0, "it never comes to rest");
    }
    assert_eq!(lay(&mut motions, page(0.0, -3.0), 9.0).nodes[1].scaled, 0.0);
    let mut motions = Motions::default();
    lay(&mut motions, page(f32::NAN, f32::NAN), 0.0);
    lay(&mut motions, page(f32::NAN, f32::NAN), 0.1);
    assert!(!motions.moving(), "a number that is none is not a change");
}
