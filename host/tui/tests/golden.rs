//! What a draw list comes to in the grid, written out by hand: each case is a page of a few boxes and
//! words, and the cells it has to be drawn into.

use pencere_tui::{
    Bg, Cells, Depth, ImageMode, NoPictures, Options, Pictures, Raster, Rgb, Screen, Theme, render,
    text_width,
};
use pencere_view::{
    Color, DrawList, Family, Figure, Font, Gradient, Measure, Paint, Picture, Point, Quad, Rect,
    Rgba, Role, Run, Stroke, path,
};

const COLS: u16 = 12;

fn world() -> Rect {
    Rect::new(-1e6, -1e6, 2e6, 2e6)
}

fn font() -> Font {
    Font {
        family: Family::Terminal,
        size: 13.0,
        weight: 400,
        italic: false,
    }
}

fn quad(rect: Rect, fill: impl Into<Color>) -> Paint {
    Paint::Quad(Quad {
        rect,
        radii: [0.0; 4],
        fill: Some(fill.into()),
        gradient: None,
        border: None,
        shadow: None,
        clip: world(),
        exact: None,
    })
}

fn words(x: f32, y: f32, text: &str, color: impl Into<Color>) -> Paint {
    words_in(x, y, text, color, world())
}

fn words_in(x: f32, y: f32, text: &str, color: impl Into<Color>, clip: Rect) -> Paint {
    Paint::Text(Run {
        origin: Point::new(x, y),
        text: text.to_owned(),
        font: font(),
        color: color.into(),
        opacity: 1.0,
        grounds: Vec::new(),
        clip,
        underline: false,
        strike: false,
    })
}

fn dark() -> Theme {
    Theme::dark()
}

fn draw(items: Vec<Paint>, cols: u16, rows: u16) -> pencere_tui::Frame {
    render(
        &DrawList { items },
        cols,
        rows,
        &dark(),
        &NoPictures,
        &Options::default(),
    )
}

/// The frame as letters: a cell of ground is `.`, one of another colour the letter the key gives it,
/// a half of one `▀` or `▄`, a character itself, a ground the terminal's own `_`.
fn sketch(frame: &pencere_tui::Frame, key: &[(Rgb, char)]) -> String {
    let theme = dark();
    let letter = |color: Rgb| {
        if color == theme.background {
            '.'
        } else {
            key.iter()
                .find(|(rgb, _)| *rgb == color)
                .map_or('?', |(_, letter)| *letter)
        }
    };
    let mut out = String::new();
    for row in frame.grid.rows_of_cells() {
        for cell in row.iter().filter(|cell| !cell.tail) {
            let Bg::Rgb(bg) = cell.bg else {
                out.push('_');
                continue;
            };
            out.push(match cell.ch {
                ' ' => letter(bg),
                '▀' => {
                    let (top, bottom) = (letter(cell.fg), letter(bg));
                    match (top, bottom) {
                        ('.', '.') => '.',
                        ('.', _) => '▄',
                        (_, '.') => '▀',
                        _ => '▒',
                    }
                }
                other => other,
            });
        }
        out.push('\n');
    }
    out
}

fn bg(frame: &pencere_tui::Frame, col: u16, row: u16) -> Bg {
    frame
        .grid
        .cell(col, row)
        .map_or(Bg::Default, |cell| cell.bg)
}

const BOX: Rgb = Rgb::new(21, 25, 31);

#[test]
fn a_box_and_its_words_land_on_the_same_cells() {
    let frame = draw(
        vec![
            quad(Rect::new(2.0, 2.0, 6.0, 4.0), Role::Raised(1)),
            words(3.0, 2.0, "Hi", Role::Foreground),
        ],
        COLS,
        5,
    );
    assert_eq!(
        sketch(&frame, &[(BOX, '#')]),
        "............\n\
         ..#Hi###....\n\
         ..######....\n\
         ............\n\
         ............\n"
    );
}

#[test]
fn an_edge_goes_to_the_nearest_line_and_a_box_that_has_a_size_keeps_a_pixel() {
    let frame = draw(
        vec![
            // 1.5 is rounded away from zero, to 2, and 3.5 to 4.
            quad(Rect::new(1.5, 0.0, 2.0, 2.0), Role::Raised(1)),
            // A hairline an eighth of a pixel tall is a line, an eighth of a cell, at the edge it is
            // nearest.
            quad(Rect::new(0.0, 3.5, 10.0, 0.125), Role::Raised(1)),
        ],
        10,
        3,
    );
    assert_eq!(
        sketch(&frame, &[(BOX, '#')]),
        "..##......\n\
         ▁▁▁▁▁▁▁▁▁▁\n\
         ..........\n"
    );
}

#[test]
fn a_line_across_the_middle_of_a_cell_is_a_line_through_it_and_a_thick_one_a_heavy_line() {
    let frame = draw(
        vec![
            // A cell is 1 across and 2 down. The middle of the first row: a line an eighth of a pixel
            // thick, and the second row's, three tenths of a cell thick (more than an eighth of it).
            quad(Rect::new(0.0, 0.9, 10.0, 0.125), Role::Raised(1)),
            quad(Rect::new(0.0, 2.7, 10.0, 0.6), Role::Raised(1)),
        ],
        10,
        2,
    );
    assert_eq!(
        sketch(&frame, &[(BOX, '#')]),
        "──────────\n\
         ━━━━━━━━━━\n"
    );
}

#[test]
fn a_line_thinner_than_half_a_cell_is_an_eighth_of_one_and_leaves_the_character() {
    let frame = draw(
        vec![
            words(0.0, 0.0, "ab", Role::Foreground),
            // Upright, on the left of the `a`; at the right edge of an empty cell; and lying along
            // the top of the `b`, and of the empty cells of the second row.
            quad(Rect::new(0.2, 0.0, 0.2, 2.0), Role::Accent),
            quad(Rect::new(3.8, 0.0, 0.2, 2.0), Role::Accent),
            quad(Rect::new(1.0, 0.0, 1.0, 0.25), Role::Accent),
            quad(Rect::new(0.0, 2.0, 2.0, 0.25), Role::Accent),
        ],
        4,
        2,
    );
    let accent = dark().role(Role::Accent);
    let cell = |col: u16, row: u16| *frame.grid.cell(col, row).expect("a cell");
    // The line on the `a` turns its cell the other way about: the character shows on its colour.
    let a = cell(0, 0);
    assert_eq!((a.ch, a.bg), ('a', Bg::Rgb(accent)));
    assert_eq!(a.fg, dark().background);
    // One that lies over a character leaves it as it is.
    let b = cell(1, 0);
    assert_eq!((b.ch, b.bg), ('b', Bg::Rgb(dark().background)));
    // In the empty cells it is an eighth of one, in its colour, on the ground.
    let edge = cell(3, 0);
    assert_eq!((edge.ch, edge.fg), ('\u{2595}', accent));
    assert_eq!(cell(0, 1).ch, '\u{2594}');
    assert_eq!(cell(1, 1).ch, '\u{2594}');
}

/// The rows of pixels of `placed` that have some colour in them (opacity above nothing), from the top
/// of its first cell, as `(first, one past the last)`.
fn inked_rows(placed: &pencere_tui::Placement) -> Option<(u32, u32)> {
    let rgba = placed.rgba();
    let across = placed.width as usize * 4;
    let rows: Vec<u32> = rgba
        .chunks_exact(across)
        .enumerate()
        .filter(|(_, row)| row.iter().skip(3).step_by(4).any(|alpha| *alpha > 0))
        .map(|(y, _)| y as u32)
        .collect();
    Some((*rows.first()?, rows.last()? + 1))
}

#[test]
fn a_line_along_words_is_one_picture_over_them_where_the_terminal_draws_pictures() {
    let list = DrawList {
        items: vec![
            words(0.0, 0.0, "ab", Role::Foreground),
            // Along the top of the first row, over the two letters and the two empty cells after them.
            quad(Rect::new(0.0, 0.0, 4.0, 0.25), Role::Accent),
        ],
    };
    let frame = render(&list, 4, 2, &dark(), &NoPictures, &Options::default());
    let over: Vec<_> = frame.placements.iter().filter(|p| p.z > 0).collect();
    assert_eq!(over.len(), 1, "one picture for the line");
    let line = over[0];
    assert_eq!((line.col, line.row, line.cols, line.rows), (0, 0, 4, 1));
    // Whole across the four cells (48 pixels), and its quarter of a row thick (3 of 24), not a cell.
    let rgba = line.rgba();
    let across = line.width as usize;
    assert_eq!(across, 48);
    for x in 0..across {
        assert!(rgba[x * 4 + 3] > 0, "the line is there at pixel {x}");
    }
    assert_eq!(inked_rows(line), Some((0, 3)));
    // The letters are the letters, and no character of a line is in their row.
    assert_eq!(frame.text(), "ab\n\n");
}

#[test]
fn a_line_along_words_is_not_drawn_where_the_terminal_has_no_pictures() {
    let list = DrawList {
        items: vec![
            words(0.0, 0.0, "ab", Role::Foreground),
            quad(Rect::new(0.0, 0.0, 4.0, 0.25), Role::Accent),
            // A line that no word is in the way of is the characters of the edge, as ever.
            quad(Rect::new(0.0, 2.0, 4.0, 0.25), Role::Accent),
        ],
    };
    let options = Options {
        images: ImageMode::Blocks,
        ..Options::default()
    };
    let frame = render(&list, 4, 2, &dark(), &NoPictures, &options);
    assert!(frame.placements.is_empty());
    assert_eq!(frame.text(), "ab\n\u{2594}\u{2594}\u{2594}\u{2594}\n");
}

#[test]
fn a_line_along_words_is_hidden_where_a_box_is_laid_over_it_and_only_there() {
    let line = |over: Paint| {
        let list = DrawList {
            items: vec![
                words(0.0, 0.0, "ab", Role::Foreground),
                quad(Rect::new(0.0, 0.0, 4.0, 0.25), Role::Accent),
                over,
            ],
        };
        render(&list, 4, 2, &dark(), &NoPictures, &Options::default())
    };
    // A box that is solid and covers the cells (a palette, a dialog over the page) takes the line with
    // the words under it: nothing of it is left over the box.
    let covered = line(quad(Rect::new(0.0, 0.0, 4.0, 2.0), Role::Raised(1)));
    assert!(
        covered.placements.iter().all(|p| p.z <= 0),
        "a picture of the line is still over the box: {:?}",
        covered.placements.iter().map(|p| p.z).collect::<Vec<_>>()
    );
    // One over the right half of it leaves the left half: the part of the picture that is over the
    // two cells that are free.
    let half = line(quad(Rect::new(2.0, 0.0, 2.0, 2.0), Role::Raised(1)));
    let left: Vec<_> = half.placements.iter().filter(|p| p.z > 0).collect();
    assert_eq!(left.len(), 1);
    assert_eq!(
        (left[0].col, left[0].cols, left[0].x, left[0].width),
        (0, 2, 0, 24)
    );
    // One beside it leaves the line as it is.
    let beside = line(quad(Rect::new(0.0, 2.0, 4.0, 2.0), Role::Raised(1)));
    assert_eq!(beside.placements.iter().filter(|p| p.z > 0).count(), 1);
}

#[test]
fn a_line_is_cut_by_the_words_that_stay_over_its_cells_and_not_by_those_a_box_hides() {
    // The words of the right half are under a box that is laid over them and the line, and the box has
    // words of its own: of the line only the left half is left, and no word is in its cells.
    let list = DrawList {
        items: vec![
            words(2.0, 0.0, "cd", Role::Foreground),
            quad(Rect::new(0.0, 0.0, 4.0, 0.25), Role::Accent),
            quad(Rect::new(2.0, 0.0, 2.0, 2.0), Role::Raised(1)),
            words(2.0, 0.0, "ef", Role::Foreground),
        ],
    };
    for images in [ImageMode::Kitty, ImageMode::Blocks] {
        let options = Options {
            images,
            ..Options::default()
        };
        let frame = render(&list, 4, 2, &dark(), &NoPictures, &options);
        assert_eq!(
            frame.text(),
            "\u{2594}\u{2594}ef\n\n",
            "the line is the characters of its two free cells ({images:?})"
        );
        assert!(
            frame.placements.iter().all(|p| p.z <= 0),
            "no picture of the line ({images:?})"
        );
    }
    // The words that are in the free cells are the line's to run over, as ever: one picture where the
    // terminal has them, nothing where it has none.
    let list = DrawList {
        items: vec![
            words(0.0, 0.0, "ab", Role::Foreground),
            quad(Rect::new(0.0, 0.0, 4.0, 0.25), Role::Accent),
            quad(Rect::new(2.0, 0.0, 2.0, 2.0), Role::Raised(1)),
            words(2.0, 0.0, "ef", Role::Foreground),
        ],
    };
    let frame = render(&list, 4, 2, &dark(), &NoPictures, &Options::default());
    let over: Vec<_> = frame.placements.iter().filter(|p| p.z > 0).collect();
    assert_eq!(over.len(), 1);
    assert_eq!((over[0].col, over[0].cols), (0, 2));
    assert_eq!(frame.text(), "abef\n\n");
    let blocks = Options {
        images: ImageMode::Blocks,
        ..Options::default()
    };
    let frame = render(&list, 4, 2, &dark(), &NoPictures, &blocks);
    assert!(frame.placements.is_empty());
    assert_eq!(frame.text(), "abef\n\n");
}

#[test]
fn a_line_drawn_over_another_is_over_it_whether_the_words_cut_the_first_or_not() {
    // A gauge: a track across four cells, and a fill over the first of them in another colour. The track
    // has words in its cells (the right half, under a box that is laid over them later), the fill has
    // none; the fill is the later, so it is the one to be seen where they meet.
    let list = DrawList {
        items: vec![
            words(2.0, 0.0, "cd", Role::Foreground),
            quad(Rect::new(0.0, 0.0, 4.0, 0.25), Role::Accent),
            quad(Rect::new(2.0, 0.0, 2.0, 2.0), Role::Raised(1)),
            words(2.0, 0.0, "ef", Role::Foreground),
            quad(Rect::new(0.0, 0.0, 1.0, 0.25), Role::Foreground),
        ],
    };
    for images in [ImageMode::Kitty, ImageMode::Blocks] {
        let options = Options {
            images,
            ..Options::default()
        };
        let frame = render(&list, 4, 2, &dark(), &NoPictures, &options);
        let cell = |col: u16| *frame.grid.cell(col, 0).expect("a cell");
        assert_eq!(
            (cell(0).ch, cell(0).fg),
            ('\u{2594}', dark().role(Role::Foreground)),
            "the fill is where it was put ({images:?})"
        );
        assert_eq!(
            (cell(1).ch, cell(1).fg),
            ('\u{2594}', dark().role(Role::Accent)),
            "the track is the rest of the line ({images:?})"
        );
    }
}

#[test]
fn a_line_on_a_box_is_not_cut_by_the_words_the_box_was_laid_over() {
    // Words, a box over them (a palette over the page), and a rule on the box: the words are under it,
    // and the rule is the characters of the edge, with or without pictures.
    let list = DrawList {
        items: vec![
            words(0.0, 0.0, "abcd", Role::Foreground),
            quad(Rect::new(0.0, 0.0, 4.0, 2.0), Role::Raised(1)),
            quad(Rect::new(0.0, 0.0, 4.0, 0.25), Role::Accent),
        ],
    };
    for images in [ImageMode::Kitty, ImageMode::Blocks] {
        let options = Options {
            images,
            ..Options::default()
        };
        let frame = render(&list, 4, 2, &dark(), &NoPictures, &options);
        assert_eq!(
            frame.text(),
            "\u{2594}\u{2594}\u{2594}\u{2594}\n\n",
            "the rule on the box ({images:?})"
        );
        assert!(
            frame.placements.iter().all(|p| p.z <= 0),
            "no picture of the rule ({images:?})"
        );
    }
}

#[test]
fn what_is_laid_over_a_picture_is_laid_where_the_terminal_draws_that_part_of_it() {
    // A picture whose edge is not a cell's: the terminal draws it where the page has it, to the pixel
    // (over the cells it touches, which are made over to hold it), so the part laid over it is where
    // the page has it too.
    let faint = Color::from(Role::Accent).faded(0.3);
    let list = DrawList {
        items: vec![
            picture(Rect::new(0.0, 0.6, 4.0, 4.0), None),
            quad(Rect::new(1.0, 1.6, 2.0, 2.0), faint),
        ],
    };
    let frame = render(&list, 4, 3, &dark(), &One(red(1, 1)), &Options::default());
    let mark = frame
        .placements
        .iter()
        .find(|placed| placed.z == -1)
        .expect("a picture over the picture");
    // A point of the picture 1.0 down it is at 1.6 of the grid, which is 19 pixels down (12 a unit):
    // the terminal draws that point there.
    let (top, bottom) = inked_rows(mark).expect("it has colour");
    let (_, down) = mark
        .offset
        .expect("it is its own pixels, put at one of a cell");
    let first = u32::from(mark.row) * 24 + down + top;
    assert_eq!(first, 19, "its top, in the terminal's pixels");
    assert_eq!(u32::from(mark.row) * 24 + down + bottom, 43, "its bottom");
}

#[test]
fn a_character_that_is_two_columns_wide_takes_them() {
    let frame = draw(vec![words(0.0, 0.0, "a漢b", Role::Foreground)], 6, 1);
    assert_eq!(frame.text(), "a漢b\n");
    assert_eq!(text_width("a漢b"), 4);
    let tail = frame.grid.cell(2, 0).unwrap();
    assert!(tail.tail);
    assert_eq!(frame.grid.cell(3, 0).unwrap().ch, 'b');
}

#[test]
fn what_is_clipped_is_clipped_at_the_rounded_edge() {
    // The clip ends between the two columns of 漢: the wide character is not whole inside it.
    let frame = draw(
        vec![words_in(
            0.0,
            0.0,
            "a漢b",
            Role::Foreground,
            Rect::new(0.0, 0.0, 2.0, 2.0),
        )],
        6,
        1,
    );
    assert_eq!(frame.text(), "a\n");
    // A row is in a clip that holds its middle: the one below the clip is not.
    let frame = draw(
        vec![
            words(0.0, 0.0, "top", Role::Foreground),
            words_in(
                0.0,
                2.0,
                "bottom",
                Role::Foreground,
                Rect::new(0.0, 0.0, 10.0, 2.0),
            ),
        ],
        10,
        2,
    );
    assert_eq!(frame.text(), "top\n\n");
}

#[test]
fn a_role_is_the_themes_and_a_value_the_form_that_goes_with_its_brightness() {
    let value = Rgba::new(255, 0, 0, 255).or_dark(Rgba::new(0, 255, 0, 255));
    let list = DrawList {
        items: vec![
            quad(Rect::new(0.0, 0.0, 1.0, 2.0), Role::Accent),
            quad(Rect::new(1.0, 0.0, 1.0, 2.0), value),
            quad(Rect::new(2.0, 0.0, 1.0, 2.0), Role::Raised(1)),
            // Past the eighth step a surface is the eighth.
            quad(Rect::new(3.0, 0.0, 1.0, 2.0), Role::Raised(100)),
            quad(Rect::new(4.0, 0.0, 1.0, 2.0), Role::Quiet),
            quad(Rect::new(5.0, 0.0, 1.0, 2.0), Role::Border),
            quad(Rect::new(6.0, 0.0, 1.0, 2.0), Role::Red),
        ],
    };
    let dark = render(
        &list,
        7,
        1,
        &Theme::dark(),
        &NoPictures,
        &Options::default(),
    );
    let want = [
        Rgb::hex(0x58a6ff),
        Rgb::new(0, 255, 0),
        BOX,
        Rgb::new(73, 78, 85),
        // The dim text is the text a third of the way to the ground, (138, 145, 152), and the quiet
        // text that a third again.
        Rgb::new(96, 102, 109),
        // A fifth of the way from the ground to the text.
        Rgb::new(51, 55, 62),
        Rgb::hex(0xff7b72),
    ];
    for (col, want) in want.into_iter().enumerate() {
        assert_eq!(
            bg(&dark, col as u16, 0),
            Bg::Rgb(want),
            "dark, column {col}"
        );
    }
    let light = render(
        &list,
        7,
        1,
        &Theme::light(),
        &NoPictures,
        &Options::default(),
    );
    assert_eq!(bg(&light, 1, 0), Bg::Rgb(Rgb::new(255, 0, 0)));
    // On a light theme a surface is darker than the ground.
    assert_eq!(bg(&light, 2, 0), Bg::Rgb(Rgb::new(246, 246, 246)));
}

#[test]
fn what_is_not_solid_is_laid_over_what_is_under_it() {
    let frame = draw(
        vec![
            quad(Rect::new(0.0, 0.0, 4.0, 2.0), Role::Foreground.alpha(0.5)),
            words(0.0, 0.0, "ab", Role::Foreground.alpha(0.5)),
        ],
        4,
        1,
    );
    let half = Rgb::new(107, 113, 120);
    assert_eq!(bg(&frame, 3, 0), Bg::Rgb(half));
    // The words are half of the foreground laid over the ground they stand on, which is the half
    // already.
    let cell = frame.grid.cell(0, 0).unwrap();
    assert_eq!(cell.ch, 'a');
    assert_eq!(cell.fg, half.mix(Rgb::hex(0xc9d1d9), 0.5));
}

#[test]
fn a_gradient_is_one_colour_to_a_pixel() {
    let list = DrawList {
        items: vec![Paint::Quad(Quad {
            rect: Rect::new(0.0, 0.0, 4.0, 2.0),
            radii: [0.0; 4],
            fill: None,
            gradient: Some(Gradient {
                angle: 90.0,
                from: Rgba::new(0, 0, 0, 255).solid(),
                to: Rgba::new(100, 100, 100, 255).solid(),
            }),
            border: None,
            shadow: None,
            clip: world(),
            exact: None,
        })],
    };
    let frame = render(&list, 4, 1, &dark(), &NoPictures, &Options::default());
    // The middle of each column is an eighth, three eighths, five and seven of the way along: 12.5,
    // 37.5, 62.5 and 87.5 of a hundred, rounded away from zero.
    for (col, want) in [13, 38, 63, 88].into_iter().enumerate() {
        assert_eq!(
            bg(&frame, col as u16, 0),
            Bg::Rgb(Rgb::new(want, want, want)),
            "column {col}"
        );
    }
}

#[test]
fn a_border_is_a_pixel_wide_inside_the_box() {
    let list = DrawList {
        items: vec![Paint::Quad(Quad {
            rect: Rect::new(0.0, 0.0, 6.0, 4.0),
            radii: [0.0; 4],
            fill: None,
            gradient: None,
            border: Some(pencere_view::Border {
                width: 1.0,
                color: Role::Accent.solid(),
            }),
            shadow: None,
            clip: world(),
            exact: None,
        })],
    };
    let frame = render(&list, 6, 2, &dark(), &NoPictures, &Options::default());
    let line = Rgb::hex(0x58a6ff);
    assert_eq!(
        sketch(&frame, &[(line, 'B')]),
        "B▀▀▀▀B\n\
         B▄▄▄▄B\n"
    );
}

/// `items` drawn in cells of 8 by 18 points, as a page laid out on the cells has them.
fn draw_cells(items: Vec<Paint>, cols: u16, rows: u16) -> pencere_tui::Frame {
    let options = Options {
        col: 8.0,
        row: 18.0,
        ..Options::default()
    };
    render(
        &DrawList { items },
        cols,
        rows,
        &dark(),
        &NoPictures,
        &options,
    )
}

/// A box of `cols` by `rows` cells at (`col`, `row`) with a border of a point all round.
fn bordered(
    (col, row): (u16, u16),
    (cols, rows): (u16, u16),
    radius: f32,
    border: impl Into<Color>,
) -> Paint {
    Paint::Quad(Quad {
        rect: Rect::new(
            f32::from(col) * 8.0,
            f32::from(row) * 18.0,
            f32::from(cols) * 8.0,
            f32::from(rows) * 18.0,
        ),
        radii: [radius; 4],
        fill: Some(Role::Raised(1).into()),
        gradient: None,
        border: Some(pencere_view::Border {
            width: 1.0,
            color: border.into(),
        }),
        shadow: None,
        clip: world(),
        exact: None,
    })
}

#[test]
fn a_box_with_a_border_on_every_side_is_a_ring_of_line_characters() {
    // Room inside (a cell of it before the words): the ring closes at the corners, rounded where the
    // box is and square where it is not, in the colour of the border on the ground of the box.
    let list = |radius| {
        vec![
            bordered((0, 0), (6, 3), radius, Role::Accent),
            words(16.0, 18.0, "Hi", Role::Foreground),
        ]
    };
    let frame = draw_cells(list(4.0), 6, 3);
    assert_eq!(frame.text(), "╭────╮\n│ Hi │\n╰────╯\n");
    let square = draw_cells(list(0.0), 6, 3);
    assert_eq!(square.text(), "┌────┐\n│ Hi │\n└────┘\n");
    for (col, row) in [(0, 0), (3, 0), (5, 0), (0, 1), (5, 1), (0, 2), (5, 2)] {
        let cell = frame.grid.cell(col, row).expect("a cell");
        assert_eq!(cell.fg, Rgb::hex(0x58a6ff), "{col}, {row}");
        assert_eq!(cell.bg, Bg::Rgb(BOX), "{col}, {row}");
    }
    // The inside is the box's ground, and the ring holds nothing it was not given.
    assert_eq!(bg(&frame, 2, 1), Bg::Rgb(BOX));
}

#[test]
fn the_ring_of_a_box_is_the_border_with_its_opacity_and_none_where_it_has_none() {
    let line = |alpha: f32| {
        draw_cells(
            vec![bordered(
                (0, 0),
                (4, 3),
                4.0,
                Color::from(Role::Accent).faded(alpha),
            )],
            4,
            3,
        )
    };
    // Faint over the hover, firm on the one chosen: two colours that tell them apart.
    let (faint, firm) = (line(0.5), line(0.9));
    let color = |frame: &pencere_tui::Frame| frame.grid.cell(0, 0).expect("a cell").fg;
    assert_eq!(color(&faint), BOX.mix(Rgb::hex(0x58a6ff), 0.5));
    assert_eq!(color(&firm), BOX.mix(Rgb::hex(0x58a6ff), 0.9));
    assert_ne!(color(&faint), color(&firm));
    // A border that is not there leaves no character.
    assert_eq!(line(0.0).text(), "\n\n\n");
}

#[test]
fn two_cards_side_by_side_leave_the_column_between_them_alone() {
    let frame = draw_cells(
        vec![
            bordered((0, 0), (4, 3), 4.0, Role::Accent),
            bordered((5, 0), (4, 3), 4.0, Role::Accent),
        ],
        9,
        3,
    );
    assert_eq!(frame.text(), "╭──╮ ╭──╮\n│  │ │  │\n╰──╯ ╰──╯\n");
}

/// A box of `cols` by `rows` cells at (`col`, `row`) with a border of a point all round, and the
/// fill it is given.
fn framed(
    (col, row): (u16, u16),
    (cols, rows): (u16, u16),
    fill: Option<Color>,
    border: impl Into<Color>,
) -> Paint {
    Paint::Quad(Quad {
        rect: Rect::new(
            f32::from(col) * 8.0,
            f32::from(row) * 18.0,
            f32::from(cols) * 8.0,
            f32::from(rows) * 18.0,
        ),
        radii: [4.0; 4],
        fill,
        gradient: None,
        border: Some(pencere_view::Border {
            width: 1.0,
            color: border.into(),
        }),
        shadow: None,
        clip: world(),
        exact: None,
    })
}

/// The ground of a cell, as a colour.
fn ground(frame: &pencere_tui::Frame, col: u16, row: u16) -> Rgb {
    match bg(frame, col, row) {
        Bg::Rgb(rgb) => rgb,
        // Not a colour: one that no ground of a test is.
        _ => Rgb::new(255, 0, 255),
    }
}

/// No character of a line along the edge of a cell.
fn no_part_of_a_line(text: &str) -> bool {
    !text.contains(['\u{258f}', '\u{2595}', '\u{2581}', '\u{2594}'])
}

#[test]
fn a_box_with_words_in_its_edge_is_a_bar_of_the_tone_of_its_border() {
    // A field of a row, its words in the cells its ring would stand in: no part of a line, and no
    // gap in a frame; the box is a bar, and the border is the tone it is.
    let theme = dark();
    let border = theme.role(Role::Border);
    let faint = Color::from(Role::Foreground).faded(0.05);
    let bar = |fill: Option<Color>, border: Color| {
        draw_cells(
            vec![
                framed((0, 0), (6, 1), fill, border),
                words(0.0, 0.0, "abcd", Role::Foreground),
            ],
            6,
            1,
        )
    };
    // A fill the ground does not tell apart (a twentieth of the way to the foreground): the bar is
    // the fill with a third of the border's tone over it.
    let frame = bar(Some(faint), Role::Border.into());
    assert_eq!(frame.text(), "abcd\n");
    let want = theme
        .background
        .mix(theme.role(Role::Foreground), 0.05)
        .mix(border, 0.3);
    for col in 0..6 {
        assert_eq!(ground(&frame, col, 0), want, "{col}");
    }
    // None at all: the ground, with the tone laid over it.
    let frame = bar(None, Role::Border.into());
    assert_eq!(ground(&frame, 3, 0), theme.background.mix(border, 0.3));
    // A fill that does tell it apart (two steps up): that, with a little of the tone.
    let frame = bar(Some(Role::Raised(2).into()), Role::Border.into());
    assert_eq!(
        ground(&frame, 3, 0),
        theme.role(Role::Raised(2)).mix(border, 0.1)
    );
    // A border nobody sees leaves the fill, and no line.
    let frame = bar(Some(faint), Color::from(Role::Accent).faded(0.0));
    assert_eq!(frame.text(), "abcd\n");
    assert_eq!(
        ground(&frame, 3, 0),
        theme.background.mix(theme.role(Role::Foreground), 0.05)
    );
}

#[test]
fn a_bar_is_told_from_the_ground_in_256_colours_and_a_field_in_focus_from_one_that_is_not() {
    for theme in [Theme::dark(), Theme::light()] {
        let draw_bar = |border: Color| {
            render(
                &DrawList {
                    items: vec![framed(
                        (0, 0),
                        (6, 1),
                        Some(Color::from(Role::Foreground).faded(0.05)),
                        border,
                    )],
                },
                6,
                1,
                &theme,
                &NoPictures,
                &Options {
                    col: 8.0,
                    row: 18.0,
                    ..Options::default()
                },
            )
        };
        let idle = ground(&draw_bar(Role::Border.into()), 2, 0);
        let focus = ground(&draw_bar(Color::from(Role::Accent).faded(0.8)), 2, 0);
        // The bar is not the page's ground, in the 256 of the palette as in true colour, and the
        // focus is not the idle bar in either.
        assert_ne!(idle.to_indexed(), theme.background.to_indexed());
        assert_ne!(focus.to_indexed(), idle.to_indexed());
        assert_ne!(focus, idle);
    }
}

#[test]
fn a_box_of_rows_with_words_in_its_edge_has_no_part_of_a_line_and_one_ground() {
    // A panel whose last row is words, as the palette's: no ring fits, and it is not lines either.
    let frame = draw_cells(
        vec![
            framed((0, 0), (8, 3), Some(Role::Raised(2).into()), Role::Border),
            words(8.0, 0.0, "Files", Role::Foreground),
            words(8.0, 36.0, "Esc close", Role::Foreground),
        ],
        8,
        3,
    );
    assert!(no_part_of_a_line(&frame.text()), "{:?}", frame.text());
    assert_eq!(frame.text(), " Files\n\n Esc clo\n");
    let corner = ground(&frame, 0, 0);
    for (col, row) in [(7, 0), (0, 2), (7, 2), (3, 1)] {
        assert_eq!(ground(&frame, col, row), corner, "{col}, {row}");
    }
}

#[test]
fn a_line_on_one_side_is_a_line_along_the_edge() {
    // The underline of a tab, the bar of a change: a fill a point thick, as before.
    let frame = draw_cells(
        vec![quad(Rect::new(0.0, 35.0, 48.0, 1.0), Role::Accent)],
        6,
        2,
    );
    assert_eq!(frame.text(), "\n▁▁▁▁▁▁\n");
}

#[test]
fn a_character_stands_on_the_ground_of_the_box_in_front() {
    // The box begins in the lower half of the row the words are in.
    let frame = draw(
        vec![
            quad(Rect::new(0.0, 1.0, 6.0, 3.0), Role::Raised(1)),
            words(1.0, 0.0, "ab", Role::Foreground),
        ],
        6,
        2,
    );
    assert_eq!(bg(&frame, 1, 0), Bg::Rgb(BOX));
    assert_eq!(frame.stats.text_cells, 2);
    assert_eq!(frame.stats.split, 2);
}

#[test]
fn what_is_drawn_in_front_covers_the_words_behind() {
    let frame = draw(
        vec![
            words(0.0, 0.0, "abc", Role::Foreground),
            quad(Rect::new(1.0, 0.0, 1.0, 2.0), Role::Raised(1)),
        ],
        4,
        1,
    );
    assert_eq!(frame.text(), "a c\n");
}

#[test]
fn an_edge_in_the_middle_of_a_cell_leaves_the_word_in_it() {
    // The box in front starts at the lower half of the cell of the word: a rounding, not a cover.
    let frame = draw(
        vec![
            words(0.0, 0.0, "abc", Role::Foreground),
            quad(Rect::new(1.0, 1.0, 1.0, 1.0), Role::Raised(1)),
        ],
        4,
        1,
    );
    assert_eq!(frame.text(), "abc\n");
}

struct One(Raster);

impl Pictures for One {
    fn raster(&self, _picture: &Picture) -> Option<Raster> {
        Some(self.0.clone())
    }
}

fn picture(rect: Rect, tint: Option<Color>) -> Paint {
    Paint::Image(Picture {
        src: "a.png".to_owned(),
        page: 1,
        rect,
        cells: rect,
        crop: Rect::new(0.0, 0.0, 1.0, 1.0),
        shown: Rect::new(0.0, 0.0, 1.0, 1.0),
        tint,
        opacity: 1.0,
        radii: [0.0; 4],
        clip: world(),
        turn: 0.0,
        exact: None,
    })
}

fn red(width: u32, height: u32) -> Raster {
    Raster {
        id: None,
        width,
        height,
        pixels: [255, 0, 0, 255].repeat((width * height) as usize),
        crop: Rect::new(0.0, 0.0, 1.0, 1.0),
    }
}

#[test]
fn a_picture_is_placed_in_the_cells_of_its_box_and_the_cells_are_the_terminals() {
    let list = DrawList {
        items: vec![picture(Rect::new(2.0, 2.0, 8.0, 6.0), None)],
    };
    let frame = render(
        &list,
        COLS,
        5,
        &dark(),
        &One(red(4, 4)),
        &Options::default(),
    );
    assert_eq!(frame.placements.len(), 1);
    let placed = &frame.placements[0];
    assert_eq!(
        (placed.col, placed.row, placed.cols, placed.rows),
        (2, 1, 8, 3)
    );
    assert_eq!((placed.width, placed.height), (4, 4));
    assert_eq!(placed.rgba().get(..4), Some(&[255, 0, 0, 255][..]));
    // It covers its cells altogether: it is drawn under their grounds, which are the terminal's.
    assert_eq!(placed.z, -1_073_741_825);
    assert_eq!(bg(&frame, 2, 1), Bg::Default);
    assert_eq!(bg(&frame, 9, 3), Bg::Default);
    assert_eq!(bg(&frame, 1, 1), Bg::Rgb(dark().background));
    assert_eq!(bg(&frame, 10, 1), Bg::Rgb(dark().background));
    assert!(!frame.waiting);
}

/// What the pixels of a placement say about where its red is: how many rows and columns have some.
fn red_across(placed: &pencere_tui::Placement) -> (u32, u32) {
    let (width, height) = (placed.width as usize, placed.height as usize);
    let pixels = placed.rgba();
    let is_red = |x: usize, y: usize| {
        pixels.get((y * width + x) * 4..(y * width + x) * 4 + 4) == Some(&[255, 0, 0, 255][..])
    };
    let rows = (0..height)
        .filter(|&y| (0..width).any(|x| is_red(x, y)))
        .count();
    let columns = (0..width)
        .filter(|&x| (0..height).any(|y| is_red(x, y)))
        .count();
    (columns as u32, rows as u32)
}

/// Red pixels as many as the picture's rectangle is wide and high, as the sandbox's are for the
/// size a picture is drawn at.
struct RedAsked;

impl Pictures for RedAsked {
    fn raster(&self, picture: &Picture) -> Option<Raster> {
        let width = picture.rect.w.round().max(1.0) as u32;
        let height = picture.rect.h.round().max(1.0) as u32;
        Some(red(width, height))
    }
}

#[test]
fn a_square_picture_is_a_square_on_the_pixels_of_a_terminal_whose_cell_is_not_one_to_two() {
    // The page is laid out in a cell of one by two; this terminal's is nine pixels by twenty, so the
    // box of twelve by twelve (twelve columns, six rows) is 108 by 120 of its pixels.
    let options = Options {
        pixels: (9, 20),
        ..Options::default()
    };
    let list = DrawList {
        items: vec![picture(Rect::new(0.0, 0.0, 12.0, 12.0), None)],
    };
    let frame = render(&list, 12, 6, &dark(), &RedAsked, &options);
    let (under, over): (Vec<_>, Vec<_>) = frame
        .placements
        .iter()
        .partition(|placed| placed.z < -1_073_741_825);
    let [placed] = &over[..] else {
        panic!("one picture: {:?}", frame.placements.len());
    };
    // The picture is 108 across and 108 down, put 6 pixels down the first of the six rows it
    // touches: its pixels as they are, and the ground that was behind its box under what the first
    // and the last row have of the rest.
    assert_eq!(
        (placed.col, placed.row, placed.cols, placed.rows),
        (0, 0, 12, 6)
    );
    assert_eq!(placed.offset, Some((0, 6)));
    assert_eq!((placed.width, placed.height), (108, 108));
    assert_eq!(red_across(placed), (108, 108));
    assert_eq!(placed.z, -1_073_741_825);
    let rows: Vec<_> = under
        .iter()
        .map(|ground| (ground.col, ground.row, ground.cols, ground.rows))
        .collect();
    assert_eq!(
        rows,
        vec![(0, 0, 12, 1), (0, 5, 12, 1)],
        "the colour under the cells it covers a part of"
    );
    // A picture that fills its box (cut at its sides) fills all of it: it is on the cells, whole.
    let mut cover = picture(Rect::new(0.0, 0.0, 12.0, 12.0), None);
    if let Paint::Image(picture) = &mut cover {
        picture.crop = Rect::new(0.0, 0.25, 1.0, 0.5);
        picture.shown = picture.crop;
    }
    let list = DrawList { items: vec![cover] };
    let frame = render(&list, 12, 6, &dark(), &RedAsked, &options);
    let [placed] = &frame.placements[..] else {
        panic!("one picture, and nothing under it: {:?}", frame.placements);
    };
    assert_eq!(
        (placed.col, placed.row, placed.cols, placed.rows),
        (0, 0, 12, 6)
    );
    assert_eq!(placed.offset, Some((0, 0)));
    assert_eq!((placed.width, placed.height), (108, 120));
    assert!(
        placed
            .rgba()
            .chunks(4)
            .all(|pixel| pixel == [255, 0, 0, 255]),
        "it covers its cells and leaves none of them bare"
    );
}

#[test]
fn an_icon_is_stretched_over_the_cells_it_is_nearest_and_a_picture_on_the_cells_is_sent_as_it_is() {
    // A terminal with the page's own proportions and a picture that is on its cells: nothing is made
    // over, the picture's pixels go as they are.
    let options = Options {
        pixels: (10, 20),
        ..Options::default()
    };
    let list = DrawList {
        items: vec![picture(Rect::new(2.0, 2.0, 8.0, 6.0), None)],
    };
    let frame = render(&list, COLS, 5, &dark(), &One(red(4, 4)), &options);
    let [placed] = &frame.placements[..] else {
        panic!("one picture");
    };
    assert_eq!((placed.width, placed.height), (4, 4));
    // An icon is not moved to a place that its pixels would say: its cells are the nearest to its box,
    // and it fills them.
    let list = DrawList {
        items: vec![picture(
            Rect::new(1.0, 0.0, 1.5, 1.6),
            Some(Role::Accent.solid()),
        )],
    };
    let frame = render(&list, 4, 1, &dark(), &One(red(16, 16)), &options);
    let [placed] = &frame.placements[..] else {
        panic!("one icon");
    };
    assert_eq!(
        (placed.col, placed.row, placed.cols, placed.rows),
        (1, 0, 2, 1)
    );
    assert_eq!((placed.width, placed.height), (16, 16));
}

#[test]
fn an_icon_the_page_wrote_a_place_for_is_drawn_there_on_the_pixels_of_the_terminal_and_not_stretched()
 {
    // The cells of the icon are two columns and a row (16 by 18 pixels of the terminal's 10 by 20 on the
    // page's 1 by 2), and the page wrote it 1.5 by 1.5 in the middle of them: it is 15 by 15 pixels of
    // the terminal, at the pixel the layout has it at, and not 20 by 20 over the cells.
    let options = Options {
        pixels: (10, 20),
        ..Options::default()
    };
    let Paint::Image(mut icon) = picture(Rect::new(1.0, 0.0, 2.0, 2.0), Some(Role::Accent.solid()))
    else {
        panic!("a picture");
    };
    icon.exact = Some(Rect::new(1.25, 0.25, 1.5, 1.5));
    let list = DrawList {
        items: vec![Paint::Image(icon)],
    };
    let frame = render(&list, 4, 1, &dark(), &One(red(16, 16)), &options);
    let [placed] = &frame.placements[..] else {
        panic!("one icon: {:?}", frame.placements);
    };
    assert_eq!((placed.width, placed.height), (15, 15), "{placed:?}");
    assert_eq!(
        (placed.col, placed.row, placed.cols, placed.rows),
        (1, 0, 2, 1),
        "{placed:?}"
    );
    // At the pixel the page put it: 12.5 across and 2.5 down.
    let (x, y) = placed.offset.expect("a pixel of its first cell");
    assert!(
        (2..=3).contains(&x) && (2..=3).contains(&y),
        "the offset is {:?}",
        placed.offset
    );
    // A cell that is not the page's to a hundredth: the icon keeps its proportions in the middle of the
    // place that was written (it is square).
    let tall = Options {
        pixels: (10, 30),
        ..Options::default()
    };
    let frame = render(&list, 4, 1, &dark(), &One(red(16, 16)), &tall);
    let [placed] = &frame.placements[..] else {
        panic!("one icon");
    };
    assert_eq!(
        placed.width, placed.height,
        "a square written is a square drawn: {placed:?}"
    );
}

/// The pictures of a stream written to a terminal: how many sent pixels (`a=t`, `a=T`) and how many
/// placed them (`a=p`) it holds.
fn graphics(stream: &[u8]) -> (usize, usize) {
    let count = |needle: &[u8]| {
        stream
            .windows(needle.len())
            .filter(|window| *window == needle)
            .count()
    };
    (
        count(b"\x1b_Ga=t,") + count(b"\x1b_Ga=T,"),
        count(b"\x1b_Ga=p,"),
    )
}

/// Pixels the size that is asked for, as the sandbox's are, all one picture whatever the part of it
/// that is seen: the same name for every frame.
struct Named(u64);

impl Pictures for Named {
    fn raster(&self, picture: &Picture) -> Option<Raster> {
        let width = picture.rect.w.round().max(1.0) as u32;
        let height = picture.rect.h.round().max(1.0) as u32;
        Some(Raster {
            id: Some(self.0),
            pixels: [200, 200, 200, 255].repeat((width * height) as usize),
            width,
            height,
            crop: Rect::new(0.0, 0.0, 1.0, 1.0),
        })
    }
}

/// A mark on a picture the way the page writes one: a faint box with the place it was written at.
fn mark(rect: Rect) -> Paint {
    Paint::Quad(Quad {
        rect,
        radii: [0.0; 4],
        fill: Some(Color::from(Role::Accent).faded(0.3)),
        gradient: None,
        border: None,
        shadow: None,
        clip: world(),
        exact: Some(rect),
    })
}

#[test]
fn a_picture_that_is_scrolled_is_sent_once_and_only_put_where_it_moved_to() {
    // A page of a document in a box of the screen, a mark on a word of it, and the page carried up the
    // screen by less than a cell at a time, the way a wheel turns it: on a terminal whose cell is 13
    // by 29 pixels, which is not the page's one to two. What the terminal is sent for it is the
    // pixels of the page and of the mark once, and then where they are.
    let options = Options {
        pixels: (13, 29),
        ..Options::default()
    };
    let mut screen = Screen::new(Depth::True);
    let mut sent = Vec::new();
    for step in 0..24 {
        let top = 3.0 - step as f32 * 0.7;
        let list = DrawList {
            items: vec![
                quad(Rect::new(0.0, 0.0, 40.0, 40.0), Role::Raised(1)),
                picture(Rect::new(4.0, top, 30.0, 40.0), None),
                mark(Rect::new(10.3, top + 5.2, 6.5, 1.1)),
            ],
        };
        let frame = render(&list, 40, 20, &dark(), &Named(7), &options);
        let bytes = screen.draw(&frame, None);
        let (loads, places) = graphics(&bytes);
        sent.push((loads, places));
    }
    // The first frame is sent the page, the mark and the colour under the cells that the page covers
    // a part of; no frame after it is sent anything but where they are.
    assert!(
        sent.first().is_some_and(|(loads, _)| *loads <= 3),
        "the first frame sends a picture, a mark and a ground: {sent:?}"
    );
    assert!(
        sent.iter().skip(1).all(|(loads, _)| *loads == 0),
        "pixels sent again over 24 frames of a scroll: {sent:?}"
    );
    assert!(
        sent.iter().all(|(_, places)| *places > 0),
        "every frame puts the picture where it is: {sent:?}"
    );
}

#[test]
fn a_picture_is_drawn_where_the_page_laid_it_out_and_is_cut_at_its_clip_and_not_at_its_cells() {
    // The box the grid made is whole cells and the picture the page wrote for it is not: it is the
    // written size in the middle of them, and stands a few pixels out of them. It is drawn there, at
    // the pixels the layout gave it, and cut where it is not seen (its clip), not where its box ends.
    let options = Options {
        col: 8.0,
        row: 18.0,
        pixels: (13, 29),
        ..Options::default()
    };
    let cells = Rect::new(80.0, 90.0, 160.0, 180.0);
    let exact = Rect::new(76.8, 87.3, 166.4, 185.4);
    let draw = |clip: Rect| {
        let mut paint = picture(exact, None);
        if let Paint::Image(picture) = &mut paint {
            picture.cells = cells;
            picture.clip = clip;
        }
        let list = DrawList { items: vec![paint] };
        render(&list, 40, 20, &dark(), &Named(3), &options)
    };
    let frame = draw(world());
    let placed = frame
        .placements
        .iter()
        .find(|placed| placed.z == -1_073_741_825)
        .expect("the picture");
    // 9.6 across is the pixel 125 (cell 9, 8 pixels in); 9.7 down is the pixel 141 (row 4, 25 in).
    assert_eq!((placed.col, placed.row), (9, 4));
    assert_eq!(placed.offset, Some((8, 25)));
    assert_eq!((placed.width, placed.height), (270, 298));
    // Cut at a clip that is the box, it stops at the box.
    let frame = draw(cells);
    let placed = frame
        .placements
        .iter()
        .find(|placed| placed.z == -1_073_741_825)
        .expect("the picture");
    assert_eq!((placed.col, placed.row), (10, 5));
    assert_eq!(placed.offset, Some((0, 0)));
    assert_eq!((placed.width, placed.height), (260, 290));
}

#[test]
fn a_picture_that_is_not_there_yet_is_waited_for() {
    let list = DrawList {
        items: vec![picture(Rect::new(0.0, 0.0, 8.0, 8.0), None)],
    };
    let frame = render(&list, 8, 4, &dark(), &NoPictures, &Options::default());
    assert!(frame.waiting);
    assert!(frame.placements.is_empty());
}

#[test]
fn a_picture_a_terminal_cannot_draw_is_half_blocks_where_it_is_large() {
    let options = Options {
        images: ImageMode::Blocks,
        ..Options::default()
    };
    let list = DrawList {
        items: vec![
            picture(Rect::new(0.0, 0.0, 8.0, 8.0), None),
            // An icon is left out: a pixel of the grid is a cell's half.
            picture(Rect::new(10.0, 0.0, 2.0, 2.0), Some(Role::Accent.solid())),
        ],
    };
    let frame = render(&list, 12, 4, &dark(), &One(red(4, 4)), &options);
    assert!(frame.placements.is_empty());
    assert_eq!(bg(&frame, 3, 2), Bg::Rgb(Rgb::new(255, 0, 0)));
    assert_eq!(bg(&frame, 10, 0), Bg::Rgb(dark().background));
}

#[test]
fn an_icon_keeps_the_grounds_under_it_and_is_the_colour_it_is_tinted() {
    let faint = Raster {
        id: None,
        width: 1,
        height: 1,
        pixels: vec![64, 0, 0, 128],
        crop: Rect::new(0.0, 0.0, 1.0, 1.0),
    };
    let list = DrawList {
        items: vec![
            quad(Rect::new(0.0, 0.0, 4.0, 2.0), Role::Raised(1)),
            picture(Rect::new(1.0, 0.0, 2.0, 2.0), Some(Role::Accent.solid())),
        ],
    };
    let frame = render(&list, 4, 1, &dark(), &One(faint), &Options::default());
    let placed = &frame.placements[0];
    assert_eq!(placed.z, -1);
    assert_eq!(bg(&frame, 1, 0), Bg::Rgb(BOX));
    // The tint, with the opacity the pixel has.
    assert_eq!(placed.rgba(), vec![0x58, 0xa6, 0xff, 128]);
}

#[test]
fn an_icon_under_a_box_drawn_after_it_is_left_out_and_one_beside_it_is_not() {
    let faint = Raster {
        id: None,
        width: 1,
        height: 1,
        pixels: vec![64, 0, 0, 128],
        crop: Rect::new(0.0, 0.0, 1.0, 1.0),
    };
    let icon = || picture(Rect::new(1.0, 0.0, 2.0, 2.0), Some(Role::Accent.solid()));
    // A box the page lays over it (a palette over the page) hides it: a picture the terminal draws
    // over the grounds would show through.
    let list = DrawList {
        items: vec![
            quad(Rect::new(0.0, 0.0, 8.0, 2.0), Role::Raised(1)),
            icon(),
            quad(Rect::new(0.0, 0.0, 4.0, 2.0), Role::Raised(2)),
        ],
    };
    let frame = render(
        &list,
        8,
        1,
        &dark(),
        &One(faint.clone()),
        &Options::default(),
    );
    assert!(frame.placements.is_empty());
    // One the box is not over stays, and so does one whose ground was drawn before it.
    let list = DrawList {
        items: vec![
            quad(Rect::new(0.0, 0.0, 8.0, 2.0), Role::Raised(1)),
            icon(),
            quad(Rect::new(4.0, 0.0, 4.0, 2.0), Role::Raised(2)),
        ],
    };
    let frame = render(&list, 8, 1, &dark(), &One(faint), &Options::default());
    assert_eq!(frame.placements.len(), 1);
}

#[test]
fn a_pixel_the_terminal_gets_is_not_premultiplied() {
    let faint = Raster {
        id: None,
        width: 1,
        height: 1,
        pixels: vec![64, 32, 0, 128],
        crop: Rect::new(0.0, 0.0, 1.0, 1.0),
    };
    let list = DrawList {
        items: vec![picture(Rect::new(0.0, 0.0, 2.0, 2.0), None)],
    };
    let frame = render(&list, 2, 1, &dark(), &One(faint), &Options::default());
    assert_eq!(frame.placements[0].rgba(), vec![128, 64, 0, 128]);
}

fn line(from: (f32, f32), to: (f32, f32), width: f32) -> Paint {
    Paint::Figure(Figure {
        rect: Rect::new(0.0, 0.0, 4.0, 2.0),
        scale: 1.0,
        paths: vec![{
            let mut line = path().move_to(from.0, from.1).line_to(to.0, to.1);
            line.stroke = Some(Stroke {
                width,
                color: Role::Foreground.solid(),
            });
            line
        }],
        clip: world(),
    })
}

#[test]
fn a_line_of_a_canvas_is_the_braille_dots_it_passes() {
    // Across the middle of a cell, between its second and third row of dots: both are lit.
    let frame = draw(vec![line((0.0, 1.0), (4.0, 1.0), 0.1)], 4, 1);
    assert_eq!(frame.text(), "⠶⠶⠶⠶\n");
    let cell = frame.grid.cell(0, 0).unwrap();
    assert_eq!(cell.fg, dark().foreground);
    assert_eq!(cell.bg, Bg::Rgb(dark().background));
}

/// Whether two colours are the same to within what a figure's pixels round to.
fn near(a: Rgb, b: Rgb) -> bool {
    [(a.r, b.r), (a.g, b.g), (a.b, b.b)]
        .iter()
        .all(|(x, y)| x.abs_diff(*y) <= 2)
}

/// A figure of one path, in a canvas of `rect`.
fn figure(rect: Rect, path: pencere_view::Path) -> Paint {
    Paint::Figure(Figure {
        rect,
        scale: 1.0,
        paths: vec![path],
        clip: world(),
    })
}

#[test]
fn a_canvas_makes_no_ground_and_leaves_out_what_it_covers_little() {
    // A box, and over it a canvas that fills all of it with a faint colour (a grid, the fill under a
    // line): not a dot of it is drawn, and the ground of every cell is the box's.
    let mut faint = path()
        .move_to(0.0, 0.0)
        .line_to(4.0, 0.0)
        .line_to(4.0, 4.0)
        .line_to(0.0, 4.0)
        .close();
    faint.fill = Some(Color::from(Role::Accent).faded(0.2));
    let list = vec![
        quad(Rect::new(0.0, 0.0, 4.0, 4.0), Role::Raised(1)),
        figure(Rect::new(0.0, 0.0, 4.0, 4.0), faint),
    ];
    let frame = draw(list, 4, 2);
    assert_eq!(frame.text(), "\n\n");
    for (col, row) in [(0, 0), (3, 0), (0, 1), (3, 1)] {
        assert_eq!(bg(&frame, col, row), Bg::Rgb(BOX), "{col}, {row}");
    }
    // What it draws solidly is dots, in the colour of the line, still on the box's ground.
    let mut solid = path().move_to(0.0, 1.0).line_to(4.0, 1.0);
    solid.stroke = Some(Stroke {
        width: 0.1,
        color: Role::Accent.solid(),
    });
    let list = vec![
        quad(Rect::new(0.0, 0.0, 4.0, 4.0), Role::Raised(1)),
        figure(Rect::new(0.0, 0.0, 4.0, 4.0), solid),
    ];
    let frame = draw(list, 4, 2);
    assert_eq!(frame.text(), "⠶⠶⠶⠶\n\n");
    let cell = frame.grid.cell(1, 0).expect("a cell");
    assert!(near(cell.fg, dark().role(Role::Accent)));
    assert_eq!(bg(&frame, 1, 0), Bg::Rgb(BOX));
}

#[test]
fn where_two_lines_cross_a_cell_it_is_the_colour_that_has_more_of_it() {
    // A cell of a chart where a thin line of one colour and a thick one of another run through it:
    // the cell is the thick line's colour, not a mean of the two that neither line has.
    let mut thin = path().move_to(0.0, 0.25).line_to(4.0, 0.25);
    thin.stroke = Some(Stroke {
        width: 0.5,
        color: Role::Red.solid(),
    });
    let mut thick = path().move_to(0.0, 1.25).line_to(4.0, 1.25);
    thick.stroke = Some(Stroke {
        width: 1.5,
        color: Role::Accent.solid(),
    });
    let list = vec![Paint::Figure(Figure {
        rect: Rect::new(0.0, 0.0, 4.0, 4.0),
        scale: 1.0,
        paths: vec![thin, thick],
        clip: world(),
    })];
    let frame = draw(list, 4, 2);
    assert_eq!(frame.text(), "⣿⣿⣿⣿\n\n");
    let cell = frame.grid.cell(1, 0).expect("a cell");
    assert!(near(cell.fg, dark().role(Role::Accent)));
}

#[test]
fn a_canvas_of_two_cells_is_an_icon_drawn_in_quarter_blocks_that_fills_them() {
    // A chevron pointing down, small in the room it has (a cell and a half across and a third of a
    // row down, in the middle of two cells): its ink is what is drawn in the quarters, so that it
    // reads as the arrow it is: ▚▞.
    let mut chevron = path().move_to(0.5, 0.7).line_to(1.0, 1.2).line_to(1.5, 0.7);
    chevron.stroke = Some(Stroke {
        width: 0.2,
        color: Role::Accent.solid(),
    });
    let frame = draw(vec![figure(Rect::new(0.0, 0.0, 2.0, 2.0), chevron)], 2, 1);
    assert_eq!(frame.text(), "▚▞\n");
    assert!(near(
        frame.grid.cell(0, 0).expect("a cell").fg,
        dark().role(Role::Accent)
    ));
    // And upward, the same two characters the other way about.
    let mut chevron = path().move_to(0.5, 1.2).line_to(1.0, 0.7).line_to(1.5, 1.2);
    chevron.stroke = Some(Stroke {
        width: 0.2,
        color: Role::Accent.solid(),
    });
    let frame = draw(vec![figure(Rect::new(0.0, 0.0, 2.0, 2.0), chevron)], 2, 1);
    assert_eq!(frame.text(), "▞▚\n");
}

#[test]
fn an_icon_that_is_not_flat_is_braille_with_its_ink_filling_the_cells() {
    // A cross, small in the room it has: its ink is spread over the two cells, in dots, which have
    // twice the rows of quarters and keep the two strokes apart.
    let mut cross = path()
        .move_to(0.5, 0.5)
        .line_to(1.5, 1.5)
        .move_to(1.5, 0.5)
        .line_to(0.5, 1.5);
    cross.stroke = Some(Stroke {
        width: 0.2,
        color: Role::Accent.solid(),
    });
    let frame = draw(vec![figure(Rect::new(0.0, 0.0, 2.0, 2.0), cross)], 2, 1);
    assert_eq!(frame.text(), "⡱⢎\n");
}

#[test]
fn a_canvas_the_terminal_draws_is_one_picture_over_the_grounds() {
    let options = Options {
        canvases: pencere_tui::CanvasMode::Picture,
        ..Options::default()
    };
    let list = DrawList {
        items: vec![line((0.0, 1.0), (4.0, 1.0), 1.0)],
    };
    let frame = render(&list, 4, 1, &dark(), &NoPictures, &options);
    let placed = &frame.placements[0];
    assert_eq!((placed.cols, placed.rows), (4, 1));
    assert_eq!((placed.width, placed.height), (48, 24));
    assert_eq!(placed.z, -1);
}

#[test]
fn a_frame_is_the_bytes_a_terminal_is_sent() {
    let frame = draw(vec![words(0.0, 0.0, "ab", Role::Foreground)], 3, 1);
    assert_eq!(
        String::from_utf8(frame.ansi()).unwrap(),
        "\x1b[H\x1b[2J\x1b[38;2;201;209;217m\x1b[48;2;13;17;23mab \x1b[0m\x1b[2;1H"
    );
    let list = DrawList {
        items: vec![picture(Rect::new(0.0, 0.0, 2.0, 2.0), None)],
    };
    let frame = render(&list, 2, 1, &dark(), &One(red(1, 1)), &Options::default());
    let bytes = String::from_utf8(frame.ansi()).unwrap();
    assert!(bytes.contains(
        "\x1b[1;1H\x1b_Ga=T,f=32,s=1,v=1,c=2,r=1,z=-1073741825,C=1,q=2,m=0;/wAA/w==\x1b\\"
    ));
    // A cell the terminal draws the ground of is told to use its own.
    assert!(bytes.contains("\x1b[49m"));
}

#[test]
fn a_tint_over_a_picture_the_terminal_draws_is_a_picture_over_it() {
    let faint = Color::from(Role::Accent).faded(0.3);
    let list = DrawList {
        items: vec![
            picture(Rect::new(0.0, 0.0, 4.0, 4.0), None),
            // Over two of the picture's cells, and over a cell of the page's own.
            quad(Rect::new(1.0, 0.0, 2.0, 2.0), faint),
            quad(Rect::new(0.0, 4.0, 1.0, 2.0), faint),
        ],
    };
    let frame = render(&list, 4, 3, &dark(), &One(red(1, 1)), &Options::default());
    // The cells stay the terminal's: its picture shows through them.
    for col in [1, 2] {
        let cell = frame.grid.cell(col, 0).expect("a cell");
        assert_eq!(cell.bg, Bg::Default, "column {col}");
    }
    let tint = frame
        .placements
        .iter()
        .find(|placed| placed.z == -1)
        .expect("a picture over the picture");
    assert_eq!((tint.col, tint.row, tint.cols, tint.rows), (1, 0, 2, 1));
    assert_eq!(tint.rgba()[3], 77);
    // On the page's own ground it is laid on the ground, as ever.
    let own = frame.grid.cell(0, 2).expect("a cell");
    assert_ne!(own.bg, Bg::Default);
}

#[test]
fn text_is_measured_in_cells() {
    let mut cells = Cells::new(2.0, 4.0);
    let metrics = cells.measure(&font(), "a漢");
    assert_eq!((metrics.width, metrics.height), (6.0, 4.0));
    assert_eq!(
        cells.offsets(&font(), "a漢b", &[0, 1, 4, 5, 99]),
        vec![0.0, 2.0, 6.0, 8.0, 8.0]
    );
    assert!(Theme::dark().is_dark());
    assert!(!Theme::light().is_dark());
}
