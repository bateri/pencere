//! Text, as the engine sees it: a font, a string and the size the host says it takes. The
//! engine has no fonts of its own — the host shapes and measures (it is also what draws, so
//! what was measured is what appears) and answers through [`Measure`]. It has no images of its own
//! either: the host says how large the image of a path is.

use crate::Size;

/// The face a line is set in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    /// The system's interface font.
    System,
    /// The terminal's own face: the page reads as part of the terminal it sits in.
    Terminal,
}

/// A font as the host is asked for it: the size is in **pixels** (the engine multiplies the
/// points a page is written in by the scale) and the weight on the CSS scale (100–900,
/// 400 regular, 700 bold).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Font {
    pub family: Family,
    pub size: f32,
    pub weight: u16,
}

impl Eq for Font {}

impl std::hash::Hash for Font {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.family.hash(state);
        self.size.to_bits().hash(state);
        self.weight.hash(state);
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
