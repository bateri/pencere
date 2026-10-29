//! What a page may ask of the system its host runs on: the time of day, how that system makes and
//! writes its keyboard shortcuts, how the host's terminal draws its cursor and its selection (as the
//! person set them), and what the host draws beyond what every host does. The host says all of it; the
//! engine reads no clock and knows no system.

#[cfg(test)]
use crate::Key;
use crate::edit::{Caret, CaretMotion, CaretShape, CaretUnfocused, EditorOptions, SelectionLook};
use crate::{Mods, Shortcut};

/// The time of day, as the host's system says it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WallTime {
    /// Seconds since 1970-01-01 00:00:00 UTC, leap seconds not counted (Unix time).
    pub seconds: f64,
    /// How far the person's local time is ahead of UTC now, seconds (behind it, below 0).
    pub utc_offset: i32,
}

/// How the host's system makes and writes its keyboard shortcuts, as an editor's keybindings
/// tell its platforms apart: which modifier is the main one, and the words or signs of the keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum KeyConvention {
    /// A Mac's: ⌘ the main modifier, the keys written as signs (⌘⇧K).
    #[default]
    Mac,
    /// Windows's: Ctrl the main modifier, the keys written as words (Ctrl+Shift+K), the logo key
    /// Win.
    Windows,
    /// Linux's, and any other system's: as Windows's, the logo key Super.
    Linux,
}

/// How the host's terminal draws its cursor, in the words and units of the kit's editor
/// ([`Caret`]): each field is said only where the host has such a setting, and a field it does not
/// say is the kit's own default for the editor. A page that wants its editors to look as the
/// person's terminal does asks for this ([`TerminalLook::fill`]) and decides what it takes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TerminalCaret {
    pub shape: Option<CaretShape>,
    /// How wide a bar is, and how thick a line, points: what the terminal draws (the host turns its
    /// own pixels into points).
    pub width: Option<f32>,
    /// A fraction of a row's height ([`crate::edit::limits::ROUNDING`]).
    pub radius: Option<f32>,
    /// One number ([`crate::edit::limits::CARET_GLOW`]).
    pub glow: Option<f32>,
    pub blink: Option<bool>,
    /// Seconds a caret is lit, and then dark.
    pub blink_interval: Option<f32>,
    pub unfocused: Option<CaretUnfocused>,
    pub motion: Option<CaretMotion>,
}

/// How the host's terminal draws its selection.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TerminalSelection {
    /// A fraction of a row's height ([`crate::edit::limits::ROUNDING`]).
    pub radius: Option<f32>,
}

/// How the host's terminal draws its cursor and its selection: what it says is what the person
/// chose there, and a host that has no such setting says nothing. The colours are not here (the
/// theme's roles are the host's to resolve), nor Reduce Motion (every frame has it).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TerminalLook {
    pub caret: TerminalCaret,
    pub selection: TerminalSelection,
}

impl TerminalLook {
    /// `options` with what this says put where the page said nothing: a field of `options` that is
    /// the editor's default is one the page left, and takes the terminal's; one the page set to
    /// something else stays. A Rust page cannot tell a value it wrote that is the default from one it
    /// left (a page that wants a bar, the default, gets the terminal's shape): one that must keep it
    /// sets the field after. A host that says nothing gives `options` back as it is. The page that asks for this is the one that decides to.
    #[must_use]
    pub fn fill(&self, mut options: EditorOptions) -> EditorOptions {
        let default = EditorOptions::default();
        let (caret, was) = (&self.caret, &default.caret);
        let take = |field: &mut Caret| {
            if field.shape == was.shape {
                field.shape = caret.shape.unwrap_or(field.shape);
            }
            if field.width == was.width {
                field.width = caret.width.unwrap_or(field.width);
            }
            if field.radius == was.radius {
                field.radius = caret.radius.unwrap_or(field.radius);
            }
            if field.glow == was.glow {
                field.glow = caret.glow.unwrap_or(field.glow);
            }
            if field.blink == was.blink {
                field.blink = caret.blink.unwrap_or(field.blink);
            }
            if field.blink_interval == was.blink_interval {
                field.blink_interval = caret.blink_interval.unwrap_or(field.blink_interval);
            }
            if field.unfocused == was.unfocused {
                field.unfocused = caret.unfocused.unwrap_or(field.unfocused);
            }
            if field.motion == was.motion {
                field.motion = caret.motion.unwrap_or(field.motion);
            }
        };
        take(&mut options.caret);
        if options.selection == SelectionLook::default() {
            options.selection.radius = self.selection.radius.unwrap_or(options.selection.radius);
        }
        options
    }
}

/// Something a host may draw that not every host does ([`crate::edit::Caret::glow`] asks for one).
/// A host says which it does; a page can tell what it will not see from what it does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Drawing {
    /// A box's soft shadow (`Quad::shadow`), which a caret's glow is.
    Shadow,
}

impl Drawing {
    /// The name the kit gives it: `"shadow"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Shadow => "shadow",
        }
    }
}

impl KeyConvention {
    /// The modifier a shortcut is made with on this system (CodeMirror's `Mod`, Electron's
    /// `CmdOrCtrl`): command on a Mac, control elsewhere.
    pub fn primary(self) -> Mods {
        match self {
            Self::Mac => Mods {
                command: true,
                ..Mods::default()
            },
            Self::Windows | Self::Linux => Mods {
                control: true,
                ..Mods::default()
            },
        }
    }

    /// The name the kit gives it: `"mac"`, `"windows"` or `"linux"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Mac => "mac",
            Self::Windows => "windows",
            Self::Linux => "linux",
        }
    }
}

impl Shortcut {
    /// The keys `words` say ([`Shortcut::parse`]) on a system of the convention `convention`,
    /// where `mod` is its main modifier: `"mod+s"` is ⌘S on a Mac and Ctrl+S elsewhere.
    pub fn parse_for(words: &str, convention: KeyConvention) -> Option<Self> {
        let parts: Vec<&str> = words.split('+').collect();
        let rest: Vec<&str> = parts
            .iter()
            .copied()
            .filter(|part| !part.trim().eq_ignore_ascii_case("mod"))
            .collect();
        if rest.len() == parts.len() {
            return Self::parse(words);
        }
        let mut shortcut = Self::parse(&rest.join("+"))?;
        let main = convention.primary();
        shortcut.mods.command |= main.command;
        shortcut.mods.control |= main.control;
        Some(shortcut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_is_the_main_modifier_of_the_system() {
        let mac = Shortcut::parse_for("mod+shift+s", KeyConvention::Mac).unwrap();
        assert!(mac.mods.command && mac.mods.shift && !mac.mods.control);
        let linux = Shortcut::parse_for("Mod+s", KeyConvention::Linux).unwrap();
        assert!(linux.mods.control && !linux.mods.command);
        assert_eq!(linux.key, Key::Character('s'));
        assert_eq!(
            Shortcut::parse_for("ctrl+k", KeyConvention::Mac),
            Shortcut::parse("ctrl+k")
        );
        assert_eq!(Shortcut::parse_for("mod", KeyConvention::Mac), None);
        assert_eq!(Shortcut::parse("mod+s"), None);
    }

    /// bateri's terminal, as a host that has such settings says it.
    fn terminal() -> TerminalLook {
        TerminalLook {
            caret: TerminalCaret {
                shape: Some(CaretShape::Block),
                width: Some(1.5),
                radius: Some(0.1),
                glow: Some(1.0),
                blink: Some(false),
                blink_interval: Some(0.75),
                unfocused: Some(CaretUnfocused::Hollow),
                motion: Some(CaretMotion::Spring(30.0)),
            },
            selection: TerminalSelection { radius: Some(0.22) },
        }
    }

    #[test]
    fn a_host_that_says_nothing_gives_the_options_back_as_they_were() {
        let options = EditorOptions {
            gutter: false,
            caret: Caret {
                shape: CaretShape::Underline,
                glow: 0.5,
                ..Caret::default()
            },
            ..EditorOptions::default()
        };
        assert_eq!(TerminalLook::default().fill(options.clone()), options);
        assert_eq!(
            TerminalLook::default().fill(EditorOptions::default()),
            EditorOptions::default()
        );
    }

    #[test]
    fn what_the_host_says_is_put_where_the_page_said_nothing_and_what_the_page_wrote_stays() {
        let filled = terminal().fill(EditorOptions::default());
        assert_eq!(filled.caret.shape, CaretShape::Block);
        assert_eq!(filled.caret.width, 1.5);
        assert_eq!(filled.caret.radius, 0.1);
        assert_eq!(filled.caret.glow, 1.0);
        assert!(!filled.caret.blink);
        assert_eq!(filled.caret.blink_interval, 0.75);
        assert_eq!(filled.caret.unfocused, CaretUnfocused::Hollow);
        assert_eq!(filled.caret.motion, CaretMotion::Spring(30.0));
        assert_eq!(filled.selection.radius, 0.22);
        // The colours and the rest of the caret are the page's, and what the terminal has no
        // setting for (how long a caret goes on blinking) is the kit's.
        assert_eq!(filled.caret.color, Caret::default().color);
        assert_eq!(filled.caret.blink_for, Caret::default().blink_for);
        // A caret the page wrote one field of keeps it and takes the others, a field at a time: a
        // page that always wants a bar still gets the terminal's rounding and light.
        let page = EditorOptions {
            caret: Caret {
                shape: CaretShape::Underline,
                glow: 2.0,
                ..Caret::default()
            },
            selection: SelectionLook { radius: 0.3 },
            ..EditorOptions::default()
        };
        let filled = terminal().fill(page);
        assert_eq!(filled.caret.shape, CaretShape::Underline);
        assert_eq!(filled.caret.glow, 2.0);
        assert_eq!(filled.caret.radius, 0.1);
        assert_eq!(filled.caret.motion, CaretMotion::Spring(30.0));
        assert_eq!(filled.selection.radius, 0.3);
    }
}
