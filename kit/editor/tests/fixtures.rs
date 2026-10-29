//! The examples of `fixtures/`, run: each a text, the steps a person, a page and an input method take
//! on it, and what the editor is to hold after them (`fixtures/README.md` is the form).

// A test fails by panicking, and indexes what it has just read.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};

use pencere_editor::{Buffer, Note};
use pencere_view::edit::{
    Change, CommentTokens, Decorate, Decoration, EditCommand, EditorOptions, Folds, Found,
    GutterMark, IndentRules, Look, Mark, Movement, Origin, Range, Refusal,
};
use pencere_view::{Color, Role};
use serde_json::Value;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

#[test]
fn every_example_of_the_fixtures_holds() {
    let mut files: Vec<PathBuf> = std::fs::read_dir(fixtures())
        .unwrap_or_else(|error| panic!("fixtures: {error}"))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no fixtures");
    let mut ran = 0;
    for file in files {
        let source = std::fs::read_to_string(&file).unwrap_or_else(|error| panic!("{error}"));
        let doc: Value = serde_json::from_str(&source)
            .unwrap_or_else(|error| panic!("{}: {error}", file.display()));
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        for example in doc["examples"]
            .as_array()
            .unwrap_or_else(|| panic!("{name}: no examples"))
        {
            run(&name, example);
            ran += 1;
        }
    }
    eprintln!("{ran} examples");
}

fn run(file: &str, example: &Value) {
    let about = format!("{file}: {}", example["about"].as_str().unwrap_or("?"));
    let mut buffer = Buffer::new(example["text"].as_str().unwrap_or_default())
        .unwrap_or_else(|refusal| panic!("{about}: {refusal:?}"));
    buffer.set_options(options(&example["options"]));
    if let Some(room) = example["room"].as_u64() {
        buffer.set_room(room as usize);
    }
    let mut now = 0.0;
    for (index, step) in example["steps"]
        .as_array()
        .unwrap_or_else(|| panic!("{about}: no steps"))
        .iter()
        .enumerate()
    {
        let here = format!("{about}, step {index}");
        if let Some(at) = step["at"].as_f64() {
            now = at;
        }
        let action = step["do"]
            .as_str()
            .unwrap_or_else(|| panic!("{here}: no `do`"));
        for _ in 0..step["repeat"].as_u64().unwrap_or(1) {
            match action {
                "select" => buffer.select(ranges(&step["ranges"]), main(step)),
                "type" => buffer.type_text(text(step), now),
                "paste" => buffer.paste(text(step), now),
                "tab" => buffer.tab(now),
                "cut" => {
                    let cut = buffer.cut(now);
                    if let Some(want) = step["gives"].as_str() {
                        assert_eq!(cut.as_deref(), Some(want), "{here}: cut");
                    }
                }
                "copy" => {
                    assert_eq!(
                        buffer.copy_text().as_deref(),
                        step["gives"].as_str(),
                        "{here}: copy"
                    );
                }
                "command" => buffer.command(command(step, &here), now),
                "change" => {
                    let _ = buffer.change_as(
                        version(step, "base"),
                        changes(&step["changes"]),
                        now,
                        step["history"].as_bool().unwrap_or(true),
                        step["tag"].as_str().map(str::to_owned),
                    );
                }
                "page_select" => {
                    let _ = buffer.select_at(
                        version(step, "version"),
                        ranges(&step["ranges"]),
                        main(step),
                    );
                }
                "decorate" => {
                    let _ = buffer.decorate(Decorate {
                        layer: step["layer"].as_str().unwrap_or_default().to_owned(),
                        version: version(step, "version"),
                        from: number(&step["from"]),
                        to: number(&step["to"]),
                        decorations: decorations(&step["decorations"], &here),
                    });
                }
                "load" => {
                    let _ = buffer.load(text(step), step["more"].as_bool().unwrap_or(false));
                }
                "compose" => {
                    let selected = step["selected"]
                        .as_array()
                        .map(|pair| (number(&pair[0]), number(&pair[1])))
                        .unwrap_or((0, 0));
                    let replace = step["replace"]
                        .as_array()
                        .map(|pair| (number(&pair[0]), number(&pair[1])));
                    buffer.set_marked(text(step), selected, replace, now);
                }
                "commit" => buffer.unmark(),
                "commit_text" => buffer.insert(text(step), None, now),
                "abort" => buffer.abort_marked(now),
                "frame" => {
                    buffer.search_again();
                }
                "expect" => expect(&mut buffer, step, &here),
                other => panic!("{here}: no step `{other}`"),
            }
        }
    }
}

fn expect(buffer: &mut Buffer, want: &Value, here: &str) {
    let notes = buffer.take_notes();
    if let Some(text) = want["text"].as_str() {
        assert_eq!(buffer.text(), text, "{here}: text");
    }
    if !want["ranges"].is_null() {
        assert_eq!(
            buffer.ranges(),
            ranges(&want["ranges"]).as_slice(),
            "{here}: ranges"
        );
    }
    if let Some(main) = want["main"].as_u64() {
        assert_eq!(buffer.selection().main, main as usize, "{here}: main");
    }
    if let Some(version) = want["version"].as_u64() {
        assert_eq!(buffer.version(), version, "{here}: version");
    }
    if let Some(composing) = want["composing"].as_bool() {
        assert_eq!(buffer.composing(), composing, "{here}: composing");
    }
    if let Some(marked) = want.get("marked") {
        let expected = marked
            .as_array()
            .map(|pair| (number(&pair[0]), number(&pair[1])));
        assert_eq!(buffer.marked_range(), expected, "{here}: marked");
    }
    if let Some(layers) = want["decorations"].as_object() {
        for (layer, items) in layers {
            let held: Vec<(usize, usize, String)> = held_decorations(buffer, layer);
            let expected: Vec<(usize, usize, String)> = items
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .map(|item| {
                            (
                                number(&item[0]),
                                number(&item[1]),
                                item[2].as_str().unwrap_or_default().to_owned(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            assert_eq!(held, expected, "{here}: layer {layer}");
        }
    }
    if !want["folded"].is_null() {
        assert_eq!(
            buffer.folded(),
            pairs(&want["folded"]).as_slice(),
            "{here}: folded"
        );
    }
    if !want["folds"].is_null() {
        let offered: Vec<(usize, usize)> = (0..buffer.len_lines())
            .filter_map(|line| buffer.fold_region(line))
            .collect();
        assert_eq!(offered, pairs(&want["folds"]), "{here}: folds");
        // What the gutter asks of the lines is what folds from them.
        let lines: Vec<usize> = offered
            .iter()
            .map(|(from, _)| buffer.line_of(*from))
            .collect();
        assert_eq!(
            buffer.fold_lines(0, buffer.len_lines().saturating_sub(1)),
            lines,
            "{here}: the lines that fold"
        );
    }
    if !want["hidden"].is_null() {
        assert_eq!(
            buffer.hidden_lines(),
            pairs(&want["hidden"]),
            "{here}: hidden lines"
        );
    }
    if !want["found"].is_null() {
        assert_eq!(
            buffer.found(),
            pairs(&want["found"]).as_slice(),
            "{here}: found"
        );
    }
    if let Some(conversions) = want["utf16"].as_array() {
        for pair in conversions {
            let (place, units) = (number(&pair[0]), number(&pair[1]));
            assert_eq!(
                buffer.utf16_of(place),
                Some(units),
                "{here}: utf-16 of {place}"
            );
            assert_eq!(
                buffer.place_of_utf16(units),
                Some(place),
                "{here}: place of {units}"
            );
        }
    }
    if let Some(bracket) = want.get("bracket") {
        let at = number(&bracket[0]);
        let expected = bracket[1].as_u64().map(|to| (at, to as usize));
        let found = buffer.matching_bracket(at).map(|(_, to)| (at, to));
        assert_eq!(found, expected, "{here}: bracket at {at}");
    }
    if let Some(expected) = want["notes"].as_array() {
        let expected: Vec<Note> = expected.iter().map(|note| note_of(note, here)).collect();
        // A selection note, and one of what is found, say the version too: the one the editor is
        // at.
        let notes: Vec<Note> = notes
            .into_iter()
            .map(|note| match note {
                Note::Selection(mut selection) => {
                    selection.version = 0;
                    Note::Selection(selection)
                }
                Note::Found(mut found) => {
                    found.version = 0;
                    Note::Found(found)
                }
                other => other,
            })
            .collect();
        assert_eq!(notes, expected, "{here}: notes");
    }
}

fn held_decorations(buffer: &Buffer, layer: &str) -> Vec<(usize, usize, String)> {
    // The layers are told apart by the colour of their marks in the examples: each example's
    // layer is named, and its marks are found by a range as wide as the document.
    let len = buffer.len_chars();
    let names = layer_names(buffer);
    let Some(index) = names.iter().position(|name| name == layer) else {
        return Vec::new();
    };
    let mut out: Vec<(usize, usize, String)> = buffer
        .decorations_in(0, len)
        .into_iter()
        .filter(|(at, _)| *at == index)
        .map(|(_, item)| (item.from, item.to, kind_of(&item.mark).to_owned()))
        .collect();
    out.sort();
    out
}

/// The names of the buffer's layers, in order: read back from what a decoration of each carries.
fn layer_names(buffer: &Buffer) -> Vec<String> {
    buffer.layer_names()
}

fn kind_of(mark: &Mark) -> &'static str {
    match mark {
        Mark::Style(_) => "style",
        Mark::Line(_) => "line",
        Mark::Gutter(_) => "gutter",
        Mark::Fold => "fold",
        Mark::Ruler(_) => "ruler",
        Mark::Zone(_) => "zone",
        Mark::Anchor(_) => "anchor",
        Mark::Widget(_) => "widget",
    }
}

fn note_of(note: &Value, here: &str) -> Note {
    if let Some(edited) = note.get("edited") {
        return Note::Edited(pencere_view::edit::Edited {
            version: edited["version"].as_u64().unwrap_or(0),
            changes: changes(&edited["changes"]),
            origin: Origin::named(edited["origin"].as_str().unwrap_or("person"))
                .unwrap_or_else(|| panic!("{here}: no origin")),
            tag: edited["tag"].as_str().map(str::to_owned),
        });
    }
    if let Some(selection) = note.get("selection") {
        return Note::Selection(pencere_view::edit::Selection {
            ranges: ranges(&selection["ranges"]),
            main: selection["main"].as_u64().unwrap_or(0) as usize,
            version: 0,
            composing: selection["composing"].as_bool().unwrap_or(false),
        });
    }
    if let Some(refused) = note.get("refused").and_then(Value::as_str) {
        return Note::Refused(
            Refusal::named(refused).unwrap_or_else(|| panic!("{here}: no refusal `{refused}`")),
        );
    }
    if let Some(found) = note.get("found") {
        return Note::Found(Found {
            version: 0,
            count: number(&found["count"]),
            current: found["current"].as_u64().map(|current| current as usize),
            more: found["more"].as_bool().unwrap_or(false),
        });
    }
    panic!("{here}: a note is `edited`, `selection`, `refused` or `found`")
}

fn options(value: &Value) -> EditorOptions {
    let mut options = EditorOptions::default();
    if let Some(width) = value["tab_width"].as_u64() {
        options.tab_width = width as u8;
    }
    if let Some(hard) = value["hard_tabs"].as_bool() {
        options.hard_tabs = hard;
    }
    if let Some(read_only) = value["read_only"].as_bool() {
        options.read_only = read_only;
    }
    match &value["comment"] {
        Value::String(line) => options.comment = CommentTokens::line(line.as_str()),
        Value::Object(tokens) => {
            options.comment = CommentTokens {
                line: tokens
                    .get("line")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                block: tokens.get("block").and_then(Value::as_array).map(|pair| {
                    (
                        pair[0].as_str().unwrap_or_default().to_owned(),
                        pair[1].as_str().unwrap_or_default().to_owned(),
                    )
                }),
            };
        }
        _ => {}
    }
    let rule = |name: &str| value["indent"][name].as_str().map(str::to_owned);
    options.indent = IndentRules {
        increase: rule("increase"),
        decrease: rule("decrease"),
    };
    if value["folds"].as_str() == Some("indent") {
        options.folds = Folds::Indent;
    }
    if let Some(pairs) = value["pairs"].as_array() {
        options.pairs = pairs
            .iter()
            .filter_map(|pair| {
                let mut chars = pair.as_str()?.chars();
                Some((chars.next()?, chars.next()?))
            })
            .collect();
    }
    options
}

fn command(step: &Value, here: &str) -> EditCommand {
    let name = step["command"].as_str().unwrap_or_default();
    match name {
        "move" => EditCommand::Move {
            movement: movement(step["movement"].as_str().unwrap_or_default(), here),
            extend: step["extend"].as_bool().unwrap_or(false),
        },
        "insert" => EditCommand::Insert(text(step).to_owned()),
        "newline" => EditCommand::Newline,
        "delete_backward" => EditCommand::DeleteBackward,
        "delete_forward" => EditCommand::DeleteForward,
        "delete_word_backward" => EditCommand::DeleteWordBackward,
        "delete_word_forward" => EditCommand::DeleteWordForward,
        "indent" => EditCommand::Indent,
        "outdent" => EditCommand::Outdent,
        "move_lines_up" => EditCommand::MoveLinesUp,
        "move_lines_down" => EditCommand::MoveLinesDown,
        "duplicate_lines" => EditCommand::DuplicateLines,
        "toggle_comment" => EditCommand::ToggleComment,
        "toggle_block_comment" => EditCommand::ToggleBlockComment,
        "select_all" => EditCommand::SelectAll,
        "add_caret_above" => EditCommand::AddCaretAbove,
        "add_caret_below" => EditCommand::AddCaretBelow,
        "select_next" => EditCommand::SelectNext,
        "single_selection" => EditCommand::SingleSelection,
        "undo" => EditCommand::Undo,
        "redo" => EditCommand::Redo,
        "fold" => EditCommand::Fold,
        "unfold" => EditCommand::Unfold,
        "toggle_fold" => EditCommand::ToggleFold,
        "unfold_all" => EditCommand::UnfoldAll,
        "fold_all" => EditCommand::FoldAll,
        "delete_lines" => EditCommand::DeleteLines,
        "select_line" => EditCommand::SelectLine,
        "duplicate_lines_up" => EditCommand::DuplicateLinesUp,
        "join_lines" => EditCommand::JoinLines,
        "insert_blank_line" => EditCommand::InsertBlankLine,
        "split_line" => EditCommand::SplitLine,
        "transpose_chars" => EditCommand::TransposeChars,
        "delete_to_line_start" => EditCommand::DeleteToLineStart,
        "delete_to_line_end" => EditCommand::DeleteToLineEnd,
        "find" => EditCommand::Find {
            pattern: step["pattern"].as_str().unwrap_or_default().to_owned(),
            regex: step["regex"].as_bool().unwrap_or(false),
            case: step["case"].as_bool().unwrap_or(true),
            word: step["word"].as_bool().unwrap_or(false),
        },
        "find_next" => EditCommand::FindNext,
        "find_previous" => EditCommand::FindPrevious,
        "stop_finding" => EditCommand::StopFinding,
        "replace" => EditCommand::Replace(text(step).to_owned()),
        "replace_all" => EditCommand::ReplaceAll(text(step).to_owned()),
        "select_found" => EditCommand::SelectFound,
        "select_occurrences" => EditCommand::SelectOccurrences,
        other => panic!("{here}: no command `{other}`"),
    }
}

fn movement(name: &str, here: &str) -> Movement {
    match name {
        "left" => Movement::Left,
        "right" => Movement::Right,
        "word_left" => Movement::WordLeft,
        "word_right" => Movement::WordRight,
        "up" => Movement::Up,
        "down" => Movement::Down,
        "page_up" => Movement::PageUp,
        "page_down" => Movement::PageDown,
        "line_start" => Movement::LineStart,
        "line_end" => Movement::LineEnd,
        "document_start" => Movement::DocumentStart,
        "document_end" => Movement::DocumentEnd,
        "bracket" => Movement::Bracket,
        other => panic!("{here}: no movement `{other}`"),
    }
}

fn decorations(value: &Value, here: &str) -> Vec<Decoration> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let mark = match item[2].as_str().unwrap_or("style") {
                        "style" => Mark::Style(Look {
                            color: Some(Color::from(Role::Accent)),
                            ..Look::default()
                        }),
                        "line" => Mark::Line(Color::from(Role::Error)),
                        "gutter" => Mark::Gutter(GutterMark::dot(Color::from(Role::Warning))),
                        "fold" => Mark::Fold,
                        other => panic!("{here}: no mark `{other}`"),
                    };
                    Decoration::new(number(&item[0]), number(&item[1]), mark)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn ranges(value: &Value) -> Vec<Range> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|pair| Range::new(number(&pair[0]), number(&pair[1])))
                .collect()
        })
        .unwrap_or_default()
}

fn pairs(value: &Value) -> Vec<(usize, usize)> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|pair| (number(&pair[0]), number(&pair[1])))
                .collect()
        })
        .unwrap_or_default()
}

fn changes(value: &Value) -> Vec<Change> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    Change::new(
                        number(&item[0]),
                        number(&item[1]),
                        item[2].as_str().unwrap_or_default(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn number(value: &Value) -> usize {
    value.as_u64().unwrap_or(0) as usize
}

fn version(step: &Value, key: &str) -> u64 {
    step[key].as_u64().unwrap_or(0)
}

fn main(step: &Value) -> usize {
    step["main"].as_u64().unwrap_or(0) as usize
}

fn text(step: &Value) -> &str {
    step["text"].as_str().unwrap_or_default()
}
