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
            Paint::Quad(_)
            | Paint::Image(_)
            | Paint::Figure(_)
            | Paint::Editor(_)
            | Paint::Browser(_) => None,
        })
        .collect()
}

fn quads(list: &DrawList) -> Vec<&crate::Quad> {
    list.items
        .iter()
        .filter_map(|item| match item {
            Paint::Quad(quad) => Some(quad),
            Paint::Text(_)
            | Paint::Image(_)
            | Paint::Figure(_)
            | Paint::Editor(_)
            | Paint::Browser(_) => None,
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
fn a_field_under_an_overlay_is_covered_and_one_beside_what_follows_it_is_not() {
    let field = |id| {
        div()
            .text_field(Id::new(id))
            .w(120.0)
            .h(18.0)
            .child(text("Filter"))
    };
    // What follows a field in the row is beside it, not over it.
    let open = div().row().child(field(1)).child(text("12 processes"));
    let laid = layout(open, Size::new(300.0, 100.0), 1.0, &mut Fixed);
    assert!(!laid.fields()[0].covered);
    // A card over the page, where the field stands.
    let covered = div().child(field(1)).child(
        div()
            .overlay()
            .inset(Some(0.0), None, None, Some(40.0))
            .w(200.0)
            .h(80.0)
            .child(field(2)),
    );
    let laid = layout(covered, Size::new(300.0, 100.0), 1.0, &mut Fixed);
    let fields = laid.fields();
    assert!(fields[0].covered, "the page's field is under the card");
    assert!(!fields[1].covered, "the card's own field is on top");
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
            .scroll()
            .scrolled(0.0, offset)
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
    // The words keep their own colour, and say how much of them shows apart from it: a host that
    // holds the colour to its contrast lays this over what it holds.
    assert_eq!(run.color.alpha, 1.0);
    assert_eq!(run.opacity, 0.25);
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
fn an_element_can_take_back_the_italic_and_the_lines_it_would_inherit() {
    let mut inside = div().child(text("taken back"));
    inside.style.text.italic = Some(false);
    inside.style.text.underline = Some(false);
    inside.style.text.strike = Some(false);
    let root = div().italic().underline().strike().child(inside);
    let laid = layout(root, Size::new(200.0, 100.0), 1.0, &mut Fixed);
    let list = laid.paint(&mut Fixed);
    let runs = runs(&list);
    assert!(!runs[0].font.italic && !runs[0].underline && !runs[0].strike);
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
fn a_control_with_no_label_and_no_words_is_said_by_its_tip_and_not_by_its_keys() {
    use crate::AccessibleRole;
    let icon = |name: &str| div().row().press(Id::named(name)).size(20.0, 20.0);
    let root = div()
        .col()
        .child(icon("alone").tip("Containers", Some("1".to_owned())))
        .child(icon("labelled").label("Open").tip("Tip", None))
        .child(icon("worded").tip("Tip", None).child(text("Words inside")))
        .child(icon("untipped"))
        .child(icon("emptied").label("").tip("Emptied label", None))
        .child(icon("tab").role(AccessibleRole::Tab).tip("Images", None))
        .child(
            div()
                .row()
                .size(20.0, 20.0)
                .role(AccessibleRole::Heading(2))
                .tip("Not a name", None),
        )
        .child(
            icon("blank")
                .tip("Blank is not a name", None)
                .child(text("  ")),
        );
    let laid = layout(root, Size::new(400.0, 400.0), 1.0, &mut Fixed);
    let read: Vec<(AccessibleRole, String)> = laid
        .accessible()
        .iter()
        .map(|item| (item.role, item.words.clone()))
        .collect();
    assert_eq!(
        read,
        [
            (AccessibleRole::Button, "Containers".to_owned()),
            (AccessibleRole::Button, "Open".to_owned()),
            (AccessibleRole::Button, "Words inside".to_owned()),
            (AccessibleRole::Button, String::new()),
            (AccessibleRole::Button, "Emptied label".to_owned()),
            (AccessibleRole::Tab, "Images".to_owned()),
            (AccessibleRole::Heading(2), String::new()),
            (AccessibleRole::Button, "Blank is not a name".to_owned()),
        ]
    );
}

/// The page as a screen reader is told it: each item's role and words.
fn read_of(root: crate::Element) -> Vec<(crate::AccessibleRole, String)> {
    layout(root, Size::new(400.0, 400.0), 1.0, &mut Fixed)
        .accessible()
        .iter()
        .map(|item| (item.role, item.words.clone()))
        .collect()
}

#[test]
fn a_dialog_over_a_scrim_that_answers_the_pointer_is_read_as_its_buttons_and_fields() {
    use crate::AccessibleRole::{Button, Field};
    let button = |name: &str, words: &str| {
        div()
            .row()
            .press(Id::named(name))
            .h(20.0)
            .child(text(words))
    };
    let dialog = div()
        .col()
        .child(text("Dialog title"))
        .child(
            div()
                .text_field(Id::named("name"))
                .h(18.0)
                .child(text("Name")),
        )
        .child(
            div()
                .row()
                .child(button("ok", "OK"))
                .child(button("cancel", "Cancel")),
        );
    let scrim = |label: Option<&str>| {
        let scrim = div()
            .press(Id::named("scrim"))
            .overlay()
            .inset(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
            .child(dialog.clone());
        match label {
            Some(label) => scrim.label(label),
            None => scrim,
        }
    };
    let page = |scrim| div().child(text("Page")).child(scrim);
    // Said by the words inside it that are in none of the controls, and each control apart.
    assert_eq!(
        read_of(page(scrim(None))),
        [
            (crate::AccessibleRole::Text, "Page".to_owned()),
            (Button, "Dialog title".to_owned()),
            (Field(FieldKind::Line), "Name".to_owned()),
            (Button, "OK".to_owned()),
            (Button, "Cancel".to_owned()),
        ]
    );
    // Said by its label alone where it has one: the title is the dialog's, read in its place as
    // words, and the buttons and the field apart.
    assert_eq!(
        read_of(page(scrim(Some("Close dialog")))),
        [
            (crate::AccessibleRole::Text, "Page".to_owned()),
            (Button, "Close dialog".to_owned()),
            (crate::AccessibleRole::Text, "Dialog title".to_owned()),
            (Field(FieldKind::Line), "Name".to_owned()),
            (Button, "OK".to_owned()),
            (Button, "Cancel".to_owned()),
        ]
    );
    // With no words and no tip it is not read at all, and what it holds still is.
    let bare = div().press(Id::named("scrim")).child(
        div()
            .row()
            .child(button("ok", "OK"))
            .child(button("cancel", "Cancel")),
    );
    assert_eq!(
        read_of(bare.clone()),
        [(Button, "OK".to_owned()), (Button, "Cancel".to_owned())]
    );
    // Its tip names it, as it does a control with no words.
    assert_eq!(
        read_of(bare.tip("Dismiss", None)),
        [
            (Button, "Dismiss".to_owned()),
            (Button, "OK".to_owned()),
            (Button, "Cancel".to_owned())
        ]
    );
}

#[test]
fn a_row_with_a_checkbox_in_it_is_the_row_and_the_checkbox_apart() {
    use crate::AccessibleRole::{Button, Checkbox, Link, Text};
    use crate::{rich, rich::span};
    let mark = || {
        div()
            .press(Id::named("mark"))
            .role(Checkbox)
            .checked(true)
            .label("Marked")
            .size(10.0, 10.0)
    };
    let row = div()
        .row()
        .press(Id::named("row"))
        .h(20.0)
        .child(mark())
        .child(text("report.txt"))
        .child(text("12 KB"));
    assert_eq!(
        read_of(row),
        [
            (Button, "report.txt 12 KB".to_owned()),
            (Checkbox, "Marked".to_owned()),
        ]
    );
    // A label is the row's name alone; the words inside it that are no control's are read apart.
    let labelled = div()
        .row()
        .press(Id::named("row"))
        .label("Open report")
        .h(20.0)
        .child(mark())
        .child(text("report.txt"));
    assert_eq!(
        read_of(labelled),
        [
            (Button, "Open report".to_owned()),
            (Checkbox, "Marked".to_owned()),
            (Text, "report.txt".to_owned()),
        ]
    );
    // A link in the words of a button is a control: the button is said by the words, the link apart.
    let linked = div()
        .press(Id::named("note"))
        .h(20.0)
        .child(rich([span("see "), span("docs").link(Id::named("docs"))]));
    assert_eq!(
        read_of(linked),
        [(Button, "see docs".to_owned()), (Link, "docs".to_owned())]
    );
    // An element that says what it is carries controls too; a box with a label alone is not one.
    let heading = div()
        .role(crate::AccessibleRole::Heading(2))
        .child(text("Files"))
        .child(div().press(Id::named("sort")).child(text("Sort")));
    let plain = div().label("Running").child(text("inside"));
    assert_eq!(
        read_of(div().child(heading).child(plain)),
        [
            (crate::AccessibleRole::Heading(2), "Files".to_owned()),
            (Button, "Sort".to_owned()),
            (Text, "Running".to_owned()),
        ]
    );
}

#[test]
fn a_button_with_no_control_in_it_is_still_said_by_all_the_words_inside_it() {
    use crate::AccessibleRole::Button;
    let button = div()
        .col()
        .press(Id::named("card"))
        .child(text("Title"))
        .child(div().size(8.0, 8.0).label("Badge"))
        .child(div().child(text("Detail")));
    assert_eq!(read_of(button), [(Button, "Title Detail".to_owned())]);
    // With a label it is that and nothing else: the words inside it are not said.
    let labelled = div()
        .col()
        .press(Id::named("card"))
        .label("Open card")
        .child(text("Title"))
        .child(div().child(text("Detail")));
    assert_eq!(read_of(labelled), [(Button, "Open card".to_owned())]);
}

/// The page as a screen reader is told it, each item with its state.
fn read_states(root: crate::Element) -> Vec<(crate::AccessibleRole, String, crate::State)> {
    layout(root, Size::new(400.0, 400.0), 1.0, &mut Fixed)
        .accessible()
        .iter()
        .map(|item| (item.role, item.words.clone(), item.state.clone()))
        .collect()
}

#[test]
fn a_button_that_holds_a_heading_a_picture_or_a_progress_is_still_said_by_its_words() {
    use crate::AccessibleRole::{Button, Heading, Image, Progress};
    // What only shows is not a control: the button is said by the words inside it, as it was.
    let section = div()
        .press(Id::named("sec"))
        .h(20.0)
        .child(text("Settings").role(Heading(2)));
    assert_eq!(read_of(section), [(Button, "Settings".to_owned())]);
    let card = div()
        .press(Id::named("card"))
        .h(20.0)
        .child(div().size(8.0, 8.0).role(Image).label("Logo"))
        .child(text("Build"))
        .child(div().size(8.0, 8.0).role(Progress).value("40%"));
    assert_eq!(read_of(card), [(Button, "Build".to_owned())]);
    // What a person works on is one: the button is read apart from it.
    let slider = div()
        .press(Id::named("card"))
        .h(20.0)
        .child(text("Volume"))
        .child(
            div()
                .press(Id::named("level"))
                .role(crate::AccessibleRole::Slider)
                .label("Level")
                .size(8.0, 8.0),
        );
    assert_eq!(
        read_of(slider),
        [
            (Button, "Volume".to_owned()),
            (crate::AccessibleRole::Slider, "Level".to_owned())
        ]
    );
}

#[test]
fn a_row_that_says_what_it_is_or_what_state_it_is_in_is_read_though_it_has_no_words_of_its_own() {
    use crate::AccessibleRole::{Button, Checkbox, MenuItem};
    let mark = |name: &str| {
        div()
            .press(Id::named(name))
            .role(Checkbox)
            .label("Mark")
            .size(10.0, 10.0)
    };
    let row = |row: crate::Element| {
        row.row()
            .press(Id::named("row"))
            .h(20.0)
            .child(mark("a"))
            .child(mark("b"))
    };
    // A row of a grid with every cell a control: it is the row, chosen, and the controls apart.
    let read = read_states(row(div().role(MenuItem).selected(true)));
    assert_eq!(read[0].0, MenuItem);
    assert!(read[0].2.selected);
    assert_eq!(read.len(), 3);
    // A state alone is enough.
    let read = read_states(row(div().expanded(false)));
    assert_eq!((read[0].0, read[0].2.expanded), (Button, Some(false)));
    assert_eq!(read.len(), 3);
    // A plain one that holds controls and says nothing (a scrim) is not read.
    assert_eq!(
        read_of(row(div())),
        [(Checkbox, "Mark".to_owned()), (Checkbox, "Mark".to_owned())]
    );
}

#[test]
fn what_a_label_says_is_not_said_again_by_the_words_inside_the_labelled_box() {
    use crate::AccessibleRole::{Button, Slider, Tab, Text};
    let close = || {
        div()
            .press(Id::named("close"))
            .role(Button)
            .label("Close")
            .size(8.0, 8.0)
    };
    // A tab named by its title, with a close button in it: the title is not read twice.
    let tab = div()
        .row()
        .press(Id::named("tab"))
        .role(Tab)
        .label("Files")
        .h(20.0)
        .child(text("Files"))
        .child(close());
    assert_eq!(
        read_of(tab),
        [(Tab, "Files".to_owned()), (Button, "Close".to_owned())]
    );
    // A head named "name, sorted ascending" with an edge in it: the word in it is the label's.
    let edge = || {
        div()
            .press(Id::named("edge"))
            .role(Slider)
            .label("name column width")
            .value("120 pt")
            .size(4.0, 20.0)
    };
    let head = div()
        .row()
        .press(Id::named("head"))
        .role(Button)
        .label("name, sorted ascending")
        .h(20.0)
        .child(text("Name"))
        .child(edge());
    assert_eq!(
        read_of(head),
        [
            (Button, "name, sorted ascending".to_owned()),
            (Slider, "name column width".to_owned())
        ]
    );
    // What the label does not say is read apart, as before; so is a word that only looks like a
    // part of one ("a" is not in "tab").
    let more = div()
        .row()
        .press(Id::named("tab"))
        .role(Tab)
        .label("Open tab")
        .h(20.0)
        .child(text("a"))
        .child(text("Unsaved changes"))
        .child(text("  OPEN   Tab "))
        .child(close());
    // A word of a part of the label is not the part: "Add rate limiting to the public API" says no
    // chip "api" beside it, and "ACM-412" is the label's part.
    let row = div()
        .row()
        .press(Id::named("row"))
        .role(Tab)
        .label("ACM-412, Add rate limiting to the public API")
        .h(20.0)
        .child(text("ACM-412"))
        .child(text("api"))
        .child(close());
    assert_eq!(
        read_of(row),
        [
            (
                Tab,
                "ACM-412, Add rate limiting to the public API".to_owned()
            ),
            (Text, "api".to_owned()),
            (Button, "Close".to_owned())
        ]
    );
    // Whole words only: the label "Settings" does not say "Set", nor "Setting", but says "settings".
    let settings = div()
        .row()
        .press(Id::named("tab"))
        .role(Tab)
        .label("Settings")
        .h(20.0)
        .child(text("Set"))
        .child(text("Setting"))
        .child(text("settings"))
        .child(text("ettings"))
        .child(close());
    assert_eq!(
        read_of(settings),
        [
            (Tab, "Settings".to_owned()),
            (Text, "Set".to_owned()),
            (Text, "Setting".to_owned()),
            (Text, "ettings".to_owned()),
            (Button, "Close".to_owned())
        ]
    );
    assert_eq!(
        read_of(more),
        [
            (Tab, "Open tab".to_owned()),
            (Text, "a".to_owned()),
            (Text, "Unsaved changes".to_owned()),
            (Button, "Close".to_owned())
        ]
    );
}

#[test]
fn a_heading_or_a_picture_that_is_pressed_is_a_control_and_is_read_apart_in_a_box_with_a_label() {
    use crate::AccessibleRole::{Button, Heading, Image};
    // A heading that answers the pointer is read apart, with its name, in a box with a label.
    let panel = div()
        .row()
        .press(Id::named("panel"))
        .label("Panel")
        .h(20.0)
        .child(
            text("Advanced")
                .role(Heading(2))
                .press(Id::named("advanced")),
        )
        .child(
            div()
                .press(Id::named("logo"))
                .role(Image)
                .label("Logo")
                .size(8.0, 8.0),
        );
    assert_eq!(
        read_of(panel),
        [
            (Button, "Panel".to_owned()),
            (Heading(2), "Advanced".to_owned()),
            (Image, "Logo".to_owned())
        ]
    );
}

#[test]
fn a_label_is_not_cut_at_a_comma_or_a_colon_between_digits_and_an_overlay_is_not_said_by_it() {
    use crate::AccessibleRole::{Button, Tab, Text};
    let close = || {
        div()
            .press(Id::named("close"))
            .role(Button)
            .label("Close")
            .size(8.0, 8.0)
    };
    let tab = |label: &str, inner: &str| {
        div()
            .row()
            .press(Id::named("tab"))
            .role(Tab)
            .label(label)
            .h(20.0)
            .child(text(inner))
            .child(close())
    };
    // "1,024 files" is one number and a noun: "024 files" is not a part of it.
    assert_eq!(
        read_of(tab("1,024 files", "024 files")),
        [
            (Tab, "1,024 files".to_owned()),
            (Text, "024 files".to_owned()),
            (Button, "Close".to_owned())
        ]
    );
    assert_eq!(
        read_of(tab("Time: 10:30", "30")),
        [
            (Tab, "Time: 10:30".to_owned()),
            (Text, "30".to_owned()),
            (Button, "Close".to_owned())
        ]
    );
    assert_eq!(
        read_of(tab("Time: 10:30", "Time")),
        [
            (Tab, "Time: 10:30".to_owned()),
            (Button, "Close".to_owned())
        ]
    );
    // A tip or a popover inside that says the label's words is the page's own, and is read.
    let tipped = div()
        .row()
        .press(Id::named("tab"))
        .role(Tab)
        .label("Files")
        .h(20.0)
        .child(close())
        .child(
            div()
                .overlay()
                .inset(Some(0.0), Some(0.0), None, None)
                .child(text("Files")),
        );
    assert_eq!(
        read_of(tipped),
        [
            (Tab, "Files".to_owned()),
            (Button, "Close".to_owned()),
            (Text, "Files".to_owned())
        ]
    );
}

#[test]
fn what_an_overlay_in_a_box_that_carries_controls_says_is_read_in_its_place_and_not_as_its_words() {
    use crate::AccessibleRole::{Button, Checkbox, Text};
    let row = div()
        .row()
        .press(Id::named("row"))
        .h(20.0)
        .child(
            div()
                .press(Id::named("mark"))
                .role(Checkbox)
                .label("Mark")
                .size(10.0, 10.0),
        )
        .child(text("report.txt"))
        .child(
            div()
                .overlay()
                .inset(Some(0.0), Some(0.0), None, None)
                .child(text("Last changed today")),
        );
    assert_eq!(
        read_of(row),
        [
            (Button, "report.txt".to_owned()),
            (Checkbox, "Mark".to_owned()),
            (Text, "Last changed today".to_owned()),
        ]
    );
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

/// The page `card` is the box of, laid out with the stand-in measure: a row, so that the card is as
/// large as it is written.
fn spill_page(card: crate::Element) -> crate::Laid {
    let root = div().row().items_start().child(card.id(CARD));
    layout(root, Size::new(400.0, 600.0), 1.0, &mut Fixed)
}

const CARD: Id = Id::new(1);
const INNER: Id = Id::new(20);

#[test]
fn a_box_that_stands_past_the_inside_of_its_box_is_said_and_one_that_does_not_is_not() {
    // Two hundred points in a card that has 180 inside: twenty past the padding.
    let wide = div().col().w(208.0).h(180.0).p(14.0).no_shrink().child(
        div()
            .id(INNER)
            .w(200.0)
            .h(36.0)
            .no_shrink()
            .child(text("wide")),
    );
    let spills = spill_page(wide).spills();
    assert_eq!(spills.len(), 1, "{spills:?}");
    let spill = &spills[0];
    assert_eq!(spill.id, Some(INNER));
    assert_eq!(spill.within, Some(CARD));
    assert_eq!(spill.past, [0.0, 0.0, 20.0, 0.0]);
    assert_eq!(spill.words, "wide");
    // A card whose parts are as wide as its inside has none.
    let fits = div()
        .col()
        .w(208.0)
        .h(180.0)
        .p(14.0)
        .no_shrink()
        .child(div().id(INNER).w(180.0).h(36.0).no_shrink());
    assert_eq!(spill_page(fits).spills(), []);
}

#[test]
fn what_a_box_that_scrolls_holds_and_what_is_laid_over_a_box_are_not_said_to_stand_past_it() {
    let scrolls = div()
        .col()
        .w(200.0)
        .h(180.0)
        .scroll()
        .no_shrink()
        .child(div().w(300.0).h(400.0).no_shrink());
    assert_eq!(spill_page(scrolls).spills(), []);
    let over = div().col().w(200.0).h(180.0).p(14.0).no_shrink().child(
        div()
            .absolute()
            .inset(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
            .w(200.0)
            .h(180.0),
    );
    assert_eq!(spill_page(over).spills(), []);
}

// --- The model of what is read: layers, groups, `none`, the name a page falls back on ----------------

/// The words a screen reader is told of `root`, one line each, a group's children under it by the
/// two spaces of their depth, and how many items the page behind a modal layer has.
fn told_of(root: crate::Element) -> (Vec<String>, usize) {
    let laid = layout(root, Size::new(400.0, 400.0), 1.0, &mut Fixed);
    let (items, behind) = laid.accessible_behind();
    let depth = |mut at: Option<usize>| {
        let mut depth = 0;
        while let Some(outer) = at {
            depth += 1;
            at = items[outer].parent;
        }
        depth
    };
    let told = items
        .iter()
        .map(|item| {
            format!(
                "{}{:?} {}",
                "  ".repeat(depth(item.parent)),
                item.role,
                item.words
            )
        })
        .collect();
    (told, behind)
}

/// A page with a button, a modal over it (a scrim that answers the pointer and says nothing, a card
/// with a title and a button) and the layers `extra` writes after it.
fn under_a_modal(modal: bool, extra: Vec<crate::Element>) -> crate::Element {
    let card = div()
        .col()
        .child(text("Settings"))
        .child(div().press(Id::named("ok")).h(20.0).child(text("OK")));
    let layer = div()
        .overlay()
        .inset(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .layer(modal)
        .child(
            div()
                .absolute()
                .inset(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .press(Id::named("scrim"))
                .role(crate::AccessibleRole::None),
        )
        .child(card);
    let mut page = div()
        .child(text("Page"))
        .child(
            div()
                .press(Id::named("behind"))
                .h(20.0)
                .child(text("Behind")),
        )
        .child(layer);
    for more in extra {
        page = page.child(more);
    }
    page
}

#[test]
fn a_modal_layer_shuts_the_page_behind_it_out_of_what_is_read_and_counts_what_it_shut_out() {
    let (told, behind) = told_of(under_a_modal(true, vec![]));
    // The scrim is `none`: it is not read, and the card's words are.
    assert_eq!(told, ["Text Settings", "Button OK"]);
    assert_eq!(behind, 2, "the text and the button of the page");
    // The same layer that is not modal leaves the page, and the scrim, a control no more, is not there.
    let (told, behind) = told_of(under_a_modal(false, vec![]));
    assert_eq!(
        told,
        ["Text Page", "Button Behind", "Text Settings", "Button OK"]
    );
    assert_eq!(behind, 0);
}

#[test]
fn what_a_modal_opens_from_inside_it_is_in_it_and_the_modal_in_front_is_the_layer() {
    // A popover written inside the card (an overlay laid after the rest) is in the modal; a toast, a layer
    // of its own that does not shut the page, is read though it is not inside; a dialog written after
    // is the modal in front, and the first modal and the page are behind it.
    let popover = div()
        .overlay()
        .inset(Some(0.0), Some(0.0), None, None)
        .layer(false)
        .child(text("Hint"));
    let mut card = div()
        .col()
        .child(text("Settings"))
        .child(div().press(Id::named("more")).h(20.0).child(text("More")))
        .child(popover);
    card = card.child(text("tail"));
    let modal = div()
        .overlay()
        .inset(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .layer(true)
        .child(card);
    let toast = div()
        .overlay()
        .inset(None, Some(0.0), Some(0.0), None)
        .layer(false)
        .child(text("Saved"));
    let dialog = div()
        .overlay()
        .inset(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .layer(true)
        .child(text("Sure?"));
    let page = |layers: Vec<crate::Element>| {
        let mut page = div().child(text("Page"));
        for layer in layers {
            page = page.child(layer);
        }
        page
    };
    let (told, behind) = told_of(page(vec![modal.clone(), toast.clone()]));
    assert_eq!(
        told,
        [
            "Text Settings",
            "Button More",
            "Text tail",
            "Text Saved",
            "Text Hint"
        ]
    );
    assert_eq!(behind, 1);
    let (told, _) = told_of(page(vec![modal, toast, dialog]));
    assert_eq!(told, ["Text Sure?"]);
}

#[test]
fn an_element_with_a_role_that_holds_controls_is_a_group_and_what_is_in_it_is_its_children() {
    use crate::AccessibleRole::{MenuItem, Text};
    let button = |name: &str, words: &str| div().press(Id::named(name)).h(20.0).child(text(words));
    let row = div()
        .col()
        .press(Id::named("row"))
        .role(MenuItem)
        .label("alpha")
        .child(text("alpha"))
        .child(text("extra"))
        .child(button("mark", "Mark"));
    let (told, _) = told_of(div().child(row).child(text("after")));
    assert_eq!(
        told,
        [
            format!("{MenuItem:?} alpha"),
            format!("  {Text:?} extra"),
            "  Button Mark".to_owned(),
            format!("{Text:?} after"),
        ]
    );
    // A box with no role that holds controls stays as it was: its controls are apart, no group.
    let plain = div()
        .col()
        .press(Id::named("card"))
        .label("card")
        .child(button("inside", "Inside"));
    let (told, _) = told_of(plain);
    assert_eq!(told, ["Button card", "Button Inside"]);
}

#[test]
fn a_row_with_no_name_of_its_own_is_read_by_the_name_the_page_falls_back_on_and_never_by_it_over_a_name()
 {
    use crate::AccessibleRole::MenuItem;
    let control = |name: &str| {
        div()
            .press(Id::named(name))
            .role(crate::AccessibleRole::Checkbox)
            .label(name)
            .h(10.0)
    };
    let row = |fallback: bool| {
        let row = div()
            .col()
            .press(Id::named("row"))
            .role(MenuItem)
            .child(control("mark"));
        if fallback {
            row.fallback_name("Row 3")
        } else {
            row
        }
    };
    let (told, _) = told_of(row(true));
    assert_eq!(told[0], format!("{MenuItem:?} Row 3"));
    let (told, _) = told_of(row(false));
    assert_eq!(told[0], format!("{MenuItem:?} "));
    // Where the row has words of its own, they are its name.
    let named = row(true).label("alpha");
    let (told, _) = told_of(named);
    assert_eq!(told[0], format!("{MenuItem:?} alpha"));
}

// --- The vocabulary: what a tip says more than a name, actions, what the page calls a kind --------------

fn items_of(root: crate::Element) -> Vec<crate::Accessible> {
    layout(root, Size::new(400.0, 400.0), 1.0, &mut Fixed).accessible()
}

#[test]
fn what_a_tip_says_more_than_the_name_the_keys_the_actions_and_the_page_s_name_of_a_kind_are_told_with_the_element()
 {
    use crate::{AccessibleRole::Button, Command};
    let root = div()
        // The page's own command, on its outermost box: it is no action of an element.
        .command(Command::new(Id::named("page"), "Page"))
        .child(
            div()
                .press(Id::named("stop"))
                .h(20.0)
                .role_description("process")
                .tip("Ends the process", Some("ctrl+k".to_owned()))
                .command(Command::new(Id::named("kill"), "Kill"))
                .command(Command::new(Id::named("stop-it"), "Stop it").enabled(false))
                .child(text("Stop")),
        )
        // What the tip says is the name: nothing is said twice.
        .child(
            div()
                .press(Id::named("close"))
                .h(20.0)
                .tip("Close", None)
                .child(text("Close")),
        );
    let items = items_of(root);
    let stop = &items[0];
    assert_eq!(stop.role, Button);
    assert_eq!(stop.words, "Stop");
    assert_eq!(stop.detail.description.as_deref(), Some("Ends the process"));
    assert_eq!(stop.detail.keys.as_deref(), Some("ctrl+k"));
    assert_eq!(stop.detail.role_description.as_deref(), Some("process"));
    let actions: Vec<(&str, bool)> = stop
        .detail
        .actions
        .iter()
        .map(|action| (action.title.as_str(), action.enabled))
        .collect();
    assert_eq!(actions, [("Kill", true), ("Stop it", false)]);
    let close = &items[1];
    assert_eq!(close.detail, crate::Detail::default());
}

#[test]
fn the_states_a_page_gives_an_element_are_told_as_it_gave_them() {
    use crate::{AccessibleRole::TreeItem, Sort};
    let item = div()
        .press(Id::named("src"))
        .h(20.0)
        .role(TreeItem)
        .label("src")
        .level(2)
        .position(3, 12)
        .expanded(true)
        .mixed(true)
        .checked(true)
        .table_size(200, 4)
        .cell_at(Some(5), Some(2))
        .sorted(Sort::Descending);
    let items = items_of(div().child(item));
    let state = &items[0].state;
    assert_eq!(items[0].role, TreeItem);
    assert_eq!(state.level, Some(2));
    assert_eq!((state.posinset, state.setsize), (Some(3), Some(12)));
    assert_eq!((state.rowcount, state.colcount), (Some(200), Some(4)));
    assert_eq!((state.rowindex, state.colindex), (Some(5), Some(2)));
    assert_eq!(state.sort, Some(Sort::Descending));
    assert_eq!(state.expanded, Some(true));
    // Mixed is neither on nor off: it took the other away; a later `checked` gives it back.
    assert!(state.mixed);
    assert_eq!(state.checked, Some(true));
    let mixed = div()
        .press(Id::named("all"))
        .h(20.0)
        .checked(true)
        .mixed(true);
    assert_eq!(items_of(div().child(mixed))[0].state.checked, None);
}

#[test]
fn a_dialog_that_holds_buttons_is_a_group_with_the_buttons_as_its_children() {
    use crate::AccessibleRole::{Button, Dialog};
    let dialog = div()
        .col()
        .role(Dialog)
        .label("Delete it?")
        .child(text("This cannot be undone."))
        .child(div().press(Id::named("yes")).h(20.0).child(text("Delete")))
        .child(div().press(Id::named("no")).h(20.0).child(text("Keep")));
    let (told, _) = told_of(div().child(dialog));
    assert_eq!(
        told,
        [
            format!("{Dialog:?} Delete it?"),
            format!("  {:?} This cannot be undone.", crate::AccessibleRole::Text),
            format!("  {Button:?} Delete"),
            format!("  {Button:?} Keep"),
        ]
    );
}
