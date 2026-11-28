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
use pencere_view::{
    Color, Font, Id, Measure, Paint, Point, Quad, Rect, Role, Run, break_rows, field_line_height,
};
use pencere_view::{FieldKind, FieldState, Fill, Key, Mods};

#[cfg(feature = "shot")]
use crate::Field;

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
/// the page: each says what it holds, in the font of its element (`Field::font`; a secret a dot for
/// each character), or its hint, quiet, where it holds nothing. The text stands in its box
/// (`Field::text`), inside the padding of the element. One line stands in the middle of its box, with
/// what is selected under its words and, in the field that has the keyboard, the caret where nothing
/// is selected, and it scrolls as a field does to keep the caret in its box (and is cut at the room, so
/// that a box shorter than a line does not cut the letters, and not at the sides of the padding);
/// lines are broken at a word where the box is full and stand from its top, a line of the text to a
/// row, with no selection or caret, and the rows past the box run on into the padding and are cut at
/// the padding box (`Field::padding_box`: the room less the border). The page is
/// laid out with the same `fonts` ([`super::Opening::measure`]), so that the line is the one its boxes
/// were made of. A field the page covers is not drawn. `fields` are the last frame's (the host's
/// points), `held` what they hold, `focus` the one with the keyboard and `scale` the view's pixels to
/// a point.
#[cfg(feature = "shot")]
pub(super) fn drawn(
    fields: &[Field],
    held: &HashMap<Id, Held>,
    focus: Option<Id>,
    scale: f32,
    fonts: &crate::shot::Fonts,
) -> Vec<Paint> {
    let mut measure = fonts.clone();
    let mut items = Vec::new();
    for field in fields.iter().filter(|field| !field.covered) {
        // The field's face and line as the page was laid out with them: the font the field says
        // (the text of its element, in the view's pixels) and the line asked of the same font.
        let font = Font {
            size: field.font.size * scale,
            ..field.font
        };
        let line = field_line_height(&mut measure, &font);
        // The text stands in its box, inside the padding of the element; the room is the pointer's.
        let room = Rect::new(
            field.text.x * scale,
            field.text.y * scale,
            field.text.w * scale,
            field.text.h * scale,
        );
        // A line is cut at the sides of its box (the padding is not for words) and the top and bottom
        // of its room, which its element made at least a line high; lines at the padding box, which
        // the rows past their box run on into, and the border is not theirs.
        let whole = Rect::new(room.x, field.rect.y * scale, room.w, field.rect.h * scale);
        let padding_box = Rect::new(
            field.padding_box.x * scale,
            field.padding_box.y * scale,
            field.padding_box.w * scale,
            field.padding_box.h * scale,
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
            // Broken at a word where the box is full, as the host's own field breaks it.
            let rows = break_rows(words, room.w, &mut |run| measure.measure(&font, run).width);
            for (row, span) in rows.iter().enumerate() {
                let words = words.get(span.clone()).unwrap_or("").trim_end_matches(' ');
                if words.is_empty() {
                    continue;
                }
                #[allow(clippy::cast_precision_loss)]
                let top = room.y + row as f32 * line;
                items.push(words_at((room.x, top), words, font, ink, padding_box));
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
            items.push(ground(rect, Role::Selection, whole));
        }
        if let Some(hint) = hint {
            items.push(words_at((room.x, top), hint, font, Role::Quiet, whole));
        } else if !text.is_empty() {
            items.push(words_at((left, top), &text, font, Role::Foreground, whole));
        }
        if has && selection.is_empty() {
            let rect = Rect::new(left + head, top, caret, line);
            items.push(ground(rect, Role::Cursor, whole));
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
        clip: pencere_view::Clip::new(clip),
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
        clip: pencere_view::Clip::new(clip),
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
        use crate::ZOOM_TEXT;
        use crate::shot::Fonts;
        use pencere_view::Family;

        const NAME: Id = Id::new(7);

        /// A field of `kind` with its room at (10, 20) points, 100 across and 24 high.
        fn field(kind: FieldKind, hint: Option<&str>) -> Field {
            Field {
                id: NAME,
                kind,
                rect: Rect::new(10.0, 20.0, 100.0, 24.0),
                padding_box: Rect::new(10.0, 20.0, 100.0, 24.0),
                text: Rect::new(10.0, 20.0, 100.0, 24.0),
                font: Font {
                    family: Family::Terminal,
                    size: ZOOM_TEXT,
                    weight: 400,
                    italic: false,
                },
                hint: hint.map(str::to_owned),
                label: None,
                covered: false,
            }
        }

        fn holding(text: &str) -> HashMap<Id, Held> {
            HashMap::from([(NAME, Held::holding(text))])
        }

        /// The field `field` holding `held`, drawn at twice its points, `focus` with the keyboard. A page
        /// at `zoom` has its fields' fonts that many times as large in the host's points.
        fn draw(field: &Field, held: &HashMap<Id, Held>, focus: bool, zoom: f32) -> Vec<Paint> {
            let focus = focus.then_some(NAME);
            let field = Field {
                font: Font {
                    size: field.font.size * zoom,
                    ..field.font
                },
                ..field.clone()
            };
            drawn(
                std::slice::from_ref(&field),
                held,
                focus,
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
        fn lines_are_broken_at_a_word_in_the_width_of_the_text_box_and_stand_from_its_top() {
            // The room is the pointer's; the text is in it, inside the element's padding: 5 points in
            // from the left and 6 down, and as wide as the first two words of the text (and a hair).
            let mut padded = field(FieldKind::Lines, None);
            padded.text = Rect::new(15.0, 26.0, (wide("aaaa bbbb", 1.0) + 1.0) / 2.0, 40.0);
            let items = draw(&padded, &holding("aaaa bbbb cccc"), false, 1.0);
            let line = Fonts::none().measure(&face(1.0), "M").height;
            let drawn: Vec<(&str, Point)> = runs(&items)
                .iter()
                .map(|run| (run.text.as_str(), run.origin))
                .collect();
            assert_eq!(
                drawn,
                [
                    ("aaaa bbbb", Point::new(30.0, 52.0)),
                    ("cccc", Point::new(30.0, 52.0 + line))
                ]
            );
            // The room has no border here: lines are cut at it, and the padding is inside it.
            assert_eq!(runs(&items)[0].clip.rect.x, 20.0, "cut at the padding box");
        }

        #[test]
        fn lines_past_their_box_run_on_into_the_padding_and_are_cut_at_the_padding_box() {
            // A room of 100 across and 70 down with a border of 1, 4 points of padding above and 26
            // below: its text stands in 38 points of it, which is not as many rows as there are.
            let mut field = field(FieldKind::Lines, None);
            field.rect = Rect::new(10.0, 20.0, 100.0, 70.0);
            field.padding_box = Rect::new(11.0, 21.0, 98.0, 68.0);
            field.text = Rect::new(14.0, 25.0, 92.0, 38.0);
            let items = draw(&field, &holding("a\nb\nc\nd\ne\nf"), true, 1.0);
            let line = Fonts::none().measure(&face(1.0), "M").height;
            let runs = runs(&items);
            assert_eq!(runs.len(), 6);
            // From the top of the text box, a row to a line of the text.
            assert_eq!(runs[0].origin, Point::new(28.0, 50.0));
            assert_eq!(runs[5].origin, Point::new(28.0, 50.0 + 5.0 * line));
            // Cut at the room less its border, whose bottom is below the text box's: the rows that
            // run over the box show in the padding, and none shows over the border.
            let cut = Rect::new(22.0, 42.0, 196.0, 136.0);
            assert!(runs.iter().all(|run| run.clip == cut), "{items:?}");
            assert!(cut.bottom() > 2.0 * field.text.bottom());
            assert!(cut.bottom() < 2.0 * field.rect.bottom());
        }

        #[test]
        fn the_hint_of_lines_is_broken_as_their_text_is() {
            let mut padded = field(FieldKind::Lines, Some("aaaa bbbb cccc"));
            padded.text = Rect::new(10.0, 20.0, (wide("aaaa bbbb", 1.0) + 1.0) / 2.0, 40.0);
            let items = draw(&padded, &holding(""), false, 1.0);
            let words: Vec<&str> = runs(&items).iter().map(|run| run.text.as_str()).collect();
            assert_eq!(words, ["aaaa bbbb", "cccc"]);
        }

        #[test]
        fn a_line_is_in_the_middle_of_the_box_of_its_text_and_not_of_its_room() {
            // One row of a field of rows, with 4 points of padding above and 6 below: the room is
            // taller than the line, the box is the line.
            let line = Fonts::none().measure(&face(1.0), "M").height;
            let mut padded = field(FieldKind::Line, None);
            padded.rect = Rect::new(10.0, 20.0, 100.0, line / 2.0 + 10.0);
            padded.text = Rect::new(18.0, 24.0, 84.0, line / 2.0);
            let items = draw(&padded, &holding("Fix it"), false, 1.0);
            let [run] = runs(&items)[..] else {
                panic!("one run: {items:?}")
            };
            assert_eq!(run.origin, Point::new(36.0, 48.0));
            // Cut at the sides of the box and the top and bottom of the room, which is taller than the
            // box: a letter of the line is not cut, and a long one does not run into the padding.
            assert_eq!(run.clip, Rect::new(36.0, 40.0, 168.0, line + 20.0));
            let long = "a title much longer than the box it is typed in, with room around it";
            for focus in [false, true] {
                let items = draw(&padded, &holding(long), focus, 1.0);
                assert!(
                    runs(&items)
                        .iter()
                        .all(|run| run.clip.rect.x == 36.0 && run.clip.rect.w == 168.0),
                    "{items:?}"
                );
                assert!(
                    grounds(&items)
                        .iter()
                        .all(|quad| quad.clip.rect.x == 36.0 && quad.clip.rect.w == 168.0),
                    "{items:?}"
                );
            }
        }

        #[test]
        fn lines_stand_from_the_top_of_their_box_a_line_of_the_text_to_a_row() {
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
        fn a_field_is_drawn_in_the_font_of_its_element_at_the_views_scale() {
            // The element says 15 points in the system's face, bold: at twice the points, 30 pixels.
            let said = Font {
                family: Family::System,
                size: 15.0,
                weight: 700,
                italic: false,
            };
            let wide = Field {
                font: said,
                ..field(FieldKind::Line, None)
            };
            let items = draw(&wide, &holding("Fix it"), false, 1.0);
            let [run] = runs(&items)[..] else {
                panic!("one run: {items:?}")
            };
            let drawn_in = Font { size: 30.0, ..said };
            assert_eq!(run.font, drawn_in);
            // In the middle of its room by the line of that font.
            let line = Fonts::none().measure(&drawn_in, "M").height;
            assert_eq!(run.origin, Point::new(20.0, 40.0 + (48.0 - line) / 2.0));
        }

        #[test]
        fn a_page_with_no_field_has_nothing_drawn_over_it() {
            assert!(drawn(&[], &HashMap::new(), None, 2.0, &Fonts::none()).is_empty());
        }
    }
}
