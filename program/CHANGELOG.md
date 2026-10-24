# Changelog

All notable changes to the `pencere` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- The crate itself, as the program's side of pencere: everything a Rust program does to show a view
  in the pane it runs in, with no async runtime. `serve(Options)` takes the kinds of view the program
  offers (`View`, `Bundle`), the handlers of the view's calls and subscriptions, and a fallback for a
  terminal that shows no view, and returns how it ended (`Outcome`).
  - The handshake with the fence `CSI c` and its two-second answer window, the late answer read and
    discarded.
  - The modes of the tty (non-canonical, echo off, signals on, the input never waited for), the
    alternate screen, and the way out on every path: `bye`, the wait for the terminal's `closed`,
    the unread input thrown away, the modes given back, and the same for a panic and for `exit`.
  - `SIGINT`, `SIGTERM`, `SIGHUP` and `SIGQUIT` end the view; `SIGTSTP` gives the terminal back,
    stops the program, and takes the terminal again when it is continued.
  - Writes by one thread, a few bytes at a time, so that a terminal that does not read cannot keep
    the program from leaving.
  - Calls run on threads of their own, with `Call::progress` and `Call::is_cancelled`; a
    subscription is fed from any thread with `Subscription::push`, which waits while the terminal is
    behind.
  - `Terminal::controlling` for a filter in a pipeline.

### Changed

- **The name `pencere` is this crate's.** The host library that was published under it is
  `pencere-host` now, and its first release under that name replaces nothing a program used. This
  crate starts at 0.2.0 so that its versions follow the last one published under the name.
