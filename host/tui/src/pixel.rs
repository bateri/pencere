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
//! picture where it stands, by the protocol's edit of a frame (`a=f`). Where the rows of a frame are
//! rows the terminal holds already, a little higher or lower (a page that scrolled), the terminal is
//! told to copy them (`a=c`) and only the rows that came into view are sent. Those rows are found
//! from the two draw lists, before a row of the new frame is drawn (`Pixel::slides`), so that the
//! rows that are copied are not drawn at all; where the lists do not say it, the rows are drawn and
//! looked for in what was drawn (`Pixel::moved`). A terminal that refuses the edit is sent the bands
//! as pictures of their own over the window, and the whole window again after [`MOST_LAID`] of them.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};
use std::fmt::Write as _;

use miniz_oxide::deflate::core::{
    CompressorOxide, TDEFLFlush, TDEFLStatus, compress, create_comp_flags_from_zip_params,
};
use pencere_drive::shot::{
    Fonts, Palette, Scratch, Side, across, raster_part_in, raster_rows_in, reach, slide, spread,
};
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

/// How many pixels more a rectangle may have than the two it is made of have between them, for the one
/// to be sent and not the two: what a rectangle costs besides its pixels (the command that says where
/// it goes and how large it is, and the terminal's reading of it) is worth about this many pixels of a
/// page that are the same, which are sent too where they lie inside it. Chosen by joining the
/// rectangles of recorded frames (a selection that lights a word all over a page, and a drag over
/// lines) at 100 to 3200: the bytes were fewest from 1000 to 1600, and the lowest of those is taken.
const JOIN_PIXELS: i64 = 1000;

/// How many pixels more a rectangle may have than a run of a row that stands in its columns, for the
/// run to be of the rectangle (a stack of rows) and not a rectangle of its own: a rectangle grows
/// while it is stacked, and each row that is added is more to what it already has. Chosen with
/// [`JOIN_PIXELS`] over the same frames, from 100 to 1600: the same bytes from 400 to 1200, more
/// above it, where a stack takes in what is beside it.
const STACK_PIXELS: i64 = 400;

/// How many rectangles one frame is sent as, at the most: past what [`JOIN_PIXELS`] allows, more are
/// joined, the two that cost the least first. Chosen from recorded frames and a terminal's timing: over
/// the frames of a selection the bytes were 15% fewer with 32 rectangles than with 16, and 2% fewer
/// with 64 than with 32; a terminal (Ghostty on a Mac, under load) read a command in the order of 15
/// microseconds, as it read about 0.4 KB, so that the commands of a frame are no more than the bytes
/// of a few rectangles. The number is a point on the curve of the bytes and not a limit of the terminal.
pub(crate) const MOST_RECTS: usize = 32;

/// How many rectangles are joined two by two to be few; more than this are one at once.
const MOST_JOINED: usize = 200;

/// How many pieces (the rectangles the runs of rows are stacked as) are joined, by [`join`], which
/// keeps the pairs of them in a queue (the squares of the pieces); where there are more the rectangles
/// are those that `regions` makes, as they were made before the rows were stacked, and that may be the
/// one box that has them all.
const MOST_PIECES: usize = 256;

/// How many tall rectangles a frame is looked in for rows that moved, at the most: the same as it was
/// before the rectangles a frame is sent as were changed, so that the rows that moved are found as
/// they were.
const MOST_REGIONS: usize = 16;

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

/// Two pieces that may be joined, as `join` queues them: what it costs to make them one (the pair
/// that costs the least first), which two they are, and how many times each had been joined when the
/// pair was made (a pair of what was joined since is passed over).
pub(crate) type Pair = Reverse<(i64, usize, usize, u32, u32)>;

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
    /// How many pixels were drawn for the frame: the strips of the bands, as wide as they were drawn
    /// (the window whole where it was sent so).
    pub rastered: u64,
    /// A picture the page has is not drawn yet: the frame is drawn again soon.
    pub waiting: bool,
}

/// How a frame that scrolled is made of the window the terminal holds ([`Pixel::slides`]).
struct Plan {
    /// Rows to copy, in the columns: the first row, the end, and how many rows further down the rows
    /// are that they are copied from (up where negative).
    spans: Vec<(u32, u32, i32)>,
    /// The columns the rows are copied in, from and to.
    columns: (u32, u32),
    /// The rows that are drawn after the copies: the first row and how many.
    strips: Vec<(u32, u32)>,
}

/// Rows of the window that are drawn alone, and the columns of them that are: none where it is the
/// width of the picture.
#[derive(Clone, Copy, Debug)]
struct Strip {
    /// The first row, and how many rows.
    top: u32,
    rows: u32,
    /// The first column and the end of the columns drawn: the rest of the strip is as it was.
    columns: Option<(u32, u32)>,
}

/// What a frame was made of, and where in the window each item reaches.
struct Frame {
    list: DrawList,
    /// For each item, which pixels of a picture it was drawn with (none: the picture was not there);
    /// zero for what is no picture.
    ready: Vec<u64>,
    /// The items that reach each band, in the order they are drawn.
    bands: Vec<Vec<u32>>,
    /// The rows each item reaches, pixels (`reach`): none for what draws nothing.
    reach: Vec<Option<(f32, f32)>>,
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
    /// The buffers the bands are drawn in, kept from frame to frame.
    scratch: Scratch,
    /// What the pixels are packed and written in, kept from frame to frame.
    wire: Wire,
    /// The queue in which the rectangles of a frame are joined, kept from frame to frame: it holds the
    /// pairs of them, which are many where a frame has many changes far apart.
    pairs: Vec<Pair>,
}

/// What the pixels of a frame are packed in, kept from one frame to the next: the deflate's state is
/// as large as a picture's worth of pages, and a frame does not ask the system for it again for each
/// rectangle it sends.
pub(crate) struct Wire {
    pub(crate) comp: CompressorOxide,
    /// The packed pixels, as large as the largest pack of a rectangle.
    pub(crate) deflated: Vec<u8>,
    /// The base64 of what is sent.
    encoded: String,
    /// The pixels of a rectangle that is not as wide as the window, cut from its band.
    cut: Vec<u8>,
}

impl Wire {
    pub(crate) fn new() -> Self {
        let flags = create_comp_flags_from_zip_params(i32::from(output::PACK_LEVEL), 1, 0);
        Self {
            comp: CompressorOxide::new(flags),
            deflated: Vec::new(),
            encoded: String::new(),
            cut: Vec::new(),
        }
    }
}

/// `rgba` packed with zlib in `deflated`, how many bytes that is: where it is smaller than `rgba` is,
/// and `None` where it is not. The pack is what `miniz_oxide::deflate::compress_to_vec_zlib` makes
/// (the same level, and any zlib reader reads it as the pixels), with the state of `comp` and the
/// buffer kept: the buffer grows to what a rectangle packs to, and no more.
pub(crate) fn deflate(
    comp: &mut CompressorOxide,
    deflated: &mut Vec<u8>,
    rgba: &[u8],
) -> Option<usize> {
    /// The least the buffer is, and so how much a rectangle that packs small asks for.
    const LEAST: usize = 16 * 1024;
    comp.reset();
    if deflated.len() < LEAST {
        deflated.resize(LEAST, 0);
    }
    let (mut input, mut at) = (rgba, 0);
    loop {
        let (status, taken, made) =
            compress(comp, input, deflated.get_mut(at..)?, TDEFLFlush::Finish);
        at += made;
        input = input.get(taken..)?;
        match status {
            TDEFLStatus::Done => return (at < rgba.len()).then_some(at),
            TDEFLStatus::Okay if at < rgba.len() => {
                // More room: twice as much, and no more than the pixels are (the pack that is not
                // smaller than they are is not sent).
                if deflated.len() - at < 30 {
                    let more = (deflated.len() * 2).min(rgba.len().max(LEAST));
                    deflated.resize(more.max(deflated.len() + 30), 0);
                }
            }
            _ => return None,
        }
    }
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
    if found.dark {
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
            scratch: Scratch::new(),
            wire: Wire::new(),
            pairs: Vec::new(),
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
                    let mut shown = shown;
                    // A page that scrolled is the rows the terminal holds, copied, and what is
                    // left of them (found from the two lists, before a row is drawn).
                    let plan = if self.edits && self.compose {
                        self.slides(&shown, &frame, &rows, size)
                    } else {
                        None
                    };
                    if let Some(plan) = plan {
                        self.scrolled(
                            &mut out,
                            &mut shown,
                            size,
                            &frame,
                            images,
                            plan,
                            &mut written,
                        );
                    } else {
                        // Many bands are one, from the first row that changed to the last; most of
                        // the window changing is the whole window as one edit.
                        let runs = if touched * HALF > count || runs.len() > MOST_BANDS {
                            let first = runs.first().map_or(0, |run| run.0);
                            let last = runs.last().map_or(0, |run| run.0 + run.1);
                            vec![(first, last - first)]
                        } else {
                            runs
                        };
                        let strips: Vec<Strip> = runs
                            .iter()
                            .map(|&(first, count)| {
                                let top = u32::try_from(first).unwrap_or(0) * self.cell.1;
                                let rows = u32::try_from(count).unwrap_or(0) * self.cell.1;
                                // Over the window as pictures the strips are as wide as it is.
                                let columns = if self.edits {
                                    self.columns_of(&shown.frame, &frame, first, count, size.0)
                                } else {
                                    None
                                };
                                Strip {
                                    top,
                                    rows: rows.min(size.1.saturating_sub(top)),
                                    columns,
                                }
                            })
                            .collect();
                        self.bands(
                            &mut out,
                            &mut shown,
                            size,
                            &frame,
                            images,
                            &strips,
                            true,
                            &mut written,
                        );
                    }
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
        // The window and the page's edge past it, drawn in buffers of their own: what is drawn is
        // what the terminal holds, and is kept as it is (the buffers of the bands are not grown to
        // the size of the window for it).
        let mut fresh = Scratch::new();
        let Ok(length) = raster_rows_in(
            &mut fresh,
            &frame.list,
            size,
            &self.fonts,
            images,
            self.palette,
            (0, size.1),
            self.cell,
        )
        .map(<[u8]>::len) else {
            return;
        };
        let pixels = fresh.into_pixels(length);
        let past = self.extent();
        let id = self.number();
        let layer = self.next_layer();
        // At the window's corner, the cursor where it was.
        out.push_str("\x1b[H");
        let header = format!("a=T,f=32,s={},v={},i={id},z={layer},C=1", past.0, past.1);
        // Packed in a state of its own, which goes with the frame: the one the bands are sent with is
        // not grown to the size of the window for a frame that comes once.
        transmit(&mut Wire::new(), out, &header, &pixels, self.packed, 1);
        for old in self.held.drain(..) {
            output::free(out, old);
        }
        self.held.push(id);
        self.laid = 0;
        self.edited = None;
        self.copied = None;
        written.whole = true;
        written.bands = vec![(0, past.1)];
        written.rastered = u64::from(past.0) * u64::from(past.1);
        self.shown = Some(Shown {
            size,
            id,
            frame,
            held: pixels,
        });
    }

    /// The strips `strips` (rows, and the columns of them that are not as they were), drawn alone, and
    /// what of them is not what the terminal holds put where it belongs; `moving`: rows of what it
    /// holds a little further down or up (a page that scrolled, that the two lists did not say) are
    /// looked for in what is drawn, and copied.
    #[allow(clippy::too_many_arguments)]
    fn bands(
        &mut self,
        out: &mut String,
        shown: &mut Shown,
        size: (u32, u32),
        frame: &Frame,
        images: Option<&Images>,
        strips: &[Strip],
        moving: bool,
        written: &mut Written,
    ) {
        // The picture is wider than the window, and what is held of it is as wide.
        let past = self.extent();
        let width = past.0 as usize;
        // The tall rectangles in which rows that moved are looked for are shared by the strips.
        let regions_most = (MOST_REGIONS / strips.len().max(1)).max(2);
        // The buffers are taken out of `self` while the strips are drawn in them and sent.
        let mut scratch = std::mem::take(&mut self.scratch);
        let mut cutting = std::mem::take(&mut self.wire.cut);
        let mut pairs = std::mem::take(&mut self.pairs);
        for (
            at,
            &Strip {
                top,
                rows,
                mut columns,
            },
        ) in strips.iter().enumerate()
        {
            // The rectangles a frame is sent as are shared by its strips: what the ones before this
            // one left of them, but two for each of the ones after it.
            let after = strips.len().saturating_sub(at + 1);
            let most = MOST_RECTS
                .saturating_sub(written.areas.len() + 2 * after)
                .max(2);
            // The band that reaches the foot of the window goes on to the foot of the picture, and
            // every band to the page's edge at the right: drawn so, in the buffer, with no copy.
            let below = if top + rows >= size.1 { self.cell.1 } else { 0 };
            let (pixels, left, across, areas, regions) = loop {
                let (left, to) = columns.unwrap_or((0, size.0));
                // The page's edge at the right is drawn where the columns go to the window's edge.
                let right = if to >= size.0 { self.cell.0 } else { 0 };
                let drawn = raster_part_in(
                    &mut scratch,
                    &frame.list,
                    size,
                    &self.fonts,
                    images,
                    self.palette,
                    (top, rows),
                    (left, to),
                    (right, below),
                );
                let Ok(pixels) = drawn else {
                    if columns.take().is_some() {
                        // A part that cannot be drawn alone (a shadow reaches it) is the whole width.
                        continue;
                    }
                    break (None, 0, 0, Vec::new(), Vec::new());
                };
                let across = to - left + right;
                written.rastered += u64::from(across) * u64::from(rows + below);
                if !self.edits {
                    break (Some(pixels), left, across, Vec::new(), Vec::new());
                }
                let (areas, regions) = self.differing(
                    &shown.held,
                    pixels,
                    past.0,
                    (left, across),
                    (top, rows + below),
                    most,
                    (moving && self.compose).then_some(regions_most),
                    &mut pairs,
                );
                break (Some(pixels), left, across, areas, regions);
            };
            let Some(pixels) = pixels else {
                continue;
            };
            let rows = rows + below;
            written.bands.push((top, rows));
            if !self.edits {
                // Over the window, in the row of cells it starts at.
                let id = self.number();
                let layer = self.next_layer();
                let row = top / self.cell.1 + 1;
                let _ = write!(out, "\x1b[{row};1H");
                let header = format!("a=T,f=32,s={},v={rows},i={id},z={layer},C=1", past.0);
                transmit(&mut self.wire, out, &header, pixels, self.packed, 1);
                self.held.push(id);
                self.laid += 1;
                let area = Area {
                    x: 0,
                    y: top,
                    w: past.0,
                    h: rows,
                };
                written.areas.push(area);
                keep_area(
                    &mut shown.held,
                    width,
                    top,
                    pixels,
                    (past.0 as usize, 0),
                    area,
                );
                continue;
            }
            // What moved (a page that scrolled) is copied, within what the terminal holds; what
            // is left to differ after that is sent.
            let moved = if moving && self.compose {
                self.moved(
                    out,
                    shown,
                    pixels,
                    past,
                    (left, across),
                    top,
                    &regions,
                    frame,
                    written,
                )
            } else {
                false
            };
            let areas = if moved {
                self.differing(
                    &shown.held,
                    pixels,
                    past.0,
                    (left, across),
                    (top, rows),
                    most,
                    None,
                    &mut pairs,
                )
                .0
            } else {
                areas
            };
            for area in areas {
                let cut = cut(&mut cutting, pixels, (across as usize, left), top, area);
                let header = format!(
                    "a=f,i={},r=1,x={},y={},s={},v={},f=32",
                    shown.id, area.x, area.y, area.w, area.h
                );
                transmit(&mut self.wire, out, &header, cut, self.packed, 1);
                self.edited = Some(shown.id);
                written.areas.push(area);
                // What is not in a rectangle is what the terminal holds already.
                keep_area(
                    &mut shown.held,
                    width,
                    top,
                    pixels,
                    (across as usize, left as usize),
                    area,
                );
            }
        }
        self.scratch = scratch;
        self.wire.cut = cutting;
        self.pairs = pairs;
    }

    /// In what columns the bands `first`..`first + count` of the window are not what they were, as
    /// far as the two lists say: the items of a band that the two lists have the same at their start,
    /// and the same at their end, are what they were; of the ones between, the pair at the same place
    /// that is not the same (all of them, where the two lists have not as many between them: what was put
    /// in or taken out moved the ones after it) are not, and the columns they reach are not. What is
    /// outside those is drawn by the same items in the same order in both, so it is what the terminal
    /// holds. None where they are most of the window's width, where an item reaches across it (a
    /// shadow's blur), or where none is known: the strip is the width of the picture (and so it is where
    /// a shadow that no item changed reaches it: it is blurred over the width, and the part is refused
    /// by what draws it).
    fn columns_of(
        &self,
        old: &Frame,
        new: &Frame,
        first: usize,
        count: usize,
        width: u32,
    ) -> Option<(u32, u32)> {
        let mut span: Option<(f32, f32)> = None;
        // An item that reaches more than one band of the strip is looked at once.
        let mut seen: HashSet<(bool, usize)> = HashSet::new();
        for band in first..first + count {
            let (was, now) = (old.bands.get(band)?, new.bands.get(band)?);
            let same = |(&a, &b): (&u32, &u32)| {
                let (a, b) = (a as usize, b as usize);
                old.list.items.get(a) == new.list.items.get(b)
                    && old.ready.get(a) == new.ready.get(b)
            };
            let head = was.iter().zip(now).take_while(|&pair| same(pair)).count();
            let tail = was
                .iter()
                .rev()
                .zip(now.iter().rev())
                .take(was.len().min(now.len()) - head)
                .take_while(|&pair| same(pair))
                .count();
            let (Some(was), Some(now)) = (
                was.get(head..was.len() - tail),
                now.get(head..now.len() - tail),
            ) else {
                return None;
            };
            // Between them, as many items in the two lists are the pairs at the same places; where
            // there are more in the one, what was put in or taken out moved the ones after it, and
            // they are all not what they were.
            let paired = was.len() == now.len();
            for (frame, items) in [(old, was), (new, now)] {
                for (at, &n) in items.iter().enumerate() {
                    if paired && was.get(at).zip(now.get(at)).is_some_and(same) {
                        continue;
                    }
                    if !seen.insert((std::ptr::eq(frame, new), n as usize)) {
                        continue;
                    }
                    let Some(item) = frame.list.items.get(n as usize) else {
                        continue;
                    };
                    if let Some((left, right)) = across(item, &self.fonts) {
                        span =
                            Some(span.map_or((left, right), |(l, r)| (l.min(left), r.max(right))));
                    }
                }
            }
        }
        let (left, right) = span?;
        whole_columns(left, right, width)
    }

    /// The columns the rows of a strip of a frame that scrolled are drawn in, `rows` of them from `top`,
    /// after the rows of the frame before it were copied in `copied`: where what draws beside the
    /// columns (an item that is not wholly inside them) is the same, one for one and in the same order,
    /// in every band the rows are in, nothing outside the columns and what is inside them reaches
    /// changed, and they are the columns of the copies and of the ink of what is inside them, in the
    /// two frames. None where it is not so, or where they are most of the window's width: the strip is
    /// the width of the picture.
    fn columns_beside(
        &self,
        old: &Frame,
        new: &Frame,
        copied: (u32, u32),
        (top, rows): (u32, u32),
        size: (u32, u32),
    ) -> Option<(u32, u32)> {
        let cell = self.cell.1;
        let last = top.checked_add(rows.checked_sub(1)?)?;
        #[allow(clippy::cast_precision_loss)]
        let (from, to, width) = (copied.0 as f32, copied.1 as f32, size.0 as f32);
        let (mut left, mut right) = (from, to);
        let mut seen: HashSet<(bool, usize)> = HashSet::new();
        for band in (top / cell) as usize..=(last / cell) as usize {
            if !unchanged_outside(old, new, band, (from, to), width) {
                return None;
            }
            for frame in [old, new] {
                for &n in frame.bands.get(band)? {
                    if !seen.insert((std::ptr::eq(frame, new), n as usize)) {
                        continue;
                    }
                    let Some(item) = frame.list.items.get(n as usize) else {
                        continue;
                    };
                    if beside(item, (from, to), width) {
                        continue;
                    }
                    if let Some((l, r)) = across(item, &self.fonts) {
                        (left, right) = (left.min(l), right.max(r));
                    }
                }
            }
        }
        whole_columns(left, right, size.0)
    }

    /// The rectangles of the rows `top`..`top + rows` whose pixels are not what `held` (`width` pixels
    /// across) has: the rows of `pixels` (a strip of those rows, `across` pixels each from the column
    /// `left`) are compared with it, and the runs of pixels that differ are stacked into rectangles
    /// (`stacks`); two of them are one where what the one has in excess of the two is worth less than
    /// a second command (`join`), and past `most` of them the ones that cost the least to join are.
    /// With them, where `searching` says how many, the tall rectangles in which rows that moved are
    /// looked for (`regions`). `pairs` is the queue `join` keeps its pairs in.
    #[allow(clippy::too_many_arguments)]
    fn differing(
        &self,
        held: &[u8],
        pixels: &[u8],
        width: u32,
        (left, across): (u32, u32),
        (top, rows): (u32, u32),
        most: usize,
        searching: Option<usize>,
        pairs: &mut Vec<Pair>,
    ) -> (Vec<Area>, Vec<Area>) {
        let runs = row_runs(
            held,
            pixels,
            width,
            (left, across),
            top,
            rows,
            self.cell.0 * JOIN_CELLS,
        );
        let searched = searching.map_or_else(Vec::new, |limit| regions(&runs, top, limit));
        let pieces = stacks(&runs, top, JOIN_ROWS);
        let areas = if pieces.len() > MOST_PIECES {
            regions(&runs, top, most)
        } else {
            join(pieces, most, pairs)
        };
        (areas, searched)
    }

    /// Tells the terminal to copy `rows` rows from `first` (in the columns `from` and how many)
    /// from `shift` rows further down (up where negative), and does the same to what is kept of the
    /// window (`shown.held`, `width` pixels across). The rows are copied in chunks that do not meet
    /// where they are from, in the order that leaves each chunk's source as it was.
    fn copy_rows(
        &mut self,
        out: &mut String,
        shown: &mut Shown,
        width: usize,
        (x, across): (u32, u32),
        (first, rows, shift): (u32, u32, i32),
        written: &mut Written,
    ) {
        let step = shift.unsigned_abs().max(1);
        let (mut done, mut chunks) = (0, Vec::new());
        while done < rows {
            let count = step.min(rows - done);
            chunks.push((first + done, count));
            done += count;
        }
        if shift < 0 {
            chunks.reverse();
        }
        for (at, count) in chunks {
            let from = at.checked_add_signed(shift).unwrap_or(at);
            let _ = write!(
                out,
                "\x1b_Ga=c,i={},r=1,c=1,X={x},Y={from},x={x},y={at},w={across},h={count},C=1,q=1\x1b\\",
                shown.id
            );
            for row in 0..count {
                let (a, b) = (
                    (from + row) as usize * width + x as usize,
                    (at + row) as usize * width + x as usize,
                );
                shown
                    .held
                    .copy_within(a * 4..(a + across as usize) * 4, b * 4);
            }
            written.copies.push(Moved {
                to: Area {
                    x,
                    y: at,
                    w: across,
                    h: count,
                },
                shift,
            });
            self.copied = Some(shown.id);
        }
    }

    /// Tells the terminal to copy the rows of the rectangles `areas` that are rows it holds a little
    /// further down or up, as far as the page's own items say they moved; the copies are made in
    /// what is kept of the window (`shown.held`) too. Whether any was. `pixels` is the strip of the
    /// rows from `top`, `across` pixels each from the column `left`.
    #[allow(clippy::too_many_arguments)]
    fn moved(
        &mut self,
        out: &mut String,
        shown: &mut Shown,
        pixels: &[u8],
        size: (u32, u32),
        strip: (u32, u32),
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
            let mut found = spans(&shown.held, pixels, width, strip, top, area, &shifts, least);
            if shifts.first().is_some_and(|shift| *shift < 0) {
                found.reverse();
            }
            for span in found {
                self.copy_rows(
                    out,
                    shown,
                    width,
                    (area.x, area.w),
                    (span.first, span.rows, span.shift),
                    written,
                );
                any = true;
            }
        }
        any
    }

    /// How a frame that scrolled is made of the window the terminal holds: from the two lists, before
    /// a row of the new one is drawn. The rows of the changed bands that are, to the pixel, rows the
    /// terminal holds a little further down or up (what the two lists say of their lines and boxes,
    /// where the page's own rules for drawing them are known: `pencere_drive::shot::slide`), in the
    /// columns the moved lines are cut to, and where nothing else of those bands changed. None where no
    /// line says the page moved, or the rows are few: the frame is then made as any other.
    ///
    /// The rows between two of them that are not (a bar, a caret's line) are copied too and drawn, and
    /// what is drawn is compared with what the terminal holds then, so that no more than what differs
    /// is sent. What is left to draw is a few strips (the two with the least between them are one
    /// where there are many). The bands of the foot of the window are drawn, whatever they are, for the page's edge
    /// is repeated down past them.
    fn slides(&self, shown: &Shown, new: &Frame, bands: &[bool], size: (u32, u32)) -> Option<Plan> {
        let cell = self.cell.1;
        let count = bands.len();
        // The changed bands but the last, which has the foot, by their rows.
        let mut ranges: Vec<(u32, u32)> = Vec::new();
        for (band, changed) in bands.iter().enumerate().take(count.saturating_sub(1)) {
            if !changed {
                continue;
            }
            let top = u32::try_from(band).ok()? * cell;
            match ranges.last_mut() {
                Some((first, rows)) if *first + *rows == top => *rows += cell,
                _ => ranges.push((top, cell)),
            }
        }
        let least = LEAST_MOVE_CELLS * cell;
        if ranges.iter().map(|range| range.1).sum::<u32>() < least * 2 {
            return None;
        }
        let slid = slide(
            Side {
                list: &shown.frame.list,
                reach: &shown.frame.reach,
            },
            Side {
                list: &new.list,
                reach: &new.reach,
            },
            &self.fonts,
            size,
            &ranges,
        )?;
        #[allow(clippy::cast_precision_loss)]
        let columns = (slid.columns.0 as f32, slid.columns.1 as f32);
        let height = size.1 as usize;
        // The rows that are the rows of a frame the terminal holds and have nothing else changed
        // (outside the columns they are copied in), and the rows of the changed bands.
        let mut known: Vec<Option<i32>> = vec![None; height];
        let mut changed = vec![false; height];
        for (band, yes) in bands.iter().enumerate() {
            if !yes {
                continue;
            }
            let rows = band * cell as usize..((band + 1) * cell as usize).min(height);
            for row in rows.clone() {
                if let Some(slot) = changed.get_mut(row) {
                    *slot = true;
                }
            }
            if band + 1 < count
                && unchanged_outside(&shown.frame, new, band, columns, size.0 as f32)
            {
                for row in rows {
                    if let (Some(slot), Some(shift)) = (known.get_mut(row), slid.rows.get(row)) {
                        *slot = *shift;
                    }
                }
            }
        }
        // Runs of rows of one shift; two of a shift with rows between them that are changed and
        // have a row to be copied from are one.
        let mut spans: Vec<(u32, u32, i32)> = Vec::new();
        let mut row = 0;
        while row < height {
            let Some(shift) = known.get(row).copied().flatten() else {
                row += 1;
                continue;
            };
            let first = row;
            while known.get(row).copied().flatten() == Some(shift) {
                row += 1;
            }
            let (first, end) = (u32::try_from(first).ok()?, u32::try_from(row).ok()?);
            let source = |at: u32| at.checked_add_signed(shift).is_some_and(|at| at < size.1);
            match spans.last_mut() {
                Some(last)
                    if last.2 == shift
                        && (last.1..first)
                            .all(|at| changed.get(at as usize) == Some(&true) && source(at)) =>
                {
                    last.1 = end;
                }
                _ => spans.push((first, end, shift)),
            }
        }
        spans.retain(|span| span.1 - span.0 >= least);
        if spans.is_empty() {
            return None;
        }
        // What is left to draw: the changed rows that are not rows to be copied, in strips that are
        // joined where a cell's rows or fewer lie between them.
        let mut left = changed;
        for &(first, end, _) in &spans {
            for at in first as usize..end as usize {
                if known.get(at).copied().flatten().is_some()
                    && let Some(slot) = left.get_mut(at)
                {
                    *slot = false;
                }
            }
        }
        let mut strips: Vec<(u32, u32)> = Vec::new();
        for (at, yes) in left.iter().enumerate() {
            if !yes {
                continue;
            }
            let at = u32::try_from(at).ok()?;
            match strips.last_mut() {
                Some((first, rows)) if at <= *first + *rows + cell => *rows = at + 1 - *first,
                _ => strips.push((at, 1)),
            }
        }
        // Many strips are few: the two with the least between them are one, the rows between them
        // drawn too (and what is the same there is not sent).
        while strips.len() > MOST_BANDS {
            let mut least: Option<(usize, u32)> = None;
            for (at, pair) in strips.windows(2).enumerate() {
                if let [above, below] = pair {
                    let gap = below.0.saturating_sub(above.0 + above.1);
                    if least.is_none_or(|(_, so_far)| gap < so_far) {
                        least = Some((at, gap));
                    }
                }
            }
            let Some((at, _)) = least else {
                break;
            };
            let next = strips.remove(at + 1);
            if let Some(strip) = strips.get_mut(at) {
                strip.1 = (next.0 + next.1).saturating_sub(strip.0);
            }
        }
        Some(Plan {
            spans,
            columns: slid.columns,
            strips,
        })
    }

    /// A frame that scrolled: the rows of `plan` are copied where they are in what the terminal
    /// holds, and what is left of the changed bands is drawn and what differs sent.
    #[allow(clippy::too_many_arguments)]
    fn scrolled(
        &mut self,
        out: &mut String,
        shown: &mut Shown,
        size: (u32, u32),
        frame: &Frame,
        images: Option<&Images>,
        plan: Plan,
        written: &mut Written,
    ) {
        let past = self.extent();
        let width = past.0 as usize;
        // Where the columns go to the window's right the picture's right is the page's edge repeated,
        // which the rows have as they are copied.
        let (x, right) = plan.columns;
        let across = if right >= size.0 {
            past.0 - x
        } else {
            right - x
        };
        let mut spans = plan.spans;
        // The rows come from further down (up) than they go: copied from the top (the foot) first, so
        // that no row that is copied from has been written over.
        if spans.first().is_some_and(|span| span.2 < 0) {
            spans.reverse();
        }
        for (first, end, shift) in spans {
            self.copy_rows(
                out,
                shown,
                width,
                (x, across),
                (first, end - first, shift),
                written,
            );
        }
        // What the copies left is drawn in the columns of the copies and of what is inside them, where
        // what is beside them is the same in the two frames; across the picture where it is not.
        let strips: Vec<Strip> = plan
            .strips
            .iter()
            .map(|&(top, rows)| Strip {
                top,
                rows,
                columns: self.columns_beside(&shown.frame, frame, plan.columns, (top, rows), size),
            })
            .collect();
        self.bands(out, shown, size, frame, images, &strips, false, written);
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
        let reaches: Vec<Option<(f32, f32)>> = list
            .items
            .iter()
            .map(|item| reach(item, &self.fonts))
            .collect();
        for (n, reached) in reaches.iter().enumerate() {
            let Some((top, bottom)) = *reached else {
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
        Frame {
            list,
            ready,
            bands,
            reach: reaches,
        }
    }
}

/// The pixel columns `left` to `right` (rounded outwards, cut to a picture `width` across) a strip is
/// drawn in, or `None` where there are none or they are most of the width: that is not worth the
/// other cut, and the strip is the width of the picture. The two ways of choosing columns, the
/// typed frame's and the scrolled frame's, share it so that they cut alike.
fn whole_columns(left: f32, right: f32, width: u32) -> Option<(u32, u32)> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (left, right) = (
        left.floor().max(0.0) as u32,
        (right.ceil().max(0.0) as u32).min(width),
    );
    (right > left && u64::from(right - left) * 8 <= u64::from(width) * 7).then_some((left, right))
}

/// Whether `item` may draw beside the columns `columns` of a window `width` across: what is past the
/// window's edges is not drawn, so the columns are cut to them too.
fn beside(item: &Paint, columns: (f32, f32), width: f32) -> bool {
    let (left, right) = spread(item);
    left.max(0.0) < columns.0 || right.min(width) > columns.1
}

/// Whether what reaches the band `band` and is not wholly inside the columns (what is drawn in the
/// columns beside them too) is the same in `new` as in `old`: where it is, the band's pixels outside
/// the columns are what they were.
fn unchanged_outside(
    old: &Frame,
    new: &Frame,
    band: usize,
    columns: (f32, f32),
    width: f32,
) -> bool {
    let beside_of = |frame: &Frame, band: &[u32]| -> Vec<u32> {
        band.iter()
            .copied()
            .filter(|&n| {
                frame
                    .list
                    .items
                    .get(n as usize)
                    .is_some_and(|item| beside(item, columns, width))
            })
            .collect()
    };
    let (Some(was), Some(now)) = (old.bands.get(band), new.bands.get(band)) else {
        return false;
    };
    let (was, now) = (beside_of(old, was), beside_of(new, now));
    was.len() == now.len()
        && was.iter().zip(&now).all(|(&a, &b)| {
            let (a, b) = (a as usize, b as usize);
            old.list.items.get(a) == new.list.items.get(b) && old.ready.get(a) == new.ready.get(b)
        })
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
fn transmit(wire: &mut Wire, out: &mut String, header: &str, rgba: &[u8], packed: bool, quiet: u8) {
    let Wire {
        comp,
        deflated,
        encoded,
        ..
    } = wire;
    encoded.clear();
    if packed && let Some(made) = deflate(comp, deflated, rgba) {
        output::base64_into(encoded, deflated.get(..made).unwrap_or_default());
        let header = format!("{header},o=z");
        output::pieces_quiet(out, &header, encoded, quiet);
        return;
    }
    output::base64_into(encoded, rgba);
    output::pieces_quiet(out, header, encoded, quiet);
}

/// Where in each of the `rows` rows from `top` the pixels of `pixels` (a strip of those rows, `across`
/// pixels each, the columns from `left` of the window's `width`) are not what `held` (the whole
/// window) has: runs of pixels, in the window's columns, a run and the next being one where fewer
/// than `join` pixels are between them.
fn row_runs(
    held: &[u8],
    pixels: &[u8],
    width: u32,
    (left, across): (u32, u32),
    top: u32,
    rows: u32,
    join: u32,
) -> Vec<Vec<(u32, u32)>> {
    let (row, part) = (width as usize * 4, across as usize * 4);
    let from = left as usize * 4;
    (0..rows as usize)
        .map(|r| {
            let was =
                held.get((top as usize + r) * row + from..(top as usize + r) * row + from + part);
            let now = pixels.get(r * part..(r + 1) * part);
            match (was, now) {
                (Some(was), Some(now)) => {
                    let mut runs = diff_runs(was, now, join);
                    if left > 0 {
                        for run in &mut runs {
                            *run = (run.0 + left, run.1 + left);
                        }
                    }
                    runs
                }
                _ => Vec::new(),
            }
        })
        .collect()
}

/// Whether the words of two blocks are not all the same (a block of a multiple of eight bytes).
#[inline(always)]
fn blocks_differ<const N: usize>(a: &[u8; N], b: &[u8; N]) -> bool {
    a.as_chunks::<8>()
        .0
        .iter()
        .zip(b.as_chunks::<8>().0)
        .fold(0, |folded, (p, q)| {
            folded | (u64::from_ne_bytes(*p) ^ u64::from_ne_bytes(*q))
        })
        != 0
}

/// Whether `a` and `b` are the same bytes, compared eight at a time with no call to libc: the
/// `memcmp` the standard library's `==` on bytes comes to is, in musl's aarch64, a loop of one byte
/// at a time (a Raspberry Pi's pages of pixels were compared at that speed: two fifths of a frame).
/// The words of a block are folded into one before it is looked at, so that the compiler has no
/// chain of comparisons to turn back into the call, and has the loop to make into vectors.
pub(crate) fn same(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let ((blocks_a, tail_a), (blocks_b, tail_b)) = (a.as_chunks::<64>(), b.as_chunks::<64>());
    if blocks_a
        .iter()
        .zip(blocks_b)
        .any(|(p, q)| blocks_differ(p, q))
    {
        return false;
    }
    tail_a
        .iter()
        .zip(tail_b)
        .fold(0, |folded, (p, q)| folded | (p ^ q))
        == 0
}

/// The runs of pixels, four bytes each, that `a` and `b` do not share. The pixels are compared sixteen
/// at a time, and the blocks that are the same (most of a row, mostly) are passed over whole.
pub(crate) fn diff_runs(a: &[u8], b: &[u8], join: u32) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    if same(a, b) {
        return runs;
    }
    let length = a.len().min(b.len());
    let (Some(a), Some(b)) = (a.get(..length), b.get(..length)) else {
        return runs;
    };
    let mut differ = |x: usize| {
        let x = u32::try_from(x).unwrap_or(u32::MAX);
        match runs.last_mut() {
            Some(last) if x < last.1.saturating_add(join) => last.1 = x + 1,
            _ => runs.push((x, x + 1)),
        }
    };
    let ((blocks_a, tail_a), (blocks_b, tail_b)) = (a.as_chunks::<64>(), b.as_chunks::<64>());
    for (n, (block_a, block_b)) in blocks_a.iter().zip(blocks_b).enumerate() {
        if !blocks_differ(block_a, block_b) {
            continue;
        }
        for (at, (p, q)) in block_a
            .as_chunks::<4>()
            .0
            .iter()
            .zip(block_b.as_chunks::<4>().0)
            .enumerate()
        {
            if u32::from_ne_bytes(*p) != u32::from_ne_bytes(*q) {
                differ(n * 16 + at);
            }
        }
    }
    let first = blocks_a.len() * 16;
    for (at, (p, q)) in tail_a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(tail_b.as_chunks::<4>().0)
        .enumerate()
    {
        if u32::from_ne_bytes(*p) != u32::from_ne_bytes(*q) {
            differ(first + at);
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

/// `areas` as no more than `most` rectangles: the two that cost the least to make one (the room
/// the one rectangle has more than the two had) are joined, until they are few enough. Where
/// there are very many they are the one rectangle that holds them all.
fn squeeze(mut areas: Vec<Area>, most: usize) -> Vec<Area> {
    if areas.len() > MOST_JOINED {
        return bounding(&areas).into_iter().collect();
    }
    while areas.len() > most.max(1) {
        let mut best: Option<(i64, usize, usize, Area)> = None;
        for (i, a) in areas.iter().enumerate() {
            for (j, b) in areas.iter().enumerate().skip(i + 1) {
                let waste = extra(a, b);
                if best.is_none_or(|(least, ..)| waste < least) {
                    best = Some((waste, i, j, hull(a, b)));
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

/// The tall rectangles in which a page that scrolled is looked for (not what is sent): the runs of
/// the rows that stand over one another are one rectangle, and where there are more than `most` of
/// them, or they are most of what they are in, they are joined, the two that cost the least first.
fn regions(runs: &[Vec<(u32, u32)>], top: u32, most: usize) -> Vec<Area> {
    let mut areas = clusters(runs, top, JOIN_ROWS);
    if areas.len() > most {
        areas = squeeze(areas, most);
    }
    // Rectangles over most of what they are drawn on are one.
    let sum: u64 = areas.iter().map(|a| u64::from(a.w) * u64::from(a.h)).sum();
    if areas.len() > 1
        && let Some(whole) = bounding(&areas)
        && sum * 2 > u64::from(whole.w) * u64::from(whole.h)
    {
        areas = vec![whole];
    }
    areas
}

/// The room a rectangle has, in pixels.
fn size(area: &Area) -> i64 {
    i64::from(area.w) * i64::from(area.h)
}

/// The smallest rectangle that has the two.
fn hull(a: &Area, b: &Area) -> Area {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    Area {
        x,
        y,
        w: a.right().max(b.right()) - x,
        h: a.bottom().max(b.bottom()) - y,
    }
}

/// The pixels the rectangle that has the two has more than the two have: what is sent more for one
/// command than for two (less than none where they cover one another).
fn extra(a: &Area, b: &Area) -> i64 {
    size(&hull(a, b)) - size(a) - size(b)
}

/// The rectangles the runs of rows (the first of them the row `top`) make, each of rows that stand
/// over one another: a run is of the rectangle in whose columns it stands and that it adds the fewest
/// pixels to, where that is under [`STACK_PIXELS`] and the rectangle's last row is no more than `join`
/// rows above it, and else it is a rectangle of its own. A rectangle grows by a few pixels at a time:
/// a run that is wide, at the top of a stack of narrow ones, is not one with them (it would take in
/// the columns the narrow ones are not in), but a row that is narrow under a bar of fewer pixels than
/// [`STACK_PIXELS`] is, and what such a stack has in excess of its rows is not bounded by it.
fn stacks(rows: &[Vec<(u32, u32)>], top: u32, join: u32) -> Vec<Area> {
    let mut open: Vec<Area> = Vec::new();
    let mut done: Vec<Area> = Vec::new();
    for (r, runs) in rows.iter().enumerate() {
        let row = top.saturating_add(u32::try_from(r).unwrap_or(u32::MAX));
        // The rectangles that more than `join` rows lie between and this one are done.
        let mut at = 0;
        while at < open.len() {
            if open
                .get(at)
                .is_some_and(|area| row >= area.bottom().saturating_add(join))
            {
                done.push(open.swap_remove(at));
            } else {
                at += 1;
            }
        }
        for &(from, to) in runs {
            let line = Area {
                x: from,
                y: row,
                w: to.saturating_sub(from),
                h: 1,
            };
            let nearest = open
                .iter()
                .enumerate()
                .filter(|(_, area)| area.x < line.right() && line.x < area.right())
                .map(|(at, area)| (extra(area, &line), at))
                .min();
            match nearest {
                Some((added, at)) if added < STACK_PIXELS => {
                    if let Some(area) = open.get_mut(at) {
                        *area = hull(area, &line);
                    }
                }
                _ => open.push(line),
            }
        }
    }
    done.extend(open);
    done.sort_by_key(|area| (area.y, area.x));
    done
}

/// `areas` as fewer rectangles: the two that cost the least to make one ([`extra`]) are joined while
/// that costs under [`JOIN_PIXELS`], and, where there are more than `most` still, until they are
/// few enough. The queue of their pairs is `pairs`, emptied and kept for the next time.
pub(crate) fn join(areas: Vec<Area>, most: usize, pairs: &mut Vec<Pair>) -> Vec<Area> {
    struct Slot {
        area: Area,
        // How many times it was joined to another: a pair in the queue is of what it was.
        stamp: u32,
        alive: bool,
    }
    let mut slots: Vec<Slot> = areas
        .into_iter()
        .map(|area| Slot {
            area,
            stamp: 0,
            alive: true,
        })
        .collect();
    // The pairs, the cheapest first; a pair of rectangles that were joined since is passed over.
    let mut kept = std::mem::take(pairs);
    kept.clear();
    let mut queue = BinaryHeap::from(kept);
    for (i, a) in slots.iter().enumerate() {
        for (j, b) in slots.iter().enumerate().skip(i + 1) {
            queue.push(Reverse((extra(&a.area, &b.area), i, j, 0_u32, 0_u32)));
        }
    }
    let mut count = slots.len();
    while count > 1 {
        let Some(Reverse((cost, i, j, stamp_i, stamp_j))) = queue.pop() else {
            break;
        };
        let current = |at: usize, stamp: u32| {
            slots
                .get(at)
                .is_some_and(|slot| slot.alive && slot.stamp == stamp)
        };
        if !current(i, stamp_i) || !current(j, stamp_j) {
            continue;
        }
        if cost >= JOIN_PIXELS && count <= most.max(1) {
            break;
        }
        let (Some(a), Some(b)) = (
            slots.get(i).map(|slot| slot.area),
            slots.get(j).map(|slot| slot.area),
        ) else {
            break;
        };
        let joined = hull(&a, &b);
        if let Some(slot) = slots.get_mut(j) {
            slot.alive = false;
        }
        let Some(slot) = slots.get_mut(i) else {
            break;
        };
        slot.area = joined;
        slot.stamp = slot.stamp.saturating_add(1);
        let stamp = slot.stamp;
        count -= 1;
        for (k, other) in slots.iter().enumerate() {
            if k == i || !other.alive {
                continue;
            }
            let cost = extra(&joined, &other.area);
            queue.push(Reverse(if i < k {
                (cost, i, k, stamp, other.stamp)
            } else {
                (cost, k, i, other.stamp, stamp)
            }));
        }
    }
    *pairs = queue.into_vec();
    let mut joined: Vec<Area> = slots
        .into_iter()
        .filter(|slot| slot.alive)
        .map(|slot| slot.area)
        .collect();
    joined.sort_by_key(|area| (area.y, area.x));
    joined
}

/// The smallest rectangle that has all of `areas`.
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

/// The pixels of `area` from a strip of rows (the first of them the row `top` of the window, each
/// `across` pixels, the first column of them `left`): the rows themselves where the rectangle is as
/// wide as the strip, and else a cut of them made in `out`.
fn cut<'a>(
    out: &'a mut Vec<u8>,
    strip: &'a [u8],
    (across, left): (usize, u32),
    top: u32,
    area: Area,
) -> &'a [u8] {
    let row = across * 4;
    let first = (area.y - top) as usize;
    let column = (area.x - left) as usize;
    if column == 0 && area.w as usize == across {
        return strip
            .get(first * row..(first + area.h as usize) * row)
            .unwrap_or_default();
    }
    out.clear();
    for line in first..first + area.h as usize {
        let from = line * row + column * 4;
        if let Some(pixels) = strip.get(from..from + area.w as usize * 4) {
            out.extend_from_slice(pixels);
        }
    }
    out
}

/// The pixels of `area` from a strip of rows (the first of them the row `top` of the window, each
/// `across` pixels, the first column of them `left`) put in what is kept of the window (`width`
/// pixels across): the rectangle that was sent.
fn keep_area(
    held: &mut [u8],
    width: usize,
    top: u32,
    strip: &[u8],
    (across, left): (usize, usize),
    area: Area,
) {
    let row = width * 4;
    for line in area.y..area.bottom() {
        let (from, to) = (
            (line - top) as usize * across * 4 + (area.x as usize - left) * 4,
            line as usize * row + area.x as usize * 4,
        );
        let length = area.w as usize * 4;
        if let (Some(pixels), Some(kept)) = (
            strip.get(from..from + length),
            held.get_mut(to..to + length),
        ) {
            kept.copy_from_slice(pixels);
        }
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

/// The spans of the rows of `area` (of a strip of rows from `top`, `across` pixels each from the
/// column `left`, of a window `width` across) that are, to the pixel, rows `held` has further down or
/// up by one of `shifts`. A span is as long as one of them goes on matching, and is kept if it is
/// `least` rows or more, and changes as many.
#[allow(clippy::too_many_arguments)]
fn spans(
    held: &[u8],
    strip: &[u8],
    width: usize,
    (left, across): (u32, u32),
    top: u32,
    area: &Area,
    shifts: &[i32],
    least: u32,
) -> Vec<Span> {
    let height = held.len() / (width * 4).max(1);
    let (x, w) = (area.x as usize * 4, area.w as usize * 4);
    let in_strip = (area.x.saturating_sub(left) as usize * 4, w);
    fn row_at(pixels: &[u8], row: usize, width: usize, (x, w): (usize, usize)) -> Option<&[u8]> {
        let from = row * width * 4 + x;
        pixels.get(from..from + w)
    }
    macro_rules! row_of {
        (held, $row:expr) => {
            row_at(held, $row, width, (x, w))
        };
        (strip, $row:expr) => {
            row_at(strip, $row, across as usize, in_strip)
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
        let changing = !row_of!(held, y as usize).is_some_and(|was| same(was, now));
        let matching = |shift: i32| -> bool {
            y.checked_add_signed(shift)
                .filter(|from| (*from as usize) < height)
                .and_then(|from| row_of!(held, from as usize))
                .is_some_and(|was| same(was, now))
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
