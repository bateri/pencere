//! Nothing a page sends breaks the editor: thousands of changes, selections, decorations, loads
//! and compositions with places picked at random, most of them not the document's, and each answer
//! is a change done or a refusal.

// A test fails by panicking, and indexes what it has just read.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use pencere_editor::Buffer;
use pencere_view::edit::{
    Change, Decorate, Decoration, EditCommand, EditorOptions, Mark, Movement, Range,
};

/// A small generator of numbers that are the same every run.
struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % (n.max(1) as u64)) as usize
    }

    /// A place: mostly in the document, sometimes past its end.
    fn place(&mut self, len: usize) -> usize {
        if self.below(8) == 0 {
            len + self.below(10)
        } else {
            self.below(len + 1)
        }
    }

    fn text(&mut self) -> String {
        const PIECES: [&str; 8] = ["a", "ğ", "😀", "\n", "e\u{301}", "  ", "(", "\t"];
        (0..self.below(4))
            .map(|_| PIECES[self.below(PIECES.len())])
            .collect()
    }
}

#[test]
fn nothing_a_page_sends_breaks_the_editor() {
    let mut dice = Dice(0x9e37_79b9_7f4a_7c15);
    let mut buffer = Buffer::new("start\nof the\ntext 😀").expect("a small text");
    buffer.set_options(EditorOptions {
        pairs: vec![('(', ')'), ('"', '"')],
        comment: Some("--".into()),
        ..EditorOptions::default()
    });
    for round in 0..5_000 {
        let now = round as f64 * 0.05;
        let len = buffer.len_chars();
        let version = buffer.version();
        let base = version.saturating_sub(dice.below(6) as u64) + dice.below(2) as u64;
        match dice.below(12) {
            0 => {
                let (a, b) = (dice.place(len), dice.place(len));
                let _ = buffer.change(base, vec![Change::new(a, b, dice.text())], now);
            }
            1 => {
                let ranges = (0..1 + dice.below(3))
                    .map(|_| Range::new(dice.place(len), dice.place(len)))
                    .collect();
                buffer.select(ranges, dice.below(4));
            }
            2 => {
                let ranges = vec![Range::new(dice.place(len), dice.place(len))];
                let _ = buffer.select_at(base, ranges, 0);
            }
            3 => {
                let (from, to) = (dice.place(len), dice.place(len));
                let decorations = (0..dice.below(5))
                    .map(|_| {
                        let mark = if dice.below(2) == 0 {
                            Mark::Fold
                        } else {
                            Mark::Gutter(pencere_view::Role::Red.into())
                        };
                        Decoration::new(dice.place(len), dice.place(len), mark)
                    })
                    .collect();
                let _ = buffer.decorate(Decorate {
                    layer: format!("layer {}", dice.below(20)),
                    version: base,
                    from,
                    to,
                    decorations,
                });
            }
            4 => buffer.type_text(&dice.text(), now),
            5 => {
                let text = dice.text();
                let n = text.chars().count();
                let replace = (dice.below(3) == 0).then(|| (dice.place(len), dice.place(len)));
                buffer.set_marked(&text, (dice.below(n + 3), dice.below(n + 3)), replace, now);
            }
            6 => match dice.below(3) {
                0 => buffer.unmark(),
                1 => buffer.abort_marked(now),
                _ => buffer.insert(&dice.text(), None, now),
            },
            7 => {
                const COMMANDS: [EditCommand; 14] = [
                    EditCommand::DeleteBackward,
                    EditCommand::DeleteForward,
                    EditCommand::DeleteWordBackward,
                    EditCommand::Indent,
                    EditCommand::Outdent,
                    EditCommand::MoveLinesUp,
                    EditCommand::MoveLinesDown,
                    EditCommand::DuplicateLines,
                    EditCommand::ToggleComment,
                    EditCommand::Undo,
                    EditCommand::Redo,
                    EditCommand::ToggleFold,
                    EditCommand::SelectNext,
                    EditCommand::Newline,
                ];
                buffer.command(COMMANDS[dice.below(COMMANDS.len())].clone(), now);
            }
            8 => {
                const MOVES: [Movement; 8] = [
                    Movement::Left,
                    Movement::Right,
                    Movement::WordLeft,
                    Movement::WordRight,
                    Movement::Up,
                    Movement::Down,
                    Movement::LineStart,
                    Movement::Bracket,
                ];
                let movement = MOVES[dice.below(MOVES.len())];
                buffer.command(
                    EditCommand::Move {
                        movement,
                        extend: dice.below(2) == 0,
                    },
                    now,
                );
            }
            9 => {
                let _ = buffer.load(&dice.text(), dice.below(2) == 0);
            }
            10 => {
                let _ = buffer.utf16_of(dice.place(len));
                let _ = buffer.place_of_utf16(dice.place(len * 2));
                let _ = buffer.matching_bracket(dice.place(len));
                let _ = buffer.cut(now);
            }
            _ => {
                let pattern = dice.text();
                buffer.command(
                    EditCommand::Find {
                        pattern,
                        regex: dice.below(2) == 0,
                        case: dice.below(2) == 0,
                        word: dice.below(2) == 0,
                    },
                    now,
                );
                let command = match dice.below(6) {
                    0 => EditCommand::FindNext,
                    1 => EditCommand::Replace(dice.text()),
                    2 => EditCommand::ReplaceAll(format!("${{1}}{}$0", dice.text())),
                    3 => EditCommand::SelectFound,
                    4 => EditCommand::SelectOccurrences,
                    _ => EditCommand::FindPrevious,
                };
                buffer.command(command, now);
                buffer.search_again();
            }
        }
        let _ = buffer.take_notes();
        // What the editor holds stays a document: its selections are in it, in order.
        let len = buffer.len_chars();
        let ranges = buffer.ranges();
        assert!(!ranges.is_empty());
        assert!(
            ranges
                .iter()
                .all(|range| range.anchor <= len && range.head <= len)
        );
        assert!(
            ranges
                .windows(2)
                .all(|pair| pair[0].end() <= pair[1].start())
        );
        for (from, to) in buffer.folded() {
            assert!(from < to && *to <= len);
        }
        for (_, item) in buffer.decorations_in(0, len) {
            assert!(item.from <= item.to && item.to <= len);
        }
    }
    // And it was changed, again and again, along the way.
    assert!(buffer.version() > 500, "only {} versions", buffer.version());
}
