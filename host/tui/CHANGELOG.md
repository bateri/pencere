# Changelog

All notable changes to the `pencere-tui` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- The crate: a draw list drawn into the cells of a terminal (`render`), measured by `Cells` (which
  tells the engine the cells, `Measure::grid`, so that the page is laid out on them), as a
  grid of cells (`Grid`) and the places of the pictures to draw over it (`Placement`), written as
  true-colour escapes with Kitty graphics (`Frame::ansi`) or as plain text (`Frame::text`).
- Lines thinner than half a cell as eighth blocks at the edge of their cell, a thin upright line over
  a letter as a reversed cell, and a translucent box over a picture as a tint drawn over it.
- A light and a dark `Theme`, from the palettes the page's roles are resolved in.
- Pictures as terminal pictures, as half blocks, or left out (`ImageMode`); canvases as braille dots
  or as pictures (`CanvasMode`).
- The `frame` example, which opens an app folder with no screen and prints one frame of it.
