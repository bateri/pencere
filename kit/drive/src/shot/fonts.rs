//! The faces a picture's words are drawn in, and what measures them: the terminal's family and the
//! system's, each with its upright, bold and italic faces (a variable face is set to the weight a
//! run asks for), and, for a character neither has, the first face of the machine that has it
//! (symbols, the scripts of East Asia, a colour emoji). A character no face has is a hollow box.
//!
//! One resolver, `(font, character) → (face, glyph)`, serves the measure and the drawing, so that a
//! page laid out with [`Fonts`] fills the boxes it measured, and the glyph images (an outline's
//! coverage, or a colour glyph's pixels) are kept by face, glyph, size and weight.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use pencere_view::{Family, Font, Measure, Metrics};
use swash::proxy::CharmapProxy;
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source as Kind, StrikeWith};
use swash::zeno::Format;
use swash::{CacheKey, FontRef, tag_from_bytes};

use super::find::{Files, Platform, find};
use crate::headless::Fixed;

/// The faces the program carries, for a machine with none of its own, packed with zlib by the build
/// (`build.rs`) and unpacked, once, where a face is first drawn in: JetBrains Mono for the terminal's
/// family and Inter for the system's (both SIL OFL 1.1; the variable faces, upright and italic,
/// whose weight axis serves every weight, and Inter's optical size the size of the text).
const MONO_UPRIGHT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/JetBrainsMono[wght].ttf.z"));
const MONO_ITALIC: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/JetBrainsMono-Italic[wght].ttf.z"
));
const SANS_UPRIGHT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/InterVariable.ttf.z"));
const SANS_ITALIC: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/InterVariable-Italic.ttf.z"));

/// Where a face's bytes are.
#[derive(Clone)]
pub(super) enum Origin {
    File(PathBuf),
    Memory(Arc<[u8]>),
    /// Carried by the program, packed (zlib).
    Packed(&'static [u8]),
}

/// A face: a file, and which of its collection.
#[derive(Clone)]
pub(super) struct Src {
    pub origin: Origin,
    pub index: usize,
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum SrcKey {
    File(PathBuf, usize),
    Memory(usize, usize, usize),
}

impl Src {
    fn key(&self) -> SrcKey {
        match &self.origin {
            Origin::File(path) => SrcKey::File(path.clone(), self.index),
            Origin::Memory(bytes) => {
                SrcKey::Memory(bytes.as_ptr() as usize, bytes.len(), self.index)
            }
            Origin::Packed(bytes) => {
                SrcKey::Memory(bytes.as_ptr() as usize, bytes.len(), self.index)
            }
        }
    }
}

/// A family's faces by style; a style with none is the upright face (set to the weight, where it
/// is a variable face).
#[derive(Clone, Default)]
pub(super) struct Slots {
    pub upright: Option<Src>,
    pub bold: Option<Src>,
    pub italic: Option<Src>,
    pub bold_italic: Option<Src>,
}

impl Slots {
    fn pick(&self, weight: u16, italic: bool) -> Option<&Src> {
        let bold = weight >= 600;
        let chosen = match (bold, italic) {
            (true, true) => self
                .bold_italic
                .as_ref()
                .or(self.italic.as_ref())
                .or(self.bold.as_ref()),
            (true, false) => self.bold.as_ref(),
            (false, true) => self.italic.as_ref(),
            (false, false) => None,
        };
        chosen.or(self.upright.as_ref())
    }

    fn put(&mut self, style: Style, src: Src) {
        match style {
            Style::Upright => self.upright = Some(src),
            Style::Bold => self.bold = Some(src),
            Style::Italic => self.italic = Some(src),
        }
    }
}

#[derive(Clone, Copy)]
enum Style {
    Upright,
    Bold,
    Italic,
}

/// Which faces make a set of fonts: its two families, and where a character they lack is looked for.
#[derive(Clone, Default)]
pub(super) struct Layout {
    pub terminal: Slots,
    pub system: Slots,
    /// Faces to look a missing character in, in order (symbols, East Asia).
    pub named: Vec<Src>,
    /// Colour faces: tried first for a character of the emoji planes, last otherwise.
    pub emoji: Vec<Src>,
    /// Every other face of the machine, the last to be looked in.
    pub scan: Vec<Src>,
}

impl Layout {
    /// The families of `family` in the order they are looked in: its own, then the other's.
    fn chain(&self, family: Family) -> [&Slots; 2] {
        match family {
            Family::Terminal => [&self.terminal, &self.system],
            Family::System => [&self.system, &self.terminal],
        }
    }
}

/// How a run is asked for: what picks its face.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Pick {
    family: Family,
    weight: u16,
    italic: bool,
}

impl Pick {
    fn of(font: &Font) -> Self {
        Self {
            family: font.family,
            weight: font.weight.clamp(1, 1000),
            italic: font.italic,
        }
    }
}

enum Bytes {
    Map(memmap2::Mmap),
    Heap(Arc<[u8]>),
}

impl Bytes {
    fn get(&self) -> &[u8] {
        match self {
            Self::Map(map) => map,
            Self::Heap(bytes) => bytes,
        }
    }
}

/// A face opened: its bytes, and what is asked of it more than once.
struct Face {
    bytes: Bytes,
    offset: u32,
    key: CacheKey,
    charmap: CharmapProxy,
    upem: f32,
    /// Above the baseline, below it and between lines, in ems.
    line: (f32, f32, f32),
    /// The weights its variable axis has (none: a static face).
    weight: Option<(f32, f32)>,
    /// The optical sizes, in points, its variable axis has (none: no such axis).
    optical: Option<(f32, f32)>,
    /// Whether it carries colour glyphs (outline layers or bitmaps).
    colour: bool,
}

impl Face {
    fn open(src: &Src) -> Option<Self> {
        let bytes = match &src.origin {
            Origin::File(path) => {
                let file = std::fs::File::open(path).ok()?;
                // SAFETY: a font file of the system is read, never written, by anything that holds
                // it open; a file changed under the map is a face no picture of this one's is
                // drawn in (the same bargain every program that maps a font makes).
                #[allow(unsafe_code)]
                let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
                Bytes::Map(map)
            }
            Origin::Memory(bytes) => Bytes::Heap(Arc::clone(bytes)),
            Origin::Packed(packed) => Bytes::Heap(Arc::from(
                miniz_oxide::inflate::decompress_to_vec_zlib(packed).ok()?,
            )),
        };
        let font = FontRef::from_index(bytes.get(), src.index)?;
        let upem = f32::from(font.metrics(&[]).units_per_em).max(1.0);
        let metrics = font.metrics(&[]);
        let weight = font
            .variations()
            .find_by_tag(tag_from_bytes(b"wght"))
            .map(|axis| (axis.min_value(), axis.max_value()));
        let optical = font
            .variations()
            .find_by_tag(tag_from_bytes(b"opsz"))
            .map(|axis| (axis.min_value(), axis.max_value()));
        let colour = [b"sbix", b"CBDT", b"COLR"]
            .iter()
            .any(|table| font.table(tag_from_bytes(table)).is_some());
        let (offset, key, charmap) = (font.offset, font.key, CharmapProxy::from_font(&font));
        Some(Self {
            line: (
                metrics.ascent / upem,
                metrics.descent / upem,
                metrics.leading / upem,
            ),
            bytes,
            offset,
            key,
            charmap,
            upem,
            weight,
            optical,
            colour,
        })
    }

    fn font(&self) -> FontRef<'_> {
        FontRef {
            data: self.bytes.get(),
            offset: self.offset,
            key: self.key,
        }
    }

    fn glyph_of(&self, c: char) -> u16 {
        self.charmap.materialize(&self.font()).map(c)
    }

    /// What a run of `weight` and `size` pixels, drawn at `scale` pixels to the point, sets the axes of
    /// this face to: the weight asked for, and the size of the text in points, each as far as the
    /// axis goes.
    fn settings(&self, weight: u16, size: f32, scale: f32) -> Settings {
        let weight = self.weight.map_or(0, |(least, most)| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let set = f32::from(weight).clamp(least, most).round() as u16;
            set.max(1)
        });
        let optical = self.optical.map_or(0, |(least, most)| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let set = ((size / scale).clamp(least, most) * 10.0).round() as u16;
            set.max(1)
        });
        Settings { weight, optical }
    }

    /// How far the pen moves after `glyph` set so, in ems.
    fn advance_em(&self, glyph: u16, settings: Settings) -> f32 {
        let font = self.font();
        let coords: Vec<i16> = font
            .variations()
            .normalized_coords(settings.axes())
            .collect();
        font.glyph_metrics(&coords).advance_width(glyph) / self.upem
    }
}

/// What a run sets the axes of a variable face to: the weight (`0`: no such axis) and the optical
/// size in tenths of a point (`0`: none).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Settings {
    weight: u16,
    optical: u16,
}

impl Settings {
    fn axes(self) -> Vec<(&'static str, f32)> {
        let mut axes = Vec::new();
        if self.weight != 0 {
            axes.push(("wght", f32::from(self.weight)));
        }
        if self.optical != 0 {
            axes.push(("opsz", f32::from(self.optical) / 10.0));
        }
        axes
    }
}

/// A glyph drawn: its image and where it goes, from the pen on the baseline (pixels).
pub(super) struct Glyph {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    /// Opaque-or-not RGBA of its own colours (`true`), or the coverage of the text's colour.
    pub colour: bool,
    pub data: Vec<u8>,
}

/// What a character is drawn as.
pub(super) enum Shape {
    Nothing,
    /// The hollow box of a character no face has.
    Box,
    Glyph(Arc<Glyph>),
}

/// A character set down: how far from the run's start, and how far it moves the pen.
pub(super) struct Mark {
    pub at: f32,
    pub advance: f32,
    pub shape: Shape,
}

/// A run set down.
pub(super) struct Line {
    pub width: f32,
    pub marks: Vec<Mark>,
}

#[derive(Default)]
struct State {
    faces: Vec<Face>,
    loaded: HashMap<SrcKey, Option<usize>>,
    scaler: Option<ScaleContext>,
    chosen: HashMap<(char, Pick), Option<(usize, u16)>>,
    covering: HashMap<char, Option<(usize, u16)>>,
    advances: HashMap<(usize, u16, Settings), f32>,
    glyphs: HashMap<(usize, u16, u32, Settings), Option<Arc<Glyph>>>,
}

/// Characters that draw nothing and move nothing: the zero-width ones, and the selectors that say
/// how the one before is drawn.
fn nothing(c: char) -> bool {
    c.is_control()
        || matches!(c as u32, 0x200B..=0x200F | 0x2060..=0x2064 | 0xFE00..=0xFE0F | 0xFEFF)
}

impl State {
    /// The face of `src`, opened once.
    fn face(&mut self, src: &Src) -> Option<usize> {
        let key = src.key();
        if let Some(known) = self.loaded.get(&key) {
            return *known;
        }
        let opened = Face::open(src).map(|face| {
            self.faces.push(face);
            self.faces.len() - 1
        });
        self.loaded.insert(key, opened);
        opened
    }

    /// A face of `srcs`, the first that has `c`.
    fn first_with(&mut self, srcs: &[Src], c: char) -> Option<(usize, u16)> {
        srcs.iter().find_map(|src| {
            let id = self.face(src)?;
            let glyph = self.faces.get(id)?.glyph_of(c);
            (glyph != 0).then_some((id, glyph))
        })
    }

    /// The face and glyph a character not in a family is looked for in.
    fn elsewhere(&mut self, layout: &Layout, c: char) -> Option<(usize, u16)> {
        if let Some(known) = self.covering.get(&c) {
            return *known;
        }
        let found = if c as u32 >= 0x1F000 {
            self.first_with(&layout.emoji, c)
                .or_else(|| self.first_with(&layout.named, c))
        } else {
            self.first_with(&layout.named, c)
                .or_else(|| self.first_with(&layout.emoji, c))
        }
        .or_else(|| self.first_with(&layout.scan, c));
        self.covering.insert(c, found);
        found
    }

    /// The face and glyph a run drawn as `pick` draws `c` with, if any face has one.
    fn resolve(&mut self, layout: &Layout, pick: Pick, c: char) -> Option<(usize, u16)> {
        if let Some(known) = self.chosen.get(&(c, pick)) {
            return *known;
        }
        let mut found = None;
        for slots in layout.chain(pick.family) {
            if let Some(src) = slots.pick(pick.weight, pick.italic) {
                let hit = self.face(src).and_then(|id| {
                    let glyph = self.faces.get(id)?.glyph_of(c);
                    (glyph != 0).then_some((id, glyph))
                });
                if hit.is_some() {
                    found = hit;
                    break;
                }
            }
        }
        let found = found.or_else(|| self.elsewhere(layout, c));
        self.chosen.insert((c, pick), found);
        found
    }

    /// The first face of the families `pick` asks for that is there: what a line's height is that of.
    fn primary(&mut self, layout: &Layout, pick: Pick) -> Option<usize> {
        layout.chain(pick.family).into_iter().find_map(|slots| {
            slots
                .pick(pick.weight, pick.italic)
                .and_then(|src| self.face(src))
        })
    }

    /// How far the pen moves after `glyph` of face `id`, in ems.
    fn advance_em(&mut self, id: usize, glyph: u16, settings: Settings) -> f32 {
        *self
            .advances
            .entry((id, glyph, settings))
            .or_insert_with(|| {
                self.faces
                    .get(id)
                    .map_or(0.0, |face| face.advance_em(glyph, settings))
            })
    }

    /// The image of `glyph` of face `id`, drawn once.
    fn image(
        &mut self,
        id: usize,
        glyph: u16,
        size: f32,
        settings: Settings,
    ) -> Option<Arc<Glyph>> {
        let key = (id, glyph, size.to_bits(), settings);
        if let Some(known) = self.glyphs.get(&key) {
            return known.clone();
        }
        let scaler = self.scaler.get_or_insert_with(ScaleContext::new);
        let drawn = self
            .faces
            .get(id)
            .and_then(|face| draw(scaler, face, glyph, size, settings));
        self.glyphs.insert(key, drawn.clone());
        drawn
    }

    fn line(&mut self, layout: &Layout, font: &Font) -> Option<(f32, f32)> {
        let id = self.primary(layout, Pick::of(font))?;
        let (ascent, descent, leading) = self.faces.get(id)?.line;
        Some((ascent * font.size, (ascent + descent + leading) * font.size))
    }

    /// `text` set down in `font`; the images drawn too where `images`.
    fn set_down(
        &mut self,
        layout: &Layout,
        font: &Font,
        text: &str,
        scale: f32,
        images: bool,
    ) -> Line {
        let pick = Pick::of(font);
        let settings_of = |faces: &[Face], id: usize| {
            faces.get(id).map_or(
                Settings {
                    weight: 0,
                    optical: 0,
                },
                |face| face.settings(pick.weight, font.size, scale),
            )
        };
        // A character no face has is as wide as the zero of the face the run is drawn in.
        let blank = self.resolve(layout, pick, '0').map_or(0.6, |(id, glyph)| {
            let settings = settings_of(&self.faces, id);
            self.advance_em(id, glyph, settings)
        }) * font.size;
        let mut marks = Vec::new();
        let mut at = 0.0_f32;
        for c in text.chars() {
            let (advance, shape) = if nothing(c) {
                (0.0, Shape::Nothing)
            } else if let Some((id, glyph)) = self.resolve(layout, pick, c) {
                let settings = settings_of(&self.faces, id);
                let advance = self.advance_em(id, glyph, settings) * font.size;
                let shape = if images {
                    self.image(id, glyph, font.size, settings)
                        .map_or(Shape::Nothing, Shape::Glyph)
                } else {
                    Shape::Nothing
                };
                (advance, shape)
            } else {
                (blank, Shape::Box)
            };
            marks.push(Mark { at, advance, shape });
            at += advance;
        }
        Line { width: at, marks }
    }
}

/// The glyph drawn: a colour layer or bitmap where the face has them, else its outline.
fn draw(
    context: &mut ScaleContext,
    face: &Face,
    glyph: u16,
    size: f32,
    settings: Settings,
) -> Option<Arc<Glyph>> {
    let mut builder = context
        .builder_with_id(face.font(), [face.key.value(), 0])
        .size(size)
        .hint(false);
    let axes = settings.axes();
    if !axes.is_empty() {
        builder = builder.variations(axes);
    }
    let mut scaler = builder.build();
    let sources: &[Kind] = if face.colour {
        &[
            Kind::ColorOutline(0),
            Kind::ColorBitmap(StrikeWith::BestFit),
            Kind::Outline,
        ]
    } else {
        &[Kind::Outline]
    };
    let mut render = Render::new(sources);
    render.format(Format::Alpha);
    let image = render.render(&mut scaler, glyph)?;
    Some(Arc::new(Glyph {
        left: image.placement.left,
        top: image.placement.top,
        width: image.placement.width,
        colour: image.content == Content::Color,
        data: image.data,
    }))
}

/// A font file the caller gave that is not one: why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontError(pub String);

impl std::fmt::Display for FontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the font is not one: {}", self.0)
    }
}

impl std::error::Error for FontError {}

/// The faces a picture's words are drawn in, and what measures them. Cloning one shares what it
/// has opened and drawn.
#[derive(Clone)]
pub struct Fonts {
    layout: Option<Arc<Layout>>,
    state: Arc<Mutex<State>>,
    fixed: Fixed,
    /// How many pixels a point of the page is (for the optical size of a face that has one).
    scale: f32,
}

fn locked(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Fonts {
    fn with(layout: Option<Layout>, fixed: Fixed) -> Self {
        Self {
            layout: layout.map(Arc::new),
            state: Arc::new(Mutex::new(State::default())),
            fixed,
            scale: 1.0,
        }
    }

    /// These faces for a page drawn at `scale` pixels to the point (the page's own: `Headless`'s
    /// `.scale`, a terminal's cell over the page's): a face with an optical size axis (SF, Inter) is
    /// set to the size of the text in points, which is its size in pixels over the scale. Without it
    /// the size is taken to be in points.
    #[must_use]
    pub fn scaled(mut self, scale: f32) -> Self {
        self.scale = if scale > 0.0 { scale } else { 1.0 };
        self
    }

    /// No faces: every character a box as wide as [`Fixed`]'s default (what the host with no screen
    /// measures with unless told).
    pub fn none() -> Self {
        Self::boxes(Fixed::default())
    }

    /// No faces: every character a box as wide as `fixed` measures it.
    pub fn boxes(fixed: Fixed) -> Self {
        Self::with(None, fixed)
    }

    /// The faces of the machine: on a Mac SF Mono and SF (what bateri draws in), on Linux the first
    /// mono and sans faces of those it has; for a character they lack the first face of the machine
    /// that has it, and a box where none. Where the machine has no mono face, or no sans one, the carried
    /// one (JetBrains Mono, Inter).
    pub fn installed() -> Self {
        let platform = Platform::here();
        let home = std::env::var_os("HOME").map(PathBuf::from);
        Self::found(platform, &platform.dirs(home.as_deref()))
    }

    /// As [`Fonts::installed`], the faces looked for in `dirs` alone (a machine's layout for a
    /// test, or one made for it).
    pub fn in_dirs(dirs: &[PathBuf]) -> Self {
        Self::found(Platform::here(), dirs)
    }

    fn found(platform: Platform, dirs: &[PathBuf]) -> Self {
        let mut layout = find(platform, &Files::in_dirs(dirs));
        if layout.terminal.upright.is_none() {
            layout.terminal = carried_mono();
        }
        if layout.system.upright.is_none() {
            layout.system = carried_sans();
        }
        Self::with(Some(layout), Fixed::default())
    }

    /// The faces the program carries, and no others (a machine with none of its own: JetBrains Mono
    /// for the terminal's family and Inter for the system's; a colour emoji and an ideograph are
    /// boxes).
    pub fn carried() -> Self {
        Self::with(
            Some(Layout {
                terminal: carried_mono(),
                system: carried_sans(),
                ..Layout::default()
            }),
            Fixed::default(),
        )
    }

    fn put(mut self, family: Family, style: Style, bytes: &[u8]) -> Result<Self, FontError> {
        let bytes: Arc<[u8]> = Arc::from(bytes);
        if FontRef::from_index(&bytes, 0).is_none() {
            return Err(FontError("not a TrueType or OpenType file".to_owned()));
        }
        let src = Src {
            origin: Origin::Memory(bytes),
            index: 0,
        };
        let mut layout = self.layout.as_deref().cloned().unwrap_or_default();
        match family {
            Family::Terminal => layout.terminal.put(style, src),
            Family::System => layout.system.put(style, src),
        }
        self.layout = Some(Arc::new(layout));
        // What was opened and chosen belongs to the faces there were.
        self.state = Arc::new(Mutex::new(State::default()));
        Ok(self)
    }

    /// The terminal's face, upright: the file's bytes (a variable face is set to the weight a run
    /// asks for: its bold too).
    ///
    /// # Errors
    ///
    /// The bytes are not a font.
    pub fn terminal(self, bytes: &[u8]) -> Result<Self, FontError> {
        self.put(Family::Terminal, Style::Upright, bytes)
    }

    /// The terminal's face, bold.
    ///
    /// # Errors
    ///
    /// The bytes are not a font.
    pub fn bold(self, bytes: &[u8]) -> Result<Self, FontError> {
        self.put(Family::Terminal, Style::Bold, bytes)
    }

    /// The terminal's face, italic.
    ///
    /// # Errors
    ///
    /// The bytes are not a font.
    pub fn italic(self, bytes: &[u8]) -> Result<Self, FontError> {
        self.put(Family::Terminal, Style::Italic, bytes)
    }

    /// The system's face, upright.
    ///
    /// # Errors
    ///
    /// The bytes are not a font.
    pub fn system(self, bytes: &[u8]) -> Result<Self, FontError> {
        self.put(Family::System, Style::Upright, bytes)
    }

    /// Whether some face has a glyph for `c` in a run drawn as `font` (none: the box of a missing
    /// one is drawn); `None` where there is no face at all.
    pub fn covers(&self, font: &Font, c: char) -> Option<bool> {
        let layout = self.layout.as_deref()?;
        if nothing(c) {
            return Some(true);
        }
        Some(
            locked(&self.state)
                .resolve(layout, Pick::of(font), c)
                .is_some(),
        )
    }

    /// `text` set down in `font` and drawn, or `None` where there is no face (a box a character).
    pub(super) fn set_down(&self, font: &Font, text: &str) -> Option<Line> {
        let layout = self.layout.as_deref()?;
        Some(locked(&self.state).set_down(layout, font, text, self.scale, true))
    }

    /// Where a line of `font` has its baseline, below its top, and how high it is, pixels.
    pub(super) fn line(&self, font: &Font) -> (f32, f32) {
        let measured = self
            .layout
            .as_deref()
            .and_then(|layout| locked(&self.state).line(layout, font));
        measured.unwrap_or((self.fixed.line * 0.8, self.fixed.line))
    }

    /// How many pixels a box of a character is wide where there is no face.
    pub(super) fn box_advance(&self) -> f32 {
        self.fixed.advance
    }
}

/// A carried family: its upright and italic faces, packed.
fn carried_family(upright: &'static [u8], italic: &'static [u8]) -> Slots {
    let face = |bytes| {
        Some(Src {
            origin: Origin::Packed(bytes),
            index: 0,
        })
    };
    Slots {
        upright: face(upright),
        italic: face(italic),
        ..Slots::default()
    }
}

/// JetBrains Mono, the terminal's family a machine with no mono face is drawn in.
fn carried_mono() -> Slots {
    carried_family(MONO_UPRIGHT, MONO_ITALIC)
}

/// Inter, the system's family a machine with no sans face is drawn in.
fn carried_sans() -> Slots {
    carried_family(SANS_UPRIGHT, SANS_ITALIC)
}

impl Measure for Fonts {
    fn measure(&mut self, font: &Font, text: &str) -> Metrics {
        match self.layout.as_deref() {
            Some(layout) => {
                let mut state = locked(&self.state);
                let width = state.set_down(layout, font, text, self.scale, false).width;
                let height = state
                    .line(layout, font)
                    .map_or(font.size * 1.25, |line| line.1);
                Metrics { width, height }
            }
            None => self.fixed.measure(font, text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The faces the program carries are packed by the build and unpacked where they are drawn in: what
    /// comes out is the file that went in, every byte, and the pack is the smaller.
    #[test]
    fn the_carried_faces_unpack_to_the_files_they_were_packed_from() {
        let faces: [(&str, &[u8], &[u8]); 4] = [
            (
                "JetBrainsMono[wght].ttf",
                MONO_UPRIGHT,
                include_bytes!("../../fonts/JetBrainsMono[wght].ttf"),
            ),
            (
                "JetBrainsMono-Italic[wght].ttf",
                MONO_ITALIC,
                include_bytes!("../../fonts/JetBrainsMono-Italic[wght].ttf"),
            ),
            (
                "InterVariable.ttf",
                SANS_UPRIGHT,
                include_bytes!("../../fonts/InterVariable.ttf"),
            ),
            (
                "InterVariable-Italic.ttf",
                SANS_ITALIC,
                include_bytes!("../../fonts/InterVariable-Italic.ttf"),
            ),
        ];
        for (name, packed, file) in faces {
            let src = Src {
                origin: Origin::Packed(packed),
                index: 0,
            };
            let Some(face) = Face::open(&src) else {
                panic!("{name} does not open");
            };
            let Ok(unpacked) = miniz_oxide::inflate::decompress_to_vec_zlib(packed) else {
                panic!("{name} does not unpack");
            };
            assert!(
                unpacked == file,
                "{name} is not the file it was packed from"
            );
            assert!(
                face.bytes.get() == file,
                "{name} opens as other than the file it was packed from"
            );
            assert!(
                packed.len() < file.len(),
                "{name}: {} packed bytes of {}",
                packed.len(),
                file.len()
            );
        }
    }
}
