//! PDF documents: the pages of a file a program sent, drawn at any size.
//!
//! A PDF is read by `hayro`, a PDF interpreter and rasteriser in Rust, and like an SVG it is
//! drawn from what was read, at the size the page draws it at, so that a page that is shown
//! larger is sharp. What it is held to is what an SVG is held to, and more, since a PDF can
//! name a good deal: the file may not be more than a file of the bundle may (the caller's),
//! the document not more pages than [`MOST_PAGES`], and drawing a page is approved with the
//! memory it works in ([`Document::working`]) before a pixel is made.
//!
//! **It reads nothing of the machine it is drawn on.** A file the PDF names is not opened and a
//! URL it holds is not followed (the interpreter has no such door: a link is a mark on a page, and
//! a form or a script is not run). The fonts a PDF does not carry (the fourteen it may leave out)
//! are the ones `hayro` holds itself (`embed-fonts`): no font of the machine is looked for, so
//! that a page is drawn in the same letters wherever it is shown.
//!
//! A page's own size is what the document says (the crop box, turned by `/Rotate`), in points,
//! one point to a pixel as an SVG's; the paper is white, as every viewer of a PDF draws it.

use std::sync::Arc;

use hayro::hayro_interpret::{InterpreterSettings, TransformExt as _};
use hayro::hayro_syntax::{LoadPdfError, Pdf};
use hayro::kurbo::Affine;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::vello_cpu::{Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit};
use hayro::{RenderCache, RenderSettings, render_into};
use pencere_view::Size;

use crate::decode::{Failure, Raster, pixel_bytes};

/// The most pages a document may have: far more than a book has, and few enough that the sizes of
/// them all, which are read when the file is, take no time or room worth the name.
pub(crate) const MOST_PAGES: usize = 20_000;

/// The widest or tallest a page's own size is taken to be, points: past it the size is scaled down
/// to it with the proportions kept (what a page is drawn at is what the view says, not this).
const MOST_NATURAL: f32 = 16_384.0;

/// What a parsed document takes besides its file, for each page: the share of the page tree and
/// of what is read of its objects.
const PAGE_BYTES: usize = 256;

/// How many layers the same size as the surface a page is drawn on are held at once, as nearly as
/// can be said without asking the allocator: the surface, the group the page is drawn in and one
/// more for the group a page may draw inside it. The renderer keeps no tally of its own.
const LAYERS: usize = 3;

/// A parsed PDF: pages, each with a size, that can be drawn at any other.
pub(crate) struct Document {
    pdf: Pdf,
    /// The own size of each page, in the order of the document.
    sizes: Vec<Size>,
    /// What it takes in memory, as nearly as can be said: the file it is read from and a share
    /// for each page.
    bytes: usize,
}

impl Document {
    /// Reads the PDF in `bytes`: its pages and their sizes; the pages are drawn later.
    pub(crate) fn parse(bytes: &Arc<[u8]>) -> Result<Self, Failure> {
        let data: Arc<Arc<[u8]>> = Arc::new(Arc::clone(bytes));
        let pdf = Pdf::new(data).map_err(|error| {
            Failure::Broken(match error {
                LoadPdfError::Decryption(_) => "it is encrypted, and no password is given".into(),
                LoadPdfError::Invalid => "it is not a PDF that can be read".into(),
            })
        })?;
        let pages = pdf.pages();
        if pages.len() > MOST_PAGES {
            return Err(Failure::Pages {
                count: pages.len(),
                most: MOST_PAGES,
            });
        }
        let sizes: Vec<Size> = pages
            .iter()
            .map(|page| {
                let (w, h) = page.render_dimensions();
                natural(w, h)
            })
            .collect();
        if sizes.is_empty() {
            return Err(Failure::Broken("it has no pages".into()));
        }
        let bytes = bytes.len().saturating_add(sizes.len() * PAGE_BYTES);
        Ok(Self { pdf, sizes, bytes })
    }

    /// The size of each page, in the order of the document.
    pub(crate) fn sizes(&self) -> &[Size] {
        &self.sizes
    }

    /// What the parsed document comes to.
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    /// The bytes drawing at `width` by `height` works in besides the pixels it makes: layers the
    /// size of the surface.
    pub(crate) fn working(&self, width: u32, height: u32) -> usize {
        pixel_bytes(width, height)
            .unwrap_or(usize::MAX)
            .saturating_mul(LAYERS)
    }

    /// The page at `index` (from 0) drawn on a `width` by `height` surface, so that it fills it.
    pub(crate) fn draw(&self, index: usize, width: u32, height: u32) -> Result<Raster, Failure> {
        let pages = self.pdf.pages();
        let page = pages
            .get(index)
            .ok_or_else(|| Failure::Broken("it has no such page".into()))?;
        let (pixels_w, pixels_h) = match (u16::try_from(width), u16::try_from(height)) {
            (Ok(w), Ok(h)) if w > 0 && h > 0 => (w, h),
            _ => return Err(Failure::Broken("no room to draw it".into())),
        };
        let (base_w, base_h) = page.render_dimensions();
        // The surface is what the page is drawn to fill: the scale is the surface's to the page's.
        // It is made here and not by `hayro::render`, which takes the surface's size from the
        // page's times the scale and so can fall a pixel short of the size asked for.
        let scale = Affine::scale_non_uniform(
            f64::from(width) / f64::from(base_w.max(f32::EPSILON)),
            f64::from(height) / f64::from(base_h.max(f32::EPSILON)),
        );
        let mut surface = RenderContext::new(pixels_w, pixels_h);
        render_into(
            page,
            &RenderCache::new(),
            &InterpreterSettings::default(),
            &RenderSettings::default(),
            &mut surface,
            scale * page.initial_transform(true).to_kurbo(),
        );
        surface.flush();
        let mut pixmap = Pixmap::new(pixels_w, pixels_h);
        surface.render_with(
            &mut pixmap,
            &mut Resources::default(),
            RasterizerSettings {
                target_init: TargetInit::Clear(WHITE),
                ..Default::default()
            },
        );
        Ok(Raster {
            width,
            height,
            pixels: pixmap.data_as_u8_slice().to_vec(),
        })
    }
}

/// A page's own size, not past [`MOST_NATURAL`] on its long side, the proportions kept.
fn natural(width: f32, height: f32) -> Size {
    let (w, h) = (width.max(1.0), height.max(1.0));
    let shrink = (MOST_NATURAL / w.max(h)).min(1.0);
    Size::new(w * shrink, h * shrink)
}
