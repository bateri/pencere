# Changelog

All notable changes to the `pencere-wire` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- `bundle::media_type_of`: the media type a file of a bundle is given by its extension where its
  program says none (a script of a page of the kit is `LUA_MEDIA_TYPE`). It was the program
  library's; it is here so that a host that reads a view's files from a directory (a test's) gives
  them the type the program would.
- `link`: which links a view may ask its host to open (`is_openable`, `MAX_LINK`; specification
  §9.7, §9.8), a web page's `link` message or the link of a kit view's reply. The rule was the host
  crate's; it is here so that the kit's sandbox and the host hold both kinds of view to one copy.
- The messages of files beside a bundle (specification §8.8 to §8.10): the control keys `h`, `o`,
  `end` and `e` (`frame::FileParams`, `Message::handle`, `offset`, `end`, `error_code`), the
  payloads `message::Attach`, `Detach` and `Read`, and the host's answers `need_refusing` (a `need`
  with `refused`) and `wrote`. `bundle::Receiving::from_sizes` waits for files that are not a
  manifest's, and `give_up` lets go of those that did not come.

### Changed

- `frame::Outgoing` has `file`, the keys of the messages of files; `FileParams::NONE` for any
  other message. `bundle::Received::Failed` says how the file failed (`Mismatch`).

## [0.3.1] - 2026-10-26

### Fixed

- 0.3.0 never reached crates.io: it was tagged and drafted, and the release workflow then skipped
  its publishing. This is the same code, released.

## [0.3.0] - 2026-10-26

Released with the other crates; no change of its own.

## [0.2.0] - 2026-10-25

### Added

- The crate itself, and its first release: the part of the host crate that reads and writes bytes
  and does not decide what to do with them moves here, so that a program and a terminal share one
  copy of the wire. It holds `frame`, `json`, `base64`, the payloads of `message`, the manifest,
  digests and the verification of files of `bundle`, and the six view messages of a program and its
  view (`messages`), which a program and the module of its view in the terminal's sandbox both use.
  `bundle::Manifest::missing` and `is_held` take a closure that says whether a digest is held, for
  the store (the host crate's `Store`) stays the terminal's. The pencere crates are released
  together at one version, which began at 0.2.0: there is no 0.1.0 of this one.
- A message is read as the specification says. `Message::parse` refuses an `id` above 2^53 - 1
  (`Refused::NoId`), reads a number whose value is a whole number (`1.0`, `1e3`) as an `id`, and
  refuses a text nested 128 levels deep or more; `json::from_object` reads nothing that deep.
  `json::MAX_DEPTH` and `json::too_deep` give the nesting at which no receiver takes a JSON text
  (specification §4.5), and `messages::MAX_ID` and `Message::has_usable_id` the range of an `id`: 1
  to 2^53 - 1 (§9.2). `message::welcome_offer` takes the `opens` of the `welcome` (§5.2).
