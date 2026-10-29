//! Pictures of a page, for the eye: a frame's draw list drawn into a PNG in the fonts the caller
//! gives and a theme of the kit's roles — the boxes, the words, the colours, the pictures and the
//! editors where the kit put them, which is what an eye checks (what is aligned, what overlaps, what
//! is cut). Not a terminal's drawing: no shaping, one face of each family, and a soft shadow made as
//! three passes of a box blur (a good Gaussian), which is what the eye needs of one.
//!
//! **The fonts are the caller's.** Where it gives none, each character is drawn as a box a character
//! wide ([`Fonts::none`]): a picture made on any machine, with no font of its system, is the same
//! picture, byte for byte. Where it gives faces, the page is to be laid out with the same
//! [`Fonts`] (they are a [`Measure`]), so that its words fill the boxes they were measured for.

use fontdue::{Font as Face, FontSettings};
use pencere_sandbox::{Images, draw_figure};
use pencere_view::{Color, DrawList, Family, Font, Measure, Metrics, Paint, Rect, Role, Source};
use tiny_skia::{
    FillRule, Mask, Paint as Ink, PathBuilder, Pixmap, PixmapPaint, PixmapRef, Stroke, Transform,
};

use crate::headless::Fixed;

/// The faces a picture's words are drawn in, and what measures them: the terminal's family
/// (regular, bold, italic) and the system's. A family with no face falls back on the other; with no
/// face at all, every character is a box the [`Fixed`] measure's width.
#[derive(Clone)]
pub struct Fonts {
    terminal: Option<Face>,
    bold: Option<Face>,
    italic: Option<Face>,
    system: Option<Face>,
    fixed: Fixed,
}

/// A font the caller gave that is not one: why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontError(pub String);

impl std::fmt::Display for FontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the font is not one: {}", self.0)
    }
}

impl std::error::Error for FontError {}

/// The face of the font file in `bytes` (the first of a collection).
fn face(bytes: &[u8]) -> Result<Face, FontError> {
    Face::from_bytes(bytes, FontSettings::default()).map_err(|why| FontError(why.to_owned()))
}

impl Fonts {
    /// No faces: every character a box as wide as [`Fixed`]'s default (what the host with no screen
    /// measures with unless told).
    pub fn none() -> Self {
        Self::boxes(Fixed::default())
    }

    /// No faces: every character a box as wide as `fixed` measures it.
    pub fn boxes(fixed: Fixed) -> Self {
        Self {
            terminal: None,
            bold: None,
            italic: None,
            system: None,
            fixed,
        }
    }

    /// The terminal's face, regular: the file's bytes.
    ///
    /// # Errors
    ///
    /// The bytes are not a font.
    pub fn terminal(mut self, bytes: &[u8]) -> Result<Self, FontError> {
        self.terminal = Some(face(bytes)?);
        Ok(self)
    }

    /// The terminal's face, bold.
    ///
    /// # Errors
    ///
    /// The bytes are not a font.
    pub fn bold(mut self, bytes: &[u8]) -> Result<Self, FontError> {
        self.bold = Some(face(bytes)?);
        Ok(self)
    }

    /// The terminal's face, italic.
    ///
    /// # Errors
    ///
    /// The bytes are not a font.
    pub fn italic(mut self, bytes: &[u8]) -> Result<Self, FontError> {
        self.italic = Some(face(bytes)?);
        Ok(self)
    }

    /// The system's face.
    ///
    /// # Errors
    ///
    /// The bytes are not a font.
    pub fn system(mut self, bytes: &[u8]) -> Result<Self, FontError> {
        self.system = Some(face(bytes)?);
        Ok(self)
    }

    /// The face `font` is drawn in, if there is one.
    fn face(&self, font: &Font) -> Option<&Face> {
        let regular = || self.terminal.as_ref().or(self.system.as_ref());
        match font.family {
            Family::Terminal if font.weight >= 600 => self.bold.as_ref().or_else(regular),
            Family::Terminal if font.italic => self.italic.as_ref().or_else(regular),
            Family::Terminal => regular(),
            Family::System => self.system.as_ref().or(self.terminal.as_ref()),
        }
    }

    /// Where a line of `font` has its baseline, below its top, and how high it is, pixels.
    fn line(&self, font: &Font) -> (f32, f32) {
        match self.face(font) {
            Some(face) => face
                .horizontal_line_metrics(font.size)
                .map_or((font.size * 0.8, font.size * 1.25), |line| {
                    (line.ascent, line.ascent - line.descent + line.line_gap)
                }),
            None => (self.fixed.line * 0.8, self.fixed.line),
        }
    }
}

impl Measure for Fonts {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        match self.face(font) {
            Some(face) => Metrics {
                width: text
                    .chars()
                    .map(|c| face.metrics(c, font.size).advance_width)
                    .sum(),
                height: self.line(font).1,
            },
            None => self.fixed.measure(font, text),
        }
    }
}

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
    (width, height): (u32, u32),
    fonts: &Fonts,
    images: Option<&Images>,
    palette: Palette,
) -> Result<Vec<u8>, Error> {
    let mut pixmap = Pixmap::new(width, height).ok_or(Error::Size(width, height))?;
    let [r, g, b, _] = palette.role(Role::Background);
    if let Some(ground) = tiny_skia::Color::from_rgba(r, g, b, 1.0) {
        pixmap.fill(ground);
    }
    let mut canvas = Canvas {
        pixmap: &mut pixmap,
        fonts,
        palette,
    };
    for item in &list.items {
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
                if let Some(bitmap) =
                    images.and_then(|images| images.bitmap(&picture.src, picture.full()))
                {
                    canvas.bitmap(
                        bitmap.pixels(),
                        (bitmap.width, bitmap.height),
                        (picture.rect, picture.turn),
                        picture.crop,
                        (picture.tint, picture.opacity),
                        picture.clip,
                    );
                }
            }
            // An editor's place no editor filled: its room's fill, under it, is what shows.
            Paint::Editor(_) => {}
        }
    }
    pixmap
        .encode_png()
        .map_err(|why| Error::Encode(why.to_string()))
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
        match self.fonts.face(&run.font) {
            Some(face) => {
                for c in run.text.chars() {
                    let (metrics, coverage) = face.rasterize(c, run.font.size);
                    #[allow(clippy::cast_precision_loss)]
                    let (xmin, top) = (
                        metrics.xmin as f32,
                        metrics.height as f32 + metrics.ymin as f32,
                    );
                    self.blit(
                        &coverage,
                        metrics.width,
                        (pen + xmin, baseline - top),
                        color,
                        run.clip,
                    );
                    pen += metrics.advance_width;
                }
            }
            None => {
                // A box for each character that is not a space, a character's width apart.
                let advance = self.fonts.fixed.advance;
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
                    .chunks_exact(4)
                    .flat_map(|pixel| {
                        let coverage = f32::from(pixel.get(3).copied().unwrap_or(0)) / 255.0 * a;
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
