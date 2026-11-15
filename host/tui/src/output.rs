//! A frame as the bytes a terminal takes, and as text.

use std::fmt::Write;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use crate::color::Rgb;
use crate::grid::{Attrs, Bg, Cell};
use crate::paint::Frame;
use crate::pictures::Raster;

/// What names the pixels of a picture: two pictures with the same name are the same pixels, which a
/// terminal that was sent them once does not need to be sent again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Name {
    /// One pixel of a colour (a box the terminal draws over the cells' grounds).
    Solid([u8; 4]),
    /// A raster as it is shown: the size it is made to, and the tint and the opacity it is drawn with,
    /// change its pixels.
    Raster {
        source: Source,
        size: (u32, u32),
        tint: Option<([u8; 3], u32)>,
        opacity: u32,
    },
    /// A piece of a picture that is too large to be kept whole, of a grid laid over it that does not
    /// move: named by what the picture is, how large it is drawn, and where in it the piece is, so that
    /// it is the same pixels whichever drawing of the picture they were cut from.
    Tile {
        key: u64,
        whole: (u32, u32),
        at: (u32, u32),
        scale: (u32, u32),
        opacity: u32,
    },
}

/// Where the pixels of a raster are from, for what names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Source {
    /// The id the sandbox gave them ([`Raster::id`]).
    Image(u64),
    /// What they are: for pixels the renderer drew itself, which have no id.
    Content(u64),
}

enum Made {
    Solid([u8; 4]),
    Raster {
        raster: Arc<Raster>,
        tint: Option<(Rgb, f32)>,
        opacity: f32,
    },
    /// The pixels of a raster from `from` (its pixel at the piece's top-left corner), `step` of them
    /// to a pixel of the piece.
    Tile {
        raster: Arc<Raster>,
        from: (f32, f32),
        step: (f32, f32),
        opacity: f32,
    },
}

/// The pixels of a picture the terminal draws, whole: what a placement shows a part of. They are
/// made into what the terminal takes (straight, not premultiplied, with the tint and the opacity)
/// only when a terminal is to be sent them.
pub struct Pixels {
    name: Name,
    width: u32,
    height: u32,
    made: Made,
}

/// Which piece of which picture: `key` names the picture, `whole` is how large it is drawn, `at` is
/// where in it the piece starts and `size` how large the piece is (all in pixels), and `scale` how
/// large the pixels it is cut from are to the picture as it is drawn (in twentieths): pixels drawn at
/// another size are other pixels of the same piece.
#[derive(Debug, Clone, Copy)]
pub struct Piece {
    pub key: u64,
    pub whole: (u32, u32),
    pub at: (u32, u32),
    pub size: (u32, u32),
    pub scale: (u32, u32),
}

impl Pixels {
    /// One pixel of `rgba`.
    #[must_use]
    pub fn solid(rgba: [u8; 4]) -> Self {
        Self {
            name: Name::Solid(rgba),
            width: 1,
            height: 1,
            made: Made::Solid(rgba),
        }
    }

    /// `raster` drawn with `tint` (its colour and how much of it) and `opacity`.
    #[must_use]
    pub fn of(raster: Arc<Raster>, tint: Option<(Rgb, f32)>, opacity: f32) -> Self {
        let size = (raster.width, raster.height);
        Self::scaled(raster, size, tint, opacity)
    }

    /// As [`Pixels::of`], made `width` by `height` pixels: the raster is drawn again at that size when the
    /// pixels are made for a terminal (an average of what is under each, so that a picture that is
    /// held larger than it is drawn is no coarser for it), not before; two of them are the same pixels
    /// where they come of the same raster, at the same size.
    #[must_use]
    pub fn scaled(
        raster: Arc<Raster>,
        (width, height): (u32, u32),
        tint: Option<(Rgb, f32)>,
        opacity: f32,
    ) -> Self {
        let source = raster.id.map_or_else(
            || {
                let mut hash = DefaultHasher::new();
                (raster.width, raster.height).hash(&mut hash);
                raster.pixels.hash(&mut hash);
                Source::Content(hash.finish())
            },
            Source::Image,
        );
        Self {
            name: Name::Raster {
                source,
                size: (width.max(1), height.max(1)),
                tint: tint.map(|(color, share)| ([color.r, color.g, color.b], share.to_bits())),
                opacity: opacity.to_bits(),
            },
            width: width.max(1),
            height: height.max(1),
            made: Made::Raster {
                raster,
                tint,
                opacity,
            },
        }
    }

    /// Whether these are one pixel of one colour, which is what stretches over any cells.
    pub(crate) fn is_solid(&self) -> bool {
        matches!(self.made, Made::Solid(_))
    }

    /// The piece of a picture that `piece` says: the part of `raster` that is there, `from` its pixel
    /// at the corner and `step` pixels of it to one of the piece. Two of them are the same pixels where
    /// they are of the same picture at the same size and place, whatever raster they were cut from.
    #[must_use]
    pub fn tile(
        raster: Arc<Raster>,
        piece: Piece,
        from: (f32, f32),
        step: (f32, f32),
        opacity: f32,
    ) -> Self {
        Self {
            name: Name::Tile {
                key: piece.key,
                whole: piece.whole,
                at: piece.at,
                scale: piece.scale,
                opacity: opacity.to_bits(),
            },
            width: piece.size.0.max(1),
            height: piece.size.1.max(1),
            made: Made::Tile {
                raster,
                from,
                step,
                opacity,
            },
        }
    }

    pub(crate) fn name(&self) -> Name {
        self.name
    }

    /// How many bytes of pixels these are, as the terminal takes them.
    pub(crate) fn bytes(&self) -> usize {
        4 * self.width as usize * self.height as usize
    }

    /// The pixels of the part from `x0`, `y0` to `x1`, `y1` as the terminal takes them: `4 * width *
    /// height` bytes, sRGB with the opacity **not** multiplied in, rows from the top.
    #[must_use]
    pub fn rgba(&self, (x0, y0, x1, y1): (u32, u32, u32, u32)) -> Vec<u8> {
        let (raster, tint, opacity) = match &self.made {
            Made::Solid(rgba) => return rgba.to_vec(),
            Made::Raster {
                raster,
                tint,
                opacity,
            } => (raster, *tint, *opacity),
            Made::Tile {
                raster,
                from,
                step,
                opacity,
            } => {
                let mut rgba = Vec::with_capacity((x1 - x0) as usize * (y1 - y0) as usize * 4);
                let columns: Vec<(u32, Vec<f32>)> = (x0..x1)
                    .map(|x| span_weights(raster.width, from.0, step.0, x))
                    .collect();
                for y in y0..y1 {
                    let row = span_weights(raster.height, from.1, step.1, y);
                    for column in &columns {
                        rgba.extend_from_slice(&straight(
                            average(raster, column, &row),
                            None,
                            *opacity,
                        ));
                    }
                }
                return rgba;
            }
        };
        let mut rgba = Vec::with_capacity((x1 - x0) as usize * (y1 - y0) as usize * 4);
        // Where it is not the size of the raster, what is under each of its pixels.
        let native = (raster.width, raster.height) == (self.width, self.height);
        let columns: Vec<(u32, Vec<f32>)> = if native {
            Vec::new()
        } else {
            (x0..x1)
                .map(|x| weights(raster.width, self.width, x))
                .collect()
        };
        let rows: Vec<(u32, Vec<f32>)> = if native {
            Vec::new()
        } else {
            (y0..y1)
                .map(|y| weights(raster.height, self.height, y))
                .collect()
        };
        for y in y0..y1 {
            for x in x0..x1 {
                let pixel = if native {
                    raster.at(x, y)
                } else {
                    let (column, row) =
                        (columns.get((x - x0) as usize), rows.get((y - y0) as usize));
                    match (column, row) {
                        (Some(column), Some(row)) => average(raster, column, row),
                        _ => [0; 4],
                    }
                };
                rgba.extend_from_slice(&straight(pixel, tint, opacity));
            }
        }
        rgba
    }
}

/// A pixel of a raster (premultiplied) as the terminal takes it: with the tint and the opacity, and
/// the opacity **not** multiplied in.
fn straight([r, g, b, a]: [u8; 4], tint: Option<(Rgb, f32)>, opacity: f32) -> [u8; 4] {
    let alpha = f32::from(a) / 255.0 * opacity;
    match tint {
        Some((color, strength)) => [
            color.r,
            color.g,
            color.b,
            (alpha * strength * 255.0).round().clamp(0.0, 255.0) as u8,
        ],
        None if a == 0 => [0; 4],
        None => {
            let straight = |c: u8| {
                (f32::from(c) * 255.0 / f32::from(a))
                    .round()
                    .clamp(0.0, 255.0) as u8
            };
            [
                straight(r),
                straight(g),
                straight(b),
                (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
            ]
        }
    }
}

/// The pixels of a raster `from` across (or down) that a pixel `at` of one `to` across takes, and how
/// much of each: the first pixel and the shares, which add up to one. Where the raster is shrunk each
/// pixel takes the share of the ones it covers; where it is grown it is a blend of the two about its
/// middle.
fn weights(from: u32, to: u32, at: u32) -> (u32, Vec<f32>) {
    span_weights(from, 0.0, from.max(1) as f32 / to.max(1) as f32, at)
}

/// As [`weights`], for the pixel `at` of a part of a picture that starts at the pixel `start` of a
/// raster `length` across (or down) and takes `step` of its pixels to one of its own.
fn span_weights(length: u32, start: f32, step: f32, at: u32) -> (u32, Vec<f32>) {
    let last_pixel = length.saturating_sub(1);
    if step >= 1.0 {
        let low = (start + at as f32 * step).max(0.0);
        let high = start + (at + 1) as f32 * step;
        let first = low.floor() as u32;
        let last = (high.ceil() as u32).clamp(first + 1, length.max(1));
        let shares: Vec<f32> = (first..last)
            .map(|i| ((i + 1) as f32).min(high) - (i as f32).max(low))
            .collect();
        let total: f32 = shares.iter().sum();
        let shares = shares
            .into_iter()
            .map(|share| share / total.max(f32::EPSILON))
            .collect();
        (first.min(last_pixel), shares)
    } else {
        let middle = start + (at as f32 + 0.5) * step - 0.5;
        let first = (middle.floor().max(0.0) as u32).min(last_pixel);
        let after = (first + 1).min(last_pixel);
        let t = (middle - first as f32).clamp(0.0, 1.0);
        if after == first {
            (first, vec![1.0])
        } else {
            (first, vec![1.0 - t, t])
        }
    }
}

/// The pixel of `raster` that the `across` and `down` shares (from [`weights`]) average, premultiplied.
fn average(raster: &Raster, (x, across): &(u32, Vec<f32>), (y, down): &(u32, Vec<f32>)) -> [u8; 4] {
    let mut sum = [0.0_f32; 4];
    for (j, share_y) in down.iter().enumerate() {
        for (i, share_x) in across.iter().enumerate() {
            let pixel = raster.at(x + i as u32, y + j as u32);
            let share = share_x * share_y;
            for (total, channel) in sum.iter_mut().zip(pixel) {
                *total += f32::from(channel) * share;
            }
        }
    }
    sum.map(|channel| channel.round().clamp(0.0, 255.0) as u8)
}

impl Clone for Pixels {
    fn clone(&self) -> Self {
        Self {
            name: self.name,
            width: self.width,
            height: self.height,
            made: match &self.made {
                Made::Solid(rgba) => Made::Solid(*rgba),
                Made::Raster {
                    raster,
                    tint,
                    opacity,
                } => Made::Raster {
                    raster: Arc::clone(raster),
                    tint: *tint,
                    opacity: *opacity,
                },
                Made::Tile {
                    raster,
                    from,
                    step,
                    opacity,
                } => Made::Tile {
                    raster: Arc::clone(raster),
                    from: *from,
                    step: *step,
                    opacity: *opacity,
                },
            },
        }
    }
}

impl std::fmt::Debug for Pixels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pixels")
            .field("name", &self.name)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl PartialEq for Pixels {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.width == other.width && self.height == other.height
    }
}

impl Eq for Pixels {}

/// A picture the terminal draws: where it goes, and the part of some pixels it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    /// The cell its top-left corner is in, from 0.
    pub col: u16,
    pub row: u16,
    /// The cells it covers: where it has no `offset` the part of the pixels is drawn to fill them,
    /// and where it has, they are the cells that the part, drawn as large as it is, touches.
    pub cols: u16,
    pub rows: u16,
    /// Where in that cell its corner stands, pixels, for a part drawn as large as it is (the pixels
    /// of the terminal and not of the cells it passes over); `None` for one that is stretched over
    /// `cols` by `rows` cells.
    pub offset: Option<(u32, u32)>,
    /// The pixels it is a part of, which stay the same as it moves.
    pub pixels: Pixels,
    /// Where in them the part it shows starts, and how large it is (pixels).
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// The terminal's layer: below `-1_073_741_824` it is under the grounds of the cells that have
    /// one (and shows only where a cell's ground is the terminal's own), from there to `-1` over the
    /// grounds and under the text.
    pub z: i32,
}

impl Placement {
    /// The pixels of the part it shows, as the terminal takes them: `4 * width * height` bytes,
    /// sRGB with the opacity **not** multiplied in, rows from the top.
    #[must_use]
    pub fn rgba(&self) -> Vec<u8> {
        self.pixels
            .rgba((self.x, self.y, self.x + self.width, self.y + self.height))
    }
}

/// The layer of a picture that covers its cells altogether.
pub(crate) const GROUND: i32 = -1_073_741_825;
/// The layer of a picture that does not: it is over the grounds, under the text.
pub(crate) const BEHIND_TEXT: i32 = -1;
/// Over the text: for a picture that is a line the characters of a cell cannot draw without the
/// words of the cell breaking it.
pub(crate) const OVER_TEXT: i32 = 1_000_000;

/// The most base64 text one escape sequence of a picture carries.
const CHUNK: usize = 4096;

const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn sextet(n: u32, shift: u32) -> char {
    let index = ((n >> shift) & 63) as usize;
    char::from(TABLE.get(index).copied().unwrap_or(b'A'))
}

/// `bytes` as base64, padded.
pub(crate) fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut n = 0_u32;
        for (i, byte) in chunk.iter().enumerate() {
            n |= u32::from(*byte) << (16 - 8 * i);
        }
        out.push(sextet(n, 18));
        out.push(sextet(n, 12));
        out.push(if chunk.len() > 1 { sextet(n, 6) } else { '=' });
        out.push(if chunk.len() > 2 { sextet(n, 0) } else { '=' });
    }
    out
}

/// The sequences of an image's `data` (base64), in pieces the terminal takes: the first begins with
/// `first` and the rest with what a continuation has. `q=2` keeps the terminal from answering into
/// the input of whatever shows the file.
fn pieces(out: &mut String, first: &str, data: &str) {
    pieces_quiet(out, first, data, 2);
}

/// As [`pieces`], with the terminal's answers kept down as `quiet` says: `2` keeps all of them, `1`
/// keeps the ones that say it took the picture and lets a refusal through.
pub(crate) fn pieces_quiet(out: &mut String, first: &str, data: &str, quiet: u8) {
    let chunks: Vec<&[u8]> = data.as_bytes().chunks(CHUNK).collect();
    let last = chunks.len().saturating_sub(1);
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i < last);
        let text = std::str::from_utf8(chunk).unwrap_or("");
        if i == 0 {
            let _ = write!(out, "\x1b_G{first},q={quiet},m={more};{text}\x1b\\");
        } else {
            let _ = write!(out, "\x1b_Gm={more},q={quiet};{text}\x1b\\");
        }
    }
}

/// The sequence that draws `placement` (the terminal's graphics protocol, Kitty's), at the cell the
/// cursor is at, with its pixels in it: for a frame that is written once.
pub(crate) fn kitty(out: &mut String, placement: &Placement) {
    // `C=1` leaves the cursor where it is.
    let first = format!(
        "a=T,f=32,s={},v={}{},z={},C=1",
        placement.width,
        placement.height,
        geometry(placement),
        placement.z,
    );
    pieces(out, &first, &base64(&placement.rgba()));
}

/// The keys that say where in its cells a placement is drawn, and how large: the cells it is stretched
/// over, or the pixels it is in its first cell.
fn geometry(placement: &Placement) -> String {
    match placement.offset {
        None => format!(",c={},r={}", placement.cols, placement.rows),
        Some((x, y)) => {
            let mut keys = String::new();
            if x > 0 {
                let _ = write!(keys, ",X={x}");
            }
            if y > 0 {
                let _ = write!(keys, ",Y={y}");
            }
            keys
        }
    }
}

/// How hard the pixels of a picture are packed (`miniz_oxide`'s levels run 0 to 10): the fastest that
/// still shrinks them, since a page is packed while the person waits for it.
pub(crate) const PACK_LEVEL: u8 = 1;

/// The sequences that give a terminal the whole of `pixels` to keep under `id`, and draw nothing; with
/// the pixels compressed (zlib, which the terminal says it takes) where `compress` says to be, and
/// they are smaller so.
pub(crate) fn store(out: &mut String, id: u32, pixels: &Pixels, compress: bool) {
    let mut first = format!("a=t,i={id},f=32,s={},v={}", pixels.width, pixels.height);
    let whole = (0, 0, pixels.width, pixels.height);
    let rgba = pixels.rgba(whole);
    if compress {
        let packed = miniz_oxide::deflate::compress_to_vec_zlib(&rgba, PACK_LEVEL);
        if packed.len() < rgba.len() {
            first.push_str(",o=z");
            pieces(out, &first, &base64(&packed));
            return;
        }
    }
    pieces(out, &first, &base64(&rgba));
}

/// The sequence that draws the part `placement` shows of the pixels kept under `id`, as the
/// placement `put` of them, at the cell the cursor is at, in the layer `z`. A placement that has the
/// number of one that is there is put in its place.
pub(crate) fn put(out: &mut String, id: u32, put: u32, placement: &Placement, z: i32) {
    let _ = write!(
        out,
        "\x1b_Ga=p,i={id},p={put}{},z={z},C=1,q=2",
        geometry(placement),
    );
    let pixels = &placement.pixels;
    if (placement.x, placement.y, placement.width, placement.height)
        != (0, 0, pixels.width, pixels.height)
    {
        let _ = write!(
            out,
            ",x={},y={},w={},h={}",
            placement.x, placement.y, placement.width, placement.height
        );
    }
    out.push_str("\x1b\\");
}

/// The sequence that takes the placement `put` of the pixels under `id` off the screen, and keeps
/// the pixels.
pub(crate) fn lift(out: &mut String, id: u32, put: u32) {
    let _ = write!(out, "\x1b_Ga=d,d=i,i={id},p={put},q=2\x1b\\");
}

/// The sequence that gives the pixels under `id` up, and the placements of them with them.
pub(crate) fn free(out: &mut String, id: u32) {
    let _ = write!(out, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\");
}

/// The sequence that gives every picture up.
pub(crate) const FREE_ALL: &str = "\x1b_Ga=d,d=A,q=2\x1b\\";

/// How many colours the terminal takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    /// Sixteen million: `38;2;r;g;b`.
    True,
    /// The 256 of the terminals that have no more: the colour cube and the greys, nearest to the
    /// colour the page asks for.
    Indexed,
}

/// What the terminal has been told about how the next characters are drawn.
pub(crate) struct Sgr {
    depth: Depth,
    fg: Option<Rgb>,
    bg: Option<Bg>,
    attrs: Attrs,
}

impl Sgr {
    pub(crate) fn new(depth: Depth) -> Self {
        Self {
            depth,
            fg: None,
            bg: None,
            attrs: Attrs::NONE,
        }
    }

    pub(crate) fn set(&mut self, out: &mut String, cell: &Cell) {
        let flags = [
            (Attrs::BOLD, 1, 22),
            (Attrs::ITALIC, 3, 23),
            (Attrs::UNDERLINE, 4, 24),
            (Attrs::STRIKE, 9, 29),
        ];
        for (attr, on, off) in flags {
            let (had, has) = (self.attrs.contains(attr), cell.attrs.contains(attr));
            if has != had {
                let _ = write!(out, "\x1b[{}m", if has { on } else { off });
            }
        }
        self.attrs = cell.attrs;
        if self.fg != Some(cell.fg) {
            self.color(out, 38, cell.fg);
            self.fg = Some(cell.fg);
        }
        if self.bg != Some(cell.bg) {
            match cell.bg {
                Bg::Rgb(rgb) => self.color(out, 48, rgb),
                Bg::Default => out.push_str("\x1b[49m"),
            }
            self.bg = Some(cell.bg);
        }
    }

    /// `ground` is 38 for the text's colour and 48 for the cell's.
    fn color(&self, out: &mut String, ground: u8, rgb: Rgb) {
        let Rgb { r, g, b } = rgb;
        match self.depth {
            Depth::True => {
                let _ = write!(out, "\x1b[{ground};2;{r};{g};{b}m");
            }
            Depth::Indexed => {
                let _ = write!(out, "\x1b[{ground};5;{}m", rgb.to_indexed());
            }
        }
    }
}

/// The frame as what a terminal is sent to show it: the screen cleared, the rows from its top-left
/// corner in true colour, then the pictures at their cells, and the cursor left under the grid. Needs
/// a terminal of the grid's size and a row more.
pub(crate) fn ansi(frame: &Frame) -> Vec<u8> {
    let mut out = String::from("\x1b[H\x1b[2J");
    let last = usize::from(frame.grid.rows()).saturating_sub(1);
    for (index, row) in frame.grid.rows_of_cells().enumerate() {
        let mut state = Sgr::new(Depth::True);
        for cell in row.iter().filter(|cell| !cell.tail) {
            state.set(&mut out, cell);
            out.push(cell.ch);
        }
        out.push_str("\x1b[0m");
        if index < last {
            out.push_str("\r\n");
        }
    }
    for placement in &frame.placements {
        let _ = write!(out, "\x1b[{};{}H", placement.row + 1, placement.col + 1);
        kitty(&mut out, placement);
    }
    let _ = write!(out, "\x1b[{};1H", frame.grid.rows() + 1);
    out.into_bytes()
}
