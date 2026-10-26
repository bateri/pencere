//! Text of more than one look: a paragraph whose parts are set apart — a word in bold, a name in
//! the accent, a path in the terminal's face on a ground of its own, a link that answers a click —
//! and that wraps as one text does, the parts running on from line to line ([`rich`]).
//!
//! A part ([`Span`]) says only what it changes: what it leaves out is the element's, as the words
//! of a plain [`crate::text`] take theirs from the boxes they are in. The size is the element's for
//! every part, so that the lines are one height and the words of a line stand on one line.

use crate::layout::TextLine;
use crate::{Color, Family, Font, Id, Measure};

/// One part of a text of many looks: its words and what it changes of the element's look.
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub text: String,
    pub color: Option<Color>,
    /// On the CSS scale (400 regular, 700 bold).
    pub weight: Option<u16>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strike: Option<bool>,
    pub family: Option<Family>,
    /// A ground behind the words of the part alone, as a highlight or the box of a piece of code.
    pub background: Option<Color>,
    /// The part answers the pointer as an element that is pressed does ([`crate::Element::press`]),
    /// by this name, and a screen reader says it is a link.
    pub link: Option<Id>,
}

/// A part of a text of many looks: `words`, in the element's look until told otherwise.
pub fn span(words: impl Into<String>) -> Span {
    Span {
        text: words.into(),
        color: None,
        weight: None,
        italic: None,
        underline: None,
        strike: None,
        family: None,
        background: None,
        link: None,
    }
}

impl Span {
    #[must_use]
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = Some(color.into());
        self
    }

    #[must_use]
    pub fn weight(mut self, weight: u16) -> Self {
        self.weight = Some(weight);
        self
    }

    #[must_use]
    pub fn bold(self) -> Self {
        self.weight(700)
    }

    #[must_use]
    pub fn italic(mut self) -> Self {
        self.italic = Some(true);
        self
    }

    #[must_use]
    pub fn underline(mut self) -> Self {
        self.underline = Some(true);
        self
    }

    #[must_use]
    pub fn strike(mut self) -> Self {
        self.strike = Some(true);
        self
    }

    #[must_use]
    pub fn family(mut self, family: Family) -> Self {
        self.family = Some(family);
        self
    }

    /// A ground behind the words of this part.
    #[must_use]
    pub fn bg(mut self, color: impl Into<Color>) -> Self {
        self.background = Some(color.into());
        self
    }

    /// The part is a link named `id`: it answers the pointer, and it is underlined unless it says
    /// otherwise.
    #[must_use]
    pub fn link(mut self, id: Id) -> Self {
        self.link = Some(id);
        if self.underline.is_none() {
            self.underline = Some(true);
        }
        self
    }

    /// The part's look over the element's.
    pub(crate) fn over(&self, base: &TextLine) -> Look {
        Look {
            font: Font {
                family: self.family.unwrap_or(base.font.family),
                weight: self.weight.map_or(base.font.weight, |w| w.clamp(100, 900)),
                italic: self.italic.unwrap_or(base.font.italic),
                size: base.font.size,
            },
            color: self.color.unwrap_or(base.color),
            underline: self.underline.unwrap_or(base.underline),
            strike: self.strike.unwrap_or(base.strike),
            background: self.background,
            link: self.link,
        }
    }
}

/// What a part of a text comes to, set over its element's look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Look {
    pub font: Font,
    pub color: Color,
    pub underline: bool,
    pub strike: bool,
    pub background: Option<Color>,
    pub link: Option<Id>,
}

/// A piece of a line of a rich text, placed: its words, which part they are of, and where they
/// stand against the element's box, pixels.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Piece {
    pub text: String,
    pub span: usize,
    pub x: f32,
    pub width: f32,
}

/// One line of a rich text: its pieces, and how wide they are together.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RichLine {
    pub pieces: Vec<Piece>,
    pub width: f32,
}

/// What a rich text is made of, for breaking it into lines.
#[derive(Clone, Debug)]
enum Item<'a> {
    /// A word: the runs of each part it is made of, with nothing between them (`**bold**,` is
    /// one word of two parts). A line does not break inside it unless it is longer than a line.
    Word(Vec<(usize, &'a str)>),
    /// The spaces between two words, of a part.
    Space(usize, &'a str),
    /// A line break.
    Break,
}

/// The words, spaces and line breaks of `spans`, in order.
fn items(spans: &[Span]) -> Vec<Item<'_>> {
    let mut out: Vec<Item<'_>> = Vec::new();
    for (index, span) in spans.iter().enumerate() {
        let mut lines = span.text.split('\n').peekable();
        while let Some(line) = lines.next() {
            let mut rest = line;
            while !rest.is_empty() {
                let space = rest.starts_with(char::is_whitespace);
                let end = rest
                    .char_indices()
                    .find(|(_, c)| c.is_whitespace() != space)
                    .map_or(rest.len(), |(at, _)| at);
                let (run, after) = rest.split_at(end);
                if space {
                    out.push(Item::Space(index, run));
                } else if let Some(Item::Word(word)) = out.last_mut() {
                    // Straight after a word of the part before: the same word.
                    word.push((index, run));
                } else {
                    out.push(Item::Word(vec![(index, run)]));
                }
                rest = after;
            }
            if lines.peek().is_some() {
                out.push(Item::Break);
            }
        }
    }
    out
}

/// The lines `spans` take in `budget` pixels where `wrap` (one line for each of its line breaks
/// where not), every piece measured in the font of its part. Lines break between words, and a word
/// longer than a line where it must; the spaces a line broke at are on neither line.
pub(crate) fn lay_lines(
    spans: &[Span],
    looks: &[Look],
    budget: f32,
    wrap: bool,
    measure: &mut dyn FnMut(&Font, &str) -> f32,
) -> Vec<RichLine> {
    let mut lines = vec![RichLine::default()];
    // Whether the last line began where the one before it wrapped (not at a line break).
    let mut wrapped = false;
    for item in items(spans) {
        match item {
            Item::Break => {
                trim_end(lines.last_mut(), looks, measure);
                lines.push(RichLine::default());
                wrapped = false;
            }
            Item::Space(span, text) => {
                let empty = lines.last().is_none_or(|line| line.pieces.is_empty());
                if wrap && empty && wrapped {
                    continue;
                }
                if let Some(look) = looks.get(span) {
                    let width = measure(&look.font, text);
                    append(&mut lines, span, text, width);
                }
            }
            Item::Word(runs) => {
                let widths: Vec<f32> = runs
                    .iter()
                    .map(|(span, text)| looks.get(*span).map_or(0.0, |l| measure(&l.font, text)))
                    .collect();
                let total: f32 = widths.iter().sum();
                let (used, empty) = lines
                    .last()
                    .map_or((0.0, true), |line| (line.width, line.pieces.is_empty()));
                if wrap && !empty && used + total > budget {
                    trim_end(lines.last_mut(), looks, measure);
                    lines.push(RichLine::default());
                    wrapped = true;
                }
                for ((span, text), width) in runs.into_iter().zip(widths) {
                    let used = lines.last().map_or(0.0, |line| line.width);
                    if !wrap || used + width <= budget {
                        append(&mut lines, span, text, width);
                        continue;
                    }
                    // Longer than a line: broken where it must be.
                    let Some(font) = looks.get(span).map(|look| look.font) else {
                        continue;
                    };
                    let mut rest = text;
                    loop {
                        let used = lines.last().map_or(0.0, |line| line.width);
                        let room = budget - used;
                        let rest_width = measure(&font, rest);
                        if rest_width <= room {
                            append(&mut lines, span, rest, rest_width);
                            break;
                        }
                        let fits = rest
                            .char_indices()
                            .skip(1)
                            .map(|(at, _)| at)
                            .take_while(|&at| measure(&font, &rest[..at]) <= room)
                            .last();
                        match fits {
                            Some(at) => {
                                let head = &rest[..at];
                                let head_width = measure(&font, head);
                                append(&mut lines, span, head, head_width);
                                rest = &rest[at..];
                            }
                            // Not one character fits on a line that has something on it: the
                            // rest goes on the next; on an empty one it has the line to itself.
                            None if used > 0.0 => {}
                            None => {
                                append(&mut lines, span, rest, rest_width);
                                break;
                            }
                        }
                        lines.push(RichLine::default());
                        wrapped = true;
                    }
                }
            }
        }
    }
    trim_end(lines.last_mut(), looks, measure);
    lines
}

/// Puts `text` of the part `span` at the end of the last line; a piece of the same part as the one
/// before it is one piece with it.
fn append(lines: &mut [RichLine], span: usize, text: &str, width: f32) {
    let Some(line) = lines.last_mut() else { return };
    match line.pieces.last_mut() {
        Some(last) if last.span == span => {
            last.text.push_str(text);
            last.width += width;
        }
        _ => line.pieces.push(Piece {
            text: text.to_owned(),
            span,
            x: line.width,
            width,
        }),
    }
    line.width += width;
}

/// Takes the spaces off the end of `line`: they are where it broke, not part of it.
fn trim_end(
    line: Option<&mut RichLine>,
    looks: &[Look],
    measure: &mut dyn FnMut(&Font, &str) -> f32,
) {
    let Some(line) = line else { return };
    while let Some(last) = line.pieces.last_mut() {
        let trimmed = last.text.trim_end();
        if trimmed.len() == last.text.len() {
            break;
        }
        if trimmed.is_empty() {
            line.pieces.pop();
            continue;
        }
        let Some(look) = looks.get(last.span) else {
            break;
        };
        last.text.truncate(trimmed.len());
        last.width = measure(&look.font, &last.text);
        break;
    }
    line.width = line.pieces.last().map_or(0.0, |last| last.x + last.width);
}

/// The longest start of `line` that fits `budget` pixels with a "…" after it, cut inside the piece
/// where it runs over. The "…" is in the look of the piece it ends.
pub(crate) fn cut_line(
    line: &RichLine,
    looks: &[Look],
    budget: f32,
    measure: &mut dyn Measure,
) -> RichLine {
    const ELLIPSIS: char = '\u{2026}';
    let mut out = RichLine {
        pieces: Vec::new(),
        width: 0.0,
    };
    for piece in &line.pieces {
        let Some(look) = looks.get(piece.span) else {
            continue;
        };
        let ellipsis = measure.measure(&look.font, &ELLIPSIS.to_string()).width;
        if piece.x + piece.width + ellipsis <= budget || piece.x + piece.width <= budget {
            // Whole, unless what comes after it is cut and there is no room for the "…" here: the
            // next piece puts it on.
            out.pieces.push(piece.clone());
            out.width = piece.x + piece.width;
            continue;
        }
        // This piece runs over: as much of it as fits with the "…".
        let room = budget - piece.x;
        let ends: Vec<usize> = piece
            .text
            .char_indices()
            .map(|(at, c)| at + c.len_utf8())
            .collect();
        let mut best = None;
        for end in ends.iter().rev() {
            let mut candidate = piece
                .text
                .get(..*end)
                .unwrap_or_default()
                .trim_end()
                .to_owned();
            candidate.push(ELLIPSIS);
            let width = measure.measure(&look.font, &candidate).width;
            if width <= room {
                best = Some((candidate, width));
                break;
            }
        }
        let (text, width) = best.unwrap_or_else(|| (ELLIPSIS.to_string(), ellipsis));
        if width <= room || out.pieces.is_empty() {
            out.pieces.push(Piece {
                text,
                span: piece.span,
                x: piece.x,
                width,
            });
            out.width = piece.x + width;
        } else if let Some(last) = out.pieces.last_mut() {
            // Not even the "…" fits after where this piece starts: it ends the one before.
            last.text.push(ELLIPSIS);
            last.width += ellipsis;
            out.width = last.x + last.width;
        }
        return out;
    }
    out
}

/// The words of a rich text, as one string.
pub(crate) fn words(spans: &[Span]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}
