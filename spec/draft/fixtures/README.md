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
{ "protocol": 1, "tokens": {...}, "decode": [...], "session": [...] }
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
what a conforming writer produces for `messages`: writing them must give the file back.

A **`session`** entry is a conversation with a host. `config` gives the host's identity, the
tokens it issues, in order, to successive answered hellos, and the digests its store
already holds. `steps` alternate: an `in` step is a file fed to the host; the next `out` step is
the file holding the bytes the host must write in answer, or `null` when it writes nothing.
Message ids in these files are the ids the two sides would use: each side counts its own
messages from 1.

Error names are those of Section 4.7 of the specification: `malformed`, `too-long`,
`chunk-order`, `abandoned`, `message-too-long`. The message limits are too large for a file
to cross (16 MiB and 1 MiB), so tests of `message-too-long` build their stream; the frame
limits are covered by files at the limit and one byte over.

## Decode fixtures

### Program to host (`to-host`)

| File                         | Scope           | What it is and the expected result                                                                                                                           |
| ---------------------------- | --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `to-host/hello.frame`        | body, canonical | the capability query. delivers hello#1; no errors.                                                                                                           |
| `to-host/hello-fence.frame`  | stream          | hello followed by the CSI c fence, as a program sends them. delivers hello#1; no errors.                                                                     |
| `to-host/open.frame`         | body, canonical | open without a bundle. delivers open#2; no errors.                                                                                                           |
| `to-host/open-bundle.frame`  | body, canonical | open with a three-file bundle manifest. delivers open#2; no errors.                                                                                          |
| `to-host/file.frame`         | body, canonical | file in one frame. delivers file#3(d=72735d03…); no errors.                                                                                                  |
| `to-host/file-chunked.frame` | body            | file in two chunks of 60 base64 characters, the sender's choice. delivers file#3(d=bcc81d22…); no errors.                                                    |
| `to-host/msg.frame`          | body, canonical | msg carrying an MCP initialize result. delivers msg#4; no errors.                                                                                            |
| `to-host/msg-chunked.frame`  | body            | msg in three chunks of 100 payload bytes, the sender's choice. delivers msg#5; no errors.                                                                    |
| `to-host/bye.frame`          | body, canonical | bye after SIGINT. delivers bye#6; no errors.                                                                                                                 |
| `to-host/noise.frame`        | stream          | one hello among terminal output, a foreign APC and a frame of an unknown kind (delivered, then ignored by the receiver). delivers hello#1, zzz#2; no errors. |
| `to-host/unknown-kind.frame` | body            | a frame of an unknown kind is delivered by the decoder and ignored by the receiver. delivers zzz#1; no errors.                                               |
| `to-host/unknown-key.frame`  | body            | an unknown control key is ignored but reported with the message. delivers hello#1(x=1); no errors.                                                           |
| `to-host/incomplete.frame`   | stream          | a frame that never ends produces nothing, not even an error. delivers nothing; no errors.                                                                    |
| `to-host/limit-frame.frame`  | body, canonical | a frame of exactly 4096 bytes is accepted. delivers msg#7; no errors.                                                                                        |

### Host to program (`to-program`)

| File                                   | Scope           | What it is and the expected result                                                                               |
| -------------------------------------- | --------------- | ---------------------------------------------------------------------------------------------------------------- |
| `to-program/welcome.frame`             | body, canonical | the answer to a hello. delivers welcome#1(re=1); no errors.                                                      |
| `to-program/welcome-restart.frame`     | body, canonical | an unsolicited welcome that asks a program with an open view to open it again. delivers welcome#2; no errors.    |
| `to-program/welcome-unsupported.frame` | body, canonical | the answer to a hello that shares no version with the host. delivers welcome#1(re=1); no errors.                 |
| `to-program/need.frame`                | body, canonical | the digests the host lacks. delivers need#2(re=2); no errors.                                                    |
| `to-program/need-empty.frame`          | body, canonical | the host has every file. delivers need#2(re=2); no errors.                                                       |
| `to-program/need-chunked.frame`        | body, canonical | twelve missing digests need three frames at the 511 byte limit. delivers need#2(re=2); no errors.                |
| `to-program/closed-refused.frame`      | body, canonical | an open that was refused. delivers closed#2(re=2); no errors.                                                    |
| `to-program/closed-user.frame`         | body, canonical | the person closed the view. delivers closed#5; no errors.                                                        |
| `to-program/closed-error.frame`        | body, canonical | the bundle failed verification. delivers closed#5(re=2); no errors.                                              |
| `to-program/msg-initialize.frame`      | body, canonical | msg carrying an MCP initialize request. delivers msg#3; no errors.                                               |
| `to-program/msg-chunked.frame`         | body, canonical | a view's tools/call, longer than one frame, with non-ASCII text written as escapes. delivers msg#4; no errors.   |
| `to-program/noise.frame`               | stream          | a welcome followed by the terminal's answer to CSI c and stray typed bytes. delivers welcome#1(re=1); no errors. |
| `to-program/limit-frame.frame`         | body, canonical | a frame of exactly 511 bytes is accepted. delivers msg#7; no errors.                                             |

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

## Session fixtures

| Session                | What it shows                                                                | Files                           |
| ---------------------- | ---------------------------------------------------------------------------- | ------------------------------- |
| `handshake`            | a hello is answered with a version and a token                               | `session/handshake/`            |
| `version-mismatch`     | a hello that shares no version gets no token                                 | `session/version-mismatch/`     |
| `open-without-token`   | an open that carries no token is refused                                     | `session/open-without-token/`   |
| `open-wrong-token`     | an open with a token the host never issued is refused                        | `session/open-wrong-token/`     |
| `token-single-use`     | a token opens once; the second open with it is refused and the first goes on | `session/token-single-use/`     |
| `hello-replaces-token` | a new hello invalidates the token of the previous one                        | `session/hello-replaces-token/` |
| `open-busy`            | a second view cannot open while one is opening                               | `session/open-busy/`            |
| `bundle-partial`       | the host names only the digests it lacks, in manifest order                  | `session/bundle-partial/`       |
| `bundle-path-escape`   | a manifest path that leaves the bundle is refused                            | `session/bundle-path-escape/`   |

Within a session directory the files are numbered in the order of the conversation, `NN-in.frame`
for what the program wrote and `NN-out.frame` for what the host writes back.
