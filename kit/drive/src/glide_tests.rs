//! The guards of a caret's glide: the three motions over the frames' time, what a spring does not do
//! (overshoot, ask for frames for ever) and does (keep its speed where it is sent on), and every
//! case where a caret is at its place at once.

use pencere_editor::CaretSpots;
use pencere_view::edit::CaretMotion;
use pencere_view::{Id, Point, Rect};

use crate::glide::Glide;

const EDITOR: Id = Id::new(1);
/// A row is 20 pixels, the room 400 by 200.
const ROW: f32 = 20.0;

fn spots(places: &[usize], x: &[f32], motion: CaretMotion) -> CaretSpots {
    CaretSpots {
        editor: EDITOR,
        places: places.to_vec(),
        at: x.iter().map(|x| Some(Point::new(*x, 24.0))).collect(),
        room: Rect::new(0.0, 0.0, 400.0, 200.0),
        row: ROW,
        interacting: false,
        motion,
    }
}

/// The caret at `place`, `x` across, in the first frame (at rest there), and then moved to `to` at
/// `now`.
fn moved(motion: CaretMotion, now: f64) -> (Glide, Vec<Point>) {
    let mut glide = Glide::default();
    glide.shifts(&spots(&[0], &[4.0], motion), now, false);
    let shifts = glide.shifts(&spots(&[5], &[54.0], motion), now, false);
    (glide, shifts)
}

fn x(shifts: &[Point]) -> f32 {
    shifts.first().map_or(f32::NAN, |shift| shift.x)
}

#[test]
fn a_caret_that_snaps_is_at_its_place_at_once_and_asks_for_no_frame() {
    let (glide, shifts) = moved(CaretMotion::Snap, 1.0);
    assert_eq!(shifts, vec![Point::new(0.0, 0.0)]);
    assert!(!glide.moving());
}

#[test]
fn an_ease_starts_where_the_caret_was_is_between_half_way_through_and_is_there_when_it_ends() {
    let motion = CaretMotion::Ease(0.18);
    let (mut glide, shifts) = moved(motion, 1.0);
    // Drawn where it was, 50 pixels behind its place.
    assert_eq!(x(&shifts), -50.0);
    assert!(glide.moving());
    // Half the time: the curve is a long way on (1 - 0.5^3 = 0.875 of it).
    let half = glide.shifts(&spots(&[5], &[54.0], motion), 1.09, false);
    assert!((x(&half) - (-50.0 * 0.125)).abs() < 1e-3, "{half:?}");
    // The time's end: at its place, and no more frames.
    let end = glide.shifts(&spots(&[5], &[54.0], motion), 1.18, false);
    assert_eq!(end, vec![Point::new(0.0, 0.0)]);
    assert!(!glide.moving());
}

#[test]
fn an_ease_takes_the_time_the_page_says_whatever_the_way() {
    for seconds in [0.05, 0.5] {
        let motion = CaretMotion::Ease(seconds);
        let (mut glide, _) = moved(motion, 0.0);
        let before = glide.shifts(
            &spots(&[5], &[54.0], motion),
            f64::from(seconds) * 0.9,
            false,
        );
        assert!(glide.moving() && x(&before) < 0.0);
        glide.shifts(&spots(&[5], &[54.0], motion), f64::from(seconds), false);
        assert!(!glide.moving());
    }
}

#[test]
fn a_spring_goes_to_its_place_with_no_overshoot_and_settles_inside_the_belt() {
    let motion = CaretMotion::Spring(30.0);
    let (mut glide, shifts) = moved(motion, 0.0);
    assert!(x(&shifts) < 0.0, "behind its place: {shifts:?}");
    let mut now = 0.0;
    let mut last = x(&shifts);
    let mut frames = 0;
    while glide.moving() {
        now += 1.0 / 60.0;
        frames += 1;
        let at = glide.shifts(&spots(&[5], &[54.0], motion), now, false);
        // Never past its place, and only nearer.
        assert!(x(&at) <= 0.0 && x(&at) >= last - 1e-3, "{at:?}");
        last = x(&at);
        assert!(now <= 0.7 + 0.1, "still going at {now}");
        assert!(frames < 200);
    }
    // Settled: a quarter of a second or so for a caret that went 50 pixels.
    assert!((0.15..0.7).contains(&now), "{now}");
    assert_eq!(last, 0.0);
}

#[test]
fn a_softer_spring_is_let_the_longer_to_settle() {
    let motion = CaretMotion::Spring(8.0);
    let (mut glide, _) = moved(motion, 0.0);
    let mut now = 0.0;
    while glide.moving() {
        now += 1.0 / 60.0;
        glide.shifts(&spots(&[5], &[54.0], motion), now, false);
        assert!(now < 3.0);
    }
    assert!(
        now > 0.7,
        "it took {now}: more than a terminal's belt, which would have cut it"
    );
}

#[test]
fn a_spring_sent_on_in_flight_keeps_its_speed_as_a_key_held_down_does() {
    let motion = CaretMotion::Spring(30.0);
    let frame = 1.0 / 60.0;
    // A caret sent far, for a tenth of a second, and then sent 10 pixels further: where it is drawn
    // after the next frame.
    let mut flying = Glide::default();
    flying.shifts(&spots(&[0], &[4.0], motion), 0.0, false);
    let mut now = 0.0;
    let mut drawn = 4.0;
    for _ in 0..6 {
        now += frame;
        let shifts = flying.shifts(&spots(&[1], &[204.0], motion), now, false);
        drawn = 204.0 + x(&shifts);
    }
    assert!(drawn > 4.0 + 10.0, "it is on its way: {drawn}");
    now += frame;
    let next = 214.0 + x(&flying.shifts(&spots(&[2], &[214.0], motion), now, false));
    // A caret at rest where it was drawn, sent to the same place, goes less far in a frame.
    let mut resting = Glide::default();
    resting.shifts(&spots(&[0], &[drawn], motion), 0.0, false);
    let from_rest = 214.0 + x(&resting.shifts(&spots(&[2], &[214.0], motion), frame, false));
    assert!(
        next - drawn > from_rest - drawn,
        "a caret that was going goes on faster: {} {}",
        next - drawn,
        from_rest - drawn
    );
}

#[test]
fn a_caret_is_at_its_place_at_once_with_less_motion_with_another_count_out_of_the_room_and_so_on() {
    let motion = CaretMotion::Ease(0.18);
    let shifted = |glide: &mut Glide, spots: &CaretSpots, still: bool| {
        glide
            .shifts(spots, 1.0, still)
            .iter()
            .all(|p| *p == Point::default())
    };
    let start = |glide: &mut Glide| {
        glide.shifts(&spots(&[0], &[4.0], motion), 0.0, false);
    };
    // Reduce Motion.
    let mut glide = Glide::default();
    start(&mut glide);
    assert!(shifted(&mut glide, &spots(&[5], &[54.0], motion), true));
    assert!(!glide.moving());
    // Another number of carets.
    let mut glide = Glide::default();
    start(&mut glide);
    assert!(shifted(
        &mut glide,
        &spots(&[5, 9], &[54.0, 90.0], motion),
        false
    ));
    assert!(!glide.moving());
    // The new place out of the room (the room scrolls to it), and the old.
    let mut glide = Glide::default();
    start(&mut glide);
    let mut away = spots(&[500], &[54.0], motion);
    away.at = vec![Some(Point::new(54.0, 900.0))];
    assert!(shifted(&mut glide, &away, false));
    assert!(!glide.moving());
    let mut glide = Glide::default();
    glide.shifts(&away, 0.0, false);
    assert!(shifted(&mut glide, &spots(&[5], &[54.0], motion), false));
    // The layout moved it and its place is the same (a size, a fold, a wrapping).
    let mut glide = Glide::default();
    start(&mut glide);
    assert!(shifted(&mut glide, &spots(&[0], &[90.0], motion), false));
    assert!(!glide.moving());
    // The pointer selecting, an input method composing.
    let mut glide = Glide::default();
    start(&mut glide);
    let mut busy = spots(&[5], &[54.0], motion);
    busy.interacting = true;
    assert!(shifted(&mut glide, &busy, false));
    assert!(!glide.moving());
    // Another editor has the keyboard.
    let mut glide = Glide::default();
    start(&mut glide);
    let mut other = spots(&[5], &[54.0], motion);
    other.editor = Id::new(2);
    assert!(shifted(&mut glide, &other, false));
    // A caret on no line that shows has nowhere to be drawn from or to.
    let mut glide = Glide::default();
    start(&mut glide);
    let mut folded = spots(&[5], &[54.0], motion);
    folded.at = vec![None];
    assert!(shifted(&mut glide, &folded, false));
    assert!(shifted(&mut glide, &spots(&[9], &[90.0], motion), false));
}

#[test]
fn a_glide_the_editor_stops_asking_about_is_at_rest_and_asks_for_no_frame() {
    let motion = CaretMotion::Ease(0.18);
    let (mut glide, _) = moved(motion, 1.0);
    assert!(glide.moving());
    // A frame in which no editor said where its carets are (the keyboard went to the page).
    glide.drawn();
    assert!(glide.moving(), "this frame did ask");
    glide.drawn();
    assert!(!glide.moving(), "the next did not");
    // And the editor that gets the keyboard back starts from where it is.
    let back = glide.shifts(&spots(&[5], &[54.0], motion), 2.0, false);
    assert_eq!(back, vec![Point::default()]);
}

#[test]
fn scrolling_does_not_make_a_caret_glide_and_a_caret_that_glides_stays_in_the_text() {
    let motion = CaretMotion::Ease(0.18);
    let (mut glide, _) = moved(motion, 1.0);
    // The room is scrolled 30 pixels down while the caret glides: the content's coordinates are
    // what the glide goes by, so the shift is what it is without the scroll.
    let mut scrolled = spots(&[5], &[54.0], motion);
    scrolled.room = Rect::new(0.0, 30.0, 400.0, 200.0);
    let shifts = glide.shifts(&scrolled, 1.09, false);
    assert!((x(&shifts) - (-50.0 * 0.125)).abs() < 1e-3, "{shifts:?}");
    // And a scroll alone, the place and the box as they were, moves nothing.
    let mut still = Glide::default();
    still.shifts(&spots(&[3], &[40.0], motion), 0.0, false);
    scrolled.places = vec![3];
    scrolled.at = vec![Some(Point::new(40.0, 24.0))];
    assert_eq!(still.shifts(&scrolled, 0.1, false), vec![Point::default()]);
    assert!(!still.moving());
}

#[test]
fn a_frame_a_long_time_after_does_not_step_a_whole_glide_at_once() {
    let motion = CaretMotion::Spring(30.0);
    let (mut glide, _) = moved(motion, 0.0);
    // A view that was not seen for ten seconds: one step of at most a tenth of a second.
    let after = glide.shifts(&spots(&[5], &[54.0], motion), 10.0, false);
    assert!(glide.moving() || x(&after) == 0.0);
    assert!(
        x(&after) < 0.0,
        "not settled in one frame of ten seconds: {after:?}"
    );
}
