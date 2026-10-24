# Changelog

All notable changes to the `pencere-view` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- The crate itself, taken from the page engine of the terminal that wrote it first: elements and
  their style, layout, the draw list, what a page remembers between frames, the flat list a
  sandboxed page hands out (`wire`). The container list that served as its worked page and gauge is
  the example `containers-page`, no longer part of the engine.
