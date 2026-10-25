# Changelog

All notable changes to the `pencere-wire` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

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
