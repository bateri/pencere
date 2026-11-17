//! A terminal that draws pictures as a host of pencere's native views.
//!
//! A page of the kit hands out a draw list: boxes, lines of text, pictures and canvases, in pixels.
//! [`serve`] is the program library's `serve` with this crate's screen behind it: where no terminal
//! hosts the program's view the program's `lua` view is run in this process, drawn into pixels in the
//! faces of the machine, and sent to the terminal it was started in as the picture it is (the Kitty
//! graphics protocol). A terminal that does not speak that protocol is given nothing: the program
//! takes the fallback it has.
//!
//! The terminal is taken as the library takes it for any view, with the keys of the line discipline
//! that would get in the way (flow control, suspend, quit, the next character taken literally) off and
//! ⌃C left an interrupt, and given back the same way: at the end, on a signal, in a panic. It is asked
//! what it can do (the graphics protocol, the size of its cells, its colours, the keyboard protocol)
//! before the screen is left, and a terminal that cannot show the pictures is not left.
//!
//! The window is one picture the size of the terminal's cells; what a key or a wheel turn changes is
//! sent as the rectangles that changed. The page is laid out in points on the pixels of the window
//! (the scale is the one that makes a column of the page a column of the terminal), as it is in
//! bateri.

mod chord;
mod chrome;
#[cfg(test)]
mod chrome_tests;
mod color;
mod field;
mod files;
#[cfg(test)]
mod files_tests;
mod input;
mod link;
mod live;
mod memory;
mod output;
mod overlay;
mod pixel;
#[cfg(test)]
mod pixel_tests;
mod probe;

pub use live::{KIT_VERSION, run};

/// Serves the program's views as [`pencere::serve`] does, and where no terminal hosts a view shows
/// the program's `lua` view on its own terminal: `options` carry the screen ([`pencere::Options::screen`]).
///
/// # Errors
///
/// As [`pencere::serve`].
pub fn serve(options: pencere::Options) -> Result<pencere::Outcome, pencere::Error> {
    pencere::serve(options.screen(run))
}
