# Changelog

All notable changes to the `pencere-tui` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- The screen tells its page what it draws (`Program::draws`): always `cells`, and `coarse_images`
  where the terminal has no Kitty graphics. The page is started before the terminal is asked what it
  is, so a page that asks first waits (on its own thread, ten seconds at the most) for the answer.
- The crate: a draw list drawn into the cells of a terminal (`render`), measured by `Cells` (which
  tells the engine the cells, `Measure::grid`, so that the page is laid out on them), as a
  grid of cells (`Grid`) and the places of the pictures to draw over it (`Placement`), written as
  true-colour escapes with Kitty graphics (`Frame::ansi`) or as plain text (`Frame::text`).
- Lines thinner than half a cell as eighth blocks at the edge of their cell, a thin upright line over
  a letter as a reversed cell, and a translucent box over a picture as a tint drawn over it.
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
  `render`.
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
