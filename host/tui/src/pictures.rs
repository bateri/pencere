//! The pixels of a picture, which the renderer is told by whoever has the files.
//!
//! The decoders of images and the rasteriser of a page of a PDF are the sandbox's alone: every
//! terminal accepts the same formats and sizes. This crate asks for the pixels ([`Pictures`]) and
//! draws what it is given.

use pencere_sandbox::Images;
use pencere_view::{Picture, Rect};

/// The pixels of a picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    /// `4 * width * height` bytes, sRGB, **premultiplied** by the opacity, rows from the top.
    pub pixels: Vec<u8>,
    /// What the picture's rectangle shows of the pixels, as shares of them.
    pub crop: Rect,
}

impl Raster {
    /// The pixel at `x`, `y`: red, green, blue and opacity, premultiplied.
    pub(crate) fn at(&self, x: u32, y: u32) -> [u8; 4] {
        if x >= self.width || y >= self.height {
            return [0; 4];
        }
        let start = (y as usize * self.width as usize + x as usize) * 4;
        match self.pixels.get(start..start + 4) {
            Some(&[r, g, b, a]) => [r, g, b, a],
            _ => [0; 4],
        }
    }

    /// The mean of the pixels in the part `u0, v0` to `u1, v1` of these pixels (shares of them):
    /// red, green, blue and opacity, premultiplied, each `0.0..=255.0`. A part smaller than a pixel
    /// is the pixel it lies in.
    pub(crate) fn mean(&self, (u0, v0): (f32, f32), (u1, v1): (f32, f32)) -> [f32; 4] {
        let edge = |share: f32, side: u32| (share * side as f32).clamp(0.0, side as f32);
        let (x0, x1) = (edge(u0.min(u1), self.width), edge(u0.max(u1), self.width));
        let (y0, y1) = (edge(v0.min(v1), self.height), edge(v0.max(v1), self.height));
        let first = |start: f32| start.floor() as u32;
        let last = |start: f32, end: f32| (end.ceil() as u32).max(first(start) + 1);
        let mut sum = [0.0_f32; 4];
        let mut count = 0.0_f32;
        for y in first(y0)..last(y0, y1) {
            for x in first(x0)..last(x0, x1) {
                let pixel = self.at(x, y);
                for (total, channel) in sum.iter_mut().zip(pixel) {
                    *total += f32::from(channel);
                }
                count += 1.0;
            }
        }
        if count > 0.0 {
            sum.map(|total| total / count)
        } else {
            [0.0; 4]
        }
    }

    /// Where the picture has something drawn: the smallest rectangle (shares of the picture) that holds
    /// every pixel that is more than a hint of colour. `None` for a picture with nothing in it.
    pub(crate) fn ink_bounds(&self) -> Option<Rect> {
        let mut found: Option<(u32, u32, u32, u32)> = None;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.at(x, y)[3] < 16 {
                    continue;
                }
                found = Some(match found {
                    Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                    None => (x, y, x, y),
                });
            }
        }
        let (x0, y0, x1, y1) = found?;
        let (width, height) = (self.width as f32, self.height as f32);
        Some(Rect::new(
            x0 as f32 / width,
            y0 as f32 / height,
            (x1 + 1 - x0) as f32 / width,
            (y1 + 1 - y0) as f32 / height,
        ))
    }

    /// Whether every pixel covers what is under it altogether.
    pub(crate) fn is_opaque(&self) -> bool {
        self.pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| matches!(pixel, [_, _, _, 255]))
    }
}

/// Where the pixels of the pictures of a page are from.
pub trait Pictures {
    /// The pixels for `picture`: a picture that is as many pixels across as its rectangle is wide in
    /// the draw list's pixels. `None` while they are not there: an image is decoded, and a page of a
    /// PDF drawn, where the sandbox does it, and the renderer says that it waited
    /// ([`crate::Frame::waiting`]) so that it is asked again.
    fn raster(&self, picture: &Picture) -> Option<Raster>;
}

/// No pictures: each is waited for forever, which is to say left out.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoPictures;

impl Pictures for NoPictures {
    fn raster(&self, _picture: &Picture) -> Option<Raster> {
        None
    }
}

impl Pictures for Images {
    fn raster(&self, picture: &Picture) -> Option<Raster> {
        let bitmap = Images::picture(self, picture)?;
        Some(Raster {
            width: bitmap.width,
            height: bitmap.height,
            pixels: bitmap.pixels().to_vec(),
            crop: bitmap.crop_of(picture),
        })
    }
}

/// The images of a page the host has, where it has them: none is no pixels, waited for.
impl Pictures for Option<&Images> {
    fn raster(&self, picture: &Picture) -> Option<Raster> {
        self.and_then(|images| Pictures::raster(images, picture))
    }
}
