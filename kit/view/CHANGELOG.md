# Changelog

All notable changes to the `pencere-view` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- **Colours.** `Tone` gains the theme's sixteen ANSI colours (`Black` … `BrightWhite`, in the order
  of the palette: `Tone::ANSI`, `Tone::ansi_index`) and `Selection`. A `Color` is now a `Source`
  (a role, or an `Rgba` value with an optional form for a dark theme) and an opacity; it is
  `Color { source, alpha }` where it was `Color { tone, alpha }` (`Color::role()` gives the tone).
  `Rgba::parse` reads `#rrggbb` and `#rrggbbaa`. The engine resolves none of it: the host takes the
  role from its theme and the form of a value that goes with its brightness (`Source::value`).
- **`Run::grounds`.** Each line of text in the draw list names the fills under it, nearest first,
  up to the first opaque one (`Color::is_opaque`), unresolved: what a host needs to hold the text
  to its lowest contrast.

- The crate itself, taken from the page engine of the terminal that wrote it first: elements and
  their style, layout, the draw list, what a page remembers between frames, the flat list a
  sandboxed page hands out (`wire`). The container list that served as its worked page and gauge is
  the example `containers-page`, no longer part of the engine.
