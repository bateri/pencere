# Fixtures

Byte-exact examples of the pencere wire and the expected result of each. The
Rust crate and the TypeScript SDK are both tested from these files, so that two
implementations that pass agree on the wire; the examples in the specification are
checked against them by `tools/check-spec.mjs`, which `make check` runs.

The files are raw bytes. A file holds a stream: the frames of one example, with the
noise around them where the example is about noise. Nothing here is written with a
newline at the end unless the stream has one. A fixture that a published version of
the specification has used is never edited; a change of the wire is a new version
and new files.

Tokens in the files are fixed test values. The host identity is
`pencere-test` `0.0.0`. The bundle in the examples is three files: `index.html`,
`app.js` and `style.css`, with their real SHA-256 digests.

## `manifest.json`

```
{ "protocol": 1, "tokens": {...}, "decode": [...], "session": [...], "bridge": [...], "message": [...] }
```

A **`decode`** entry has `file`, `direction` (`to-host`: bytes the program writes;
`to-program`: bytes the host writes into the tty's input), `scope`, `canonical`,
`messages`, `errors` and a `note`. Feeding the file to a receiver of that direction,
whole and split at every byte boundary, delivers exactly `messages` — each
`{ kind, id, control, payload }`, where `control` holds the first chunk's keys other than
`id`, `seq` and `more`, and `payload` is the reassembled payload text — and reports exactly
`errors`, in order. Decoders deliver every well-formed message whatever its kind. `scope`
`stream` needs a decoder that finds frames in raw bytes (noise, ESC, C0, bytes above 0x7E);
`scope` `body` can also be run through a body decoder fed the body of each frame, which is
what a host gets from its terminal parser. `canonical: true` means the bytes are exactly
what a conforming writer produces for `messages`: writing them must give the file back. `relayed: true` marks a payload that is a page's text passed on as it came, which is not
compact JSON (the spaces between its tokens are kept).

A **`session`** entry is a conversation with a host. `config` gives the host's identity, the
tokens it issues, in order, to successive answered hellos, the digests its store
already holds and, when it is not the default of `web` at `1.0` alone, `views`: the kinds it opens,
each with the versions it speaks, in its own order. `steps` alternate: a step that
happens to the host is followed by an `out` step that says what the host does. What happens is either an
`in` step, a file fed to the host (the program's bytes), or an `event` step, something on the terminal's side:
`observe` (with `alternate-screen`: the pane is on the alternate screen or not), `allow-opens` (with
`allow`), `consent` (with `grant`, `all` or `none`: the person answers the question that was asked last),
`backlog` (with `waiting`: the terminal asks whether it may hold that many bytes of what the view says), `timeout`, `close-view` (the person closes the view), `restart`, `program-ended`, `set-size` (with `width`
and `height`), `grant-file` (with `size`: the person gave the view a file) and `grant-save` (the person chose a place to save in), whose handle is the next value of the host's randomness written in hex, as the bytes of the next token in `tokens`, `file-read` (with `read`, the id of the program's read, and `bytes` or `error`: what the terminal read), `file-written` (with `write` and, if it failed, `error`), `module-pick` (with `multiple`) and `module-save` (with `name` and `type`: a view the terminal draws itself asks for a panel), `panel-closed` (with `panel`) and `page` (the page sends a message: the bridge file named by `file`, or, with `pad`, a
message of exactly that many bytes, `{"pad":"aaa…"}`). The `out` step has `out`, the file holding the bytes the host
must write in answer, or `null` when it writes nothing; it may also have `acted`, the other actions the
host takes, in order (`consent`, `withdraw-consent`, `start-view`, `to-view`, `open-link`,
`destroy-view`, `read-file`, `write-file`, `abort-write`, `forget-files`, `pick`, `save`), and `to-view`, the files whose text the host passes to the view. Message ids in
these files are the ids the two sides would use: each side counts its own messages from 1.

Error names are those of Section 4.7 of the specification: `malformed`, `too-long`,
`chunk-order`, `abandoned`, `message-too-long`. The message limits are too large for a file
to cross (16 MiB and 6 MiB), so tests of `message-too-long` build their stream; the frame
limits are covered by files at the limit and one byte over.

## Decode fixtures

### Program to host (`to-host`)

| File                             | Scope           | What it is and the expected result                                                                                                                                                                                   |
| -------------------------------- | --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `to-host/hello.frame`            | body, canonical | the capability query: the kinds of view the program can show, each with the versions of its interface it speaks. delivers hello#1; no errors.                                                                        |
| `to-host/hello-fence.frame`      | stream          | hello followed by the CSI c fence, as a program sends them. delivers hello#1; no errors.                                                                                                                             |
| `to-host/hello-views.frame`      | body, canonical | a hello with three kinds and, for one, two versions in the order the program prefers them. delivers hello#1; no errors.                                                                                              |
| `to-host/open-web.frame`         | body, canonical | open for a web page: the kind and its version, the names, what the page asks to reach, and a three-file bundle. delivers open#2; no errors.                                                                          |
| `to-host/open-wasm.frame`        | body, canonical | open for a view that is a WebAssembly component: the kind and its version, the program's and the view's names, an entry of type application/wasm. delivers open#2; no errors.                                        |
| `to-host/open-lua.frame`         | body, canonical | open for a view that is a Lua script: the kind and its version, the program's and the view's names, an entry of type text/x-lua. delivers open#2; no errors.                                                         |
| `to-host/open-lua-modules.frame` | body, canonical | open for a view that is a Lua script of three files: the entry, and two it may `require`, in directories of the bundle (`parts/init.luau`, `screens/logs.lua`), each of type text/x-lua. delivers open#2; no errors. |
| `to-host/open-icons.frame`       | body, canonical | open with an icon for the program: a data URI of a PNG. delivers open#2; no errors.                                                                                                                                  |
| `to-host/file.frame`             | body, canonical | file in one frame. delivers file#3(d=72735d03…); no errors.                                                                                                                                                          |
| `to-host/file-chunked.frame`     | body            | file in two chunks of 60 base64 characters, the sender's choice. delivers file#3(d=bcc81d22…); no errors.                                                                                                            |
| `to-host/msg-response.frame`     | body, canonical | msg carrying the program's response to a call of the view. delivers msg#4; no errors.                                                                                                                                |
| `to-host/msg-chunked.frame`      | body            | msg in three chunks of 59 payload bytes, the sender's choice. delivers msg#5; no errors.                                                                                                                             |
| `to-host/msg-push.frame`         | body, canonical | msg carrying a value of a subscription of the view. delivers msg#4; no errors.                                                                                                                                       |
| `to-host/msg-progress.frame`     | body, canonical | msg carrying how far a call that takes a while has got. delivers msg#4; no errors.                                                                                                                                   |
| `to-host/bye.frame`              | body, canonical | bye after SIGINT, with the token the view was opened with. delivers bye#6; no errors.                                                                                                                                |
| `to-host/dismiss.frame`          | body, canonical | dismiss: the program closes its view and keeps running, with the token the view was opened with. delivers dismiss#6; no errors.                                                                                      |
| `to-host/noise.frame`            | stream          | one hello among terminal output, a foreign APC and a frame of an unknown kind (delivered, then ignored by the receiver). delivers hello#1, zzz#2; no errors.                                                         |
| `to-host/attach.frame`           | body, canonical | attach: two files the program adds to the view that runs, each by its digest and size. delivers attach#7; no errors.                                                                                                 |
| `to-host/detach.frame`           | body, canonical | detach: the program lets go of a file it attached. delivers detach#9; no errors.                                                                                                                                     |
| `to-host/read.frame`             | body, canonical | read: bytes of a file the person gave, by its handle. delivers read#4; no errors.                                                                                                                                    |
| `to-host/write.frame`            | body, canonical | write: bytes for the place the person chose to save in, with its handle and offset. delivers write#5(h=664de63f…, o=0); no errors.                                                                                   |
| `to-host/write-end.frame`        | body, canonical | write: the last bytes of the file, with end. delivers write#6(h=664de63f…, o=6, end=1); no errors.                                                                                                                   |
| `to-host/write-chunked.frame`    | body, canonical | write in two chunks: the handle and the offset on the first chunk only, the base64 cut on four characters. delivers write#5(h=664de63f…, o=4096); no errors.                                                         |
| `to-host/unknown-kind.frame`     | body            | a frame of an unknown kind is delivered by the decoder and ignored by the receiver. delivers zzz#1; no errors.                                                                                                       |
| `to-host/unknown-key.frame`      | body            | an unknown control key is ignored but reported with the message. delivers hello#1(x=1); no errors.                                                                                                                   |
| `to-host/incomplete.frame`       | stream          | a frame that never ends produces nothing, not even an error. delivers nothing; no errors.                                                                                                                            |
| `to-host/limit-frame.frame`      | body, canonical | a frame of exactly 4096 bytes is accepted. delivers msg#7; no errors.                                                                                                                                                |

| `to-host/open-wasm-image.frame` | body, canonical | open for a view of the kit at the version 1.0 of its interface, with a picture in its bundle beside its entry. delivers open#2; no errors. |
| `to-host/file-image.frame` | body, canonical | the picture of the open above as a file message. delivers file#3; no errors. |

### Host to program (`to-program`)

| File                                    | Scope           | What it is and the expected result                                                                                                                                                                                       |
| --------------------------------------- | --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `to-program/welcome.frame`              | body, canonical | the answer to a hello: a version, a token and, for each kind of view the host opens, the versions of it that both speak. delivers welcome#1(re=1); no errors.                                                            |
| `to-program/welcome-restart.frame`      | body, canonical | an unsolicited welcome that asks a program with an open view to open it again. delivers welcome#2; no errors.                                                                                                            |
| `to-program/welcome-unsupported.frame`  | body, canonical | the answer to a hello that shares no version with the host. delivers welcome#1(re=1); no errors.                                                                                                                         |
| `to-program/welcome-wasm.frame`         | body, canonical | the answer of a host that shows WebAssembly views: wasm among the views, with its version. delivers welcome#1(re=1); no errors.                                                                                          |
| `to-program/welcome-opens.frame`        | body, canonical | the answer to a program that listed wasm at 2.0 and web at 1.0 for a host that opens the kit at 1.0: wasm is left out of views and named in opens, with the version the host opens. delivers welcome#1(re=1); no errors. |
| `to-program/msg-pretty.frame`           | body, canonical | what the host writes for bridge/call-pretty.json: the line feeds between the tokens gone and the spaces kept. delivers msg#3; no errors.                                                                                 |
| `to-program/msg-escapes.frame`          | body, canonical | what the host writes for bridge/call-escapes.json: the accent, the DEL and the character outside the first plane as escapes. delivers msg#3; no errors.                                                                  |
| `to-program/welcome-versions.frame`     | body, canonical | the answer to the hello of three kinds: a kind with two versions in common, in the program's order; a kind the host does not open is left out. delivers welcome#1(re=1); no errors.                                      |
| `to-program/welcome-no-views.frame`     | body, canonical | the answer of a host that opens none of the kinds the program listed. delivers welcome#1(re=1); no errors.                                                                                                               |
| `to-program/need.frame`                 | body, canonical | the digests the host lacks. delivers need#2(re=2); no errors.                                                                                                                                                            |
| `to-program/need-empty.frame`           | body, canonical | the host has every file. delivers need#2(re=2); no errors.                                                                                                                                                               |
| `to-program/need-chunked.frame`         | body, canonical | twelve missing digests need three frames at the 511 byte limit. delivers need#2(re=2); no errors.                                                                                                                        |
| `to-program/closed-refused.frame`       | body, canonical | an open that was refused. delivers closed#2(re=2); no errors.                                                                                                                                                            |
| `to-program/closed-user.frame`          | body, canonical | the person closed the view. delivers closed#5; no errors.                                                                                                                                                                |
| `to-program/closed-program.frame`       | body, canonical | the answer to a dismiss: the view is gone. delivers closed#5(re=6); no errors.                                                                                                                                           |
| `to-program/closed-error.frame`         | body, canonical | the bundle failed verification. delivers closed#5(re=2); no errors.                                                                                                                                                      |
| `to-program/closed-error-view.frame`    | body, canonical | closed for a view the host draws itself that failed: reason error, code view, and the words it failed with. delivers closed#5(re=2); no errors.                                                                          |
| `to-program/msg-call.frame`             | body, canonical | msg carrying a call of the view. delivers msg#3; no errors.                                                                                                                                                              |
| `to-program/msg-subscribe.frame`        | body, canonical | msg carrying a subscription of the view. delivers msg#3; no errors.                                                                                                                                                      |
| `to-program/msg-cancel.frame`           | body, canonical | msg carrying the view giving up a subscription. delivers msg#4; no errors.                                                                                                                                               |
| `to-program/msg-chunked.frame`          | body, canonical | a view's call, longer than one frame, with non-ASCII text written as escapes. delivers msg#4; no errors.                                                                                                                 |
| `to-program/noise.frame`                | stream          | a welcome followed by the terminal's answer to CSI c and stray typed bytes. delivers welcome#1(re=1); no errors.                                                                                                         |
| `to-program/need-refused.frame`         | body, canonical | need answering an attach: one digest missing, one refused for a limit. delivers need#3(re=7); no errors.                                                                                                                 |
| `to-program/data.frame`                 | body, canonical | data: the bytes a read asked for, in base64. delivers data#4(re=4); no errors.                                                                                                                                           |
| `to-program/data-error.frame`           | body, canonical | data with no bytes and why: the file changed since the person gave it. delivers data#4(re=4, e=gone); no errors.                                                                                                         |
| `to-program/data-chunked.frame`         | body, canonical | data over several frames of 511 bytes, the base64 cut on four characters. delivers data#4(re=4); no errors.                                                                                                              |
| `to-program/wrote.frame`                | body, canonical | wrote: the file the program wrote is complete. delivers wrote#5(re=6); no errors.                                                                                                                                        |
| `to-program/wrote-error.frame`          | body, canonical | wrote: a write came at the wrong offset. delivers wrote#5(re=6); no errors.                                                                                                                                              |
| `to-program/noise-around.frame`         | stream          | a welcome among typed bytes, another protocol's string and the terminal's answer to CSI c. delivers welcome#1(re=1); no errors.                                                                                          |
| `to-program/noise-between-chunks.frame` | stream          | the three chunks of one message with typed text and a terminal answer between them. delivers msg#4; no errors.                                                                                                           |
| `to-program/limit-frame.frame`          | body, canonical | a frame of exactly 511 bytes is accepted. delivers msg#7; no errors.                                                                                                                                                     |

### Refused, program to host

| File                                        | Scope  | What it is and the expected result                                                                                                          |
| ------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `invalid/to-host/frame-too-long.frame`      | body   | a frame of 4097 bytes is refused. delivers nothing; errors `too-long`.                                                                      |
| `invalid/to-host/esc-in-body.frame`         | stream | an ESC inside the body ends the frame as malformed. delivers nothing; errors `malformed`.                                                   |
| `invalid/to-host/raw-lf.frame`              | stream | a raw line feed inside the body. delivers nothing; errors `malformed`.                                                                      |
| `invalid/to-host/raw-del.frame`             | stream | a raw DEL inside the body. delivers nothing; errors `malformed`.                                                                            |
| `invalid/to-host/non-ascii.frame`           | stream | a raw non-ASCII character inside the body. delivers nothing; errors `malformed`.                                                            |
| `invalid/to-host/c1-st.frame`               | stream | U+2700 whose UTF-8 encoding contains the byte 0x9C, which some parsers read as ST. delivers nothing; errors `malformed`.                    |
| `invalid/to-host/missing-id.frame`          | body   | no id. delivers nothing; errors `malformed`.                                                                                                |
| `invalid/to-host/zero-id.frame`             | body   | id 0. delivers nothing; errors `malformed`.                                                                                                 |
| `invalid/to-host/duplicate-key.frame`       | body   | a control key twice. delivers nothing; errors `malformed`.                                                                                  |
| `invalid/to-host/bad-value.frame`           | body   | id is not decimal. delivers nothing; errors `malformed`.                                                                                    |
| `invalid/to-host/bad-kind.frame`            | body   | kind has an uppercase letter. delivers nothing; errors `malformed`.                                                                         |
| `invalid/to-host/bad-more.frame`            | body   | more is neither 0 nor 1. delivers nothing; errors `malformed`.                                                                              |
| `invalid/to-host/bad-chunk-order.frame`     | body   | chunk 0 then chunk 2: the open message is discarded. delivers nothing; errors `chunk-order`.                                                |
| `invalid/to-host/chunk-id-mismatch.frame`   | body   | the second chunk belongs to another message. delivers nothing; errors `chunk-order`.                                                        |
| `invalid/to-host/chunk-kind-mismatch.frame` | body   | the second chunk has another kind. delivers nothing; errors `chunk-order`.                                                                  |
| `invalid/to-host/orphan-chunk.frame`        | body   | a continuation chunk with no message open. delivers nothing; errors `chunk-order`.                                                          |
| `invalid/to-host/abandoned-message.frame`   | body   | a new message starts before the previous one ended: the old one is dropped, the new one is delivered. delivers hello#5; errors `abandoned`. |
| `invalid/to-host/bad-handle.frame`          | body   | a handle of 31 hex digits. delivers nothing; malformed.                                                                                     |
| `invalid/to-host/bad-offset.frame`          | body   | an offset with a leading zero. delivers nothing; malformed.                                                                                 |
| `invalid/to-host/bad-end.frame`             | body   | an end that is neither 0 nor 1. delivers nothing; malformed.                                                                                |

### Refused, host to program

| File                                       | Scope  | What it is and the expected result                                                                       |
| ------------------------------------------ | ------ | -------------------------------------------------------------------------------------------------------- |
| `invalid/to-program/frame-too-long.frame`  | body   | a frame of 512 bytes is refused. delivers nothing; errors `too-long`.                                    |
| `invalid/to-program/raw-lf.frame`          | stream | a raw line feed inside a reply: the shell would run a line. delivers nothing; errors `malformed`.        |
| `invalid/to-program/raw-cr.frame`          | stream | a raw carriage return inside a reply. delivers nothing; errors `malformed`.                              |
| `invalid/to-program/raw-etx.frame`         | stream | a raw ETX inside a reply: a tty with ISIG would deliver SIGINT. delivers nothing; errors `malformed`.    |
| `invalid/to-program/raw-del.frame`         | stream | a raw DEL inside a reply: a canonical tty would erase a character. delivers nothing; errors `malformed`. |
| `invalid/to-program/non-ascii.frame`       | stream | a raw non-ASCII character inside a reply. delivers nothing; errors `malformed`.                          |
| `invalid/to-program/esc-in-body.frame`     | stream | an ESC inside the body. delivers nothing; errors `malformed`.                                            |
| `invalid/to-program/bad-chunk-order.frame` | body   | chunk 0 then chunk 2. delivers nothing; errors `chunk-order`.                                            |
| `invalid/to-program/bad-error-code.frame`  | body   | an error code with a capital letter. delivers nothing; malformed.                                        |

## Session fixtures

| Session                    | What it shows                                                                                                                                                                                                                                                                             | Files                               |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------- |
| `handshake`                | a hello is answered with a version and a token                                                                                                                                                                                                                                            | `session/handshake/`                |
| `version-mismatch`         | a hello that shares no version gets no token                                                                                                                                                                                                                                              | `session/version-mismatch/`         |
| `open-without-token`       | an open that carries no token is refused                                                                                                                                                                                                                                                  | `session/open-without-token/`       |
| `open-wrong-token`         | an open with a token the host never issued is refused                                                                                                                                                                                                                                     | `session/open-wrong-token/`         |
| `token-single-use`         | a token opens once; the second open with it is refused and the first goes on                                                                                                                                                                                                              | `session/token-single-use/`         |
| `hello-replaces-token`     | a new hello invalidates the token of the previous one                                                                                                                                                                                                                                     | `session/hello-replaces-token/`     |
| `open-busy`                | a second view cannot open while one is opening                                                                                                                                                                                                                                            | `session/open-busy/`                |
| `bundle-partial`           | the host names only the digests it lacks, in manifest order                                                                                                                                                                                                                               | `session/bundle-partial/`           |
| `bundle-path-escape`       | a manifest path that leaves the bundle is refused                                                                                                                                                                                                                                         | `session/bundle-path-escape/`       |
| `bye-needs-the-token`      | a bye without the token, or with another, ends nothing; the one with it does                                                                                                                                                                                                              | `session/bye-needs-the-token/`      |
| `dismiss-and-reopen`       | a program closes its view without ending and opens one again; the second open finds its file in the store                                                                                                                                                                                 | `session/dismiss-and-reopen/`       |
| `wasm-view`                | a host that shows WebAssembly views says so, asks for the component, hands the program's msg to the view and a dismiss closes it                                                                                                                                                          | `session/wasm-view/`                |
| `wasm-unsupported`         | a host that does not show them does not say so and refuses such an open as unsupported                                                                                                                                                                                                    | `session/wasm-unsupported/`         |
| `dismiss-needs-the-token`  | a dismiss closes the view only with the token that opened it; a token issued while the view was opening outlasts it                                                                                                                                                                       | `session/dismiss-needs-the-token/`  |
| `no-common-version`        | a kind with no version in common with the program is left out of `views` and cannot be opened, but is named in `opens` with the versions the host does open; with no kind left the welcome still has a token                                                                              | `session/no-common-version/`        |
| `several-versions`         | the versions both speak are listed in the program's order; an open names one of them and a version outside the list is refused                                                                                                                                                            | `session/several-versions/`         |
| `manifest-before-limit`    | an open that is malformed and too big is refused as malformed, whichever way it is malformed: the entry of the wrong type, an access of the wrong shape, an access with an entry that is not a string past the bound of its list                                                          | `session/manifest-before-limit/`    |
| `access-limit`             | an access lists at most 32 origins in a list and 64 in all, valid or not; an open over a bound is refused as limit, one at the bound goes on                                                                                                                                              | `session/access-limit/`             |
| `screen-refused`           | a pane that is not on the alternate screen has its open refused as screen; once it is, the same program opens                                                                                                                                                                             | `session/screen-refused/`           |
| `policy-refused`           | a host whose own policy lets no program open a view answers a hello and refuses the open as policy                                                                                                                                                                                        | `session/policy-refused/`           |
| `bundle-corrupt`           | a file whose bytes do not have the digest it is sent under ends the open as error bundle                                                                                                                                                                                                  | `session/bundle-corrupt/`           |
| `bundle-files-ignored`     | a file nobody asked for and a file already verified for this open are ignored; the view starts when the files asked for are all here                                                                                                                                                      | `session/bundle-files-ignored/`     |
| `bundle-duplicate-digests` | two files with the same bytes are one digest: the need names it once and one file completes the bundle                                                                                                                                                                                    | `session/bundle-duplicate-digests/` |
| `bundle-limits`            | a file over 8 MiB, more than 64 MiB in all: refused as limit; exactly 8 MiB a file and 64 MiB in all goes on                                                                                                                                                                              | `session/bundle-limits/`            |
| `bundle-too-many-files`    | a manifest of 1025 files is refused as limit                                                                                                                                                                                                                                              | `session/bundle-too-many-files/`    |
| `files-timeout`            | an open whose files do not come in the time the host chose ends as error timeout                                                                                                                                                                                                          | `session/files-timeout/`            |
| `consent-timeout`          | an open that waits in vain for the person's answer ends as error timeout and the question is taken back                                                                                                                                                                                   | `session/consent-timeout/`          |
| `view-ends`                | the person closing a view and the view asking to close are told to the program as user and view, with no re; nothing the view says afterwards is written; a close is written after what the page said before it                                                                           | `session/view-ends/`                |
| `page-message-too-long`    | a message of the page over 1 MiB ends the view as error view and is not written                                                                                                                                                                                                           | `session/page-message-too-long/`    |
| `restart`                  | a host that restarts a running view tears it down without a closed and offers a new token with restart; the old token is no more                                                                                                                                                          | `session/restart/`                  |
| `messages-outside-a-view`  | a msg of the program reaches no view before one runs and none after it has closed; nothing of the view is written once it is over                                                                                                                                                         | `session/messages-outside-a-view/`  |
| `hello-edges`              | a hello with no kind, or whose protocol versions are not a list of integers, is not answered; a kind listed twice counts once; an empty list of protocol versions shares none                                                                                                             | `session/hello-edges/`              |
| `kit-open-ignores-access`  | the access of an open of a view the host draws itself is not read: a broken one does not refuse the open and a good one asks the person nothing                                                                                                                                           | `session/kit-open-ignores-access/`  |
| `context-resent`           | the page is told its context again when a member changes, and only then                                                                                                                                                                                                                   | `session/context-resent/`           |
| `view-backlog`             | a view whose words wait for the program up to 16 MiB goes on; asked about more, the host closes it as error view                                                                                                                                                                          | `session/view-backlog/`             |
| `attach-files`             | a program attaches two files to its view: the host asks for both, keeps the one that is what its digest says and refuses the other in a second need, sends nothing for a file already attached, forgets one at detach and attaches it again from its store, and refuses a file over 8 MiB | `session/attach-files/`             |
| `attach-without-files`     | an attach while no view runs, and one to a view opened at lua 2.0, are answered with every digest refused for view                                                                                                                                                                        | `session/attach-without-files/`     |
| `read-files`               | the program reads a file the person gave: two reads out, a third busy; bytes and a failure come back as data; an unknown handle and a length over 1 MiB are refused; after a restart the handle is nothing                                                                                | `session/read-files/`               |
| `save-files`               | the Save panel and a second one dropped; a file written to its end; a write at the wrong offset fails the file and later writes are ignored; a view that ends mid-file throws it away; an unknown handle is told                                                                          | `session/save-files/`               |

Within a session directory the files are numbered by step, in the order of the conversation,
`NN-in.frame` for what the program wrote and `NN-out.frame` for what the host writes back; a step
where the host writes nothing has no file and its number is skipped.

## Message fixtures

What a program library reads of the messages a view says (Section 9.2), whatever the host did with
them: each file is the payload of a `msg`, and an entry of **`message`** has `file`, `note` and
`reads`: `null` when the message is ignored, or `{ kind, id }`, the sort of message it is
(`call`, `subscribe` or `cancel`) and the `id` it is read with. The Rust program library and the
TypeScript SDK are both tested from them.

| File                        | What it is and the expected result                                                         |
| --------------------------- | ------------------------------------------------------------------------------------------ |
| `messages/id-max.json`      | the greatest id, 2^53 - 1. Read as a cancel of that id.                                    |
| `messages/id-over.json`     | an id of 2^53. Ignored.                                                                    |
| `messages/id-float.json`    | an id written `1.0`. Read as a cancel of 1.                                                |
| `messages/id-exponent.json` | an id written `1e3`. Read as a cancel of 1000.                                             |
| `messages/id-fraction.json` | an id of `1.5`. Ignored.                                                                   |
| `messages/id-zero.json`     | an id of 0. Ignored.                                                                       |
| `messages/id-string.json`   | an id that is a string. Ignored.                                                           |
| `messages/nested-127.json`  | a call nested 127 levels deep, the outermost object counting as the first. Read as a call. |
| `messages/nested-128.json`  | a call nested 128 levels deep. Ignored.                                                    |

## Bridge fixtures

The messages of Section 9, which a host and a web page exchange in the page's own channel and
which never cross the terminal's stream, so they are not frames: each file is one line of JSON,
without a newline at the end unless the entry is about whitespace. A **`bridge`** entry has `file`,
`direction`, `note` and `outcome`.

- `page-to-host`: a message of the page, fed to the host. `outcome` is what the host does with it:
  `context` (writes its context to the page), `close` (closes the view), `link` (asks the person
  to open the link in `url`), `passed` (hands it to the program as a `msg`, unread; when the entry has
  `wrote`, the host writes exactly the frame in that file) or `dropped` (does nothing, and counts it
  as ignored).
- `program-to-page`: the payload of a `msg` of the program, fed to the host. `outcome` is `passed`
  (the page gets it as it is) or `dropped`.
- `host-to-page`: what a host with the entry's `config` writes to the page, byte for byte
  (`outcome` is `written`). `config` gives the theme's roles (the roles it leaves out are made up,
  Section 11.2), the font, the size of the pane, the person's language and time zone, whether
  motion is reduced and what the person allowed.

| File                             | Direction       | What it is and the expected result                                                                                                                                                                                                         |
| -------------------------------- | --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `bridge/context.json`            | host-to-page    | the host's context for a page: every theme role, its selection and its palette, the terminal's font, the size of the pane, the person's language and time zone, the accessibility setting, and what the person allowed. Outcome `written`. |
| `bridge/context-defaults.json`   | host-to-page    | the context of a host that knows only the background and the foreground of its theme: the other roles, the selection and the palette are made up, and what is not known is left out. Outcome `written`.                                    |
| `bridge/context-request.json`    | page-to-host    | a page that started listening late asks for the context again. Outcome `context`.                                                                                                                                                          |
| `bridge/close.json`              | page-to-host    | the page asks to be closed. Outcome `close`.                                                                                                                                                                                               |
| `bridge/link.json`               | page-to-host    | the page asks to open a link in the person's browser. Outcome `link`. The link is `https://example.com/docs`.                                                                                                                              |
| `bridge/link-not-http.json`      | page-to-host    | a link that is not http or https is dropped. Outcome `dropped`.                                                                                                                                                                            |
| `bridge/call.json`               | page-to-host    | a call of the page goes to the program as it is. Outcome `passed`.                                                                                                                                                                         |
| `bridge/two-members.json`        | page-to-host    | an object with more than one member is nobody's message of the host's: it goes to the program as it is. Outcome `passed`.                                                                                                                  |
| `bridge/not-an-object.json`      | page-to-host    | what is not an object is dropped. Outcome `dropped`.                                                                                                                                                                                       |
| `bridge/response.json`           | program-to-page | the program's answer goes to the page as it is. Outcome `passed`.                                                                                                                                                                          |
| `bridge/context-forged.json`     | program-to-page | a message that is named like one of the host's own is dropped on its way to the page. Outcome `dropped`.                                                                                                                                   |
| `bridge/nested-127.json`         | page-to-host    | a message nested 127 levels deep goes to the program as it is. Outcome `passed`.                                                                                                                                                           |
| `bridge/nested-128.json`         | page-to-host    | a message nested 128 levels deep is dropped. Outcome `dropped`.                                                                                                                                                                            |
| `bridge/nested-127-program.json` | program-to-page | the same from the program: it goes to the page as it is. Outcome `passed`.                                                                                                                                                                 |
| `bridge/nested-128-program.json` | program-to-page | the same from the program, 128 levels: dropped. Outcome `dropped`.                                                                                                                                                                         |
| `bridge/link-max.json`           | page-to-host    | a link of exactly 8192 bytes is put to the person. Outcome `link`.                                                                                                                                                                         |
| `bridge/link-too-long.json`      | page-to-host    | a link of 8193 bytes is dropped; the limit is 8192. Outcome `dropped`.                                                                                                                                                                     |
| `bridge/link-with-space.json`    | page-to-host    | a link with a space in it is dropped. Outcome `dropped`.                                                                                                                                                                                   |
| `bridge/link-non-ascii.json`     | page-to-host    | a link with a character that is not ASCII is dropped. Outcome `dropped`.                                                                                                                                                                   |
| `bridge/call-pretty.json`        | page-to-host    | a call written over several lines: the host writes the frame of to-program/msg-pretty.frame. Outcome `passed`.                                                                                                                             |
| `bridge/call-escapes.json`       | page-to-host    | a call with an accent, a DEL and a character outside the first plane: the host writes the frame of to-program/msg-escapes.frame. Outcome `passed`.                                                                                         |
| `bridge/context-resized.json`    | host-to-page    | the context the page is told when the pane is resized in the session `context-resent` (it is the entry's text, not a bridge entry).                                                                                                        |
