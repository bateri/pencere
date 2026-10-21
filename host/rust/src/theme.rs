//! The terminal's look as MCP Apps styling (specification §11.2): the roles of a
//! terminal theme mapped to the CSS variables of MCP Apps § Theming.

use std::collections::BTreeMap;

/// A colour, as the terminal draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// A map of CSS variables, in a stable order.
pub type Variables = BTreeMap<&'static str, String>;

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
}

/// The roles of a terminal theme the view is given. Only the background and the
/// foreground are required; the others are what a host knows and are made up
/// from those two when it does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// The default background.
    pub background: Rgb,
    /// The default foreground.
    pub foreground: Rgb,
    /// The dimmed foreground (secondary text). Default: a third of the way from
    /// the foreground to the background.
    pub dim: Option<Rgb>,
    /// The accent colour (focus rings). Default: the information colour.
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
    font_mono: String,
}

impl Theme {
    /// A theme of a background, a foreground and the terminal's font (a CSS
    /// `font-family` list, `"Menlo", monospace`). Characters a font list does not
    /// need are dropped from it: it is put into the view's styles.
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
            font_mono,
        }
    }

    /// Whether the theme is dark: the default background is darker than the
    /// default foreground by relative luminance (specification §11.2).
    pub fn is_dark(&self) -> bool {
        self.background.luminance() < self.foreground.luminance()
    }

    /// `dark` or `light`, as `hostContext.theme` has it.
    pub fn mode(&self) -> &'static str {
        if self.is_dark() { "dark" } else { "light" }
    }

    /// The font list.
    pub fn font_mono(&self) -> &str {
        &self.font_mono
    }

    /// The CSS variables for `hostContext.styles.variables`: whole families, so
    /// that a view that reads one colour of a family never falls back on its own
    /// value for the rest (MCP Apps § Theming). Plain colours: a terminal theme has
    /// one mode, so no `light-dark()`.
    pub fn variables(&self) -> Variables {
        let (bg, fg) = (self.background, self.foreground);
        let info = self.info.unwrap_or(Rgb(0x58, 0xa6, 0xff));
        let roles = [
            ("info", info),
            ("danger", self.error.unwrap_or(Rgb(0xf8, 0x51, 0x49))),
            ("success", self.success.unwrap_or(Rgb(0x3f, 0xb9, 0x50))),
            ("warning", self.warning.unwrap_or(Rgb(0xd2, 0x99, 0x22))),
        ];
        let dim = self.dim.unwrap_or_else(|| fg.toward(bg, 1.0 / 3.0));
        let accent = self.accent.unwrap_or(info);
        let border = self.border.unwrap_or_else(|| bg.toward(fg, 0.2));
        let transparent = "transparent".to_owned();

        let mut v = Variables::new();
        v.insert("--color-background-primary", bg.css());
        v.insert("--color-background-secondary", bg.toward(fg, 0.06).css());
        v.insert("--color-background-tertiary", bg.toward(fg, 0.12).css());
        v.insert("--color-background-inverse", fg.css());
        v.insert("--color-background-ghost", transparent.clone());
        v.insert("--color-background-disabled", bg.toward(fg, 0.03).css());
        v.insert("--color-text-primary", fg.css());
        v.insert("--color-text-secondary", dim.css());
        v.insert("--color-text-tertiary", dim.toward(bg, 0.35).css());
        v.insert("--color-text-inverse", bg.css());
        v.insert("--color-text-ghost", dim.css());
        v.insert("--color-text-disabled", fg.toward(bg, 0.55).css());
        v.insert("--color-border-primary", border.css());
        v.insert("--color-border-secondary", border.toward(bg, 0.4).css());
        v.insert("--color-border-tertiary", border.toward(bg, 0.7).css());
        v.insert("--color-border-inverse", fg.css());
        v.insert("--color-border-ghost", transparent);
        v.insert("--color-border-disabled", border.toward(bg, 0.5).css());
        v.insert("--color-ring-primary", accent.css());
        v.insert("--color-ring-secondary", border.css());
        v.insert("--color-ring-inverse", fg.css());
        for (name, role) in roles {
            let key = |prefix: &str| -> &'static str {
                match (prefix, name) {
                    ("background", "info") => "--color-background-info",
                    ("background", "danger") => "--color-background-danger",
                    ("background", "success") => "--color-background-success",
                    ("background", "warning") => "--color-background-warning",
                    ("text", "info") => "--color-text-info",
                    ("text", "danger") => "--color-text-danger",
                    ("text", "success") => "--color-text-success",
                    ("text", "warning") => "--color-text-warning",
                    ("border", "info") => "--color-border-info",
                    ("border", "danger") => "--color-border-danger",
                    ("border", "success") => "--color-border-success",
                    ("border", "warning") => "--color-border-warning",
                    ("ring", "info") => "--color-ring-info",
                    ("ring", "danger") => "--color-ring-danger",
                    ("ring", "success") => "--color-ring-success",
                    _ => "--color-ring-warning",
                }
            };
            v.insert(key("background"), bg.toward(role, 0.18).css());
            v.insert(key("text"), role.css());
            v.insert(key("border"), role.css());
            v.insert(key("ring"), role.css());
        }
        if !self.font_mono.is_empty() {
            v.insert("--font-mono", self.font_mono.clone());
        }
        v
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
    fn a_variable_comes_with_the_whole_family_it_belongs_to() {
        let variables = white_on_black().variables();
        for family in ["background", "text", "border"] {
            for member in [
                "primary",
                "secondary",
                "tertiary",
                "inverse",
                "ghost",
                "info",
                "danger",
                "success",
                "warning",
                "disabled",
            ] {
                let name = format!("--color-{family}-{member}");
                assert!(variables.contains_key(name.as_str()), "{name}");
            }
        }
        for member in [
            "primary",
            "secondary",
            "inverse",
            "info",
            "danger",
            "success",
            "warning",
        ] {
            let name = format!("--color-ring-{member}");
            assert!(variables.contains_key(name.as_str()), "{name}");
        }
        assert_eq!(variables["--color-background-primary"], "#000000");
        assert_eq!(variables["--color-text-primary"], "#eeeeee");
        assert_eq!(variables["--font-mono"], "monospace");
        assert!(
            variables
                .values()
                .all(|v| v == "transparent" || v == "monospace" || Rgb::parse(v).is_some())
        );
    }

    #[test]
    fn what_the_host_gives_is_used_and_the_rest_is_made_up() {
        let mut theme = black_on_white();
        theme.accent = Some(Rgb(1, 2, 3));
        theme.error = Some(Rgb(9, 8, 7));
        theme.dim = Some(Rgb(5, 5, 5));
        let v = theme.variables();
        assert_eq!(v["--color-ring-primary"], "#010203");
        assert_eq!(v["--color-text-danger"], "#090807");
        assert_eq!(v["--color-text-secondary"], "#050505");
        assert_eq!(v["--font-mono"], "\"Menlo\", monospace");
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
        assert!(!empty.variables().contains_key("--font-mono"));
    }
}
