//! The page as pixels: what is written for a terminal, read back by a terminal that keeps the window
//! as pixels. The terminal here follows the stream as the graphics protocol says (a picture sent and
//! put, a frame of it edited in place, a picture given up, the cursor that says where a picture is
//! put), and what it shows is compared with the same draw list drawn from the start.

use pencere_drive::shot::{Fonts, Palette, raster};
use pencere_view::{
    Border, Color, DrawList, Family, FieldKind, Font, Id, Measure, Paint, Picture, Point, Quad,
    Rect, Role, Run, Shadow, Size, div, layout,
};

use crate::pixel::{Pixel, Written, column_face};
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
        clip: Rect::new(0.0, 0.0, 800.0, 400.0).into(),
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
        clip: Rect::new(0.0, 0.0, 800.0, 400.0).into(),
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
        clip: Rect::new(0.0, 0.0, 800.0, 400.0).into(),
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
    let across = extent(size).0 as usize;
    let wrong: Vec<usize> = shown
        .chunks(4)
        .zip(want.chunks(4))
        .enumerate()
        .filter(|(_, (a, b))| a.iter().zip(*b).any(|(p, q)| p.abs_diff(*q) > slack))
        .map(|(at, _)| at)
        .collect();
    assert!(
        wrong.is_empty(),
        "{what}: {} pixels of the window are not the list drawn anew, the first at (x, y) {:?}",
        wrong.len(),
        wrong
            .iter()
            .take(6)
            .map(|at| (at % across, at / across))
            .collect::<Vec<_>>()
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

/// A bar across the window (a line that is selected), and under it five lines of six marks each, far
/// apart in their line (a word seen again and again); the bar and the marks take another colour.
fn bar_and_marks(colour: Role) -> DrawList {
    let mut list = page(&lines(), 0.0);
    list.items
        .push(quad(Rect::new(0.0, 40.0, 600.0, 20.0), colour.solid()));
    for n in 0..30_u32 {
        let (x, y) = (160.0 + (n % 6) as f32 * 80.0, 60.0 + (n / 6) as f32 * 20.0);
        list.items
            .push(quad(Rect::new(x, y, 6.0, 20.0), colour.solid()));
    }
    list
}

#[test]
fn small_changes_under_a_wide_one_are_not_sent_as_the_box_that_has_them_all() {
    let (mut window, mut pixel) = (Window::default(), pixel(true));
    send(&mut window, &mut pixel, bar_and_marks(Role::Raised(1)));
    let next = bar_and_marks(Role::Raised(4));
    let written = send(&mut window, &mut pixel, next.clone());
    assert!(!written.whole);
    assert!(
        written.areas.len() <= crate::pixel::MOST_RECTS,
        "{} rectangles",
        written.areas.len()
    );
    // What changed is the bar and the marks; the pixels sent are not many more than those.
    let changed = 600 * 20 + 30 * 6 * 20;
    let sent: u64 = written
        .areas
        .iter()
        .map(|a| u64::from(a.w) * u64::from(a.h))
        .sum();
    assert!(
        sent * 2 <= changed * 3,
        "{sent} pixels sent for {changed} that changed: {:?}",
        written.areas
    );
    compare(&window, &pixel, &next, "a bar and marks under it");
}

#[test]
fn marks_inside_a_bar_that_changes_are_the_bar_and_not_more_rectangles() {
    let (mut window, mut pixel) = (Window::default(), pixel(true));
    let selected = |colour: Role, mark: Role| {
        let mut list = page(&lines(), 0.0);
        list.items
            .push(quad(Rect::new(160.0, 40.0, 400.0, 20.0), colour.solid()));
        for n in 0..10_u32 {
            list.items.push(quad(
                Rect::new(170.0 + n as f32 * 38.0, 46.0, 6.0, 6.0),
                mark.solid(),
            ));
        }
        list
    };
    send(
        &mut window,
        &mut pixel,
        selected(Role::Raised(1), Role::Raised(1)),
    );
    let next = selected(Role::Raised(4), Role::Raised(2));
    let written = send(&mut window, &mut pixel, next.clone());
    assert_eq!(written.areas.len(), 1, "{:?}", written.areas);
    let area = written.areas[0];
    assert!(
        area.x >= 159 && area.right() <= 561 && area.y >= 39 && area.bottom() <= 61,
        "the bar and no more: {area:?}"
    );
    compare(&window, &pixel, &next, "marks inside a bar");
}

#[test]
fn a_line_that_is_selected_is_one_rectangle_with_its_words_and_all() {
    let (mut window, mut pixel) = (Window::default(), pixel(true));
    send(&mut window, &mut pixel, page(&lines(), 0.0));
    let mut selected = page(&lines(), 0.0);
    // The selection is under the words of the line, across the window.
    selected.items.insert(
        4,
        quad(Rect::new(0.0, 200.0, 600.0, 20.0), Role::Raised(4).solid()),
    );
    let written = send(&mut window, &mut pixel, selected.clone());
    assert_eq!(written.areas.len(), 1, "{:?}", written.areas);
    let area = written.areas[0];
    assert!(
        area.y >= 199 && area.bottom() <= 221,
        "the line and no more: {area:?}"
    );
    compare(&window, &pixel, &selected, "a line selected");
}

#[test]
fn one_busy_strip_keeps_the_rectangles_that_the_quiet_ones_leave() {
    // A window of forty rows, so that the strips are many.
    let mut window = Window::default();
    let mut pixel = Pixel::new(Fonts::carried(), &found(true), CELL, (60, 40));
    let frame = |colour: Role| {
        let mut list = page(&lines(), 0.0);
        // Eight marks far apart in the third row of cells, and one mark each in four rows that are
        // far from it and from one another: five strips, one of them busy.
        for n in 0..8_u32 {
            list.items.push(quad(
                Rect::new(10.0 + n as f32 * 70.0, 40.0, 6.0, 20.0),
                colour.solid(),
            ));
        }
        for (row, x) in [(8.0, 300.0), (14.0, 420.0), (20.0, 540.0), (26.0, 160.0)] {
            list.items
                .push(quad(Rect::new(x, row * 20.0, 6.0, 20.0), colour.solid()));
        }
        list
    };
    send(&mut window, &mut pixel, frame(Role::Raised(1)));
    let next = frame(Role::Raised(4));
    let written = send(&mut window, &mut pixel, next.clone());
    assert!(!written.whole);
    let changed = 12 * 6 * 20;
    let sent: u64 = written
        .areas
        .iter()
        .map(|a| u64::from(a.w) * u64::from(a.h))
        .sum();
    assert!(
        sent * 3 <= changed * 4,
        "{sent} pixels sent for {changed} that changed: {:?}",
        written.areas
    );
    compare(&window, &pixel, &next, "one busy strip");
}

#[test]
fn more_pieces_than_can_be_joined_are_no_more_rectangles_than_a_frame_has_and_drawn_as_before() {
    // A mark every 28 pixels along twenty lines of the page: four hundred pieces, in one strip (the
    // strips are more than a frame has).
    let mut window = Window::default();
    let mut pixel = Pixel::new(Fonts::carried(), &found(true), CELL, (60, 40));
    let marks = |colour: Role| {
        let mut list = page(&lines(), 0.0);
        for n in 0..420_u32 {
            let (x, y) = (4.0 + (n % 21) as f32 * 28.0, (n / 21) as f32 * 20.0);
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
    compare(&window, &pixel, &next, "more pieces than are joined");
}

#[test]
fn a_frame_of_very_many_small_changes_is_sent_as_no_more_than_the_most_a_frame_has() {
    let (mut window, mut pixel) = (Window::default(), pixel(true));
    let marks = |colour: Role| {
        let mut list = page(&lines(), 0.0);
        for n in 0..120_u32 {
            let (x, y) = (
                160.0 + (n % 12) as f32 * 36.0,
                64.0 + (n / 12) as f32 * 20.0,
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
    compare(&window, &pixel, &next, "very many small changes");
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

/// The rectangles of a frame that changes many places far apart (a word selected all over the page)
/// are joined by the pairs of them, which are the squares of how many they are: the queue they are
/// in is kept from one frame to the next, and is not asked of the system again by each.
#[test]
fn rectangles_are_joined_in_a_queue_that_is_kept_and_not_asked_of_the_system_again() {
    use crate::alloc_count_tests as large;
    use crate::pixel::{Area, Pair, join};
    let pieces = || -> Vec<Area> {
        (0..100_u32)
            .map(|n| Area {
                x: n % 10 * 60,
                y: n / 10 * 40,
                w: 6,
                h: 6,
            })
            .collect()
    };
    let mut pairs: Vec<Pair> = Vec::new();
    let first = pieces();
    let joined = join(first.clone(), crate::pixel::MOST_RECTS, &mut pairs);
    assert!(joined.len() <= crate::pixel::MOST_RECTS, "{joined:?}");
    // Every piece is in a rectangle that is sent.
    for piece in &first {
        assert!(
            joined.iter().any(|area| area.x <= piece.x
                && area.y <= piece.y
                && piece.right() <= area.right()
                && piece.bottom() <= area.bottom()),
            "{piece:?} is in none of {joined:?}"
        );
    }
    for _ in 0..4 {
        let next = pieces();
        large::reset();
        let again = join(next, crate::pixel::MOST_RECTS, &mut pairs);
        let (count, bytes) = large::large();
        assert_eq!(again, joined);
        assert_eq!(
            count,
            0,
            "asked for memory of {} bytes or more: {count} times, {bytes} bytes",
            large::LARGE
        );
    }
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
    run.clip = clip.into();
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
        clip: clip.into(),
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
        // The rows are found in what is drawn: the room is a part of the window's width (as the
        // columns of what changed are), and a tall rectangle in it is looked for across the picture.
        assert!(
            !written.copies.is_empty(),
            "{what}: the rows that moved are copied"
        );
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

// A character typed changes a line of the page and the status bar, which are bands the width of the
// window and are changed in a part of it: what is drawn is that part.

/// 60 × 20 cells of 10 × 20: a window of 600 × 400 pixels.
const TYPING_CELLS: (u16, u16) = (60, 20);

fn typist(frames: bool, compose: bool) -> Pixel {
    Pixel::new(
        Fonts::carried(),
        &found_with(frames, compose),
        CELL,
        TYPING_CELLS,
    )
}

/// The page a person types into: a tab strip, a side, lines of code with a caret and a selection,
/// and a status bar at the foot.
#[derive(Clone)]
struct Typing {
    /// The words of the line that is typed in (the fifth), from its left.
    line: String,
    /// Where the caret stands in the line, and the selection that ends where it does.
    caret: f32,
    selection: Option<(f32, f32)>,
    /// What the status bar says, and where it starts.
    status: String,
    status_at: f32,
    tab: String,
    /// The typed line is in italics, and as large as this (points).
    italic: bool,
    size: f32,
    /// A card with a shadow that stands over the typed line.
    card: bool,
    /// The ground of the room, and of the status bar across the window.
    ground: Role,
    bar: Role,
    /// A small box drawn last of all, over the room, from this column.
    badge: Option<f32>,
}

impl Typing {
    fn new() -> Self {
        Self {
            line: "let value = 1".to_owned(),
            caret: 174.0 + 13.0 * 9.6,
            selection: None,
            status: "Ln 5, Col 14".to_owned(),
            status_at: 470.0,
            tab: "main.rs".to_owned(),
            italic: false,
            size: 16.0,
            card: false,
            ground: Role::Raised(1),
            bar: Role::Raised(2),
            badge: None,
        }
    }

    fn list(&self) -> DrawList {
        let window = Rect::new(0.0, 0.0, 600.0, 400.0);
        let room = Rect::new(130.0, 40.0, 470.0, 340.0);
        let text = Rect::new(170.0, 40.0, 430.0, 340.0);
        let mut items = vec![
            box_in(window, Role::Background.solid(), 0.0, window),
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
            words_in(140.0, 12.0, &self.tab, window, false),
            box_in(room, self.ground.solid(), 0.0, room),
        ];
        if self.card {
            items.push(Paint::Quad(Quad {
                rect: Rect::new(200.0, 100.0, 120.0, 60.0),
                radii: [6.0; 4],
                fill: Some(Role::Raised(2).solid()),
                gradient: None,
                border: None,
                shadow: Some(Shadow {
                    dx: 0.0,
                    dy: 4.0,
                    blur: 12.0,
                    color: Role::Shadow.alpha(0.5),
                }),
                clip: room.into(),
            }));
        }
        if let Some((from, to)) = self.selection {
            items.push(box_in(
                Rect::new(from, 130.0, to - from, 20.0),
                Role::Selection.solid(),
                3.0,
                text,
            ));
        }
        for k in 0..10_u32 {
            let y = 50.0 + k as f32 * 20.0;
            items.push(words_in(134.0, y, &format!("{k:>3}"), room, false));
            if k == 4 {
                let mut line = words_in(174.0, y, &self.line, text, false);
                if let Paint::Text(run) = &mut line {
                    run.font.italic = self.italic;
                    run.font.size = self.size;
                    if self.italic {
                        // The interface's face is one whose letters lean past their advances.
                        run.font.family = Family::System;
                    }
                }
                items.push(line);
            } else {
                items.push(words_in(174.0, y, &code_line(k as usize), text, false));
            }
        }
        items.push(box_in(
            Rect::new(self.caret, 130.0, 2.0, 20.0),
            Role::Cursor.solid(),
            0.0,
            text,
        ));
        items.push(box_in(
            Rect::new(0.0, 380.0, 600.0, 20.0),
            self.bar.solid(),
            0.0,
            window,
        ));
        items.push(words_in(self.status_at, 381.0, &self.status, window, false));
        if let Some(x) = self.badge {
            items.push(box_in(
                Rect::new(x, 200.0, 30.0, 14.0),
                Role::Accent.solid(),
                4.0,
                room,
            ));
        }
        DrawList { items }
    }
}

/// The pixels the strips of a frame would have if they were as wide as the picture.
fn across_the_picture(written: &Written, pixel: &Pixel) -> u64 {
    let rows: u64 = written.bands.iter().map(|band| u64::from(band.1)).sum();
    rows * u64::from(pixel.extent().0)
}

/// `before` then `after` sent to a terminal of `compose`: what the terminal shows is `after` drawn
/// anew, and what the second frame drew (`rastered`) is less than a `share`th of what strips across
/// the picture are (any, where `share` is 0).
fn typed(before: &Typing, after: &Typing, compose: bool, share: u64, what: &str) -> Written {
    let (mut window, mut pixel) = (Window::default(), typist(true, compose));
    send(&mut window, &mut pixel, before.list());
    let list = after.list();
    let written = send(&mut window, &mut pixel, list.clone());
    compare(&window, &pixel, &list, what);
    assert!(!written.whole, "{what}: the window is not sent whole");
    let full = across_the_picture(&written, &pixel);
    assert!(
        share == 0 || written.rastered * share < full,
        "{what}: {} pixels drawn of {full} across the picture: {:?}",
        written.rastered,
        written.bands
    );
    written
}

#[test]
fn a_character_typed_in_a_line_draws_the_columns_of_the_line_and_not_the_width_of_the_window() {
    let before = Typing::new();
    let mut after = before.clone();
    after.line.push('2');
    after.caret += 9.6;
    after.status = "Ln 5, Col 15".to_owned();
    typed(&before, &after, false, 2, "a character typed");
}

#[test]
fn a_character_typed_where_the_line_goes_to_the_window_edge_has_the_margin_past_the_window() {
    // The line is cut by the room at the window's right: the columns are the line's to the edge, and
    // what is drawn goes on into the margin, the edge of the page repeated.
    let mut before = Typing::new();
    before.line = "x".repeat(50);
    before.caret = 174.0 + 50.0 * 9.6;
    let mut after = before.clone();
    after.line.push('y');
    after.caret += 9.6;
    typed(&before, &after, false, 1, "typed at the edge");
}

#[test]
fn a_status_bar_that_says_another_thing_draws_the_columns_of_what_it_says() {
    let before = Typing::new();
    let mut after = before.clone();
    after.status = "Ln 5, Col 15".to_owned();
    typed(&before, &after, false, 3, "the status changed");
    // Where it says it at the window's edge: the margin and the foot are drawn with it.
    let mut before = Typing::new();
    before.status_at = 520.0;
    let mut after = before.clone();
    after.status = "Ln 5, Col 99".to_owned();
    typed(&before, &after, false, 1, "the status at the edge changed");
}

#[test]
fn a_tab_that_says_another_thing_draws_the_columns_of_the_tab() {
    let before = Typing::new();
    let mut after = before.clone();
    after.tab = "lib.rs".to_owned();
    typed(&before, &after, false, 3, "the tab changed");
}

#[test]
fn a_selection_and_a_caret_that_move_draw_the_columns_they_are_in() {
    let mut before = Typing::new();
    before.selection = Some((174.0, 174.0 + 4.0 * 9.6));
    let mut after = before.clone();
    after.selection = Some((174.0, 174.0 + 8.0 * 9.6));
    after.caret = 174.0 + 8.0 * 9.6;
    typed(&before, &after, false, 2, "the selection grew");
    // It appears and it goes: an item the one list has and the other has not.
    let none = Typing::new();
    // An item in the middle of the list moves the ones after it: they are compared with the ones that
    // stood where they stand now, and what is drawn is the columns of them all.
    typed(&none, &after, false, 1, "the selection appeared");
    typed(&after, &none, false, 1, "the selection went");
}

#[test]
fn an_item_that_the_list_has_more_of_at_its_end_is_drawn_in_its_columns() {
    // It has none of the other list to be compared with: the end of the longer list is what changed.
    let none = Typing::new();
    let mut badge = Typing::new();
    badge.badge = Some(400.0);
    typed(&none, &badge, false, 3, "a badge appeared");
    typed(&badge, &none, false, 3, "a badge went");
    let mut moved = badge.clone();
    moved.badge = Some(333.5);
    typed(&badge, &moved, false, 3, "a badge moved");
}

#[test]
fn italic_letters_that_lean_past_their_advance_are_drawn_to_where_they_lean() {
    // Large, in the interface's italic face, so that the lean is more than the pixel a box's edge
    // has past it: the last letters are f and j, whose tops lean over the advance of what follows
    // them (nothing, at the end).
    let mut before = Typing::new();
    before.italic = true;
    before.size = 36.0;
    before.line = "the final f".to_owned();
    // The caret is out of the way (it is at the line's start), so that what ends the columns is the
    // line's last letter.
    before.caret = 176.0;
    let mut after = before.clone();
    after.line.push_str("jf");
    typed(&before, &after, false, 2, "italics typed");
    let mut shorter = after.clone();
    shorter.line.truncate(shorter.line.len() - 1);
    typed(&after, &shorter, false, 2, "italics deleted");
}

#[test]
fn what_stretches_across_the_window_or_has_a_shadow_is_drawn_across_the_window() {
    let before = Typing::new();
    let mut after = before.clone();
    after.bar = Role::Raised(3);
    let written = typed(&before, &after, false, 0, "the bar changed");
    let full = across_the_picture(&written, &typist(true, false));
    assert_eq!(written.rastered, full, "the bar is as wide as the window");
    // The room is most of the window's width but not all of it: a part, and the edge of the page is
    // past it.
    let mut after = before.clone();
    after.ground = Role::Raised(3);
    typed(&before, &after, false, 0, "the ground changed");
    // A shadow's blur is made over the window's width: a line it is under is drawn as wide.
    let mut under = Typing::new();
    under.card = true;
    let mut after = under.clone();
    after.line.push('2');
    after.caret += 9.6;
    let written = typed(&under, &after, false, 0, "typed under a shadow");
    let full = across_the_picture(&written, &typist(true, false));
    assert_eq!(
        written.rastered, full,
        "a shadow reaches the strip: {:?}",
        written.bands
    );
}

#[test]
fn a_tall_box_that_changes_is_drawn_in_its_columns_where_nothing_says_that_rows_moved() {
    // Its rectangle is high enough to be rows that moved, but no text of the page says it did.
    let before = Typing::new();
    let after = before.clone();
    let bar = |typing: &Typing, color: Role| {
        let mut list = typing.list();
        list.items.push(box_in(
            Rect::new(500.0, 60.0, 8.0, 70.0),
            color.solid(),
            0.0,
            Rect::new(0.0, 0.0, 600.0, 400.0),
        ));
        list
    };
    for compose in [false, true] {
        let (mut window, mut pixel) = (Window::default(), typist(true, compose));
        send(&mut window, &mut pixel, bar(&before, Role::Raised(1)));
        let list = bar(&after, Role::Accent);
        let written = send(&mut window, &mut pixel, list.clone());
        compare(&window, &pixel, &list, "a tall box of another colour");
        assert!(
            written.rastered * 2 < across_the_picture(&written, &pixel),
            "compose {compose}: {} pixels drawn: {:?}",
            written.rastered,
            written.bands
        );
    }
}

#[test]
fn pages_typed_in_for_a_long_time_are_the_list_drawn_anew_at_every_frame() {
    for (frames, compose) in [(true, false), (true, true), (false, false)] {
        let (mut window, mut pixel) = (Window::default(), typist(frames, compose));
        let mut page = Typing::new();
        send(&mut window, &mut pixel, page.list());
        let mut seed = 0x2545_f491_u64;
        let mut next = move |n: u64| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) % n
        };
        for step in 0..120 {
            let what = format!("step {step} (frames {frames}, compose {compose})");
            match next(9) {
                0..=2 => {
                    let chars = ["a", "w", "f", "j", "m", "_", "{", "é"];
                    page.line.push_str(chars[next(8) as usize]);
                    page.caret = 174.0 + page.line.chars().count() as f32 * 9.6;
                }
                3 => {
                    page.line.pop();
                    page.caret = 174.0 + page.line.chars().count() as f32 * 9.6;
                }
                4 => {
                    page.selection = if page.selection.is_some() {
                        None
                    } else {
                        Some((174.0, 174.0 + next(20) as f32 * 9.6 + 3.3))
                    };
                }
                5 => page.status = format!("Ln {}, Col {}", next(500), next(500)),
                6 => page.status_at = 400.0 + next(200) as f32 + 0.37 * next(3) as f32,
                7 => {
                    page.tab = ["main.rs", "lib.rs", "a very long name of a tab.rs"]
                        [next(3) as usize]
                        .to_owned()
                }
                _ => page.italic = !page.italic,
            }
            let list = page.list();
            send(&mut window, &mut pixel, list.clone());
            compare(&window, &pixel, &list, &what);
        }
    }
}

// A part of a page that scrolls (a list beside a page that stands still, a room beside a side that
// does) is drawn in its own columns: what the strips of a scroll leave to draw is not the width of the
// window.

/// A list at the left of a page that stands still, in a window of 600 × 400 pixels: the list is the
/// 130 pixels at the left, and scrolls; what is beside it (a tab strip, lines of text, a status bar)
/// is the same in every frame but where `beside` says another thing.
#[derive(Clone, Copy)]
struct Listing {
    scroll: f32,
    /// How many pixels a row is below the one before it.
    row: f32,
    /// The row whose ground is the selection's.
    selected: Option<usize>,
    /// The list goes down to the foot of the window.
    foot: bool,
    /// The bar at the list's right edge, and where it is is the scroll's.
    bar: bool,
    /// What the line of the page beside the list says: another thing is a change beside it.
    beside: u32,
    /// A part of the page beside the list has a shadow (blurred over the width of the picture).
    shadowed: bool,
    /// A row has a picture (an icon) where it has a square: a list of pictures does not slide by
    /// the pairing of its items (`pencere_drive::shot::slide` pairs only boxes and words).
    icons: bool,
}

impl Listing {
    fn at(scroll: f32, row: f32) -> Self {
        Self {
            scroll,
            row,
            selected: Some(3),
            foot: false,
            bar: true,
            beside: 0,
            shadowed: false,
            icons: false,
        }
    }

    fn list(&self) -> DrawList {
        let window = Rect::new(0.0, 0.0, 600.0, 400.0);
        let height = if self.foot { 360.0 } else { 300.0 };
        let list = Rect::new(0.0, 40.0, 130.0, height);
        let page = Rect::new(130.0, 40.0, 470.0, 360.0);
        let mut items = vec![
            box_in(window, Role::Background.solid(), 0.0, window),
            box_in(
                Rect::new(0.0, 0.0, 600.0, 40.0),
                Role::Raised(2).solid(),
                0.0,
                window,
            ),
            words_in(140.0, 8.0, "a tab strip that stands still", window, false),
            box_in(list, Role::Raised(1).solid(), 0.0, list),
            box_in(page, Role::Raised(2).solid(), 0.0, page),
        ];
        if self.shadowed {
            items.push(Paint::Quad(Quad {
                rect: Rect::new(300.0, 120.0, 120.0, 60.0),
                radii: [6.0; 4],
                fill: Some(Role::Raised(3).solid()),
                gradient: None,
                border: None,
                shadow: Some(Shadow {
                    dx: 0.0,
                    dy: 4.0,
                    blur: 12.0,
                    color: Role::Shadow.alpha(0.5),
                }),
                clip: page.into(),
            }));
        }
        for k in 0..16 {
            items.push(words_in(
                140.0,
                50.0 + k as f32 * 20.0,
                &format!("{} {}", code_line(k), if k == 5 { self.beside } else { 0 }),
                page,
                false,
            ));
        }
        for n in 0..200 {
            let y = list.y + 4.0 + n as f32 * self.row - self.scroll;
            if y + self.row <= list.y || y >= list.bottom() {
                continue;
            }
            if self.selected == Some(n) {
                items.push(box_in(
                    Rect::new(2.0, y, 126.0, self.row),
                    Role::Selection.solid(),
                    0.0,
                    list,
                ));
            }
            // Square: the corners of a rounded box that a clip's edge nearly meets are drawn another
            // way in a strip than in the window (a few levels, which a later whole frame mends:
            // `columns_drawn_alone_are_the_columns_of_the_rows_drawn_across_the_picture`).
            if self.icons {
                let rect = Rect::new(8.0, y + 4.0, 10.0, 10.0);
                items.push(Paint::Image(Picture {
                    src: "icon.svg".to_owned(),
                    page: 1,
                    rect,
                    crop: Rect::new(0.0, 0.0, 1.0, 1.0),
                    shown: Rect::new(0.0, 0.0, 1.0, 1.0),
                    tint: None,
                    opacity: 1.0,
                    radii: [0.0; 4],
                    clip: list.into(),
                    turn: 0.0,
                }));
            } else {
                items.push(box_in(
                    Rect::new(8.0, y + 4.0, 10.0, 10.0),
                    Role::Accent.solid(),
                    0.0,
                    list,
                ));
            }
            items.push(words_in(24.0, y, &format!("file_{n}.rs"), list, false));
        }
        if self.bar {
            items.push(box_in(
                Rect::new(124.0, list.y + self.scroll * 0.3, 6.0, 24.0),
                Role::Accent.solid(),
                0.0,
                list,
            ));
        }
        items.push(words_in(
            140.0,
            380.0,
            &format!("a status bar {}", self.beside),
            window,
            false,
        ));
        DrawList { items }
    }
}

/// `before` then `after`, the second made of the first: what the terminal shows is `after` drawn anew
/// (within `slack`), and the pixels drawn for it (`rastered`) are less than a `share`th of the strips
/// across the picture (any, where `share` is 0).
fn scrolled_listing(
    before: &Listing,
    after: &Listing,
    share: u64,
    slack: u8,
    what: &str,
) -> Written {
    let (mut window, mut pixel) = (Window::default(), scroller(true));
    send(&mut window, &mut pixel, before.list());
    let list = after.list();
    let written = send(&mut window, &mut pixel, list.clone());
    compare_within(&window, &pixel, &list, what, slack);
    assert!(!written.whole, "{what}: the window is not sent whole");
    assert!(!written.copies.is_empty(), "{what}: rows are copied");
    let full = across_the_picture(&written, &pixel);
    assert!(
        share == 0 || written.rastered * share < full,
        "{what}: {} pixels drawn of {full} across the picture: {:?}",
        written.rastered,
        written.bands
    );
    written
}

#[test]
fn a_list_that_scrolls_beside_a_page_that_stands_still_draws_the_columns_of_the_list() {
    for row in [20.0, 21.6013, 17.3571] {
        for by in [row, 3.0 * row, -3.0 * row, 2.0 * row + 0.4137, -40.0] {
            for foot in [false, true] {
                let page = |scroll: f32| {
                    let mut page = Listing::at(scroll, row);
                    page.foot = foot;
                    page
                };
                let what = format!("{row} a row, by {by}, to the foot: {foot}");
                scrolled_listing(&page(200.0), &page(200.0 + by), 3, 1, &what);
            }
        }
    }
}

#[test]
fn a_list_of_pictures_that_scrolls_beside_a_page_that_stands_still_draws_the_columns_of_the_list() {
    // The icons of a tree's rows are pictures: such a list is not slid by the pairing of its items,
    // it is scrolled by the pictures of the frames, and is drawn in the columns that changed.
    for row in [20.0, 21.6013] {
        for by in [row, 3.0 * row, -3.0 * row, 2.0 * row + 0.4137] {
            let page = |scroll: f32| {
                let mut page = Listing::at(scroll, row);
                page.icons = true;
                page
            };
            let what = format!("{row} a row of pictures, by {by}");
            scrolled_listing(&page(200.0), &page(200.0 + by), 3, 1, &what);
        }
    }
}

#[test]
fn a_list_that_scrolls_with_nothing_but_its_lines_in_the_frame_has_no_bar_or_selection_to_draw() {
    let page = |scroll: f32| {
        let mut page = Listing::at(scroll, 20.0);
        page.selected = None;
        page.bar = false;
        page
    };
    for by in [60.0, -60.0, 20.0] {
        let what = format!("by {by}");
        scrolled_listing(&page(200.0), &page(200.0 + by), 4, 0, &what);
    }
}

#[test]
fn a_list_that_scrolls_while_what_is_beside_it_changes_is_the_list_drawn_anew() {
    for foot in [false, true] {
        for by in [60.0, -40.0] {
            let page = |scroll: f32, beside: u32| {
                let mut page = Listing::at(scroll, 20.0);
                page.foot = foot;
                page.beside = beside;
                page
            };
            let what = format!("by {by}, to the foot: {foot}");
            scrolled_listing(&page(200.0, 0), &page(200.0 + by, 1), 0, 1, &what);
        }
    }
}

#[test]
fn a_list_that_scrolls_beside_a_shadow_is_the_list_drawn_anew() {
    let page = |scroll: f32| {
        let mut page = Listing::at(scroll, 20.0);
        page.shadowed = true;
        page
    };
    for by in [60.0, -40.0] {
        let what = format!("by {by}");
        scrolled_listing(&page(200.0), &page(200.0 + by), 0, 1, &what);
    }
}

#[test]
fn a_room_that_scrolls_beside_a_side_draws_the_columns_of_the_room() {
    // The room of the editor (130 to 600 of 600 pixels) is not the width of the window.
    for (row, by) in [(21.6013, 3.0 * 21.6013), (20.0, 40.0), (20.0, -60.0)] {
        let page = |scroll: f32| {
            let mut page = Editor::at(scroll, row);
            page.selection = false;
            page.caret = false;
            page
        };
        let (mut window, mut pixel) = (Window::default(), scroller(true));
        send(&mut window, &mut pixel, page(150.0).list());
        let list = page(150.0 + by).list();
        let written = send(&mut window, &mut pixel, list.clone());
        let what = format!("{row} a row, by {by}");
        compare_within(&window, &pixel, &list, &what, 1);
        assert!(!written.copies.is_empty(), "{what}: rows are copied");
        let full = across_the_picture(&written, &pixel);
        assert!(
            written.rastered * 10 < full * 9,
            "{what}: {} pixels drawn of {full} across the picture: {:?}",
            written.rastered,
            written.bands
        );
    }
}

#[test]
fn lists_scrolled_by_parts_of_pixels_with_the_ground_and_the_bar_moving_are_the_list_drawn_anew() {
    let mut seed = 11_u32;
    let mut next = |most: u32| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) % most
    };
    let (mut narrow, mut across) = (0_u64, 0_u64);
    for variant in 0..6 {
        let row = [21.6013, 20.0, 17.3571, 24.4137, 19.0, 22.2531][variant];
        let (mut window, mut pixel) = (Window::default(), scroller(true));
        let flags = |scroll: f32, selected: usize, beside: u32| {
            let mut page = Listing::at(scroll, row);
            page.selected = (selected != 0).then_some(selected);
            page.bar = variant != 2;
            page.foot = variant % 2 == 1;
            page.beside = beside;
            page.icons = variant >= 3;
            page
        };
        let mut at = 120.0 + next(1000) as f32 / 31.0;
        let mut selected = 3;
        let mut beside = 0;
        send(&mut window, &mut pixel, flags(at, selected, beside).list());
        for step in 0..16 {
            let by = match next(4) {
                0 => row,
                1 => (1 + next(6)) as f32 * row,
                2 => next(3000) as f32 / 97.0,
                _ => 3.0 * row + next(1000) as f32 / 331.0,
            };
            at = (at + if next(3) == 0 { -by } else { by }).max(0.0);
            if next(4) == 0 {
                selected = next(12) as usize;
            }
            if next(6) == 0 {
                beside += 1;
            }
            let list = flags(at, selected, beside).list();
            let written = send(&mut window, &mut pixel, list.clone());
            compare_within(
                &window,
                &pixel,
                &list,
                &format!("variant {variant}, step {step}, scrolled to {at} at {row} a row"),
                1,
            );
            if !written.copies.is_empty() {
                narrow += written.rastered;
                across += across_the_picture(&written, &pixel);
            }
        }
    }
    assert!(
        narrow * 2 < across,
        "{narrow} pixels drawn of {across} across the picture"
    );
}

#[test]
fn a_text_field_is_a_line_of_cells_high_whatever_size_its_text_says() {
    let pixel = pixel(false);
    let scale = pixel.scale();
    // What the terminal draws a field in: its face at the size a column is a cell across.
    let mut fonts = pixel.fonts().clone();
    let face = fonts.measure(&column_face(scale, false), "M");
    let mut measure = pixel.measure();
    let big = Font {
        family: Family::Terminal,
        size: 13.5 * scale,
        weight: 400,
        italic: false,
    };
    assert_eq!(measure.field_line(&big), Some(face.height));
    assert_eq!(measure.field_baseline(&big), Some(face.ascent));
    // A field of one row in 13.5 points is a row of cells high, where the font's own line is not.
    let field = || {
        div()
            .field(Id::new(1), FieldKind::Lines)
            .field_rows(1)
            .text_size(13.5)
            .w(200.0)
    };
    let laid = layout(
        div().child(field()),
        Size::new(600.0, 240.0),
        scale,
        &mut *measure,
    );
    let high = laid.fields()[0].rect.h * scale;
    assert!(
        (high - face.height).abs() <= 0.5,
        "the field is {high} high, a row of cells {}",
        face.height
    );
    assert!(
        (fonts.measure(&big, "M").height - face.height).abs() > 1.0,
        "the 13.5 points line is not the cells' one: {} and {}",
        fonts.measure(&big, "M").height,
        face.height
    );
}
