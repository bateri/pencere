# Changelog

All notable changes to the `pencere-tui` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- **A program's own screen: `serve` and `run`.** Where no terminal hosts the program's view, the
  program's `lua` page is drawn as pictures in a terminal that speaks Kitty graphics, in its own faces,
  with shadows, rounded corners and icons. A terminal with no Kitty graphics, one that does not say how
  large its cells are, and a multiplexer get the program's own fallback.
- **Only what changed is sent.** The window is one picture; a key or a wheel turn sends the rectangles
  that changed, and a picture is sent once and then only placed.
- **Changes far apart are sent apart.** Selecting a word that is on the page many times sends the
  places that changed, and no longer one large box that holds all of them.
- **A band is drawn in the columns that changed.** A key changes a part of a line and of the status
  bar, and only those columns are drawn and compared, not the width of the window. So is a part of
  the page that scrolls beside what stands still (the files of a sidebar, the lines of an editor): its
  rows are moved and drawn in its own columns.
- **Frames are made in memory kept from one to the next.** The bands, their mask and the deflate's
  state are not asked of the system again for each frame, and only the rectangles sent are copied.
- **The page's keys.** A ⌘ key with a letter is `Ctrl+Shift` and the letter, and a sign that
  `Ctrl+Shift` makes is the character the terminal says it makes. The keyboard protocol is asked to
  report the keys with their alternates.
- **A menu, a file panel and a link over the page.** `Host::menu`, `Host::panel` and `Host::link`; a
  link is shown as its address on the bottom row, over the whole of the strip the page has there (a
  status line is higher than a row of cells).
- **The editor.** Typing, a selection, a paste in brackets, and `Ctrl+Shift+C` to copy with `OSC 52`;
  `Ctrl+C` still ends the program.
- **A link is followed on the person's `Enter` or press.** It opens in the system's browser; over ssh,
  or where that fails, the address goes to the terminal's pasteboard and the foot of the window says
  "Link copied".
- **Saving, uploading and downloading work.** The program reads the file the person chose and writes
  only where they chose.
- **The mouse pointer takes the shape the page asks for** (`OSC 22`).
- **A terminal lent to a command.** A program's hand-off gives the terminal back, runs the command and
  draws the view anew.
- **The files the program attaches to its view** (a PDF, a picture) are drawn, kept while the view is
  shown again, and let go when the program says so.
- **Features `compiler`, `pdf` and `precompile`,** as those of `pencere-sandbox`: a program can take the
  crate with no code generator and no PDF reader.
- `PENCERE_TUI`, `PENCERE_TUI_THEME` and `PENCERE_TUI_TRACE` for a person to force the screen, say the
  terminal is light, and see where the time of the start went.
