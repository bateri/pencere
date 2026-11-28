//! Pictures of a page, for the eye: a frame's draw list drawn into a PNG in the fonts the caller
//! gives and a theme of the kit's roles — the boxes, the words, the colours, the pictures and the
//! editors where the kit put them, which is what an eye checks (what is aligned, what overlaps, what
//! is cut). Not a terminal's drawing: no shaping, and a soft shadow made as three passes of a box
//! blur (a good Gaussian), which is what the eye needs of one. A host's text fields are not in a
//! draw list: [`crate::Headless::shot`] draws the headless host's over it.
//!
//! **The fonts are the caller's.** Where it gives none, each character is drawn as a box a character
//! wide ([`Fonts::none`]): a picture made on any machine, with no font of its system, is the same
//! picture, byte for byte. Where it gives faces ([`Fonts::installed`]: the machine's, with a face
//! for a character one lacks; [`Fonts::carried`]: the ones the program carries), the page is to be
//! laid out with the same [`Fonts`] (they are a `Measure`), so that its words fill the boxes they were
//! measured for.

use pencere_sandbox::{Images, draw_figure};
use pencere_view::{Clip, Color, DrawList, Paint, Rect, Role, Source};
use tiny_skia::{
    FillRule, Mask, Paint as Ink, PathBuilder, Pixmap, PixmapMut, PixmapPaint, PixmapRef, Stroke,
    Transform,
};

mod find;
mod fonts;
mod slide;

use fonts::Shape;
pub use fonts::{FontError, Fonts};
pub use slide::{Side, Slide, slide, spread};

/// The colours a picture gives the kit's roles: a theme of a terminal's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Palette {
    /// A dark terminal's (GitHub's dark colours).
    Dark,
    /// A light terminal's (GitHub's light colours).
    Light,
}

fn hex(rgb: u32) -> [f32; 4] {
    let channel = |shift: u32| f32::from(u8::try_from((rgb >> shift) & 0xff).unwrap_or(0)) / 255.0;
    [channel(16), channel(8), channel(0), 1.0]
}

fn mix(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let at = |i: usize| {
        let (x, y) = (
            a.get(i).copied().unwrap_or(0.0),
            b.get(i).copied().unwrap_or(0.0),
        );
        x + (y - x) * t
    };
    [at(0), at(1), at(2), 1.0]
}

impl Palette {
    /// What `role` is in this theme, as straight RGBA from 0 to 1.
    pub fn role(self, role: Role) -> [f32; 4] {
        const DARK: [u32; 16] = [
            0x484f58, 0xff7b72, 0x3fb950, 0xd29922, 0x58a6ff, 0xbc8cff, 0x39c5cf, 0xb1bac4,
            0x6e7681, 0xffa198, 0x56d364, 0xe3b341, 0x79c0ff, 0xd2a8ff, 0x56d4dd, 0xf0f6fc,
        ];
        const LIGHT: [u32; 16] = [
            0x24292f, 0xcf222e, 0x116329, 0x4d2d00, 0x0969da, 0x8250df, 0x1b7c83, 0x6e7781,
            0x57606a, 0xa40e26, 0x1a7f37, 0x633c01, 0x218bff, 0xa475f9, 0x3192aa, 0x8c959f,
        ];
        let (ground, ink, dim, accent, ansi) = match self {
            Self::Dark => (0x0d1117, 0xc9d1d9, 0x8b949e, 0x58a6ff, DARK),
            Self::Light => (0xffffff, 0x1f2328, 0x656d76, 0x0969da, LIGHT),
        };
        let (ground, ink, dim) = (hex(ground), hex(ink), hex(dim));
        match role {
            Role::Background => ground,
            Role::Foreground => ink,
            Role::Dim => dim,
            Role::Accent | Role::Info => hex(accent),
            Role::Cursor => hex(accent),
            Role::Success => hex(ansi[2]),
            Role::Error => hex(ansi[1]),
            Role::Warning => hex(ansi[3]),
            Role::Border => mix(ground, ink, 0.18),
            Role::Selection => mix(ground, hex(accent), 0.3),
            Role::Raised(n) => mix(ground, ink, 0.04 * f32::from(n.min(8))),
            Role::Quiet => mix(dim, ground, 1.0 / 3.0),
            Role::Shadow => [0.0, 0.0, 0.0, 1.0],
            other => {
                let index = Role::ANSI
                    .iter()
                    .position(|ansi| *ansi == other)
                    .unwrap_or(7);
                hex(ansi.get(index).copied().unwrap_or(0x808080))
            }
        }
    }

    /// `color` in this theme.
    fn resolve(self, color: Color) -> [f32; 4] {
        color.resolve(|source| match source {
            Source::Role(role) => self.role(role),
            Source::Rgba { light, dark } => {
                let value = match self {
                    Self::Dark => dark.unwrap_or(light),
                    Self::Light => light,
                };
                [
                    f32::from(value.r) / 255.0,
                    f32::from(value.g) / 255.0,
                    f32::from(value.b) / 255.0,
                    f32::from(value.a) / 255.0,
                ]
            }
        })
    }
}

/// Why no picture was made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A picture of no pixels, or of more than a picture may have.
    Size(u32, u32),
    /// A soft shadow reaches the part of a picture asked for, and is blurred over the picture's whole
    /// width: the part cannot be drawn alone.
    Shadow,
    /// The PNG could not be written.
    Encode(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Size(w, h) => write!(f, "no picture is {w} × {h} pixels"),
            Self::Shadow => write!(f, "a soft shadow reaches the part of the picture asked for"),
            Self::Encode(why) => write!(f, "the picture could not be written: {why}"),
        }
    }
}

impl std::error::Error for Error {}

/// The draw list `list` of a view of `width` × `height` pixels drawn into a PNG: in `fonts`, in the
/// colours of `palette`, the pictures from `images` (a page's, [`crate::Page::images`]). The same
/// list, fonts and pictures make the same bytes.
///
/// # Errors
///
/// A picture of no pixels, or one that could not be written.
pub fn png(
    list: &DrawList,
    size: (u32, u32),
    fonts: &Fonts,
    images: Option<&Images>,
    palette: Palette,
) -> Result<Vec<u8>, Error> {
    raster(list, size, fonts, images, palette)?.png()
}

/// A picture's pixels, drawn and not yet written as a file: opaque RGBA, a row after a row.
pub struct Raster {
    pixmap: Pixmap,
}

impl Raster {
    /// How many pixels across.
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    /// How many pixels down.
    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    /// The pixels, four bytes each, red first (every one opaque, so straight and premultiplied are
    /// the same).
    pub fn rgba(&self) -> &[u8] {
        self.pixmap.data()
    }

    /// A picture of the opaque RGBA `bytes` of `width` × `height` pixels (a measurement's).
    pub fn from_rgba(width: u32, height: u32, bytes: Vec<u8>) -> Option<Raster> {
        Pixmap::from_vec(bytes, tiny_skia::IntSize::from_wh(width, height)?)
            .map(|pixmap| Raster { pixmap })
    }

    /// The `rows` rows from row `top`, as a picture of their own.
    pub fn strip(&self, top: u32, rows: u32) -> Option<Raster> {
        let rect = tiny_skia::IntRect::from_xywh(0, i32::try_from(top).ok()?, self.width(), rows)?;
        self.pixmap.clone_rect(rect).map(|pixmap| Raster { pixmap })
    }

    /// The pixels as a PNG.
    ///
    /// # Errors
    ///
    /// The PNG could not be written.
    pub fn png(&self) -> Result<Vec<u8>, Error> {
        self.pixmap
            .encode_png()
            .map_err(|why| Error::Encode(why.to_string()))
    }
}

/// The buffers a picture is drawn in, kept from one picture to the next: a frame that comes at the
/// rate of keys is drawn in the memory the frame before it was, and asks the system for no picture's
/// worth of fresh pages (which the system makes zero, and which the drawing then writes over). A
/// buffer grows to the largest picture asked of it and stays. What a picture leaves in one is not
/// what the next starts from: the rows are drawn over from the ground, and the mask is cleared where
/// its last clip was.
#[derive(Default)]
pub struct Scratch {
    /// The rows being drawn, four bytes a pixel.
    pixels: Vec<u8>,
    tools: Tools,
    grown: u32,
}

impl Scratch {
    /// Buffers with nothing in them yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The first `length` bytes of the buffer the rows are drawn in: for a picture drawn from its
    /// top (`raster_rows_in` with a `top` of 0) in a `Scratch` of its own, which the bytes it
    /// handed back are all of, and which is kept as it was drawn.
    pub fn into_pixels(mut self, length: usize) -> Vec<u8> {
        self.pixels.truncate(length);
        self.pixels
    }

    /// How many times a buffer had to grow, to hold a picture larger than any before it: a picture
    /// drawn in what the pictures before it were adds none.
    pub fn grown(&self) -> u32 {
        self.grown + self.tools.grown
    }
}

/// What the drawing is cut through and what a soft shadow is made in: the buffers of a [`Scratch`]
/// that are not the pixels.
#[derive(Default)]
struct Tools {
    /// The mask the drawing is cut through: one byte a pixel, zero but where `inked` says.
    mask: Option<Mask>,
    /// The clip `mask` is the cut of.
    clip: Option<Clip>,
    /// The columns and rows (end excluded) `mask` is not zero in.
    inked: Option<(usize, usize, usize, usize)>,
    /// A shadow's silhouette as pixels, and the coverage of the silhouette and of the box itself.
    under: Vec<u8>,
    cover: Vec<f32>,
    own: Vec<f32>,
    /// The room the blur of a shadow works in.
    blur: pencere_sandbox::Blur,
    grown: u32,
}

impl Tools {
    /// The mask of `clip` for a picture of `size`: what is drawn through it is cut there. The one
    /// buffer is cleared where its last clip was and written where this one is, and is as it was
    /// where the clip is the same. `None` where the clip is no rectangle.
    fn masked(&mut self, size: (u32, u32), clip: Clip) -> Option<&Mask> {
        let rect = tiny_skia::Rect::from_xywh(clip.rect.x, clip.rect.y, clip.rect.w, clip.rect.h)?;
        let (w, h) = (size.0 as usize, size.1 as usize);
        if self.mask.as_ref().map(|mask| (mask.width(), mask.height())) != Some(size) {
            let (mut bytes, across) = self.mask.take().map_or((Vec::new(), 0), |mask| {
                let across = mask.width();
                (mask.take(), across)
            });
            if across != size.0 {
                // Rows of another width are not rows of this one: begin again from zero.
                bytes.clear();
                self.inked = None;
            }
            if bytes.capacity() < w * h {
                self.grown += 1;
            }
            bytes.resize(w * h, 0);
            self.mask = Some(Mask::from_vec(
                bytes,
                tiny_skia::IntSize::from_wh(size.0, size.1)?,
            )?);
            self.clip = None;
        }
        let mask = self.mask.as_mut()?;
        if self.clip != Some(clip) {
            if let Some((x0, y0, x1, y1)) = self.inked.take() {
                let data = mask.data_mut();
                for row in y0..y1.min(h) {
                    if let Some(cells) = data.get_mut(row * w + x0..row * w + x1.min(w)) {
                        cells.fill(0);
                    }
                }
            }
            fill_clip(mask, clip, rect);
            // Where a fill of the rectangle can write: the pixels it covers and the rounding of its edges.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let inked = (
                ((clip.rect.x.floor() - 1.0).max(0.0) as usize).min(w),
                ((clip.rect.y.floor() - 1.0).max(0.0) as usize).min(h),
                ((clip.rect.right().ceil() + 1.0).max(0.0) as usize).min(w),
                ((clip.rect.bottom().ceil() + 1.0).max(0.0) as usize).min(h),
            );
            self.inked = Some(inked);
            self.clip = Some(clip);
        }
        self.mask.as_ref()
    }
}

/// How much of the pixel at (`px`, `py`) the round corners and the soft edge of a clip let be drawn:
/// all of it where there are none.
fn clip_cover(clip: Clip, px: i64, py: i64) -> f32 {
    if clip.round.is_none() && clip.fade.is_none() {
        return 1.0;
    }
    #[allow(clippy::cast_precision_loss)]
    clip.cover(px as f32 + 0.5, py as f32 + 0.5)
}

/// A mask of its own, of `clip`, for a picture of `size`: for a drawing that cuts it further.
fn fresh_mask(size: (u32, u32), clip: Clip) -> Option<Mask> {
    let mut mask = Mask::new(size.0, size.1)?;
    let rect = tiny_skia::Rect::from_xywh(clip.rect.x, clip.rect.y, clip.rect.w, clip.rect.h)?;
    fill_clip(&mut mask, clip, rect);
    Some(mask)
}

/// Writes `clip` into a mask that is zero where it writes: the rectangle `rect` (the clip's) and,
/// where the clip is rounded, the part of it the corners leave, their edge smoothed.
fn fill_clip(mask: &mut Mask, clip: Clip, rect: tiny_skia::Rect) {
    match clip
        .round
        .and_then(|round| rounded(round.rect, round.radii))
    {
        Some(shape) => {
            mask.fill_path(&shape, FillRule::Winding, true, Transform::identity());
            mask.intersect_path(
                &PathBuilder::from_rect(rect),
                FillRule::Winding,
                false,
                Transform::identity(),
            );
        }
        None => mask.fill_path(
            &PathBuilder::from_rect(rect),
            FillRule::Winding,
            false,
            Transform::identity(),
        ),
    }
    if let Some(fade) = clip.fade {
        thin_out(mask, fade, clip.rect);
    }
}

/// What a soft edge leaves of a mask that was written for its clip's rectangle `rect`: each pixel
/// of it kept as much as the fade says.
fn thin_out(mask: &mut Mask, fade: pencere_view::FadeEdge, rect: Rect) {
    let (w, h) = (mask.width() as usize, mask.height() as usize);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x0, y0, x1, y1) = (
        ((rect.x.floor() - 1.0).max(0.0) as usize).min(w),
        ((rect.y.floor() - 1.0).max(0.0) as usize).min(h),
        ((rect.right().ceil() + 1.0).max(0.0) as usize).min(w),
        ((rect.bottom().ceil() + 1.0).max(0.0) as usize).min(h),
    );
    let data = mask.data_mut();
    for row in y0..y1 {
        #[allow(clippy::cast_precision_loss)]
        let y = row as f32 + 0.5;
        let Some(cells) = data.get_mut(row * w + x0..row * w + x1) else {
            continue;
        };
        for (column, cell) in cells.iter_mut().enumerate() {
            if *cell == 0 {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let x = (x0 + column) as f32 + 0.5;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let kept = (f32::from(*cell) * fade.cover(x, y))
                .round()
                .clamp(0.0, 255.0) as u8;
            *cell = kept;
        }
    }
}

/// The draw list `list` of a view of `width` × `height` pixels drawn into pixels: as [`png`] draws
/// them, before they are written as a file.
///
/// # Errors
///
/// A picture of no pixels.
pub fn raster(
    list: &DrawList,
    size: (u32, u32),
    fonts: &Fonts,
    images: Option<&Images>,
    palette: Palette,
) -> Result<Raster, Error> {
    let (width, height) = size;
    let mut pixmap = Pixmap::new(width, height).ok_or(Error::Size(width, height))?;
    let mut view = pixmap.as_mut();
    ground(&mut view, palette);
    #[allow(clippy::cast_precision_loss)]
    let bounds = (width as f32, height as f32);
    draw(
        view,
        bounds,
        &mut Tools::default(),
        list.items.iter(),
        fonts,
        images,
        palette,
        0,
    );
    Ok(Raster { pixmap })
}

/// The `rows` rows of the picture that [`raster`] draws, from row `top`, drawn alone: the same pixels,
/// to the byte, and only what reaches those rows ([`reach`]) is drawn, on a picture of those rows
/// (and the rows a soft shadow that reaches them is blurred over, which are drawn and not kept). What
/// a frame is made of whose other rows did not change.
///
/// # Errors
///
/// A picture of no pixels, or rows that are none of it.
pub fn raster_rows(
    list: &DrawList,
    size: (u32, u32),
    fonts: &Fonts,
    images: Option<&Images>,
    palette: Palette,
    (top, rows): (u32, u32),
) -> Result<Raster, Error> {
    let width = size.0;
    let rows = rows.min(size.1.saturating_sub(top));
    let bytes = raster_rows_in(
        &mut Scratch::new(),
        list,
        size,
        fonts,
        images,
        palette,
        (top, rows),
        (0, 0),
    )?
    .to_vec();
    Raster::from_rgba(width, rows, bytes).ok_or(Error::Size(width, rows))
}

/// The rows [`raster_rows`] draws, drawn in `scratch` and handed back where they were drawn, with no
/// copy of them made: nothing is asked of the system once `scratch` has held rows as many. `margin`
/// is how many pixels more there are at the right and at the foot of the rows than the picture has:
/// the picture's last column is repeated into the first of them and its last row (and the margin of
/// that row) into the second, as a terminal's window that is not a whole number of cells has the
/// page's edge. The bytes are `width + right` pixels across and `rows + below` down, four bytes each.
///
/// # Errors
///
/// A picture of no pixels, or rows that are none of it.
#[allow(clippy::too_many_arguments)]
pub fn raster_rows_in<'a>(
    scratch: &'a mut Scratch,
    list: &DrawList,
    size: (u32, u32),
    fonts: &Fonts,
    images: Option<&Images>,
    palette: Palette,
    (top, rows): (u32, u32),
    (right, below): (u32, u32),
) -> Result<&'a [u8], Error> {
    raster_part_in(
        scratch,
        list,
        size,
        fonts,
        images,
        palette,
        (top, rows),
        (0, size.0),
        (right, below),
    )
}

/// The same, of the columns `left`..`to` of the rows alone: only what reaches them ([`reach`],
/// [`across`]) is drawn, on a buffer of those columns, and the bytes are `to - left + right` pixels
/// across. A box, a picture or a figure is moved to the buffer's corner as the rows are (so its edge
/// that is between two pixels may be a level or sixteen apart from the same edge drawn across the
/// picture, in a pixel or two of a rounded corner: a sixteenth of a pixel's coverage), the words are
/// not (where a letter is is rounded in the picture's columns, and the columns left of the part are
/// taken off after it): they are the same to the byte. The margin at the right is the page's edge
/// only where the columns go to the picture's right (`to` is its width), and none where they do not.
/// A soft shadow is blurred over the whole picture's width and not over part of it: where one reaches
/// the part, it is not drawn alone.
///
/// # Errors
///
/// A picture of no pixels, rows that are none of it, or columns that are none of it; or a shadow that
/// reaches a part that is not the whole width ([`Error::Shadow`]).
#[allow(clippy::too_many_arguments)]
pub fn raster_part_in<'a>(
    scratch: &'a mut Scratch,
    list: &DrawList,
    size: (u32, u32),
    fonts: &Fonts,
    images: Option<&Images>,
    palette: Palette,
    (top, rows): (u32, u32),
    (left, to): (u32, u32),
    (right, below): (u32, u32),
) -> Result<&'a [u8], Error> {
    let (width, height) = size;
    let to = to.min(width);
    let bottom = top.saturating_add(rows).min(height);
    if bottom <= top || width == 0 || to <= left {
        return Err(Error::Size(width, rows));
    }
    let whole_width = left == 0 && to == width;
    // The margin at the right is the picture's edge repeated: of the part of it that has the edge.
    let right = if to == width { right } else { 0 };
    // What reaches the rows (and the columns); the whole picture is every item.
    let all = top == 0 && bottom >= height && whole_width;
    #[allow(clippy::cast_precision_loss)]
    let (above, under, before, after) = (top as f32, bottom as f32, left as f32, to as f32);
    let reaching: Vec<(&Paint, (f32, f32))> = list
        .items
        .iter()
        .filter_map(|item| {
            if all {
                return Some((item, (above, under)));
            }
            reach(item, fonts).map(|rows| (item, rows))
        })
        .filter(|(_, (first, last))| *first < under && *last > above)
        .filter(|(item, _)| {
            // A line of words is set down to know where it ends only where its clip is in the columns.
            whole_width
                || (!matches!(item, Paint::Text(run) if run.clip.rect.x >= after || run.clip.rect.right() <= before)
                    && across(item, fonts).is_some_and(|(first, last)| first < after && last > before))
        })
        .collect();
    // A shadow is blurred over the rows it reaches, cut at its clip, as a grid of them: the picture
    // has them all, so that it is blurred over the same grid as in the whole picture.
    let (mut from, mut to_row) = (top, bottom);
    for (item, (first, last)) in &reaching {
        if !all && matches!(item, Paint::Quad(quad) if quad.shadow.is_some()) {
            if !whole_width {
                return Err(Error::Shadow);
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                from = from.min(first.floor().max(0.0) as u32);
                to_row = to_row.max((last.ceil().max(0.0) as u32).min(height));
            }
        }
    }
    // The rows drawn, and the rows the buffer holds: the margin at the foot is under the last kept.
    let part = to - left;
    let across = part.checked_add(right).ok_or(Error::Size(width, rows))?;
    let kept = (bottom - top) as usize;
    let first = (top - from) as usize;
    let high = (to_row - from).max(bottom - from + below);
    let row = across as usize * 4;
    let len = row * high as usize;
    if scratch.pixels.len() < len {
        scratch.grown += 1;
        scratch.pixels.resize(len, 0);
    }
    let Scratch { pixels, tools, .. } = scratch;
    let all_rows = pixels.get_mut(..len).ok_or(Error::Size(across, high))?;
    {
        let mut view =
            PixmapMut::from_bytes(&mut *all_rows, across, high).ok_or(Error::Size(across, high))?;
        ground(&mut view, palette);
        let moved: Vec<Paint>;
        let items: Vec<&Paint> = if from == 0 && left == 0 {
            reaching.iter().map(|(item, _)| *item).collect()
        } else {
            #[allow(clippy::cast_precision_loss)]
            {
                moved = reaching
                    .iter()
                    .map(|(item, _)| shifted(item, -(left as f32), -(from as f32)))
                    .collect();
            }
            moved.iter().collect()
        };
        // The shadows are blurred inside the picture the rows are of, not inside the margin.
        #[allow(clippy::cast_precision_loss)]
        let bounds = (width as f32, (to_row - from) as f32);
        draw(
            view,
            bounds,
            tools,
            items.into_iter(),
            fonts,
            images,
            palette,
            left,
        );
    }
    // The margin: what was drawn there is not the page's edge, which is the last column and last row.
    if right > 0 {
        for at in first..first + kept {
            let last = at * row + (part as usize - 1) * 4;
            let Some(edge) = all_rows
                .get(last..last + 4)
                .and_then(|pixel| <[u8; 4]>::try_from(pixel).ok())
            else {
                continue;
            };
            if let Some(margin) = all_rows.get_mut(last + 4..(at + 1) * row) {
                for pixel in margin.as_chunks_mut::<4>().0 {
                    *pixel = edge;
                }
            }
        }
    }
    let last = (first + kept - 1) * row;
    for more in 1..=below as usize {
        all_rows.copy_within(last..last + row, last + more * row);
    }
    all_rows
        .get(first * row..(first + kept + below as usize) * row)
        .ok_or(Error::Size(across, high))
}

/// The ground of `palette` under a picture.
fn ground(pixmap: &mut PixmapMut<'_>, palette: Palette) {
    let [r, g, b, _] = palette.role(Role::Background);
    if let Some(color) = tiny_skia::Color::from_rgba(r, g, b, 1.0) {
        pixmap.fill(color);
    }
}

/// Where the rows of a picture are that `item` can change: from its top to its bottom, pixels, and
/// `None` where it draws nothing. What a row of the picture is is what the items that reach it draw
/// on it, in order; so a row whose items are the same as before is the same, and a row that none
/// reaches is the ground. The bounds are wide where a drawing reaches past its box (a shadow's
/// blur, a letter's accent or tail, a turned picture's corners) and cut at the item's clip.
pub fn reach(item: &Paint, fonts: &Fonts) -> Option<(f32, f32)> {
    let (top, bottom, clip) = match item {
        Paint::Quad(quad) => {
            let (mut top, mut bottom) = (quad.rect.y, quad.rect.bottom());
            if let Some(shadow) = quad.shadow {
                // As `Canvas::shadow` works out the rows it is blurred over.
                let sigma = (shadow.blur * 0.5).max(0.5);
                let blur = (sigma * 3.0).ceil() + 1.0;
                top = top.min(quad.rect.y + shadow.dy - blur);
                bottom = bottom.max(quad.rect.bottom() + shadow.dy + blur);
            }
            (top - 1.0, bottom + 1.0, quad.clip.rect)
        }
        Paint::Text(run) => {
            let (_, line) = fonts.line(&run.font);
            let line = line.max(run.font.size);
            (
                run.origin.y - line * 0.25 - 1.0,
                run.origin.y + line * 1.25 + 1.0,
                run.clip.rect,
            )
        }
        Paint::Image(picture) => {
            let rect = picture.rect;
            if picture.turn == 0.0 {
                (rect.y - 1.0, rect.bottom() + 1.0, picture.clip.rect)
            } else {
                // Turned about its middle: as far as its corners go.
                let half = rect.w.hypot(rect.h) / 2.0;
                let middle = rect.y + rect.h / 2.0;
                (middle - half - 1.0, middle + half + 1.0, picture.clip.rect)
            }
        }
        Paint::Figure(figure) => (
            figure.rect.y - 1.0,
            figure.rect.bottom() + 1.0,
            figure.clip.rect,
        ),
        Paint::Editor(_) | Paint::Browser(_) => return None,
    };
    let (top, bottom) = (top.max(clip.y), bottom.min(clip.bottom()));
    (bottom > top).then_some((top, bottom))
}

/// Where the columns of a picture are that `item` can change: from its left to its right, pixels, and
/// `None` where it draws nothing. What a column of a row is is what the items that reach the row
/// and the column draw on it, in order; so a column whose items are the same as before is the same.
/// The bounds are wide where a drawing reaches past its box (the lean of a letter, a turned
/// picture's corners), a box's edge is a pixel past it, a line of words is as wide as it is set down
/// (the lean of its letters and its rule included), all cut at the item's clip; and they are the
/// whole line where a shadow's blur, which is made over the width of the picture, is.
pub fn across(item: &Paint, fonts: &Fonts) -> Option<(f32, f32)> {
    let (left, right, clip) = match item {
        Paint::Quad(quad) => {
            if quad.shadow.is_some() {
                return Some((f32::NEG_INFINITY, f32::INFINITY));
            }
            (quad.rect.x - 1.0, quad.rect.right() + 1.0, quad.clip.rect)
        }
        Paint::Text(run) => {
            let (left, right) = words_across(run, fonts)?;
            (left - 1.0, right + 1.0, run.clip.rect)
        }
        Paint::Image(picture) => {
            let rect = picture.rect;
            if picture.turn == 0.0 {
                (rect.x - 1.0, rect.right() + 1.0, picture.clip.rect)
            } else {
                // Turned about its middle: as far as its corners go.
                let half = rect.w.hypot(rect.h) / 2.0;
                let middle = rect.x + rect.w / 2.0;
                (middle - half - 1.0, middle + half + 1.0, picture.clip.rect)
            }
        }
        Paint::Figure(figure) => (
            figure.rect.x - 1.0,
            figure.rect.right() + 1.0,
            figure.clip.rect,
        ),
        Paint::Editor(_) | Paint::Browser(_) => return None,
    };
    let (left, right) = (left.max(clip.x), right.min(clip.right()));
    (right > left).then_some((left, right))
}

/// From where to where the words of `run` are drawn, as `Canvas::text` draws them: the pixels of its
/// letters, the box of one a face lacks, and the rule under or through it. None where it draws none.
fn words_across(run: &pencere_view::Run, fonts: &Fonts) -> Option<(f32, f32)> {
    let mut span: Option<(f32, f32)> = None;
    let mut take = |left: f32, right: f32| {
        span = Some(span.map_or((left, right), |(l, r)| (l.min(left), r.max(right))));
    };
    let width = match fonts.set_down(&run.font, &run.text) {
        Some(set) => {
            for mark in &set.marks {
                let x = run.origin.x + mark.at;
                match &mark.shape {
                    Shape::Nothing => {}
                    Shape::Glyph(glyph) => {
                        #[allow(clippy::cast_precision_loss)]
                        let at = x + glyph.left as f32;
                        #[allow(clippy::cast_precision_loss)]
                        take(at, at + glyph.width as f32);
                    }
                    Shape::Box => take(x, x + mark.advance.max(4.0)),
                }
            }
            set.width
        }
        None => {
            // A box for each character that is not a space, a character's width apart.
            let advance = fonts.box_advance();
            #[allow(clippy::cast_precision_loss)]
            let width = run.text.chars().count() as f32 * advance;
            if run.text.chars().any(|c| !c.is_whitespace()) {
                take(run.origin.x, run.origin.x + width + 1.0);
            }
            width
        }
    };
    if run.underline || run.strike {
        take(run.origin.x, run.origin.x + width);
    }
    span
}

/// `item` moved down by `dy` pixels, and right by `dx`.
fn shifted(item: &Paint, dx: f32, dy: f32) -> Paint {
    let moved = |r: Rect| Rect::new(r.x + dx, r.y + dy, r.w, r.h);
    match item {
        Paint::Quad(quad) => {
            let mut quad = quad.clone();
            quad.rect = moved(quad.rect);
            quad.clip = quad.clip.translate(dx, dy);
            Paint::Quad(quad)
        }
        // The words are not moved across: where a letter is is rounded in the picture's columns,
        // and the same sum rounds the same where the canvas takes `dx` off after it (`Canvas::left`).
        Paint::Text(run) => {
            let mut run = run.clone();
            run.origin.y += dy;
            run.clip = run.clip.translate(dx, dy);
            Paint::Text(run)
        }
        Paint::Image(picture) => {
            let mut picture = picture.clone();
            picture.rect = moved(picture.rect);
            picture.clip = picture.clip.translate(dx, dy);
            Paint::Image(picture)
        }
        Paint::Figure(figure) => {
            let mut figure = figure.clone();
            figure.rect = moved(figure.rect);
            figure.clip = figure.clip.translate(dx, dy);
            Paint::Figure(figure)
        }
        Paint::Editor(slot) => Paint::Editor(slot.clone()),
        Paint::Browser(slot) => Paint::Browser(slot.clone()),
    }
}

/// `items` drawn into `pixmap`, in order, what is cut through a mask or made in a buffer in `tools`;
/// `bounds` is how far the picture goes (a shadow is blurred inside it), which the pixmap may be more
/// than; `left` is how many columns of the picture are left of the pixmap's first (the words, which
/// are not moved to it, are put in it by it).
#[allow(clippy::too_many_arguments)]
fn draw<'a>(
    pixmap: PixmapMut<'_>,
    bounds: (f32, f32),
    tools: &mut Tools,
    items: impl Iterator<Item = &'a Paint>,
    fonts: &Fonts,
    images: Option<&Images>,
    palette: Palette,
    left: u32,
) {
    let mut canvas = Canvas {
        pixmap,
        fonts,
        palette,
        bounds,
        tools,
        left: i64::from(left),
    };
    for item in items {
        match item {
            Paint::Quad(quad) => canvas.quad(quad),
            Paint::Text(run) => canvas.text(run),
            Paint::Figure(figure) => {
                if let Some(bitmap) = draw_figure(figure, |color| palette.resolve(color)) {
                    canvas.bitmap(
                        bitmap.pixels(),
                        (bitmap.width, bitmap.height),
                        (figure.rect, 0.0),
                        Rect::new(0.0, 0.0, 1.0, 1.0),
                        (None, 1.0),
                        figure.clip,
                    );
                }
            }
            Paint::Image(picture) => {
                if let Some(bitmap) = images.and_then(|images| images.picture(picture)) {
                    canvas.bitmap(
                        bitmap.pixels(),
                        (bitmap.width, bitmap.height),
                        (picture.rect, picture.turn),
                        bitmap.crop_of(picture),
                        (picture.tint, picture.opacity),
                        picture.clip,
                    );
                }
            }
            // An editor's place no editor filled: its room's fill, under it, is what shows.
            Paint::Editor(_) => {}
            // A browser's room is where the host's engine draws; a picture has only its frame
            // (`Headless::shot_in` draws one), the engine being the host's.
            Paint::Browser(_) => {}
        }
    }
}

/// What a picture is drawn on, and with.
struct Canvas<'a, 'b> {
    pixmap: PixmapMut<'b>,
    fonts: &'a Fonts,
    palette: Palette,
    /// How wide and how high the picture is that is drawn: what a shadow is blurred inside.
    bounds: (f32, f32),
    tools: &'a mut Tools,
    /// How many columns of the picture are left of the pixmap's first, where the pixmap is a part of
    /// the picture. The words are in the picture's columns, and where a letter is is rounded in them
    /// (and a letter of the picture's column 100.5 is in 101 where the part starts at 40 and is in its
    /// 61): `blit` and `fill` take this off after they have rounded.
    left: i64,
}

/// Where two rectangles meet.
fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = (a.x + a.w).min(b.x + b.w);
    let bottom = (a.y + a.h).min(b.y + b.h);
    (right > x && bottom > y).then(|| Rect::new(x, y, right - x, bottom - y))
}

/// What `path` covers of a picture of `size`, from 0 to 1 a pixel, in `kept`; made in `under`, which
/// is cleared first.
fn coverage(
    under: &mut Vec<u8>,
    grown: &mut u32,
    kept: &mut Vec<f32>,
    size: (u32, u32),
    path: &tiny_skia::Path,
) -> Option<()> {
    let bytes = size.0 as usize * size.1 as usize * 4;
    if under.capacity() < bytes {
        *grown += 1;
    }
    under.clear();
    under.resize(bytes, 0);
    let mut view = PixmapMut::from_bytes(under.as_mut_slice(), size.0, size.1)?;
    let mut white = Ink::default();
    white.set_color(tiny_skia::Color::WHITE);
    white.anti_alias = true;
    view.fill_path(path, &white, FillRule::Winding, Transform::identity(), None);
    kept.clear();
    kept.extend(
        under
            .chunks(4)
            .map(|pixel| f32::from(pixel.get(3).copied().unwrap_or(0)) / 255.0),
    );
    Some(())
}

/// `rect` with its corners rounded by `radii` (top-left, top-right, bottom-right, bottom-left).
fn rounded(rect: Rect, radii: [f32; 4]) -> Option<tiny_skia::Path> {
    let limit = rect.w.min(rect.h) / 2.0;
    let [tl, tr, br, bl] = radii.map(|r| r.clamp(0.0, limit.max(0.0)));
    let (x, y, right, bottom) = (rect.x, rect.y, rect.x + rect.w, rect.y + rect.h);
    let mut path = PathBuilder::new();
    path.move_to(x + tl, y);
    path.line_to(right - tr, y);
    path.quad_to(right, y, right, y + tr);
    path.line_to(right, bottom - br);
    path.quad_to(right, bottom, right - br, bottom);
    path.line_to(x + bl, bottom);
    path.quad_to(x, bottom, x, bottom - bl);
    path.line_to(x, y + tl);
    path.quad_to(x, y, x + tl, y);
    path.close();
    path.finish()
}

fn ink([r, g, b, a]: [f32; 4]) -> Ink<'static> {
    let mut paint = Ink::default();
    if let Some(color) = tiny_skia::Color::from_rgba(
        r.clamp(0.0, 1.0),
        g.clamp(0.0, 1.0),
        b.clamp(0.0, 1.0),
        a.clamp(0.0, 1.0),
    ) {
        paint.set_color(color);
    }
    paint.anti_alias = true;
    paint
}

impl Canvas<'_, '_> {
    fn quad(&mut self, quad: &pencere_view::Quad) {
        // The shadow first: under the box, which it never shows through (and which a clip that
        // cuts the box away may leave the light of in).
        if let Some(shadow) = quad.shadow {
            self.shadow(quad, shadow);
        }
        let Some(shown) = intersect(quad.rect, quad.clip.rect) else {
            return;
        };
        let fill = quad
            .fill
            .or(quad.gradient.map(pencere_view::Gradient::middle));
        // The box's own corners, cut where its clip cuts it: a box a scrolled list half hides
        // keeps its corners where they are and is cut straight at the edge.
        let whole = shown == quad.rect && quad.clip.round.is_none() && quad.clip.fade.is_none();
        let size = (self.pixmap.width(), self.pixmap.height());
        if let (Some(fill), Some(path)) = (fill, rounded(quad.rect, quad.radii)) {
            let mask = if whole {
                None
            } else {
                self.tools.masked(size, quad.clip)
            };
            self.pixmap.fill_path(
                &path,
                &ink(self.palette.resolve(fill)),
                FillRule::Winding,
                Transform::identity(),
                mask,
            );
        }
        if let Some(border) = quad.border {
            let half = border.width / 2.0;
            let inner = Rect::new(
                quad.rect.x + half,
                quad.rect.y + half,
                quad.rect.w - border.width,
                quad.rect.h - border.width,
            );
            let mask = self.tools.masked(size, quad.clip);
            if let Some(path) = rounded(inner, quad.radii) {
                let stroke = Stroke {
                    width: border.width,
                    ..Stroke::default()
                };
                self.pixmap.stroke_path(
                    &path,
                    &ink(self.palette.resolve(border.color)),
                    &stroke,
                    Transform::identity(),
                    mask,
                );
            }
        }
    }

    /// The soft shadow of `quad`'s box: its silhouette moved by `(dx, dy)` and blurred (`blur` is
    /// the standard deviation's double, as the host's renderer takes it), what the box itself covers
    /// left out, cut at the box's clip.
    fn shadow(&mut self, quad: &pencere_view::Quad, shadow: pencere_view::Shadow) {
        let sigma = (shadow.blur * 0.5).max(0.5);
        let reach = (sigma * 3.0).ceil() + 1.0;
        let color = self.palette.resolve(shadow.color);
        let moved = Rect::new(
            quad.rect.x + shadow.dx,
            quad.rect.y + shadow.dy,
            quad.rect.w,
            quad.rect.h,
        );
        // The picture's pixels the shadow may reach, and the clip cuts: its work area.
        let (width, height) = self.bounds;
        let area = Rect::new(
            (moved.x - reach).min(quad.rect.x).floor(),
            (moved.y - reach).min(quad.rect.y).floor(),
            0.0,
            0.0,
        );
        let right = (moved.right() + reach).max(quad.rect.right()).ceil();
        let bottom = (moved.bottom() + reach).max(quad.rect.bottom()).ceil();
        let Some(work) = intersect(
            Rect::new(area.x, area.y, right - area.x, bottom - area.y),
            Rect::new(0.0, 0.0, width, height),
        )
        .and_then(|work| intersect(work, quad.clip.rect)) else {
            return;
        };
        let (x0, y0) = (work.x.floor(), work.y.floor());
        let (w, h) = (
            (work.right().ceil() - x0) as usize,
            (work.bottom().ceil() - y0) as usize,
        );
        // The silhouette's coverage, then blurred; what the box covers, to leave out.
        let (Ok(across), Ok(down)) = (u32::try_from(w), u32::try_from(h)) else {
            return;
        };
        let (Some(moved_path), Some(own_path)) = (
            rounded(
                Rect::new(
                    quad.rect.x + shadow.dx - x0,
                    quad.rect.y + shadow.dy - y0,
                    quad.rect.w,
                    quad.rect.h,
                ),
                quad.radii,
            ),
            rounded(
                Rect::new(quad.rect.x - x0, quad.rect.y - y0, quad.rect.w, quad.rect.h),
                quad.radii,
            ),
        ) else {
            return;
        };
        let tools = &mut *self.tools;
        for (path, kept) in [(&moved_path, &mut tools.cover), (&own_path, &mut tools.own)] {
            if coverage(
                &mut tools.under,
                &mut tools.grown,
                kept,
                (across, down),
                path,
            )
            .is_none()
            {
                return;
            }
        }
        tools.blur.coverage(&mut tools.cover, (w, h), sigma);
        let (cover, own) = (&tools.cover, &tools.own);
        let stride = self.pixmap.width() as usize;
        let pixels = self.pixmap.data_mut();
        for row in 0..h {
            for column in 0..w {
                let (px, py) = (x0 as usize + column, y0 as usize + row);
                let (cx, cy) = (px as f32 + 0.5, py as f32 + 0.5);
                if cx < quad.clip.rect.x
                    || cx >= quad.clip.rect.right()
                    || cy < quad.clip.rect.y
                    || cy >= quad.clip.rect.bottom()
                {
                    continue;
                }
                let at = row * w + column;
                #[allow(clippy::cast_possible_wrap)]
                let corner = clip_cover(quad.clip, px as i64, py as i64);
                let alpha = cover.get(at).copied().unwrap_or(0.0)
                    * (1.0 - own.get(at).copied().unwrap_or(0.0))
                    * corner
                    * color[3];
                if alpha <= 0.0 {
                    continue;
                }
                let Some(pixel) =
                    pixels.get_mut((py * stride + px) * 4..(py * stride + px) * 4 + 4)
                else {
                    continue;
                };
                for (channel, under) in pixel.iter_mut().take(3).enumerate() {
                    let over = color.get(channel).copied().unwrap_or(0.0) * 255.0;
                    let mixed = f32::from(*under) + (over - f32::from(*under)) * alpha;
                    *under = mixed.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }

    fn text(&mut self, run: &pencere_view::Run) {
        let [r, g, b, a] = self.palette.resolve(run.color);
        let color = [r, g, b, a * run.opacity.clamp(0.0, 1.0)];
        let (ascent, line) = self.fonts.line(&run.font);
        let baseline = run.origin.y + ascent;
        let mut pen = run.origin.x;
        match self.fonts.set_down(&run.font, &run.text) {
            Some(set) => {
                for mark in &set.marks {
                    let x = run.origin.x + mark.at;
                    match &mark.shape {
                        Shape::Nothing => {}
                        Shape::Glyph(glyph) => {
                            #[allow(clippy::cast_precision_loss)]
                            let at = (x + glyph.left as f32, baseline - glyph.top as f32);
                            if glyph.colour {
                                self.blit_colour(glyph, at, color[3], run.clip);
                            } else {
                                self.blit(&glyph.data, glyph.width as usize, at, color, run.clip);
                            }
                        }
                        Shape::Box => self.hollow_box(x, baseline, mark.advance, run, color),
                    }
                }
                pen += set.width;
            }
            None => {
                // A box for each character that is not a space, a character's width apart.
                let advance = self.fonts.box_advance();
                for c in run.text.chars() {
                    if !c.is_whitespace() {
                        let cell = Rect::new(
                            (pen + 1.0).round(),
                            (run.origin.y + line * 0.25).round(),
                            (advance - 2.0).round().max(1.0),
                            (line * 0.5).round().max(1.0),
                        );
                        self.fill(cell, run.clip, color);
                    }
                    pen += advance;
                }
            }
        }
        if run.underline || run.strike {
            let y = if run.underline {
                baseline + 2.0
            } else {
                baseline - ascent * 0.3
            };
            let rule = Rect::new(
                run.origin.x.round(),
                y.round(),
                (pen - run.origin.x).round(),
                1.0,
            );
            self.fill(rule, run.clip, color);
        }
    }

    /// What a character no face has is: the outline of a box on the baseline, the height of a capital,
    /// a little narrower than the room the character takes.
    fn hollow_box(
        &mut self,
        x: f32,
        baseline: f32,
        advance: f32,
        run: &pencere_view::Run,
        color: [f32; 4],
    ) {
        let (left, width) = (
            (x + advance * 0.1).round(),
            (advance * 0.8).round().max(3.0),
        );
        let height = (run.font.size * 0.72).round().max(3.0);
        let top = (baseline - height).round();
        let line = (run.font.size / 16.0).round().max(1.0);
        for rect in [
            Rect::new(left, top, width, line),
            Rect::new(left, top + height - line, width, line),
            Rect::new(left, top, line, height),
            Rect::new(left + width - line, top, line, height),
        ] {
            self.fill(rect, run.clip, color);
        }
    }

    /// A colour glyph's own pixels (straight RGBA) at `at`, `opacity` of them showing, cut at `clip`.
    fn blit_colour(&mut self, glyph: &fonts::Glyph, at: (f32, f32), opacity: f32, clip: Clip) {
        let (soft, clip) = (clip, clip.rect);
        #[allow(clippy::cast_possible_truncation)]
        let (x, y) = (at.0.round() as i64 - self.left, at.1.round() as i64);
        let (w, h) = (
            i64::from(self.pixmap.width()),
            i64::from(self.pixmap.height()),
        );
        let width = glyph.width as usize;
        let pixels = self.pixmap.data_mut();
        for (row, line) in glyph.data.chunks(width.max(1) * 4).enumerate() {
            let py = y + i64::try_from(row).unwrap_or(i64::MAX);
            #[allow(clippy::cast_precision_loss)]
            if py < 0 || py >= h || (py as f32) < clip.y || (py as f32) >= clip.y + clip.h {
                continue;
            }
            for (column, pixel) in line.chunks(4).enumerate() {
                let px = x + i64::try_from(column).unwrap_or(i64::MAX);
                #[allow(clippy::cast_precision_loss)]
                if px < 0 || px >= w || (px as f32) < clip.x || (px as f32) >= clip.x + clip.w {
                    continue;
                }
                let &[red, green, blue, alpha] = pixel else {
                    continue;
                };
                let alpha = f32::from(alpha) / 255.0 * opacity * clip_cover(soft, px, py);
                if alpha <= 0.0 {
                    continue;
                }
                let Ok(to) = usize::try_from((py * w + px) * 4) else {
                    continue;
                };
                let Some(under) = pixels.get_mut(to..to + 4) else {
                    continue;
                };
                for (channel, over) in under.iter_mut().zip([red, green, blue]) {
                    let mixed =
                        f32::from(*channel) + (f32::from(over) - f32::from(*channel)) * alpha;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let mixed = mixed.round().clamp(0.0, 255.0) as u8;
                    *channel = mixed;
                }
                if let Some(opaque) = under.get_mut(3) {
                    *opaque = 255;
                }
            }
        }
    }

    /// `rect`, cut at `clip`, filled with `color`.
    fn fill(&mut self, rect: Rect, clip: Clip, color: [f32; 4]) {
        // The rectangle is in the picture's columns, rounded there; the clip is in the pixmap's.
        #[allow(clippy::cast_precision_loss)]
        let rect = Rect::new(rect.x - self.left as f32, rect.y, rect.w, rect.h);
        let Some(shown) = intersect(rect, clip.rect) else {
            return;
        };
        if let Some(rect) = tiny_skia::Rect::from_xywh(shown.x, shown.y, shown.w, shown.h) {
            let mut paint = ink(color);
            paint.anti_alias = false;
            // Where the clip has round corners the fill goes through its mask.
            let size = (self.pixmap.width(), self.pixmap.height());
            let mask = if clip.round.is_some() || clip.fade.is_some() {
                self.tools.masked(size, clip)
            } else {
                None
            };
            self.pixmap
                .fill_rect(rect, &paint, Transform::identity(), mask);
        }
    }

    /// A glyph's coverage (one byte a pixel, `width` wide) at `at`, in `color`, cut at `clip`.
    fn blit(&mut self, coverage: &[u8], width: usize, at: (f32, f32), color: [f32; 4], clip: Clip) {
        let (soft, clip) = (clip, clip.rect);
        #[allow(clippy::cast_possible_truncation)]
        let (x, y) = (at.0.round() as i64 - self.left, at.1.round() as i64);
        let (w, h) = (
            i64::from(self.pixmap.width()),
            i64::from(self.pixmap.height()),
        );
        let pixels = self.pixmap.data_mut();
        for (row, line) in coverage.chunks(width.max(1)).enumerate() {
            let py = y + i64::try_from(row).unwrap_or(i64::MAX);
            #[allow(clippy::cast_precision_loss)]
            let inside_y =
                py >= 0 && py < h && (py as f32) >= clip.y && (py as f32) < clip.y + clip.h;
            if !inside_y {
                continue;
            }
            for (column, value) in line.iter().enumerate() {
                let px = x + i64::try_from(column).unwrap_or(i64::MAX);
                #[allow(clippy::cast_precision_loss)]
                let inside_x =
                    px >= 0 && px < w && (px as f32) >= clip.x && (px as f32) < clip.x + clip.w;
                if !inside_x || *value == 0 {
                    continue;
                }
                let alpha = f32::from(*value) / 255.0
                    * color.get(3).copied().unwrap_or(1.0)
                    * clip_cover(soft, px, py);
                let Ok(at) = usize::try_from((py * w + px) * 4) else {
                    continue;
                };
                let Some(pixel) = pixels.get_mut(at..at + 4) else {
                    continue;
                };
                // Over what is there, which is opaque: the ground was filled first.
                for (channel, under) in pixel.iter_mut().take(3).enumerate() {
                    let over = color.get(channel).copied().unwrap_or(0.0) * 255.0;
                    let mixed = f32::from(*under) + (over - f32::from(*under)) * alpha;
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let mixed = mixed.round().clamp(0.0, 255.0) as u8;
                    *under = mixed;
                }
                if let Some(opaque) = pixel.get_mut(3) {
                    *opaque = 255;
                }
            }
        }
    }

    /// Premultiplied RGBA `pixels` of `size`, the part `crop` says (shares of it) drawn into
    /// `rect`, turned `turn` degrees clockwise about its middle, in `tint` where there is one,
    /// `opacity` of it showing, cut at `clip`.
    fn bitmap(
        &mut self,
        pixels: &[u8],
        (width, height): (u32, u32),
        (rect, turn): (Rect, f32),
        crop: Rect,
        (tint, opacity): (Option<Color>, f32),
        clip: Clip,
    ) {
        let tinted;
        let pixels = match tint {
            Some(tint) => {
                let [r, g, b, a] = self.palette.resolve(tint);
                tinted = pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|&[_, _, _, alpha]| {
                        let coverage = f32::from(alpha) / 255.0 * a;
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let byte =
                            |value: f32| (value * coverage * 255.0).round().clamp(0.0, 255.0) as u8;
                        [byte(r), byte(g), byte(b), byte(1.0)]
                    })
                    .collect::<Vec<u8>>();
                tinted.as_slice()
            }
            None => pixels,
        };
        let Some(source) = PixmapRef::from_bytes(pixels, width, height) else {
            return;
        };
        #[allow(clippy::cast_precision_loss)]
        let (whole_w, whole_h) = (
            width as f32 * crop.w.max(f32::EPSILON),
            height as f32 * crop.h.max(f32::EPSILON),
        );
        let (sx, sy) = (rect.w / whole_w, rect.h / whole_h);
        #[allow(clippy::cast_precision_loss)]
        let (cx, cy) = (crop.x * width as f32, crop.y * height as f32);
        let transform = Transform::from_row(sx, 0.0, 0.0, sy, rect.x - cx * sx, rect.y - cy * sy);
        if turn != 0.0 {
            // Turned about the middle of its box, and cut at the box turned with it.
            let turned =
                Transform::from_rotate_at(turn, rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
            // Nothing shows where the clip or the box is no room at all.
            let size = (self.pixmap.width(), self.pixmap.height());
            let (Some(mut mask), Some(room)) = (
                fresh_mask(size, clip),
                tiny_skia::Rect::from_xywh(rect.x, rect.y, rect.w, rect.h),
            ) else {
                return;
            };
            mask.intersect_path(
                &PathBuilder::from_rect(room),
                FillRule::Winding,
                true,
                turned,
            );
            let paint = PixmapPaint {
                opacity: opacity.clamp(0.0, 1.0),
                ..PixmapPaint::default()
            };
            self.pixmap.draw_pixmap(
                0,
                0,
                source,
                &paint,
                transform.post_concat(turned),
                Some(&mask),
            );
            return;
        }
        let Some(shown) = intersect(rect, clip.rect) else {
            return;
        };
        let size = (self.pixmap.width(), self.pixmap.height());
        let mask = self.tools.masked(
            size,
            Clip {
                rect: shown,
                ..clip
            },
        );
        let paint = PixmapPaint {
            opacity: opacity.clamp(0.0, 1.0),
            ..PixmapPaint::default()
        };
        self.pixmap
            .draw_pixmap(0, 0, source, &paint, transform, mask);
    }
}
