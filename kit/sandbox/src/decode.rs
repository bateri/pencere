//! Reading the formats the images of a page come in: PNG, JPEG and SVG.
//!
//! The bytes are not trusted: they are a file a program sent, and what is in them is whatever its
//! author wrote. So the format is told from the bytes and not from a name, the size of an image is
//! read from its header and approved **before** a pixel is made ([`Approve`]), and a decoder that
//! fails — or gives up by panicking, which the caller catches — leaves the image refused and
//! nothing else harmed.
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
    /// The image would take the pixels of the view past what the view may hold.
    Budget { most: usize },
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
            Self::Paths { most } => write!(f, "is past the {most} images a view may name"),
        }
    }
}

/// What approves an image of a size, once its header says it, before any pixel is made.
pub(crate) type Approve<'a> = &'a mut dyn FnMut(u32, u32) -> Result<(), Failure>;

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

fn png(bytes: &[u8], approve: Approve<'_>) -> Result<Raster, Failure> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    // Palettes, see-through colours and depths other than eight become eight-bit channels.
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(broken)?;
    let (width, height) = reader.info().size();
    approve(width, height)?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| Failure::Broken("the image is larger than memory".into()))?;
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

fn jpeg(bytes: &[u8], approve: Approve<'_>) -> Result<Raster, Failure> {
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder.decode_headers().map_err(broken)?;
    let (width, height) = decoder
        .dimensions()
        .and_then(|(w, h)| Some((u32::try_from(w).ok()?, u32::try_from(h).ok()?)))
        .ok_or_else(|| Failure::Broken("no size in the header".into()))?;
    approve(width, height)?;
    let size = pixel_bytes(width, height)
        .ok_or_else(|| Failure::Broken("the image is larger than memory".into()))?;
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
    Ok(Raster {
        width,
        height,
        pixels,
    })
}

/// A parsed SVG: shapes, with a size, that can be drawn at any other.
pub(crate) struct Vector {
    tree: Tree,
}

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

impl Vector {
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, Failure> {
        let tree = Tree::from_data(bytes, &options()).map_err(broken)?;
        Ok(Self { tree })
    }

    /// The size the SVG says it has, one point to a pixel.
    pub(crate) fn natural(&self) -> (f32, f32) {
        let size = self.tree.size();
        let (w, h) = (size.width(), size.height());
        let shrink = (MOST_NATURAL / w.max(h)).min(1.0);
        (w * shrink, h * shrink)
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
