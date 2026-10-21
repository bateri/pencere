# Changelog

All notable changes to `@pencere/sdk` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the package follows
[semantic versioning](https://semver.org/).

## [Unreleased]

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
  answer `unsupported`, and leave the view alone, when the host does not list `dismiss`.
- Speaks version 1 of the wire (draft), including `dismiss` (specification §10.8) and `closed`
  `reason` `program`.
