# Changelog

All notable changes to the `pencere-page` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- The crate is published. It is the SDK the page modules of the kit are written with: `Page`,
  `export_page!`, `Frame`, `Layout` and `say`, the style language of the page engine
  (`pencere-view`) and the messages of the wire (`pencere-wire`), re-exported. The package carries a
  copy of the interface it implements (`wit/view.wit`, `pencere:view@1.0.0`), so that building it
  reads nothing outside the crate.
