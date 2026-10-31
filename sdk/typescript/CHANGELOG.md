# Changelog

All notable changes to `@pencere/sdk` are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the package follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- **Files beside the view's bundle** (specification §8.8 to §8.10), for a view opened at `lua` 2.1
  or `wasm` 3.0 and later: `ViewControl.attach(bytes)` adds a file for the view to draw and resolves
  with the name it draws it by (`sha256:` and the digest) once the host holds it, sending nothing
  for a file the view has attached already or the host still holds; `detach`, `read(handle, offset,
length)` of a file the person gave and `write(handle, offset, bytes, end)` to the place the person
  chose in the Save panel are the messages themselves. A refusal is a `FileError` with the host's
  code; `MAX_PIECE` is the most a read asks for or a write carries.
- **The host's folder** (specification §8.11), for a host that lists `stage` in its `welcome`:
  `WelcomeOffer.stage` says where it is. `ViewControl.stageFolder()` proves it (an absolute path; a
  directory of the program's own user that the group and others cannot use; a `marker` file, opened
  without following a link, that holds exactly the token) and returns a `StageFolder` whose
  `openPipe(name, "read" | "write")` opens a pipe the host named, without following a link and without
  blocking, if it is a FIFO of the program's user. `ViewControl.stage(handle, offset)` and
  `ViewControl.fetch(handle, size, offset)` send the file of a handle, or the program's file for the
  place of a handle, through that pipe, and answer a `Transfer`: the `pipe` to open, `done` with where
  the file ended, `unstage()` to take it back. A `FileError` carries `at` with the `route` of a
  `fetch`. The bytes cross no message; what the program asks and what it is told stay messages.

## [0.2.0] - 2026-10-25

### Added

- **`.luau` is `text/x-lua`** in a bundle made from a directory, as `.lua` is: the scripts a Lua
  view requires, by Luau's rules, are those of its bundle that are of that type.
- **A host that opens a kind of the program's in other versions is told in words.** The `no-view` result with the reason `no-common-view` carries a `message` that names both sides, and `serve` writes it to standard error as `pencere: …` before the `fallback` runs (specification §5.2, §5.3). `WelcomeOffer.opens` is the host's list; `noVersionSentence` makes the sentence.
- `Colors` of a page's `context` has `cursor`, `selection` and `ansi` (§9.5).
- **Deno and Bun**, beside Node (20 and later): the same code on all three. What is Node's — the
  tty's modes, the signals, the files, `Buffer` — sits behind a small layer that each runtime
  answers in its own way; the rule of §4.9 is the same on all three (`stty`, ISIG on). Deno needs
  `--allow-read --allow-run`; a program compiled with `deno compile` works, and the `containers`
  example has the task. On Bun 1.0 a `filter` finds no host (its stream for `/dev/tty` delivers
  nothing).
- **`@pencere/sdk/page`**: the bridge of a page (§9.4) as a browser ES module with no
  dependency — `call` (with `onProgress` and an `AbortSignal` that sends `cancel`), `subscribe`
  (with `onPush`, `onError` and `onEnd`), `onContext` and `context()`, `close`, `openLink`, and
  `createBridge` for a channel of your own. `bundleFromDirectory` takes a `files` option for the
  module and anything else that is not in the directory.
- **`createRouter`**: the program's dispatch of the view's `call`, `subscribe` and `cancel`
  to the handlers, with the errors mapped as `serve` maps them, without a terminal, for tests and
  development harnesses. `serve` uses it.
- **⌃Z.** SIGTSTP closes the view (`dismiss`, or `bye` for a host that does not list it), gives the
  screen and the modes back, and stops the program; when it is continued the program takes the
  modes again and asks for the view anew. Before, ⌃Z left the tty raw in the shell.

### Fixed

- **⌃C ends the program every time, on Node.** `stty` ran with the tty as its standard input,
  and Node turns `O_NONBLOCK` off on a child's standard streams: the flag belongs to the open
  file, so the program's own reads of its input began to wait. ⌃C flushes the tty's input
  (Appendix B), and when it emptied the queue between the moment the event loop learned of bytes
  and its read, the read waited for a byte that never came; the SIGINT was never heard, no `bye`
  went out and the program had to be killed. `stty` now gets the tty as descriptor 3, which leaves
  the flag alone.
- A process that exits with its view up reads the host's last word before it leaves. The
  `exit` handler said `bye`, threw away what the tty held and restored the modes at once, so the
  host's `closed` landed in the shell as typed text; it now reads the tty itself, with no event
  loop, until the answer to the `bye` or `byeTimeout`, as the signal path waits (§10.3).
- A signal, or an abort, before the host has answered the `hello` throws away what the host and
  the terminal still write: it waits for the terminal's answer to `CSI c` (`byeTimeout` at most)
  and empties the input. A late `welcome` and the reply no longer reach the shell (§5.3). The same
  holds for a process that exits in that time.
- `serve` checks the list of views once, not twice.
- The 1 MiB limit of a message the program writes to its view is counted as the host counts it
  (specification §9.3): on the text before the escapes of the wire, in bytes. A call answered with
  nearly 1 MiB of Turkish text went through the wire as 3 MiB of `\uXXXX` and was refused as too
  long; it goes through now.

### Changed

- **Breaking: `FEATURE_BUNDLE` is gone.** The `welcome` no longer lists `bundle` (§8.1). A message nested 128 levels deep or more is ignored (§4.5), as is one with an `id` above 2^53 - 1.
- **Breaking: one protocol, with no MCP in it.** `serve` no longer takes an MCP server and a
  program is no longer an MCP server on stdio: it lists the views it can show (`views`: kind,
  versions, bundle and, for a page, its `access`), answers the view's calls (`calls`), feeds its
  subscriptions (`topics`) and learns how the run ended from the result (`ServeResult`: `closed`
  with who ended it, or `no-view` with why). `@modelcontextprotocol/*`, `ext-apps` and `zod` are no
  dependencies any more, and the bundle has no `meta`. The wire of the draft changed in place, and
  there is no support for the previous draft.
  - **The six messages.** The view and the program talk in `call`/`response`, `subscribe`/`push`,
    `cancel` and `progress` (specification §9); a handler gets a `CallContext` (`signal`,
    `progress`) or a `Subscription` (`push`, `signal`), and ends in error with a `ViewError`. A page
    reaches the host through the bridge of §9.4, which `serve` does not read.
  - **Kinds with versions.** `hello` lists `views`, the kinds the program can show with the
    versions of their interface, in its order of preference; the `welcome` is read for the ones the
    host opens, and a host that opens none of them ends `serve` at once with the `no-view` reason
    `no-common-view` instead of after the wait for an answer. `open` names the `view` and its
    `version`, the `program`, `title`, `icons` and a page's `access`.
  - `ViewControl.reopen` takes `views` where it took a bundle.
  - After a `bye` the program waits for the host's `closed`, reading and throwing away the input,
    so that nothing the host wrote is left in the tty for whatever reads it next
    (specification §10.3; `byeTimeout`). A process that exits reads the tty itself.
  - The `closed` of a view the host draws itself that failed carries `message`, the words it
    failed with (`NoView.message`, `ServeResult.message`, specification §8.8). `.lua` files are
    served as `text/x-lua`.
  - A message the host writes into the program's input may be up to 6 MiB on the wire (`LIMITS`,
    specification §4.6): the 1 MiB limit of a view's message is counted before the host escapes it,
    and the escapes can make it six times as long.

### Added

- Views of the kinds `wasm` and `lua`, which the terminal draws itself (specification §8.8), beside
  `web`: `ViewSpec.versions` says which versions of the kit's interface they are written against,
  and `ViewSpec.bundle` may be a function of the version that was agreed.

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
