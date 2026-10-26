//! The engine's guards: where boxes stand, what a line of text does when it does not fit,
//! and what the draw list holds. The host's measuring is a fixed-width stand-in.

use crate::{
    Align, Color, DrawList, Family, FieldKind, Font, Id, Measure, Metrics, Paint, Rect, Rgba, Role,
    Size, Source, Track, div, layout, spacer, spark, text,
};

/// Every character is `0.5 × size` wide and the line `1.25 × size` tall: round numbers.
struct Fixed;

impl Measure for Fixed {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().count() as f32 * font.size * 0.5,
            height: font.size * 1.25,
        }
    }
}

fn rects(root: crate::Element, w: f32, h: f32, scale: f32) -> Vec<Rect> {
    layout(root, Size::new(w * scale, h * scale), scale, &mut Fixed)
        .rects()
        .collect()
}

fn paint(root: crate::Element, w: f32, h: f32, scale: f32) -> DrawList {
    layout(root, Size::new(w * scale, h * scale), scale, &mut Fixed).paint(&mut Fixed)
}

fn runs(list: &DrawList) -> Vec<&crate::Run> {
    list.items
        .iter()
        .filter_map(|item| match item {
            Paint::Text(run) => Some(run),
            Paint::Quad(_) | Paint::Image(_) => None,
        })
        .collect()
}

fn quads(list: &DrawList) -> Vec<&crate::Quad> {
    list.items
        .iter()
        .filter_map(|item| match item {
            Paint::Quad(quad) => Some(quad),
            Paint::Text(_) | Paint::Image(_) => None,
        })
        .collect()
}

#[test]
fn a_row_puts_its_children_side_by_side_inside_its_padding() {
    let root = div()
        .row()
        .p(10.0)
        .gap(4.0)
        .child(div().size(20.0, 30.0))
        .child(div().size(40.0, 10.0));
    let boxes = rects(root, 200.0, 100.0, 1.0);
    assert_eq!(boxes[0], Rect::new(0.0, 0.0, 200.0, 100.0));
    assert_eq!(boxes[1], Rect::new(10.0, 10.0, 20.0, 30.0));
    assert_eq!(boxes[2], Rect::new(34.0, 10.0, 40.0, 10.0));
}

#[test]
fn a_column_is_the_default_and_stretches_its_children() {
    let root = div().child(div().h(10.0)).child(div().h(20.0));
    let boxes = rects(root, 80.0, 100.0, 1.0);
    assert_eq!(boxes[1], Rect::new(0.0, 0.0, 80.0, 10.0));
    assert_eq!(boxes[2], Rect::new(0.0, 10.0, 80.0, 20.0));
}

#[test]
fn a_spacer_pushes_what_follows_it_to_the_end() {
    let root = div()
        .row()
        .child(div().size(10.0, 10.0))
        .child(spacer())
        .child(div().size(10.0, 10.0));
    let boxes = rects(root, 100.0, 10.0, 1.0);
    assert_eq!(boxes[1].x, 0.0);
    assert_eq!(boxes[3].x, 90.0);
}

#[test]
fn items_are_centred_on_the_cross_axis() {
    let root = div().row().items_center().child(div().size(10.0, 20.0));
    let boxes = rects(root, 100.0, 100.0, 1.0);
    assert_eq!(boxes[1], Rect::new(0.0, 40.0, 10.0, 20.0));
}

#[test]
fn a_grid_shares_what_the_fixed_tracks_leave_by_fr_and_keeps_its_minimum() {
    let cell = || div().h(10.0);
    let root = div()
        .grid([
            Track::px(20.0),
            Track::range(30.0, 1.0),
            Track::range(30.0, 3.0),
        ])
        .gap_x(10.0)
        .child(cell())
        .child(cell())
        .child(cell());
    // 200 − 20 − 2×10 = 160, shared 1:3.
    let boxes = rects(root, 200.0, 50.0, 1.0);
    assert_eq!(boxes[1], Rect::new(0.0, 0.0, 20.0, 10.0));
    assert_eq!(boxes[2], Rect::new(30.0, 0.0, 40.0, 10.0));
    assert_eq!(boxes[3], Rect::new(80.0, 0.0, 120.0, 10.0));

    // Too narrow for the shares: each range keeps its minimum.
    let root = div()
        .grid([
            Track::px(20.0),
            Track::range(30.0, 1.0),
            Track::range(30.0, 3.0),
        ])
        .gap_x(10.0)
        .child(cell())
        .child(cell())
        .child(cell());
    let boxes = rects(root, 100.0, 50.0, 1.0);
    assert_eq!(boxes[2].w, 30.0);
    assert_eq!(boxes[3].w, 30.0);
}

#[test]
fn a_page_written_in_points_is_laid_out_in_pixels() {
    let root = div().row().p(10.0).child(
        div()
            .size(20.0, 30.0)
            .rounded(4.0)
            .border(1.0, Role::Border),
    );
    let list = {
        let laid = layout(root, Size::new(400.0, 200.0), 2.0, &mut Fixed);
        assert_eq!(laid.rects().nth(1), Some(Rect::new(20.0, 20.0, 40.0, 60.0)));
        laid.paint(&mut Fixed)
    };
    let quad = quads(&list)[0];
    assert_eq!(quad.radii, [8.0; 4]);
    assert_eq!(quad.border.map(|b| b.width), Some(2.0));
}

#[test]
fn text_takes_what_the_host_says_and_inherits_the_boxes_around_it() {
    let root = div()
        .text_size(10.0)
        .weight(600)
        .color(Role::Dim)
        .family(Family::System)
        .child(text("abcd"))
        .child(text("ef").text_size(20.0).weight(400));
    let list = paint(root, 100.0, 100.0, 2.0);
    let runs = runs(&list);
    assert_eq!(runs.len(), 2);
    // 10 pt at 2 px a point.
    assert_eq!(runs[0].font.size, 20.0);
    assert_eq!(runs[0].font.weight, 600);
    assert_eq!(runs[0].font.family, Family::System);
    assert_eq!(runs[0].color.role(), Some(Role::Dim));
    assert_eq!(runs[1].font.size, 40.0);
    assert_eq!(runs[1].font.weight, 400);
    assert_eq!(
        runs[1].color.role(),
        Some(Role::Dim),
        "the colour is still the box's"
    );
    // The second line sits under the first, whose box is 1.25 × 20 px tall.
    assert_eq!(runs[0].origin.y, 0.0);
    assert_eq!(runs[1].origin.y, 25.0);
}

#[test]
fn a_line_that_may_be_cut_ends_in_an_ellipsis_where_it_does_not_fit() {
    // 12.5 pt at one px a point: 6.25 px a character, a 50 px box fits 8 characters.
    let root = div()
        .row()
        .child(text("abcdefghijklmnop").truncate().w(50.0));
    let list = paint(root, 200.0, 20.0, 1.0);
    let run = runs(&list)[0];
    assert_eq!(run.text, "abcdefg\u{2026}");
    assert_eq!(run.clip.w, 50.0, "and it stays in its box");
}

#[test]
fn a_line_that_fits_is_left_whole_and_one_that_may_not_be_cut_is_not() {
    let root = div()
        .row()
        .child(text("abc").truncate().w(50.0))
        .child(text("abcdefghijklmnop").w(50.0));
    let list = paint(root, 200.0, 20.0, 1.0);
    let runs = runs(&list);
    assert_eq!(runs[0].text, "abc");
    assert_eq!(runs[1].text, "abcdefghijklmnop");
}

#[test]
fn a_line_that_may_be_cut_gives_its_room_away_before_the_others() {
    let root = div()
        .row()
        .w(100.0)
        .child(text("a very long name indeed").truncate())
        .child(div().w(40.0).no_shrink());
    let boxes = rects(root, 100.0, 20.0, 1.0);
    assert_eq!(boxes[1].w, 60.0, "what the fixed box leaves");
    assert_eq!(boxes[2].w, 40.0);
}

#[test]
fn text_that_wraps_goes_on_in_new_lines_at_its_spaces_and_its_box_is_as_tall_as_they_are() {
    // 6.25 px a character, 15.625 px a line: a 100 px box fits 16 characters.
    let root = div()
        .w(100.0)
        .child(text("zero running · six cpu · 11.7GiB").wrap());
    let laid = layout(root, Size::new(100.0, 200.0), 1.0, &mut Fixed);
    let boxes: Vec<Rect> = laid.rects().collect();
    // Taffy rounds the boxes to whole pixels.
    assert_eq!(boxes[1].h, (15.625_f32 * 3.0).round(), "three lines");
    let list = laid.paint(&mut Fixed);
    let runs = runs(&list);
    let lines: Vec<&str> = runs.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(lines, ["zero running ·", "six cpu ·", "11.7GiB"]);
    assert_eq!(
        runs[1].origin.y - runs[0].origin.y,
        15.625,
        "one line under the other"
    );
    assert_eq!(runs[2].origin.y - runs[1].origin.y, 15.625);
    assert!(runs.iter().all(|run| run.origin.x == 0.0));
}

#[test]
fn text_that_wraps_breaks_a_word_longer_than_its_box_and_keeps_its_line_breaks() {
    let root = div().w(25.0).child(text("abcdefghij\nab").wrap());
    let list = paint(root, 25.0, 200.0, 1.0);
    let lines: Vec<String> = runs(&list).iter().map(|run| run.text.clone()).collect();
    assert_eq!(lines, ["abcd", "efgh", "ij", "ab"]);
}

#[test]
fn text_that_wraps_is_one_line_where_it_fits_and_stands_where_it_is_aligned() {
    let root = div()
        .w(100.0)
        .child(text("abcd").wrap().align(Align::End))
        .child(text("abcd efgh ijkl mnop qrst").wrap().align(Align::Center));
    let laid = layout(root, Size::new(100.0, 200.0), 1.0, &mut Fixed);
    let boxes: Vec<Rect> = laid.rects().collect();
    assert_eq!(boxes[1].h, 15.625_f32.round(), "one line");
    let list = laid.paint(&mut Fixed);
    let runs = runs(&list);
    assert_eq!(runs[0].origin.x, 75.0);
    // "abcd efgh ijkl" is 14 characters, 87.5 px, and "mnop qrst" 9, 56.25 px.
    assert_eq!(runs[1].text, "abcd efgh ijkl");
    assert_eq!(runs[1].origin.x, 6.25);
    assert_eq!(runs[2].origin.x, 21.875);
}

#[test]
fn text_that_wraps_in_a_row_gives_room_down_to_its_longest_word() {
    let root = div()
        .row()
        .items_start()
        .w(100.0)
        .child(text("aaaa bb cc").wrap())
        .child(div().w(80.0).no_shrink());
    let boxes = rects(root, 100.0, 100.0, 1.0);
    assert_eq!(boxes[1].w, 25.0, "the longest word");
    assert_eq!(boxes[1].h, (15.625_f32 * 3.0).round(), "aaaa / bb / cc");
}

#[test]
fn the_text_in_the_room_of_the_hosts_field_is_its_hint_and_the_page_does_not_draw_it() {
    let root = div().row().child(text("before")).child(
        div()
            .text_field(Id::new(1))
            .w(120.0)
            .h(18.0)
            .child(text("Filter images")),
    );
    let laid = layout(root, Size::new(200.0, 20.0), 1.0, &mut Fixed);
    let fields = laid.fields();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].hint.as_deref(), Some("Filter images"));
    let list = laid.paint(&mut Fixed);
    let drawn: Vec<&str> = runs(&list).iter().map(|run| run.text.as_str()).collect();
    assert_eq!(drawn, ["before"]);
}

#[test]
fn a_field_with_no_text_in_its_room_has_no_hint() {
    let root = div()
        .child(text("outside"))
        .child(div().text_field(Id::new(1)).w(120.0).h(18.0));
    let laid = layout(root, Size::new(200.0, 40.0), 1.0, &mut Fixed);
    let fields = laid.fields();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].hint, None);
}

#[test]
fn a_page_has_as_many_fields_as_it_names_each_of_its_kind_and_where_it_stands() {
    let root = div()
        .gap(4.0)
        .child(
            div()
                .field(Id::new(1), FieldKind::Line)
                .h(18.0)
                .child(text("Name")),
        )
        .child(
            div()
                .field(Id::new(2), FieldKind::Secret)
                .label("Password")
                .h(18.0),
        )
        .child(div().field(Id::new(3), FieldKind::Lines).h(60.0).hidden())
        .child(div().h(18.0).child(text("not a field")));
    let laid = layout(root, Size::new(200.0, 200.0), 2.0, &mut Fixed);
    let fields = laid.fields();
    assert_eq!(fields.len(), 2, "a hidden field is not one");
    assert_eq!(
        (fields[0].id, fields[0].kind),
        (Id::new(1), FieldKind::Line)
    );
    assert_eq!(fields[0].hint.as_deref(), Some("Name"));
    assert_eq!(
        (fields[1].id, fields[1].kind),
        (Id::new(2), FieldKind::Secret)
    );
    assert_eq!(fields[1].label.as_deref(), Some("Password"));
    assert_eq!(
        fields[1].rect,
        Rect::new(0.0, 22.0, 100.0, 18.0),
        "in points"
    );
}

#[test]
fn nothing_of_a_line_is_drawn_where_not_even_the_ellipsis_fits() {
    let root = div()
        .row()
        .child(text("abcdefghijklmnop").truncate().w(3.0));
    let list = paint(root, 200.0, 20.0, 1.0);
    assert!(runs(&list).is_empty());
}

#[test]
fn a_line_stands_at_the_end_or_the_middle_of_its_box() {
    let root = div()
        .w(100.0)
        .child(text("abcd").align(Align::End))
        .child(text("abcd").align(Align::Center))
        .child(text("abcd"));
    let list = paint(root, 100.0, 100.0, 1.0);
    let runs = runs(&list);
    // Four characters, 25 px.
    assert_eq!(runs[0].origin.x, 75.0);
    assert_eq!(runs[1].origin.x, 37.5);
    assert_eq!(runs[2].origin.x, 0.0);
}

#[test]
fn a_hidden_box_takes_no_room_and_draws_nothing_nor_do_its_children() {
    let root = div()
        .child(div().h(10.0).bg(Role::Accent))
        .child(
            div()
                .hidden()
                .h(10.0)
                .bg(Role::Error)
                .child(text("gone").color(Role::Error)),
        )
        .child(div().h(10.0).bg(Role::Success));
    let laid = layout(root, Size::new(50.0, 50.0), 1.0, &mut Fixed);
    // The root, the first box, the hidden one and its line, the last box.
    assert_eq!(laid.rects().nth(4), Some(Rect::new(0.0, 10.0, 50.0, 10.0)));
    let list = laid.paint(&mut Fixed);
    assert_eq!(quads(&list).len(), 2);
    assert!(runs(&list).is_empty());
}

#[test]
fn a_box_that_clips_cuts_what_is_drawn_inside_it() {
    let root = div().child(
        div()
            .size(30.0, 20.0)
            .clip()
            .child(div().size(100.0, 100.0).bg(Role::Accent)),
    );
    let list = paint(root, 100.0, 100.0, 1.0);
    let quad = quads(&list)[0];
    assert_eq!(quad.rect.w, 100.0, "the child is as big as it was written");
    assert_eq!(quad.clip, Rect::new(0.0, 0.0, 30.0, 20.0));
}

#[test]
fn a_scrolling_box_moves_its_content_behind_it() {
    let rows = |offset: f32| {
        div()
            .size(50.0, 40.0)
            .scroll_y(offset)
            .child(div().h(30.0).no_shrink().bg(Role::Accent))
            .child(div().h(30.0).no_shrink().bg(Role::Success))
            .child(div().h(30.0).no_shrink().bg(Role::Error))
    };
    let at = |offset: f32| {
        let list = paint(div().child(rows(offset)), 100.0, 100.0, 1.0);
        quads(&list).iter().map(|q| q.rect.y).collect::<Vec<_>>()
    };
    assert_eq!(at(0.0), [0.0, 30.0, 60.0]);
    assert_eq!(at(20.0), [-20.0, 10.0, 40.0]);
    let list = paint(div().child(rows(20.0)), 100.0, 100.0, 1.0);
    assert!(
        quads(&list)
            .iter()
            .all(|q| q.clip == Rect::new(0.0, 0.0, 50.0, 40.0)),
        "all of it is cut at the box"
    );
}

#[test]
fn an_absolute_box_is_placed_against_its_parent_and_takes_no_room() {
    let root = div()
        .p(8.0)
        .child(div().h(10.0))
        .child(
            div()
                .absolute()
                .size(20.0, 20.0)
                .inset(Some(0.0), Some(0.0), None, None),
        )
        .child(div().h(10.0));
    let boxes = rects(root, 100.0, 100.0, 1.0);
    assert_eq!(boxes[2], Rect::new(80.0, 0.0, 20.0, 20.0));
    assert_eq!(boxes[3], Rect::new(8.0, 18.0, 84.0, 10.0));
}

#[test]
fn a_sparkline_stays_in_its_box_and_ends_at_the_newest_value() {
    let values = [1.0, 4.0, 2.0, 8.0, 3.0, 5.0];
    let root = div().child(spark(&values, Role::Accent).size(60.0, 20.0));
    let list = paint(root, 100.0, 100.0, 1.0);
    let quads = quads(&list);
    assert!(!quads.is_empty());
    let right = quads.iter().map(|q| q.rect.right()).fold(0.0_f32, f32::max);
    assert!(
        (right - 60.0).abs() < 0.01,
        "the newest value is at the right edge"
    );
    for quad in &quads {
        assert!(quad.rect.x >= 0.0 && quad.rect.bottom() <= 21.0);
        assert_eq!(quad.clip, Rect::new(0.0, 0.0, 60.0, 20.0));
    }
}

#[test]
fn a_host_with_no_text_gets_the_boxes_and_no_words() {
    struct Mute;
    impl Measure for Mute {
        fn measure(&mut self, _: &Font, _: &str) -> Metrics {
            Metrics::default()
        }
    }
    let root = div()
        .row()
        .child(div().size(10.0, 10.0).bg(Role::Accent))
        .child(text("words"));
    let laid = layout(root, Size::new(100.0, 20.0), 1.0, &mut Mute);
    let list = laid.paint(&mut Mute);
    assert_eq!(quads(&list).len(), 1);
    assert!(runs(&list).is_empty());
}

// --- Colours -------------------------------------------------------------------------------

#[test]
fn the_ansi_palette_is_sixteen_roles_in_the_order_of_its_colours() {
    assert_eq!(Role::ANSI.len(), 16);
    for (at, role) in Role::ANSI.iter().enumerate() {
        assert_eq!(role.ansi_index(), Some(at as u8));
    }
    assert_eq!(Role::Black.ansi_index(), Some(0));
    assert_eq!(Role::BrightWhite.ansi_index(), Some(15));
    // The theme's own roles are not in the palette, not even the ones that share a colour with it.
    for role in [
        Role::Error,
        Role::Selection,
        Role::Foreground,
        Role::Raised(1),
    ] {
        assert_eq!(role.ansi_index(), None);
    }
}

#[test]
fn a_hex_value_is_six_or_eight_digits_after_a_hash_and_nothing_else() {
    assert_eq!(Rgba::parse("#1a2B3c"), Some(Rgba::rgb(0x1a, 0x2b, 0x3c)));
    assert_eq!(
        Rgba::parse("#1a2b3c80"),
        Some(Rgba::new(0x1a, 0x2b, 0x3c, 0x80))
    );
    for bad in [
        "",
        "#",
        "1a2b3c",
        "#123",
        "#1a2b3",
        "#1a2b3c8",
        "#1a2b3c800",
        "#gg0000",
        "# 1a2b3c",
        "#+1a2b3c",
        "#1a2b3é",
    ] {
        assert_eq!(Rgba::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn a_value_has_a_form_for_each_brightness_and_the_light_one_where_it_gave_one_only() {
    let light = Rgba::rgb(10, 20, 30);
    let dark = Rgba::rgb(200, 210, 220);
    let one = Source::Rgba { light, dark: None };
    let both = Source::Rgba {
        light,
        dark: Some(dark),
    };
    assert_eq!(one.value(false), Some(light));
    assert_eq!(one.value(true), Some(light));
    assert_eq!(both.value(false), Some(light));
    assert_eq!(both.value(true), Some(dark));
    assert_eq!(Source::Role(Role::Red).value(true), None);
}

#[test]
fn a_colour_is_opaque_when_every_form_of_it_covers_what_is_under_it() {
    assert!(Role::Raised(1).solid().is_opaque());
    assert!(!Role::Accent.alpha(0.99).is_opaque());
    assert!(Rgba::rgb(1, 2, 3).solid().is_opaque());
    assert!(!Rgba::new(1, 2, 3, 254).solid().is_opaque());
    assert!(Rgba::rgb(1, 2, 3).or_dark(Rgba::rgb(4, 5, 6)).is_opaque());
    // One translucent form is enough: the host may wear either.
    assert!(
        !Rgba::rgb(1, 2, 3)
            .or_dark(Rgba::new(4, 5, 6, 200))
            .is_opaque()
    );
    let half = Color {
        alpha: 0.5,
        ..Rgba::rgb(1, 2, 3).solid()
    };
    assert!(!half.is_opaque());
    assert!(!Role::Accent.alpha(f32::NAN).is_opaque());
}

fn grounds(root: crate::Element, line: &str) -> Vec<Color> {
    let list = paint(root, 100.0, 100.0, 1.0);
    let runs = runs(&list);
    let run = runs
        .iter()
        .find(|run| run.text == line)
        .expect("the line was drawn");
    run.grounds.clone()
}

#[test]
fn a_line_names_the_fills_under_it_nearest_first_and_stops_at_the_first_opaque_one() {
    let root = div().bg(Role::Background).child(
        div().bg(Role::Raised(1)).child(
            div()
                .bg(Role::Accent.alpha(0.2))
                .child(div().child(text("deep").bg(Rgba::new(9, 9, 9, 128).solid()))),
        ),
    );
    assert_eq!(
        grounds(root, "deep"),
        vec![
            Rgba::new(9, 9, 9, 128).solid(),
            Role::Accent.alpha(0.2),
            // The opaque card ends it: the page's background beyond it is not named.
            Role::Raised(1).solid(),
        ]
    );
}

#[test]
fn nested_translucent_fills_come_in_the_order_they_are_laid_over_each_other_outward() {
    let root = div().bg(Role::Foreground.alpha(0.1)).child(
        div()
            .bg(Role::Accent.alpha(0.2))
            .child(div().bg(Role::Error.alpha(0.3)).child(text("x"))),
    );
    // None is opaque: all three are named and the page's background is under the last.
    assert_eq!(
        grounds(root, "x"),
        vec![
            Role::Error.alpha(0.3),
            Role::Accent.alpha(0.2),
            Role::Foreground.alpha(0.1)
        ]
    );
}

#[test]
fn a_line_on_nothing_has_no_grounds_and_a_fill_with_nothing_drawn_is_not_one() {
    assert_eq!(
        grounds(div().child(text("bare")), "bare"),
        Vec::<Color>::new()
    );
    let root = div()
        .bg(Role::Raised(1))
        // No room: it is not drawn, and the line that spills out of it stands on the card.
        .child(
            div()
                .row()
                .size(0.0, 20.0)
                .bg(Role::Accent.alpha(0.5))
                .child(text("spill").no_shrink()),
        );
    assert_eq!(grounds(root, "spill"), vec![Role::Raised(1).solid()]);
}

#[test]
fn both_forms_of_a_value_reach_the_draw_list_unresolved() {
    let ink = Rgba::rgb(0x20, 0x30, 0x40).or_dark(Rgba::rgb(0xe0, 0xd0, 0xc0));
    let ground = Rgba::new(0x11, 0x22, 0x33, 0x44).or_dark(Rgba::rgb(0xaa, 0xbb, 0xcc));
    let root = div().bg(ground).child(text("hue").color(ink));
    let list = paint(root, 100.0, 100.0, 1.0);
    let run = runs(&list)[0].clone();
    assert_eq!(run.color, ink);
    assert_eq!(run.grounds, vec![ground]);
    assert_eq!(quads(&list)[0].fill, Some(ground));
}

#[test]
fn a_colour_taken_toward_another_is_mixed_in_srgb_before_its_opacity() {
    let value = |source: Source| match source {
        Source::Role(Role::Dim) => [0.2, 0.2, 0.2, 1.0],
        Source::Role(Role::Foreground) => [1.0, 1.0, 1.0, 1.0],
        _ => [0.0, 0.0, 0.0, 0.0],
    };
    let half = Role::Dim.alpha(0.5).toward(Role::Foreground, 0.25);
    let [r, g, b, a] = half.resolve(value);
    assert!((r - 0.4).abs() < 1e-6 && (g - 0.4).abs() < 1e-6 && (b - 0.4).abs() < 1e-6);
    assert!((a - 0.5).abs() < 1e-6, "the alpha is on top of the mix");
    assert_eq!(
        Role::Dim.solid().resolve(value),
        [0.2, 0.2, 0.2, 1.0],
        "no mix, as it is"
    );
    assert_eq!(
        Role::Dim
            .solid()
            .toward(Role::Foreground, 7.0)
            .resolve(value),
        [1.0, 1.0, 1.0, 1.0],
        "the amount is held to 0..1"
    );
    // A value that is see-through makes the mix see-through too, and not opaque.
    let glass = Rgba::new(0, 0, 0, 0).solid();
    assert!(!Role::Dim.solid().toward(glass, 0.5).is_opaque());
    assert!(Role::Dim.solid().toward(Role::Foreground, 0.5).is_opaque());
}

#[test]
fn an_elements_opacity_fades_all_it_draws_and_all_inside_it() {
    let root = div().child(
        div()
            .opacity(0.5)
            .bg(Role::Background)
            .border(1.0, Role::Border)
            .child(
                div()
                    .opacity(0.5)
                    .bg(Role::Accent)
                    .h(20.0)
                    .child(text("half of a half")),
            ),
    );
    let laid = layout(root, Size::new(200.0, 100.0), 1.0, &mut Fixed);
    let list = laid.paint(&mut Fixed);
    let quads = quads(&list);
    assert_eq!(quads[0].fill.map(|fill| fill.alpha), Some(0.5));
    assert_eq!(quads[0].border.map(|border| border.color.alpha), Some(0.5));
    assert_eq!(
        quads[1].fill.map(|fill| fill.alpha),
        Some(0.25),
        "its own times the box's"
    );
    let run = runs(&list)[0];
    assert_eq!(run.color.alpha, 0.25);
    assert_eq!(
        run.grounds
            .iter()
            .map(|ground| ground.alpha)
            .collect::<Vec<_>>(),
        [0.25, 0.5],
        "what it is read against is as faint as it is drawn"
    );
    // Nothing at all shows of a box at nothing, nor of what is inside it.
    let gone = div().opacity(0.0).bg(Role::Accent).child(text("gone"));
    let list =
        layout(div().child(gone), Size::new(200.0, 100.0), 1.0, &mut Fixed).paint(&mut Fixed);
    assert!(list.items.is_empty());
}

#[test]
fn italic_and_the_lines_under_and_through_the_words_are_inherited() {
    let root = div()
        .italic()
        .underline()
        .child(text("both"))
        .child(div().strike().child(text("all three")));
    let laid = layout(root, Size::new(200.0, 100.0), 1.0, &mut Fixed);
    let list = laid.paint(&mut Fixed);
    let runs = runs(&list);
    assert!(runs[0].font.italic && runs[0].underline && !runs[0].strike);
    assert!(runs[1].font.italic && runs[1].underline && runs[1].strike);
    let plain = layout(
        div().child(text("plain")),
        Size::new(100.0, 20.0),
        1.0,
        &mut Fixed,
    )
    .paint(&mut Fixed);
    let plain = &crate::tests::runs(&plain)[0];
    assert!(!plain.font.italic && !plain.underline && !plain.strike);
}

#[test]
fn a_page_is_read_as_its_buttons_fields_and_words_each_said_once() {
    use crate::{AccessibleRole, Element};
    let row = |name: &str, label: Option<&str>| {
        let button: Element = div().row().press(Id::named(name)).h(20.0);
        let button = match label {
            Some(label) => button.label(label),
            None => button,
        };
        button.child(text(name)).child(text("more"))
    };
    let root = div()
        .gap(2.0)
        .child(text("Containers"))
        .child(row("restart", Some("Restart the container")))
        .child(row("stop", None))
        .child(
            div()
                .text_field(Id::named("filter"))
                .h(18.0)
                .child(text("Filter by name")),
        )
        .child(div().label("Running").size(8.0, 8.0))
        .child(text("hidden").hidden())
        .child(text("after"));
    let laid = layout(root, Size::new(400.0, 400.0), 2.0, &mut Fixed);
    let items = laid.accessible();
    let read: Vec<(AccessibleRole, String)> = items
        .iter()
        .map(|item| (item.role, item.words.clone()))
        .collect();
    assert_eq!(
        read,
        [
            (AccessibleRole::Text, "Containers".to_owned()),
            (AccessibleRole::Button, "Restart the container".to_owned()),
            (AccessibleRole::Button, "stop more".to_owned()),
            (
                AccessibleRole::Field(FieldKind::Line),
                "Filter by name".to_owned()
            ),
            (AccessibleRole::Text, "Running".to_owned()),
            (AccessibleRole::Text, "after".to_owned()),
        ]
    );
    assert_eq!(items[1].id, Some(Id::named("restart")));
    assert_eq!(items[1].rect.h, 20.0, "in points");
}

#[test]
fn a_box_inside_padding_stands_in_its_middle_at_a_scale_that_is_not_whole() {
    // A segmented control's thumb: two points of padding round a box 24 tall, the page drawn at
    // 2.2 pixels to a point (a terminal's zoom). Each padding is as many pixels, not 4 above and
    // 5 below as edges rounded where 4.4 and 52.8 add up to.
    let thumb = Id::new(9);
    let page = div().child(
        div()
            .p(2.0)
            .border(1.0, Role::Border)
            .child(div().id(thumb).h(24.0).w(40.0)),
    );
    for top in [0.0, 3.0, 7.0] {
        let laid = layout(
            div().pt(top).child(page.clone()),
            Size::new(400.0, 200.0),
            2.2,
            &mut Fixed,
        );
        let outer = laid.nodes[2].rect;
        let inner = laid.bounds(thumb).unwrap();
        let s = 2.2;
        let (above, below) = (
            inner.y * s - outer.y,
            outer.bottom() - (inner.y + inner.h) * s,
        );
        assert!(
            (above - below).abs() < 0.01,
            "{above} above, {below} below at {top}"
        );
    }
}
