//! What a draw list comes to in the grid, written out by hand: each case is a page of a few boxes and
//! words, and the cells it has to be drawn into.

use pencere_tui::{
    Bg, Cells, ImageMode, NoPictures, Options, Pictures, Raster, Rgb, Theme, render, text_width,
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
            quad(Rect::new(0.0, 3.0, 10.0, 0.125), Role::Raised(1)),
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
        crop: Rect::new(0.0, 0.0, 1.0, 1.0),
        shown: Rect::new(0.0, 0.0, 1.0, 1.0),
        tint,
        opacity: 1.0,
        radii: [0.0; 4],
        clip: world(),
        turn: 0.0,
    })
}

fn red(width: u32, height: u32) -> Raster {
    Raster {
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
    assert_eq!(placed.rgba.get(..4), Some(&[255, 0, 0, 255][..]));
    // It covers its cells altogether: it is drawn under their grounds, which are the terminal's.
    assert_eq!(placed.z, -1_073_741_825);
    assert_eq!(bg(&frame, 2, 1), Bg::Default);
    assert_eq!(bg(&frame, 9, 3), Bg::Default);
    assert_eq!(bg(&frame, 1, 1), Bg::Rgb(dark().background));
    assert_eq!(bg(&frame, 10, 1), Bg::Rgb(dark().background));
    assert!(!frame.waiting);
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
    assert_eq!(placed.rgba, vec![0x58, 0xa6, 0xff, 128]);
}

#[test]
fn a_pixel_the_terminal_gets_is_not_premultiplied() {
    let faint = Raster {
        width: 1,
        height: 1,
        pixels: vec![64, 32, 0, 128],
        crop: Rect::new(0.0, 0.0, 1.0, 1.0),
    };
    let list = DrawList {
        items: vec![picture(Rect::new(0.0, 0.0, 2.0, 2.0), None)],
    };
    let frame = render(&list, 2, 1, &dark(), &One(faint), &Options::default());
    assert_eq!(frame.placements[0].rgba, vec![128, 64, 0, 128]);
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
    assert_eq!(tint.rgba[3], 77);
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
