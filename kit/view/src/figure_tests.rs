//! The engine's guards for what a path carries: its transform, how a spin and an opacity act on the
//! fields the effects added, and the order a page's transform parts are put in.

use crate::{
    Blend, Brush, Cap, Join, Matrix, Measure, Metrics, Paint, Point, Rect, Role, Size, Stop,
    canvas, div, layout, path,
};

struct Fixed;

impl Measure for Fixed {
    fn measure(&mut self, font: &crate::Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().count() as f32 * font.size * 0.5,
            height: font.size * 1.25,
        }
    }
}

fn near(got: Point, want: (f32, f32)) {
    assert!(
        (got.x - want.0).abs() < 1e-4 && (got.y - want.1).abs() < 1e-4,
        "{got:?} is not {want:?}"
    );
}

/// A path with every field the effects added, so that what is done to one is seen on each.
fn every_field() -> crate::Path {
    path()
        .rect(0.0, 0.0, 10.0, 10.0)
        .fill_brush(Brush::linear(
            0.0,
            0.0,
            10.0,
            0.0,
            [Stop::new(0.0, Role::Accent), Stop::new(1.0, Role::Success)],
        ))
        .stroke_brush(
            2.0,
            Brush::radial(5.0, 5.0, 5.0, [Stop::new(0.5, Role::Accent)]),
        )
        .dash(vec![3.0, 1.0], 0.5)
        .cap(Cap::Butt)
        .join(Join::Miter)
        .blend(Blend::Add)
        .shadow(4.0, 0.0, 6.0, Role::Success)
        .transform(Matrix::from_parts(
            (2.0, 1.0),
            0.0,
            (10.0, 0.0),
            Point::new(0.0, 0.0),
        ))
}

#[test]
fn the_parts_of_a_transform_are_scaled_then_turned_both_about_the_origin_then_moved() {
    // Twice as wide, a quarter turn clockwise, then ten to the right: (1, 0) is (2, 0) and then
    // (0, 2), and then (10, 2). Turned and then scaled it would be (0, 1) and then (0, 1).
    let both = Matrix::from_parts((2.0, 1.0), 90.0, (10.0, 0.0), Point::new(0.0, 0.0));
    near(both.apply(Point::new(1.0, 0.0)), (10.0, 2.0));
    // About a point of its own: the point itself stays where it is.
    let about = Matrix::from_parts((3.0, 3.0), 90.0, (0.0, 0.0), Point::new(5.0, 5.0));
    near(about.apply(Point::new(5.0, 5.0)), (5.0, 5.0));
    near(about.apply(Point::new(6.0, 5.0)), (5.0, 8.0));
    assert_eq!(
        Matrix::from_parts((1.0, 1.0), 0.0, (0.0, 0.0), Point::new(7.0, 7.0)),
        Matrix::IDENTITY
    );
}

#[test]
fn a_path_turned_keeps_its_points_and_its_transform_goes_first_and_the_turn_after() {
    let moved = path()
        .rect(0.0, 0.0, 4.0, 4.0)
        .transform(Matrix::from_parts(
            (1.0, 1.0),
            0.0,
            (10.0, 0.0),
            Point::new(0.0, 0.0),
        ));
    let turned = moved.clone().turned(90.0, Point::new(0.0, 0.0));
    assert_eq!(turned.segments, moved.segments, "no point is moved");
    // The path's own move to (10, 0) and then the turn: (0, 10).
    near(turned.transform.apply(Point::new(0.0, 0.0)), (0.0, 10.0));
    near(turned.transform.apply(Point::new(4.0, 0.0)), (0.0, 14.0));
}

#[test]
fn a_spin_turns_the_offset_of_a_shadow_as_it_turns_the_picture() {
    let glow =
        path()
            .rect(0.0, 0.0, 4.0, 4.0)
            .fill(Role::Accent)
            .shadow(4.0, 0.0, 6.0, Role::Success);
    let turned = glow.turned(90.0, Point::new(10.0, 10.0));
    let shadow = turned.shadow.expect("a shadow");
    // To the right of the shape, then the picture a quarter clockwise: below it.
    assert!(
        shadow.dx.abs() < 1e-4 && (shadow.dy - 4.0).abs() < 1e-4,
        "{shadow:?}"
    );
    assert_eq!(shadow.blur, 6.0, "the blur is round, it does not turn");
}

#[test]
fn faded_and_turned_carry_every_field_the_effects_add() {
    let faint = every_field().faded(0.5);
    let Some(Brush::Linear { stops, .. }) = &faint.fill else {
        panic!("a gradient");
    };
    assert_eq!(stops[0].color.alpha, 0.5);
    assert_eq!(stops[1].color.alpha, 0.5);
    assert_eq!(stops[1].offset, 1.0);
    let stroke = faint.stroke.as_ref().expect("a line");
    let Brush::Radial { stops, radius, .. } = &stroke.brush else {
        panic!("a gradient");
    };
    assert_eq!((stops[0].color.alpha, *radius), (0.5, 5.0));
    assert_eq!(
        (
            stroke.width,
            stroke.dash.clone(),
            stroke.dash_offset,
            stroke.cap,
            stroke.join
        ),
        (2.0, vec![3.0, 1.0], 0.5, Cap::Butt, Join::Miter)
    );
    assert_eq!(faint.shadow.expect("a shadow").color.alpha, 0.5);
    assert_eq!(faint.blend, Blend::Add);
    assert_eq!(faint.transform, every_field().transform);

    let turned = every_field().turned(180.0, Point::new(5.0, 5.0));
    assert_eq!(turned.segments, every_field().segments);
    assert_eq!(turned.fill, every_field().fill);
    assert_eq!(turned.blend, Blend::Add);
    assert_eq!(turned.stroke, every_field().stroke);
    let shadow = turned.shadow.expect("a shadow");
    assert!(
        (shadow.dx + 4.0).abs() < 1e-4 && shadow.dy.abs() < 1e-4,
        "{shadow:?}"
    );
}

#[test]
fn a_canvas_that_fades_hands_the_host_its_paths_with_the_effects_as_faint_as_it_is() {
    let root = div().child(canvas([every_field()]).size(20.0, 20.0).opacity(0.5));
    let list = layout(root, Size::new(100.0, 100.0), 1.0, &mut Fixed).paint(&mut Fixed);
    let Some(Paint::Figure(figure)) = list.items.first() else {
        panic!("a figure");
    };
    assert_eq!(figure.rect, Rect::new(0.0, 0.0, 20.0, 20.0));
    assert_eq!(figure.paths[0], every_field().faded(0.5));
}

#[test]
fn a_canvas_that_is_scaled_and_turned_has_its_room_grown_about_its_middle_and_its_paths_posed() {
    // 40 points square at (10, 10), twice as large and a quarter turn clockwise: its room is 80 and
    // starts at (-10, -10), and (10, 0) of its paths, which is (20, 0) of the grown room, is a
    // quarter turn about the middle (40, 40) of it.
    let root = div().child(
        canvas([path().rect(0.0, 0.0, 10.0, 10.0)])
            .size(40.0, 40.0)
            .scaled(2.0)
            .turned(90.0),
    );
    let list = layout(root, Size::new(100.0, 100.0), 1.0, &mut Fixed).paint(&mut Fixed);
    let Some(Paint::Figure(figure)) = list.items.first() else {
        panic!("a figure");
    };
    assert_eq!(figure.rect, Rect::new(-20.0, -20.0, 80.0, 80.0));
    near(
        figure.paths[0].transform.apply(Point::new(10.0, 0.0)),
        (80.0, 20.0),
    );
    // Not turned and not scaled it is as it was, untouched.
    let root = div().child(canvas([every_field()]).size(20.0, 20.0));
    let list = layout(root, Size::new(100.0, 100.0), 1.0, &mut Fixed).paint(&mut Fixed);
    let Some(Paint::Figure(figure)) = list.items.first() else {
        panic!("a figure");
    };
    assert_eq!(figure.paths[0], every_field());
}
