# pencere-tui

A terminal that speaks the Kitty graphics protocol as a host of [pencere](https://github.com/bateri/pencere)'s
native views. A page of the kit hands out a draw list of boxes, lines of text, pictures and canvases, in
pixels. This crate draws that list into the pixels of the terminal's window, in the faces of the machine,
with shadows, rounded corners and icons, as a window of any host draws it, for a program whose terminal
does not host pencere's views: the page the program carries is the interface it shows there, with no
second interface written in characters. A terminal with no Kitty graphics is given nothing, and the
program runs the fallback it has.

The drawing does no I/O and keeps no state between frames; the screen a program shows with it (`serve`)
does what a terminal needs: it takes the terminal, asks it what it can do, reads its keys and mouse and
writes only what changed. The crate depends on the page engine (`pencere-view`) and its drive
(`pencere-drive`, whose `shot` draws the pixels and finds the faces), on the sandbox for the pixels of an
image and of a canvas (`pencere-sandbox`), on the program library (`pencere`), on `unicode-width` and
on `miniz_oxide`; on no platform library and no terminal library (`make check`).

## A program's own screen

```rust
// Where the terminal hosts no view, the program's `lua` view is drawn as pictures in its terminal.
let outcome = pencere_tui::serve(options)?; // as `pencere::serve`, with the screen behind it
```

`PENCERE_TUI=1` draws it without asking the terminal first (also inside a terminal that hosts views),
`PENCERE_TUI=0` never does, and `PENCERE_TUI_THEME=light` is the colours of a terminal that does not say
its own. The window is one picture the size of the terminal's cells; what a key or a wheel turn changes
is sent as the rectangles that changed, and a picture of the page is sent once and only placed after.
The terminal is asked for its graphics, the size of its cells, its colours and its keyboard protocol
before the screen is left. The shape of the mouse pointer is the page's, asked of the terminal with
`OSC 22`; a terminal that does not know it ignores it. `PENCERE_TUI_TRACE=1` says, when the program ends,
where the time of the start went.

The keys are the page's own. A terminal has no ⌘ key, so a ⌘ shortcut with a letter is `Ctrl+Shift` and
the letter (a menu says `Ctrl+Shift+C`), and the sign that `Ctrl+Shift` makes is the character the
terminal says it makes. An editor has the page's caret, takes typing, a selection and a paste in brackets;
`Ctrl+Shift+C` copies the selection with `OSC 52`, and `Ctrl+C` ends the program. A menu is a framed list
over the page, a file panel is one line for a path (Tab completes it; the host does not read or write
the file yet), and a link is an address shown on the bottom row until a press or Esc turns it away; the
terminal does not open it. A program that lends its terminal to a command (a shell, an
editor) has the screen given back for it, and its view drawn anew after.

## Features

`compiler` and `pdf`, both on by default, and `precompile`, off, are those of `pencere-sandbox`: the
code generator that compiles the interpreter of a page as the first page opens, the PDF reader, and
the interpreter compiled while the program is built instead of the code generator. A program that has
no use for the first two takes the crate with `default-features = false, features = ["precompile"]`:
the interpreter is then compiled while the program is built and loaded as it is, and a PDF a page
shows is refused as an image that cannot be read. (`default-features = false` with no `precompile`
does not build: the build says to turn one of `compiler` and `precompile` on.)

## License

Apache-2.0.
