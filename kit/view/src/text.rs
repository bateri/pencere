//! Text, as the engine sees it: a font, a string and the size the host says it takes. The
//! engine has no fonts of its own — the host shapes and measures (it is also what draws, so
//! what was measured is what appears) and answers through [`Measure`]. It has no images of its own
//! either: the host says how large the image of a path is.

use crate::{Id, Rect, Size};

/// The face a line is set in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    /// The system's interface font.
    System,
    /// The terminal's own face: the page reads as part of the terminal it sits in.
    Terminal,
}

/// A font as the host is asked for it: the size is in **pixels** (the engine multiplies the
/// points a page is written in by the scale), the weight on the CSS scale (100–900,
/// 400 regular, 700 bold), and whether it is the face's italic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Font {
    pub family: Family,
    pub size: f32,
    pub weight: u16,
    pub italic: bool,
}

impl Eq for Font {}

impl std::hash::Hash for Font {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.family.hash(state);
        self.size.to_bits().hash(state);
        self.weight.hash(state);
        self.italic.hash(state);
    }
}

/// What a line of text takes, in pixels: the width of the line and the height of its line
/// box (ascent, descent and leading).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Metrics {
    pub width: f32,
    pub height: f32,
}

/// The size of the kit's text where a page does not say one, points: the size of a text field of
/// the host's too, at the page's own size (a host whose text is this large draws a page at its
/// size).
pub const ZOOM_TEXT: f32 = 12.5;

/// The font of a text field of the host's where a page is laid out at `scale`: the terminal's
/// face, regular, at [`ZOOM_TEXT`] points. What [`field_line_height`] asks the host for.
pub fn field_font(scale: f32) -> Font {
    Font {
        family: Family::Terminal,
        size: ZOOM_TEXT * scale,
        weight: 400,
        italic: false,
    }
}

/// How tall one line of a text field of the host's is where a page is laid out at `scale`,
/// pixels: what the host says ([`Measure::field_line`]), or the height of a line it measures in
/// the field's font where it says nothing.
pub fn field_line_height(measure: &mut dyn Measure, scale: f32) -> f32 {
    let font = field_font(scale);
    measure
        .field_line(&font)
        .unwrap_or_else(|| measure.measure(&font, "M").height)
}

/// The rows `text` is broken into where a room is `budget` across (in the unit of `width`, which is
/// the width of a run of text set in one line), as a field of lines breaks its text: at its line
/// breaks, and where a word does not fit on a row at its start, so that each row is as many words as
/// fit and a word wider than a row is broken where it must be (one character, at the least, to a
/// row). A space is kept where it stands and ends its row where the row is broken there (it hangs
/// past the edge, it does not break a row of its own); the line breaks are between the rows. Each row is
/// a range of bytes of `text`, and there is at least one (an empty text is a row of nothing).
pub fn break_rows(
    text: &str,
    budget: f32,
    width: &mut dyn FnMut(&str) -> f32,
) -> Vec<std::ops::Range<usize>> {
    let budget = budget + FITS;
    let mut rows = Vec::new();
    let mut start = 0;
    for paragraph in text.split('\n') {
        let end = start + paragraph.len();
        break_paragraph(text, start..end, budget, width, &mut rows);
        start = end + 1;
    }
    rows
}

/// The rows of one paragraph (no line break in it) of `text`, put in `rows`.
fn break_paragraph(
    text: &str,
    span: std::ops::Range<usize>,
    budget: f32,
    width: &mut dyn FnMut(&str) -> f32,
    rows: &mut Vec<std::ops::Range<usize>>,
) {
    let part = |from: usize, to: usize| text.get(from..to).unwrap_or("");
    let before = rows.len();
    let mut from = span.start;
    let mut at = span.start;
    while at < span.end {
        // The word at `at` and the spaces after it, which go with it.
        let rest = part(at, span.end);
        let word_end = at + rest.find(' ').unwrap_or(rest.len());
        let tail = part(word_end, span.end);
        let token_end = word_end + (tail.len() - tail.trim_start_matches(' ').len());
        if width(part(from, word_end)) <= budget {
            at = token_end;
            continue;
        }
        if from < at {
            // The word goes to the next row.
            rows.push(from..at);
            from = at;
            continue;
        }
        // The word alone is wider than a row: broken where it must be.
        let mut cut = from;
        for (index, c) in part(from, word_end).char_indices() {
            let next = from + index + c.len_utf8();
            if cut > from && width(part(from, next)) > budget {
                break;
            }
            cut = next;
        }
        rows.push(from..cut);
        from = cut;
        at = cut;
    }
    if from < span.end || rows.len() == before {
        rows.push(from..span.end);
    }
}

/// The host's side of text: how large one line is.
///
/// A host with no proportional text answers [`Metrics::default`] (nothing): the boxes of the
/// page are laid out and drawn and its words are not.
pub trait Measure {
    /// The size of `text` set in one line in `font`.
    fn measure(&mut self, font: &Font, text: &str) -> Metrics;

    /// The size in **pixels of the image itself** of the image at `src` (the path of a file of
    /// the view's bundle), or `None` while it is not known: the image is still being decoded, is
    /// not a file the host can read as an image, or is one that was refused. The engine takes one
    /// point to a pixel. The host that has an image asked for here lays the page out again once
    /// it knows it; until then the element has the room its style gives it and draws nothing.
    ///
    /// `page` is the page of the file the element shows, from 1 (a PDF has many; for an image that
    /// has one, any other is `None`, as a page the file does not have is).
    ///
    /// A host with no images answers `None` (the default).
    fn image(&mut self, _src: &str, _page: u32) -> Option<Size> {
        None
    }

    /// How tall one line of a text field of the host's is, set in `font` (the field's, which the
    /// engine names: [`field_font`]), in the unit of [`Measure::measure`]; `None` where the host
    /// has not said, and the engine takes the height of a line it measures in `font`. A page
    /// that says how many lines a field is as tall as ([`crate::Element::field_rows`]) gets a box
    /// of that many of these: the host that draws the field is the one that knows what its line
    /// is, and a line measured by the engine's own measure (the page's) can differ from it, by
    /// a little that adds up with every line and cuts the last.
    fn field_line(&mut self, _font: &Font) -> Option<f32> {
        None
    }

    /// Where across `text` set in one line in `font` each of the places `at` is (bytes into the
    /// text, each where a character starts or at its end), pixels from the start of the line: what an
    /// editor places its carets and the characters a person reads (graphemes) at. A place that is not
    /// one of the text is where the text ends.
    ///
    /// By default the start of the text up to each place is measured ([`Measure::measure`]); a host
    /// that shapes a line once and knows where each part of it went answers from that.
    fn offsets(&mut self, font: &Font, text: &str, at: &[usize]) -> Vec<f32> {
        at.iter()
            .map(|place| match text.get(..*place) {
                Some("") => 0.0,
                Some(start) => self.measure(font, start).width,
                None => self.measure(font, text).width,
            })
            .collect()
    }

    /// What the host's editor named `id` has laid out of its document ([`crate::Element::editor`]),
    /// from its last layout: where its caret is and how large its content is. The engine makes
    /// the room scroll that far and stands what is anchored to the caret ([`Id::caret`]) beside
    /// it. The room's size is decided in the same layout, so what is said here is of the room as it
    /// was: an editor whose room changed size says what it comes to the next time.
    ///
    /// A host with no editors answers `None` (the default): the room scrolls nothing and the caret
    /// is nowhere.
    fn editor(&mut self, _id: Id) -> Option<EditorMetrics> {
        None
    }

    /// The gaps the host's editor named `id` keeps open for the page ([`crate::edit::Zone`]), each
    /// by the zone's name and its box in the content (pixels, from the top-left corner of the
    /// content as it is when nothing is scrolled): what an element of the page that says it is a
    /// zone's is put in. A host with no editors answers none.
    fn zones(&mut self, _id: Id) -> Vec<(Id, Rect)> {
        Vec::new()
    }

    /// The ranges of the editor named `id` that the page named ([`crate::edit::Mark::Anchor`]): each
    /// by its [`Id::anchor`] and its box in the content (pixels, as the caret's: from the top-left
    /// corner of the content as it is when nothing is scrolled). A host with no editors answers
    /// none.
    fn anchors(&mut self, _id: Id) -> Vec<(Id, Rect)> {
        Vec::new()
    }
}

/// What an editor has laid out of its document ([`Measure::editor`]), pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EditorMetrics {
    /// The main caret's box, from the top-left corner of the content as it is when nothing is
    /// scrolled; `None` where it is not on a line that shows (one that is folded away).
    pub caret: Option<Rect>,
    /// How large the content is: what the room scrolls over.
    pub content: Size,
}

/// How much wider than its room a line may come out and still fit, pixels: what adding up the
/// same widths in another order can make of the sum, so that a line measured to fit a box is
/// drawn in one line there. Far less than can be seen.
pub(crate) const FITS: f32 = 1.0 / 64.0;

/// The lines `text` takes in `budget` pixels, broken where it has spaces (and at its line
/// breaks): each line is as many words as fit, and a word longer than a line is broken where it
/// must be. `width` is the width of a run of text set in one line. A budget of nothing gives
/// each word a line of its own, which is how narrow the text can be.
pub(crate) fn wrap_lines(
    text: &str,
    budget: f32,
    width: &mut dyn FnMut(&str) -> f32,
) -> Vec<String> {
    let budget = budget + FITS;
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let joined = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if width(&joined) <= budget {
                line = joined;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // The word alone: on a line of its own, or broken where it must be.
            let mut rest = word;
            while width(rest) > budget {
                let fits = rest
                    .char_indices()
                    .skip(1)
                    .map(|(at, _)| at)
                    .take_while(|&at| width(&rest[..at]) <= budget)
                    .last();
                let Some(at) = fits else {
                    // Not even one character fits: it has the line to itself.
                    break;
                };
                lines.push(rest[..at].to_owned());
                rest = &rest[at..];
            }
            line = rest.to_owned();
        }
        lines.push(line);
    }
    lines
}

/// Where a line stands in the box it is given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}
