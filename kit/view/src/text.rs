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
    /// A host with no images answers `None` (the default).
    fn image(&mut self, _src: &str) -> Option<Size> {
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

/// The lines `text` takes in `budget` pixels, broken where it has spaces (and at its line
/// breaks): each line is as many words as fit, and a word longer than a line is broken where it
/// must be. `width` is the width of a run of text set in one line. A budget of nothing gives
/// each word a line of its own, which is how narrow the text can be.
pub(crate) fn wrap_lines(
    text: &str,
    budget: f32,
    width: &mut dyn FnMut(&str) -> f32,
) -> Vec<String> {
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
