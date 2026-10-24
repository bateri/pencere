# Changelog

All notable changes to the `pencere` command are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the command follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Fixed

- `fs.watch` on macOS told of what happened before the watch started (a directory made just before it); the stream's beginning is now told from the past by an event of its own. `fs.watch` returns without waiting for the system to confirm that it has begun. The Rust project template is `Cargo.toml.tmpl` in the repository, so that cargo does not read it as a manifest in a project that takes this repository by git.

### Added

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
