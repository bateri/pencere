//! The editor's keys as the page changed them (`keymap`), and the chords among them: a key that
//! starts a chord is waited on (the page is told, so that a status line can say `⌘K …`), the next
//! key finishes it or, where it is no key of it, ends it and does nothing; Escape and the time
//! `chord_timeout` end it too.

use pencere_view::edit::{KeyAction, Keybinding};
use pencere_view::{Id, Key, Mods, Shortcut};

use super::{Action, EditorEvent, Editors};

/// A chord begun: where, the keys so far and the time it is waited on until.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Chord {
    editor: Id,
    keys: Vec<Shortcut>,
    until: f64,
}

/// What the page's keys say of a key.
pub(super) enum Said {
    /// Not the page's to say: the editor's own keys.
    Own,
    /// A key of a chord: done, wait for the next.
    Waiting,
    /// The key ends a chord that was begun, and is nothing: done.
    Ended,
    /// The page says this (`None`: nothing of the editor's).
    Does(Option<Action>),
}

impl From<&KeyAction> for Action {
    fn from(action: &KeyAction) -> Self {
        match action {
            KeyAction::Command(command) => Action::Command(command.clone()),
            KeyAction::Tab => Action::Tab,
            KeyAction::Copy => Action::Copy,
            KeyAction::Cut => Action::Cut,
            KeyAction::Paste => Action::Paste,
            KeyAction::Escape => Action::Escape,
            KeyAction::ToLineStart => Action::ToLineStart,
            KeyAction::ToLineEnd => Action::ToLineEnd,
        }
    }
}

/// The keys as a page writes them.
pub(super) fn words(shortcut: &Shortcut) -> String {
    let mut out = String::new();
    for (held, word) in [
        (shortcut.mods.control, "ctrl+"),
        (shortcut.mods.alt, "alt+"),
        (shortcut.mods.shift, "shift+"),
        (shortcut.mods.command, "cmd+"),
    ] {
        if held {
            out.push_str(word);
        }
    }
    out.push_str(&match shortcut.key {
        Key::Up => "up".into(),
        Key::Down => "down".into(),
        Key::Left => "left".into(),
        Key::Right => "right".into(),
        Key::PageUp => "page_up".into(),
        Key::PageDown => "page_down".into(),
        Key::Home => "home".into(),
        Key::End => "end".into(),
        Key::Enter => "enter".into(),
        Key::Escape => "escape".into(),
        Key::Tab => "tab".into(),
        Key::Backspace => "backspace".into(),
        Key::Delete => "delete".into(),
        Key::Function(n) => format!("f{n}"),
        Key::Character(c) => c.to_string(),
    });
    out
}

impl Editors {
    /// What the page's keys say of `key` held with `mods` in the editor named `id`, at `now`.
    pub(super) fn keymap_says(&mut self, id: Id, key: Key, mods: Mods, now: f64) -> Said {
        let step = Shortcut::new(key, mods);
        let held = match self.chord.take() {
            Some(chord) if chord.editor == id => Some(chord),
            Some(_) | None => None,
        };
        let Some(editor) = self.editors.get(&id) else {
            return Said::Own;
        };
        let options = &editor.view.options;
        let mut keys = held
            .as_ref()
            .map_or_else(Vec::new, |chord| chord.keys.clone());
        keys.push(step);
        let is = |binding: &Keybinding, upto: usize| {
            binding.keys.len() >= upto
                && binding
                    .keys
                    .iter()
                    .zip(&keys)
                    .all(|(wanted, pressed)| wanted.matches(pressed.key, pressed.mods))
        };
        // Escape ends a chord that waits.
        if held.is_some() && key == Key::Escape && mods == Mods::default() {
            self.events.push(EditorEvent::KeymapPending {
                editor: id,
                keys: Vec::new(),
            });
            return Said::Ended;
        }
        let whole = options
            .keymap
            .iter()
            .rev()
            .find(|binding| binding.keys.len() == keys.len() && is(binding, keys.len()));
        if let Some(binding) = whole {
            let action = binding.action.as_ref().map(Action::from);
            if held.is_some() {
                self.events.push(EditorEvent::KeymapPending {
                    editor: id,
                    keys: Vec::new(),
                });
            }
            return Said::Does(action);
        }
        let begins = options
            .keymap
            .iter()
            .any(|binding| binding.keys.len() > keys.len() && is(binding, keys.len()));
        if begins {
            let until = now + f64::from(options.chord_timeout);
            self.events.push(EditorEvent::KeymapPending {
                editor: id,
                keys: keys.iter().map(words).collect(),
            });
            self.chord = Some(Chord {
                editor: id,
                keys,
                until,
            });
            return Said::Waiting;
        }
        if held.is_some() {
            self.events.push(EditorEvent::KeymapPending {
                editor: id,
                keys: Vec::new(),
            });
            return Said::Ended;
        }
        Said::Own
    }

    /// The time a chord that waits is given up at.
    pub(super) fn chord_due(&self) -> Option<f64> {
        self.chord.as_ref().map(|chord| chord.until)
    }

    /// The frame at `now`: a chord whose time has come is given up.
    pub(super) fn tick_chord(&mut self, now: f64) {
        if let Some(chord) = self.chord.take_if(|chord| now >= chord.until) {
            self.events.push(EditorEvent::KeymapPending {
                editor: chord.editor,
                keys: Vec::new(),
            });
        }
    }
}
