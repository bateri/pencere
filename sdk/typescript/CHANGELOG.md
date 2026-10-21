# Changelog

All notable changes to `@pencere/sdk` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the package follows
[semantic versioning](https://semver.org/).

## [Unreleased]

## [0.1.1] - 2026-10-21

### Fixed

- A program run by a parent that forwards signals (`npm start`, for one) is no longer ended by
  ⌃C before it has given the terminal back. The parent sends the interrupt the tty already sent a
  moment later, and `serve` had put the signal's default action back before writing `bye` and
  leaving the alternate screen: the second interrupt ended the process between the two, and the
  shell's prompt came back on the alternate screen. The listeners now go after the terminal is
  given back.

## [0.1.0] - 2026-10-21

### Added

- `serve`: run an MCP server as a pencere program — ask the terminal for a host, open the
  view in the program's own pane on the alternate screen, answer the view's calls, and give the
  terminal back on exit, on ⌃C and on `closed`; with no tty, or no host, an ordinary stdio MCP
  server or the program's own fallback.
- `bundleFromDirectory`: a view of many files as a bundle, sent once and then served from the
  terminal's store by content.
- `ViewControl` (`ServeOptions.control`): close the view and go on, or show another in its place,
  while `serve` runs. `reopen` sends `dismiss`, waits for the host's `closed`, asks for a new
  token and opens the view again with the same files or new ones (the host's store keeps what did
  not change) on a fresh MCP session; `close` ends `serve` with `closed` by `program`. Both
  answer `unsupported`, and leave the view alone, when the host does not list `dismiss`. A host that
  starts the view again (§10.7) while a reopening is under way ends it as `unavailable` and gets the
  view it asked for, with a token that is alive: the `dismiss` of a reopening is not sent with the
  token of a view the host has forgotten, and a host that offers the view while the program waits for
  an offer is taken at its word.
- `ServeOptions.reopenTimeout` (10 seconds by default): how long a reopening waits for the host's
  `closed` after its `dismiss`, and then for the `welcome` after its `hello`. A host that does not
  answer in time ends `serve` as `no-view` with the new reason `timeout`, which says that a host
  that had answered went quiet: it is not `no-host`, which says that there is none. The wait for the
  first `hello` is `answerTimeout` as before.
- `ServeOptions.filter`: a program in the middle of a pipeline (`… | tool --select | mail`),
  whose stdin and stdout are its data, opens its view on its controlling terminal (`/dev/tty`)
  instead, and leaves stdin and stdout alone. It is never an MCP server on stdio; with no
  controlling terminal it has no view (`no-host`) and its `fallback` runs. A tool sets it from a
  flag of its own: a pipeline and an MCP client's launch look the same from inside, so `serve`
  does not guess.
- Speaks version 1 of the wire (draft), including `dismiss` (specification §10.8) and `closed`
  `reason` `program`.
