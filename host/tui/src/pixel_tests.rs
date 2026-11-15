//! The page as pixels: what is written for a terminal, read back by a terminal that keeps the window
//! as pixels. The terminal here follows the stream as the graphics protocol says (a picture sent and
//! put, a frame of it edited in place, a picture given up, the cursor that says where a picture is
//! put), and what it shows is compared with the same draw list drawn from the start.

use pencere_drive::shot::{Fonts, Palette, raster};
use pencere_view::{
    Border, Color, DrawList, Family, Font, Paint, Point, Quad, Rect, Role, Run, Shadow,
};

use crate::pixel::{Pixel, Written};
use crate::probe::Found;

const CELL: (u32, u32) = (10, 20);

fn found(frames: bool) -> Found {
    found_with(frames, false)
}

fn found_with(frames: bool, compose: bool) -> Found {
    Found {
        dark: true,
        graphics: true,
        compression: true,
        frames,
        compose: compose && frames,
        pixel_mouse: true,
        window: None,
        cell: Some(CELL),
        keyboard: false,
        multiplexer: false,
        fenced: true,
    }
}

#[path = "../tests/common/window.rs"]
mod window;
use window::{Done, Window};

fn quad(rect: Rect, color: Color) -> Paint {
    Paint::Quad(Quad {
        rect,
        radii: [0.0; 4],
        fill: Some(color),
        gradient: None,
        border: None,
        shadow: None,
        clip: Rect::new(0.0, 0.0, 800.0, 400.0),
    })
}

fn words(x: f32, y: f32, text: &str, color: Color) -> Paint {
    Paint::Text(Run {
        origin: Point::new(x, y),
        text: text.to_owned(),
        font: Font {
            family: Family::Terminal,
            size: 16.0,
            weight: 400,
            italic: false,
        },
        color,
        opacity: 1.0,
        grounds: Vec::new(),
        clip: Rect::new(0.0, 0.0, 800.0, 400.0),
        underline: false,
        strike: false,
    })
}

/// A page of twelve lines in a window of 60 × 12 cells: a ground, a side with a line down it, a
/// card with a shadow and a border, and the lines of text, each in its row, moved up by `shift`.
fn page(lines: &[String], shift: f32) -> DrawList {
    let mut items = vec![
        quad(Rect::new(0.0, 0.0, 800.0, 400.0), Role::Background.solid()),
        quad(Rect::new(0.0, 0.0, 150.0, 400.0), Role::Raised(1).solid()),
        quad(Rect::new(149.0, 0.0, 1.0, 400.0), Role::Border.solid()),
    ];
    items.push(Paint::Quad(Quad {
        rect: Rect::new(300.0, 60.0, 200.0, 100.0),
        radii: [6.0; 4],
        fill: Some(Role::Raised(2).solid()),
        gradient: None,
        border: Some(Border {
            width: 1.0,
            color: Role::Border.solid(),
        }),
        shadow: Some(Shadow {
            dx: 0.0,
            dy: 4.0,
            blur: 12.0,
            color: Role::Shadow.alpha(0.5),
        }),
        clip: Rect::new(0.0, 0.0, 800.0, 400.0),
    }));
    for (row, line) in lines.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let y = row as f32 * 20.0 - shift;
        items.push(words(8.0, y, line, Role::Foreground.solid()));
    }
    DrawList { items }
}

fn lines() -> Vec<String> {
    (0..12)
        .map(|n| format!("line {n}: the page is drawn as pixels"))
        .collect()
}

fn pixel(frames: bool) -> Pixel {
    Pixel::new(Fonts::carried(), &found(frames), CELL, (60, 12))
}

/// What the terminal is sent of a window drawn (`drawn`, `size` pixels): a cell more to the right and
/// below, where the terminal's window is not a whole number of cells, with the last column and the
/// last row of the page repeated into it.
fn padded(drawn: &[u8], size: (u32, u32), cell: (u32, u32)) -> Vec<u8> {
    let (width, height) = (size.0 as usize, size.1 as usize);
    let mut out = Vec::new();
    for row in 0..height + cell.1 as usize {
        let from = row.min(height - 1) * width * 4;
        out.extend_from_slice(&drawn[from..from + width * 4]);
        let last = &drawn[from + (width - 1) * 4..from + width * 4];
        for _ in 0..cell.0 {
            out.extend_from_slice(last);
        }
    }
    out
}

/// How many pixels the picture of a window is, across and down: its cells and a cell more.
fn extent(size: (u32, u32)) -> (u32, u32) {
    (size.0 + CELL.0, size.1 + CELL.1)
}

/// The window as the terminal shows it, and the same list drawn from the start.
fn compare(window: &Window, pixel: &Pixel, list: &DrawList, what: &str) {
    let size = pixel.size();
    let drawn = raster(list, size, pixel.fonts(), None, Palette::Dark).expect("a picture");
    let want = padded(drawn.rgba(), size, CELL);
    let shown = window.shows(extent(size), CELL);
    let wrong = shown
        .chunks(4)
        .zip(want.chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        wrong, 0,
        "{what}: {wrong} pixels of the window are not the list drawn anew"
    );
}

fn send(window: &mut Window, pixel: &mut Pixel, list: DrawList) -> Written {
    let written = pixel.draw(list, None);
    window.feed(&written.bytes);
    written
}

/// The rows of the window a line of text can change: the bands its box reaches.
fn bands_of(row: u32) -> (u32, u32) {
    // A line of 16 pixels is a line box of 21 or so: from a fifth above its top to a fifth and a
    // whole line below.
    let top = row * 20;
    let first = top.saturating_sub(5) / CELL.1;
    let last = (top + 21 + 26) / CELL.1;
    (first * CELL.1, (last - first + 1) * CELL.1)
}

#[test]
fn the_window_is_sent_whole_first_and_a_line_that_changes_is_the_rows_it_is_in() {
    let (mut window, mut pixel) = (Window::default(), pixel(true));
    let mut text = lines();
    let first = page(&text, 0.0);
    let written = send(&mut window, &mut pixel, first.clone());
    assert!(written.whole);
    compare(&window, &pixel, &first, "the first frame");
    let whole = written.bytes.len();
    // One line of text is another (a key typed).
    text[6] = "line 6: a word typed here".to_owned();
    let next = page(&text, 0.0);
    let written = send(&mut window, &mut pixel, next.clone());
    assert!(
        !written.whole,
        "a line that changes is not the whole window"
    );
    assert_eq!(written.bands.len(), 1, "the rows it reaches are one band");
    let (top, rows) = written.bands[0];
    let (want_top, want_rows) = bands_of(6);
    assert!(
        top <= want_top
            && top + rows >= want_top + want_rows - CELL.1
            && rows <= want_rows + CELL.1,
        "the band is the rows of the line ({top}, {rows}), about ({want_top}, {want_rows})"
    );
    assert!(
        matches!(window.done.last(), Some(Done::Edit { at: (x, y), size: (w, h), .. })
            if *y >= top && y + h <= top + rows && x + w <= 600),
        "what changed of the band is edited into the picture where it is: {:?}",
        window.done.last()
    );
    // The rows it is in are some of the window's twelve: their share of the bytes, not a tenth.
    let height = pixel.size().1 as usize;
    assert!(
        written.bytes.len() * height < whole * rows as usize * 2,
        "a line is the share of the window's bytes its rows are ({} of {whole}, {rows} of {height} pixels)",
        written.bytes.len()
    );
    compare(&window, &pixel, &next, "a line changed");
    // Nothing changed: nothing is written.
    let written = send(&mut window, &mut pixel, next);
    assert!(written.bytes.is_empty() && written.bands.is_empty());
}

#[test]
fn a_frame_asks_the_terminal_for_nothing_but_what_it_refuses() {
    let (mut window, mut pixel) = (Window::default(), pixel(true));
    let mut text = lines();
    send(&mut window, &mut pixel, page(&text, 0.0));
    text[3].push('!');
    send(&mut window, &mut pixel, page(&text, 0.0));
    assert!(
        window.quiet.iter().filter(|q| *q == "1").count() >= 2,
        "the picture and its edit: {:?}",
        window.quiet
    );
    assert!(
        window.quiet.iter().all(|q| q == "1" || q == "2"),
        "an answer for a refusal only, or none: {:?}",
        window.quiet
    );
}

#[test]
fn scrolling_moves_the_text_and_the_window_is_what_the_list_drawn_anew_is() {
    let (mut window, mut pixel) = (Window::default(), pixel(true));
    let text = lines();
    send(&mut window, &mut pixel, page(&text, 0.0));
    for shift in [7.0, 14.0, 21.0, 28.0, 35.0] {
        let list = page(&text, shift);
        let written = send(&mut window, &mut pixel, list.clone());
        assert!(
            !written.whole,
            "a scroll is an edit of the window, not a new one"
        );
        compare(&window, &pixel, &list, "scrolled");
    }
}

#[test]
fn a_window_of_another_size_is_sent_whole_and_the_old_one_is_given_up() {
    let (mut window, mut pixel) = (Window::default(), pixel(true));
    let text = lines();
    send(&mut window, &mut pixel, page(&text, 0.0));
    let held = window.images.keys().copied().collect::<Vec<_>>();
    pixel.fit(CELL, (80, 12));
    let list = page(&text, 0.0);
    let written = send(&mut window, &mut pixel, list.clone());
    assert!(written.whole);
    for id in held {
        assert!(window.done.contains(&Done::Free(id)), "{id} is given up");
    }
    assert_eq!(window.images.len(), 1, "one picture is the window");
    let size = pixel.size();
    assert_eq!(size, (800, 240));
    compare(&window, &pixel, &list, "a wider window");
    // Taller, and cells of another size: the scale of the page changes with them.
    let scale = pixel.scale();
    assert!(pixel.fit((12, 24), (60, 10)));
    assert!(pixel.scale() > scale);
    let list = page(&text, 0.0);
    let written = send(&mut window, &mut pixel, list.clone());
    assert!(written.whole);
    let drawn = raster(&list, pixel.size(), pixel.fonts(), None, Palette::Dark).expect("a picture");
    assert_eq!(
        window
            .images
            .values()
            .next()
            .map(|image| image.rgba.as_slice()),
        Some(padded(drawn.rgba(), pixel.size(), (12, 24)).as_slice())
    );
}

#[test]
fn a_terminal_that_refuses_the_edit_of_a_frame_is_sent_the_rows_as_pictures_over_the_window() {
    let (mut window, mut pixel) = (Window::default(), pixel(false));
    let mut text = lines();
    send(&mut window, &mut pixel, page(&text, 0.0));
    text[2].push('?');
    let list = page(&text, 0.0);
    let written = send(&mut window, &mut pixel, list.clone());
    assert!(!written.whole && written.bands.len() == 1);
    let Some(Done::Put { at, size, z, .. }) = window.done.last() else {
        panic!("a picture laid over the window: {:?}", window.done.last());
    };
    assert_eq!(
        (at.1, size.0),
        (0, 610),
        "across the window, and a cell past it"
    );
    assert_eq!(at.0 * CELL.1, written.bands[0].0, "at its first row");
    assert!(*z > 1, "over the window");
    compare(&window, &pixel, &list, "a row laid over");
    // Row after row: the window is sent whole again after a number of them, so that the pictures
    // the terminal holds do not grow for ever.
    for n in 0..90 {
        text[5] = format!("line 5: {n}");
        let list = page(&text, 0.0);
        send(&mut window, &mut pixel, list.clone());
        assert!(
            window.images.len() <= 62,
            "{n}: {} pictures held",
            window.images.len()
        );
    }
    let list = page(&text, 0.0);
    compare(&window, &pixel, &list, "after many rows");
    assert!(
        window
            .done
            .iter()
            .filter(|d| matches!(d, Done::Put { size, .. } if *size == (610, 260)))
            .count()
            >= 2,
        "the window is sent whole again"
    );
}

#[test]
fn an_edit_the_terminal_refuses_is_the_whole_window_and_no_edit_again() {
    let (mut window, mut pixel) = (Window::default(), pixel(true));
    let mut text = lines();
    send(&mut window, &mut pixel, page(&text, 0.0));
    let Some(Done::Put { id, .. }) = window.done.last().cloned() else {
        panic!("the window sent");
    };
    text[1].push('x');
    let written = send(&mut window, &mut pixel, page(&text, 0.0));
    assert!(matches!(window.done.last(), Some(Done::Edit { .. })) && !written.whole);
    // The terminal's answer to that edit is a refusal (an answer for another picture is not).
    assert!(!pixel.answered(id + 50, false));
    assert!(!pixel.answered(id, true));
    assert!(pixel.answered(id, false), "the frame that is due");
    text[1].push('y');
    let list = page(&text, 0.0);
    let written = send(&mut window, &mut pixel, list.clone());
    assert!(written.whole, "the window is sent whole");
    compare(&window, &pixel, &list, "after a refusal");
    text[1].push('z');
    let list = page(&text, 0.0);
    send(&mut window, &mut pixel, list.clone());
    assert!(
        !window
            .done
            .iter()
            .rev()
            .take(3)
            .any(|d| matches!(d, Done::Edit { .. })),
        "an edit is not tried again"
    );
    compare(&window, &pixel, &list, "laid over");
}

#[test]
fn a_light_terminal_is_drawn_in_the_light_palette() {
    let mut found = found(true);
    found.dark = false;
    let mut pixel = Pixel::new(Fonts::carried(), &found, CELL, (60, 12));
    let mut window = Window::default();
    send(&mut window, &mut pixel, DrawList::default());
    let shown = window.shows(pixel.size(), CELL);
    assert_eq!(shown.get(..4), Some(&[255, 255, 255, 255][..]));
}

#[test]
fn the_page_is_as_wide_as_the_cells_in_columns_of_its_mono_text() {
    // A column of the page's mono text (12.5 points of the face) is a cell across at this scale.
    let fonts = Fonts::carried();
    let scale = crate::pixel::scale_for(&fonts, CELL.0);
    let mut fonts = fonts.scaled(scale);
    let font = Font {
        family: Family::Terminal,
        size: 12.5 * scale,
        weight: 400,
        italic: false,
    };
    let width = pencere_view::Measure::measure(&mut fonts, &font, "MMMMMMMMMM").width;
    assert!(
        (width - 10.0 * CELL.0 as f32).abs() < 0.5,
        "ten columns are ten cells: {width}"
    );
}

fn pixel_with(compose: bool) -> Pixel {
    Pixel::new(Fonts::carried(), &found_with(true, compose), CELL, (60, 12))
}

/// The line `n` of a long text: not like the ones next to it, as lines of code are not.
fn code_line(n: usize) -> String {
    const WORDS: [&str; 7] = ["fn", "let", "match", "return", "impl", "struct", "where"];
    format!(
        "{} item_{}({}, {}) {{ // {:x}",
        WORDS[n % 7],
        n * 31 % 97,
        n * 7 % 13,
        "xyzw".chars().nth(n % 4).unwrap_or('q'),
        n * 2654435761 % 4096
    )
}

/// Lines of text over a plain ground, line `first + n` in the row `n`: a page scrolled by lines.
fn plain(first: usize, rows: usize) -> DrawList {
    let mut items = vec![quad(
        Rect::new(0.0, 0.0, 800.0, 400.0),
        Role::Background.solid(),
    )];
    for n in 0..rows {
        #[allow(clippy::cast_precision_loss)]
        let y = n as f32 * 20.0;
        items.push(words(
            8.0,
            y,
            &code_line(first + n),
            Role::Foreground.solid(),
        ));
    }
    DrawList { items }
}

#[test]
fn a_character_typed_is_sent_as_the_rectangle_it_changed_and_not_as_the_rows_it_is_in() {
    let (mut window, mut pixel) = (Window::default(), pixel(true));
    let mut text = lines();
    let first = page(&text, 0.0);
    let whole = send(&mut window, &mut pixel, first).bytes.len();
    text[6].push('!');
    let next = page(&text, 0.0);
    let written = send(&mut window, &mut pixel, next.clone());
    assert!(!written.whole && written.copies.is_empty());
    assert_eq!(written.areas.len(), 1, "one rectangle: {:?}", written.areas);
    let area = written.areas[0];
    assert!(
        area.w <= 3 * CELL.0 && area.h <= 2 * CELL.1,
        "the character and its line, no more: {area:?}"
    );
    assert!(
        written.bytes.len() * 8 < whole,
        "{} bytes for a character, {whole} for the window",
        written.bytes.len()
    );
    compare(&window, &pixel, &next, "a character typed");
}

#[test]
fn a_box_that_takes_another_colour_is_sent_as_the_box() {
    let (mut window, mut pixel) = (Window::default(), pixel(true));
    let button = |colour: Role| {
        let mut list = page(&lines(), 0.0);
        list.items
            .push(quad(Rect::new(300.0, 190.0, 100.0, 30.0), colour.solid()));
        list
    };
    send(&mut window, &mut pixel, button(Role::Raised(1)));
    let next = button(Role::Raised(4));
    let written = send(&mut window, &mut pixel, next.clone());
    assert_eq!(written.areas.len(), 1, "{:?}", written.areas);
    let area = written.areas[0];
    assert!(
        area.x >= 299 && area.right() <= 401 && area.y >= 189 && area.bottom() <= 221,
        "the box and no more: {area:?}"
    );
    compare(&window, &pixel, &next, "a box of another colour");
}

#[test]
fn many_small_changes_are_few_rectangles_and_not_the_box_that_holds_them_all() {
    let (mut window, mut pixel) = (Window::default(), pixel(true));
    let marks = |colour: Role| {
        let mut list = page(&lines(), 0.0);
        // Thirty marks over two strips of the window, far apart in a column: more than a frame is
        // sent as, and the one box that has them all is most of the window.
        for n in 0..30_u32 {
            let (x, y) = (
                10.0 + (n % 15) as f32 * 38.0,
                if n < 15 { 20.0 } else { 220.0 },
            );
            list.items
                .push(quad(Rect::new(x, y, 6.0, 6.0), colour.solid()));
        }
        list
    };
    send(&mut window, &mut pixel, marks(Role::Raised(1)));
    let next = marks(Role::Raised(4));
    let written = send(&mut window, &mut pixel, next.clone());
    assert!(!written.whole);
    assert!(
        written.areas.len() <= crate::pixel::MOST_RECTS,
        "{} rectangles",
        written.areas.len()
    );
    let sent: u64 = written
        .areas
        .iter()
        .map(|a| u64::from(a.w) * u64::from(a.h))
        .sum();
    let size = pixel.size();
    assert!(
        sent * 3 < u64::from(size.0) * u64::from(size.1),
        "{sent} pixels sent of {}: {:?}",
        size.0 * size.1,
        written.areas
    );
    compare(&window, &pixel, &next, "many small changes");
}

#[test]
fn rows_that_scrolled_are_copied_in_the_picture_and_only_the_rows_that_came_into_view_are_sent() {
    let run = |compose: bool| {
        let (mut window, mut pixel) = (Window::default(), pixel_with(compose));
        send(&mut window, &mut pixel, plain(0, 12));
        // Two lines up: what was in the row 2 is in the row 0, and two new lines come in below.
        let list = plain(2, 12);
        let written = send(&mut window, &mut pixel, list.clone());
        compare(&window, &pixel, &list, "scrolled by two lines");
        (written, window, pixel)
    };
    let (copied, window, mut pixel) = run(true);
    let (edited, _, _) = run(false);
    assert!(!copied.copies.is_empty(), "rows are copied");
    assert!(edited.copies.is_empty());
    assert!(
        copied.bytes.len() * 2 < edited.bytes.len(),
        "{} bytes with the copy, {} without",
        copied.bytes.len(),
        edited.bytes.len()
    );
    // What is sent is what came in: no more than the two lines (and a little).
    let sent: u32 = copied.areas.iter().map(|a| a.w * a.h).sum();
    assert!(
        sent <= 2 * 20 * 600,
        "{sent} pixels sent: {:?}",
        copied.areas
    );
    // The copies do not meet where they are from (the window of the test checks it), and are of
    // rows the window held: the shift is the two lines.
    assert!(copied.copies.iter().all(|c| c.shift == 40));
    // A terminal that refuses a copy is not asked to copy again.
    let id = window.images.keys().copied().next().expect("a picture");
    assert!(pixel.answered(id, false), "the frame that is due");
    let (mut window, mut again) = (Window::default(), pixel_with(true));
    send(&mut window, &mut again, plain(0, 12));
    assert!(!again.answered(*window.images.keys().next().unwrap(), true));
    drop(pixel);
}

#[test]
fn a_page_that_scrolls_by_a_whole_line_a_second_time_is_the_same_picture_as_one_drawn_anew() {
    let (mut window, mut pixel) = (Window::default(), pixel_with(true));
    send(&mut window, &mut pixel, plain(0, 12));
    for first in [1, 2, 4, 3, 0, 7] {
        let list = plain(first, 12);
        send(&mut window, &mut pixel, list.clone());
        compare(&window, &pixel, &list, &format!("scrolled to {first}"));
    }
}

/// A page of the window's size with a bar along its foot and a stripe down its right edge.
fn edged(bar: Role, stripe: Role) -> DrawList {
    let mut list = page(&lines(), 0.0);
    list.items
        .push(quad(Rect::new(0.0, 220.0, 800.0, 20.0), bar.solid()));
    list.items
        .push(quad(Rect::new(594.0, 0.0, 6.0, 400.0), stripe.solid()));
    list
}

/// The pixel at `x`, `y` of a picture of `across` pixels.
fn at(rgba: &[u8], across: u32, x: u32, y: u32) -> &[u8] {
    let from = 4 * (y as usize * across as usize + x as usize);
    &rgba[from..from + 4]
}

#[test]
fn the_picture_is_a_cell_wider_and_taller_than_the_cells_and_what_is_past_them_is_the_pages_edge() {
    let (mut window, mut pixel) = (Window::default(), pixel(true));
    let size = pixel.size();
    let past = extent(size);
    let written = send(
        &mut window,
        &mut pixel,
        edged(Role::Raised(3), Role::Accent),
    );
    assert!(written.whole);
    assert!(
        matches!(window.done.last(), Some(Done::Put { size, .. }) if *size == past),
        "the picture is the cells and a cell more: {:?}",
        window.done.last()
    );
    let image = window.images.values().next().expect("the window");
    for y in 0..past.1 {
        for x in size.0..past.0 {
            assert_eq!(
                at(&image.rgba, past.0, x, y),
                at(&image.rgba, past.0, size.0 - 1, y.min(size.1 - 1)),
                "right of the cells, at {x}, {y}, is the edge's"
            );
        }
    }
    for x in 0..past.0 {
        for y in size.1..past.1 {
            assert_eq!(
                at(&image.rgba, past.0, x, y),
                at(&image.rgba, past.0, x.min(size.0 - 1), size.1 - 1),
                "below the cells, at {x}, {y}, is the foot's"
            );
        }
    }
    // The foot's colour is what goes down, and the stripe's what goes right: not the ground's.
    let ground = Palette::Dark
        .role(Role::Background)
        .map(|c| (c * 255.0).round() as u8);
    assert_ne!(at(&image.rgba, past.0, 5, size.1 + 3)[..3], ground[..3]);
    assert_ne!(at(&image.rgba, past.0, size.0 + 3, 5)[..3], ground[..3]);
    // The bar takes another colour: the rows past the cells change with it, in the same edit.
    let list = edged(Role::Raised(6), Role::Accent);
    let written = send(&mut window, &mut pixel, list.clone());
    assert!(!written.whole);
    assert!(
        written.areas.iter().any(|a| a.bottom() == past.1),
        "the edit reaches the foot of the picture: {:?}",
        written.areas
    );
    compare(&window, &pixel, &list, "the foot took another colour");
    // The stripe takes another colour: so does what is past the cells at its side.
    let list = edged(Role::Raised(6), Role::Success);
    let written = send(&mut window, &mut pixel, list.clone());
    assert!(
        written.areas.iter().any(|a| a.right() == past.0),
        "the edit reaches the right of the picture: {:?}",
        written.areas
    );
    compare(&window, &pixel, &list, "the stripe took another colour");
}

/// Lines of text scrolled, and a bar at the right edge that moves with them.
fn with_a_bar(first: usize) -> DrawList {
    let mut list = plain(first, 12);
    #[allow(clippy::cast_precision_loss)]
    let top = 10.0 * first as f32;
    list.items
        .push(quad(Rect::new(592.0, top, 8.0, 60.0), Role::Accent.solid()));
    list
}

#[test]
fn a_page_that_scrolls_with_something_at_its_edge_keeps_what_is_past_the_cells_the_edge_of_the_picture()
 {
    let (mut window, mut copying) = (Window::default(), pixel_with(true));
    send(&mut window, &mut copying, with_a_bar(0));
    for first in [1, 2, 4, 3, 0, 7] {
        let list = with_a_bar(first);
        send(&mut window, &mut copying, list.clone());
        compare(&window, &copying, &list, &format!("scrolled to {first}"));
    }
    // And with the edit refused: the rows laid over the window are as wide as the picture.
    let (mut window, mut pixel) = (Window::default(), self::pixel(false));
    send(&mut window, &mut pixel, with_a_bar(0));
    for first in [1, 2] {
        let list = with_a_bar(first);
        send(&mut window, &mut pixel, list.clone());
        compare(&window, &pixel, &list, &format!("laid over at {first}"));
    }
}
