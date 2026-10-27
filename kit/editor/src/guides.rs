//! The guides of the indent ([`pencere_view::edit::EditorOptions::indent_guides`]): how many a line
//! shows, and the block the main caret is in, as VS Code works them out (its `getLinesIndentGuides`
//! and `getActiveIndentGuide`, for a language that is not off-side).
//!
//! Like the regions that fold by the indent, they are worked out from the text as it is when the
//! lines are drawn, and never kept, so they are never out of date; and no more of a document is
//! read for them than the lines drawn and the nearest lines with text about them.

use std::cmp::Ordering;

use crate::buffer::Buffer;
use crate::indent::BLANK_REACH;

/// How far from the main caret's line the look for the lines of its block goes, lines: where the
/// caret is out of view, a block that reaches the view from further away than that is not drawn as
/// the caret's.
const ACTIVE_REACH: usize = 10_000;

/// How many guides the lines show, worked out as each is asked for.
pub(crate) struct Levels<'a> {
    buffer: &'a Buffer,
    /// How many columns a level of indent is.
    tab: usize,
    /// The last run of lines with no text asked about: its first line and its last, and how many
    /// guides each of them shows (they all show as many), so that the lines of a run asked for one
    /// after the other look for the lines with text about them once.
    blank: Option<(usize, usize, usize)>,
}

impl<'a> Levels<'a> {
    pub fn new(buffer: &'a Buffer) -> Self {
        Self {
            buffer,
            tab: buffer.tab_columns(),
            blank: None,
        }
    }

    /// How many guides line `line` shows: one for each level its indent reaches into; on a line
    /// with no text, as many as the nearest lines with text above and below it give (none at the
    /// top or the bottom of the document, nor in a run of more than [`BLANK_REACH`] lines with no
    /// text).
    pub fn of(&mut self, line: usize) -> usize {
        if let Some(columns) = self.buffer.indent_of(line) {
            return columns.div_ceil(self.tab);
        }
        if let Some((first, last, level)) = self.blank
            && (first..=last).contains(&line)
        {
            return level;
        }
        let lines = self.buffer.len_lines();
        let with_text = |n: usize| self.buffer.indent_of(n).map(|columns| (n, columns));
        let above = (line.saturating_sub(BLANK_REACH + 1)..line)
            .rev()
            .find_map(with_text);
        let below = (line + 1..lines.min(line + BLANK_REACH + 2)).find_map(with_text);
        // The lines looked over have no text: they are the run's, and so is what was not looked
        // past, in a run too long to show guides.
        let first = above.map_or(line.saturating_sub(BLANK_REACH + 1), |(n, _)| n + 1);
        let last = below.map_or(
            (line + BLANK_REACH + 1).min(lines.saturating_sub(1)),
            |(n, _)| n.saturating_sub(1),
        );
        let level = match (above, below) {
            (Some((a, above)), Some((b, below))) if b - a - 1 <= BLANK_REACH => {
                self.between(above, below)
            }
            _ => 0,
        };
        self.blank = Some((first, last, level));
        level
    }

    /// How many guides a line with no text shows between a line `above` columns in and one `below`
    /// columns in.
    fn between(&self, above: usize, below: usize) -> usize {
        match above.cmp(&below) {
            // In the block the line above opens.
            Ordering::Less => 1 + above / self.tab,
            Ordering::Equal => below.div_ceil(self.tab),
            // In the block the line below ends.
            Ordering::Greater => 1 + below / self.tab,
        }
    }
}

/// The guide of the block the main caret's line `caret` is in: which of the lines' guides it is
/// (the first is 0), and the first and the last line it is drawn on, looked for over the lines
/// `top..=bottom` in view (and no further than [`ACTIVE_REACH`] lines from the caret's). As VS Code
/// does: a line that opens a block (the line after it shows a guide more) has the block's guide,
/// one that ends a block (the line before it shows a guide more) the block's it ends, any other its
/// own last guide; the block goes on over the lines that show that guide. None where the line shows
/// no guide and opens and ends no block.
pub(crate) fn active_guide(
    levels: &mut Levels<'_>,
    caret: usize,
    top: usize,
    bottom: usize,
) -> Option<(usize, usize, usize)> {
    let lines = levels.buffer.len_lines();
    let here = levels.of(caret);
    let (mut first, mut last, level, up, down) =
        if caret + 1 < lines && levels.of(caret + 1) == here + 1 {
            (caret + 1, caret + 1, here + 1, false, true)
        } else if caret > 0 && levels.of(caret - 1) == here + 1 {
            (caret - 1, caret - 1, here + 1, true, false)
        } else if here > 0 {
            (caret, caret, here, true, true)
        } else {
            return None;
        };
    while up && first > top && caret - first < ACTIVE_REACH && levels.of(first - 1) >= level {
        first -= 1;
    }
    while down
        && last < bottom
        && last + 1 < lines
        && last - caret < ACTIVE_REACH
        && levels.of(last + 1) >= level
    {
        last += 1;
    }
    Some((level - 1, first, last))
}
