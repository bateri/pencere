//! The rows of a document counted again after a key: what that costs must not grow with the length
//! of the document, and the width of its widest line is what the lines say.

use std::cell::Cell;

use pencere_view::edit::{Change, Folds};
use pencere_view::{Font, Measure, Metrics, Rect};

use super::View;
use crate::buffer::Buffer;
use crate::text::{LINE_SEARCHES, LONGEST_SCANS, Text};

/// A view of a room 200 wide, that wraps where `wrap` says.
fn view_of(wrap: bool) -> View {
    View {
        wrap,
        room: Rect::new(0.0, 0.0, 200.0, 300.0),
        ..View::default()
    }
}

/// A document of `lines` lines of code with a letter typed into it, and how many times the rows
/// counted again after that looked a line up in the text.
fn searches_after_a_key(lines: usize, wrap: bool) -> usize {
    let mut buffer = Buffer::new(&"let x = 1;\n".repeat(lines)).expect("a document");
    let mut view = view_of(wrap);
    view.count(&buffer);
    let base = buffer.version();
    buffer
        .change(base, vec![Change::new(4, 4, "y")], 0.0)
        .expect("the change");
    let before = LINE_SEARCHES.with(Cell::get);
    view.count(&buffer);
    LINE_SEARCHES.with(Cell::get) - before
}

#[test]
fn counting_the_rows_again_after_a_key_looks_lines_up_as_often_in_a_long_document_as_in_a_short_one()
 {
    for wrap in [false, true] {
        let short = searches_after_a_key(20, wrap);
        let long = searches_after_a_key(2500, wrap);
        assert_eq!(
            long, short,
            "wrap {wrap}: 2500 lines looked up {long} times, 20 lines {short}"
        );
    }
}

/// Letters 7 wide, and every one of them a place to break at: the lines' own widths.
struct Even;

impl Measure for Even {
    fn measure(&mut self, _font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text.chars().count() as f32 * 7.0,
            height: 16.0,
        }
    }
}

#[test]
fn the_rows_of_a_wrapped_document_are_what_each_line_says_of_its_own() {
    // Each line asked for by itself, as the rows were counted one line at a time.
    let mut text = String::new();
    for line in 0..300 {
        match line % 7 {
            0 => {}
            1 => text.push_str("short"),
            2 => text.push_str(&"word ".repeat(line % 23 + 1)),
            3 => text.push_str("\ttabbed\tline with accénts and 😀 emoji"),
            4 => text.push_str(&"x".repeat(80)),
            5 => text.push_str("a\r"),
            _ => text.push_str(&"ab cd".repeat(15)),
        }
        text.push('\n');
    }
    text.push_str("no break at the end, and long enough to take more than one row of the room");
    let buffer = Buffer::new(&text).expect("a document");
    let mut view = view_of(true);
    // Some of the lines are laid out: their rows are where they broke and not a guess.
    for line in (0..buffer.len_lines()).step_by(3) {
        view.shape(&buffer.line(line), &mut Even);
    }
    view.count(&buffer);
    let mut top = 0;
    for line in 0..buffer.len_lines() {
        assert_eq!(view.tops[line], top, "line {line}");
        top += view.rows_of(&buffer, line, &buffer.line(line));
    }
    assert_eq!(view.tops.last().copied(), Some(top));
    assert!(top > buffer.len_lines(), "some lines take more than a row");
    // A fold takes its lines out of the rows, and the lines after it keep theirs.
    let mut folded =
        Buffer::new("a\n  b\n  c\nd long line here that wraps\n  e\nf").expect("a document");
    let mut options = folded.options().clone();
    options.folds = Folds::Indent;
    folded.set_options(options);
    assert!(folded.fold_at(0, Some(true)) && folded.fold_at(3, Some(true)));
    assert!(!folded.hidden_lines().is_empty());
    let mut view = view_of(true);
    view.count(&folded);
    let hidden = folded.hidden_lines();
    let mut top = 0;
    for line in 0..folded.len_lines() {
        assert_eq!(view.tops[line], top, "line {line}");
        if !hidden
            .iter()
            .any(|(first, last)| (*first..=*last).contains(&line))
        {
            top += view.rows_of(&folded, line, &folded.line(line));
        }
    }
}

/// How many characters the widest line of `text` has, as the view says it and as the lines' own
/// places say it (each line from its start to its end, line break not counted).
fn widest_of(text: &str) -> (usize, usize) {
    let buffer = Buffer::new(text).expect("a document");
    let mut view = View::default();
    view.count(&buffer);
    let said = view.widest.expect("counted").1;
    let by_places = (0..buffer.len_lines())
        .map(|line| buffer.line_end(line) - buffer.line_start(line))
        .max()
        .unwrap_or(0);
    (said, by_places)
}

#[test]
fn the_widest_line_is_in_characters_without_its_line_break_as_the_places_of_the_lines_say() {
    for (text, widest) in [
        ("", 0),
        ("\n", 0),
        ("\n\n\n", 0),
        ("abc", 3),
        ("abc\n", 3),
        ("a\nbcd\nef", 3),
        ("a\nbcd\nefghi", 5),
        ("héllo wörld 😀\nab", 13),
        ("ab\r\ncdef\r\n", 5),
        ("a\n\n\nb", 1),
        // Only `\n` breaks a line: a lone carriage return and the separators of Unicode do not.
        ("a\rbc\ndef", 4),
        ("a\u{2028}bcd\u{85}e\nfg", 7),
    ] {
        assert_eq!(widest_of(text), (widest, widest), "{text:?}");
    }
    // Many pieces of the rope, one line much wider than the rest.
    let mut text = String::new();
    for line in 0..3000 {
        text.push_str(&"é".repeat(line % 37));
        if line == 1500 {
            text.push_str(&"x".repeat(900));
        }
        text.push('\n');
    }
    assert_eq!(widest_of(&text), (900 + 1500 % 37, 900 + 1500 % 37));
}

#[test]
fn the_widest_line_is_counted_again_for_the_text_after_a_change() {
    let mut buffer = Buffer::new("ab\ncd").expect("a document");
    let mut view = View::default();
    view.count(&buffer);
    assert_eq!(view.widest.map(|(_, chars)| chars), Some(2));
    let base = buffer.version();
    buffer
        .change(base, vec![Change::new(5, 5, "efg")], 0.0)
        .expect("the change");
    view.count(&buffer);
    assert_eq!(view.widest.map(|(_, chars)| chars), Some(5));
}

/// A document of `lines` lines of code with a letter typed into it, and how many times every line of
/// the text was walked, after that, to find the widest.
fn scans_after_a_key(lines: usize, wrap: bool) -> usize {
    let mut buffer = Buffer::new(&"let x = 1;\n".repeat(lines)).expect("a document");
    let mut view = view_of(wrap);
    view.count(&buffer);
    let base = buffer.version();
    buffer
        .change(base, vec![Change::new(4, 4, "y")], 0.0)
        .expect("the change");
    let before = LONGEST_SCANS.with(Cell::get);
    view.count(&buffer);
    LONGEST_SCANS.with(Cell::get) - before
}

#[test]
fn counting_the_rows_again_after_a_key_does_not_walk_every_line_to_find_the_widest() {
    for wrap in [false, true] {
        for lines in [20, 2500] {
            let walks = scans_after_a_key(lines, wrap);
            assert_eq!(walks, 0, "wrap {wrap}, {lines} lines: {walks} walks");
        }
    }
}

/// How many characters the longest line of `text` has, each line from its start to its end.
fn longest_by_places(text: &Text) -> usize {
    (0..text.len_lines())
        .map(|line| text.line_end(line) - text.line_start(line))
        .max()
        .unwrap_or(0)
}

#[test]
fn the_longest_line_is_what_the_lines_say_after_any_change_that_the_text_is_given() {
    // What is put in, now and then a line break, a pair of them, a letter of two bytes, and a line
    // longer than any a document has.
    let pieces = [
        "",
        "a",
        "bc",
        "\n",
        "xyz\n",
        "\u{e9}",
        "\r\n",
        "a line that is longer than every one the document has, by far, or so it seems",
        "\n\n",
        "\u{f1}\n\u{f6}",
    ];
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move |below: usize| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((state >> 33) as usize) % below.max(1)
    };
    let mut text = Text::new(&"let x = 1;\n".repeat(200));
    let walks_before = LONGEST_SCANS.with(Cell::get);
    for step in 0..3000 {
        // Known before the change, in most steps (the walk is the other way to know it).
        if next(8) != 0 {
            text.longest_line();
        }
        let length = text.len_chars();
        let from = next(length + 1);
        // Mostly small, now and then a hundred lines or more.
        let reach = if next(40) == 0 { 1500 } else { 14 };
        let to = (from + next(reach + 1)).min(length);
        let insert = pieces[next(pieces.len())];
        assert!(text.replace(from, to, insert), "step {step}");
        assert_eq!(
            text.longest_line(),
            longest_by_places(&text),
            "step {step}: {from}..{to} became {insert:?}"
        );
    }
    // Most changes are said what they did to the longest line: the lines were walked for a few.
    let walks = LONGEST_SCANS.with(Cell::get) - walks_before;
    assert!(walks < 1500, "{walks} walks in 3000 changes");
}

#[test]
fn a_change_that_shortens_the_only_longest_line_walks_the_lines_to_find_the_next() {
    let mut text = Text::new("ab\ncdefg\nhi");
    assert_eq!(text.longest_line(), 5);
    let before = LONGEST_SCANS.with(Cell::get);
    // Another line, shorter than the longest: no walk.
    assert!(text.replace(0, 0, "x"));
    assert_eq!(text.longest_line(), 5);
    assert_eq!(LONGEST_SCANS.with(Cell::get) - before, 0);
    // The longest, made shorter than the next: a walk.
    assert!(text.replace(4, 8, ""));
    assert_eq!(text.longest_line(), 3);
    assert_eq!(LONGEST_SCANS.with(Cell::get) - before, 1);
}

#[test]
fn the_text_of_every_line_is_what_each_line_says_of_its_own() {
    // Lines in one piece of the rope, lines that run over several (a line of thousands of letters),
    // a document that ends with a line break, and one that has none, line breaks of both kinds, and
    // letters of more than a byte.
    let long = "x\u{e9}".repeat(3000);
    let big = format!("{long}\n{}\nshort\n{long}", "y".repeat(5000));
    for document in [
        "",
        "\n",
        "a",
        "a\n",
        "a\nb",
        "a\r\nb\r\n\r\n",
        "\u{f1}\n\u{f6}\u{f6}\n\n\n",
        big.as_str(),
    ] {
        let text = Text::new(document);
        let lines: Vec<String> = text.lines().map(|line| line.into_owned()).collect();
        let by_places: Vec<String> = (0..text.len_lines()).map(|line| text.line(line)).collect();
        assert_eq!(
            lines,
            by_places,
            "{:?}",
            document.chars().take(40).collect::<String>()
        );
    }
    let mut text = Text::new(&"let x = 1;\n".repeat(900));
    text.replace(5, 5, &"w".repeat(4000));
    let lines: Vec<String> = text.lines().map(|line| line.into_owned()).collect();
    let by_places: Vec<String> = (0..text.len_lines()).map(|line| text.line(line)).collect();
    assert_eq!(lines, by_places);
}
