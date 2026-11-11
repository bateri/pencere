//! Text measured in cells.

use pencere_view::{Font, Grid, Measure, Metrics};
use unicode_width::UnicodeWidthChar;

/// How many columns `c` takes: a letter one, a Chinese character or an emoji two, a combining mark
/// or a control character none. The one width the measure and the painter both use: a line is as
/// wide as the characters it is drawn with come to, so a width taken another way (a whole string
/// at once, where an emoji sequence is one glyph) would draw a line over what its box holds.
pub fn char_width(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}

/// How many columns `text` takes in one line.
pub fn text_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

/// The measure of a grid of cells: a column is `col` pixels of the draw list across and a row `row`
/// pixels down, and a line of text, whatever its font, is as many columns as its characters take
/// ([`char_width`]) and one row. It tells the engine the grid ([`Measure::grid`]), so that the page
/// is laid out on the cells.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cells {
    /// The pixels a column is across.
    pub col: f32,
    /// The pixels a row is down.
    pub row: f32,
}

impl Cells {
    pub const fn new(col: f32, row: f32) -> Self {
        Self { col, row }
    }
}

impl Measure for Cells {
    fn measure(&mut self, _font: &Font, text: &str) -> Metrics {
        Metrics {
            width: text_width(text) as f32 * self.col,
            height: self.row,
        }
    }

    fn offsets(&mut self, _font: &Font, text: &str, at: &[usize]) -> Vec<f32> {
        at.iter()
            .map(|place| {
                let start = text.get(..*place).unwrap_or(text);
                text_width(start) as f32 * self.col
            })
            .collect()
    }

    fn grid(&self) -> Option<Grid> {
        Grid::new(self.col, self.row)
    }
}
