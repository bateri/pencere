# Changelog

All notable changes to the `pencere-sandbox` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- The crate itself, taken from the terminal that wrote it first: the sandbox, its limits and
  watchdog, the thread a module runs on, and the embedded interpreter. The interface is
  `pencere:view@1.0.0` and the Lua global is `pencere`.
- A helper of the Lua page API that is given the wrong argument (`pencere.text` of a value that is
  not a string or a number, `pencere.spark` of a non-list, `children` of a non-list) raises the
  error at the line of the script that called it, not at a line of the prelude.
