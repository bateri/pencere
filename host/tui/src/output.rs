//! A frame as the bytes a terminal takes, and as text.

use std::fmt::Write;

use crate::color::Rgb;
use crate::grid::{Attrs, Bg, Cell};
use crate::paint::Frame;

/// A picture the terminal draws: where it goes, and its pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    /// The cell its top-left corner is at, from 0.
    pub col: u16,
    pub row: u16,
    /// The cells it covers: the pixels are drawn to fill them.
    pub cols: u16,
    pub rows: u16,
    pub width: u32,
    pub height: u32,
    /// `4 * width * height` bytes, sRGB with the opacity **not** multiplied in, rows from the top.
    pub rgba: Vec<u8>,
    /// The terminal's layer: below `-1_073_741_824` it is under the grounds of the cells that have
    /// one (and shows only where a cell's ground is the terminal's own), from there to `-1` over the
    /// grounds and under the text.
    pub z: i32,
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

/// The sequence that draws `placement` (the terminal's graphics protocol, Kitty's), at the cell the
/// cursor is at.
fn kitty(out: &mut String, placement: &Placement) {
    let data = base64(&placement.rgba);
    let chunks: Vec<&[u8]> = data.as_bytes().chunks(CHUNK).collect();
    let last = chunks.len().saturating_sub(1);
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i < last);
        let text = std::str::from_utf8(chunk).unwrap_or("");
        if i == 0 {
            // `C=1` leaves the cursor where it is and `q=2` keeps the terminal from answering into
            // the input of whatever shows the file.
            let _ = write!(
                out,
                "\x1b_Ga=T,f=32,s={},v={},c={},r={},z={},C=1,q=2,m={more};{text}\x1b\\",
                placement.width, placement.height, placement.cols, placement.rows, placement.z,
            );
        } else {
            let _ = write!(out, "\x1b_Gm={more},q=2;{text}\x1b\\");
        }
    }
}

/// What the terminal has been told about how the next characters are drawn.
struct Sgr {
    fg: Option<Rgb>,
    bg: Option<Bg>,
    attrs: Attrs,
}

impl Sgr {
    fn set(&mut self, out: &mut String, cell: &Cell) {
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
            let Rgb { r, g, b } = cell.fg;
            let _ = write!(out, "\x1b[38;2;{r};{g};{b}m");
            self.fg = Some(cell.fg);
        }
        if self.bg != Some(cell.bg) {
            match cell.bg {
                Bg::Rgb(Rgb { r, g, b }) => {
                    let _ = write!(out, "\x1b[48;2;{r};{g};{b}m");
                }
                Bg::Default => out.push_str("\x1b[49m"),
            }
            self.bg = Some(cell.bg);
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
        let mut state = Sgr {
            fg: None,
            bg: None,
            attrs: Attrs::NONE,
        };
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
