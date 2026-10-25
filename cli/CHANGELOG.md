# Changelog

All notable changes to the `pencere` command are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the command follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- An app can be a filter in a pipeline. `io.read_stdin(on_data[, on_end])` reads standard input as
  it arrives, a piece at a time and exactly as written, without holding the app: the view and the
  other work go on while the data comes. It gives a reading with `:stop()` (the writer is told the
  pipe has no reader), `:active()` and `:wait()` (waits until the input has ended or was stopped).
  `io.is_terminal("stdin" | "stdout" | "stderr")` says whether a stream is a terminal. `serve{
filter = true }` shows the view on the controlling terminal, whatever the standard streams are
  (specification section 13), so that stdin and stdout stay the pipeline's. `close_view()` ends the
  view from the app's side, so that a program that has chosen something writes it with the pane
  given back. `process.spawn` and `process.run` take `session = true` to start the program in a
  session of its own, without a controlling terminal, for a command that must not reach the pane
  (it still leads its group, which is what a kill or a limit signals).

### Changed

- An app has one source layout: the program in `src/program/` (`app.lua` and its modules), the page
  in `src/view/` (`page.lua` or `index.html`) and the pictures in `src/assets/`; builds go to
  `dist/`, and nothing generated is under `src/`. `pencere new` makes every template so (Lua, Lua
  with a web page, TypeScript, Rust). Once packed an app is `app.lua` and its modules at the top,
  `view/` and `assets/`, and every path it writes is a path of that: `serve{}` told no view shows
  what `view/` has, and `icon = "assets/icon.png"`. `pencere build` and `pencere dev` take
  `--web DIR` (the page is packed from the directory a tool built it into, `dist/view`, instead of
  `src/view/`) and `--name NAME` (the executable's name and the name in the app's own messages; the
  directory's name by default), and `pencere build` writes `dist/<name>` unless `-o` says where
  (the directory of the file is made). A directory with an `app.lua` in it is still an app, the
  simple form, and is packed as it lies.
- A program that `process.spawn` or `process.run` starts runs in a group of its own, and `kill`, a
  `timeout`, `max_output` and the end of the app signal the whole group. Before, a shell's
  pipeline kept the pipe open after the shell was ended, and `run` did not return. `max_output`
  counts the bytes as they are read, line ends included, and keeps nothing past the limit (empty
  lines were counted as nothing, and a program that said only those filled the memory); a pipe is
  read a bounded amount in a turn. An app that shows nothing and is ended by ⌃C or a hangup asks the
  programs it started to end first (they are not in the group the terminal signals). `on_exit` is told as a third value whether the program was ended
  for saying too much.
- `text.wire_size` and `text.wire_fit` count as the protocol counts a message: the text's UTF-8 before
  any escape (they counted six bytes for each unit outside ASCII). `text.index`, `wire_fit` and
  `units` walk the bytes they are given: a run of bytes that is not UTF-8 is one character and the
  places are those of the real bytes, and a text sent in pieces costs the text, not the text for
  each piece.
- `regex` uses an engine that does not backtrack, so a pattern a person typed or a page sent cannot hold
  the app. Back-references and look-ahead or look-behind are refused ("this pattern uses a
  back-reference, which search does not support"). `whole_word` is checked in code around the match,
  and a search that would read the same stretch again and again says so and stops. `:find` takes
  a negative `init`, as `string.find` does.
- `pencere build` refuses a Mac runner without the room in its header for the app's section.
  `serve{ icons }` reads the size only of a file that is a PNG; `os.username` has a buffer that
  grows for a long record.

### Fixed

- `fs.watch` on macOS told of what happened before the watch started (a directory made just before it); the stream's beginning is now told from the past by an event of its own. `fs.watch` returns without waiting for the system to confirm that it has begun. The Rust project template is `Cargo.toml.tmpl` in the repository, so that cargo does not read it as a manifest in a project that takes this repository by git.

### Added

- For an app that edits files, such as an editor: `fs.create` (a new file, where nothing is: it does
  not truncate a file or write through a link), `fs.rename`, `fs.writable`, `link` in `fs.list`;
  `process.run` with `raw` (output that is not text, exactly as written) and `max_output`;
  `json.null`; `os.hostname` and `os.username`; `text` (names in the order people read them, and a
  text counted in UTF-16 units); `regex` (patterns a person types, without back-references or look-around); `serve{ icons }` (an icon for
  each background and size).
- A Mac's runner is linked with room in its header for the segment `pencere build` adds. Without
  it an app could be written over its first functions, and fail at the first call of one.

- The command itself, and the runner a built app is: one executable that, with an app inside it,
  runs that app, and without one is `pencere`.
  - Apps are written in Luau (the dialect of the kit's page API) and run on the `pencere` program
    library: `require("pencere")` gives `serve`, `process`, `fs`, `json`, `time`, `env`, `args`
    and `path`, on one thread with one loop for the terminal, the timers and the pipes of the
    processes it starts. `process.run`, `time.sleep` and `serve` wait without holding the loop.
  - `pencere new <name> --lang lua|ts|rust` makes a project to start from.
  - `pencere dev` runs a Lua app in the pane and runs it again when a script changes; when the
    page or an asset changes, the view is closed and shown again without ending the program.
  - `pencere build [--target T] [--runner FILE] [-o FILE]` makes one executable of an app, for
    this machine or for any of the four targets (Apple silicon and Intel Macs, Linux musl on arm64
    and x86_64), without the toolchain of the target. A Mac's executable is signed ad hoc.
  - `--log FILE` sends an app's `print` and errors to a file, since the terminal is under the view.
  - The view of an app is a page of the kit (`page`), a web page (`web`, the files of a directory
    whose entry is `index.html`, with an `access` for what it may reach), or both in the order the
    app prefers (`views`). `pencere new <name> --page web` starts an app with a web page, carrying
    the SDK's page module so that there is no npm and no build step.
  - For apps that do more than show a list: `fs.watch` (FSEvents on a Mac, inotify on Linux;
    nothing is polled), `handoff` (gives the terminal to a program, such as the person's shell, and
    shows the view again when it ends), `process.run` with a `timeout`, `hash.sha256`, `os`, and
    `io.write` and `io.read_line` for an app's text interface.
