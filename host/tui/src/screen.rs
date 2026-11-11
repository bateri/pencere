//! The frames of a live terminal: only the cells that changed are written.

use std::fmt::Write;

use crate::grid::{Cell, Grid};
use crate::output::{self, Depth, Placement, Sgr};
use crate::paint::Frame;

/// How many unchanged cells between two changed ones are written over rather than skipped: moving
/// the cursor costs about that many bytes.
const GAP: usize = 4;

/// What the terminal shows, so that the next frame is written as the difference.
#[derive(Debug)]
pub struct Screen {
    depth: Depth,
    shown: Option<Grid>,
    placed: Vec<Placement>,
    /// Where the cursor was left, once it is known (`Some(None)`: hidden).
    cursor: Option<Option<(u16, u16)>>,
}

impl Screen {
    /// A screen whose contents are not known: the first frame draws all of it.
    #[must_use]
    pub fn new(depth: Depth) -> Self {
        Self {
            depth,
            shown: None,
            placed: Vec::new(),
            cursor: None,
        }
    }

    /// What the terminal shows is not what was written (it was resized, or another program had it):
    /// the next frame draws all of it.
    pub fn forget(&mut self) {
        self.shown = None;
        self.placed.clear();
        self.cursor = None;
    }

    /// What is to be written for `frame` (and the cursor at `cursor`, a cell from 0, or hidden):
    /// nothing where the terminal shows it already.
    #[must_use]
    pub fn draw(&mut self, frame: &Frame, cursor: Option<(u16, u16)>) -> Vec<u8> {
        let now = &frame.grid;
        let before = self
            .shown
            .take()
            .filter(|grid| grid.cols() == now.cols() && grid.rows() == now.rows());
        let mut body = String::new();
        let mut state = Sgr::new(self.depth);
        for (row, cells) in now.rows_of_cells().enumerate() {
            let old = before
                .as_ref()
                .and_then(|grid| grid.rows_of_cells().nth(row));
            spans(
                &mut body,
                &mut state,
                u16::try_from(row).unwrap_or(0),
                cells,
                old,
            );
        }
        let pictures = frame.placements != self.placed;
        let moved = self.cursor != Some(cursor);
        self.shown = Some(now.clone());
        self.cursor = Some(cursor);
        if before.is_some() && body.is_empty() && !pictures && !moved {
            return Vec::new();
        }
        // Hidden while it is drawn, so that it does not flicker over the cells it passes.
        let mut out = String::from("\x1b[?25l");
        if before.is_none() {
            out.push_str("\x1b[0m\x1b[2J");
        }
        out.push_str(&body);
        out.push_str("\x1b[0m");
        if pictures {
            out.push_str("\x1b_Ga=d,d=A,q=2\x1b\\");
            for placement in &frame.placements {
                let _ = write!(out, "\x1b[{};{}H", placement.row + 1, placement.col + 1);
                output::kitty(&mut out, placement);
            }
            self.placed.clone_from(&frame.placements);
        }
        if let Some((col, row)) = cursor {
            let _ = write!(out, "\x1b[{};{}H\x1b[?25h", row + 1, col + 1);
        }
        out.into_bytes()
    }
}

/// The cells of `row` that differ from `old` (all, where there is none), as runs.
fn spans(out: &mut String, state: &mut Sgr, row: u16, now: &[Cell], old: Option<&[Cell]>) {
    let same = |at: usize| match (old, now.get(at)) {
        (Some(old), Some(cell)) => old.get(at) == Some(cell),
        _ => false,
    };
    let mut at = 0;
    while at < now.len() {
        if same(at) {
            at += 1;
            continue;
        }
        let mut start = at;
        // A change in the right half of a wide character is the character's.
        if now.get(start).is_some_and(|cell| cell.tail) {
            start = start.saturating_sub(1);
        }
        let mut end = at;
        let mut next = at + 1;
        while next < now.len() {
            if !same(next) {
                end = next;
                next += 1;
                continue;
            }
            let ahead = (next..now.len().min(next + GAP)).find(|index| !same(*index));
            match ahead {
                Some(index) => {
                    end = index;
                    next = index + 1;
                }
                None => break,
            }
        }
        let _ = write!(out, "\x1b[{};{}H", row + 1, start + 1);
        for cell in now.get(start..=end).unwrap_or_default() {
            if cell.tail {
                continue;
            }
            state.set(out, cell);
            out.push(cell.ch);
        }
        at = end + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Options, Theme, render};
    use pencere_view::DrawList;

    fn frame(cols: u16, rows: u16) -> Frame {
        render(
            &DrawList::default(),
            cols,
            rows,
            &Theme::dark(),
            &crate::NoPictures,
            &Options::default(),
        )
    }

    #[test]
    fn a_frame_that_is_the_same_writes_nothing() {
        let mut screen = Screen::new(Depth::Indexed);
        let one = frame(10, 3);
        let first = screen.draw(&one, Some((1, 1)));
        assert!(first.len() > 30);
        assert!(screen.draw(&one, Some((1, 1))).is_empty());
        // The cursor alone moved: it is all that is written.
        let moved = String::from_utf8(screen.draw(&one, Some((2, 1)))).unwrap();
        assert!(moved.ends_with("\x1b[2;3H\x1b[?25h") && !moved.contains("\x1b[2J"));
    }

    #[test]
    fn a_new_size_clears_and_draws_all() {
        let mut screen = Screen::new(Depth::True);
        let _ = screen.draw(&frame(10, 3), None);
        let bytes = String::from_utf8(screen.draw(&frame(12, 3), None)).unwrap();
        assert!(bytes.contains("\x1b[2J"));
    }

    #[test]
    fn a_colour_is_one_of_256_where_the_terminal_has_no_more() {
        let mut screen = Screen::new(Depth::Indexed);
        let bytes = String::from_utf8(screen.draw(&frame(2, 1), Some((1, 0)))).unwrap();
        assert!(bytes.contains("48;5;"), "{bytes:?}");
        assert!(!bytes.contains("38;2;") && !bytes.contains("48;2;"));
        assert!(bytes.ends_with("\x1b[1;2H\x1b[?25h"));
    }
}
