//! What a terminal shows of a page's pictures, as the pixels a person sees: the grounds of the cells,
//! the pictures the terminal draws under and over them, put together the way the terminal does
//! ([`pencere_tui::compose`]) from the bytes the screen writes. Each case is a page of a few boxes and
//! pictures laid out as the page lays them out (a cell of 8 by 18 points), shown on terminals whose
//! cells are not that.

use pencere_tui::compose::{Image, Terminal};
use pencere_tui::{
    Bg, Depth, Frame, ImageMode, Options, Pictures, Raster, Rgb, Screen, Theme, render,
};
use pencere_view::{Color, DrawList, Paint, Picture, Quad, Rect, Rgba, Role};

/// The terminal's own ground: no pixel of a screen may be this, for each has a cell's or a picture's.
const SENTINEL: Rgb = Rgb::new(255, 0, 255);
const WHITE: Rgb = Rgb::new(255, 255, 255);
const COLS: u16 = 80;
const ROWS: u16 = 30;
/// The cells of the page's layout: points across and down.
const COLUMN: f32 = 8.0;
const ROW: f32 = 18.0;
/// The terminals' cells the guards are run on: the one of Ghostty at the size of the screen the page
/// was seen on, and two that are not.
const CELLS: [(u32, u32); 3] = [(13, 29), (12, 24), (9, 20)];

fn world() -> Rect {
    Rect::new(-1e6, -1e6, 2e6, 2e6)
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

/// A picture the way the page writes it: `rect` where the image is in its box (to the pixel), `cells`
/// the box the page's grid made of it, `clip` the room it is seen in.
fn picture(rect: Rect, cells: Rect, clip: Rect) -> Paint {
    let seen = Rect::new(
        rect.x.max(clip.x),
        rect.y.max(clip.y),
        (rect.x + rect.w).min(clip.x + clip.w) - rect.x.max(clip.x),
        (rect.y + rect.h).min(clip.y + clip.h) - rect.y.max(clip.y),
    );
    Paint::Image(Picture {
        src: "page.png".to_owned(),
        page: 1,
        rect,
        cells,
        crop: Rect::new(0.0, 0.0, 1.0, 1.0),
        shown: Rect::new(
            (seen.x - rect.x) / rect.w,
            (seen.y - rect.y) / rect.h,
            seen.w / rect.w,
            seen.h / rect.h,
        ),
        tint: None,
        opacity: 1.0,
        radii: [0.0; 4],
        clip,
        turn: 0.0,
        exact: None,
    })
}

/// Opaque pixels, as many as the rectangle it is asked at is across and down, all of one colour: what
/// the sandbox gives for a picture that is drawn whole.
struct Flat(Rgb);

impl Pictures for Flat {
    fn raster(&self, picture: &Picture) -> Option<Raster> {
        let width = picture.rect.w.round().max(1.0) as u32;
        let height = picture.rect.h.round().max(1.0) as u32;
        Some(Raster {
            id: Some(u64::from(width) << 20 | u64::from(height)),
            width,
            height,
            pixels: [self.0.r, self.0.g, self.0.b, 255].repeat((width * height) as usize),
            crop: Rect::new(0.0, 0.0, 1.0, 1.0),
        })
    }
}

fn options(cell: (u32, u32)) -> Options {
    Options {
        col: COLUMN,
        row: ROW,
        images: ImageMode::Kitty,
        pixels: cell,
        ..Options::default()
    }
}

fn frame_of(items: Vec<Paint>, pictures: &dyn Pictures, cell: (u32, u32)) -> Frame {
    render(
        &DrawList { items },
        COLS,
        ROWS,
        &Theme::dark(),
        pictures,
        &options(cell),
    )
}

/// What a terminal shows of `frame`, once it was written to it by `screen`.
/// Writes `image` as a PPM file in the folder `TUI_FRAMES` names, where it is set: a frame of a guard,
/// for the eye.
fn keep(image: &Image, name: &str) {
    if let Ok(folder) = std::env::var("TUI_FRAMES")
        && std::fs::create_dir_all(&folder).is_ok()
    {
        let _ = std::fs::write(
            std::path::Path::new(&folder).join(format!("{name}.ppm")),
            image.ppm(),
        );
    }
}

fn shown(screen: &mut Screen, terminal: &mut Terminal, frame: &Frame) -> Image {
    terminal.feed(&screen.draw(frame, None));
    terminal.show(frame)
}

/// What a terminal shows of `frame` written to a screen that had nothing.
fn once(frame: &Frame, cell: (u32, u32)) -> Image {
    shown(
        &mut Screen::new(Depth::True),
        &mut Terminal::new(cell, SENTINEL),
        frame,
    )
}

fn ground() -> Paint {
    quad(
        Rect::new(0.0, 0.0, f32::from(COLS) * COLUMN, f32::from(ROWS) * ROW),
        Role::Background,
    )
}

/// The ground of the page, as the terminal shows it.
fn ground_color() -> Rgb {
    let frame = frame_of(vec![ground()], &Flat(WHITE), (13, 29));
    match frame.grid.cell(0, 0).map(|cell| cell.bg) {
        Some(Bg::Rgb(color)) => color,
        _ => SENTINEL,
    }
}

/// The room the pages are seen in: columns 6 to 74, the whole height.
fn room() -> Rect {
    Rect::new(48.0, 0.0, 544.0, f32::from(ROWS) * ROW)
}

/// Two pages of a document one above the other, 68 columns wide, as the page lays them out when it
/// is scrolled: the first up past the top, a gap between them, each in a box of whole cells that the
/// place the page wrote for it is not (it is a few points past or short of one edge).
fn two_pages() -> Vec<Paint> {
    let height = 544.0 / std::f32::consts::FRAC_1_SQRT_2;
    let first = Rect::new(48.0, -455.5, 544.0, height);
    let second = Rect::new(48.0, 330.0, 544.0, height);
    let boxed = |rect: Rect| {
        let (y0, y1) = (
            (rect.y / ROW).round() * ROW,
            ((rect.y + rect.h) / ROW).round() * ROW,
        );
        Rect::new(rect.x, y0, rect.w, y1 - y0)
    };
    vec![
        ground(),
        picture(first, boxed(first), room()),
        picture(second, boxed(second), room()),
    ]
}

/// Where the paper of a page is, to the pixel: where the layout has it where the terminal's cell has the
/// page's proportions to a hundredth, and else the largest rectangle of the page's proportions that
/// is in the box the page wrote for it (on the terminal's pixels), in the middle of it.
fn paper(rect: Rect, cell: (u32, u32)) -> (f32, f32, f32, f32) {
    let (across, down) = (cell.0 as f32 / COLUMN, cell.1 as f32 / ROW);
    let (x, y, w, h) = (
        rect.x * across,
        rect.y * down,
        rect.w * across,
        rect.h * down,
    );
    if (across / down - 1.0).abs() <= 0.01 {
        return (x, y, w, h);
    }
    let natural = rect.w / rect.h;
    let width = w.min(h * natural);
    let height = width / natural;
    (x + (w - width) / 2.0, y + (h - height) / 2.0, width, height)
}

fn rows_of_paper(image: &Image, paper_color: Rgb) -> Vec<(u32, u32, u32)> {
    let mut rows = Vec::new();
    for y in 0..image.height {
        let mut first = None;
        let mut last = 0;
        for x in 0..image.width {
            if image.at(x, y) == paper_color {
                first.get_or_insert(x);
                last = x;
            }
        }
        if let Some(first) = first {
            rows.push((y, first, last + 1));
        }
    }
    rows
}

fn none_of_the_terminals_own(image: &Image) -> Option<(u32, u32)> {
    (0..image.height)
        .flat_map(|y| (0..image.width).map(move |x| (x, y)))
        .find(|(x, y)| image.at(*x, *y) == SENTINEL)
}

#[test]
fn two_pages_one_above_the_other_have_their_edges_on_the_same_pixel_columns() {
    // The step the page had on the screen it was seen on: the page above 1950 pixels across and the
    // one below 1927, a strip of the ground at the sides of the narrower one. The pages are as wide
    // as one another, and what the box of cells that each is in makes of the place the page wrote
    // is not the pages' to follow.
    let mut wrong = Vec::new();
    for cell in CELLS {
        let frame = frame_of(two_pages(), &Flat(WHITE), cell);
        let image = once(&frame, cell);
        if let Some(at) = none_of_the_terminals_own(&image) {
            wrong.push(format!("{cell:?}: the terminal's own ground at {at:?}"));
        }
        let rows = rows_of_paper(&image, WHITE);
        let lefts = rows.iter().map(|(_, left, _)| *left);
        let rights = rows.iter().map(|(_, _, right)| *right);
        let (left, right) = (
            lefts.clone().max().unwrap_or(0) - lefts.min().unwrap_or(0),
            rights.clone().max().unwrap_or(0) - rights.min().unwrap_or(0),
        );
        if left > 1 || right > 1 {
            wrong.push(format!(
                "{cell:?}: the edges of the paper move by {left} pixels at the left and {right} at the right"
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn the_paper_stands_where_the_pages_box_puts_it_and_the_ground_stands_where_it_does_not() {
    let mut wrong = Vec::new();
    let pages = two_pages();
    let rects: Vec<Rect> = pages
        .iter()
        .filter_map(|paint| match paint {
            Paint::Image(picture) => Some(picture.rect),
            _ => None,
        })
        .collect();
    for cell in CELLS {
        let frame = frame_of(pages.clone(), &Flat(WHITE), cell);
        let image = once(&frame, cell);
        let (across, down) = (cell.0 as f32 / COLUMN, cell.1 as f32 / ROW);
        let seen = (
            48.0 * across,
            0.0,
            592.0 * across,
            f32::from(ROWS) * ROW * down,
        );
        for rect in &rects {
            let (x, y, w, h) = paper(*rect, cell);
            // Where the paper is seen: its rectangle cut by the room.
            let (x0, y0) = (x.max(seen.0), y.max(seen.1));
            let (x1, y1) = ((x + w).min(seen.2), (y + h).min(seen.3));
            for py in 0..image.height {
                for px in 0..image.width {
                    let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                    let inside =
                        fx >= x0 + 1.0 && fx <= x1 - 1.0 && fy >= y0 + 1.0 && fy <= y1 - 1.0;
                    let near = fx >= x0 - 1.0 && fx <= x1 + 1.0 && fy >= y0 - 1.0 && fy <= y1 + 1.0;
                    let color = image.at(px, py);
                    if inside && color != WHITE {
                        wrong.push(format!(
                            "{cell:?}: {color:?} inside the paper at ({px}, {py})"
                        ));
                        break;
                    }
                    if !near && color == WHITE {
                        let in_other = rects.iter().any(|other| {
                            let (ox, oy, ow, oh) = paper(*other, cell);
                            fx >= ox - 1.0
                                && fx <= ox + ow + 1.0
                                && fy >= oy - 1.0
                                && fy <= oy + oh + 1.0
                        });
                        if !in_other {
                            wrong
                                .push(format!("{cell:?}: paper outside the paper at ({px}, {py})"));
                            break;
                        }
                    }
                }
            }
        }
    }
    wrong.truncate(12);
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn a_square_picture_is_a_square_with_the_ground_of_its_parent_at_its_sides_and_not_the_ground_of_its_box()
 {
    // A card the page made of the box of an image: 30 columns across and 13 rows down, its own ground
    // lighter than the page's, a square image the largest square in it. On a terminal whose cell is
    // 13 by 29 the box is 390 by 377 pixels and the square 377 by 377: the 13 that are left are in
    // the cells at the sides, which the picture covers a part of, and are the page's ground, not the
    // card's.
    let card = Rect::new(0.0, 0.0, 240.0, 234.0);
    let items = vec![
        ground(),
        quad(card, Role::Raised(3)),
        picture(Rect::new(3.0, 0.0, 234.0, 234.0), card, world()),
    ];
    let red = Rgb::new(255, 0, 0);
    let mut wrong = Vec::new();
    for cell in CELLS {
        let frame = frame_of(items.clone(), &Flat(red), cell);
        let image = once(&frame, cell);
        let (cols, rows) = (30 * cell.0, 13 * cell.1);
        let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
        for y in 0..rows {
            for x in 0..cols {
                if image.at(x, y) == red {
                    left = left.min(x);
                    right = right.max(x + 1);
                    top = top.min(y);
                    bottom = bottom.max(y + 1);
                }
            }
        }
        if right <= left {
            wrong.push(format!("{cell:?}: no picture"));
            continue;
        }
        let (across, down) = (right - left, bottom - top);
        if cell == (13, 29) {
            // A cell within a hundredth of the page's proportions: the picture is where the layout has
            // it, 234 points by 234 (380 by 377 pixels), not a square to the pixel.
            if across.abs_diff(380) > 1 || down.abs_diff(377) > 1 {
                wrong.push(format!(
                    "{cell:?}: the picture is {across} by {down}, the layout's is 380 by 377"
                ));
            }
        } else if across.abs_diff(down) > 2 {
            wrong.push(format!("{cell:?}: the square is {across} by {down}"));
        }
        // The cells the picture covers a part of are the page's ground where it does not cover them; the
        // cells of the card it covers none of keep the card's own.
        let mut others = std::collections::HashMap::new();
        for y in 0..rows {
            for x in 0..cols {
                let (cx, cy) = (x / cell.0, y / cell.1);
                let touched = cx >= left / cell.0
                    && cx < right.div_ceil(cell.0)
                    && cy >= top / cell.1
                    && cy < bottom.div_ceil(cell.1);
                let color = image.at(x, y);
                if touched && color != red && color != ground_color() {
                    *others.entry(color).or_insert(0_u32) += 1;
                }
            }
        }
        if !others.is_empty() {
            wrong.push(format!(
                "{cell:?}: not the ground of the page around it: {others:?}"
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// The rows a screen is drawn in with a palette open and shut again: the page, the page under the
/// palette (a scrim over all of it, a panel over a part), and the page again.
fn palette_scenes() -> [Vec<Paint>; 3] {
    let page = vec![
        ground(),
        // A cell of white beside the page, which the scrim is over as it is over the paper.
        quad(
            Rect::new(8.0, 18.0, 32.0, 36.0),
            Rgba::new(255, 255, 255, 255).solid(),
        ),
        picture(
            Rect::new(48.0, 0.0, 544.0, 540.0),
            Rect::new(48.0, 0.0, 544.0, 540.0),
            room(),
        ),
    ];
    let mut covered = page.clone();
    covered.push(quad(
        Rect::new(0.0, 0.0, 640.0, 540.0),
        Color::from(Rgba::new(0, 0, 0, 255)).faded(0.5),
    ));
    covered.push(quad(Rect::new(160.0, 90.0, 320.0, 180.0), Role::Raised(3)));
    [page.clone(), covered, page]
}

/// How many pictures of pixels (more than a handful) a stream holds.
fn pages_sent(terminal: &Terminal, before: usize) -> usize {
    terminal
        .loads()
        .iter()
        .skip(before)
        .filter(|load| load.width * load.height > 64)
        .count()
}

#[test]
fn a_page_under_a_palette_is_as_dim_as_the_cells_are_and_comes_back_without_being_sent_again() {
    let cell = (13, 29);
    let mut screen = Screen::new(Depth::True);
    let mut terminal = Terminal::new(cell, SENTINEL);
    let mut seen = Vec::new();
    let mut sent = Vec::new();
    for items in palette_scenes() {
        let before = terminal.loads().len();
        let frame = frame_of(items, &Flat(WHITE), cell);
        seen.push(shown(&mut screen, &mut terminal, &frame));
        sent.push(pages_sent(&terminal, before));
    }
    // A white cell and a white page, both under the scrim: one colour, to a few of 255.
    let (across, down) = (cell.0 as f32 / COLUMN, cell.1 as f32 / ROW);
    let at = |x: f32, y: f32| ((x * across) as u32, (y * down) as u32);
    let (cx, cy) = at(20.0, 30.0);
    let (px, py) = at(100.0, 400.0);
    let covered = &seen[1];
    let (reference, paper) = (covered.at(cx, cy), covered.at(px, py));
    let near = |a: u8, b: u8| a.abs_diff(b) <= 3;
    assert!(
        near(reference.r, paper.r) && near(reference.g, paper.g) && near(reference.b, paper.b),
        "the cell under the scrim is {reference:?} and the page {paper:?}"
    );
    assert_ne!(paper, WHITE, "the scrim is over the page");
    // The page was sent for the first screen and for none after it.
    assert_eq!(
        (sent[1], sent[2]),
        (0, 0),
        "pictures of pixels sent for each screen: {sent:?}"
    );
    // It is the page again when the palette is gone.
    assert_eq!(seen[2].at(px, py), WHITE);
}

/// The sandbox as it draws a page that is larger than a screen can hold whole: the part of it that is
/// asked for (what is seen, and a margin of 128 pixels on each side, rounded out to a grid of 128), kept
/// for as long as it holds what is asked for, no more than two of them, and each given a number of its
/// own when it is drawn. The pixels say where in the page each is: red is the column, green the row.
/// A part of a page: the pixel its left, top, right and bottom edges are at.
type Part = (u32, u32, u32, u32);

struct Sandbox {
    parts: std::cell::RefCell<Vec<(u64, Part)>>,
    next: std::cell::Cell<u64>,
}

impl Sandbox {
    fn new() -> Self {
        Self {
            parts: std::cell::RefCell::new(Vec::new()),
            next: std::cell::Cell::new(1),
        }
    }
}

impl Pictures for Sandbox {
    fn raster(&self, picture: &Picture) -> Option<Raster> {
        let whole = picture.full();
        let (w, h) = (whole.w.round() as u32, whole.h.round() as u32);
        let wanted = (
            (picture.shown.x * w as f32).floor().max(0.0) as u32,
            (picture.shown.y * h as f32).floor().max(0.0) as u32,
            ((picture.shown.x + picture.shown.w) * w as f32)
                .ceil()
                .min(w as f32) as u32,
            ((picture.shown.y + picture.shown.h) * h as f32)
                .ceil()
                .min(h as f32) as u32,
        );
        let held = self
            .parts
            .borrow()
            .iter()
            .find(|(_, part)| {
                part.0 <= wanted.0 && part.1 <= wanted.1 && part.2 >= wanted.2 && part.3 >= wanted.3
            })
            .copied();
        let (id, part) = held.unwrap_or_else(|| {
            let grid = |v: u32, up: bool| {
                if up {
                    v.div_ceil(128) * 128
                } else {
                    v / 128 * 128
                }
            };
            let part = (
                grid(wanted.0.saturating_sub(128), false),
                grid(wanted.1.saturating_sub(128), false),
                grid(wanted.2 + 128, true).min(w),
                grid(wanted.3 + 128, true).min(h),
            );
            let id = self.next.get();
            self.next.set(id + 1);
            let mut parts = self.parts.borrow_mut();
            parts.push((id, part));
            if parts.len() > 2 {
                parts.remove(0);
            }
            (id, part)
        });
        let (pw, ph) = (part.2 - part.0, part.3 - part.1);
        let mut pixels = Vec::with_capacity((pw * ph * 4) as usize);
        for y in 0..ph {
            for x in 0..pw {
                let (wx, wy) = (part.0 + x, part.1 + y);
                pixels.extend_from_slice(&[(wx % 251) as u8, (wy % 241) as u8, 200, 255]);
            }
        }
        Some(Raster {
            id: Some(id),
            width: pw,
            height: ph,
            pixels,
            crop: Rect::new(
                (picture.crop.x - part.0 as f32 / w as f32) / (pw as f32 / w as f32),
                (picture.crop.y - part.1 as f32 / h as f32) / (ph as f32 / h as f32),
                picture.crop.w / (pw as f32 / w as f32),
                picture.crop.h / (ph as f32 / h as f32),
            ),
        })
    }
}

#[test]
fn a_page_larger_than_a_screen_can_hold_is_sent_in_pieces_each_once_as_it_is_scrolled_down_and_back()
 {
    // A page of 1040 pixels across and 12000 down (12.5 million: more than is asked of the sandbox
    // whole), scrolled 28 pixels at a time by 50 steps and back. What the terminal is sent is the pieces
    // of a grid that are seen, each once: the page is across two of them and the screen is across one
    // to two of them down; a piece that is seen again is not sent again, whatever the sandbox does
    // with the parts it draws.
    let cell = (13, 29);
    let page = |step: i32| {
        let up = f32::from(step as i16) * 28.0 / (29.0 / ROW);
        let rect = Rect::new(0.0, -up, 640.0, 12000.0 / (29.0 / ROW));
        let clip = Rect::new(0.0, 0.0, 640.0, f32::from(ROWS) * ROW);
        vec![ground(), picture(rect, rect, clip)]
    };
    let sandbox = Sandbox::new();
    let mut screen = Screen::new(Depth::True);
    let mut terminal = Terminal::new(cell, SENTINEL);
    let mut at_the_end = None;
    for step in (0..=50).chain((0..50).rev()) {
        let frame = frame_of(page(step), &sandbox, cell);
        terminal.feed(&screen.draw(&frame, None));
        if step == 50 && at_the_end.is_none() {
            at_the_end = Some(terminal.show(&frame));
        }
    }
    let loads: Vec<_> = terminal
        .loads()
        .iter()
        .filter(|load| load.width * load.height > 64)
        .collect();
    // The screen after 1400 pixels is across the rows 1 and 2 of the grid (of 1024 pixels): six pieces
    // in all, 1040 by 3072 pixels.
    let area: u32 = loads.iter().map(|load| load.width * load.height).sum();
    assert!(
        loads.len() <= 6 && area <= 1040 * 3072,
        "pieces of the page sent over a scroll down and back, {area} pixels: {loads:?}"
    );
    // The pixels are where the page has them: 1400 pixels down.
    let image = at_the_end.expect("the screen after the fiftieth step");
    keep(&image, "big-page-scrolled");
    for (x, y) in [(5_u32, 3_u32), (700, 400), (1030, 860)] {
        let (wx, wy) = (x, y + 1400);
        let want = Rgb::new((wx % 251) as u8, (wy % 241) as u8, 200);
        assert_eq!(image.at(x, y), want, "the pixel ({x}, {y}) of the screen");
    }
}

/// Pixels a third of the size they are asked at, all of one grey: the page that stands in for a part of
/// a large one that is not drawn yet.
struct Third;

impl Pictures for Third {
    fn raster(&self, picture: &Picture) -> Option<Raster> {
        let whole = picture.full();
        let (width, height) = (
            (whole.w / 3.0).round().max(1.0) as u32,
            (whole.h / 3.0).round().max(1.0) as u32,
        );
        Some(Raster {
            id: Some(900),
            width,
            height,
            pixels: [90, 90, 90, 255].repeat((width * height) as usize),
            crop: Rect::new(0.0, 0.0, 1.0, 1.0),
        })
    }
}

#[test]
fn a_page_in_pieces_is_not_kept_from_pixels_that_only_stand_in_for_it() {
    // The pieces are kept under a name that does not say what they were cut from, so a blurry page
    // that stands in for the sharp one must not be cut: it is stretched over the cells, the frame says
    // it is waiting, and the pieces that come after are the sharp ones.
    let cell = (13, 29);
    let rect = Rect::new(0.0, 0.0, 640.0, 12000.0 / (29.0 / ROW));
    let clip = Rect::new(0.0, 0.0, 640.0, f32::from(ROWS) * ROW);
    let items = vec![ground(), picture(rect, rect, clip)];
    let mut screen = Screen::new(Depth::True);
    let mut terminal = Terminal::new(cell, SENTINEL);
    let pieces = |terminal: &Terminal| {
        terminal
            .loads()
            .iter()
            .filter(|load| load.width == 1024)
            .count()
    };
    let standing = frame_of(items.clone(), &Third, cell);
    terminal.feed(&screen.draw(&standing, None));
    assert!(
        standing.waiting,
        "the frame is not waiting for the sharp page"
    );
    assert_eq!(
        pieces(&terminal),
        0,
        "pieces cut from pixels a third of their size"
    );
    let sharp = frame_of(items, &Sandbox::new(), cell);
    terminal.feed(&screen.draw(&sharp, None));
    assert!(pieces(&terminal) > 0, "no piece of the sharp page is sent");
    let image = terminal.show(&sharp);
    for (x, y) in [(5_u32, 3_u32), (700, 400), (1030, 860)] {
        let want = Rgb::new((x % 251) as u8, (y % 241) as u8, 200);
        assert_eq!(image.at(x, y), want, "the pixel ({x}, {y}) of the screen");
    }
}
