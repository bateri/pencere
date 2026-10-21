# Changelog

All notable changes to `create-pencere` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the package follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- `npm create pencere@latest [directory] [--template react|vue]`: write a pencere tool, an MCP
  server whose view opens in the terminal pane, with a React or Vue view built by Vite. It asks for
  the directory and the template when they are left out and a person can answer; it writes nothing
  into a directory that holds anything but a `.git`, and replaces nothing.
- The server (`@pencere/sdk`, a tool with a view and a tool only the view calls) and the view
  (`@modelcontextprotocol/ext-apps`, the host's theme) are written, with a Vite build that makes the
  view twice: many files for a terminal that keeps them by content, and one document for every other
  MCP Apps host. `npm run dev` shows the view in the pane and shows it again when a source changes.
- A project that cannot be written (a directory that is not yours, a read-only file system) says
  which path and why, and what was written before is taken back, with the directories made for it:
  a second run finds the place as it was and does not refuse it for being in use. A template named
  that is not one, at the question, is said so before the question is asked again.
- In the generated project, `npm run dev` ends cleanly when `pencere` cannot be started, and does
  not report a build as failed that a signal ended (^C reaches the build with the rest).
