# Changelog

All notable changes to the `pencere-tui` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- **The screen holds the files the program attaches to its view.** A program that attached a PDF or a
  picture for its page to draw (as textpen does) now has it drawn; the files are kept when the view
  is shown again, and let go when the program says so.
- **Features `compiler` and `pdf`, both on by default, and `precompile`, off,** which are those of
  `pencere-sandbox`. A program that takes the crate with `default-features = false` and
  `features = ["precompile"]` has an executable with no code generator for the interpreter (it is
  compiled when the program is built) and no PDF reader: a PDF a page shows is refused as an image
  that cannot be read. The terminal draws the same pages; the first page no longer waits for the
  interpreter to be compiled. With neither `compiler` nor `precompile` the build fails, naming both.
- **A menu, a panel and a link over the page.** `Host::menu` is a framed list of cells on the
  page's own ground, so that the frame alone is its edge (the arrows, Enter, Escape and the pointer
  pick, and a press outside turns it away; a Paste entry has no line: a page cannot read the
  terminal's pasteboard, the person pastes with the terminal's own paste, and a separator it leaves
  at an edge or beside another goes with it). `Host::panel` is one line
  for a path, with Tab to complete it, for Open (a file becomes a handle the page draws with
  `file:<handle>` and has a `path`) and for Save (the page's proposed name goes after a folder); at
  the right it says what its keys do (`Enter opens  Esc cancels`, `Enter saves  Esc cancels`), and
  leaves that out where the path would have less than 24 columns. `Host::link` shows an http or https address on the bottom row as a link
  the terminal opens (`OSC 8`); no browser is started. Written as cells over the page, so the screen's
  difference writes them like the rest.
- **The editor.** The terminal's own cursor stands at the caret of the editor that has the keyboard,
  as a bar (a block would cover the character it is before) that the terminal blinks; the page does
  not draw a caret of its own. The cursor is given back to the terminal's own shape when no caret is
  shown, when the terminal is lent to a command, and when the screen is left, however it is. A paste in brackets
  goes in at the caret, and what an input method composes arrives as the text it makes. `Ctrl+Shift+C`
  copies what is selected to the terminal's pasteboard (`OSC 52`) where the terminal tells it from
  `Ctrl+C`, which ends the program. There is no cut on `Ctrl+X`: a terminal may not let what is cut
  reach its pasteboard, and the text would be lost. What the page asks to copy on its own is never
  written to the pasteboard. The editor's keys are those of a system with no ⌘ (`Ctrl+A` selects
  all, `Ctrl+Z` and `Ctrl+Y` undo and redo, `Ctrl` with the arrows goes by words, `Ctrl+Home` and
  `Ctrl+End` to the ends), and the page reads them with `pencere.layout.keymap`.
- **A terminal lent to a command.** A program's hand-off (`Said::HandedOff`) gives the modes back,
  runs the command as the terminal's foreground job, takes the terminal again and draws the view
  anew.
- **Pictures are sent once.** The pixels of a picture go to a terminal with the Kitty graphics
  protocol once, under a number, and a placement is put by number with the part it shows; scrolling
  a picture writes only where it goes. Pixels that are not shown are given up when more than 128 MiB
  are kept, the least lately shown first, and all of them when the terminal is given back, when the
  window is resized and after a hand-off. Placements that overlap in one layer stay in the order they
  were drawn.
- **A picture is sent at the size it is drawn, and a page that scrolls is not sent again.** A picture
  is made as large as it is drawn and stored once; each frame only puts it, at its pixel and with the
  part of it that shows, and a mark or a line over words is a small picture of its own, stored once
  per size and colour. The terminal is asked how large a cell is (`CSI 16 t`, asked again when the
  window changes) and pixels go packed (`o=z`) to a terminal that says it takes them.
- `Pixels`, the whole of the pixels of a picture the terminal draws (made into what the terminal
  takes only when it is sent them), and `Raster::id`, which names pixels.
- `Frame::links` and `Link`: the runs of cells that are links, written under `OSC 8`.
- The screen tells its page what it draws (`Program::draws`): always `cells`, and `coarse_images`
  where the terminal has no Kitty graphics. The page is started before the terminal is asked what it
  is, so a page that asks first waits (on its own thread, ten seconds at the most) for the answer.
- The crate: a draw list drawn into the cells of a terminal (`render`), measured by `Cells` (which
  tells the engine the cells, `Measure::grid`, so that the page is laid out on them), as a
  grid of cells (`Grid`) and the places of the pictures to draw over it (`Placement`), written as
  true-colour escapes with Kitty graphics (`Frame::ansi`) or as plain text (`Frame::text`).
- Lines thinner than half a cell as eighth blocks at the edge of their cell, a thin upright line over
  a letter as a reversed cell, and a translucent box over a picture as a tint drawn over it.
- A line along the middle of a cell is `─` (`━` from an eighth of a cell thick). A line the letters of
  its cells would break is one picture over them where the terminal draws pictures, and is left out
  where it does not. A mark laid on a picture the terminal draws, as the mark of a word found in a
  PDF, is where the terminal draws that part of the picture, in a picture of its own at the pixel it
  begins at.
- A box with a thin border on every side, and no text in the cells of its edge, is drawn with a ring
  of line characters (`╭─╮│╰╯`, `┌┐└┘` where its corners are square) in the colour of the border, so
  that its frame closes; a border that cannot be seen (fully faded) is not drawn. A box with no room
  for a ring (text in its edge, a row or a column of cells) has no part of a line either: it is a
  bar, told from the ground by its fill where that is far enough from it, and else by the colour of
  its border laid over it (three tenths of the border's opacity; a tenth over a fill that tells it
  apart), so that a field in focus is not the one that is not. A line on one side and a border as
  thick as a cell are as they were.
- A light and a dark `Theme`, from the palettes the page's roles are resolved in.
- Pictures as terminal pictures, as half blocks, or left out (`ImageMode`); canvases as braille dots
  or as pictures (`CanvasMode`). A canvas as characters makes no ground: only the dots a figure
  covers are drawn, in the colour that covers them (where two colours cross in a cell, the one that
  has more of it), over the box under the canvas; a canvas of a cell or two (an icon) has its ink
  fill the cells, in quarter blocks where it is flat (an arrow) and in braille where it is squarer
  (a cross).
- The `frame` example, which opens an app folder with no screen and prints one frame of it.
- **A program's own screen: `serve`, `run`.** Where no terminal hosts the program's view (or
  `PENCERE_TUI=1`), the program's `lua` view runs in the process and is drawn in the cells of the
  terminal it was started in, on the program library's `Tty` and with its handlers served by a
  `Conversation`. Only the cells that changed are written (`Screen`), in true colour where the
  terminal says it has it (`COLORTERM`) and in the 256 of the palette where not (`Depth`). The
  terminal is asked, with the device attributes as the fence and a second at the most, for its colours
  (the page's roles are made of them: `PENCERE_TUI_THEME=light` is the guess where it says nothing;
  the page is started while the answers come),
  the Kitty graphics protocol (pictures as the terminal's, half blocks where it has none) and the
  Kitty keyboard protocol (switched on, and off again, where it has it). The keys come from the old
  encodings and from the Kitty protocol's `CSI u`, the mouse in the long and in the old form, pastes
  in brackets; the window's size is asked before the first frame and again at every `SIGWINCH`. The
  page's text fields are drawn over their rooms, with their texts, the selection, and the terminal's
  cursor at the caret; the page's wheel, clicks and drags, and ⌃C, which ends it in the way a signal
  does wherever the keyboard is.
- **The mouse pointer takes the shape the page asks for.** It is written as `OSC 22` (Kitty's
  pointer shapes, by CSS's names: `pointer`, `text`, `ew-resize`, `not-allowed`, `grab`, …) only when
  it changes, and given back to the terminal's own, with the rest of the terminal, before the screen is
  left. Over the room of a field of the host's, where the page says the arrow, it is the I-beam. A
  terminal that does not know the sequence ignores it.
- **The words an empty field says** (its hint) are drawn, dim, in the fields nobody has typed in
  yet; they were only where a field had been pressed. `show_hints` draws them over a frame of
  `render`, and `show_texts` draws them with the texts a test or a picture has put in the fields.
- `PENCERE_TUI_TRACE=1` prints, when the screen ends, when the alternate screen was entered, the page
  started, the terminal finished answering, each of the first frames was written and each time the
  window changed its size, each with the memory the process holds then (and the most it has held),
  the first word the program said and, every thirty seconds, the memory again; and the time the
  keys and the frames took, the bytes and the pictures written, and the most memory held.
- **A run of one key of movement is told to the page at once.** An arrow, a page key, home or end
  sent again before the page answered the one before (held down, or in one write) is told the page
  without waiting for each answer, and the page is made once for the run (the engine's
  `Module` does that for keys that follow one another), where each key waited for a page of its
  own and a held key fell behind. A key the page binds to a command (`Page::binds`) still waits for
  its answer.
- **tmux and screen are not asked for pictures.** The probe knows them by what they answer to the
  terminal's name (`CSI > q`, `CSI > c`) and by `TMUX`, `STY` and `TERM`, and sends no Kitty graphics
  query, which they keep as the pane's title; the page is drawn without pictures there.

### Changed

- **Breaking: `Placement` has `offset`.** A part put at a pixel of its first cell, as large as its
  own pixels, has `Some((x, y))`; one that has `None` is stretched over its cells, as before.
- **A picture is where the page put it.** Where the terminal's cell has the page's proportions to a
  hundredth, a picture is at the pixel its layout has it at and is cut where the page cuts it; on a
  cell that has not, it keeps its proportions inside its box, with the ground of the box's parent at
  its sides.
- **Pages of a PDF stand on the same pixel columns,** scrolled by less than a cell or not, and a
  picture under a palette is as dim as the cells are, and is not sent again for it.
- **A page larger than a screen is sent in pieces,** a grid of 1024 pixels laid over it that does not
  move: a piece that is seen again is not sent again, whatever part of the page the pixels came from.
- Pixels are packed (`o=z`) by `miniz_oxide`; the crate's own deflate is gone.
- **The marks of a found word and the lines over words are pictures of their own size,** put where the
  page has them, to the pixel, and not the cells they touch.
- **A tmux variable that leaked into another terminal does not keep the host from being asked.**
  Where only `TMUX` or `STY` says it, the terminal is asked what it is; `TERM` and `TERM_PROGRAM` say it
  for sure.
- **A picture the terminal draws over the grounds** (an icon, the tint laid over a picture) **is left
  out where a box drawn after it covers it** (a palette, a dialog): it showed through them. A picture
  that covers its cells altogether was under their grounds already.
- **A field's text is on the middle row of its room, the lower of two,** where the page centres its own
  words in the room (a palette's scope beside its query).
- `Placement` is a part of some `Pixels` (`pixels`, `x`, `y`, `width`, `height`) and not the bytes of
  the part: `Placement::rgba` makes them. `Raster` has an `id`.
- **A picture is where the page put it and as wide as it is high.** A picture whose box was written
  in pixels keeps its proportions on the terminal's own pixels (240 by 240 is a square) and stands in
  the middle of its cells, and what is laid over it (the marks of a found word in a PDF) stands on
  what it marks, to the pixel. Icons are as they were: they fill their cells.
