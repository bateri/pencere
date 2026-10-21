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
- Speaks version 1 of the wire (draft).
