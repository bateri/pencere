# Changelog

All notable changes to the `pencere-host` crate (published as `pencere` until its first release
under this name) are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- **`opens` in the `welcome`** (specification §5.2): a kind the program listed that the host opens, but at none of the versions the program speaks, is named with the versions the host does open, so that the program can say which would have served. `message::welcome_offer` takes it.
- **`Action::WriteView`, `Session::backlog` and `session::MAX_BACKLOG`** (§4.8). What a view says reaches the terminal as `Action::WriteView` (a terminal writes it as `Action::Write`, which stays the host's own words). Before it holds one, the terminal asks `backlog(waiting)` with the bytes of `WriteView` that wait for the tty, that one included; it answers `false` for more than 16 MiB, closes the view with `error` `view`, and the terminal holds nothing of the view's words. **A terminal that matches `Action` must handle the new variant**: an arm that ignores what it does not know drops the view's words.
- **`cursor` in a web page's `context.colors`**, after `accent` (made up as the accent): `Theme::cursor` and `Roles::cursor` (§9.5, §11.2).
- **`selection` and `ansi` in a web page's `context.colors`.** `theme::Theme` gains `selection`
  (made up as a third of the way from the background to the accent) and `ansi`, the sixteen colours
  of the palette (made up as xterm's: `theme::XTERM_ANSI`); `Roles` carries both. The page is told
  them after `info`, the palette as an array of sixteen `#rrggbb` strings (specification §9.5,
  §11.2).

### Changed

- **Breaking: `Config::bundle` is gone and the `welcome` no longer lists `bundle` in `features`.** Every view arrives as a bundle, so there was nothing to ask; `features` is `["dismiss"]`.
- **A message nested 128 levels deep or more is dropped** in either direction (§4.5, §9.3), as is one that is not a complete JSON object; a `hello`, `open`, `bye` or `dismiss` that deep is not read either.
- **The default policy of a page has `form-action 'none'`** (§12.1): a form could otherwise submit the page to any address.
- **Breaking: the crate is `pencere-host`** (the library is `pencere_host`), and this is its first
  release under that name, at the version the pencere crates share. The name `pencere` is the
  program library's, the one a program's author adds to a project; nobody used the crate that was
  published under it (0.1.0, below).
- **The wire moved out to `pencere-wire`**, so that a program and a terminal share one copy of it:
  `frame`, `json`, `base64`, `message` and the manifest, digests and verification of files of
  `bundle`. They are re-exported, so `pencere_host::frame` is still where a terminal finds them. What
  stays here is what a terminal decides: `session`, `sandbox`, `theme`, `icon`, `token` and the
  `bundle::Store` it keeps files in. `Manifest::missing` and `Manifest::is_held` take a closure that
  says whether a digest is held, instead of a store; `Manifest::check`, `RawBundle`, `RawFile`,
  `message::probe_token`, `message::OpenBody` and `json::Object` are public, for the crate the
  session lives in is no longer the one that defines them.

- **Breaking: one protocol, with no MCP in it.** Every kind of view speaks the same six view
  messages (specification §9), and the host only carries them; a `web` page additionally exchanges
  the bridge messages `context`, `close` and `link` with the host (§9.4 to §9.7). The wire of the
  draft changed in place, and there is no support for the previous draft.
  - **Gone:** the MCP client toward the program and the MCP Apps host toward the view (`jsonrpc`,
    `session::mcp`, `session::view`, `ToolCall`, `ViewSource`, `UiMeta`, `Theme::variables`,
    `Action::ViewLog`, `Config::logging`), the `tool` of an `open`, `bundle.meta`, `serverInfo` and
    `resources/read`: a view is always a bundle. So is the teardown round trip (`State::Closing`,
    `Session::requests_timed_out`): closing a view is instant for every kind of view
    (specification §10.2).
  - **Kinds with versions.** `hello.views` and `welcome.views` list the kinds of view with the
    versions of their interface (`message::ViewOffer`; `Config::views` is a `Vec<Offer>`, `web` at
    `WEB_BRIDGE_VERSION` by default). The `welcome` lists the kinds the host opens, at the
    versions both speak, in the program's order. `open` names the `view` and its `version`, the
    `program` and `title`, `icons` and, for a page, the `access` it asks to reach
    (`sandbox::Access`: connect, resources, frames, base, permissions); the answer to the person
    and the content security policy are built from it as before. The host only eliminates: with no
    kind in common it still answers, with `views: []`, so that the program does not wait, and a
    `hello` that lists no kind is not answered. An `open` without a `view`, a `version` or a
    bundle, or whose entry is not of the kind's type (`text/html` for `web`, `application/wasm` for
    `wasm`, `text/x-lua` for `lua`), is refused with `manifest`; one of a kind or a version the
    `welcome` did not offer, with `unsupported`; and a `wasm` or `lua` entry over
    `Config::native_entry_limit`, with `limit`. The `welcome` of a restart (specification §10.7)
    offers every kind the host opens, in the order of `Config::views`.
  - **The page.** `ViewStart` carries the page's first `context` message, which the terminal
    delivers when the page has loaded; `Session::set_locale` and `Session::set_reduce_motion` join
    `set_size` and `set_theme` in what a later `context` tells it. A page's `{"context":{}}` is
    answered with the current one.
  - `Config::wasm_entry_limit` is `native_entry_limit`: it holds the entry of a `lua` view too.
  - A believed `bye` is answered (specification §10.3): the host writes a `closed` with `re` set to
    the `bye`'s id and `reason` `program`, the last frame it writes for the view and the only one
    it writes for a program that has said it ended. A program that reads up to it has read
    everything the host wrote before, so nothing the host had in flight when it learned of the
    `bye` is left in the tty's queue for whatever reads the tty next.
- **An `access` is bounded and read without a tree** (specification §6.3). A list of origins has
  at most `sandbox::MAX_ORIGINS_PER_LIST` (32) entries and the lists at most `MAX_ORIGINS` (64)
  between them, counted as written whether or not an entry is an origin; an `open` over a bound is
  refused with `limit`, which `Access::from_json` (new; `Access::from_value` stays) reports as
  `AccessError::Limit`. `AccessError` is an enum now (`Malformed`, `Limit`), where it was a unit
  struct. The text is read one entry at a time, a member the crate does not know is ignored, and
  a fault of shape anywhere in it is `Malformed` even when the bound was gone over first.
  `Csp`, `Grant::allow_domain` and `Grant::within` keep their origins in a set beside the list, so
  they are linear in the number of origins.
- **An `open` that is malformed and too big is refused as malformed** (§6.4 puts `manifest` before
  `limit`): the entry's type, the shape of the `access` and the shape of the manifest are all
  checked before the bundle's limits, the native entry limit and the bound of the `access`.
- **`DEFAULT_POLICY` lets a page read its own bundle.** `connect-src` and `font-src` have `'self'`
  (a page's `fetch('./data.json')` works without being allowed anything), and the policy names
  `frame-src 'none'`, `object-src 'none'` and `base-uri 'self'`, so that what a person allows adds
  those origins to it and changes nothing else (§12.1). The network is as closed as it was.
- **View messages.** The 1 MiB limit (`session::MAX_VIEW_MESSAGE`) is counted on the text before
  the escapes of the wire, not on the escaped form, in both directions (a page's text as posted, a
  program's payload with each escape counted as the character it stands for,
  `json::unescaped_len`), and the message limit toward the program is now 6 MiB (`Direction::ToProgram`, specification §4.6) so that a message within 1 MiB is always
  written, whatever its escapes make of its size; `Diagnostics::unwritable` no longer counts it.
  A page's message (`view_message`) or a component's (`module_message`) over the limit ends the
  view as a failure of the view (`closed` with `reason` `error`, `code` `view`), where it was
  dropped and the page waited for ever; the program's own message over the limit is still dropped.
  JSON whitespace between tokens is accepted and left out when the message is written
  (`json::to_frame`). A `msg` toward a view the terminal draws itself, and that view's own messages,
  have to be complete JSON objects, not text that only begins with `{`: both directions use the
  reader that tells a page's `context`, `close` and `link` apart. A page's messages and its
  `close` reach the program in the order the page sent them (§9.3), as they did, now pinned by a
  test.
- `Session::offer_restart` builds the `welcome` of a restart with `offer_for`, so that a restart
  and a `hello` that listed every kind are offered the same.

### Added

- Views the terminal draws itself (specification §8.8): the kinds `wasm` (the entry of the bundle
  is a component) and `lua` (the entry is the text of a script, `text/x-lua`, run in the sandbox
  components run in). `ViewKind::Wasm`, `ViewKind::Lua` and `ViewKind::is_native`, with
  `ViewKind::name` and `ViewKind::from_name` as the table of names; `Offer` in `Config::views`
  makes the host offer and take them. Such a view starts as soon as its files are held, with no
  document and no web view. The program's messages reach the terminal as `Action::ToView`, unread
  (a JSON object, and only while the view is open); `Session::module_message` writes the view's
  message to the program, and writes nothing once the view is over, however it ended, so a late
  message cannot reach the tty as typed input. `Session::view_failed` closes a view that failed
  and tells the program `closed` with `reason` `error`, `code` `view` and `message`, the words the
  view failed with (for a script its error and the line it is on); the session cuts them to
  `MAX_FAILURE_CHARS` and leaves out control characters and bidirectional overrides, as it does
  for the names a program gives.
- `ViewStart::kind` and `ViewStart::version`, `bundle::WASM_MEDIA_TYPE`, `bundle::LUA_MEDIA_TYPE`
  and `Manifest::entry_file`. `ViewStart` and `Config` are not `#[non_exhaustive]`, so a terminal
  that builds one itself adds the fields.

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
