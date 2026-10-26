# Changelog

All notable changes to the `pencere` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

## [0.3.1] - 2026-10-26

### Fixed

- 0.3.0 never reached crates.io: it was tagged and drafted, and the release workflow then skipped
  its publishing. This is the same code, released.

## [0.3.0] - 2026-10-26

### Fixed

- A program whose terminal goes away (an ssh connection that drops) is no longer ended by the
  hangup's `SIGHUP` after its view is over. The program learns of the hangup from the tty, `serve`
  returns, and the kernel's signal for the same hangup came a moment later to the default action
  that the terminal's handlers had been put back to, so the program died before what it does after
  its view (a draft of what was not saved). Once the terminal has hung up, `SIGHUP` stays ignored:
  the program knows from `serve` that its terminal went away.

## [0.2.0] - 2026-10-25

### Added

- The crate itself, as the program's side of pencere: everything a Rust program does to show a
  view in the pane it runs in, with no async runtime (see Changed for the name). `serve(Options)` takes the kinds of view the program
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
  - `ViewControl`, for a program that goes on after its view or shows another in its place
    (specification §10.8): `reopen` closes the view with a `dismiss` and opens one again (with
    the same files, or with other kinds of view), and `close` ends `serve` with `EndedBy::Program`.
    It is given to `Options::control` and called from another thread; the answers say when the
    terminal does not list `dismiss` (`Unsupported`), when there is no view to close
    (`Unavailable`), and when the new view could not be shown (`NoView`, with
    `NoViewReason::Timeout` for a terminal that did not answer in time). `Options::reopen_timeout`
    bounds the terminal's answers (ten seconds). A `close` answers once the terminal has been
    given back, so the program may print or run a shell as soon as it has the answer.
  - `Call` can be cloned, so that a handler can hand the call to the thread that answers it.
  - The 1 MiB limit of a view's message is counted before the wire escapes the text, in both
    directions, as the host and the TypeScript SDK count it: a message of non-ASCII text that is
    under the limit as the view said it is read and written, however long its escaped form is.
- **A terminal that opens a kind of the program's in other versions is told in words.** `NoView::message` of `NoViewReason::NoCommonView` carries the sentence that names both sides (`this program is written for wasm 2.0; this terminal opens wasm 1.0`), and `serve` prints it to standard error as `pencere: …` before the fallback runs (specification §5.2, §5.3).
- A `welcome` is read by what it lists, and a message of the view with an `id` above 2^53 - 1, or
  nested 128 levels deep or more, is ignored; an `id` written `1.0` or `1e3` is read as the whole
  number it is, and what the terminal writes nested that deep is not read either.
- A file with the extension `.luau` is `text/x-lua` in a bundle, as `.lua` is: the scripts a Lua
  view requires, by Luau's rules, are those of its bundle that are of that type.

### Changed

- **Breaking, for a user of `pencere` 0.1.0: the name `pencere` is the program library's now.**
  That version was the host library, which is the crate `pencere-host` (released with this one)
  and is what a terminal asks for. This crate starts at 0.2.0 so that its versions follow the last
  one published under the name.
