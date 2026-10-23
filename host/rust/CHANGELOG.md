# Changelog

All notable changes to the `pencere` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Changed

- The `closed` of a view the terminal draws itself that failed (specification §8.8) carries `message`,
  the words the view failed with (for a script its error and the line it is on). `Session::view_failed`
  takes them; the session cuts them to `MAX_FAILURE_CHARS` and leaves out control characters and
  bidirectional overrides, as it does for the names a program gives. `message::closed` takes the
  message. `Config::wasm_entry_limit` is `native_entry_limit`: it holds the entry of a `lua` view too.

- A believed `bye` is answered (specification §10.3): the host writes a `closed` with `re` set to
  the `bye`'s id and `reason` `program`, the last frame it writes for the view and the only one it
  writes for a program that has said it ended. A program that reads up to it has read everything
  the host wrote before, so nothing the host had in flight when it learned of the `bye` is left in
  the tty's queue for whatever reads the tty next.

### Added

- Lua views (specification §8.8), the kind `lua`: the entry of the bundle is the text of a script
  (`text/x-lua`), which the terminal runs in the sandbox it runs components in. Everything of a
  `wasm` view holds for it (no MCP session, `msg` routed unread, nothing written once the view is
  over). `ViewKind::Lua`, `ViewKind::is_native` (the kinds the terminal draws itself).
- Kinds of view (specification §5.5), named in the handshake: `hello` has to list `views`, the
  kinds the program can show in its order of preference, and the `welcome` answers with `views`,
  that list without the kinds the host will not open (the host only eliminates; with none in common
  it still answers, with `views: []`, so that the program does not wait). `open` has to say `view`,
  one of the kinds its token's `welcome` offered; the host does not infer the kind from the bundle.
  `Config::views` (`Vec<ViewKind>`, only `ViewKind::Web` by default) says what the host opens;
  `ViewKind::name` and `ViewKind::from_name` are the table of names. A `hello` without `views`
  is not answered, an `open` without `view` or whose entry is not of the kind's type
  (`text/html` for `web`, `application/wasm` for `wasm`) is refused with `manifest`, and an
  `open` of a kind the `welcome` did not offer with `unsupported`. The `welcome` of a restart offers
  every kind the host opens, in the host's order.
- WebAssembly views (specification §8.8), the kind `wasm`: the entry of the bundle is a component
  the terminal runs in a sandbox of its own and draws itself. `ViewKind::Wasm` in `Config::views`
  makes the host offer and take such an `open`, without a `tool`, with an optional `program` and
  `title`, and without an MCP session: the view starts as soon as the files are held,
  `ViewStart::kind` is `ViewKind::Wasm`, and `policy`, `allow` and `tool` are empty.
  `Config::wasm_entry_limit` refuses a component over a size of the host's choosing with `limit`.
  The program's `msg` reaches the terminal as `Action::ToView`, unread (a JSON object, and only
  while the view is open); `Session::module_message` writes the component's message to the program,
  and writes nothing once the view is over, however it ended, so a late message cannot reach the
  tty as typed input. `Session::view_failed` closes a view that failed and tells the program
  `closed` with `reason` `error` and `code` `view`.
- `ViewStart::kind` (`ViewKind`, `#[non_exhaustive]`), `Config::wasm_entry_limit`,
  `Manifest::entry_file`, `bundle::WASM_MEDIA_TYPE`. `ViewStart` and `Config` are not
  `#[non_exhaustive]`, so a terminal that builds one itself adds the fields.

## [0.1.0] - 2026-10-21

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
- `Consent::tool`, `Consent::title` and `Consent::program`: which tool the question to the person is
  about, and the words `ViewStart::title` and `ViewStart::program` carry for it, from the same
  place and cleaned the same way. A terminal that remembers an answer for "the same tool, the same
  domains" keys it by `tool` and no longer reads the `open` for the name itself, which would be a
  second copy of the crate's rules. `tool` is the name the `open` asked for, cut and cleaned like
  the others (`ViewStart::tool` stays the name the view calls), and is `None` when nothing is left
  of it: the question is still asked and there is nothing to remember it by. The three are known
  when the person is asked, since the question comes after `tools/list`. `Consent` is not
  `#[non_exhaustive]`, so a terminal that builds one itself adds the three fields;
  `UiMeta::consent` leaves them `None`, because the metadata does not know the tool.
- `Consent::id` and `Action::WithdrawConsent`: the session numbers each question it asks the person,
  and takes the number back with the answer, so that an answer to a question that is no longer the
  one being asked cannot settle the question of another view. A program that dismisses or ends its
  view, a person who closes it, or a wait that runs out while the question is on the screen ends the
  open, and the session says so with `WithdrawConsent(id)`, ahead of the `closed` it writes: the
  terminal takes the question off the screen. An answer that is already on its way is ignored
  (`Diagnostics::ignored`), which is the whole of what a program opening a view again at once could
  otherwise have used. The number is the session's own, not the `open`'s id, which is the program's
  to choose. `UiMeta::consent` leaves it `0`: it is the session that numbers a question.
- `ViewStart::tool_label`: the name of the tool as a terminal may draw it when the tool has no
  `title`, cut and cleaned as `title` and `Consent::tool` are. `ViewStart::tool` is the name the view
  calls, as the program wrote it, and is not for drawing; a terminal that fell back to it drew text
  the program chose, control characters and bidirectional overrides among it.
- What the program announces reaches the view (specification §9.6): its
  `notifications/tools/list_changed`, `resources/list_changed` and `prompts/list_changed`, when
  it declared `listChanged` for that list in its `initialize` result, and its
  `notifications/progress` for a request the view made with a `progressToken`. Before the view
  hears that the tools changed, the session reads `tools/list` again, so the view can call a tool
  it has just heard of; one reading is out at a time. A view that has not initialized hears
  nothing. The view's `ui/initialize` answer says `listChanged: true` under `serverTools` and
  `serverResources` for what the program announces.
- `ViewStart::tool_icons` and `ViewStart::program_icons`, and the `icon` module (specification
  §7.3): the icons the program gives for the tool (its MCP `icons` in `tools/list`) and for itself
  (`serverInfo.icons`), so a terminal can draw them beside `title` and `program`. Only a base64
  `data:` URI is read; an `https:` icon is never fetched, since fetching it would tell whoever
  serves it who opened the program. The bytes are the program's and a terminal decodes them in its
  own process, so an icon is kept only when its first bytes say PNG, JPEG or WebP (a `mimeType`
  or media type naming another type rejects it), it is at most `MAX_ICON_BYTES` (64 KiB) and
  `MAX_ICON_SIDE` (512) pixels a side by its header, and it is a still image; at most `MAX_ICONS`
  (4) of the first `MAX_ICON_ENTRIES` (16) entries. Each `Icon` carries its format, size, MCP
  `theme` and bytes, and `icon::pick` chooses one for the terminal's background and the size it
  draws at. `Consent` has no icons: a question the terminal asks in its own name must not carry a
  picture the program chose. An icon the session does not take does not stop the view. `ViewStart`
  is not `#[non_exhaustive]`, so a terminal that builds one itself adds the two fields.
- The view lists what the program has and gives up what it asked (§9.4): `resources/list`,
  `resources/templates/list` and `prompts/list` are forwarded like `tools/call`, and the view's
  `notifications/cancelled` reaches the program under the id the session gave the request, whose
  late answer is then dropped.

### Changed

- A view's `tools/list` is forwarded to the program, as the draft of MCP Apps and its reference
  host do; it was refused as a message of the host's own. A view needs it to act on the news
  that the tools changed.
- `Session::consent` takes the question's id: `consent(consent.id, grant)`. An answer for a question
  that is not the one being asked changes nothing. `Consent` and `ViewStart` are not
  `#[non_exhaustive]`, so a terminal that builds either itself adds the new field.
- The names the session passes on (`ViewStart::title`, `program` and `tool_label`, `Consent::tool`,
  `title` and `program`) also lose the invisible format characters (Unicode category Cf): the
  zero-width space, the word joiner, the byte order mark, the soft hyphen, the Arabic letter mark and
  the tag characters, which let a name look like another one. The zero-width joiner and non-joiner,
  which emoji sequences and several scripts are written with, and the left-to-right and right-to-left
  marks, which right-to-left names need for the punctuation beside them, stay. The three flags that
  are written with tag characters (England, Scotland, Wales) show as a black flag.
