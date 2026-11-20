//! The headless host's text fields: a stand-in for a system's, with a text, a caret and what is
//! selected, and the kit's rule of which keys a field keeps ([`pencere_view::FieldState::takes`]).
//!
//! It edits as a field of one line does, a character at a time: what is typed takes the place of
//! what is selected, backspace and delete take a character (or what is selected), the arrows move
//! the caret a character (shift selects), home and end and, in lines, the keys that go up and down
//! go to an end, ⌘A selects all and ⌘C, ⌘X and ⌘V use the host's pasteboard. Places are characters.
//!
//! A picture of the page draws them over it as a system draws its own ([`drawn`]): a page's draw
//! list has no text of the host's.

#[cfg(feature = "shot")]
use std::collections::HashMap;
use std::ops::Range;

#[cfg(feature = "shot")]
use pencere_view::{Color, Family, Font, Id, Measure, Paint, Point, Quad, Rect, Role, Run};
use pencere_view::{FieldKind, FieldState, Fill, Key, Mods};

#[cfg(feature = "shot")]
use crate::{Field, ZOOM_TEXT};

/// What one field holds: its text, and where its selection starts and where its caret is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Held {
    pub(super) text: String,
    anchor: usize,
    head: usize,
}

/// What a key did in a field.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Done {
    /// The field has no use for it: it is the page's.
    Passed,
    /// It moved the caret or the selection.
    Moved,
    /// It changed the text: the page is told what the field holds.
    Changed,
}

impl Held {
    /// A field that holds `text`, the caret at its end.
    pub(super) fn holding(text: &str) -> Self {
        let end = text.chars().count();
        Self {
            text: text.to_owned(),
            anchor: end,
            head: end,
        }
    }

    fn length(&self) -> usize {
        self.text.chars().count()
    }

    /// What is selected, in order.
    pub(super) fn selection(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    /// What a page's reply puts in the field.
    pub(super) fn fill(&mut self, fill: &Fill) {
        if let Some(text) = &fill.text {
            *self = Self::holding(text);
        }
        // The page says bytes; the field holds characters. An end inside a character is its start.
        if let Some(range) = fill.selection_in(self.text.len()) {
            self.anchor = chars_before(&self.text, range.start);
            self.head = chars_before(&self.text, range.end);
        }
    }

    /// `text` in place of what is selected, the caret after it.
    pub(super) fn insert(&mut self, text: &str) {
        let range = self.selection();
        let start = byte_at(&self.text, range.start);
        let end = byte_at(&self.text, range.end);
        self.text.replace_range(start..end, text);
        self.anchor = range.start + text.chars().count();
        self.head = self.anchor;
    }

    fn selected(&self) -> String {
        let range = self.selection();
        self.text
            .chars()
            .skip(range.start)
            .take(range.end - range.start)
            .collect()
    }

    fn caret_to(&mut self, at: usize, extend: bool) {
        self.head = at;
        if !extend {
            self.anchor = at;
        }
    }

    /// The person presses `key` with `mods` in the field, a field of `kind`; `pasteboard` is the
    /// host's.
    pub(super) fn key(
        &mut self,
        kind: FieldKind,
        key: Key,
        mods: Mods,
        pasteboard: &mut Option<String>,
    ) -> Done {
        let state = FieldState {
            kind,
            length: self.length(),
            selection: self.selection(),
        };
        if !state.takes(key, mods) {
            return Done::Passed;
        }
        let range = self.selection();
        let caret = range.is_empty();
        let end = self.length();
        let extend = mods.shift;
        match key {
            Key::Character(typed) if !(mods.command || mods.control) => {
                self.insert(&typed.to_string());
                Done::Changed
            }
            Key::Character(chord) => match chord.to_ascii_lowercase() {
                'a' => {
                    self.anchor = 0;
                    self.head = end;
                    Done::Moved
                }
                'c' => {
                    *pasteboard = Some(self.selected());
                    Done::Moved
                }
                'x' => {
                    *pasteboard = Some(self.selected());
                    self.insert("");
                    Done::Changed
                }
                'v' => {
                    let text = pasteboard.clone().unwrap_or_default();
                    self.insert(&text);
                    Done::Changed
                }
                // Undo and redo: the stand-in keeps no history.
                _ => Done::Moved,
            },
            Key::Enter => {
                self.insert("\n");
                Done::Changed
            }
            Key::Backspace | Key::Delete => {
                if caret {
                    let at = range.start;
                    if key == Key::Backspace {
                        self.anchor = at - 1;
                    } else {
                        self.head = at + 1;
                    }
                }
                self.insert("");
                Done::Changed
            }
            Key::Left if !extend && !caret => {
                self.caret_to(range.start, false);
                Done::Moved
            }
            Key::Right if !extend && !caret => {
                self.caret_to(range.end, false);
                Done::Moved
            }
            Key::Left => {
                self.caret_to(self.head.saturating_sub(1), extend);
                Done::Moved
            }
            Key::Right => {
                self.caret_to((self.head + 1).min(end), extend);
                Done::Moved
            }
            Key::Home | Key::Up | Key::PageUp => {
                self.caret_to(0, extend);
                Done::Moved
            }
            Key::End | Key::Down | Key::PageDown => {
                self.caret_to(end, extend);
                Done::Moved
            }
            Key::Escape | Key::Tab | Key::Function(_) => Done::Passed,
        }
    }
}

/// How many characters of `text` end at or before the byte `at`: a byte inside a character is the
/// place the character starts at.
fn chars_before(text: &str, at: usize) -> usize {
    text.char_indices()
        .take_while(|(byte, character)| byte + character.len_utf8() <= at)
        .count()
}

/// Where the `at`th character of `text` starts, bytes.
fn byte_at(text: &str, at: usize) -> usize {
    text.char_indices()
        .nth(at)
        .map_or(text.len(), |(byte, _)| byte)
}

/// The host's fields over a picture of the page ([`super::Headless::shot`]), as a system's are over
/// the page: each says what it holds, in the terminal's face at the kit's text size (a secret a dot
/// for each character), or its hint, quiet, where it holds nothing. One line stands in the middle of
/// its room, with what is selected under its words and, in the field that has the keyboard, the
/// caret where nothing is selected, and it scrolls as a field does to keep the caret in its room; lines stand
/// from the room's top, a line of the text to a line, with no selection or caret. A field the page
/// covers is not drawn. `fields` are the last frame's (the host's points), `held` what they hold,
/// `focus` the one with the keyboard, `zoom` the page's and `scale` the view's pixels to a point.
#[cfg(feature = "shot")]
pub(super) fn drawn(
    fields: &[Field],
    held: &HashMap<Id, Held>,
    focus: Option<Id>,
    zoom: f32,
    scale: f32,
    fonts: &crate::shot::Fonts,
) -> Vec<Paint> {
    let mut measure = fonts.clone();
    let font = Font {
        family: Family::Terminal,
        size: ZOOM_TEXT * zoom * scale,
        weight: 400,
        italic: false,
    };
    let line = measure.measure(&font, "M").height;
    let mut items = Vec::new();
    for field in fields.iter().filter(|field| !field.covered) {
        let room = Rect::new(
            field.rect.x * scale,
            field.rect.y * scale,
            field.rect.w * scale,
            field.rect.h * scale,
        );
        let none = Held::default();
        let held = held.get(&field.id).unwrap_or(&none);
        let text = if field.kind == FieldKind::Secret {
            "\u{2022}".repeat(held.length())
        } else {
            held.text.clone()
        };
        let hint = field.hint.as_deref().filter(|_| text.is_empty());
        if field.kind == FieldKind::Lines {
            let (words, ink) = hint.map_or((text.as_str(), Role::Foreground), |hint| {
                (hint, Role::Quiet)
            });
            for (row, words) in words.split('\n').enumerate() {
                if words.is_empty() {
                    continue;
                }
                #[allow(clippy::cast_precision_loss)]
                let top = room.y + row as f32 * line;
                items.push(words_at((room.x, top), words, font, ink, room));
            }
            continue;
        }
        let top = room.y + (room.h - line) / 2.0;
        let mut width = |chars: usize| {
            let before: String = text.chars().take(chars).collect();
            measure.measure(&font, &before).width
        };
        let selection = held.selection();
        let (start, end, head) = (
            width(selection.start),
            width(selection.end),
            width(held.head),
        );
        let has = focus == Some(field.id);
        let caret = scale.ceil().max(1.0);
        // As a field scrolls: the caret of the one with the keyboard is in its room.
        let left = room.x
            - if has {
                (head + caret - room.w).max(0.0)
            } else {
                0.0
            };
        if !selection.is_empty() {
            let rect = Rect::new(left + start, top, end - start, line);
            items.push(ground(rect, Role::Selection, room));
        }
        if let Some(hint) = hint {
            items.push(words_at((room.x, top), hint, font, Role::Quiet, room));
        } else if !text.is_empty() {
            items.push(words_at((left, top), &text, font, Role::Foreground, room));
        }
        if has && selection.is_empty() {
            let rect = Rect::new(left + head, top, caret, line);
            items.push(ground(rect, Role::Cursor, room));
        }
    }
    items
}

/// A line of `text` in `font` and the colour `ink`, its line box's top-left at `at`, cut at `clip`.
#[cfg(feature = "shot")]
fn words_at(at: (f32, f32), text: &str, font: Font, ink: Role, clip: Rect) -> Paint {
    Paint::Text(Run {
        origin: Point::new(at.0, at.1),
        text: text.to_owned(),
        font,
        color: Color::from(ink),
        opacity: 1.0,
        grounds: Vec::new(),
        clip,
        underline: false,
        strike: false,
    })
}

/// A box of the colour `fill`, cut at `clip`.
#[cfg(feature = "shot")]
fn ground(rect: Rect, fill: Role, clip: Rect) -> Paint {
    Paint::Quad(Quad {
        rect,
        radii: [0.0; 4],
        fill: Some(Color::from(fill)),
        gradient: None,
        border: None,
        shadow: None,
        clip,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(held: &mut Held, key: Key, mods: Mods) -> Done {
        held.key(FieldKind::Line, key, mods, &mut None)
    }

    #[test]
    fn a_field_types_over_its_selection_and_gives_the_page_what_it_cannot_use() {
        let mut held = Held::holding("draft.md");
        held.fill(&Fill::selection(0..5));
        assert_eq!(held.selection(), 0..5);
        assert_eq!(
            key(&mut held, Key::Character('n'), Mods::default()),
            Done::Changed
        );
        assert_eq!(held.text, "n.md");
        assert_eq!(held.selection(), 1..1);
        let shift = Mods {
            shift: true,
            ..Mods::default()
        };
        assert_eq!(key(&mut held, Key::Left, shift), Done::Moved);
        assert_eq!(
            key(&mut held, Key::Backspace, Mods::default()),
            Done::Changed
        );
        assert_eq!(held.text, ".md");
        assert_eq!(
            key(&mut held, Key::Backspace, Mods::default()),
            Done::Passed
        );
        assert_eq!(key(&mut held, Key::Left, Mods::default()), Done::Passed);
        assert_eq!(key(&mut held, Key::Delete, Mods::default()), Done::Changed);
        assert_eq!(held.text, "md");
    }

    #[test]
    fn a_fill_with_no_text_keeps_the_text_and_one_with_no_selection_puts_the_caret_at_its_end() {
        // The page says bytes: `é` is two, a space one, `ü` two.
        let mut held = Held::holding("é ü");
        held.fill(&Fill::selection(Range { start: 9, end: 2 }));
        assert_eq!(held.selection(), 1..3);
        // An end inside a character is where the character starts.
        held.fill(&Fill::selection(Range { start: 1, end: 4 }));
        assert_eq!(held.selection(), 0..2);
        held.fill(&Fill::text("ab"));
        assert_eq!((held.text.as_str(), held.selection()), ("ab", 2..2));
        held.insert("ç");
        assert_eq!(held.text, "abç");
    }

    #[cfg(feature = "shot")]
    mod drawn {
        use super::super::*;
        use crate::shot::Fonts;

        const NAME: Id = Id::new(7);

        /// A field of `kind` with its room at (10, 20) points, 100 across and 24 high.
        fn field(kind: FieldKind, hint: Option<&str>) -> Field {
            Field {
                id: NAME,
                kind,
                rect: Rect::new(10.0, 20.0, 100.0, 24.0),
                hint: hint.map(str::to_owned),
                label: None,
                covered: false,
            }
        }

        fn holding(text: &str) -> HashMap<Id, Held> {
            HashMap::from([(NAME, Held::holding(text))])
        }

        /// The field `field` holding `held`, drawn at twice its points, `focus` with the keyboard.
        fn draw(field: &Field, held: &HashMap<Id, Held>, focus: bool, zoom: f32) -> Vec<Paint> {
            let focus = focus.then_some(NAME);
            drawn(
                std::slice::from_ref(field),
                held,
                focus,
                zoom,
                2.0,
                &Fonts::none(),
            )
        }

        fn runs(items: &[Paint]) -> Vec<&Run> {
            items
                .iter()
                .filter_map(|item| match item {
                    Paint::Text(run) => Some(run),
                    _ => None,
                })
                .collect()
        }

        fn grounds(items: &[Paint]) -> Vec<&Quad> {
            items
                .iter()
                .filter_map(|item| match item {
                    Paint::Quad(quad) => Some(quad),
                    _ => None,
                })
                .collect()
        }

        fn face(zoom: f32) -> Font {
            Font {
                family: Family::Terminal,
                size: ZOOM_TEXT * zoom * 2.0,
                weight: 400,
                italic: false,
            }
        }

        fn wide(text: &str, zoom: f32) -> f32 {
            Fonts::none().measure(&face(zoom), text).width
        }

        #[test]
        fn a_field_says_what_it_holds_in_the_middle_of_its_room_in_the_terminals_face() {
            let items = draw(
                &field(FieldKind::Line, Some("Title")),
                &holding("Fix it"),
                false,
                1.0,
            );
            let [run] = runs(&items)[..] else {
                panic!("one run: {items:?}")
            };
            assert_eq!(run.text, "Fix it");
            assert_eq!(run.font, face(1.0));
            assert_eq!(run.color, Color::from(Role::Foreground));
            let room = Rect::new(20.0, 40.0, 200.0, 48.0);
            assert_eq!(run.clip, room);
            let line = Fonts::none().measure(&face(1.0), "M").height;
            assert_eq!(run.origin, Point::new(20.0, 40.0 + (48.0 - line) / 2.0));
            // Not the field with the keyboard, nothing selected: no caret, no selection.
            assert!(grounds(&items).is_empty(), "{items:?}");
            // At twice the page's size, twice the text.
            let items = draw(
                &field(FieldKind::Line, None),
                &holding("Fix it"),
                false,
                2.0,
            );
            assert_eq!(runs(&items)[0].font, face(2.0));
        }

        #[test]
        fn an_empty_field_says_its_hint_quietly_and_one_with_none_says_nothing() {
            let items = draw(
                &field(FieldKind::Line, Some("Title")),
                &HashMap::new(),
                false,
                1.0,
            );
            let [run] = runs(&items)[..] else {
                panic!("one run: {items:?}")
            };
            assert_eq!(
                (run.text.as_str(), run.color),
                ("Title", Color::from(Role::Quiet))
            );
            assert!(draw(&field(FieldKind::Line, None), &holding(""), false, 1.0).is_empty());
            // A field the page covers is not drawn, whatever it holds.
            let covered = Field {
                covered: true,
                ..field(FieldKind::Line, Some("Title"))
            };
            assert!(draw(&covered, &holding("Fix it"), true, 1.0).is_empty());
        }

        #[test]
        fn the_field_with_the_keyboard_has_its_caret_and_any_field_its_selection() {
            let items = draw(&field(FieldKind::Line, None), &holding("Fix it"), true, 1.0);
            let [caret] = grounds(&items)[..] else {
                panic!("a caret: {items:?}")
            };
            assert_eq!(caret.fill, Some(Color::from(Role::Cursor)));
            assert_eq!(caret.rect.x, 20.0 + wide("Fix it", 1.0));
            // Its hint, with the caret at its start, where it holds nothing.
            let items = draw(
                &field(FieldKind::Line, Some("Title")),
                &HashMap::new(),
                true,
                1.0,
            );
            assert_eq!(runs(&items)[0].text, "Title");
            assert_eq!(grounds(&items)[0].rect.x, 20.0);
            // What is selected is under the words, and no caret is drawn.
            let mut held = Held::holding("Fix it");
            held.fill(&Fill::selection(4..6));
            let items = draw(
                &field(FieldKind::Line, None),
                &HashMap::from([(NAME, held)]),
                true,
                1.0,
            );
            let [selected] = grounds(&items)[..] else {
                panic!("the selection: {items:?}")
            };
            assert_eq!(selected.fill, Some(Color::from(Role::Selection)));
            assert_eq!(selected.rect.x, 20.0 + wide("Fix ", 1.0));
            assert_eq!(selected.rect.w, wide("it", 1.0));
            assert!(
                matches!(items[0], Paint::Quad(_)),
                "under the words: {items:?}"
            );
        }

        #[test]
        fn a_field_with_the_keyboard_scrolls_to_keep_its_caret_in_its_room() {
            let long = "a title much longer than the room it is typed in";
            let items = draw(&field(FieldKind::Line, None), &holding(long), true, 1.0);
            let run = runs(&items)[0];
            let caret = grounds(&items)[0];
            assert!(run.origin.x < 20.0, "{items:?}");
            assert!(caret.rect.right() <= 220.0 + 0.01, "{items:?}");
            assert!(caret.rect.right() > 220.0 - 2.0, "{items:?}");
            // One without the keyboard shows its start.
            let items = draw(&field(FieldKind::Line, None), &holding(long), false, 1.0);
            assert_eq!(runs(&items)[0].origin.x, 20.0);
        }

        #[test]
        fn a_secret_is_a_dot_for_each_character_and_its_words_are_drawn_nowhere() {
            let items = draw(
                &field(FieldKind::Secret, None),
                &holding("lin_api_key"),
                true,
                1.0,
            );
            assert_eq!(runs(&items)[0].text, "\u{2022}".repeat(11));
            assert!(
                runs(&items).iter().all(|run| !run.text.contains("lin")),
                "{items:?}"
            );
        }

        #[test]
        fn lines_stand_from_the_top_of_their_room_a_line_of_the_text_to_a_line() {
            let items = draw(
                &field(FieldKind::Lines, None),
                &holding("one\ntwo"),
                true,
                1.0,
            );
            let line = Fonts::none().measure(&face(1.0), "M").height;
            let drawn: Vec<(&str, Point)> = runs(&items)
                .iter()
                .map(|run| (run.text.as_str(), run.origin))
                .collect();
            assert_eq!(
                drawn,
                [
                    ("one", Point::new(20.0, 40.0)),
                    ("two", Point::new(20.0, 40.0 + line))
                ]
            );
            assert!(grounds(&items).is_empty(), "{items:?}");
        }

        #[test]
        fn a_page_with_no_field_has_nothing_drawn_over_it() {
            assert!(drawn(&[], &HashMap::new(), None, 1.0, 2.0, &Fonts::none()).is_empty());
        }
    }
}
