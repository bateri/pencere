# pencere-tui

The cells of a terminal as a host of [pencere](https://github.com/bateri/pencere)'s native views.
A page of the kit hands out a draw list of boxes, lines of text, pictures and canvases, in pixels.
This crate draws that list into the characters of a terminal, for a program whose terminal does not
host pencere's views: the page the program carries is the interface it shows there, with no second
interface written in characters.

The crate does no I/O and keeps no state between frames. It depends on the page engine
(`pencere-view`), on the sandbox for the pixels of an image and of a canvas (`pencere-sandbox`), and
on `unicode-width`; on no platform, no font library and no terminal library (`make check`).

## Use

```rust
use pencere_tui::{Cells, Options, Theme, render};

// The page is laid out with a measure that says what a terminal says: a column per character,
// and a row for a line. A column is 8 pixels of the draw list across and a row 18 down, and the
// engine rounds every length the page writes to whole columns and whole rows.
let measure = Cells::new(8.0, 18.0);

// Each frame: the draw list the page hands out, the size of the terminal, the colours of the theme.
let options = Options { col: 8.0, row: 18.0, ..Options::default() };
let frame = render(&draw_list, columns, rows, &Theme::dark(), &images, &options);

stdout.write_all(&frame.ansi())?; // true colour, and the pictures in Kitty's graphics protocol
let words = frame.text();         // the same grid, as plain text
```

The `frame` example opens an app folder with no screen and prints one frame of it:

```sh
cargo run --release --example frame -p pencere-tui -- examples/containers-lua 80 24
```

The terminal needs the size of the frame plus one row, because the frame ends below its last row.

## The grid

The page is laid out on the cells. The measure says how many pixels a column is across and a row
is down (`Cells::new(col, row)`, passed on to the engine as `Measure::grid`), and the engine rounds
every length the page writes to whole columns across and whole rows down: sizes, the room around a
box, the gaps between boxes, the columns of a grid, how far a box is scrolled. A line of text is a
row, whatever its size, and a character is as many columns as it takes in a terminal. A box that is
written with a size keeps at least one cell.

What is written thinner than half a cell (a line, a caret, a rule) is drawn as it was written: an
eighth block at the edge of its cell. A thin upright line over a letter reverses that cell, so that
the letter shows under the caret. Two units of the draw list a row (`Cells::new(1.0, 2.0)`) make
half of a cell a square, which a half block draws; a row of 18 or 20 units makes the page's own
spacing, which is written for a screen, fall on whole rows.

Pictures go to the terminal as they are where it draws them (`ImageMode::Kitty`), as half blocks where
it does not (`ImageMode::Blocks`), or not at all. Canvases are drawn as braille dots
(`CanvasMode::Characters`) or as pictures (`CanvasMode::Picture`). Shadows, rounded corners and
rotation are not drawn.

## License

Apache-2.0.
