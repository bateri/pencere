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
    /// A raster as it is shown: the tint and the opacity it is drawn with change its pixels.
    Raster {
        source: Source,
        tint: Option<([u8; 3], u32)>,
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
                tint: tint.map(|(color, share)| ([color.r, color.g, color.b], share.to_bits())),
                opacity: opacity.to_bits(),
            },
            width: raster.width,
            height: raster.height,
            made: Made::Raster {
                raster,
                tint,
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
        };
        let mut rgba = Vec::with_capacity((x1 - x0) as usize * (y1 - y0) as usize * 4);
        for y in y0..y1 {
            for x in x0..x1 {
                let [r, g, b, a] = raster.at(x, y);
                let alpha = f32::from(a) / 255.0 * opacity;
                rgba.extend_from_slice(&match tint {
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
                });
            }
        }
        rgba
    }
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
    /// The cell its top-left corner is at, from 0.
    pub col: u16,
    pub row: u16,
    /// The cells it covers: the part of the pixels is drawn to fill them.
    pub cols: u16,
    pub rows: u16,
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
    let chunks: Vec<&[u8]> = data.as_bytes().chunks(CHUNK).collect();
    let last = chunks.len().saturating_sub(1);
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i < last);
        let text = std::str::from_utf8(chunk).unwrap_or("");
        if i == 0 {
            let _ = write!(out, "\x1b_G{first},q=2,m={more};{text}\x1b\\");
        } else {
            let _ = write!(out, "\x1b_Gm={more},q=2;{text}\x1b\\");
        }
    }
}

/// The sequence that draws `placement` (the terminal's graphics protocol, Kitty's), at the cell the
/// cursor is at, with its pixels in it: for a frame that is written once.
pub(crate) fn kitty(out: &mut String, placement: &Placement) {
    // `C=1` leaves the cursor where it is.
    let first = format!(
        "a=T,f=32,s={},v={},c={},r={},z={},C=1",
        placement.width, placement.height, placement.cols, placement.rows, placement.z,
    );
    pieces(out, &first, &base64(&placement.rgba()));
}

/// The sequences that give a terminal the whole of `pixels` to keep under `id`, and draw nothing.
pub(crate) fn store(out: &mut String, id: u32, pixels: &Pixels) {
    let first = format!("a=t,i={id},f=32,s={},v={}", pixels.width, pixels.height);
    let whole = (0, 0, pixels.width, pixels.height);
    pieces(out, &first, &base64(&pixels.rgba(whole)));
}

/// The sequence that draws the part `placement` shows of the pixels kept under `id`, as the
/// placement `put` of them, at the cell the cursor is at, in the layer `z`. A placement that has the
/// number of one that is there is put in its place.
pub(crate) fn put(out: &mut String, id: u32, put: u32, placement: &Placement, z: i32) {
    let _ = write!(
        out,
        "\x1b_Ga=p,i={id},p={put},c={},r={},z={z},C=1,q=2",
        placement.cols, placement.rows,
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
