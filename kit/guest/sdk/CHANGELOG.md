# Changelog

All notable changes to the `pencere-page` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Changed

- Written against `pencere:view@2.0.0`: what a page says of overlays, texts of many looks, canvases,
  commands, dragging, roles and states, and fills from one colour to another crosses the interface,
  and so do the new events. The engine's new names are exported (`rich`, `span`, `canvas`, `path`,
  `Command`, `Shortcut`, `Side`, `Drag`, `Dropped`, …), and `Layout` answers `drag_source` and
  `drop_target`.
- The application scrolls the page's boxes: `scroll_to(ScrollTo)` asks it to move one, the page is
  told where they went (`Event::Scrolled`), and `Layout`'s `scroll_extent` says how far each is
  scrolled (`Extent::offset`). `Built` is exported: which rows of a long list to build as it scrolls.

## [0.3.1] - 2026-10-26

### Fixed

- 0.3.0 never reached crates.io: it was tagged and drafted, and the release workflow then skipped
  its publishing. This is the same code, released.

## [0.3.0] - 2026-10-26

### Added

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
