//! The colours of the terminal the page is drawn in.
//!
//! A terminal tells a program its background, its foreground and the sixteen colours of its palette;
//! the roles of the kit are made of those. The nine roles that are a theme's own take what the
//! terminal gives (the accent is the palette's blue, the states its green, red, yellow and cyan),
//! and the rest are derived by the formulas of the kit's specification (`kit/SPEC.md`, Section 3.1),
//! so that a page looks alike on every host.

use pencere_view::{Color, Role, Source};

use crate::color::Rgb;

/// How much of the way from the background to the accent the selection is.
const SELECTION: f32 = 0.3;

/// How much of the way from the background to the foreground a hairline is (Section 11.2 of the
/// protocol, where a theme gives no border).
const HAIRLINE: f32 = 1.0 / 5.0;

/// How much of the way from the foreground to the background the dim text is.
const DIM: f32 = 1.0 / 3.0;

/// The most steps a surface is raised (Section 3.1).
const MOST_RAISED: u8 = 8;

/// The colours of a terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub background: Rgb,
    pub foreground: Rgb,
    /// The cursor's colour: the foreground where the terminal has none of its own.
    pub cursor: Rgb,
    /// The palette, in the order of its sixteen colours (xterm's): the normal eight, then the bright.
    pub ansi: [Rgb; 16],
}

impl Theme {
    pub const fn new(background: Rgb, foreground: Rgb, ansi: [Rgb; 16]) -> Self {
        Self {
            background,
            foreground,
            cursor: foreground,
            ansi,
        }
    }

    /// A dark terminal's colours (GitHub's dark theme), the ones the documentation's pictures are
    /// drawn in.
    pub fn dark() -> Self {
        Self::new(
            Rgb::hex(0x0d1117),
            Rgb::hex(0xc9d1d9),
            palette([
                0x484f58, 0xff7b72, 0x3fb950, 0xd29922, 0x58a6ff, 0xbc8cff, 0x39c5cf, 0xb1bac4,
                0x6e7681, 0xffa198, 0x56d364, 0xe3b341, 0x79c0ff, 0xd2a8ff, 0x56d4dd, 0xf0f6fc,
            ]),
        )
    }

    /// A light terminal's colours (GitHub's light theme).
    pub fn light() -> Self {
        Self::new(
            Rgb::hex(0xffffff),
            Rgb::hex(0x1f2328),
            palette([
                0x24292f, 0xcf222e, 0x116329, 0x4d2d00, 0x0969da, 0x8250df, 0x1b7c83, 0x6e7781,
                0x57606a, 0xa40e26, 0x1a7f37, 0x633c01, 0x218bff, 0xa475f9, 0x3192aa, 0x8c959f,
            ]),
        )
    }

    /// Whether the background is dark: where white stands out from it more than black does.
    pub fn is_dark(&self) -> bool {
        self.background.contrast(Rgb::hex(0xffffff)) > self.background.contrast(Rgb::hex(0x000000))
    }

    /// The quieter text: the foreground a third of the way toward the background.
    pub fn dim(&self) -> Rgb {
        self.foreground.mix(self.background, DIM)
    }

    /// What a role is in this theme.
    pub fn role(&self, role: Role) -> Rgb {
        let ansi = |index: usize| self.ansi.get(index).copied().unwrap_or(self.foreground);
        match role {
            Role::Background => self.background,
            Role::Foreground => self.foreground,
            Role::Dim => self.dim(),
            Role::Accent => ansi(4),
            Role::Cursor => self.cursor,
            Role::Success => ansi(2),
            Role::Error => ansi(1),
            Role::Info => ansi(6),
            Role::Warning => ansi(3),
            Role::Raised(steps) => self
                .background
                .mix(self.foreground, 0.04 * f32::from(steps.min(MOST_RAISED))),
            Role::Quiet => self.dim().mix(self.background, DIM),
            Role::Border => self.background.mix(self.foreground, HAIRLINE),
            Role::Shadow => {
                if self.is_dark() {
                    Rgb::hex(0x000000)
                } else {
                    self.foreground
                }
            }
            Role::Selection => self.background.mix(ansi(4), SELECTION),
            palette => palette
                .ansi_index()
                .map_or(self.foreground, |index| ansi(usize::from(index))),
        }
    }

    /// What `color` comes to in this theme: the colour and how much of it shows (`0.0..=1.0`). A
    /// value is the form that goes with the theme's brightness; a mix is made in sRGB.
    pub fn resolve(&self, color: &Color) -> (Rgb, f32) {
        let dark = self.is_dark();
        let unit = |c: Rgb| {
            [
                f32::from(c.r) / 255.0,
                f32::from(c.g) / 255.0,
                f32::from(c.b) / 255.0,
            ]
        };
        let [r, g, b, a] = color.resolve(|source| match source {
            Source::Role(role) => {
                let [r, g, b] = unit(self.role(role));
                [r, g, b, 1.0]
            }
            Source::Rgba { .. } => match source.value(dark) {
                Some(value) => [
                    f32::from(value.r) / 255.0,
                    f32::from(value.g) / 255.0,
                    f32::from(value.b) / 255.0,
                    f32::from(value.a) / 255.0,
                ],
                None => {
                    let [r, g, b] = unit(self.foreground);
                    [r, g, b, 1.0]
                }
            },
        });
        (Rgb::from_unit([r, g, b]), a.clamp(0.0, 1.0))
    }
}

fn palette(values: [u32; 16]) -> [Rgb; 16] {
    values.map(Rgb::hex)
}
