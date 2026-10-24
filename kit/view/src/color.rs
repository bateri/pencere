//! Colours. A page says a colour in one of two ways. The way to prefer is a [`Role`], a role of
//! the pane's theme, and, where it wants the role translucent, an alpha — `color-mix(accent 13%,
//! transparent)` is `Role::Accent.alpha(0.13)`. What the role is, in a given theme, is the
//! host's to say (a terminal resolves it in the theme of the pane), so the page follows the theme
//! the terminal wears and the arithmetic of colours stays in one place. The other way is a value
//! ([`Rgba`]), for the colour a role does not name — a brand's, a syntax's — with, if it wants
//! to stay readable on a theme of the other brightness, a second value for a dark theme.
//!
//! The engine resolves neither: a [`Color`] stays what the page said, and the host that draws it
//! turns it into a colour of its renderer — the role in its theme, the value that goes with its
//! theme's brightness ([`Source::value`]).

/// A role of the theme: its nine roles, the sixteen colours of its ANSI palette and its
/// selection, and the values the theme derives from them for surfaces, lines and shadows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// The ground of the pane.
    Background,
    /// The text.
    Foreground,
    /// The quieter text (the grid's SGR 2).
    Dim,
    /// The theme's accent.
    Accent,
    /// The cursor's colour.
    Cursor,
    /// Good: running, done.
    Success,
    /// Bad: failed.
    Error,
    /// Neutral news: a link, a place.
    Info,
    /// Needs a look.
    Warning,
    /// The ground raised by that many levels: `Raised(1)` is a card on the pane, `Raised(2)`
    /// a field or a key cap on the card. A dark theme's surface is lighter than the ground,
    /// a light theme's darker (the theme's one rule for "a step above").
    Raised(u8),
    /// The dim text a step further toward the ground: ink that must be readable and not
    /// stand out.
    Quiet,
    /// The hairline: seen, never read.
    Line,
    /// The colour a shadow is cast in: the theme's dark end (black on a dark theme, the
    /// foreground on a light one).
    Shadow,
    /// The ground of what the person selected.
    Selection,
    /// The ANSI palette, in the order of its sixteen colours (xterm's names): the first eight
    /// are the normal colours, the last eight the bright ones.
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

impl Role {
    /// The ANSI palette, in the order of its colours: `ANSI[n].ansi_index() == Some(n)`.
    pub const ANSI: [Role; 16] = [
        Role::Black,
        Role::Red,
        Role::Green,
        Role::Yellow,
        Role::Blue,
        Role::Magenta,
        Role::Cyan,
        Role::White,
        Role::BrightBlack,
        Role::BrightRed,
        Role::BrightGreen,
        Role::BrightYellow,
        Role::BrightBlue,
        Role::BrightMagenta,
        Role::BrightCyan,
        Role::BrightWhite,
    ];

    /// This role at `alpha` of its opacity.
    pub const fn alpha(self, alpha: f32) -> Color {
        Color {
            source: Source::Role(self),
            alpha,
        }
    }

    /// This role, solid.
    pub const fn solid(self) -> Color {
        self.alpha(1.0)
    }

    /// Where the role stands in the theme's ANSI palette, `0..=15`; `None` for a role that is
    /// not one of its colours.
    pub const fn ansi_index(self) -> Option<u8> {
        match self {
            Role::Black => Some(0),
            Role::Red => Some(1),
            Role::Green => Some(2),
            Role::Yellow => Some(3),
            Role::Blue => Some(4),
            Role::Magenta => Some(5),
            Role::Cyan => Some(6),
            Role::White => Some(7),
            Role::BrightBlack => Some(8),
            Role::BrightRed => Some(9),
            Role::BrightGreen => Some(10),
            Role::BrightYellow => Some(11),
            Role::BrightBlue => Some(12),
            Role::BrightMagenta => Some(13),
            Role::BrightCyan => Some(14),
            Role::BrightWhite => Some(15),
            _ => None,
        }
    }
}

/// A colour value: sRGB, eight bits to a channel, `a` the opacity (`255` solid).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// A solid colour.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }

    /// Reads `#rrggbb` or `#rrggbbaa` (any case); `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#')?;
        if !matches!(digits.len(), 6 | 8) || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |at: usize| u8::from_str_radix(digits.get(at..at + 2)?, 16).ok();
        Some(Self {
            r: channel(0)?,
            g: channel(2)?,
            b: channel(4)?,
            a: if digits.len() == 8 { channel(6)? } else { 255 },
        })
    }

    /// This colour as a [`Color`] that is the same on every theme.
    pub const fn solid(self) -> Color {
        Color {
            source: Source::Rgba {
                light: self,
                dark: None,
            },
            alpha: 1.0,
        }
    }

    /// This colour on a light theme and `dark` on a dark one.
    pub const fn or_dark(self, dark: Rgba) -> Color {
        Color {
            source: Source::Rgba {
                light: self,
                dark: Some(dark),
            },
            alpha: 1.0,
        }
    }
}

/// Where a colour comes from: a role of the theme, or a value of the page's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// A role, in the host's theme.
    Role(Role),
    /// A value. `dark` is what the colour is on a dark theme where that is not `light`; the
    /// host decides which theme it wears (a terminal: whether its background is dark), the page
    /// only gives both.
    Rgba { light: Rgba, dark: Option<Rgba> },
}

impl Source {
    /// The value of a colour that is one, on a theme that is dark or not; `None` for a role.
    pub const fn value(self, dark: bool) -> Option<Rgba> {
        match self {
            Source::Role(_) => None,
            Source::Rgba { light, dark: other } => match other {
                Some(other) if dark => Some(other),
                _ => Some(light),
            },
        }
    }
}

/// A colour with an opacity: what the page said, a role or a value, and how much of it shows
/// on top of that (`0.0` transparent, `1.0` solid; a value's own opacity, `Rgba::a`, goes with
/// it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub source: Source,
    /// `0.0` transparent, `1.0` solid.
    pub alpha: f32,
}

impl Color {
    /// The role, when the colour is one.
    pub const fn role(&self) -> Option<Role> {
        match self.source {
            Source::Role(role) => Some(role),
            Source::Rgba { .. } => None,
        }
    }

    /// Whether the colour covers what is under it altogether, whatever the theme: a solid
    /// role, or a value whose every form is solid. (A role is taken for solid at its full
    /// opacity: the engine cannot know a theme that gave a role a translucent value.)
    pub fn is_opaque(&self) -> bool {
        let solid = match self.source {
            Source::Role(_) => true,
            Source::Rgba { light, dark } => light.a == 255 && dark.is_none_or(|dark| dark.a == 255),
        };
        solid && self.alpha >= 1.0
    }
}

impl From<Role> for Color {
    fn from(role: Role) -> Self {
        role.solid()
    }
}

impl From<Rgba> for Color {
    fn from(value: Rgba) -> Self {
        value.solid()
    }
}
