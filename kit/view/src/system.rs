//! What a page may ask of the system its host runs on: the time of day, and how that system makes
//! and writes its keyboard shortcuts. The host says both; the engine reads no clock and knows no
//! system.

#[cfg(test)]
use crate::Key;
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
}
