//! Reading the formats the images of a page come in: PNG, JPEG and SVG.
//!
//! The bytes are not trusted: they are a file a program sent, and what is in them is whatever its
//! author wrote. So the format is told from the bytes and not from a name, the size of an image is
//! read from its header and approved **before** a pixel is made ([`Approve`]) — with the memory
//! the decoder works in while it makes them, which is more than the pixels it gives — and a
//! decoder that fails, or gives up by panicking (which the caller catches), leaves the image
//! refused and nothing else harmed.
//!
//! Pixels come out as sRGB, eight bits a channel, **premultiplied** by the opacity: the form a
//! renderer blends in and filters without a dark edge around what is see-through.
//!
//! An SVG draws shapes and reads nothing of the machine it is drawn on: it is built without text
//! (no font is loaded, and the words in it are not drawn), without raster images and without
//! compressed files, and the one door an SVG has to other files — an `<image>`, whether it
//! names a path, a URL or carries its data — is shut: every such image is skipped.

use std::io::Cursor;

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{ImageHrefResolver, Options, Tree};
use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

/// The formats an image can be in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    Png,
    Jpeg,
    Svg,
}

/// Why a file is not an image the page can have.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// The bundle has no such file.
    Missing,
    /// The file is not in a format the kit reads.
    Unknown,
    /// The file is larger than a file may be.
    Source { bytes: usize, most: usize },
    /// The decoder could not read it, in its words.
    Broken(String),
    /// The image is wider or taller than an image may be.
    TooLarge { width: u32, height: u32, side: u32 },
    /// The image would take the pixels of the view past what the view may hold, with what it
    /// holds now: there may be room later.
    Budget { most: usize },
    /// The image is by itself more than the view may hold: there never is room.
    Whole { bytes: usize, most: usize },
    /// Decoding or drawing the image would take more memory to work in than one image may.
    Work { bytes: usize, most: usize },
    /// The SVG is made of more than a drawing may be.
    Complex { nodes: usize, most: usize },
    /// The view names more images than it may.
    Paths { most: usize },
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => f.write_str("is not a file of the view's bundle"),
            Self::Unknown => f.write_str("is not a PNG, JPEG or SVG image"),
            Self::Source { bytes, most } => {
                write!(f, "is {bytes} bytes, past the {most} bytes a file may be")
            }
            Self::Broken(why) => write!(f, "could not be decoded: {why}"),
            Self::TooLarge {
                width,
                height,
                side,
            } => write!(
                f,
                "is {width}\u{d7}{height} pixels, past the limit of {side}\u{d7}{side}"
            ),
            Self::Budget { most } => write!(
                f,
                "would take the decoded images of the view past {} MiB",
                most >> 20
            ),
            Self::Whole { bytes, most } => write!(
                f,
                "is {} MiB of pixels, past the {} MiB a view may hold",
                bytes >> 20,
                most >> 20
            ),
            Self::Work { bytes, most } => write!(
                f,
                "would take {} MiB to work in, past the {} MiB one image may",
                bytes >> 20,
                most >> 20
            ),
            Self::Complex { nodes, most } => write!(
                f,
                "is a drawing of {nodes} parts, past the {most} a drawing may have"
            ),
            Self::Paths { most } => write!(f, "is past the {most} images a view may name"),
        }
    }
}

/// What approves an image of a size, once its header says it, before any pixel is made: its width
/// and height, and the bytes the decoder will work in besides the pixels it gives.
pub(crate) type Approve<'a> = &'a mut dyn FnMut(u32, u32, usize) -> Result<(), Failure>;

/// An image's pixels, premultiplied RGBA, eight bits a channel.
pub(crate) struct Raster {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}

/// The format of the bytes, from the bytes.
pub(crate) fn sniff(bytes: &[u8]) -> Option<Format> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(Format::Png);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(Format::Jpeg);
    }
    // A document in the XML a text file is made of, that has a root of this name in its first
    // lines (after a declaration, a doctype, comments): the parser decides the rest.
    let text = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let first = text.iter().find(|byte| !byte.is_ascii_whitespace())?;
    let head = text.get(..text.len().min(4096)).unwrap_or_default();
    (*first == b'<' && head.windows(4).any(|window| window == b"<svg")).then_some(Format::Svg)
}

/// A raster image's pixels (PNG or JPEG). `approve` is asked about the size the header says.
pub(crate) fn raster(
    bytes: &[u8],
    format: Format,
    approve: Approve<'_>,
) -> Result<Raster, Failure> {
    match format {
        Format::Png => png(bytes, approve),
        Format::Jpeg => jpeg(bytes, approve),
        Format::Svg => Err(Failure::Unknown),
    }
}

/// What the header of a PNG or a JPEG says, before a pixel is made: the size the image is shown at
/// (a photograph that is turned a quarter has its width and height swapped, as [`orient`] does), and
/// the bytes the decoder will work in besides the pixels (what [`Approve`] is told).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Header {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) working: usize,
}

/// The header of a PNG or a JPEG; nothing of its pixels is read.
pub(crate) fn header(bytes: &[u8], format: Format) -> Result<Header, Failure> {
    match format {
        Format::Png => {
            let opened = png_open(bytes)?;
            Ok(Header {
                width: opened.width,
                height: opened.height,
                working: opened.working,
            })
        }
        Format::Jpeg => {
            let opened = jpeg_open(bytes)?;
            let (width, height) = if opened.turn.quarter() {
                (opened.height, opened.width)
            } else {
                (opened.width, opened.height)
            };
            Ok(Header {
                width,
                height,
                working: opened.working,
            })
        }
        Format::Svg => Err(Failure::Unknown),
    }
}

/// How much of each source pixel goes into each pixel of an axis shrunk from `from` to `to`: for
/// every target pixel the first source pixel it covers and the share of each (the source pixels
/// the target pixel's span reaches, by the part of them it covers, the shares adding up to one).
fn spans(from: usize, to: usize) -> Vec<(usize, Vec<f32>)> {
    let scale = from as f64 / to as f64;
    (0..to)
        .map(|at| {
            let (start, end) = (
                at as f64 * scale,
                ((at + 1) as f64 * scale).min(from as f64),
            );
            let first = start.floor() as usize;
            let last = (end.ceil() as usize).clamp(first + 1, from);
            let shares = (first..last)
                .map(|source| {
                    let covered = end.min((source + 1) as f64) - start.max(source as f64);
                    (covered / (end - start)).max(0.0) as f32
                })
                .collect();
            (first, shares)
        })
        .collect()
}

/// `raster` shrunk to `width` by `height` pixels, each of which is the average of the part of the
/// image it covers (a box filter: the pixels are premultiplied, so averaging them is right where
/// the opacity differs). Not made larger: a side past the raster's is the raster's.
pub(crate) fn shrink(raster: Raster, width: u32, height: u32) -> Raster {
    let (width, height) = (
        width.min(raster.width).max(1),
        height.min(raster.height).max(1),
    );
    if (width, height) == (raster.width, raster.height) {
        return raster;
    }
    let (from_w, from_h) = (raster.width as usize, raster.height as usize);
    let (to_w, to_h) = (width as usize, height as usize);
    let across = spans(from_w, to_w);
    let down = spans(from_h, to_h);
    let mut pixels = Vec::with_capacity(to_w * to_h * 4);
    // A row of the source across the target's width (the last one made, since the rows at the
    // edge of one target row are the first of the next), and the target row it is summed into.
    let mut row = vec![0.0_f32; to_w * 4];
    let mut made = usize::MAX;
    let mut sum = vec![0.0_f32; to_w * 4];
    for (first, shares) in &down {
        sum.fill(0.0);
        for (offset, share) in shares.iter().enumerate() {
            let y = first + offset;
            if made != y {
                let at = y * from_w * 4;
                let Some(source) = raster.pixels.get(at..at + from_w * 4) else {
                    continue;
                };
                for (target, (start, weights)) in row.chunks_exact_mut(4).zip(&across) {
                    let (mut r, mut g, mut b, mut a) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
                    let from = source.get(start * 4..).unwrap_or_default();
                    for (pixel, weight) in from.chunks_exact(4).zip(weights) {
                        if let &[pr, pg, pb, pa] = pixel {
                            r += f32::from(pr) * weight;
                            g += f32::from(pg) * weight;
                            b += f32::from(pb) * weight;
                            a += f32::from(pa) * weight;
                        }
                    }
                    target.copy_from_slice(&[r, g, b, a]);
                }
                made = y;
            }
            for (into, value) in sum.iter_mut().zip(&row) {
                *into += value * share;
            }
        }
        for value in &sum {
            pixels.push((value + 0.5).clamp(0.0, 255.0) as u8);
        }
    }
    Raster {
        width,
        height,
        pixels,
    }
}

fn broken(why: impl std::fmt::Display) -> Failure {
    Failure::Broken(why.to_string())
}

/// The bytes a `width` by `height` image takes as RGBA, if that is a number.
pub(crate) fn pixel_bytes(width: u32, height: u32) -> Option<usize> {
    usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_mul(4)
}

/// `colour` laid over nothing, by `alpha`: `colour * alpha / 255`, rounded.
fn premultiplied(colour: u8, alpha: u8) -> u8 {
    u8::try_from((u16::from(colour) * u16::from(alpha) + 127) / 255).unwrap_or(u8::MAX)
}

/// A PNG opened: its reader, its size, and the bytes the decoder works in besides the pixels it
/// gives.
struct PngOpened<'a> {
    reader: png::Reader<Cursor<&'a [u8]>>,
    width: u32,
    height: u32,
    working: usize,
}

fn png_open(bytes: &[u8]) -> Result<PngOpened<'_>, Failure> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    // Palettes, see-through colours and depths other than eight become eight-bit channels.
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let reader = decoder.read_info().map_err(broken)?;
    let (width, height) = reader.info().size();
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| Failure::Broken("the image is larger than memory".into()))?;
    Ok(PngOpened {
        reader,
        width,
        height,
        working: size,
    })
}

fn png(bytes: &[u8], approve: Approve<'_>) -> Result<Raster, Failure> {
    let PngOpened {
        mut reader,
        width,
        height,
        working: size,
    } = png_open(bytes)?;
    // The decoder works in a buffer of its own, as large as the image at eight bits a channel,
    // beside the pixels it is turned into.
    approve(width, height, size)?;
    let mut raw = vec![0_u8; size];
    let info = reader.next_frame(&mut raw).map_err(broken)?;
    let raw = raw.get(..info.buffer_size()).unwrap_or_default();
    let mut pixels = Vec::with_capacity(pixel_bytes(width, height).unwrap_or(0));
    match info.color_type {
        png::ColorType::Rgba => {
            for pixel in raw.chunks_exact(4) {
                if let &[r, g, b, a] = pixel {
                    pixels.extend([
                        premultiplied(r, a),
                        premultiplied(g, a),
                        premultiplied(b, a),
                        a,
                    ]);
                }
            }
        }
        png::ColorType::Rgb => {
            for pixel in raw.chunks_exact(3) {
                if let &[r, g, b] = pixel {
                    pixels.extend([r, g, b, u8::MAX]);
                }
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for pixel in raw.chunks_exact(2) {
                if let &[v, a] = pixel {
                    let v = premultiplied(v, a);
                    pixels.extend([v, v, v, a]);
                }
            }
        }
        png::ColorType::Grayscale => {
            for &v in raw {
                pixels.extend([v, v, v, u8::MAX]);
            }
        }
        png::ColorType::Indexed => {
            return Err(Failure::Broken("a palette that was not expanded".into()));
        }
    }
    if Some(pixels.len()) != pixel_bytes(width, height) {
        return Err(Failure::Broken("fewer pixels than the header says".into()));
    }
    Ok(Raster {
        width,
        height,
        pixels,
    })
}

/// The bytes the JPEG decoder works in, a pixel at a time: its planes and the transforms it
/// holds its blocks in (a progressive file holds all of them until the last scan), which is eight
/// for every pixel of the image, and four more for the second copy of the pixels a file that is
/// turned (see [`orient`]) needs.
const fn jpeg_working(turns: bool) -> usize {
    if turns { 12 } else { 8 }
}

/// A JPEG whose headers are read: its decoder, its size as it was taken (before it is turned),
/// the way it is to be turned, the bytes its pixels take and the bytes the decoder works in
/// besides them.
struct JpegOpened<'a> {
    decoder: JpegDecoder<ZCursor<&'a [u8]>>,
    width: u32,
    height: u32,
    turn: Turn,
    size: usize,
    working: usize,
}

fn jpeg_open(bytes: &[u8]) -> Result<JpegOpened<'_>, Failure> {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder.decode_headers().map_err(broken)?;
    let (width, height) = decoder
        .dimensions()
        .and_then(|(w, h)| Some((u32::try_from(w).ok()?, u32::try_from(h).ok()?)))
        .ok_or_else(|| Failure::Broken("no size in the header".into()))?;
    // The way the file says it was held when it was taken: what a web view shows the file as.
    let turn = decoder
        .exif()
        .map_or(Turn::Upright, |exif| orientation(exif));
    let size = pixel_bytes(width, height)
        .ok_or_else(|| Failure::Broken("the image is larger than memory".into()))?;
    let working = size.saturating_mul(jpeg_working(turn != Turn::Upright)) / 4;
    Ok(JpegOpened {
        decoder,
        width,
        height,
        turn,
        size,
        working,
    })
}

fn jpeg(bytes: &[u8], approve: Approve<'_>) -> Result<Raster, Failure> {
    let JpegOpened {
        mut decoder,
        width,
        height,
        turn,
        size,
        working,
    } = jpeg_open(bytes)?;
    approve(width, height, working)?;
    if decoder.output_buffer_size() != Some(size) {
        return Err(Failure::Broken("not a colour model it can read".into()));
    }
    let mut pixels = vec![0_u8; size];
    decoder.decode_into(&mut pixels).map_err(broken)?;
    // A JPEG has no opacity: whatever the conversion left in the fourth channel, it is solid.
    for pixel in pixels.chunks_exact_mut(4) {
        if let Some(alpha) = pixel.get_mut(3) {
            *alpha = u8::MAX;
        }
    }
    Ok(orient(
        Raster {
            width,
            height,
            pixels,
        },
        turn,
    ))
}

/// How a photograph is turned to be upright: the eight values of an EXIF orientation, 1 to 8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Turn {
    /// As it is.
    Upright,
    /// Mirrored left to right.
    Mirrored,
    /// Turned half a turn.
    Half,
    /// Mirrored top to bottom.
    Flipped,
    /// Mirrored along the diagonal from the top left corner.
    Transposed,
    /// A quarter turn clockwise.
    Clockwise,
    /// Mirrored along the diagonal from the top right corner.
    Transversed,
    /// A quarter turn anticlockwise.
    Anticlockwise,
}

impl Turn {
    /// Whether the picture is turned a quarter, so that its width becomes its height.
    pub(crate) fn quarter(self) -> bool {
        matches!(
            self,
            Self::Transposed | Self::Clockwise | Self::Transversed | Self::Anticlockwise
        )
    }
}

/// The orientation the EXIF block `tiff` (from its TIFF header on) says, `Upright` where it says
/// none or the block is not one it understands: bytes from a file are never trusted to be whole.
pub(crate) fn orientation(tiff: &[u8]) -> Turn {
    read_orientation(tiff).unwrap_or(Turn::Upright)
}

fn read_orientation(tiff: &[u8]) -> Option<Turn> {
    let little = match tiff.get(..4)? {
        b"II*\0" => true,
        b"MM\0*" => false,
        _ => return None,
    };
    let short = |at: usize| -> Option<u16> {
        let two: [u8; 2] = tiff.get(at..at.checked_add(2)?)?.try_into().ok()?;
        Some(if little {
            u16::from_le_bytes(two)
        } else {
            u16::from_be_bytes(two)
        })
    };
    let long = |at: usize| -> Option<u32> {
        let four: [u8; 4] = tiff.get(at..at.checked_add(4)?)?.try_into().ok()?;
        Some(if little {
            u32::from_le_bytes(four)
        } else {
            u32::from_be_bytes(four)
        })
    };
    let ifd = usize::try_from(long(4)?).ok()?;
    let entries = usize::from(short(ifd)?);
    // A tag of the first directory: twelve bytes each, the value of a short in the last four.
    (0..entries).find_map(|n| {
        let entry = ifd.checked_add(2)?.checked_add(n.checked_mul(12)?)?;
        if short(entry)? != 0x0112 {
            return None;
        }
        Some(match short(entry.checked_add(8)?)? {
            2 => Turn::Mirrored,
            3 => Turn::Half,
            4 => Turn::Flipped,
            5 => Turn::Transposed,
            6 => Turn::Clockwise,
            7 => Turn::Transversed,
            8 => Turn::Anticlockwise,
            _ => Turn::Upright,
        })
    })
}

/// The pixels turned so that the picture is upright, as a web view shows the same file. A turn
/// of a quarter makes the width the height.
pub(crate) fn orient(raster: Raster, turn: Turn) -> Raster {
    if turn == Turn::Upright {
        return raster;
    }
    let (w, h) = (raster.width as usize, raster.height as usize);
    let across = matches!(
        turn,
        Turn::Transposed | Turn::Clockwise | Turn::Transversed | Turn::Anticlockwise
    );
    let (width, height) = if across { (h, w) } else { (w, h) };
    let mut pixels = vec![0_u8; raster.pixels.len()];
    for y in 0..h {
        for x in 0..w {
            // Where the pixel at (x, y) stands once turned.
            let (to_x, to_y) = match turn {
                Turn::Upright => (x, y),
                Turn::Mirrored => (w - 1 - x, y),
                Turn::Half => (w - 1 - x, h - 1 - y),
                Turn::Flipped => (x, h - 1 - y),
                Turn::Transposed => (y, x),
                Turn::Clockwise => (h - 1 - y, x),
                Turn::Transversed => (h - 1 - y, w - 1 - x),
                Turn::Anticlockwise => (y, w - 1 - x),
            };
            let from = (y * w + x) * 4;
            let to = (to_y * width + to_x) * 4;
            if let (Some(source), Some(target)) = (
                raster.pixels.get(from..from + 4),
                pixels.get_mut(to..to + 4),
            ) {
                target.copy_from_slice(source);
            }
        }
    }
    Raster {
        width: u32::try_from(width).unwrap_or(raster.width),
        height: u32::try_from(height).unwrap_or(raster.height),
        pixels,
    }
}

/// A parsed SVG: shapes, with a size, that can be drawn at any other.
pub(crate) struct Vector {
    tree: Tree,
    weight: Weight,
}

/// What a parsed SVG comes to, which its file does not say: `use` unfolds a part as many times as
/// it is named, and a drawing that names itself through ten levels of ten is a hundred million
/// parts in a file of two kilobytes. The parser stops at a million; this is what a view holds.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Weight {
    /// Shapes, groups and pictures, once unfolded.
    pub(crate) parts: usize,
    /// What the parsed drawing takes in memory, as nearly as can be said without asking the
    /// allocator: a fixed share for each part and the points of every shape.
    pub(crate) bytes: usize,
    /// The most layers drawn into one over another at a time (a group with opacity, a mask, a
    /// clip or a filter is drawn on a layer of its own, and the layers of the groups around it
    /// are held while it is).
    pub(crate) layers: usize,
}

/// The most parts a drawing may have once unfolded: far more than an illustration of the sizes a
/// page shows has, and far less than what the parser stops at.
const MOST_PARTS: usize = 100_000;

/// What a part of a parsed drawing takes besides its points.
const PART_BYTES: usize = 256;

/// What a point and a verb of a shape take.
const POINT_BYTES: usize = 9;

/// The images an SVG names are never read: not a path, not a URL, not its own data.
fn options() -> Options<'static> {
    Options {
        resources_dir: None,
        image_href_resolver: ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Options::default()
    }
}

/// The widest or tallest an SVG's own size is taken to be, points: past it the size is scaled down
/// to it with the proportions kept (what an SVG is drawn at is what the page says, not this).
const MOST_NATURAL: f32 = 16_384.0;

/// Counts what `tree` comes to. Without recursion: the parser allows a thousand levels.
fn weigh(tree: &Tree) -> Weight {
    use resvg::usvg::{Group, Node};
    let mut weight = Weight::default();
    // The groups still to be counted, and the layers held when they are drawn.
    let mut open: Vec<(&Group, usize)> = vec![(tree.root(), 0)];
    while let Some((group, layers)) = open.pop() {
        for node in group.children() {
            weight.parts += 1;
            weight.bytes += PART_BYTES;
            match node {
                Node::Group(inner) => {
                    let held = layers + usize::from(inner.should_isolate());
                    weight.layers = weight.layers.max(held);
                    open.push((inner, held));
                    // What a group is cut or covered by is drawn on a layer too.
                    if let Some(clip) = inner.clip_path() {
                        open.push((clip.root(), held + 1));
                    }
                    if let Some(mask) = inner.mask() {
                        open.push((mask.root(), held + 1));
                    }
                }
                Node::Path(path) => {
                    let data = path.data();
                    weight.bytes += data.points().len() * POINT_BYTES + data.verbs().len();
                }
                Node::Image(_) | Node::Text(_) => {}
            }
        }
        // A drawing that is already past what is allowed is not walked to the end.
        if weight.parts > MOST_PARTS {
            break;
        }
    }
    // The root's layer is the surface itself; a layer is counted from the first group on.
    weight.layers = weight.layers.max(1);
    weight
}

impl Vector {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, Failure> {
        let tree = Tree::from_data(bytes, &options()).map_err(broken)?;
        let weight = weigh(&tree);
        if weight.parts > MOST_PARTS {
            return Err(Failure::Complex {
                nodes: weight.parts,
                most: MOST_PARTS,
            });
        }
        Ok(Self { tree, weight })
    }

    /// What the parsed drawing comes to.
    pub(crate) fn weight(&self) -> Weight {
        self.weight
    }

    /// The size the SVG says it has, one point to a pixel.
    pub(crate) fn natural(&self) -> (f32, f32) {
        let size = self.tree.size();
        let (w, h) = (size.width(), size.height());
        let shrink = (MOST_NATURAL / w.max(h)).min(1.0);
        (w * shrink, h * shrink)
    }

    /// The bytes drawing at `width` by `height` works in besides the pixels it makes: a layer the
    /// size of the surface for every one that is held at the same time.
    pub(crate) fn working(&self, width: u32, height: u32) -> usize {
        pixel_bytes(width, height)
            .unwrap_or(usize::MAX)
            .saturating_mul(self.weight.layers)
    }

    /// The SVG drawn on a `width` by `height` surface, so that it fills it.
    pub(crate) fn draw(&self, width: u32, height: u32) -> Result<Raster, Failure> {
        let mut pixmap = Pixmap::new(width, height)
            .ok_or_else(|| Failure::Broken("no room to draw it".into()))?;
        let size = self.tree.size();
        let transform =
            Transform::from_scale(width as f32 / size.width(), height as f32 / size.height());
        resvg::render(&self.tree, transform, &mut pixmap.as_mut());
        Ok(Raster {
            width,
            height,
            pixels: pixmap.take(),
        })
    }
}
