//! The cells of a terminal as a host of pencere's native views.
//!
//! A page of the kit hands out a draw list: boxes, lines of text, pictures and canvases, in pixels.
//! This crate draws that list into the characters of a terminal, for a program whose terminal does
//! not host pencere's views: the page the program carries is the interface it shows there.
//!
//! The crate does no I/O. It measures text in cells ([`Cells`]), turns a draw list into a grid of
//! cells and the places of the pictures ([`render`]), and writes the grid as the bytes a terminal
//! takes ([`Frame::ansi`]) or as plain text ([`Frame::text`]).
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
mod grid;
mod measure;
mod output;
mod paint;
mod pictures;
mod theme;

pub use color::Rgb;
pub use grid::{Attrs, Bg, Cell, Grid};
pub use measure::{Cells, char_width, text_width};
pub use output::Placement;
pub use paint::{CanvasMode, Frame, ImageMode, Options, Stats, render};
pub use pictures::{NoPictures, Pictures, Raster};
pub use theme::Theme;
