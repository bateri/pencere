//! The keys of a Mac's ⌘ in a terminal that tells ⌃⇧ and a letter from ⌃ and the letter (the Kitty
//! keyboard protocol).
//!
//! A page writes the keys of its editors and menus for a Mac: ⌘C, ⌘X, ⌘K. A terminal has no ⌘ key: the
//! one it has for the system's copy is ⌃⇧C, which its own window takes in the cases where the program
//! has not asked for the keys. So a ⌘ key with a letter is ⌃⇧ and the letter here, the way a
//! terminal's own programs write it: it is shown so (a menu says `Ctrl+Shift+C`, not `Super+C`, the
//! key no keyboard sends) and it is taken so (the key is the page's as ⌘ and the letter).
//!
//! A page that binds ⌃⇧ and a letter to a command of its own keeps it: its own binding is what
//! the key does, and the line of a menu that would have been the same key is not shown with it,
//! for pressing it does the page's other thing. A ⌘ shortcut that has no chord is not shown either
//! (a menu never says `Super+Left`: no keyboard sends it to a terminal). A ⌘ key a terminal does
//! report as one (the Kitty protocol has a bit for it) is the page's as it was.
//!
//! Not the keys that have a ⌃ key of their own in the editors (⌃A, ⌃Z and ⌃Y: select all, undo and
//! redo, which a terminal that cannot send ⇧ with ⌃ sends for the chord as well), not a key that is no
//! letter (a ⌘ with an arrow is the line's end, and ⌃⇧ with an arrow selects by words), and not a
//! sign: the terminal says the sign ⌃⇧ makes (the decoder, `input`), and ⌃⇧/ is Ctrl and `?` where the
//! slash is a key of its own and Ctrl and `/` where it is Shift and 7.

use pencere_view::{Key, Menu, MenuEntry, Mods, Shortcut};

/// What a terminal sends for a ⌘ key: ⌃ and ⇧.
const CHORD: Mods = Mods {
    shift: true,
    control: true,
    alt: false,
    command: false,
};

/// ⌘ alone.
const COMMAND: Mods = Mods {
    shift: false,
    control: false,
    alt: false,
    command: true,
};

/// The characters whose ⌃ key (no ⇧) the editors have for themselves.
const OWN: [char; 3] = ['a', 'z', 'y'];

/// Whether the ⌘ key of `key` has a chord: a letter that is not one of [`OWN`]. Not a sign: a terminal
/// that has the alternate keys says the sign ⌃⇧ makes (⌃⇧/ is Ctrl and `?` on one keyboard, ⌃⇧7 is Ctrl
/// and `/` on another), and that is the key, with no ⌘ in it.
fn has_chord(key: Key) -> bool {
    matches!(key, Key::Character(c) if c.is_alphabetic() && !OWN.contains(&c.to_ascii_lowercase()))
}

/// What the key `key` pressed with `mods` is to the page: ⌘ and the character where it is ⌃⇧ and a
/// character the page does not bind itself (`binds`: whether it binds the key to a command).
pub(crate) fn pressed(key: Key, mods: Mods, binds: impl Fn(Key, Mods) -> bool) -> Mods {
    if mods == CHORD && has_chord(key) && !binds(key, CHORD) {
        COMMAND
    } else {
        mods
    }
}

/// The shortcut of a line of a menu as a person is to press it: ⌃⇧ and the character for ⌘ and
/// the character, and none where the page binds that chord to something else (the key is not the
/// line's) or where the ⌘ key has no chord (⌘ with an arrow, with another modifier): no keyboard
/// sends a key the line could show, so it shows none. A shortcut with no ⌘ is as it was.
pub(crate) fn shown(shortcut: Shortcut, binds: impl Fn(Key, Mods) -> bool) -> Option<Shortcut> {
    if !shortcut.mods.command {
        return Some(shortcut);
    }
    if shortcut.mods == COMMAND && has_chord(shortcut.key) && !binds(shortcut.key, CHORD) {
        return Some(Shortcut::new(shortcut.key, CHORD));
    }
    None
}

/// The shortcuts of the lines of `menu` as [`shown`] has them.
pub(crate) fn menu(menu: &mut Menu, binds: impl Fn(Key, Mods) -> bool) {
    for entry in &mut menu.entries {
        if let MenuEntry::Item { keys, .. } = entry
            && let Some(shortcut) = *keys
        {
            *keys = shown(shortcut, &binds);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::{MenuBox, label};

    fn chord(c: char) -> (Key, Mods) {
        (Key::Character(c), CHORD)
    }

    fn nothing(_: Key, _: Mods) -> bool {
        false
    }

    #[test]
    fn control_shift_and_a_character_is_command_and_the_character() {
        for c in ['c', 'x', 'k', 'd', 'C', '\u{131}'] {
            let (key, mods) = chord(c);
            assert_eq!(pressed(key, mods, nothing), COMMAND, "{c}");
        }
    }

    #[test]
    fn a_sign_is_no_chord_of_a_command_key() {
        // The terminal with the alternate keys says the sign ⌃⇧ makes (⌃⇧/ is Ctrl and `?` on a US
        // keyboard, ⌃⇧7 is Ctrl and `/` on a Turkish one), and a sign that comes with ⌃ and ⇧ is the one
        // a terminal that does not say it sends: neither is a ⌘ key.
        for c in ['/', '[', ',', '7', '=', '?'] {
            let (key, mods) = chord(c);
            assert_eq!(pressed(key, mods, nothing), mods, "{c}");
        }
        for words in ["cmd+/", "cmd+,", "cmd+7", "cmd+["] {
            let shortcut = Shortcut::parse(words).expect("keys");
            assert_eq!(shown(shortcut, nothing), None, "{words}");
        }
    }

    #[test]
    fn the_keys_that_are_no_chord_of_a_command_key_are_as_they_were() {
        // ⌃A, ⌃Z and ⌃Y have keys of their own, and a terminal that cannot tell ⇧ sends the same.
        for c in ['a', 'z', 'y', 'Z'] {
            let (key, mods) = chord(c);
            assert_eq!(pressed(key, mods, nothing), mods, "{c}");
        }
        // Not a character, or other modifiers than ⌃⇧: as they were.
        let arrow = pressed(Key::Left, CHORD, nothing);
        assert_eq!(arrow, CHORD);
        for mods in [
            Mods {
                shift: false,
                ..CHORD
            },
            Mods {
                control: false,
                ..CHORD
            },
            Mods { alt: true, ..CHORD },
            Mods {
                command: true,
                ..CHORD
            },
            COMMAND,
        ] {
            assert_eq!(pressed(Key::Character('c'), mods, nothing), mods);
        }
    }

    #[test]
    fn a_chord_the_page_binds_is_the_pages_own() {
        let binds = |key: Key, mods: Mods| key == Key::Character('k') && mods == CHORD;
        assert_eq!(pressed(Key::Character('k'), CHORD, binds), CHORD);
        assert_eq!(pressed(Key::Character('j'), CHORD, binds), COMMAND);
    }

    #[test]
    fn control_and_a_character_is_the_pages_as_it_is_and_a_menu_line_with_it_says_it() {
        // Ctrl and the slash (the comment of an editor where the keyboard has no ⌘): no chord, none to
        // take for ⌘, and the line that has it says Ctrl+/ and not the chord that is the help's.
        let control = Mods {
            shift: false,
            control: true,
            alt: false,
            command: false,
        };
        assert_eq!(pressed(Key::Character('/'), control, nothing), control);
        let taken = |key: Key, mods: Mods| key == Key::Character('/') && mods == CHORD;
        assert_eq!(pressed(Key::Character('/'), control, taken), control);
        let mut menu = menu_of(vec![
            item("Toggle Line Comment", "ctrl+/"),
            item("Help", "cmd+/"),
        ]);
        super::menu(&mut menu, taken);
        assert_eq!(written(&menu), ["Ctrl+/", ""]);
    }

    fn item(title: &str, keys: &str) -> MenuEntry {
        MenuEntry::Item {
            title: title.to_owned(),
            keys: Some(Shortcut::parse(keys).expect("keys")),
            enabled: true,
        }
    }

    fn menu_of(entries: Vec<MenuEntry>) -> Menu {
        Menu {
            serial: 1,
            at: pencere_view::Point::new(0.0, 0.0),
            entries,
        }
    }

    /// What each line of the menu says of its keys, as the box that draws it has them.
    fn written(menu: &Menu) -> Vec<String> {
        menu.entries
            .iter()
            .filter_map(|entry| match entry {
                MenuEntry::Item { keys, .. } => Some(keys.as_ref().map(label).unwrap_or_default()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_menu_shows_command_keys_as_the_chords_a_terminal_has_and_none_where_the_page_took_the_chord()
     {
        let mut menu = menu_of(vec![
            item("Cut", "cmd+x"),
            item("Copy", "cmd+c"),
            MenuEntry::Separator,
            item("Select All", "ctrl+a"),
            item("Toggle Line Comment", "cmd+/"),
            item("Undo", "cmd+z"),
            item("Find", "cmd+shift+f"),
            item("Taken", "cmd+k"),
            item("Home", "cmd+left"),
            item("F1", "f1"),
        ]);
        let binds = |key: Key, mods: Mods| key == Key::Character('k') && mods == CHORD;
        let before = written(&menu);
        assert_eq!(
            before[0], "Super+X",
            "the page's words: the key no keyboard sends"
        );
        super::menu(&mut menu, binds);
        assert_eq!(
            written(&menu),
            [
                "Ctrl+Shift+X",
                "Ctrl+Shift+C",
                "Ctrl+A",
                // ⌘/ has no chord (⌃⇧/ is Ctrl and `?`), ⌘Z has ⌃Z of its own, ⌘⇧F and ⌘← have none
                // either: no key a keyboard sends.
                "",
                "",
                "",
                // ⌃⇧K is the page's own: the line has no key to show.
                "",
                "",
                "F1",
            ]
        );
        // And the box that draws the menu says no `Super` where a chord is: it reads what is above.
        let drawn = format!("{:?}", MenuBox::new(&menu, (0, 0), (60, 20)));
        assert!(drawn.contains("Ctrl+Shift+C"), "{drawn}");
        assert!(
            !drawn.contains("Super+C") && !drawn.contains("Super+X"),
            "{drawn}"
        );
    }
}
