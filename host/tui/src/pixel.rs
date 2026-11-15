//! The page as pixels, for a terminal that draws pictures (Kitty's graphics protocol).
//!
//! The terminal's whole window is one picture. The page is laid out in the window's own pixels, at the
//! scale that makes a column of the kit's mono text as wide as a cell of the terminal, its draw list
//! is drawn by the pictures' drawing layer (`pencere_drive::shot`) in the machine's own faces, and
//! the pixels go to the terminal packed (`o=z`).
//!
//! What a frame changes of the window is not sent whole. The draw list of a frame is compared with
//! the one before it, band by band (a band is a row of the terminal's cells across the window): the
//! items that reach a band are the same or are not, and the bands that are not are drawn alone, cut
//! to those rows. What the terminal holds of the window is kept here too (its pixels), so that what
//! was drawn is compared with it, and only the rectangles whose pixels differ are sent, into the
//! picture where it stands, by the protocol's edit of a frame (`a=f`). Where the rows of a rectangle
//! are rows the terminal holds already, a little higher or lower (a page that scrolled), the
//! terminal is told to copy them (`a=c`) and only the rows that came into view are sent. A terminal
//! that refuses the edit is sent the bands as pictures of their own over the window, and the whole
//! window again after [`MOST_LAID`] of them.

use std::fmt::Write as _;

use pencere_drive::shot::{Fonts, Palette, raster, raster_rows, reach};
use pencere_sandbox::Images;
use pencere_view::{DrawList, Family, Font, Measure, Paint, Role};

use crate::output;
use crate::probe::Found;

/// How wide a column of the page's mono text is, in points: the kit's size for the terminal's family.
pub(crate) const COLUMN_POINTS: f32 = 12.5;

/// The number of the first picture this sends: the probe's questions have the ones before it.
const FIRST: u32 = 100;

/// How many bands laid over the window, each a picture of its own, are kept before the whole window
/// is sent again (a terminal that refuses the edit of a frame).
const MOST_LAID: usize = 60;

/// How many separate bands one frame is sent as before they are one, from the first row that changed
/// to the last.
const MOST_BANDS: usize = 12;

/// How many of a frame's bands changed, as a share, past which the frame is one edit of the whole
/// window.
const HALF: usize = 2;

/// How many pixels of a row that are the same, between two that differ, are one change and not two,
/// in cells' widths.
const JOIN_CELLS: u32 = 2;

/// How many rows of the same pixels between two that differ are one change and not two.
const JOIN_ROWS: u32 = 3;

/// How many rectangles one frame is sent as, at the most: more are joined, the two that cost the
/// least first.
pub(crate) const MOST_RECTS: usize = 16;

/// How many rectangles are joined two by two to be few; more than this are one at once.
const MOST_JOINED: usize = 200;

/// The fewest rows a copy of rows the terminal holds is worth, in cells' heights.
const LEAST_MOVE_CELLS: u32 = 1;

/// A rectangle of the window, in pixels: where it starts, across and down, and how large it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Area {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Area {
    pub(crate) fn right(&self) -> u32 {
        self.x + self.w
    }

    pub(crate) fn bottom(&self) -> u32 {
        self.y + self.h
    }
}

/// Rows of the window the terminal is told to copy to others, in the same picture: the rectangle
/// they are copied to, from rows `shift` further down (up where negative).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Moved {
    pub to: Area,
    pub shift: i32,
}

/// What one frame of the window wrote.
#[derive(Debug, Default)]
pub(crate) struct Written {
    /// The bytes for the terminal.
    pub bytes: Vec<u8>,
    /// The window was sent whole, as a new picture.
    pub whole: bool,
    /// The bands drawn again, as edits or as pictures laid over: the pixel row each starts at, and
    /// how many rows it is.
    pub bands: Vec<(u32, u32)>,
    /// The rectangles of the window sent (the ones of those bands whose pixels differ).
    pub areas: Vec<Area>,
    /// The rectangles the terminal was told to copy from where they stand.
    pub copies: Vec<Moved>,
    /// A picture the page has is not drawn yet: the frame is drawn again soon.
    pub waiting: bool,
}

/// What a frame was made of, and where in the window each item reaches.
struct Frame {
    list: DrawList,
    /// For each item, which pixels of a picture it was drawn with (none: the picture was not there);
    /// zero for what is no picture.
    ready: Vec<u64>,
    /// The items that reach each band, in the order they are drawn.
    bands: Vec<Vec<u32>>,
}

/// What the terminal holds of the window.
struct Shown {
    size: (u32, u32),
    /// The picture that is the window, and the layer it is in.
    id: u32,
    frame: Frame,
    /// The pixels the picture holds, four bytes each, a row after a row.
    held: Vec<u8>,
}

/// The window of a terminal as the page's pixels.
pub(crate) struct Pixel {
    /// The faces, at the scale of the page: they measure what the page lays out and draw what it
    /// painted.
    fonts: Fonts,
    palette: Palette,
    /// How many pixels a cell is, across and down, and how many cells the window has.
    cell: (u32, u32),
    cells: (u16, u16),
    scale: f32,
    /// The terminal takes pixels that are packed.
    packed: bool,
    /// The terminal takes the edit of a frame.
    edits: bool,
    /// The terminal copies a rectangle of a frame to another place in it.
    compose: bool,
    shown: Option<Shown>,
    /// The next picture's number, and the layer it is put in.
    next: u32,
    layer: i32,
    /// The pictures the terminal holds of ours, which are given up with a new window.
    held: Vec<u32>,
    /// How many of them are bands laid over the window.
    laid: usize,
    /// The last frame was sent as edits of the picture of that number.
    edited: Option<u32>,
    /// The last frame had rows of the picture of that number copied.
    copied: Option<u32>,
}

/// How many pixels a column of the kit's mono text is at a scale of one point to a pixel, as the
/// faces have it: the face's advance in ems, times the kit's size for it.
fn column_of(fonts: &Fonts) -> f32 {
    let mut fonts = fonts.clone().scaled(1.0);
    let font = Font {
        family: Family::Terminal,
        size: 1000.0,
        weight: 400,
        italic: false,
    };
    let advance = fonts.measure(&font, "M").width / 1000.0;
    COLUMN_POINTS * if advance > 0.0 { advance } else { 0.6 }
}

/// How many pixels to the point make a column of the page's mono text as wide as a cell: the cell's
/// width over the page's column in points (the face's advance in ems times the kit's text size).
pub(crate) fn scale_for(fonts: &Fonts, cell_width: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let width = cell_width.max(1) as f32;
    width / column_of(fonts)
}

/// The palette the page is drawn in, for a terminal whose ground is as `found` says.
pub(crate) fn palette_of(found: &Found) -> Palette {
    if found.theme.is_dark() {
        Palette::Dark
    } else {
        Palette::Light
    }
}

/// The page's ground in `palette`: red, green and blue, from 0 to 255 (what the terminal's own ground
/// is set to).
pub(crate) fn ground_of(palette: Palette) -> [u8; 3] {
    let [red, green, blue, _] = palette.role(Role::Background);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    [red, green, blue].map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// `pixels` (four bytes each, `width` across) with `right` columns more at the right and `below` rows
/// more at the foot: the last column and the last row repeated into them.
fn extend(pixels: &[u8], width: usize, right: usize, below: usize) -> Vec<u8> {
    let row = width * 4;
    if row == 0 || (right == 0 && below == 0) {
        return pixels.to_vec();
    }
    let rows: Vec<&[u8]> = pixels.chunks_exact(row).collect();
    let mut out = Vec::with_capacity((width + right) * (rows.len() + below) * 4);
    for at in 0..rows.len() + below {
        let Some(line) = rows.get(at).or(rows.last()) else {
            break;
        };
        out.extend_from_slice(line);
        if let Some(last) = line.rchunks_exact(4).next() {
            for _ in 0..right {
                out.extend_from_slice(last);
            }
        }
    }
    out
}

impl Pixel {
    /// The window of `cells` cells of `cell` pixels, in `fonts`, in the palette of the terminal's
    /// ground, for a terminal that answered as `found` says.
    pub(crate) fn new(fonts: Fonts, found: &Found, cell: (u32, u32), cells: (u16, u16)) -> Self {
        let scale = scale_for(&fonts, cell.0);
        Self {
            fonts: fonts.scaled(scale),
            palette: palette_of(found),
            cell: (cell.0.max(1), cell.1.max(1)),
            cells,
            scale,
            packed: found.compression,
            edits: found.frames,
            compose: found.compose,
            shown: None,
            next: FIRST,
            layer: 0,
            held: Vec::new(),
            laid: 0,
            edited: None,
            copied: None,
        }
    }

    /// What measures the page: the faces that draw it.
    pub(crate) fn measure(&self) -> Box<dyn Measure> {
        Box::new(self.fonts.clone())
    }

    /// How many pixels the window is, across and down: its cells, whole.
    pub(crate) fn size(&self) -> (u32, u32) {
        (
            u32::from(self.cells.0) * self.cell.0,
            u32::from(self.cells.1) * self.cell.1,
        )
    }

    /// How many pixels the picture is, across and down: the window's, and a cell more of each (what
    /// is past the cells is the page's edge).
    pub(crate) fn extent(&self) -> (u32, u32) {
        let size = self.size();
        (size.0 + self.cell.0, size.1 + self.cell.1)
    }

    /// The faces the page is measured and drawn in.
    pub(crate) fn fonts(&self) -> &Fonts {
        &self.fonts
    }

    /// How many pixels a cell is, and how many cells the window has.
    pub(crate) fn cell(&self) -> (u32, u32) {
        self.cell
    }

    /// How many pixels to a point of the page.
    pub(crate) fn scale(&self) -> f32 {
        self.scale
    }

    /// The window is `cells` cells of `cell` pixels now. Whether the page is to be measured again
    /// (the scale changed): the next frame is the whole window either way if its size did.
    pub(crate) fn fit(&mut self, cell: (u32, u32), cells: (u16, u16)) -> bool {
        let cell = (cell.0.max(1), cell.1.max(1));
        let was = (self.cell, self.cells, self.scale);
        let scale = scale_for(&self.fonts, cell.0);
        self.cell = cell;
        self.cells = cells;
        let rescaled = (scale - self.scale).abs() > f32::EPSILON;
        if rescaled {
            self.scale = scale;
            self.fonts = self.fonts.clone().scaled(scale);
        }
        if (cell, cells, scale) != was {
            self.shown = None;
        }
        rescaled
    }

    /// What the terminal holds is not what was sent (it was cleared, or lent to a command): the next
    /// frame is the whole window.
    pub(crate) fn forget(&mut self) {
        self.shown = None;
        self.held.clear();
        self.laid = 0;
        self.copied = None;
    }

    /// The terminal answered about a picture of ours: `ok`, or it refused. A refused edit of a frame
    /// is not made again: the bands are laid over the window, and the window is sent whole. A
    /// refused copy of rows is not made again either: the window is sent whole, and its rows are
    /// edited from then on. Whether the next frame is due for it.
    pub(crate) fn answered(&mut self, id: u32, ok: bool) -> bool {
        if ok {
            return false;
        }
        if self.copied == Some(id) && self.compose {
            self.compose = false;
            self.copied = None;
            self.shown = None;
            return true;
        }
        if self.edited != Some(id) || !self.edits {
            return false;
        }
        self.edits = false;
        self.shown = None;
        true
    }

    /// The frame `list` of the page, as what the terminal is to be written to show it.
    pub(crate) fn draw(&mut self, list: DrawList, images: Option<&Images>) -> Written {
        let size = self.size();
        let frame = self.frame_of(list, images, size.1);
        let waiting = frame
            .ready
            .iter()
            .zip(&frame.list.items)
            .any(|(id, item)| *id == 0 && matches!(item, Paint::Image(_)));
        let mut written = Written {
            waiting,
            ..Written::default()
        };
        let mut out = String::new();
        let before = self.shown.take();
        match before {
            Some(shown) if shown.size == size => {
                let rows = changed(&shown.frame, &frame);
                let runs = runs_of(&rows);
                let count = rows.len();
                let touched: usize = runs.iter().map(|run| run.1).sum();
                if runs.is_empty() {
                    self.shown = Some(Shown { frame, ..shown });
                    return written;
                }
                let fits = self.edits || self.laid + runs.len() <= MOST_LAID;
                if !fits {
                    self.whole(&mut out, size, frame, images, &mut written);
                } else {
                    // Many bands are one, from the first row that changed to the last; most of the
                    // window changing is the whole window as one edit.
                    let runs = if touched * HALF > count || runs.len() > MOST_BANDS {
                        let first = runs.first().map_or(0, |run| run.0);
                        let last = runs.last().map_or(0, |run| run.0 + run.1);
                        vec![(first, last - first)]
                    } else {
                        runs
                    };
                    let mut shown = shown;
                    self.bands(
                        &mut out,
                        &mut shown,
                        size,
                        &frame,
                        images,
                        &runs,
                        &mut written,
                    );
                    self.shown = Some(Shown { frame, ..shown });
                }
            }
            _ => self.whole(&mut out, size, frame, images, &mut written),
        }
        written.bytes = out.into_bytes();
        written
    }

    /// The window, whole, as a new picture over the one there was.
    fn whole(
        &mut self,
        out: &mut String,
        size: (u32, u32),
        frame: Frame,
        images: Option<&Images>,
        written: &mut Written,
    ) {
        let Ok(drawn) = raster(&frame.list, size, &self.fonts, images, self.palette) else {
            return;
        };
        let past = self.extent();
        let pixels = extend(
            drawn.rgba(),
            size.0 as usize,
            self.cell.0 as usize,
            self.cell.1 as usize,
        );
        let id = self.number();
        let layer = self.next_layer();
        // At the window's corner, the cursor where it was.
        out.push_str("\x1b[H");
        let header = format!("a=T,f=32,s={},v={},i={id},z={layer},C=1", past.0, past.1);
        transmit(out, &header, &pixels, self.packed, 1);
        for old in self.held.drain(..) {
            output::free(out, old);
        }
        self.held.push(id);
        self.laid = 0;
        self.edited = None;
        self.copied = None;
        written.whole = true;
        written.bands = vec![(0, past.1)];
        self.shown = Some(Shown {
            size,
            id,
            frame,
            held: pixels,
        });
    }

    /// The bands `runs` (the first band and how many, in cells' rows), drawn alone, and what of them
    /// is not what the terminal holds put where it belongs.
    #[allow(clippy::too_many_arguments)]
    fn bands(
        &mut self,
        out: &mut String,
        shown: &mut Shown,
        size: (u32, u32),
        frame: &Frame,
        images: Option<&Images>,
        runs: &[(usize, usize)],
        written: &mut Written,
    ) {
        // The picture is wider than the window, and what is held of it is as wide.
        let past = self.extent();
        let width = past.0 as usize;
        // The rectangles a frame is sent as are shared by its bands.
        let most = (MOST_RECTS / runs.len().max(1)).max(2);
        for &(first, count) in runs {
            #[allow(clippy::cast_possible_truncation)]
            let (top, rows) = (
                first as u32 * self.cell.1,
                (count as u32 * self.cell.1).min(size.1.saturating_sub(first as u32 * self.cell.1)),
            );
            let Ok(strip) = raster_rows(
                &frame.list,
                size,
                &self.fonts,
                images,
                self.palette,
                (top, rows),
            ) else {
                continue;
            };
            // The band that reaches the foot of the window goes on to the foot of the picture.
            let below = if top + rows >= size.1 { self.cell.1 } else { 0 };
            let rows = rows + below;
            let strip = extend(
                strip.rgba(),
                size.0 as usize,
                self.cell.0 as usize,
                below as usize,
            );
            let pixels = strip.as_slice();
            written.bands.push((top, rows));
            if !self.edits {
                // Over the window, in the row of cells it starts at.
                let id = self.number();
                let layer = self.next_layer();
                let row = top / self.cell.1 + 1;
                let _ = write!(out, "\x1b[{row};1H");
                let header = format!("a=T,f=32,s={},v={rows},i={id},z={layer},C=1", past.0);
                transmit(out, &header, pixels, self.packed, 1);
                self.held.push(id);
                self.laid += 1;
                written.areas.push(Area {
                    x: 0,
                    y: top,
                    w: past.0,
                    h: rows,
                });
                keep(&mut shown.held, width, top, pixels);
                continue;
            }
            let areas = self.differing(&shown.held, pixels, past, top, rows, most);
            // What moved (a page that scrolled) is copied, within what the terminal holds; what
            // is left to differ after that is sent.
            let moved = if self.compose {
                self.moved(out, shown, pixels, past, top, &areas, frame, written)
            } else {
                false
            };
            let areas = if moved {
                self.differing(&shown.held, pixels, past, top, rows, most)
            } else {
                areas
            };
            for area in areas {
                let cut = cut(pixels, width, top, area);
                let header = format!(
                    "a=f,i={},r=1,x={},y={},s={},v={},f=32",
                    shown.id, area.x, area.y, area.w, area.h
                );
                transmit(out, &header, &cut, self.packed, 1);
                self.edited = Some(shown.id);
                written.areas.push(area);
            }
            keep(&mut shown.held, width, top, pixels);
        }
    }

    /// The rectangles of the rows `top`..`top + rows` whose pixels are not what `held` has: the rows
    /// of `pixels` (a strip of those rows) are compared with it, a rectangle is where some differ
    /// and the ones near it do too, and where there are more than `most` of them the ones that
    /// cost the least to join are joined.
    fn differing(
        &self,
        held: &[u8],
        pixels: &[u8],
        size: (u32, u32),
        top: u32,
        rows: u32,
        most: usize,
    ) -> Vec<Area> {
        let runs = row_runs(held, pixels, size.0, top, rows, self.cell.0 * JOIN_CELLS);
        let mut areas = clusters(&runs, top, JOIN_ROWS);
        if areas.len() > most {
            areas = squeeze(areas, most);
        }
        // Rectangles over most of what they are drawn on are one: a frame is cheaper as one edit.
        let sum: u64 = areas.iter().map(|a| u64::from(a.w) * u64::from(a.h)).sum();
        if areas.len() > 1
            && let Some(whole) = bounding(&areas)
            && sum * 2 > u64::from(whole.w) * u64::from(whole.h)
        {
            areas = vec![whole];
        }
        areas
    }

    /// Tells the terminal to copy the rows of the rectangles `areas` that are rows it holds a little
    /// further down or up, as far as the page's own items say they moved; the copies are made in
    /// what is kept of the window (`shown.held`) too. Whether any was.
    #[allow(clippy::too_many_arguments)]
    fn moved(
        &mut self,
        out: &mut String,
        shown: &mut Shown,
        pixels: &[u8],
        size: (u32, u32),
        top: u32,
        areas: &[Area],
        new: &Frame,
        written: &mut Written,
    ) -> bool {
        let width = size.0 as usize;
        let mut any = false;
        let least = LEAST_MOVE_CELLS * self.cell.1;
        for area in areas {
            if area.h < least * 2 {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let shifts = shifts_of(&shown.frame, new, area, 2.0 * self.cell.1 as f32);
            if shifts.is_empty() {
                continue;
            }
            let mut found = spans(&shown.held, pixels, width, top, area, &shifts, least);
            if shifts.first().is_some_and(|shift| *shift < 0) {
                found.reverse();
            }
            for span in found {
                // Copied in chunks that do not meet where they are from, in the order that
                // leaves each chunk's source as it was.
                let step = span.shift.unsigned_abs().max(1);
                let (mut done, mut chunks) = (0, Vec::new());
                while done < span.rows {
                    let rows = step.min(span.rows - done);
                    chunks.push((span.first + done, rows));
                    done += rows;
                }
                if span.shift < 0 {
                    chunks.reverse();
                }
                for (first, rows) in chunks {
                    let to = Area {
                        x: area.x,
                        y: first,
                        w: area.w,
                        h: rows,
                    };
                    let from_y = first.checked_add_signed(span.shift).unwrap_or(first);
                    let _ = write!(
                        out,
                        "\x1b_Ga=c,i={},r=1,c=1,X={},Y={from_y},x={},y={first},w={},h={rows},C=1,q=1\x1b\\",
                        shown.id, area.x, area.x, area.w
                    );
                    for row in 0..rows {
                        let (a, b) = (
                            (from_y + row) as usize * width + area.x as usize,
                            (first + row) as usize * width + area.x as usize,
                        );
                        shown
                            .held
                            .copy_within(a * 4..(a + area.w as usize) * 4, b * 4);
                    }
                    written.copies.push(Moved {
                        to,
                        shift: span.shift,
                    });
                    self.copied = Some(shown.id);
                    any = true;
                }
            }
        }
        any
    }

    fn number(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }

    fn next_layer(&mut self) -> i32 {
        self.layer = self.layer.saturating_add(1);
        self.layer
    }

    /// The frame of `list`: what each item shows of a picture, and the bands it reaches.
    fn frame_of(&self, list: DrawList, images: Option<&Images>, height: u32) -> Frame {
        let ready = list
            .items
            .iter()
            .map(|item| match item {
                Paint::Image(picture) => images
                    .and_then(|images| images.picture(picture))
                    .map_or(0, |bitmap| bitmap.id.saturating_add(1)),
                _ => 0,
            })
            .collect();
        let count = usize::try_from(height.div_ceil(self.cell.1)).unwrap_or(0);
        let mut bands: Vec<Vec<u32>> = vec![Vec::new(); count];
        #[allow(clippy::cast_precision_loss)]
        let (cell, window) = (self.cell.1 as f32, height as f32);
        for (n, item) in list.items.iter().enumerate() {
            let Some((top, bottom)) = reach(item, &self.fonts) else {
                continue;
            };
            let (top, bottom) = (top.max(0.0), bottom.min(window));
            if bottom <= top {
                continue;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (first, last) = (
                (top / cell) as usize,
                ((bottom - 0.001).max(top) / cell) as usize,
            );
            for band in first..=last {
                if let (Some(items), Ok(n)) = (bands.get_mut(band), u32::try_from(n)) {
                    items.push(n);
                }
            }
        }
        Frame { list, ready, bands }
    }
}

/// Which bands of `new` are not what they were in `old`: the items that reach one, and what they
/// show of their pictures, are the same, in the same order, or they are not.
fn changed(old: &Frame, new: &Frame) -> Vec<bool> {
    new.bands
        .iter()
        .enumerate()
        .map(|(band, now)| {
            let Some(was) = old.bands.get(band) else {
                return true;
            };
            was.len() != now.len()
                || was.iter().zip(now).any(|(&a, &b)| {
                    let (a, b) = (a as usize, b as usize);
                    old.list.items.get(a) != new.list.items.get(b)
                        || old.ready.get(a) != new.ready.get(b)
                })
        })
        .collect()
}

/// The runs of `true`: where each starts and how long it is.
fn runs_of(rows: &[bool]) -> Vec<(usize, usize)> {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for (at, row) in rows.iter().enumerate() {
        if !row {
            continue;
        }
        match runs.last_mut() {
            Some((first, count)) if *first + *count == at => *count += 1,
            _ => runs.push((at, 1)),
        }
    }
    runs
}

/// The sequences that give the terminal `rgba` (the pixels of a picture or of a part of one), under
/// the keys `header`: packed with zlib where the terminal takes it and it is smaller so. `quiet`: `1`
/// has the terminal answer only a refusal.
fn transmit(out: &mut String, header: &str, rgba: &[u8], packed: bool, quiet: u8) {
    if packed {
        let deflated = miniz_oxide::deflate::compress_to_vec_zlib(rgba, output::PACK_LEVEL);
        if deflated.len() < rgba.len() {
            let header = format!("{header},o=z");
            output::pieces_quiet(out, &header, &output::base64(&deflated), quiet);
            return;
        }
    }
    output::pieces_quiet(out, header, &output::base64(rgba), quiet);
}

/// Where in each of the `rows` rows from `top` the pixels of `pixels` (a strip of those rows) are not
/// what `held` (the whole window) has: runs of pixels, a run and the next being one where fewer
/// than `join` pixels are between them.
fn row_runs(
    held: &[u8],
    pixels: &[u8],
    width: u32,
    top: u32,
    rows: u32,
    join: u32,
) -> Vec<Vec<(u32, u32)>> {
    let row = width as usize * 4;
    (0..rows as usize)
        .map(|r| {
            let was = held.get((top as usize + r) * row..(top as usize + r + 1) * row);
            let now = pixels.get(r * row..(r + 1) * row);
            match (was, now) {
                (Some(was), Some(now)) => diff_runs(was, now, join),
                _ => Vec::new(),
            }
        })
        .collect()
}

/// The runs of pixels, four bytes each, that `a` and `b` do not share.
fn diff_runs(a: &[u8], b: &[u8], join: u32) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    if a == b {
        return runs;
    }
    for (x, (p, q)) in a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .enumerate()
    {
        if p == q {
            continue;
        }
        let x = u32::try_from(x).unwrap_or(u32::MAX);
        match runs.last_mut() {
            Some(last) if x < last.1.saturating_add(join) => last.1 = x + 1,
            _ => runs.push((x, x + 1)),
        }
    }
    runs
}

/// The rectangles the runs of rows (the first of them the row `top`) make: runs that stand over one
/// another, in rows no more than `join` apart, are one.
fn clusters(rows: &[Vec<(u32, u32)>], top: u32, join: u32) -> Vec<Area> {
    // A rectangle that is still growing: its columns, and the first and the last row it has.
    struct Open {
        from: u32,
        to: u32,
        first: u32,
        last: u32,
    }
    let mut open: Vec<Open> = Vec::new();
    let mut done: Vec<Area> = Vec::new();
    let close = |o: Open, done: &mut Vec<Area>| {
        done.push(Area {
            x: o.from,
            y: top + o.first,
            w: o.to - o.from,
            h: o.last - o.first + 1,
        });
    };
    for (r, runs) in rows.iter().enumerate() {
        let r = u32::try_from(r).unwrap_or(u32::MAX);
        for &(from, to) in runs {
            match open
                .iter_mut()
                .find(|o| from < o.to && o.from < to && r <= o.last.saturating_add(join))
            {
                Some(o) => {
                    o.from = o.from.min(from);
                    o.to = o.to.max(to);
                    o.last = r;
                }
                None => open.push(Open {
                    from,
                    to,
                    first: r,
                    last: r,
                }),
            }
        }
        // Rectangles that came to meet are one.
        let mut at = 0;
        while at < open.len() {
            let mut other = at + 1;
            let mut merged = false;
            while other < open.len() {
                let meets = match (open.get(at), open.get(other)) {
                    (Some(a), Some(b)) => a.from < b.to && b.from < a.to,
                    _ => false,
                };
                if meets {
                    let b = open.remove(other);
                    if let Some(a) = open.get_mut(at) {
                        a.from = a.from.min(b.from);
                        a.to = a.to.max(b.to);
                        a.first = a.first.min(b.first);
                        a.last = a.last.max(b.last);
                    }
                    merged = true;
                } else {
                    other += 1;
                }
            }
            if !merged {
                at += 1;
            }
        }
        let mut still = Vec::new();
        for o in open {
            if r > o.last.saturating_add(join) {
                close(o, &mut done);
            } else {
                still.push(o);
            }
        }
        open = still;
    }
    for o in open {
        close(o, &mut done);
    }
    done.sort_by_key(|a| (a.y, a.x));
    done
}

/// The smallest rectangle that has all of `areas`.
/// `areas` as no more than `most` rectangles: the two that cost the least to make one (the room
/// the one rectangle has more than the two had) are joined, until they are few enough. Where
/// there are very many they are the one rectangle that holds them all.
fn squeeze(mut areas: Vec<Area>, most: usize) -> Vec<Area> {
    if areas.len() > MOST_JOINED {
        return bounding(&areas).into_iter().collect();
    }
    let size = |area: &Area| u64::from(area.w) * u64::from(area.h);
    while areas.len() > most.max(1) {
        let mut best: Option<(i64, usize, usize, Area)> = None;
        for (i, a) in areas.iter().enumerate() {
            for (j, b) in areas.iter().enumerate().skip(i + 1) {
                let Some(joined) = bounding(&[*a, *b]) else {
                    continue;
                };
                #[allow(clippy::cast_possible_wrap)]
                let waste = size(&joined) as i64 - size(a) as i64 - size(b) as i64;
                if best.is_none_or(|(least, ..)| waste < least) {
                    best = Some((waste, i, j, joined));
                }
            }
        }
        let Some((_, i, j, joined)) = best else {
            break;
        };
        areas.swap_remove(j);
        if let Some(kept) = areas.get_mut(i) {
            *kept = joined;
        }
    }
    areas
}

fn bounding(areas: &[Area]) -> Option<Area> {
    let x = areas.iter().map(|a| a.x).min()?;
    let y = areas.iter().map(|a| a.y).min()?;
    let right = areas.iter().map(Area::right).max()?;
    let bottom = areas.iter().map(Area::bottom).max()?;
    Some(Area {
        x,
        y,
        w: right - x,
        h: bottom - y,
    })
}

/// The pixels of `area` from a strip of rows (the first of them the row `top` of the window).
fn cut(strip: &[u8], width: usize, top: u32, area: Area) -> Vec<u8> {
    let mut out = Vec::with_capacity(area.w as usize * area.h as usize * 4);
    for row in area.y..area.bottom() {
        let from = ((row - top) as usize * width + area.x as usize) * 4;
        if let Some(pixels) = strip.get(from..from + area.w as usize * 4) {
            out.extend_from_slice(pixels);
        }
    }
    out
}

/// The rows of a strip (the first of them the row `top` of the window) put in what is kept of it.
fn keep(held: &mut [u8], width: usize, top: u32, strip: &[u8]) {
    let from = top as usize * width * 4;
    if let Some(rows) = held.get_mut(from..from + strip.len()) {
        rows.copy_from_slice(strip);
    }
}

/// How far down the rows of `area` were, if the text in them is text that was in the frame before,
/// further down or up: what the items of the two frames say, the likeliest first, and the rows
/// either side of it (a line's rows are rounded to a pixel). None where nothing says it.
fn shifts_of(old: &Frame, new: &Frame, area: &Area, line: f32) -> Vec<i32> {
    // A line starts above the rows its letters are in.
    #[allow(clippy::cast_precision_loss)]
    let (top, bottom) = (area.y as f32 - line, area.bottom() as f32 + 1.0);
    let mut votes: Vec<(i32, u32)> = Vec::new();
    for item in &new.list.items {
        let Paint::Text(now) = item else {
            continue;
        };
        if now.origin.y < top || now.origin.y >= bottom || now.text.trim().is_empty() {
            continue;
        }
        let was = old.list.items.iter().find_map(|item| match item {
            Paint::Text(was)
                if was.text == now.text
                    && was.font == now.font
                    && was.color == now.color
                    && (was.origin.x - now.origin.x).abs() < 0.01
                    && (was.origin.y - now.origin.y).abs() >= 1.0 =>
            {
                Some(was)
            }
            _ => None,
        });
        let Some(was) = was else {
            continue;
        };
        #[allow(clippy::cast_possible_truncation)]
        let shift = (was.origin.y - now.origin.y).round() as i32;
        match votes.iter_mut().find(|(have, _)| *have == shift) {
            Some((_, count)) => *count += 1,
            None => votes.push((shift, 1)),
        }
    }
    votes.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let mut shifts = Vec::new();
    if let Some(&(shift, count)) = votes.first()
        && count >= 2
    {
        for near in [shift, shift - 1, shift + 1] {
            if near != 0 && near.signum() == shift.signum() {
                shifts.push(near);
            }
        }
    }
    shifts
}

/// Rows of an area that are rows of the window `shift` rows further down: `rows` of them from `first`.
#[derive(Clone, Copy, Debug)]
struct Span {
    first: u32,
    rows: u32,
    shift: i32,
}

/// The spans of the rows of `area` (of a strip of rows from `top`, of a window `width` across) that
/// are, to the pixel, rows `held` has further down or up by one of `shifts`. A span is as long as one
/// of them goes on matching, and is kept if it is `least` rows or more, and changes as many.
fn spans(
    held: &[u8],
    strip: &[u8],
    width: usize,
    top: u32,
    area: &Area,
    shifts: &[i32],
    least: u32,
) -> Vec<Span> {
    let height = held.len() / (width * 4).max(1);
    let (x, w) = (area.x as usize * 4, area.w as usize * 4);
    fn row_at(pixels: &[u8], row: usize, width: usize, (x, w): (usize, usize)) -> Option<&[u8]> {
        let from = row * width * 4 + x;
        pixels.get(from..from + w)
    }
    macro_rules! row_of {
        ($pixels:expr, $row:expr) => {
            row_at($pixels, $row, width, (x, w))
        };
    }
    let mut found: Vec<Span> = Vec::new();
    let mut current: Option<(Span, u32)> = None;
    let mut close = |current: &mut Option<(Span, u32)>| {
        if let Some((span, changing)) = current.take()
            && span.rows >= least
            && changing >= least
        {
            found.push(span);
        }
    };
    for y in area.y..area.bottom() {
        let Some(now) = row_of!(strip, (y - top) as usize) else {
            close(&mut current);
            continue;
        };
        let changing = row_of!(held, y as usize) != Some(now);
        let matching = |shift: i32| -> bool {
            y.checked_add_signed(shift)
                .filter(|from| (*from as usize) < height)
                .and_then(|from| row_of!(held, from as usize))
                == Some(now)
        };
        match &mut current {
            Some((span, count)) if matching(span.shift) => {
                span.rows += 1;
                *count += u32::from(changing);
            }
            _ => {
                close(&mut current);
                if changing
                    && let Some(shift) = shifts.iter().copied().find(|shift| matching(*shift))
                {
                    current = Some((
                        Span {
                            first: y,
                            rows: 1,
                            shift,
                        },
                        1,
                    ));
                }
            }
        }
    }
    close(&mut current);
    found
}
