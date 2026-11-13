//! The cells of a terminal as a host of pencere's native views.
//!
//! A page of the kit hands out a draw list: boxes, lines of text, pictures and canvases, in pixels.
//! This crate draws that list into the characters of a terminal, for a program whose terminal does
//! not host pencere's views: the page the program carries is the interface it shows there.
//!
//! The drawing does no I/O. It measures text in cells ([`Cells`]), turns a draw list into a grid of
//! cells and the places of the pictures ([`render`]), and writes the grid as the bytes a terminal
//! takes ([`Frame::ansi`], or only what changed, [`Screen`]) or as plain text ([`Frame::text`]).
//!
//! # A program's own screen
//!
//! [`serve`] is the program library's `serve` with this crate's screen behind it: where no terminal
//! hosts the program's view (or the person forces it, `PENCERE_TUI=1`) the program's `lua` view is
//! run in this process and drawn in the terminal the program was started in. The terminal is taken
//! as the library takes it for any view, with the keys of the line discipline that would get in the
//! way (flow control, suspend, quit, the next character taken literally) off and ⌃C left an
//! interrupt, and given back the same way: at the end, on a signal, in a panic. It is asked what it
//! can do (its colours, the Kitty graphics and keyboard protocols) and it is drawn for the weakest:
//! 256 colours, no pictures but half blocks, the keys the old encodings send.
//!
//! # The grid
//!
//! The page is laid out on the cells: the measure ([`Cells`]) says how many pixels a column is across
//! and a row is down ([`pencere_view::Measure::grid`]), and the engine rounds every length the page
//! writes to whole columns across and whole rows down. A line of text is a row, whatever its size,
//! and a character is as many columns as it takes in a terminal ([`char_width`]). What is left for
//! this crate is to draw boxes that stand on cell edges: a box that is written thinner than half a
//! cell is an eighth block at the edge of its cell, and a thin upright line over a letter reverses
//! the cell, so it is a caret and not a bar over the word.

mod color;
mod field;
mod grid;
mod input;
mod live;
mod measure;
mod memory;
mod output;
mod overlay;
mod paint;
mod pictures;
mod probe;
mod screen;
mod theme;
mod zlib;

pub use color::Rgb;
pub use field::{show_hints, show_texts};
pub use grid::{Attrs, Bg, Cell, Grid};
pub use live::{KIT_VERSION, run};
pub use measure::{Cells, char_width, text_width};
pub use output::{Depth, Placement};
pub use paint::{CanvasMode, Frame, ImageMode, Link, Options, Stats, render};
pub use pictures::{NoPictures, Pictures, Raster};
pub use screen::Screen;
pub use theme::Theme;

/// Serves the program's views as [`pencere::serve`] does, and where no terminal hosts a view shows
/// the program's `lua` view on its own terminal: `options` carry the screen ([`pencere::Options::screen`]).
///
/// # Errors
///
/// As [`pencere::serve`].
pub fn serve(options: pencere::Options) -> Result<pencere::Outcome, pencere::Error> {
    pencere::serve(options.screen(run))
}
