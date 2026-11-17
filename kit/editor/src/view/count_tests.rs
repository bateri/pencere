//! The rows of a document counted again after a key: what that costs must not grow with the length
//! of the document, and the width of its widest line is what the lines say.

use std::cell::Cell;

use pencere_view::edit::{Change, Folds};
use pencere_view::{Font, Measure, Metrics, Rect};

use super::View;
use crate::buffer::Buffer;
use crate::text::LINE_SEARCHES;

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
