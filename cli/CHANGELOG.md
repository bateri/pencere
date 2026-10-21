# Changelog

All notable changes to `@pencere/cli` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the package follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- `pencere run [--tool <name>] [--args <json>] [--view <dir>] -- <command> [args...]`: run an MCP
  server that speaks stdio, written in any language, as a pencere program. The view opens in the
  terminal pane `pencere run` runs in, and closes with ⌃C or when the view or the terminal ends
  it. Without `--tool` the server is started once to ask which tool names a view; `--view` shows
  the view from a directory's files, sent to the terminal once.
- The server runs with the program's environment, in a process group of its own so that the
  terminal's ⌃C reaches `pencere run` and not the server, and is ended by closing its stdin, then
  SIGTERM and SIGKILL to its group. What it wrote to stderr is shown on the screen the person gets
  back when it failed.
- `pencere dev [--view <dir>] [--watch <path>]... [--tool <name>] [--args <json>] -- <command>
[args...]`: `pencere run`, and when the files of the view (`--view`) or the files the server reads
  (`--watch`, a file or a directory, as often as needed) change, the view closes, the server is
  started again and the view opens again in the same pane. Changes that come together are one
  reopening. The terminal keeps the view's files by content, so only the files that changed cross
  the stream. A change that leaves the files as they were, or a build that has not finished writing
  the view, shows nothing; a terminal that cannot reopen a view is told of once, after the view has
  closed, and the view runs as it does under `pencere run`.
- Exit status: 0 the view ran and closed, 1 it could not run, 2 a mistake in the command line,
  3 the terminal does not show pencere views; ⌃C and SIGTERM end the process by the same signal.
