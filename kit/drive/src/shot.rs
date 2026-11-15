//! Pictures of a page, for the eye: a frame's draw list drawn into a PNG in the fonts the caller
//! gives and a theme of the kit's roles — the boxes, the words, the colours, the pictures and the
//! editors where the kit put them, which is what an eye checks (what is aligned, what overlaps, what
//! is cut). Not a terminal's drawing: no shaping, and a soft shadow made as three passes of a box
//! blur (a good Gaussian), which is what the eye needs of one.
//!
//! **The fonts are the caller's.** Where it gives none, each character is drawn as a box a character
//! wide ([`Fonts::none`]): a picture made on any machine, with no font of its system, is the same
//! picture, byte for byte. Where it gives faces ([`Fonts::installed`]: the machine's, with a face
//! for a character one lacks; [`Fonts::carried`]: the ones the program carries), the page is to be
//! laid out with the same [`Fonts`] (they are a `Measure`), so that its words fill the boxes they were
//! measured for.

use pencere_sandbox::{Images, draw_figure};
use pencere_view::{Color, DrawList, Paint, Rect, Role, Source};
use tiny_skia::{
    FillRule, Mask, Paint as Ink, PathBuilder, Pixmap, PixmapPaint, PixmapRef, Stroke, Transform,
};

mod find;
mod fonts;

use fonts::Shape;
pub use fonts::{FontError, Fonts};

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
    /// The PNG could not be written.
    Encode(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Size(w, h) => write!(f, "no picture is {w} × {h} pixels"),
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
    ground(&mut pixmap, palette);
    draw(&mut pixmap, list.items.iter(), fonts, images, palette);
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
    let (width, height) = size;
    let bottom = top.saturating_add(rows).min(height);
    if bottom <= top {
        return Err(Error::Size(width, rows));
    }
    #[allow(clippy::cast_precision_loss)]
    let (above, below) = (top as f32, bottom as f32);
    let reaching: Vec<(&Paint, (f32, f32))> = list
        .items
        .iter()
        .filter_map(|item| reach(item, fonts).map(|rows| (item, rows)))
        .filter(|(_, (first, last))| *first < below && *last > above)
        .collect();
    // A shadow is blurred over the rows it reaches, cut at its clip, as a grid of them: the picture
    // has them all, so that it is blurred over the same grid as in the whole picture.
    let (mut from, mut to) = (top, bottom);
    for (item, (first, last)) in &reaching {
        if matches!(item, Paint::Quad(quad) if quad.shadow.is_some()) {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                from = from.min(first.floor().max(0.0) as u32);
                to = to.max((last.ceil().max(0.0) as u32).min(height));
            }
        }
    }
    let mut pixmap = Pixmap::new(width, to - from).ok_or(Error::Size(width, to - from))?;
    ground(&mut pixmap, palette);
    #[allow(clippy::cast_precision_loss)]
    let moved: Vec<Paint> = reaching
        .iter()
        .map(|(item, _)| shifted(item, -(from as f32)))
        .collect();
    draw(&mut pixmap, moved.iter(), fonts, images, palette);
    let rect = tiny_skia::IntRect::from_xywh(
        0,
        i32::try_from(top - from).unwrap_or(0),
        width,
        bottom - top,
    )
    .ok_or(Error::Size(width, rows))?;
    pixmap
        .clone_rect(rect)
        .map(|pixmap| Raster { pixmap })
        .ok_or(Error::Size(width, rows))
}

/// The ground of `palette` under a picture.
fn ground(pixmap: &mut Pixmap, palette: Palette) {
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
            (top - 1.0, bottom + 1.0, quad.clip)
        }
        Paint::Text(run) => {
            let (_, line) = fonts.line(&run.font);
            let line = line.max(run.font.size);
            (
                run.origin.y - line * 0.25 - 1.0,
                run.origin.y + line * 1.25 + 1.0,
                run.clip,
            )
        }
        Paint::Image(picture) => {
            let rect = picture.rect;
            if picture.turn == 0.0 {
                (rect.y - 1.0, rect.bottom() + 1.0, picture.clip)
            } else {
                // Turned about its middle: as far as its corners go.
                let half = rect.w.hypot(rect.h) / 2.0;
                let middle = rect.y + rect.h / 2.0;
                (middle - half - 1.0, middle + half + 1.0, picture.clip)
            }
        }
        Paint::Figure(figure) => (figure.rect.y - 1.0, figure.rect.bottom() + 1.0, figure.clip),
        Paint::Editor(_) => return None,
    };
    let (top, bottom) = (top.max(clip.y), bottom.min(clip.bottom()));
    (bottom > top).then_some((top, bottom))
}

/// `item` moved down by `dy` pixels.
fn shifted(item: &Paint, dy: f32) -> Paint {
    let down = |r: Rect| Rect::new(r.x, r.y + dy, r.w, r.h);
    match item {
        Paint::Quad(quad) => {
            let mut quad = quad.clone();
            quad.rect = down(quad.rect);
            quad.clip = down(quad.clip);
            quad.exact = quad.exact.map(down);
            Paint::Quad(quad)
        }
        Paint::Text(run) => {
            let mut run = run.clone();
            run.origin.y += dy;
            run.clip = down(run.clip);
            Paint::Text(run)
        }
        Paint::Image(picture) => {
            let mut picture = picture.clone();
            picture.rect = down(picture.rect);
            picture.cells = down(picture.cells);
            picture.clip = down(picture.clip);
            picture.exact = picture.exact.map(down);
            Paint::Image(picture)
        }
        Paint::Figure(figure) => {
            let mut figure = figure.clone();
            figure.rect = down(figure.rect);
            figure.clip = down(figure.clip);
            Paint::Figure(figure)
        }
        Paint::Editor(slot) => Paint::Editor(slot.clone()),
    }
}

/// `items` drawn into `pixmap`, in order.
fn draw<'a>(
    pixmap: &mut Pixmap,
    items: impl Iterator<Item = &'a Paint>,
    fonts: &Fonts,
    images: Option<&Images>,
    palette: Palette,
) {
    let mut canvas = Canvas {
        pixmap,
        fonts,
        palette,
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
        }
    }
}

/// What a picture is drawn on, and with.
struct Canvas<'a> {
    pixmap: &'a mut Pixmap,
    fonts: &'a Fonts,
    palette: Palette,
}

/// Where two rectangles meet.
fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = (a.x + a.w).min(b.x + b.w);
    let bottom = (a.y + a.h).min(b.y + b.h);
    (right > x && bottom > y).then(|| Rect::new(x, y, right - x, bottom - y))
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

/// One pass of a box blur across and down a grid of `size` (width, height), the box as wide as three
/// passes of it make a Gaussian of deviation `sigma` (the width of the classic "boxes for a
/// Gaussian": `sqrt(12 sigma^2 / 3 + 1)`, odd).
fn blur(grid: &mut [f32], (w, h): (usize, usize), sigma: f32) {
    let wide = ((12.0 * sigma * sigma / 3.0 + 1.0).sqrt().floor() as usize) | 1;
    let radius = wide / 2;
    let mut line = vec![0.0_f32; w.max(h)];
    for row in 0..h {
        for (column, slot) in line.iter_mut().take(w).enumerate() {
            *slot = grid.get(row * w + column).copied().unwrap_or(0.0);
        }
        smooth(&mut line, w, radius);
        for (column, value) in line.iter().take(w).enumerate() {
            if let Some(cell) = grid.get_mut(row * w + column) {
                *cell = *value;
            }
        }
    }
    for column in 0..w {
        for (row, slot) in line.iter_mut().take(h).enumerate() {
            *slot = grid.get(row * w + column).copied().unwrap_or(0.0);
        }
        smooth(&mut line, h, radius);
        for (row, value) in line.iter().take(h).enumerate() {
            if let Some(cell) = grid.get_mut(row * w + column) {
                *cell = *value;
            }
        }
    }
}

/// The first `n` of `line`, each the mean of the ones `radius` either side (what is past an end is
/// nothing).
fn smooth(line: &mut [f32], n: usize, radius: usize) {
    let source: Vec<f32> = line.iter().take(n).copied().collect();
    let scale = 1.0 / (2 * radius + 1) as f32;
    let mut sum: f32 = source.iter().take(radius + 1).sum();
    for at in 0..n {
        if let Some(out) = line.get_mut(at) {
            *out = sum * scale;
        }
        if let Some(gone) = at.checked_sub(radius).and_then(|i| source.get(i)) {
            sum -= gone;
        }
        if let Some(came) = source.get(at + radius + 1) {
            sum += came;
        }
    }
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

impl Canvas<'_> {
    fn quad(&mut self, quad: &pencere_view::Quad) {
        // The shadow first: under the box, which it never shows through (and which a clip that
        // cuts the box away may leave the light of in).
        if let Some(shadow) = quad.shadow {
            self.shadow(quad, shadow);
        }
        let Some(shown) = intersect(quad.rect, quad.clip) else {
            return;
        };
        let fill = quad
            .fill
            .or(quad.gradient.map(pencere_view::Gradient::middle));
        // The box's own corners, cut where its clip cuts it: a box a scrolled list half hides
        // keeps its corners where they are and is cut straight at the edge.
        let whole = shown == quad.rect;
        if let (Some(fill), Some(path)) = (fill, rounded(quad.rect, quad.radii)) {
            let mask = if whole { None } else { self.mask(quad.clip) };
            self.pixmap.fill_path(
                &path,
                &ink(self.palette.resolve(fill)),
                FillRule::Winding,
                Transform::identity(),
                mask.as_ref(),
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
            let mask = self.mask(quad.clip);
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
                    mask.as_ref(),
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
        let (width, height) = (self.pixmap.width() as f32, self.pixmap.height() as f32);
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
        .and_then(|work| intersect(work, quad.clip)) else {
            return;
        };
        let (x0, y0) = (work.x.floor(), work.y.floor());
        let (w, h) = (
            (work.right().ceil() - x0) as usize,
            (work.bottom().ceil() - y0) as usize,
        );
        // The silhouette's coverage, then blurred; what the box covers, to leave out.
        let paint = |rect: Rect, offset: (f32, f32)| -> Option<Vec<f32>> {
            let mut under = Pixmap::new(u32::try_from(w).ok()?, u32::try_from(h).ok()?)?;
            let path = rounded(
                Rect::new(
                    rect.x + offset.0 - x0,
                    rect.y + offset.1 - y0,
                    rect.w,
                    rect.h,
                ),
                quad.radii,
            )?;
            let mut white = Ink::default();
            white.set_color(tiny_skia::Color::WHITE);
            white.anti_alias = true;
            under.fill_path(
                &path,
                &white,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
            Some(
                under
                    .data()
                    .chunks(4)
                    .map(|pixel| f32::from(pixel.get(3).copied().unwrap_or(0)) / 255.0)
                    .collect(),
            )
        };
        let (Some(mut cover), Some(own)) = (
            paint(quad.rect, (shadow.dx, shadow.dy)),
            paint(quad.rect, (0.0, 0.0)),
        ) else {
            return;
        };
        for _ in 0..3 {
            blur(&mut cover, (w, h), sigma);
        }
        let pixels = self.pixmap.data_mut();
        let stride = width as usize;
        for row in 0..h {
            for column in 0..w {
                let (px, py) = (x0 as usize + column, y0 as usize + row);
                let (cx, cy) = (px as f32 + 0.5, py as f32 + 0.5);
                if cx < quad.clip.x
                    || cx >= quad.clip.right()
                    || cy < quad.clip.y
                    || cy >= quad.clip.bottom()
                {
                    continue;
                }
                let at = row * w + column;
                let alpha = cover.get(at).copied().unwrap_or(0.0)
                    * (1.0 - own.get(at).copied().unwrap_or(0.0))
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

    /// A mask of `clip`: what is drawn through it is cut there.
    fn mask(&self, clip: Rect) -> Option<Mask> {
        let mut mask = Mask::new(self.pixmap.width(), self.pixmap.height())?;
        let rect = tiny_skia::Rect::from_xywh(clip.x, clip.y, clip.w, clip.h)?;
        mask.fill_path(
            &PathBuilder::from_rect(rect),
            FillRule::Winding,
            false,
            Transform::identity(),
        );
        Some(mask)
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
    fn blit_colour(&mut self, glyph: &fonts::Glyph, at: (f32, f32), opacity: f32, clip: Rect) {
        #[allow(clippy::cast_possible_truncation)]
        let (x, y) = (at.0.round() as i64, at.1.round() as i64);
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
                let alpha = f32::from(alpha) / 255.0 * opacity;
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
    fn fill(&mut self, rect: Rect, clip: Rect, color: [f32; 4]) {
        let Some(shown) = intersect(rect, clip) else {
            return;
        };
        if let Some(rect) = tiny_skia::Rect::from_xywh(shown.x, shown.y, shown.w, shown.h) {
            let mut paint = ink(color);
            paint.anti_alias = false;
            self.pixmap
                .fill_rect(rect, &paint, Transform::identity(), None);
        }
    }

    /// A glyph's coverage (one byte a pixel, `width` wide) at `at`, in `color`, cut at `clip`.
    fn blit(&mut self, coverage: &[u8], width: usize, at: (f32, f32), color: [f32; 4], clip: Rect) {
        #[allow(clippy::cast_possible_truncation)]
        let (x, y) = (at.0.round() as i64, at.1.round() as i64);
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
                let alpha = f32::from(*value) / 255.0 * color.get(3).copied().unwrap_or(1.0);
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
        clip: Rect,
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
            let (Some(mut mask), Some(room)) = (
                self.mask(clip),
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
        let Some(shown) = intersect(rect, clip) else {
            return;
        };
        let mask = self.mask(shown);
        let paint = PixmapPaint {
            opacity: opacity.clamp(0.0, 1.0),
            ..PixmapPaint::default()
        };
        self.pixmap
            .draw_pixmap(0, 0, source, &paint, transform, mask.as_ref());
    }
}
