# Changelog

All notable changes to the `pencere` command are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the command follows
[semantic versioning](https://semver.org/).

## [Unreleased]

## [0.2.0] - 2026-10-25

### Added

- **The `pencere` command, released for the first time.** It makes apps for
  [pencere](https://github.com/bateri/pencere): programs that show a view in the pane of the
  terminal they run in. An app is a Lua program and a page; `pencere build` makes one executable of
  it, which runs on any machine of its kind with nothing installed, and the same executable, with
  no app inside it, is `pencere`.
  - `pencere new <name> [--lang lua|ts|rust] [--page lua|web]` starts a project: Lua with a page of
    the kit or with a web page (HTML and JavaScript, no npm and no build step), TypeScript or Rust.
    Every project has one layout: the program in `src/program/`, the page in `src/view/` and the
    pictures in `src/assets/`; builds go to `dist/`, and nothing generated is under `src/`.
  - `pencere dev` runs the app in this pane and runs it again when a script changes; when the page
    or a picture changes, the view is closed and shown again without ending the program.
  - `pencere build` makes `dist/<name>`, one file. `--target` makes it for another machine (Apple
    silicon and Intel Macs, Linux on arm64 and x86_64, static, so on any distribution) without the
    toolchain of that machine; a Mac's executable is signed ad hoc. `--web DIR` packs a page from
    the directory a tool built it into (a React page, say), `--name` names the executable and the
    app in its own messages, `-o` says where it goes.
  - A built app runs by itself, and `app --log FILE` sends what it prints, and its errors, to a
    file, since the terminal is under the view while it is up.
- **How to get it.** One line installs the newest release in `~/.local/bin`
  (`curl -fsSL https://raw.githubusercontent.com/bateri/pencere/main/cli/install.sh | sh`: see the
  README), after checking the file against the `SHA256SUMS` of its release. Each release carries
  the four executables, named `pencere-<version>-<target>`, with their sums; they are built by the
  release workflow, which attests where each was built (`gh attestation verify`).
  `cargo install --git https://github.com/bateri/pencere pencere-cli --locked` builds it from the
  source.
- **What an app can do**, with `require("pencere")` and nothing else to install: `serve` shows the
  view (a page of the kit, a web page with an `access` for what it may reach, or both in the order
  the app prefers) and answers its calls and feeds its topics; `process.spawn` and `process.run`
  start programs; `handoff` gives the terminal to a program, such as the person's shell, and shows
  the view again when it ends; `fs` (with `watch`, which polls nothing), `json`, `hash.sha256`,
  `text`, `regex`, `time`, `os`, `env`, `args`, `path`, `io.write`, `io.read_line` and `exit`.
  There is one thread and one loop for the terminal, the timers and the pipes of the programs an
  app starts; `process.run`, `time.sleep` and `serve` wait without holding it.
- For an app that edits files, such as an editor: `fs.create` (a new file, where nothing is: it
  does not truncate a file or write through a link), `fs.rename`, `fs.writable`, `link` in
  `fs.list`; `process.run` with `raw` (output that is not text, exactly as written) and
  `max_output`; `json.null`; `os.hostname` and `os.username`; `text` (names in the order people
  read them, and a text counted in UTF-16 units); `serve{ icons }` (an icon for each background and
  size).

### Changed

- **`cli-v0.1.0` was not this command.** That release was the TypeScript `@pencere/cli`, which
  this replaces: a TypeScript program shows its view with `@pencere/sdk` now, and
  `pencere new --lang ts` starts one. The command's first release is 0.2.0.

### Security

- A program that `process.spawn` or `process.run` starts runs in a group of its own, and `kill`, a
  `timeout`, `max_output` and the end of the app signal the whole group, so a shell's pipeline does
  not outlive its shell. `max_output` counts the bytes as they are read, line ends included, and
  keeps nothing past the limit. An app that shows nothing and is ended by ⌃C or a hangup asks the
  programs it started to end first, and `on_exit` is told whether the program was ended for saying
  too much.
- `regex` uses an engine that does not backtrack, so a pattern a person typed or a page sent cannot
  hold the app. Back-references and look-ahead or look-behind are refused ("this pattern uses a
  back-reference, which search does not support"); `whole_word` is checked in code around the
  match.
- `pencere build` refuses a Mac runner without the room in its header for the app's section, and a
  Mac's runner is linked with that room, so an app cannot be written over its first functions.
