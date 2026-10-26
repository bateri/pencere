//! What a page can be told to do, by name: its commands ([`Command`]), each with a title and the
//! keys that do it, which the host lists where a person looks for what can be done (its command
//! palette, its menus) and does when those keys are pressed.
//!
//! A command belongs to an element ([`crate::Element::command`]) and can be done while the keyboard
//! is on that element or on one inside it ([`crate::Element::focused`]); the commands of the
//! page's outermost box can be done whatever has the keyboard. Where two say the same keys, the
//! one nearer the keyboard is the one done — the way a menu of a list and a menu of the window
//! share a key and the list's wins while it is in front.

use crate::{Id, Key, Mods};

/// Keys that do a command: one key with the modifiers held down with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub key: Key,
    pub mods: Mods,
}

impl Shortcut {
    pub const fn new(key: Key, mods: Mods) -> Self {
        Self { key, mods }
    }

    /// The keys written as a person writes them: modifiers and a key joined by `+`, in any case —
    /// `"cmd+k"`, `"ctrl+shift+p"`, `"alt+enter"`, `"f5"`, `"escape"`. The modifiers are `cmd`
    /// (or `command`), `ctrl` (or `control`), `alt` (or `option`) and `shift`; the key is one
    /// character, or the name of a key the page reads (`up`, `page_down`, `enter`, `tab`,
    /// `backspace`, `f1`…). `None` for words that are not keys.
    pub fn parse(words: &str) -> Option<Self> {
        let mut mods = Mods::default();
        let parts: Vec<&str> = words.split('+').map(str::trim).collect();
        let (key, held) = parts.split_last()?;
        for part in held {
            match part.to_ascii_lowercase().as_str() {
                "cmd" | "command" | "super" | "meta" => mods.command = true,
                "ctrl" | "control" => mods.control = true,
                "alt" | "option" | "opt" => mods.alt = true,
                "shift" => mods.shift = true,
                _ => return None,
            }
        }
        Some(Self {
            key: Key::named(key)?,
            mods,
        })
    }

    /// Whether a key pressed with `mods` is this one. A character is the same whatever its case
    /// (shift says which).
    pub fn matches(&self, key: Key, mods: Mods) -> bool {
        mods == self.mods && same_key(self.key, key)
    }
}

fn same_key(a: Key, b: Key) -> bool {
    match (a, b) {
        (Key::Character(a), Key::Character(b)) => a.to_lowercase().eq(b.to_lowercase()),
        _ => a == b,
    }
}

impl Key {
    /// The key a word names: one character, or the name of a key (`"up"`, `"page_down"`,
    /// `"enter"`, `"escape"`, `"tab"`, `"backspace"`, `"delete"`, `"home"`, `"end"`, `"space"`,
    /// `"f1"` to `"f20"`), in any case. `None` for a word that is none.
    pub fn named(word: &str) -> Option<Self> {
        let mut chars = word.chars();
        if let (Some(only), None) = (chars.next(), chars.next()) {
            return Some(Key::Character(only));
        }
        let lower = word.to_ascii_lowercase();
        Some(match lower.as_str() {
            "up" => Key::Up,
            "down" => Key::Down,
            "left" => Key::Left,
            "right" => Key::Right,
            "page_up" | "pageup" => Key::PageUp,
            "page_down" | "pagedown" => Key::PageDown,
            "home" => Key::Home,
            "end" => Key::End,
            "enter" | "return" => Key::Enter,
            "escape" | "esc" => Key::Escape,
            "tab" => Key::Tab,
            "backspace" => Key::Backspace,
            "delete" | "del" => Key::Delete,
            "space" => Key::Character(' '),
            other => {
                let number: u8 = other.strip_prefix('f')?.parse().ok()?;
                if !(1..=20).contains(&number) {
                    return None;
                }
                Key::Function(number)
            }
        })
    }
}

/// Something a page can be told to do: what it is called (`id`, which the page is told back), what
/// a person reads in a list of them (`title`), the keys that do it, and whether it can be done now.
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub id: Id,
    pub title: String,
    pub keys: Option<Shortcut>,
    pub enabled: bool,
}

impl Command {
    /// A command that can be done now and has no keys.
    pub fn new(id: Id, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            keys: None,
            enabled: true,
        }
    }

    /// The keys that do it.
    #[must_use]
    pub fn keys(mut self, keys: Shortcut) -> Self {
        self.keys = Some(keys);
        self
    }

    /// Whether it can be done now: one that cannot is listed, greyed, and its keys do nothing.
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_read_as_a_person_writes_them() {
        let k = Shortcut::parse("Cmd+Shift+K").unwrap();
        assert_eq!(k.key, Key::Character('K'));
        assert!(k.mods.command && k.mods.shift && !k.mods.control && !k.mods.alt);
        assert_eq!(Shortcut::parse("f5").unwrap().key, Key::Function(5));
        assert_eq!(
            Shortcut::parse("ctrl+page_down").unwrap().key,
            Key::PageDown
        );
        assert_eq!(Shortcut::parse("escape").unwrap().mods, Mods::default());
        assert_eq!(Shortcut::parse("space").unwrap().key, Key::Character(' '));
        assert_eq!(Shortcut::parse("hyper+k"), None);
        assert_eq!(Shortcut::parse("f21"), None);
        assert_eq!(Shortcut::parse("nothing"), None);
        assert_eq!(Shortcut::parse(""), None);
    }

    #[test]
    fn a_character_is_the_same_key_in_either_case() {
        let k = Shortcut::parse("cmd+shift+k").unwrap();
        let held = Mods {
            command: true,
            shift: true,
            ..Mods::default()
        };
        assert!(k.matches(Key::Character('K'), held));
        assert!(!k.matches(Key::Character('K'), Mods::default()));
        assert!(!k.matches(Key::Character('j'), held));
    }
}
