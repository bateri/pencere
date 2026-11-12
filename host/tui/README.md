# pencere-tui

The cells of a terminal as a host of [pencere](https://github.com/bateri/pencere)'s native views.
A page of the kit hands out a draw list of boxes, lines of text, pictures and canvases, in pixels.
This crate draws that list into the characters of a terminal, for a program whose terminal does not
host pencere's views: the page the program carries is the interface it shows there, with no second
interface written in characters.

The drawing (`render`) does no I/O and keeps no state between frames; the screen a program shows
with it (`serve`) does what a terminal needs: it takes the terminal, asks it what it can do, reads its
keys and mouse and writes only what changed. The crate depends on the page engine (`pencere-view`) and
its drive (`pencere-drive`), on the sandbox for the pixels of an image and of a canvas
(`pencere-sandbox`), on the program library (`pencere`) and on `unicode-width`; on no platform, no
font library and no terminal library (`make check`).

## A program's own screen

```rust
// Where the terminal hosts no view, the program's `lua` view is drawn in the cells of its terminal.
let outcome = pencere_tui::serve(options)?; // as `pencere::serve`, with the screen behind it
```

`PENCERE_TUI=1` draws it without asking the terminal first (also inside a terminal that hosts
views), `PENCERE_TUI=0` never does, and `PENCERE_TUI_THEME=light` is the colours of a terminal that
does not say its own. The weakest terminal is the yardstick: no pictures (large ones are half blocks,
icons are left out), 256 colours, and the keys the old encodings send; what the terminal says it has
(true colour, the Kitty graphics and keyboard protocols) is used. The shape of the mouse pointer is
the page's, asked of the terminal with `OSC 22` (a hand over what answers a click, the I-beam over a
field) and given back with the terminal; a terminal that does not know it ignores it.
`PENCERE_TUI_TRACE=1` says, when the program ends, where the time of the start went.

What the page asks of its host is done in cells too. An editor has the terminal's own cursor at its
caret (a bar, which the terminal blinks) and takes typing, a selection, and a paste in
brackets; `Ctrl+Shift+C` copies the selection with `OSC 52` where the terminal tells it from `Ctrl+C`,
which ends the program. The editor's keys are those of a system with no ⌘ (the screen says `linux`
to the page): `Ctrl+A` selects all, `Ctrl+Z` and `Ctrl+Y` undo and redo, `Ctrl` with the arrows goes
by words and `Ctrl+Home` and `Ctrl+End` to the document's ends. There is no cut key, since a
terminal may not let what is cut reach its pasteboard. A menu is a framed list over the page (a paste
entry has no line: the person pastes with the terminal's own paste), a file
panel is one line for a path (Tab completes it, and it says what Enter and Esc do), and a link is an
address on the bottom row that the terminal opens (`OSC 8`): no browser is started, since the
terminal may be on another machine. A program that lends its terminal to
a command (a shell, an editor) has the screen given back for it, and its view drawn anew after. Where
the terminal has the Kitty graphics protocol a picture is sent once, and scrolling it only moves where
it is placed.

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

## Features

`compiler` and `pdf`, both on by default, and `precompile`, off, are those of `pencere-sandbox`: the
code generator that compiles the interpreter of a page as the first page opens, the PDF reader, and
the interpreter compiled while the program is built instead of the code generator. A program that has
no use for the first two takes the crate with `default-features = false, features = ["precompile"]`:
the interpreter is then compiled while the program is built and loaded as it is, and a PDF a page
shows is refused as an image that cannot be read, the terminal drawing a page of it as it draws any
file it cannot decode. (`default-features = false` with no `precompile` does not build: the build
says to turn one of `compiler` and `precompile` on.)

## The grid

The page is laid out on the cells. The measure says how many pixels a column is across and a row
is down (`Cells::new(col, row)`, passed on to the engine as `Measure::grid`), and the engine rounds
every length the page writes to whole columns across and whole rows down: sizes, the room around a
box, the gaps between boxes, the columns of a grid, how far a box is scrolled. A line of text is a
row, whatever its size, and a character is as many columns as it takes in a terminal. A box that is
written with a size keeps at least one cell.

What is written thinner than half a cell (a line, a caret, a rule) is drawn as it was written: an
eighth block at the edge of its cell. A thin upright line over a letter reverses that cell, so that
the letter shows under the caret. A box that has a thin border all round, and no text in the cells
of its edge, has the border as a ring of line characters (`╭─╮│╰╯`, square corners `┌┐└┘` where the
box is not rounded) in those cells, in the colour of the border over the ground of the box, so that
the frame closes at the corners. A box with a thin border all round that has no room for a ring (a
field of one row, a panel with words in its edge) is a bar and has no part of a line: its fill tells
it from the ground where it can, and where the fill is not far from the ground, or none, the colour
of the border is laid over it with a share of the border's opacity, so that a field in focus is a
different bar from one that is not. A border that is not seen is not drawn. Two units of the draw list a row (`Cells::new(1.0, 2.0)`) make
half of a cell a square, which a half block draws; a row of 18 or 20 units makes the page's own
spacing, which is written for a screen, fall on whole rows.

Pictures go to the terminal as they are where it draws them (`ImageMode::Kitty`), as half blocks where
it does not (`ImageMode::Blocks`), or not at all. Canvases are drawn as braille dots
(`CanvasMode::Characters`) or as pictures (`CanvasMode::Picture`). As characters a canvas makes no
ground of its own (the ground of a cell is the box under the canvas): only what a figure covers
(a dot of the eight of a cell more than two in five) is drawn, in the colour that covers it, and
what it covers less is left out. A canvas of a cell or two is an icon: its ink fills the cells, in
quarter blocks where it is flat (an arrow, ▚▞) and in braille where it is squarer (a cross).
Shadows, rounded corners and rotation are not drawn.

## License

Apache-2.0.
