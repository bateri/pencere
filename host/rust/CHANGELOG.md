# Changelog

All notable changes to the `pencere` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- The host side of version 1 of the wire (draft), without I/O: frames read from APC bodies or
  raw bytes and written within the limits, the session state machine (single-use token, `open`
  checks and refusals, bundles and their store, the MCP client toward the program and the MCP
  Apps host toward the view, closing and restart), the sandbox and network consent, and the
  terminal's theme as MCP Apps styling.
- `dismiss` (specification §10.8): the program closes its view and keeps running. The host lists
  `dismiss` in the `features` of its `welcome`, believes the message only with the token that
  opened the view, tears the view down as for any other end and writes `closed` `reason` `program`
  with `re` set to the `dismiss`. Closing a view forgets the view's token and nothing else: a token
  issued meanwhile, by a `hello` answered while the view was closing, stays good (§10.2).
- `ViewStart::title` and `ViewStart::program`: what the program calls the tool the view shows (the
  tool's MCP `title`, else `annotations.title`) and what it calls itself (`serverInfo.title`, else
  `serverInfo.name`), so a terminal can label the view in words people use rather than the tool's
  name and the process's. Both are text the program wrote, so the session cuts them to
  `MAX_LABEL_CHARS` (100) characters and leaves out control characters, line and paragraph
  separators and bidirectional overrides: a terminal can draw them as they are. A title or a name
  that is not a string is no title, and does not stop the view from opening. `ViewStart` is not
  `#[non_exhaustive]`, so a terminal that builds one itself adds the two fields.
