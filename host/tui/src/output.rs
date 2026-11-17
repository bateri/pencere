//! What is written to the terminal to draw a picture: the pieces of the Kitty graphics protocol's
//! sequences, and base64 for their data.

use std::fmt::Write;

/// The most base64 text one escape sequence of a picture carries.
const CHUNK: usize = 4096;

const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn sextet(n: u32, shift: u32) -> char {
    let index = ((n >> shift) & 63) as usize;
    char::from(TABLE.get(index).copied().unwrap_or(b'A'))
}

/// `bytes` as base64, padded.
pub(crate) fn base64(bytes: &[u8]) -> String {
    let mut out = String::new();
    base64_into(&mut out, bytes);
    out
}

/// `bytes` as base64, padded, put at the end of `out`: where `out` is one kept from a frame to the
/// next the text is made in what the frame before it was.
pub(crate) fn base64_into(out: &mut String, bytes: &[u8]) {
    out.reserve(bytes.len().div_ceil(3) * 4);
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
}

/// The sequences of an image's `data` (base64), in pieces the terminal takes: the first begins with
/// `first` and the rest with what a continuation has. The terminal's answers are kept down as `quiet`
/// says: `2` keeps all of them (they would go into the input of the program), `1` keeps the ones that
/// say it took the picture and lets a refusal through.
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

/// How hard the pixels of a picture are packed (`miniz_oxide`'s levels run 0 to 10): the fastest that
/// still shrinks them, since a page is packed while the person waits for it.
pub(crate) const PACK_LEVEL: u8 = 1;

/// The sequence that gives the pixels under `id` up, and the placements of them with them.
pub(crate) fn free(out: &mut String, id: u32) {
    let _ = write!(out, "\x1b_Ga=d,d=I,i={id},q=2\x1b\\");
}

/// The sequence that gives every picture up.
pub(crate) const FREE_ALL: &str = "\x1b_Ga=d,d=A,q=2\x1b\\";
