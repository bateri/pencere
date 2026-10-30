# Changelog

All notable changes to the `pencere-page` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- `pencere:view@3.0.0`: `Event::Files` with `source` and `entries` (`FileEntry`),
  `Event::FilesOver`, `Event::FilesLeft`, `Event::Saving`, and `Reply::panel` (`Panel`).
- `Pressing::Context`, `EditCommand::Copy` and `EditCommand::Cut`, `MenuEntry::Paste`, and `Place`
  with `inside` and `version`.

### Changed

- A page can ask how the application's terminal draws its cursor and its selection (`terminal_look`, a
  `TerminalLook`, whose `fill` puts it in an editor's options where the page said nothing) and what it
  draws that not every application does (`draws`), and is told `Event::TerminalLook` when it changes.

- An editor's caret `motion` crosses the interface.

- An editor's caret (`radius`, `glow`, `blink`, `blink_interval`, `blink_for`, `unfocused`) crosses the
  interface.

- An editor's `selection` (`SelectionLook`), a widget group's `margin` and a widget's `margin_left`
  and `margin_right` cross the interface.

- The gutter's columns, markers, ruler marks and zones, a node's `zone` and the event of the pointer
  in a column (`Event::Gutter`) cross the interface.

- A page can ask the time of day (`wall_clock`, a `WallTime`) and how the application's system
  writes keys (`keyboard`, a `KeyConvention`); a motion's `spin` crosses the interface.
- An editor's new commands, a change's `history` and `tag`, a reveal's `align`, an editor's
  `matches`, and the event `EditorReady` cross the interface; `Fill` and `FieldState` are exported.
- A command's keys are a list (`Command::keys` adds one), a menu's entry has keys as a command does,
  and a reply's `fill` says what is selected in a field as well as what it holds (`Fill`), across
  the interface.
- Written against `pencere:view@2.0.0`: what a page says of overlays, texts of many looks, canvases,
  commands, dragging, roles and states, and fills from one colour to another crosses the interface,
  and so do the new events. The engine's new names are exported (`rich`, `span`, `canvas`, `path`,
  `Command`, `Shortcut`, `Side`, `Drag`, `Dropped`, …), and `Layout` answers `drag_source` and
  `drop_target`.
- The application scrolls the page's boxes: `scroll_to(ScrollTo)` asks it to move one, the page is
  told where they went (`Event::Scrolled`), and `Layout`'s `scroll_extent` says how far each is
  scrolled (`Extent::offset`). `List` and `Built` are exported: a long list that builds only the rows near what is in
  view, and which rows to build as it scrolls.
- An editor's options carry how it looks (`line_height`, `padding`, `scroll_margin`, `numbers`,
  `current_line`, `caret`, `colors`) across the interface.
- An editor's search crosses the interface: `word` of a `Find`, the commands `Replace`,
  `ReplaceAll`, `SelectFound` and `SelectOccurrences`, and `Event::Found`.
- What a page says of its editor's language crosses the interface: the `comment`'s line and block
  comments, the `indent`'s rules, `folds`, and the command `ToggleBlockComment`.
- Which spaces and tabs an editor shows (`whitespace`) and its guides of the indent
  (`indent_guides`), and their colours, cross the interface.
- A reply's link (`Reply::with_link`), an address the page asks the application to open in the
  person's browser, crosses the interface as the page said it; which ones are opened is the
  application's to rule.
- A page asks to be woken: a reply's `wake` (`Reply::with_wake(seconds)`) and a frame's
  (`Frame::wake`, a field: a `Frame` written whole names it) cross the interface, and the wake comes
  back as `Event::Wake { now }`.

## [0.3.1] - 2026-10-26

### Fixed

- 0.3.0 never reached crates.io: it was tagged and drafted, and the release workflow then skipped
  its publishing. This is the same code, released.

## [0.3.0] - 2026-10-26

### Added

- `Zone::side` crosses the interface.

- `Mark::Widget`, `EditorOptions::widgets` and `HoverEvent::over`/`id` cross the interface.

- `Event::Target`, `Event::KeymapPending`, `Look::held`, `over` and `cursor`, and the editor's
  `gestures`, `keymap`, `chord_timeout` and `links` cross the interface.

- The pointer's rest (`Event::Hover`), the names of ranges (`Mark::Anchor`) and of styles
  (`Look::id`), `EditorOptions::events` and `hover`, `Column::pad` and `Queries::word_at` cross the
  interface.

- **Editors.** A page writes an editor (`div().editor(id, options)`), reads what it tells (`Event::Edit`,
  `Selection`, `Viewport`, `EditorRefused`, `Pointer`), asks of it in a reply (`Reply::editor`) and
  reads its text through `Layout` (`editor_text`, `line_start`, …); `edit` is re-exported.

- An element's `motion` and `tip` cross the interface.

- A page module can wrap a text (`text(...).wrap()`) and give the host's field its hint (a text
  inside the `text_field()` element), as `pencere-view` describes. The terminal's engine lays the
  page out, so a terminal with this release of it is what draws them.
- Everything `pencere-view` added crosses the interface: mixed colours, an element's opacity,
  italic and lines under and through words, named fields of three kinds with their texts, focus
  and fill, labels, a press's count, scrolling across, the new keys and pointer shapes.

## [0.2.0] - 2026-10-25

### Added

- The crate itself, and its first release (the pencere crates are released together at one version,
  which began at 0.2.0: there is no 0.1.0 of this one). It is the SDK the page modules of the kit
  are written with: `Page`, `export_page!`, `Frame`, `Layout` and `say`, the style language of the
  page engine (`pencere-view`) and the messages of the wire (`pencere-wire`), re-exported. The
  package carries a copy of the interface it implements (`wit/view.wit`, `pencere:view@1.0.0`), so
  that building it reads nothing outside the crate.
- The copy of the interface it carries has the `script` functions a Lua interpreter is given to
  load the other scripts of its bundle (`path`, `has-module`, `module`); an ordinary page module that asks
  is given no script and no module, and the interface stays `pencere:view@1.0.0`.
- `to_wit_frame` and `to_wit_reply` write the interface's lists: the words, values and tracks of a
  frame (and the titles of a menu) once each, and the elements name their parts.
