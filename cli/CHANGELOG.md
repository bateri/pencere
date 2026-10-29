# Changelog

All notable changes to the `pencere` command are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the command follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Fixed

- The calls and the subscriptions of a view start in the order the view made them. Each came in on a
  thread of its own and raced the others to the app's loop, so that two edits the view sent one
  after the other could be begun the other way round; they are now handed to the loop as the view
  sends them (the program library's deferred handlers), and none waits for another's answer.
- `page:wait_for` in `pencere test` goes on waiting when the page's view is over (the program closed
  it, showed another, lent the terminal), as what it waits for may be what the program does next:
  a program goes on after its `serve` on its own time, and the wait raised the view's end before
  the program had written what the test looked for. If the condition never comes the failure says
  the view is over.

### Changed

- The runner shows the views of version `2.0` of the kit's page API (`lua` `2.0`).
- `pencere.handoff` is the `pencere` library's hand-off: the same `serve` shows the view again
  instead of a new one being started, the app's loop is no longer the one that waits for the view
  to give the terminal back, and a hangup while the program has the terminal is passed on to it
  and ends `serve` once it has ended. A change to the view's files under `pencere dev` while the
  program has the terminal is the view that comes back.

### Added

- `pencere test` says how the host draws its terminal's cursor and selection (`terminal_look`, `draws`
  of `test.open`; `page:set_terminal_look(table)` changes it, and the page is told).

- `pencere test` can `dwell(id, x, y)` (the pointer rests over a word of an editor that hears of it,
  as long as it takes to be told), `hold("cmd")`, `release()` and `click_holding(x, y, "cmd")` (the modifiers a person holds) and say where a range the page named is (`anchor_rect(editor, name)`).
- **`PENCERE_CALL_MS` for `pencere test`.** The time, in milliseconds, a call of the page may take
  in a test: the kit's 250 ms (as a terminal holds a page) unless it is set. The limit is the wall
  clock's, and a test of what a page shows and does should not fail for how quick a busy runner is;
  a value that is not a whole number above zero is not read.
- **`pencere test`.** Runs the tests of an app, written in Lua (`test/**/*_test.lua`,
  `require("pencere.test")`), against the page of the app under a host with no screen: its page
  engine, editors and interpreter, with a clock the test moves. The page's program is a stand-in
  made of calls the test answers, the app's own program (Lua, in the test's process, in an
  environment of its own; a Cargo package, built once and run on a pty) or any program on a pty.
  Things are found by their words and their role, the test looks with `expect`, and a failure says
  what the page showed and keeps its picture, its words and its messages. The files run on worker
  processes (`--jobs`), a test is cut off at its timeout, and the report is pretty, TAP or JUnit.
  The runner of tests, `pencere-test`, is a program of its own and a second file of the release:
  the command runs the one beside it, or the release's of the same version (checked against its
  `SHA256SUMS`, kept in the cache), or the one `PENCERE_TEST` names. `pencere new` makes the first
  test (Lua and Rust projects; TypeScript with `--page lua`), and a simple-form app does not pack
  `test/` (a program module of that form cannot live in a directory of that name).
- **`pencere new --lang ts --page lua`.** A TypeScript project whose view is a page of the kit,
  the Lua project's page, with its first test.
- **`pencere upgrade`.** Replaces the command with the newest release of it, as `install.sh` would
  install it: the release's file for this machine, checked against the release's `SHA256SUMS`, put
  in the place of the running file in one rename. `--check` only says whether there is a newer one,
  `--version X.Y.Z` takes that one (an older one too). A command Cargo installed is left to Cargo,
  with the line that replaces it.

- **`pencere dev` and `pencere build` for a program in Rust.** In a Cargo package, `pencere dev`
  builds the program with Cargo and runs it in the pane: a change to the page or a picture shows
  the view again without ending it (the `pencere` library reads them from the project), and a
  change to anything else builds and runs it again; the program's standard error goes to the log.
  `pencere build` builds it with `cargo build --release` (`--target` passed on, with what to install
  when the toolchain lacks it) and copies it to `dist/<name>`.
- `pencere new --lang rust` makes a project whose build script carries `src/view/` and
  `src/assets/` into the program (`pencere::app!()`), so the page can be split into files as a Lua
  app's is. It needs the `pencere` crate that has `App`: the release that ships this command sets
  the version of the crate in the template to it.

## [0.2.2] - 2026-10-26

### Fixed

- 0.2.1 was never published: the release workflow stopped before it, because the crates it is
  released after were not published. This is the same command, built with the crates 0.3.1.

## [0.2.1] - 2026-10-26

### Fixed

- An app whose terminal goes away (an ssh connection that drops) is no longer killed by the
  hangup's `SIGHUP` after its view is over, before the code after `pencere.serve` has run: the
  signal that came a moment after the tty failed found its default action. Once the terminal has
  hung up, `SIGHUP` stays ignored (the `pencere` crate's fix, which the runner is built with).
- `pencere new --lang rust` makes a project on the `pencere` crate 0.3, which has the same fix.

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
- **A page of the kit can be several files.** The page, `src/view/page.lua`, may `require` the
  other `.lua` and `.luau` files of `src/view/` by Luau's own rules (`require("./screens/logs")`,
  `require("../parts/table")`, a directory with an `init.luau`), and nothing outside the view: the
  scripts go into the view's bundle with the paths the page writes them by, typed as scripts, in
  `pencere dev` and in a built app alike. `pencere dev` shows the view again when one of them
  changes, and `pencere build` refuses one that does not compile.
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
- For an app that is a filter in a pipeline, such as a picker: `serve{ filter = true }` shows the
  view on the controlling terminal whatever the standard streams are, so that stdin and stdout stay
  the pipeline's; `io.read_stdin(on_data[, on_end])` reads standard input as it arrives, a piece at
  a time and exactly as written, without holding the app (`:stop()`, `:active()`, `:wait()`);
  `io.is_terminal("stdin" | "stdout" | "stderr")`; `close_view()` ends the view from the app's
  side, so that what was chosen is written with the pane given back; and `process.spawn`/`run`
  with `session = true` start a program with no controlling terminal, for a command that must not
  reach the pane.
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
