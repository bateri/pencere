//! The terminal's look as a page is told it (specification §9.5 and §11.2): the roles of a
//! terminal theme, its ANSI palette and its selection, each as `#rrggbb`, with the ones a host
//! does not have made up.

/// A colour, as the terminal draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Reads `#rrggbb` (or `rrggbb`).
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.strip_prefix('#').unwrap_or(text);
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();
        Some(Self(channel(0)?, channel(2)?, channel(4)?))
    }

    /// The colour as a CSS value: `#rrggbb`.
    pub fn css(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    /// The relative luminance of WCAG 2.x: 0 for black, 1 for white.
    pub fn luminance(self) -> f64 {
        fn linear(channel: u8) -> f64 {
            let c = f64::from(channel) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * linear(self.0) + 0.7152 * linear(self.1) + 0.0722 * linear(self.2)
    }

    /// This colour moved `amount` (0 to 1) of the way toward `other`, in sRGB.
    pub fn toward(self, other: Self, amount: f64) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let mix = |from: u8, to: u8| {
            let (from, to) = (f64::from(from), f64::from(to));
            (from + (to - from) * amount).round().clamp(0.0, 255.0) as u8
        };
        Self(
            mix(self.0, other.0),
            mix(self.1, other.1),
            mix(self.2, other.2),
        )
    }

    /// This colour moved `part` of `whole` of the way toward `other`, in sRGB, each channel
    /// rounded to the nearest value with halves away from zero (specification §11.2). Whole
    /// numbers all through, so that the result is the same on every machine.
    fn part_toward(self, other: Self, part: u32, whole: u32) -> Self {
        let whole = whole.max(1);
        let mix = |from: u8, to: u8| {
            let part = part.min(whole);
            let total = u32::from(from) * (whole - part) + u32::from(to) * part;
            let nearest = (2 * total + whole) / (2 * whole);
            u8::try_from(nearest).unwrap_or(u8::MAX)
        };
        Self(
            mix(self.0, other.0),
            mix(self.1, other.1),
            mix(self.2, other.2),
        )
    }
}

/// The sixteen colours of the ANSI palette as xterm has them by default, in the order of the
/// palette: black, red, green, yellow, blue, magenta, cyan, white, then the bright eight. What a
/// theme that does not give its palette is made up with (specification §11.2).
pub const XTERM_ANSI: [Rgb; 16] = [
    Rgb(0x00, 0x00, 0x00),
    Rgb(0xcd, 0x00, 0x00),
    Rgb(0x00, 0xcd, 0x00),
    Rgb(0xcd, 0xcd, 0x00),
    Rgb(0x00, 0x00, 0xee),
    Rgb(0xcd, 0x00, 0xcd),
    Rgb(0x00, 0xcd, 0xcd),
    Rgb(0xe5, 0xe5, 0xe5),
    Rgb(0x7f, 0x7f, 0x7f),
    Rgb(0xff, 0x00, 0x00),
    Rgb(0x00, 0xff, 0x00),
    Rgb(0xff, 0xff, 0x00),
    Rgb(0x5c, 0x5c, 0xff),
    Rgb(0xff, 0x00, 0xff),
    Rgb(0x00, 0xff, 0xff),
    Rgb(0xff, 0xff, 0xff),
];

/// The nine roles of a theme, its palette and its selection, every one of them a colour
/// (specification §11.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Roles {
    /// The default background.
    pub background: Rgb,
    /// The default foreground.
    pub foreground: Rgb,
    /// The dimmed foreground.
    pub dim: Rgb,
    /// The accent colour.
    pub accent: Rgb,
    /// The colour of rules and borders.
    pub border: Rgb,
    /// The success colour.
    pub success: Rgb,
    /// The error colour.
    pub error: Rgb,
    /// The warning colour.
    pub warning: Rgb,
    /// The information colour.
    pub info: Rgb,
    /// The background of what the person selected.
    pub selection: Rgb,
    /// The ANSI palette, in the order of its colours (black, red, green, yellow, blue, magenta,
    /// cyan, white, then the bright eight).
    pub ansi: [Rgb; 16],
}

/// The roles of a terminal theme the page is given. Only the background and the
/// foreground are required; the others are what a host knows and are made up
/// (see [`roles`](Self::roles)) when it does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// The default background.
    pub background: Rgb,
    /// The default foreground.
    pub foreground: Rgb,
    /// The dimmed foreground (secondary text). Default: a third of the way from
    /// the foreground to the background.
    pub dim: Option<Rgb>,
    /// The accent colour. Default: the information colour.
    pub accent: Option<Rgb>,
    /// The colour of rules and borders. Default: the background a fifth of the
    /// way toward the foreground.
    pub border: Option<Rgb>,
    /// The success colour.
    pub success: Option<Rgb>,
    /// The error colour.
    pub error: Option<Rgb>,
    /// The warning colour.
    pub warning: Option<Rgb>,
    /// The information colour.
    pub info: Option<Rgb>,
    /// The background of what the person selected. Default: the background a third of the way
    /// toward the accent.
    pub selection: Option<Rgb>,
    /// The ANSI palette, in the order of its colours (black, red, green, yellow, blue, magenta,
    /// cyan, white, then the bright eight): all sixteen or none. Default: [`XTERM_ANSI`].
    pub ansi: Option<[Rgb; 16]>,
    font_mono: String,
}

impl Theme {
    /// A theme of a background, a foreground and the terminal's font (a CSS
    /// `font-family` list, `"Menlo", monospace`; empty when the host does not know it).
    /// Characters a font list does not need are dropped from it: a page may put it into
    /// its styles.
    pub fn new(background: Rgb, foreground: Rgb, font_mono: &str) -> Self {
        let font_mono = font_mono
            .chars()
            .filter(|c| {
                c.is_ascii_alphanumeric() || matches!(c, ' ' | ',' | '"' | '\'' | '-' | '_' | '.')
            })
            .collect();
        Self {
            background,
            foreground,
            dim: None,
            accent: None,
            border: None,
            success: None,
            error: None,
            warning: None,
            info: None,
            selection: None,
            ansi: None,
            font_mono,
        }
    }

    /// Whether the theme is dark: the default background is darker than the
    /// default foreground by relative luminance (specification §11.2).
    pub fn is_dark(&self) -> bool {
        self.background.luminance() < self.foreground.luminance()
    }

    /// `dark` or `light`, as the `mode` of the page's `context` has it.
    pub fn mode(&self) -> &'static str {
        if self.is_dark() { "dark" } else { "light" }
    }

    /// The font list; empty when the host gave none.
    pub fn font_mono(&self) -> &str {
        &self.font_mono
    }

    /// All nine roles, the selection and the palette: what the host gave, and for each it did
    /// not, the colour the specification makes up (§11.2): `dim` a third of the way from the
    /// foreground to the background, `accent` as `info`, `border` a fifth of the way from the
    /// background to the foreground, `success`, `error`, `warning` and `info` as `#3fb950`,
    /// `#f85149`, `#d29922` and `#58a6ff`, `selection` a third of the way from the background to
    /// the accent, and the palette as xterm's ([`XTERM_ANSI`]).
    pub fn roles(&self) -> Roles {
        let (background, foreground) = (self.background, self.foreground);
        let info = self.info.unwrap_or(Rgb(0x58, 0xa6, 0xff));
        let accent = self.accent.unwrap_or(info);
        Roles {
            background,
            foreground,
            dim: self
                .dim
                .unwrap_or_else(|| foreground.part_toward(background, 1, 3)),
            accent,
            border: self
                .border
                .unwrap_or_else(|| background.part_toward(foreground, 1, 5)),
            success: self.success.unwrap_or(Rgb(0x3f, 0xb9, 0x50)),
            error: self.error.unwrap_or(Rgb(0xf8, 0x51, 0x49)),
            warning: self.warning.unwrap_or(Rgb(0xd2, 0x99, 0x22)),
            info,
            selection: self
                .selection
                .unwrap_or_else(|| background.part_toward(accent, 1, 3)),
            ansi: self.ansi.unwrap_or(XTERM_ANSI),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn black_on_white() -> Theme {
        Theme::new(
            Rgb(255, 255, 255),
            Rgb(0x17, 0x17, 0x17),
            "\"Menlo\", monospace",
        )
    }

    fn white_on_black() -> Theme {
        Theme::new(Rgb(0, 0, 0), Rgb(0xee, 0xee, 0xee), "monospace")
    }

    #[test]
    fn colours_are_read_and_written_as_css_hex() {
        assert_eq!(Rgb::parse("#1a2B3c"), Some(Rgb(0x1a, 0x2b, 0x3c)));
        assert_eq!(Rgb::parse("1a2b3c").unwrap().css(), "#1a2b3c");
        for bad in [
            "",
            "#12345",
            "#1234567",
            "#12345g",
            "rgb(1,2,3)",
            "#\u{e9}23456",
        ] {
            assert_eq!(Rgb::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn dark_is_the_background_being_darker_than_the_foreground() {
        assert!(white_on_black().is_dark());
        assert_eq!(white_on_black().mode(), "dark");
        assert!(!black_on_white().is_dark());
        assert_eq!(black_on_white().mode(), "light");
        // A dark foreground on a mid-grey background is light by luminance.
        assert_eq!(
            Theme::new(Rgb(0x80, 0x80, 0x80), Rgb(0, 0, 0), "").mode(),
            "light"
        );
        assert!((Rgb(255, 255, 255).luminance() - 1.0).abs() < 1e-9);
        assert!(Rgb(0, 0, 0).luminance().abs() < 1e-9);
    }

    #[test]
    fn mixing_moves_a_colour_toward_another() {
        assert_eq!(Rgb(0, 0, 0).toward(Rgb(255, 255, 255), 0.0), Rgb(0, 0, 0));
        assert_eq!(
            Rgb(0, 0, 0).toward(Rgb(255, 255, 255), 1.0),
            Rgb(255, 255, 255)
        );
        assert_eq!(
            Rgb(0, 0, 0).toward(Rgb(200, 100, 50), 0.5),
            Rgb(100, 50, 25)
        );
        assert_eq!(
            Rgb(10, 10, 10).toward(Rgb(20, 20, 20), 7.0),
            Rgb(20, 20, 20)
        );
    }

    #[test]
    fn a_part_of_the_way_is_rounded_to_the_nearest_with_halves_away_from_zero() {
        let (black, white) = (Rgb(0, 0, 0), Rgb(255, 255, 255));
        assert_eq!(black.part_toward(white, 0, 3), black);
        assert_eq!(black.part_toward(white, 3, 3), white);
        // 255 / 3 is 85 exactly; 255 / 5 is 51.
        assert_eq!(black.part_toward(white, 1, 3), Rgb(85, 85, 85));
        assert_eq!(black.part_toward(white, 1, 5), Rgb(51, 51, 51));
        // Halves go up, and a part past the whole stops at the whole.
        assert_eq!(Rgb(0, 0, 0).part_toward(Rgb(5, 5, 5), 1, 2), Rgb(3, 3, 3));
        assert_eq!(Rgb(5, 5, 5).part_toward(Rgb(0, 0, 0), 1, 2), Rgb(3, 3, 3));
        assert_eq!(Rgb(1, 1, 1).part_toward(Rgb(9, 9, 9), 9, 3), Rgb(9, 9, 9));
    }

    #[test]
    fn all_nine_roles_are_there_what_the_host_gave_and_the_rest_made_up() {
        // The two a host always has; the other seven are made up from the table of §11.2.
        let made_up = Theme::new(Rgb(255, 255, 255), Rgb(0x1f, 0x23, 0x28), "").roles();
        assert_eq!(made_up.dim.css(), "#6a6c70");
        assert_eq!(made_up.border.css(), "#d2d3d4");
        assert_eq!(
            [
                made_up.accent.css(),
                made_up.success.css(),
                made_up.error.css(),
                made_up.warning.css(),
                made_up.info.css(),
            ],
            ["#58a6ff", "#3fb950", "#f85149", "#d29922", "#58a6ff"]
        );
        let mut theme = black_on_white();
        theme.accent = Some(Rgb(1, 2, 3));
        theme.error = Some(Rgb(9, 8, 7));
        theme.dim = Some(Rgb(5, 5, 5));
        theme.info = Some(Rgb(7, 7, 7));
        let roles = theme.roles();
        assert_eq!(roles.accent, Rgb(1, 2, 3));
        assert_eq!(roles.error, Rgb(9, 8, 7));
        assert_eq!(roles.dim, Rgb(5, 5, 5));
        assert_eq!(roles.info, Rgb(7, 7, 7));
        // An accent the host does not give is the information colour, whatever that is.
        theme.accent = None;
        assert_eq!(theme.roles().accent, Rgb(7, 7, 7));
        assert_eq!(roles.background, Rgb(255, 255, 255));
        assert_eq!(roles.foreground, Rgb(0x17, 0x17, 0x17));
        assert_eq!(theme.font_mono(), "\"Menlo\", monospace");
    }

    #[test]
    fn the_selection_and_the_palette_are_what_the_host_gave_or_made_up() {
        // Made up: a third of the way from the background to the accent, and xterm's palette.
        let made_up = Theme::new(Rgb(0, 0, 0), Rgb(255, 255, 255), "").roles();
        assert_eq!(made_up.selection, Rgb(0x1d, 0x37, 0x55));
        assert_eq!(made_up.ansi, XTERM_ANSI);
        assert_eq!(made_up.ansi[1].css(), "#cd0000");
        assert_eq!(made_up.ansi[15].css(), "#ffffff");
        // The selection follows an accent the host gave, and is its own when it gave one.
        let mut theme = black_on_white();
        theme.accent = Some(Rgb(0, 0, 255));
        assert_eq!(theme.roles().selection, Rgb(170, 170, 255));
        theme.selection = Some(Rgb(1, 2, 3));
        let mut palette = XTERM_ANSI;
        palette[4] = Rgb(9, 9, 9);
        theme.ansi = Some(palette);
        let roles = theme.roles();
        assert_eq!(roles.selection, Rgb(1, 2, 3));
        assert_eq!(roles.ansi[4], Rgb(9, 9, 9));
        assert_eq!(roles.ansi[5], XTERM_ANSI[5]);
    }

    #[test]
    fn a_font_list_cannot_carry_a_declaration_into_the_styles() {
        let theme = Theme::new(
            Rgb(0, 0, 0),
            Rgb(255, 255, 255),
            "Menlo; } body { x: url(http://e) <script>",
        );
        assert!(
            !theme
                .font_mono()
                .contains([';', '{', '}', '<', '(', ')', ':', '/'])
        );
        let empty = Theme::new(Rgb(0, 0, 0), Rgb(255, 255, 255), "");
        assert!(empty.font_mono().is_empty());
    }
}
