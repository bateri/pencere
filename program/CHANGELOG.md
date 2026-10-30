# Changelog

All notable changes to the `pencere` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Fixed

- The answer to a call of a view that has been replaced (`ViewControl::reopen`, a terminal that
  asks for the view again) is no longer written as the answer to the new view's call of the same
  id: a view numbers its calls from 1, so the late answer of a handler that did not stop when its
  view ended went to whichever call of the next view had its number. Progress, pushes and the start
  of a subscription are told apart from the next view's the same way.

### Changed

- A Lua app's page is offered as `lua` `2.0`, the version of the kit's page API it is now written
  against.

### Added

- **Files beside the view's bundle** (specification §8.8 to §8.10), for a view opened at `lua` `2.1`
  or `wasm` `3.0` and later. `ViewControl::attach` (and `attach_file`) adds a file for the view to
  draw and returns the name it draws it by (`sha256:` and the digest of the bytes sent), once the
  terminal holds it; a file attached already, or still in the terminal's store, does not cross
  again. `ViewControl::receive` reads a file the person gave the view, by the handle the view passed
  on, into a place of the program's, two pieces of 1 MiB at a time, through a part
  (`.<name>.pencere-part`) that is given its name at the end and deleted when the view ends, the
  program is asked to end or the receiving is given up; `ViewControl::send` writes a file to the
  place the person chose in the Save panel. Below them `read`, `write` and `detach`; `safe_name`
  makes the name the person gave a name the program may put in a path, and every refusal is a
  `FileError` with the terminal's code. The example `files` shows the four.
- **How a lent command ended, before the view is shown again.** `ViewControl::hand_off_then(command,
then)` lends the terminal as `hand_off` does and gives `then` how the command ended (or why it
  could not start) once it has, on the library's thread, before the view is asked for again: what it
  keeps is there for the first call of the view that comes back, which `HandOff::wait` on a thread
  of its own could not promise. The same in a `Conversation` (when the `Lent` says) and with no view
  up (before `wait` returns).
- **Requests answered later, in the order the view made them.** `Options::call_deferred(method,
handler)` and `Options::topic_deferred(topic, handler)` give a program with a loop of its own (an
  interpreter, an event loop) each call and subscription on the thread that reads the view, in
  order, with an `Answer` it sends from any thread when it can (`Answer<()>` says whether a
  subscription started); none waits for another's answer. An answer given before the handler returns
  is sent as it returns; one dropped, or a handler that panics, fails its request with `internal` (or the error `Answer::unanswered` says, as `unavailable` for a loop that may end with requests owed).
- **A conversation in the same process.** `Conversation::new(options)` serves a program's handlers
  with no terminal: `send` gives them a message of the view as JSON text, `receive` and
  `try_receive` take what the program says (`Said::Message`, and the requests of its `ViewControl`
  as `Said::Closed`, `Said::Reopened` and `Said::HandedOff`, whose `Lent` says how the command
  that was not run ended). The threads, the lanes, the subscriptions, the limit of a message, a
  panic as `internal` and the pace of the reader are `serve`'s, by the same code: what the view and
  the handlers say to each other is one part of the library, which `serve` carries over the
  terminal and a conversation hands to its holder.

- **Calls in order.** `Options::call_in(lane, method, handler)` answers `method` in a lane: the
  calls of one lane run one at a time, in the order the view sent them, on the lane's thread, while
  other lanes and the calls of `Options::call` go on. A call cancelled before its turn does not run
  and is not answered; a handler that panics is an `internal` error of its call and the lane goes
  on. A lane lasts as long as `serve`: a call of a view shown again waits for a handler of the view
  before it that is still running. A lane holds up to 256 calls; more are told `busy`. The limit of
  64 calls answered at once is now that of the free calls alone: the calls of the lanes are counted
  in their lanes.

- **Lending the terminal to a command.** `ViewControl::hand_off(command)` closes the view, gives the
  terminal back, runs the command as the terminal's foreground job (its own process group, the
  tty's foreground, a stopped one continued), and when it ends takes the terminal back and shows
  the view again, `serve` going on with the same options. It returns a `HandOff`, whose `wait`
  gives a `HandOffResult`: how the command ended once the view is back, or why the view did not
  come back (and `serve` ends with that). What was on its way to the view goes out before the view
  closes, the answer of the handler that asked for the hand-off among it. A hangup while the
  command has the terminal is passed on to it and ends `serve` once the command has ended; a
  reopening asked meanwhile is the view that comes back. With no `serve` running the command runs
  in the foreground all the same, and a `serve` started meanwhile is told `Error::Busy`. A command
  that stops for reaching for a terminal it does not have the foreground of (the view's terminal is
  not the program's controlling one) is ended as on a hangup rather than continued for ever.

- **An app's files, carried and reloaded.** `pencere::build::app()`, called from `build.rs`,
  carries `src/view/` and `src/assets/` into the binary; `pencere::app!()` is them, an `App`, and
  `Options::app` offers its views by the rules of a Lua app (the page of the kit, then the web
  page). `App::dir` reads them from a directory instead. Under `pencere dev`, which sets
  `PENCERE_DEV_APP`, the app is read from the project and the view is shown again whenever one of
  its files changes, without ending the program.

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
