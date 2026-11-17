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
    compare_within(window, pixel, list, what, 0);
}

/// The same, but for a level or `slack` in a channel of a pixel: a box whose edge is between two rows
/// is drawn, in a few rows of the window, a level apart from the same box drawn in the rows of
/// the picture (the rows are moved to the top of the buffer, and a number nearer to nothing has
/// more places after the point). A row that is another row is a great many levels apart.
fn compare_within(window: &Window, pixel: &Pixel, list: &DrawList, what: &str, slack: u8) {
    let size = pixel.size();
    let drawn = raster(list, size, pixel.fonts(), None, Palette::Dark).expect("a picture");
    let want = padded(drawn.rgba(), size, CELL);
    let shown = window.shows(extent(size), CELL);
    let wrong = shown
        .chunks(4)
        .zip(want.chunks(4))
        .filter(|(a, b)| a.iter().zip(*b).any(|(p, q)| p.abs_diff(*q) > slack))
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

/// What a frame asks the system for: the frames that change a line of a window, one after another, are
/// drawn, compared and packed in the memory the frames before them were. The pictures' worth of pixels
/// (a band, a mask, a copy of the band, a cut of it) and the deflate's state are not asked for again
/// by each frame; the few the system makes zero and the drawing writes over are the first frames'.
#[test]
fn frames_that_change_a_line_ask_the_system_for_no_pictures_worth_of_memory() {
    use crate::alloc_count_tests as large;
    let mut pixel = Pixel::new(Fonts::carried(), &found_with(true, true), CELL, (150, 12));
    let mut text = lines();
    let first = page(&text, 0.0);
    let mut lists = Vec::new();
    for n in 0..40 {
        text[6] = format!("line 6: typed {n}");
        text[2] = format!("line 2: and {}", n * 7);
        lists.push(page(&text, 0.0));
    }
    // The window, and the frames that grow what the others are drawn in.
    let written = pixel.draw(first, None);
    assert!(written.whole);
    let mut asked = Vec::new();
    for (n, list) in lists.into_iter().enumerate() {
        large::reset();
        let written = pixel.draw(list, None);
        let (count, bytes) = large::large();
        assert!(
            !written.whole && !written.bands.is_empty(),
            "frame {n} is bands of the window"
        );
        asked.push((n, count, bytes));
    }
    let later: Vec<_> = asked
        .iter()
        .skip(3)
        .filter(|(_, count, _)| *count > 0)
        .collect();
    assert!(
        later.is_empty(),
        "frames after the third asked for memory of {} bytes or more: (frame, times, bytes) {later:?}",
        large::LARGE
    );
}

/// The pixels packed in the state a frame keeps unpack to the pixels, over and over, whatever the size
/// of the pack (the buffer it is made in is grown as it needs, a piece at a time), and are nothing
/// where they would not be smaller than the pixels (a fresh pack is no smaller either).
#[test]
fn pixels_packed_in_the_kept_state_unpack_to_the_pixels_and_are_none_where_not_smaller() {
    let mut wire = crate::pixel::Wire::new();
    let mut seed = 12345_u32;
    // Runs of one value; noise; and a few values at random, that pack to a few parts in ten.
    for (n, len) in [
        4_usize, 4000, 65_536, 300, 120_000, 4000, 17, 700_000, 70_000,
    ]
    .into_iter()
    .enumerate()
    {
        let data: Vec<u8> = (0..len)
            .map(|at| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                match n % 3 {
                    0 => (at / 64 % 7) as u8,
                    1 => (seed >> 24) as u8,
                    _ => ((seed >> 24) % 12) as u8,
                }
            })
            .collect();
        let fresh = miniz_oxide::deflate::compress_to_vec_zlib(&data, crate::output::PACK_LEVEL);
        match crate::pixel::deflate(&mut wire.comp, &mut wire.deflated, &data) {
            Some(made) => {
                assert!(made < data.len(), "piece {n} of {len} bytes is not smaller");
                let unpacked = miniz_oxide::inflate::decompress_to_vec_zlib(&wire.deflated[..made]);
                assert_eq!(
                    unpacked.as_deref().ok(),
                    Some(data.as_slice()),
                    "piece {n} of {len} bytes"
                );
            }
            None => assert!(
                fresh.len() >= data.len(),
                "piece {n} of {len} bytes is smaller packed ({}) and was not",
                fresh.len()
            ),
        }
    }
}

/// The runs of pixels two rows do not share are the same wherever the pixels that differ fall (in the
/// blocks the rows are compared in, between them, at the end), and whatever the rows' width.
#[test]
fn the_runs_of_pixels_two_rows_do_not_share_are_found_pixel_by_pixel_and_block_by_block_alike() {
    let mut seed = 7_u32;
    let mut next = |most: u32| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) % most
    };
    for width in [1_usize, 2, 15, 16, 17, 31, 32, 33, 100, 241] {
        for round in 0..40 {
            let a: Vec<u8> = (0..width * 4).map(|at| (at % 251) as u8).collect();
            let mut b = a.clone();
            // None, a few, or many pixels that differ.
            let changes = match round % 4 {
                0 => 0,
                1 => 1,
                2 => 1 + next(4),
                _ => width as u32,
            };
            for _ in 0..changes {
                let pixel = next(width as u32) as usize;
                b[pixel * 4 + next(4) as usize] ^= 1 + next(200) as u8;
            }
            for join in [0, 1, 4, 20] {
                let want = naive_runs(&a, &b, join);
                assert_eq!(
                    crate::pixel::diff_runs(&a, &b, join),
                    want,
                    "width {width}, round {round}, join {join}"
                );
            }
        }
    }
}

/// Bytes are the same where the words they are compared in say so: whatever the length (in blocks, in
/// the tail past them, none at all) and wherever the one byte that differs falls.
#[test]
fn bytes_compared_in_words_are_the_same_where_they_are_and_not_where_one_differs() {
    for length in (0_usize..200).chain([255, 256, 257, 9_600, 9_664]) {
        let a: Vec<u8> = (0..length).map(|at| (at * 7 % 251) as u8).collect();
        assert!(crate::pixel::same(&a, &a.clone()), "length {length}");
        assert!(!crate::pixel::same(&a, &a[..length.saturating_sub(1)]) || length == 0);
        let places: Vec<usize> = if length <= 200 {
            (0..length).collect()
        } else {
            (0..length)
                .filter(|at| at % 64 < 2 || at % 64 > 61 || at % 997 == 0)
                .collect()
        };
        for at in places {
            for flip in [1_u8, 0x80, 0xff] {
                let mut b = a.clone();
                b[at] ^= flip;
                assert!(!crate::pixel::same(&a, &b), "length {length}, byte {at}");
                assert!(!crate::pixel::same(&b, &a), "length {length}, byte {at}");
            }
        }
    }
}

/// The runs of pixels, one at a time.
fn naive_runs(a: &[u8], b: &[u8], join: u32) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for (x, (p, q)) in a.chunks(4).zip(b.chunks(4)).enumerate() {
        if p == q {
            continue;
        }
        let x = x as u32;
        match runs.last_mut() {
            Some(last) if x < last.1 + join => last.1 = x + 1,
            _ => runs.push((x, x + 1)),
        }
    }
    runs
}

/// 60 × 20 cells: a window of 600 × 400 pixels, with a side, a room that scrolls, and a bar.
const SCROLL_CELLS: (u16, u16) = (60, 20);

fn scroller(compose: bool) -> Pixel {
    Pixel::new(
        Fonts::carried(),
        &found_with(true, compose),
        CELL,
        SCROLL_CELLS,
    )
}

fn words_in(x: f32, y: f32, text: &str, clip: Rect, underline: bool) -> Paint {
    let Paint::Text(mut run) = words(x, y, text, Role::Foreground.solid()) else {
        unreachable!("words are text")
    };
    run.clip = clip;
    run.underline = underline;
    Paint::Text(run)
}

fn box_in(rect: Rect, fill: Color, radius: f32, clip: Rect) -> Paint {
    Paint::Quad(Quad {
        rect,
        radii: [radius; 4],
        fill: Some(fill),
        gradient: None,
        border: None,
        shadow: None,
        clip,
    })
}

/// What a page of an editor has, and does.
#[derive(Clone, Copy)]
struct Editor {
    scroll: f32,
    /// How many pixels a line is below the one before it.
    row: f32,
    /// Where the room starts: between two pixels, its first line is cut through.
    top: f32,
    /// The room goes down to the foot of the window.
    foot: bool,
    selection: bool,
    caret: bool,
    bar: bool,
    /// Every this many lines has a line under its words (none where 0; every one, and the numbers
    /// beside them, where 1): no line says where the page moved, as it is drawn.
    underlined: usize,
    /// How round the corners of the selection are.
    round: f32,
    /// What the line of the side that stands still says.
    side: u32,
    /// Three lines in six have a ground of their own across the room, to the edge of the window.
    zebra: bool,
    /// The ground and the room go on a hundred pixels below the foot of the window (they are not cut at it).
    over: bool,
    /// The lines are cut a hundred pixels past the right edge of the window.
    wide: bool,
}

impl Editor {
    fn at(scroll: f32, row: f32) -> Self {
        Self {
            scroll,
            row,
            top: 60.0,
            foot: false,
            selection: true,
            caret: true,
            bar: true,
            underlined: 0,
            round: 0.0,
            side: 0,
            zebra: false,
            over: false,
            wide: false,
        }
    }

    fn list(&self) -> DrawList {
        let window = Rect::new(0.0, 0.0, 600.0, 400.0);
        let height = if self.foot {
            400.0 - self.top + if self.over { 100.0 } else { 0.0 }
        } else {
            300.0
        };
        let past = if self.wide { 100.0 } else { 0.0 };
        let room = Rect::new(130.0, self.top, 470.0 + past, height);
        let numbers = Rect::new(130.0, self.top, 40.0, height);
        let text = Rect::new(170.0, self.top, 430.0 + past, height);
        let mut items = vec![
            box_in(
                Rect::new(0.0, 0.0, 600.0, if self.over { 500.0 } else { 400.0 }),
                Role::Background.solid(),
                0.0,
                Rect::new(0.0, 0.0, 600.0, if self.over { 500.0 } else { 400.0 }),
            ),
            box_in(
                Rect::new(0.0, 0.0, 130.0, 400.0),
                Role::Raised(1).solid(),
                0.0,
                window,
            ),
            box_in(
                Rect::new(0.0, 0.0, 600.0, 40.0),
                Role::Raised(2).solid(),
                0.0,
                window,
            ),
            words_in(140.0, 8.0, "a tab strip that stands still", window, false),
            box_in(room, Role::Raised(1).solid(), 0.0, room),
        ];
        for k in 0..8 {
            items.push(words_in(
                8.0,
                50.0 + k as f32 * 40.0,
                &format!("side {k} {}", if k == 3 { self.side } else { 0 }),
                Rect::new(0.0, 0.0, 130.0, 400.0),
                false,
            ));
        }
        for n in 0..200 {
            let y = room.y + 4.0 + n as f32 * self.row - self.scroll;
            if y + self.row <= room.y || y >= room.bottom() {
                continue;
            }
            if self.zebra && n % 6 == 0 {
                items.push(box_in(
                    Rect::new(130.0, y, 470.0, 3.0 * self.row),
                    Role::Raised(3).solid(),
                    0.0,
                    room,
                ));
            }
            if self.selection && n % 5 == 2 {
                items.push(box_in(
                    Rect::new(172.0, y, 140.0, self.row),
                    Role::Selection.solid(),
                    self.round,
                    text,
                ));
            }
            items.push(words_in(
                174.0,
                y,
                &code_line(n),
                text,
                self.underlined != 0 && n % self.underlined == 0,
            ));
            items.push(words_in(
                134.0,
                y,
                &format!("{n:>3}"),
                numbers,
                self.underlined == 1,
            ));
            if self.caret && n == 9 {
                items.push(box_in(
                    Rect::new(300.0, y, 2.0, self.row),
                    Role::Cursor.solid(),
                    0.0,
                    text,
                ));
            }
        }
        if self.bar {
            items.push(box_in(
                Rect::new(592.0, room.y + self.scroll * 0.3, 6.0, 24.0),
                Role::Accent.solid(),
                0.0,
                room,
            ));
        }
        DrawList { items }
    }
}

/// The rows a frame was drawn in, in all.
fn drawn_rows(written: &Written) -> u32 {
    written.bands.iter().map(|band| band.1).sum()
}

#[test]
fn the_rows_a_scroll_copies_are_not_drawn_again_and_the_window_is_the_list_drawn_anew() {
    for (row, by) in [
        (21.6013, 3.0 * 21.6013),
        (20.0, 40.0),
        (17.3571, 2.0 * 17.3571 + 0.4137),
    ] {
        for direction in [1.0, -1.0] {
            let (mut window, mut pixel) = (Window::default(), scroller(true));
            let start = 150.5137;
            send(&mut window, &mut pixel, Editor::at(start, row).list());
            let list = Editor::at(start + direction * by, row).list();
            let written = send(&mut window, &mut pixel, list.clone());
            let what = format!("a tick of {by} pixels at {row} a row, {direction}");
            compare_within(&window, &pixel, &list, &what, 1);
            assert!(!written.whole, "{what}");
            assert!(!written.copies.is_empty(), "{what}: rows are copied");
            // The room is 300 rows, and drawn whole where nothing is known of what moved: what is
            // drawn again is less than that where the lines said it.
            let drawn = drawn_rows(&written);
            assert!(
                drawn < 300,
                "{what}: {drawn} rows drawn again: {:?}",
                written.bands
            );
            assert!(
                written.areas.iter().all(|a| a.h < 300),
                "{what}: {:?}",
                written.areas
            );
        }
    }
}

#[test]
fn a_page_that_scrolls_by_parts_of_pixels_up_and_down_is_the_list_drawn_anew_at_each_frame() {
    let mut seed = 5_u32;
    let mut next = |most: u32| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) % most
    };
    let mut copied_frames = 0;
    for variant in 0..6 {
        let row = [21.6013, 20.0, 17.3571, 24.4137, 19.0, 22.2531][variant];
        let (mut window, mut pixel) = (Window::default(), scroller(true));
        let mut at = 120.0 + next(1000) as f32 / 31.0;
        let flags = |scroll: f32| {
            let mut page = Editor::at(scroll, row);
            page.selection = variant % 2 == 0;
            page.caret = variant % 3 != 1;
            page.bar = variant != 2;
            page.underlined = if variant == 5 { 3 } else { 0 };
            page.top = if variant == 3 { 60.3 } else { 60.0 };
            page.foot = variant == 4;
            page
        };
        send(&mut window, &mut pixel, flags(at).list());
        for step in 0..14 {
            // One tick, or many in one frame, or a part of a line; down or up.
            let by = match next(4) {
                0 => row,
                1 => (1 + next(6)) as f32 * row,
                2 => next(3000) as f32 / 97.0,
                _ => 3.0 * row + next(1000) as f32 / 331.0,
            };
            at = (at + if next(3) == 0 { -by } else { by }).max(0.0);
            let list = flags(at).list();
            let written = send(&mut window, &mut pixel, list.clone());
            copied_frames += usize::from(!written.copies.is_empty());
            compare_within(
                &window,
                &pixel,
                &list,
                &format!("variant {variant}, step {step}, scrolled to {at} at {row} a row"),
                1,
            );
        }
    }
    assert!(copied_frames > 20, "{copied_frames} frames copied rows");
}

#[test]
fn a_frame_in_which_no_line_says_where_the_page_moved_is_made_as_any_other() {
    // Every line, and every number, has a line under its words: where the baseline is rounded is not
    // said, so no rows are found and the rows that changed are drawn (the room, whole), and the rows
    // that are the same are looked for in what was drawn.
    let (mut window, mut pixel) = (Window::default(), scroller(true));
    let page = |scroll: f32| {
        let mut page = Editor::at(scroll, 20.0);
        page.underlined = 1;
        page.selection = false;
        page
    };
    send(&mut window, &mut pixel, page(100.0).list());
    for scroll in [140.0, 100.0, 223.3] {
        let list = page(scroll).list();
        let written = send(&mut window, &mut pixel, list.clone());
        let what = format!("scrolled to {scroll}");
        compare_within(&window, &pixel, &list, &what, 1);
        assert!(drawn_rows(&written) >= 300, "{what}: {:?}", written.bands);
    }
}

#[test]
fn a_page_whose_lines_are_cut_past_the_edge_of_the_window_is_scrolled_by_copying_its_rows() {
    for by in [60.0, 40.0, -40.0] {
        let (mut window, mut pixel) = (Window::default(), scroller(true));
        let page = |scroll: f32| {
            let mut page = Editor::at(scroll, 20.0);
            page.wide = true;
            page.selection = false;
            page.caret = false;
            page.bar = false;
            page
        };
        send(&mut window, &mut pixel, page(160.0).list());
        let list = page(160.0 + by).list();
        let written = send(&mut window, &mut pixel, list.clone());
        let what = format!("scrolled by {by}");
        compare_within(&window, &pixel, &list, &what, 1);
        assert!(drawn_rows(&written) < 300, "{what}: {:?}", written.bands);
    }
}

#[test]
fn a_page_of_lines_that_scrolls_by_whole_rows_draws_only_the_rows_that_came_into_view() {
    let plain = |scroll: f32| {
        let mut page = Editor::at(scroll, 20.0);
        page.selection = false;
        page.caret = false;
        page.bar = false;
        page
    };
    for (from, to) in [
        (160.0, 220.0),
        (220.0, 160.0),
        (200.0, 240.0),
        (260.0, 240.0),
    ] {
        let (mut window, mut pixel) = (Window::default(), scroller(true));
        send(&mut window, &mut pixel, plain(from).list());
        let list = plain(to).list();
        let written = send(&mut window, &mut pixel, list.clone());
        let what = format!("from {from} to {to}");
        compare(&window, &pixel, &list, &what);
        let came = (to - from).abs() as u32;
        let drawn = drawn_rows(&written);
        // The rows that came into view, and no more than a line's rows beside them (a line's
        // reach is a line and a half: its rows by the strip are not known to be the same).
        assert!(
            drawn <= came + 40,
            "{what}: {drawn} rows drawn, {came} came into view: {:?}",
            written.bands
        );
        let rows: u32 = written.copies.iter().map(|c| c.to.h).sum();
        assert!(rows >= 300 - came - 40, "{what}: {rows} rows copied");
    }
}

#[test]
fn a_page_that_scrolls_while_what_stands_beside_it_changes_is_the_list_drawn_anew() {
    for by in [60.0, 40.0, -40.0] {
        let (mut window, mut pixel) = (Window::default(), scroller(true));
        let page = |scroll: f32, side: u32| {
            let mut page = Editor::at(scroll, 20.0);
            page.side = side;
            page.selection = false;
            page
        };
        send(&mut window, &mut pixel, page(160.0, 0).list());
        for (n, at) in [160.0 + by, 160.0 + 2.0 * by].into_iter().enumerate() {
            let list = page(at, n as u32 + 1).list();
            let written = send(&mut window, &mut pixel, list.clone());
            let what = format!("scrolled by {by}, to {at}");
            compare(&window, &pixel, &list, &what);
            // The rows beside the lines that changed are drawn, and the lines' own rows are copied.
            assert!(drawn_rows(&written) < 300, "{what}: {:?}", written.bands);
        }
    }
}

#[test]
fn a_page_that_scrolls_with_lines_across_to_the_edges_of_the_window_keeps_the_edges_of_the_picture()
{
    // Three lines in six have a ground across the room, to the right and (the room goes down to it)
    // the foot of the window: what is past them in the picture is their last column and row.
    for (row, over) in [
        (20.0, false),
        (21.6013, false),
        (20.0, true),
        (21.6013, true),
    ] {
        for by in [2.0 * row, -2.0 * row, 3.0 * row, -3.0 * row, row] {
            let (mut window, mut pixel) = (Window::default(), scroller(true));
            let page = |scroll: f32| {
                let mut page = Editor::at(scroll, row);
                page.over = over;
                page.foot = true;
                page.zebra = true;
                page.selection = false;
                page.caret = false;
                page.bar = false;
                page
            };
            send(&mut window, &mut pixel, page(200.0).list());
            let list = page(200.0 + by).list();
            let written = send(&mut window, &mut pixel, list.clone());
            let what = format!("{row} a row, by {by}, past the foot: {over}");
            compare_within(&window, &pixel, &list, &what, 1);
            assert!(!written.copies.is_empty(), "{what}: rows are copied");
            // The room is 340 rows, and the picture goes on a cell past the foot: where nothing is known
            // of what moved all of them are drawn, and more.
            assert!(drawn_rows(&written) < 340, "{what}: {:?}", written.bands);
        }
    }
}
