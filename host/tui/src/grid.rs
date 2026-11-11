//! The cells a page is drawn into.

use crate::color::Rgb;

/// What a character is drawn with besides its colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Attrs(u8);

impl Attrs {
    pub const NONE: Self = Self(0);
    pub const BOLD: Self = Self(1);
    pub const ITALIC: Self = Self(2);
    pub const UNDERLINE: Self = Self(4);
    pub const STRIKE: Self = Self(8);

    /// Whether every attribute of `other` is set.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0 && other.0 != 0
    }

    /// These and `other` together.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// The ground of a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bg {
    Rgb(Rgb),
    /// The terminal's own: where a picture the terminal draws shows through.
    Default,
}

/// One cell of the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    /// What is drawn: a space for a cell of ground, a half block where the two halves of the cell
    /// are two grounds.
    pub ch: char,
    pub fg: Rgb,
    pub bg: Bg,
    pub attrs: Attrs,
    /// The right half of a character that is two columns wide: its left half has the character.
    pub tail: bool,
}

/// The cells of a page, row by row from the top-left corner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid {
    cols: u16,
    rows: u16,
    cells: Vec<Cell>,
}

impl Grid {
    pub(crate) fn new(cols: u16, rows: u16, cells: Vec<Cell>) -> Self {
        Self { cols, rows, cells }
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    /// The cell at `col`, `row`, from 0; `None` outside the grid.
    pub fn cell(&self, col: u16, row: u16) -> Option<&Cell> {
        if col >= self.cols || row >= self.rows {
            return None;
        }
        self.cells
            .get(usize::from(row) * usize::from(self.cols) + usize::from(col))
    }

    /// The cell at `col`, `row`, from 0, to change; `None` outside the grid.
    pub(crate) fn cell_mut(&mut self, col: u16, row: u16) -> Option<&mut Cell> {
        if col >= self.cols || row >= self.rows {
            return None;
        }
        self.cells
            .get_mut(usize::from(row) * usize::from(self.cols) + usize::from(col))
    }

    /// The rows, each of its cells.
    pub fn rows_of_cells(&self) -> impl Iterator<Item = &[Cell]> {
        self.cells.chunks(usize::from(self.cols).max(1))
    }

    /// The characters of the grid, a line to a row with the spaces at its end left off: what the
    /// terminal shows but for the colours.
    pub fn text(&self) -> String {
        let mut text = String::new();
        for row in self.rows_of_cells() {
            let line: String = row
                .iter()
                .filter(|cell| !cell.tail)
                .map(|cell| cell.ch)
                .collect();
            text.push_str(line.trim_end());
            text.push('\n');
        }
        text
    }
}
