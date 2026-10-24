# pencere

**A program's view in its terminal pane** — specification draft

|                  |            |
| ---------------- | ---------- |
| Status           | Draft      |
| Protocol version | 1          |
| Draft of         | 2026-10-24 |

## 1. Introduction

A program running in a terminal pane — on the person's machine, or on a server reached over plain
ssh with no GPU — can show a view of its own in that pane, the way a full-screen program takes the
screen and gives it back when it ends. The program does not draw the view. It hands the terminal
the files of the view, the terminal runs and draws them, and while the view is up the program and
the view talk to each other in six small messages (Section 9.2).

There are two families of views, and every kind of view speaks the same six messages:

- views the terminal draws itself, in its own font and theme, with the pencere kit: a WebAssembly
  component (`wasm`) or a Lua script (`lua`), run in a sandbox of the terminal's;
- a web page (`web`): HTML and JavaScript in a sandboxed web view of the terminal's, which reaches
  nothing the person did not allow.

This document defines:

- how messages travel between a program and the terminal that hosts it, inside the terminal's own
  byte stream (Section 4);
- how a program finds out the terminal can host a view, and which kinds of view, in which versions
  of their interface, the two share (Section 5);
- who may open a view (Section 6) and where it appears and how it is named (Section 7);
- how the files of a view are loaded once and kept by content (Section 8);
- the six messages a program and its view exchange, and the bridge between a web page and its host
  (Section 9);
- when a view starts and ends (Section 10);
- what the terminal tells a view about itself (Section 11), and what a web page may reach (Section 12);
- what happens when there is no host (Section 13).

Non-goals: letting a program choose where its view appears (a view always takes the pane of the
program that asked; a person who wants it elsewhere splits the pane first and starts the program
there); reaching a host through a terminal multiplexer that does not forward the stream untouched
(such a program sees no host and falls back, Section 5.3); the interface of the kit that a `wasm`
or `lua` view is written against, which has a document of its own; the internals of any platform's
web view.

## 2. Conventions and terms

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
"SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY" and "OPTIONAL" are to be
interpreted as described in BCP 14 (RFC 2119, RFC 8174) when, and only when,
they appear in all capitals. Grammar is ABNF (RFC 5234); JSON is RFC 8259.

| Term         | Meaning                                                                                                                            |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| Host         | A terminal emulator that implements this document: it owns panes, the views and their sandboxes, and the bundle store.             |
| Program      | A process running on a pane's tty that wants to show a view.                                                                       |
| Pane         | A terminal pane: one tty and the screen it draws.                                                                                  |
| View         | What a program shows in its pane: a component, a script or a web page (Section 3).                                                 |
| Kind         | The sort of a view: a name from a closed table, with a version of its interface (Section 5.5).                                     |
| Kit          | What a `wasm` or `lua` view is written against: the component interface `pencere:view` and the Lua page API, the global `pencere`. |
| Page         | A view of the kind `web`.                                                                                                          |
| Person       | The human looking at the pane. The view runs on the person's machine; the program may run on another.                              |
| `to-host`    | The direction program → host: bytes the program writes to its tty.                                                                 |
| `to-program` | The direction host → program: bytes the host writes to the input side of the pane's tty.                                           |
| Frame        | One APC string in this protocol (Section 4.1).                                                                                     |
| Message      | A kind, control parameters and a payload, carried by one or more frames (Section 4.4).                                             |
| Chunk        | One frame of a message.                                                                                                            |
| View message | One of the six JSON objects a program and its view exchange (Section 9.2), carried in the payload of a `msg`.                      |
| Token        | A single-use secret the host gives a program so that it can open a view (Section 6.1).                                             |
| Bundle       | The files of a view, listed with their SHA-256 digests (Section 8).                                                                |

Examples of frames are written with `<ESC>` for the byte 0x1B, one frame per
line. The lines of a block are concatenated with nothing between them, so a
block is the exact stream. The repository's `fixtures/` directory holds the same
bytes as files; every example here names its file in the block's heading and a
tool checks that they match.

## 3. The two families of views

A **kit view** — kind `wasm` or `lua` — is drawn by the terminal itself. Its program hands the
terminal a WebAssembly component or a Lua script; the terminal runs it in a sandbox that gives it
nothing it was not handed, and draws what it asks to draw in the terminal's font and theme. What the
component or the script may ask of the terminal is the interface of the pencere kit: the component
interface `pencere:view` for `wasm`, the page API of the global `pencere` for `lua`. The kit has a
document of its own; this one names it and the version of it that a kind carries (Section 5.5).

A **web view** — kind `web` — is a web page that the terminal shows in a web view of its own, in a
sandbox with no network, no files and no navigation of its own unless the person allows some
(Section 12). It reaches the terminal only through the bridge of Section 9.4.

The two families are opened the same way (Section 6), arrive as a bundle (Section 8), are sized and
named the same way (Section 7), start and end the same way (Section 10), and talk to their program in
the same six messages (Section 9.2), which the terminal carries between the two and does not read.
What differs is who draws the view and, for a page, the few messages it exchanges with the host
itself (Section 9.4).

A host MAY open one family and not the other: its `welcome` says which (Section 5.2).

## 4. Transport

Messages travel in the terminal's own stream: from the program in its output, to
the program in its input. There is no port and no second channel.

### 4.1 Frames

A frame is an APC string (ECMA-48) whose body begins with `pencere;`:

```abnf
frame   = ESC "_" "pencere;" kind ";" [ control ] ";" payload ESC "\"
ESC     = %x1B
kind    = 1*16( %x61-7A / "-" )
control = param *( "," param )
param   = key "=" value
key     = 1*8( %x61-7A )
value   = 1*64( ALPHA / DIGIT / "-" / "_" / "." )
payload = *( %x20-7E )
```

The string terminator is `ESC \` and nothing else. The single byte 0x9C is not
a terminator in this protocol and does not occur in a frame (Section 4.2).
`kind`, `control` and `payload` are separated by the first two `;` after the
prefix; a payload may contain `;`.

```frame fixtures/to-host/hello.frame
<ESC>_pencere;hello;id=1;{"versions":[1],"views":[{"kind":"web","versions":["1.0"]}]}<ESC>\
```

### 4.2 Character set

Every byte of a frame between `ESC _` and `ESC \` is printable ASCII,
0x20 to 0x7E. A frame therefore contains no ESC, no C0 control, no DEL and no
byte of 0x80 or above. This is the rule that makes the protocol safe in both
directions:

- no byte in a frame can be taken for the end of the string by any parser, in
  7-bit or 8-bit mode (0x9C is also a UTF-8 continuation byte);
- no byte in a frame is one a tty's line discipline treats specially by default:
  not CR or LF (a shell would run a line), not ETX, FS or SUB (signals), not
  DEL or the erase, kill and word-erase characters (editing), not XON or XOFF —
  all of them are C0 controls or DEL;
- a terminal that does not support this protocol, or a pipe or log it passes
  through, sees one inert APC string and skips it like any other.

JSON payloads (Section 4.5) are written to satisfy it by escaping; base64
payloads satisfy it by construction.

### 4.3 Control parameters

| Key    | Value                                                  | Meaning                                                            |
| ------ | ------------------------------------------------------ | ------------------------------------------------------------------ |
| `id`   | decimal, 1 to 4294967295, no leading zero              | The message this chunk belongs to. REQUIRED.                       |
| `seq`  | decimal, 0 to 4294967295, no leading zero (except `0`) | The chunk's index in its message. Absent means 0.                  |
| `more` | `0` or `1`                                             | `1`: another chunk of this message follows. Absent means 0.        |
| `re`   | an `id`                                                | The message of the other side that this one answers (Section 4.5). |
| `d`    | 64 lowercase hex digits                                | A file's SHA-256 digest (`file` messages only, Section 8.4).       |

A key may appear once; twice is malformed. A receiver MUST ignore keys it does
not know, and MUST NOT treat a value outside the table's range as anything but
malformed. A sender MUST write the keys in the order `id`, `seq`, `more`,
`re`, `d`, MUST omit `seq` when it is 0 and `more` when it is 0, and MUST NOT
write any other key in this version. Receivers accept
any order and the explicit defaults. (The order exists so that two writers
produce the same bytes; the fixtures depend on it.)

### 4.4 Messages and chunks

A message is a kind, the parameters of its first chunk and a payload. A payload
too long for one frame is split into chunks, each a frame.

1. A sender sends the chunks of a message contiguously and in order, with no
   other frame of this protocol between them, numbering them from `seq` 0.
2. The parameters that belong to a message (`re`, `d`) appear on its first
   chunk only. Later chunks carry `id`, `seq` and `more`; a receiver ignores
   anything else on them.
3. A message ends with the first chunk whose `more` is not 1. Its payload is
   the payloads of its chunks concatenated in order, and nothing is parsed
   before that.
4. A chunk with `seq` 0 starts a message. If another message is open, it is
   dropped (error `abandoned`) and the new one starts.
5. A chunk with `seq` greater than 0 continues the open message and MUST have
   its `id`, its kind and `seq` equal to the open message's, one higher than
   the previous chunk's. Otherwise the open message, if any, is dropped and
   the chunk is ignored (error `chunk-order`).
6. When the payload so far exceeds the message limit (Section 4.6) the message
   is dropped (error `message-too-long`).
7. `file` payloads are base64 split on four-character groups: every chunk but
   the last has a payload length that is a multiple of 4, so a receiver may
   decode as it goes.
8. A sender numbers its messages from 1 and counts up; it does not reuse an
   `id` for the next message. Receivers compare ids for equality only.

A sender SHOULD fill every chunk but the last up to the frame limit, counting
the frame as written: `ESC _ pencere;`, kind, `;`, control, `;`, payload,
`ESC \`. A receiver MUST accept any split.

```frame fixtures/to-host/msg-chunked.frame
<ESC>_pencere;msg;id=5,more=1;{"response":{"id":2,"result":{"containers":[{"id":"c1","nam<ESC>\
<ESC>_pencere;msg;id=5,seq=1,more=1;e":"web","state":"running"},{"id":"c2","name":"db","state":<ESC>\
<ESC>_pencere;msg;id=5,seq=2;"running"},{"id":"c3","name":"cache","state":"exited"}]}}}<ESC>\
```

### 4.5 Message kinds

| Kind      | Direction    | Payload                                         | First-chunk parameters                            | Section |
| --------- | ------------ | ----------------------------------------------- | ------------------------------------------------- | ------- |
| `hello`   | `to-host`    | JSON `Hello`                                    | none                                              | 5       |
| `welcome` | `to-program` | JSON `Welcome`                                  | `re`: the hello answered; none when unsolicited   | 5, 10.7 |
| `open`    | `to-host`    | JSON `Open`                                     | none                                              | 6       |
| `need`    | `to-program` | JSON `Need`                                     | `re`: the open answered                           | 8.3     |
| `file`    | `to-host`    | base64 (RFC 4648 § 4, with padding) of one file | `d`: its digest                                   | 8.4     |
| `msg`     | both         | one JSON object: a view message (9.2)           | none                                              | 9       |
| `closed`  | `to-program` | JSON `Closed`                                   | `re`: the open, dismiss or bye it answers, if any | 6.4, 10 |
| `bye`     | `to-host`    | JSON `Bye`                                      | none (answered by a `closed`, 10.3)               | 10.3    |
| `dismiss` | `to-host`    | JSON `Dismiss`                                  | none                                              | 10.8    |

The JSON types are in Appendix A. A sender MUST NOT send a kind in a direction
the table does not list. A receiver MUST ignore a message of a kind it does not
know; decoders deliver every well-formed message whatever its kind and leave this
to the receiver. A message of a known kind that arrives in the other direction,
or whose payload is not valid JSON where the kind has JSON, is ignored too.

**JSON text.** A JSON payload is one line of compact JSON (no whitespace outside
strings) in which every character the rule of Section 4.2 forbids is escaped.
A sender writes: `"` as `\"` and `\` as `\\`; U+0008, U+0009, U+000A, U+000C and
U+000D as `\b`, `\t`, `\n`, `\f` and `\r`; every other code point below U+0020,
U+007F and every code point from U+0080 up as `\uXXXX` with lowercase hex
digits, a surrogate pair from U+10000 up; everything else as itself. For the
types of Appendix A the members are written in the order given there. A JSON
encoder that only escapes non-ASCII text still writes DEL raw: escape it
yourself. Receivers parse any valid JSON.

```frame fixtures/to-program/welcome.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle","dismiss"],"views":[{"kind":"web","versions":["1.0"]}]}<ESC>\
```

### 4.6 Limits

| Limit                                 | `to-host`           | `to-program`      |
| ------------------------------------- | ------------------- | ----------------- |
| A frame, `ESC _` and `ESC \` included | 4096 bytes          | 511 bytes         |
| A message's payload                   | 16 MiB (16 777 216) | 1 MiB (1 048 576) |

A receiver MUST drop a frame or a message over its direction's limit (errors
`too-long` and `message-too-long`). A sender MAY always send smaller frames.

The two directions differ because the second one passes through the tty's input
queue, which is small and which a program that is not reading does not drain.
511 is derived, not chosen, from the operating limits of that queue on the two
systems examined; the derivation is Appendix B. 4096 is the size kitty's graphics
protocol allows for the payload of a chunk; here it bounds the whole frame. The
message limits are protocol constants, not measurements: they bound what a
receiver has to buffer.

Bundle limits are in Section 8.2.

```frame fixtures/to-program/limit-frame.frame
<ESC>_pencere;msg;id=7;{"pad":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}<ESC>\
```

### 4.7 Receiving

A receiver reads a stream of bytes and finds frames in it. A host that obtains
APC bodies from its terminal parser, rather than raw bytes, follows the same
rules from step 2 on for each body; the parser has done step 1 and the
termination in step 4, and bytes it consumed or discarded (many parsers ignore
C0 controls inside a string) are invisible to the host, which is why senders MUST
NOT rely on any of them.

1. Scan for `ESC _`. Every other byte is noise — terminal output, typed
   characters, the terminal's answer to `CSI c`, other protocols' APC strings
   (`ESC _` followed by anything but `pencere;`) — and is skipped without
   error.
2. After `ESC _`, collect body bytes. Once they differ from the prefix
   `pencere;`, the string is not this protocol's; return to step 1. A string that
   ends before the prefix is complete, however it ends, is not this protocol's
   either: it is skipped without error, and steps 3 and 4 apply only to a string
   whose prefix is complete.
3. A byte below 0x20 other than ESC, or above 0x7E, ends the frame (error
   `malformed`); resume scanning after it.
4. ESC followed by `\` ends the frame. ESC followed by `_` ends it as `malformed`
   and starts a new frame. In any other case the frame ends as `malformed` and
   scanning resumes at the byte after that ESC.
5. A frame longer than the limit ends as `too-long` the moment it exceeds it;
   resume scanning after the current byte (the rest of the frame is noise).
6. A completed frame is parsed with the grammar and Section 4.3. A frame that
   does not parse is dropped (error `malformed`). A frame that does is a chunk
   and goes to Section 4.4.

A frame or message that is dropped has no other effect than ending an open
message. Receivers MAY report the error names — `malformed`, `too-long`,
`chunk-order`, `abandoned`, `message-too-long` — for diagnostics; the
fixtures use them. A stream that ends inside a frame produces nothing.

Delivery is not guaranteed and is not acknowledged. The tty may flush its input
queue (on a signal character unless `NOFLSH` is set, Appendix B), a stream may
be cut, a process may die. Whatever depends on a message therefore has its own
bounded wait (Sections 5.3, 8.4, 10), and a message that does not arrive is
never retried silently by this protocol.

```frame fixtures/invalid/to-host/bad-chunk-order.frame
<ESC>_pencere;msg;id=4,more=1;{"call":{"id":1,<ESC>\
<ESC>_pencere;msg;id=4,seq=2;"method":"list"}}<ESC>\
```

The example is refused with `chunk-order`: chunk 1 is missing, so chunk 2 does not
continue the open message, which is dropped.

### 4.8 Writing to the program

The host writes into the input side of the pane's tty, which accepts a limited
number of bytes while the program does not read (Appendix B).

- A host MUST write without blocking its own event loop, MUST expect a write
  to take fewer bytes than it was given, and MUST keep the rest and write it as
  the tty accepts it. Nothing — a keystroke, another frame — may be written
  between the first and the last byte of a frame; a keystroke that arrives
  meanwhile waits. The one exception is a signal character (below).
- A signal character flushes the tty's input (Appendix B, fact 4), so what is written after it
  is read by whatever reads the tty next, and what is written before it and not yet read is gone.
  A host that writes one (⌃C, ⌃\) for the program of a view MUST therefore put it **first**: it
  drops everything of the view's component it has not written yet, including the rest of a frame
  it has begun, and writes nothing of the component's after it. It does not wait for the frame
  that was under way, and it does not write the character behind frames that are waiting.
- Once the host knows the program has ended (Section 10.3) it MUST NOT write any frame, because
  whatever reads the tty next would receive it as typed input, until a `hello` shows that a program
  is reading again. A view that has closed is not the end of the program: the host still writes the
  `closed` that tells the program (Sections 10.5, 10.6 and 10.8) and answers a later `hello`.

### 4.9 Terminal modes

Before it sends `hello`, a program MUST put its tty into non-canonical mode
(`ICANON` off), switch `ECHO` off and leave `ISIG` on, and it MUST restore the
modes it changed when it stops using the terminal, on every exit path it can
run code on.

- Without non-canonical mode a reply is not read: a canonical tty delivers
  input only up to a line terminator, which a frame never contains, and
  discards input over its queue limit (Appendix B).
- With `ECHO` on the tty would write every reply back onto the screen.
- `ISIG` stays on so that ⌃C reaches the program as SIGINT (Section 10.4).

## 5. Capability query and versions

### 5.1 The query

A program that wants to know whether its terminal can host a view writes a
`hello` frame, then `CSI c` (primary device attributes, `ESC [ c`), then reads
its input. The fence is what tells "no host" from "no answer yet".

A `hello` has two members, and both are required: `versions`, the protocol versions the
program speaks (Section 5.4), and `views`, the kinds of view it can show (Section 5.5), in the
order it prefers them. Each entry of `views` has a `kind`, the name of a kind, and `versions`, the
versions of that kind's interface that the program speaks, in the order it prefers them. A host
ignores a `hello` that lacks either member, whose `views` is empty, or whose `views` is not a list
of objects with a string `kind` and a `versions` that is a list of strings, as it ignores a message
it cannot read. A kind that is listed twice counts once, at its first place.

```frame fixtures/to-host/hello-fence.frame
<ESC>_pencere;hello;id=1;{"versions":[1],"views":[{"kind":"web","versions":["1.0"]}]}<ESC>\
<ESC>[c
```

A program that can show several kinds, one of them in two versions:

```frame fixtures/to-host/hello-views.frame
<ESC>_pencere;hello;id=1;{"versions":[1],"views":[{"kind":"wasm","versions":["0.5","0.4"]},{"kind":"lua","versions":["0.5"]},{"kind":"web","versions":["1.0"]}]}<ESC>\
```

### 5.2 The answer

A terminal that supports this document reads the frames in its output in
stream order, so it handles `hello` before it handles the `CSI c` that follows.
It answers by writing a `welcome` frame into the pane's input **before** it
writes the reply to that `CSI c`. A host MAY decline to answer (Section 6.2); the
program then sees only the reply to `CSI c`.

The host chooses the greatest protocol version both support. If there is one, it issues a
token (Section 6.1) and answers with `version`, `token`, `host`, `features` and `views`,
and `re` set to the hello's id. If there is none, it answers with `supported`
and `host` only.

`features` lists the optional parts of this document the host supports: `bundle`
(Section 8.1) and `dismiss` (Section 10.8). A program uses only what is listed and ignores a
name it does not know.

`views` lists what the host will open: the kinds of the `hello` and, for each, the versions of its
interface that the program and the host both speak. For each entry of the `hello`'s `views`, in
order, the answer has an entry with the same `kind` and `versions` cut down to the versions the host
also offers, in the order the program listed them; an entry whose kind the host will not open, or
that shares no version with the host, is left out. The host only eliminates: it never reorders, and
never adds a kind or a version the program did not list. A name that is not in the table of Section
5.5 is a kind the host will not open. `views` is always present, and it is empty when the program and
the host share no kind at a version: the host answers all the same, with a protocol version and a
token, so that the program knows at once that there is a host and nothing it can show, and does not
wait.

```frame fixtures/session/handshake/02-out.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle","dismiss"],"views":[{"kind":"web","versions":["1.0"]}]}<ESC>\
```

```frame fixtures/to-program/welcome-versions.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle","dismiss"],"views":[{"kind":"wasm","versions":["0.5","0.4"]},{"kind":"web","versions":["1.0"]}]}<ESC>\
```

The host in this example opens `wasm` at `0.4` and `0.5` and `web` at `1.0`; the program's `hello`
(`session/several-versions`) listed `wasm` at `0.6`, `0.5` and `0.4`, `lua` and `web`. The version
`0.6` is left out because the host does not speak it, `lua` because the host does not open it, and the two
versions of `wasm` that remain are in the program's order, not the host's.

```frame fixtures/to-program/welcome-unsupported.frame
<ESC>_pencere;welcome;id=1,re=1;{"supported":[1],"host":{"name":"pencere-test","version":"0.0.0"}}<ESC>\
```

```frame fixtures/to-program/welcome-no-views.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle","dismiss"],"views":[]}<ESC>\
```

### 5.3 What the program concludes

| What the program reads first                                              | Conclusion                                                                                                                                                                                                                                                                      |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A `welcome` with `version` and `token`, `re` equal to its last hello's id | There is a host. If `views` has a kind, go on with Section 6 and open the first it can show, at the first version listed for it. If `views` is empty, there is a host and nothing to open: fall back at once, as with no host (Section 13); do not wait and do not send `open`. |
| The terminal's answer to `CSI c` (`ESC [ ? … c`)                          | There is no host. A `welcome` that arrives later MUST be ignored.                                                                                                                                                                                                               |
| A `welcome` with `supported` and no `version`                             | There is a host that shares no version with this program. Treat as no host.                                                                                                                                                                                                     |
| Nothing                                                                   | After a time, no host. The time SHOULD be 2 seconds; a program MAY wait longer on a slow link.                                                                                                                                                                                  |

2 seconds is a precedent, not a measurement (Appendix D). A program that
concludes "no host" MUST NOT send further frames of this protocol (Section 13).
One that concluded by timeout SHOULD discard a late answer to `CSI c`
(`ESC [ ? … c`) for as long as it still reads the tty, so that it does not reach
a shell as typed text.

A `welcome` is only believed when the program has sent a `hello` and
the `welcome` answers it; an unsolicited `welcome` is handled by Section 10.7 or ignored.

```frame fixtures/to-program/noise.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle","dismiss"],"views":[{"kind":"web","versions":["1.0"]}]}<ESC>\
<ESC>[?62;4cls<CR>
```

(The example is a `welcome` followed by the terminal's answer to `CSI c` and a
typed `ls` with its carriage return; a decoder delivers the one `welcome`.)

### 5.4 Versions

A protocol version is a positive integer. `hello` lists the versions the program speaks.
This document defines version 1. An implementation of version N MUST also
implement version N−1 (when N is greater than 1) and a program SHOULD list both,
so that a program built against one release and a host built against the next
or the previous still meet. A published version's fixtures never change; a
change of the wire is a new version with new fixtures.

The versions of a kind (Section 5.5) are another thing and do not move with this one: they are the
versions of the interface a view of that kind is written against.

The versions of the kit views (`wasm` and `lua`, Section 5.5) follow one rule. A change that no
existing view can notice is a new **minor** version, `1.1` after `1.0`: a new function in the
component interface, a new element or a new colour role in the page API. A change to something a
view already uses is a new **major** version, `2.0`: a type that gains a variant or a field (a new
kind of element, a new role in the interface's colour type, a new member of a record), or a
function that means something else. A host that adopts a new major version offers it together with
the one before, as Section 5.4 asks of the protocol's own versions; a host that has stopped opening
a version refuses an `open` for it as `unsupported` (Section 6.4), and SHOULD say why in words
that name both sides ("this view is written against `pencere:view@0.5`; this terminal opens 1.x and
2.x"). For `lua` the interpreter is the host's: a new element or a new role is an addition for a
script, and what breaks a script is a function of the page API that means something else.

A program MAY send `hello` again at any time; each answered `hello` gets a new
token (Section 6.1).

### 5.5 Kinds of view and their versions

What a view is made of is the program's to say and the host's to accept: it is not inferred
from the files of a bundle. A **kind** of view is named by a string in a closed table that this
document defines, and each kind has **versions**, which name the interface a view of that kind is
written against:

| Name   | The view                                                                                                                               | What its version names                                                                     |
| ------ | -------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `lua`  | A script in the Lua language that the host runs, with an interpreter of its own, in the same sandbox as a component, and draws itself. | The Lua page API of the kit, the global `pencere` (`pencere.version` is the major number). |
| `wasm` | A WebAssembly component that the host runs in a sandbox and draws itself.                                                              | The component interface of the kit, `pencere:view` (`pencere:view@1.0.0` is `1.0`).        |
| `web`  | A web page, shown in a web view of the host's.                                                                                         | The bridge of Section 9.4 between the page and its host: now `1.0`.                        |

A version is a string of two numbers, `major.minor`, such as `0.5`:

```abnf
version = number "." number
number  = "0" / ( %x31-39 *8DIGIT )
```

Versions are compared for equality and for nothing else; the order of a list is the preference of
whoever wrote it. What a version means is the interface's. For the kit, `wasm` `1.0` is the
interface `pencere:view@1.0.0` and `lua` `1.0` is the page API whose `pencere.version` is `1`; the
rule that moves the numbers is in Section 5.4, and a host that updates its kit offers the new major
version and the one before. The examples in this document use made-up versions (`0.4`, `0.5`,
`0.6`) so that they show several at once. For `web`, a change that only adds an optional member to a message of Section 9.4 moves the
minor number, and one that renames or drops a member or a message moves the major. A host MAY open
several versions of a kind — a host that updates its kit offers the one it now has and the one
before — and the program chooses among those both speak (Section 5.2).

The table grows with this document; a name in `hello.views` that a host does not know is a kind it
does not open, and it leaves it out of its `welcome`. A kind for which no version is shared is a kind
the host cannot open for this program, and it leaves it out as well (Section 5.2). The program names
the kind and the version again in its `open` (Section 6.3), and an `open` of a kind, or of a version
of it, that the `welcome` of its token did not list is refused as `unsupported` (Section 6.4).

The kinds `wasm` and `lua` are the **kit views**, the views the host draws itself (Section 8.8). They
have a component or a script for an entry and no document. The kind `web` is a page and has an HTML
document for an entry; it is the one kind that reaches the host with messages of its own (Section 9.4).

## 6. Opening a view

### 6.1 The token

A token is 16 bytes from a cryptographically secure random generator, written as
base64url without padding (RFC 4648 § 5), 22 characters. A host keeps at most one
outstanding token per pane.

- A token is spent by the first `open` that carries it, whatever the outcome of
  that `open`. A token that opened a view stays that view's identity for as long as the
  view is opening or open: the program's `bye` (Section 10.3) is believed only
  when it carries it, and no `open` can spend it again.
- A token is replaced when the host issues another for the same pane (another
  answered `hello`, or Section 10.7). The replaced token is no longer valid.
- A token ends when the pane's program ends, and a host MAY end it sooner. It does
  not end with a view: a token issued while a view was opening or open is
  still good when the view is gone (Section 10.2).
- An `open` with a token that does not match has no effect on the outstanding one.

A program that merely writes to the stream — a file shown with `cat`, a
program in the background — can send `hello` but cannot read the answer, so it
cannot open a view (Section 14).

```frame fixtures/session/token-single-use/05-in.frame
<ESC>_pencere;open;id=3;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"web","version":"1.0","program":"containers","title":"Containers","access":{"connect":["https://api.example.com"],"permissions":["clipboardWrite"]},"bundle":{"entry":"index.html","files":[{"path":"index.html","sha256":"aeba6c19a632d2153a9e336a88b385d438c474a996d5be1408f6cd2ca6898e54","size":132,"type":"text/html"},{"path":"app.js","sha256":"bcc81d22a717681517bc99bbad72438377e02c19583fae67522475fd8b3ca62c","size":67,"type":"text/javascript"},{"path":"style.css","sha256":"72735d03f2b7e5a9ddb8ebe1c5fde2da88aa74292ba7ac608490a94f0ebbac0e","size":61,"type":"text/css"}]}}<ESC>\
```

The session in the fixtures `session/token-single-use` shows this `open` refused:
the token was spent by the first.

### 6.2 Hardening the answer to a hello

The answer is written into the pane's input, where whatever reads the tty
next takes it for typed characters (Section 14). A host SHOULD therefore answer
only when what it can observe fits a program that is waiting for it. The same
holds for the refusal of an `open` whose token is not the outstanding one
(Section 6.4): a stream anyone can write to can send that as easily as a
`hello`, so where a `hello` would go unanswered such an `open` is ignored.

- **Shell integration.** While the shell reports that a prompt is being edited
  (OSC 133 between the end of the prompt and the start of the command), a host
  ignores frames of this protocol.
- **Foreground process group.** On a pane whose tty the host owns locally, a
  host answers only while the tty's foreground process group is not the shell's
  own, that is, while a command is running.
- **Terminal modes.** On the same panes, a host answers only if the tty is
  non-canonical with echo off, the state Section 4.9 requires of a program
  before it sends `hello`; a command that merely prints a file leaves the tty as
  the shell set it.

A pane whose program runs on another machine offers none of the last two to
inspect, and a host skips them there. These checks narrow what a hostile stream
can provoke; the token is what stops it opening a view.

### 6.3 The open request

A program that received a `welcome` with a token writes an `open` message,
having switched to the alternate screen (Section 6.5). Its members, in this order:

- `token`: the token from the `welcome`. REQUIRED;
- `view`: the kind of view (Section 5.5), one of the `views` of the `welcome` that issued the
  token. REQUIRED. The host does not work it out from the bundle;
- `version`: the version of that kind's interface that the view is written against, a string, one
  of the versions the `welcome` listed for the kind. REQUIRED;
- `program`: what the program calls itself, and `title`: what it calls the view. OPTIONAL strings,
  for every kind. They are text the program wrote, so the host cuts them and leaves out control
  characters and bidirectional overrides before it draws them in its own interface
  (Sections 7.3 and 12.4);
- `icons`: OPTIONAL, a list of icons for the program (Section 7.3);
- `access`: OPTIONAL, for a `web` view: the origins and the browser capabilities the page asks to
  reach, which the person is asked about (Section 12.2). It is not read for a `wasm` or `lua` view,
  which reaches nothing that the host does not hand it;
- `bundle`: the bundle of Section 8, REQUIRED for every kind. The `type` of its entry has to be that
  of the kind: `text/html` for `web`, `application/wasm` for `wasm`, `text/x-lua` for `lua`.

`access` has up to five members, each a list of strings that may be left out: `connect`, the origins
the page may send network requests to; `resources`, those it may load scripts, styles, images, fonts
and media from; `frames`, those it may nest as frames; `base`, those a `<base>` element may name; and
`permissions`, the browser capabilities it asks for, from the closed list `camera`, `microphone`,
`geolocation` and `clipboardWrite`. An origin is `scheme://host[:port]` with a scheme of `http`,
`https`, `ws` or `wss`.

```frame fixtures/to-host/open-web.frame
<ESC>_pencere;open;id=2;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"web","version":"1.0","program":"containers","title":"Containers","access":{"connect":["https://api.example.com"],"permissions":["clipboardWrite"]},"bundle":{"entry":"index.html","files":[{"path":"index.html","sha256":"aeba6c19a632d2153a9e336a88b385d438c474a996d5be1408f6cd2ca6898e54","size":132,"type":"text/html"},{"path":"app.js","sha256":"bcc81d22a717681517bc99bbad72438377e02c19583fae67522475fd8b3ca62c","size":67,"type":"text/javascript"},{"path":"style.css","sha256":"72735d03f2b7e5a9ddb8ebe1c5fde2da88aa74292ba7ac608490a94f0ebbac0e","size":61,"type":"text/css"}]}}<ESC>\
```

The `open` of a view the host draws itself is the same without `access`:

```frame fixtures/to-host/open-wasm.frame
<ESC>_pencere;open;id=2;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"wasm","version":"0.5","program":"demopen","title":"Containers","bundle":{"entry":"view.wasm","files":[{"path":"view.wasm","sha256":"93a44bbb96c751218e4c00d479e4c14358122a389acca16205b1e4d0dc5f9476","size":8,"type":"application/wasm"}]}}<ESC>\
```

### 6.4 Host checks and refusal

The host checks an `open` in this order and stops at the first that fails; it
answers a failure with a `closed` message whose `re` is the open's id,
`reason` is `refused` and `code` is:

| `code`        | When                                                                                                                                                                                                                  |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `token`       | The payload has no string `token`, or it is not the outstanding token. Checked before anything else in the payload.                                                                                                   |
| `busy`        | A view is already opening or open in this pane.                                                                                                                                                                       |
| `screen`      | The pane is not on the alternate screen (Section 6.5). A host SHOULD refuse so.                                                                                                                                       |
| `manifest`    | The payload or the bundle manifest is malformed (Sections 6.3 and 8.2): no `view`, no `version`, no bundle, an `access` that is not of the shape of Section 6.3, or an entry whose type is not that of the `view`.    |
| `limit`       | A limit of Section 8.2 is exceeded.                                                                                                                                                                                   |
| `unsupported` | The host does not support what was asked: a `view`, or a `version` of it, that the `welcome` of the token did not list (or a kind that is not in the table of Section 5.5), or a bundle without the `bundle` feature. |
| `policy`      | The host's own policy, or the person's, refuses.                                                                                                                                                                      |

A refusal MUST NOT disturb a view that is already opening or open. A receiver
treats a code it does not know as a refusal without a reason.

```frame fixtures/to-program/closed-refused.frame
<ESC>_pencere;closed;id=2,re=2;{"reason":"refused","code":"token"}<ESC>\
```

### 6.5 Alternate screen

A program SHOULD switch to the alternate screen (`CSI ? 1049 h`) before it sends
`open`, and switch back (`CSI ? 1049 l`) when it has finished with the view and
before it exits. The primary screen is left as it was, which is what makes the
pane come back when the program ends. The same rule gives the host a cheap check:
a request from a program that is not on the alternate screen is suspect, and a
host that checks refuses it with `screen`, so a program that skips the rule is
refused by such hosts.

While a view is shown it covers the pane. The program's output goes on drawing
the alternate screen underneath, and when the view closes that screen is what
the person sees, as after any full-screen program.

## 7. The pane

### 7.1 Placement

A view takes the whole content area of the pane of the program that asked, and
the protocol has no field for another place. A pane shows one view at a time.

### 7.2 Size and focus

The view's container is the content area of the pane: the host draws no border, no padding and no
title around a view inside it. A page is told its size in the `context` of Section 9.5, at its start
and when the pane is resized; a kit view is told it by the kit. What a page or a script asks for in
the way of another size changes nothing: the container is the pane, and its size is the person's.

The keyboard is the view's, except for what Section 11.3 reserves.

### 7.3 Names and icons

A host names a view in its own chrome (a tab, the header of a pane, a list of panes), never
inside the view's container (Section 7.2). What it draws there comes from the program's `open` and
from no message of the view, so a program names itself to a terminal in the same way whatever the
kind of its view:

- `program` is what the program calls itself, and `title` what it calls the view. The program wrote
  them, so a host draws them as text, without control characters, bidirectional overrides or
  invisible format characters, cut to a length of its choosing;
- `icons` is a list of icons for the program. An icon has a `src`, a `data:` URI with base64 data
  (RFC 2397), and may have a `mimeType`, a list of `sizes` (`WxH`, as HTML has them) and a `theme`,
  `light` or `dark`, the background it was drawn for. A member of another shape makes the icon
  unusable.

A program's icon and its metadata are untrusted, and a terminal host treats them so by fetching
nothing:

- A host MUST NOT fetch an icon. It uses only an icon whose `src` is a `data:` URI with
  base64 data. The host draws on the person's machine, where a view has no
  network unless the person allows it (Section 12.2), and a program at the other end of an
  ssh connection has no origin an icon could be checked against: fetching an icon would tell
  whoever serves it who opened the program, from where and when.
- A host uses an icon only when its bytes are, by their own first bytes, an image of a type
  it allows, and MUST NOT draw an SVG icon in a way that runs what it contains. A `mimeType`,
  or a media type in the `data:` URI, that names another type than the bytes makes the icon
  unusable: the media type is advisory, and a mismatch is a reason to reject.
- A host bounds an icon's bytes, its dimensions and its frames before it decodes the image.
- A host MUST NOT draw a program's icon in a question it asks the person in its own name
  (Section 12.2), where the picture could pass for the terminal's own or another
  application's.
- An icon the host does not use, and an `icons` member that is not a list of icons, are no
  reason to refuse the view.

The reference host (`host/rust` in this document's repository) takes PNG, JPEG and WebP, at
most 64 KiB, at most 512 pixels a side and only a still image, and at most four icons of the
first sixteen entries of a list.

```frame fixtures/to-host/open-icons.frame
<ESC>_pencere;open;id=2;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"wasm","version":"0.5","program":"demopen","title":"Containers","icons":[{"src":"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg==","mimeType":"image/png","sizes":["1x1"],"theme":"dark"}],"bundle":{"entry":"view.wasm","files":[{"path":"view.wasm","sha256":"93a44bbb96c751218e4c00d479e4c14358122a389acca16205b1e4d0dc5f9476","size":8,"type":"application/wasm"}]}}<ESC>\
```

## 8. Bundles

A view arrives as a bundle: its files, listed with their digests, so that they cross the stream
once and are kept by content. Every kind of view has one.

### 8.1 Support

A host that supports bundles lists `bundle` in `features` of its `welcome`. A
program sends a manifest only to such a host. Every kind of view arrives as a bundle, so a host that opens any
kind lists `bundle`; a program that does not find it has nothing to open.

### 8.2 The manifest

`open.bundle` has an `entry` (a path) and a list of `files`.
Each file has:

- `path`: relative, `/`-separated, 1 to 255 bytes, every segment one or more of
  `A–Z a–z 0–9 . _ ~ -` and not `.` or `..`; no two files share a path;
- `sha256`: the SHA-256 of the file's bytes, 64 lowercase hex digits;
- `size`: the length of the file in bytes;
- `type`: a media type `type/subtype`, lowercase, without parameters.

`entry` MUST be the path of a file. A path is the only way a view names a file; a host MUST
serve nothing that is not in the manifest.

Limits: 1024 files, 8 MiB (8 388 608 bytes) per file, 64 MiB (67 108 864 bytes)
for the sum of the sizes. These are constants of the protocol, like the message
limits; a host MAY enforce lower ones and then refuses with `limit`.

```frame fixtures/to-host/open-web.frame
<ESC>_pencere;open;id=2;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"web","version":"1.0","program":"containers","title":"Containers","access":{"connect":["https://api.example.com"],"permissions":["clipboardWrite"]},"bundle":{"entry":"index.html","files":[{"path":"index.html","sha256":"aeba6c19a632d2153a9e336a88b385d438c474a996d5be1408f6cd2ca6898e54","size":132,"type":"text/html"},{"path":"app.js","sha256":"bcc81d22a717681517bc99bbad72438377e02c19583fae67522475fd8b3ca62c","size":67,"type":"text/javascript"},{"path":"style.css","sha256":"72735d03f2b7e5a9ddb8ebe1c5fde2da88aa74292ba7ac608490a94f0ebbac0e","size":61,"type":"text/css"}]}}<ESC>\
```

### 8.3 What is missing

For an `open` it accepts that has a manifest, the host answers with a `need`
message whose `re` is the open's id and whose `missing` lists the digests it
does not hold, each once, in the order of their first appearance in `files`.
An empty list means everything is held. Then, without waiting, it starts the
view if nothing is missing (after it has asked the person, when the page asked for something to reach,
Section 12.2), or waits for the files.

```frame fixtures/to-program/need.frame
<ESC>_pencere;need;id=2,re=2;{"missing":["bcc81d22a717681517bc99bbad72438377e02c19583fae67522475fd8b3ca62c"]}<ESC>\
```

The list is as long as it is, so it may take several frames; twelve digests
need three at the 511-byte limit.

```frame fixtures/to-program/need-chunked.frame
<ESC>_pencere;need;id=2,more=1,re=2;{"missing":["d77f315c50d2294575894c81c0f319f2fc5928091b67f83bfff5a42c0b0d5e69","9f125d7a3308611eeeb07efceb88ac5c2be7c6865b156b3d123730261ac07d83","c8dd55bfb6b8f0193384bf8b190e14ce95254c62472d92ba8d4d18e24d9cb6a4","cea4fd8970ce6553945d6dcd8fe396893bf804ef164d6e599c327551b7777e40","b30f1aa1bbd21a1b1f1531a9f6c7db8e1619dd137947971bb95f5873a70dc004","ebd056c5483307f0ccb7d2e865b5beac35051d944e009758d30589f73eadad9f","ee6fe9dd74004625e1422ec6cd1d51962ac91793a1c6f1a18ecec2eea69566<ESC>\
<ESC>_pencere;need;id=2,seq=1;3d","65b79f2d5f4b4171e5960c15b2e10d9c110bd0db8b1515ea2bf480e82d41ca9b","1dfe9f6793a7374e8c5f1fa8330f91bb25e947743d5582cb6634c00286415d6a","ca948819b7563459b90d06a8ed44b3a585985f5f786f2ea3769cb5c5ee01aab2","2562874827ca26abe15817c7ee429218cb10f861fe2fe631cc3a08ba2799fc48","ce7bdd9b27c5daf751e64a5649b2fb80332bfc19a6bf81546fefaa7c6fc6ba54"]}<ESC>\
```

### 8.4 The files

The program sends one `file` message for each digest in `missing`, in that order
if it can. The payload is the file's bytes in base64 and the first chunk carries
`d`, the digest.

```frame fixtures/to-host/file-chunked.frame
<ESC>_pencere;file;id=3,more=1,d=bcc81d22a717681517bc99bbad72438377e02c19583fae67522475fd8b3ca62c;ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoImFwcCIpLnRleHRDb250ZW50ID0g<ESC>\
<ESC>_pencere;file;id=3,seq=1;ImhlbGxvIGZyb20gcGVuY2VyZSI7Cg==<ESC>\
```

The host verifies each file before it keeps it: the decoded length is within the
`size` the manifest gave for that digest and the SHA-256 of the bytes equals `d`.
A file it did not ask for, or one it already has verified for this open, is
ignored and not stored. A file that fails verification ends the open with
`closed` `reason` `error`, `code` `bundle`. The host SHOULD end an open whose files do
not arrive in a time of its choosing with `error`, `timeout`.

```frame fixtures/to-program/closed-error.frame
<ESC>_pencere;closed;id=5,re=2;{"reason":"error","code":"bundle"}<ESC>\
```

### 8.5 The store

The store is content-addressed: a file is kept under its verified digest and under
nothing else, so two files with the same bytes are one, and a file cannot be kept
under a digest it does not have. How much a host keeps and for how long is its own
policy. A host MAY drop any entry at any time, and a program MUST therefore be
able to send every file of its manifest at any open.

### 8.6 Serving

When all missing files have been verified, the host serves the view from the
store: the entry as the document and the other files at their paths, each with
the `Content-Type` of its `type`. The view's origin is unique to this instance of the
view; it is never the host's and never shared with another view, and `'self'` in
the content security policy means that origin. No path outside the manifest
answers.

### 8.7 The entry

The entry of a bundle is the view's own file, and its media type is that of the kind
(Section 6.3): an HTML document for a page, a component for `wasm`, a script for `lua`. The host does
not decide the kind from the entry, and it does not guess. A page may have other files, which it names
by their paths; the other kinds have the entry only, unless the kit says otherwise.

### 8.8 Views the host draws itself

A view of the kind `wasm` or `lua` (Section 5.5) is one the host draws itself, in the terminal's font
and theme, without a web view:

- a **WebAssembly view** (`wasm`): the entry of its bundle, whose media type is `application/wasm`,
  is a WebAssembly component (the component model's binary format) that the host runs in a sandbox
  of its own. What the component is given and may ask of the host is the interface `pencere:view`
  of the kit (`kit/wit/view.wit` in this repository), which is not part of this document;
- a **Lua view** (`lua`): the entry of its bundle, whose media type is `text/x-lua`, is the text of
  a script in the Lua language (Luau). The host runs it in the same kind of sandbox, with an
  interpreter that is itself such a component; what the script is given and may ask of the host is
  the same interface, in the interpreter's terms, as the page API of the global `pencere`, and it is
  not part of this document either (the kit's sources are in this repository, in `kit/`). The script is source text: a host does not load compiled code
  from a program.

The kit is what any terminal needs to show these views, and it is a part of this repository, under
the same licence: the page engine (`pencere-view`: the elements, their layout and the draw list a
terminal draws), the sandbox that runs a component or a script and hands out pages
(`pencere-sandbox`), the interpreter for the scripts, and the SDK a page module is written with
(`pencere-page`). A terminal that draws a draw list of its own makes the kit's views its own; the
versions of the kit are those of Section 5.4.

A host that shows such views offers `wasm`, `lua` or both in the `views` of its `welcome`
(Section 5.2), when the program listed them at a version the host speaks and the host takes bundles,
because the view arrives as a bundle. A program sends such an `open` only to a host that offered the
kind and the version; a host that did not refuses one with `unsupported` (Section 6.4). A bundle whose
entry is not of the kind's type, sent with that kind, is refused with `manifest`.

```frame fixtures/to-program/welcome-wasm.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle","dismiss"],"views":[{"kind":"wasm","versions":["0.5"]},{"kind":"web","versions":["1.0"]}]}<ESC>\
```

Such a view has no `access`: it reaches nothing that the host does not hand it, so there are no
domains to ask the person about (Section 12.2), and an `access` in its `open` is not read. A network
capability, if one is added, is an extension of the kit.

```frame fixtures/to-host/open-lua.frame
<ESC>_pencere;open;id=2;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"lua","version":"0.5","program":"demopen","title":"Containers","bundle":{"entry":"view.lua","files":[{"path":"view.lua","sha256":"7d06ba2f8d1d231826e549de62e88f14c83997c82437df8b44597470da66d081","size":34,"type":"text/x-lua"}]}}<ESC>\
```

The host asks for the files as for any bundle (Sections 8.3 and 8.4) and starts the view when it
holds them. It MAY refuse an entry over a size of its own choosing with `limit` (Section 8.2), so
a program must be ready for that.

**Colours.** A page of the kit says a colour in one of two ways. The way to prefer is a **role**
of the theme, which follows the theme the terminal wears: the nine roles of its theme
(`background`, `foreground`, `dim`, `accent`, `cursor`, `success`, `error`, `info`, `warning`),
`selection`, the sixteen colours of the palette by xterm's names (`black`, `red`, `green`,
`yellow`, `blue`, `magenta`, `cyan`, `white` and the same eight prefixed `bright-`), and the
surfaces the kit derives from them (a ground raised by some steps, quieter text, a hairline, the
colour of a shadow). The other way is a **value**, for a colour that no role says: `#rrggbb` or
`#rrggbbaa` in sRGB, with, if the page wants one, a second value for a dark theme. The host decides
which of the two a pane wears (it is `dark` as in Section 11.2) and takes that one; the kit never
does. Either way a colour has an opacity of its own on top, `0` to `1`.

The kit hands the host what the page said, not what it came to. For each line of text it also
hands the **grounds** under it: the fills of the box the line is in and of the boxes that hold it,
nearest first, up to and including the first that is opaque; the page's background (`background`)
is under the last of them when it is not. A host that keeps text readable (a terminal with a
setting for the lowest contrast of text) resolves the grounds in its theme, lays them over each
other, and holds the colour of the line to its contrast with the result. That is a rule of the
host, not of the kit: it applies to a line whether its colour is a role or a value, and only to
lines of text; lines, fills, borders and the values of a chart are drawn as the page said them.

**Ending.** Section 10.2 runs as for any view: the view is not told and not waited for, it is stopped.
A view that fails (a component traps, goes over a limit the host holds it to, or hands the host
something it cannot use; a script makes an error) is closed as if the person had
closed it, except that the program is told `closed` with `reason` `error` and `code` `view`, and
with `message`: the words the view failed with, which for a script is its error and the line it is
on. They are text the view wrote, so the host cuts them and leaves out control characters and
bidirectional overrides, as it does for `program` and `title`, but it allows them more room for
being an explanation. A program that wants its person to see them prints them once the terminal is
its own again; the host does not write to the screen the program shares with the shell. A host
that has nothing to say leaves `message` out. A host MAY end a page that fails in the same way (its
web view's process ends, say) with the same `reason` and `code`.

```frame fixtures/to-program/closed-error-view.frame
<ESC>_pencere;closed;id=5,re=2;{"reason":"error","code":"view","message":"script:12: attempt to index nil with 'x'"}<ESC>\
```

In every other way the lifecycle is that of Section 10: the program's `bye` and `dismiss`, the
person closing the view and a restart (Section 10.7) end the view in the same way, and ⌃C goes to
the program (Section 11.3).

## 9. Messages and the bridge of a page

### 9.1 Roles

The **program** is the process on the pane's tty. The **view** is what the host runs or shows in
the pane. The **host** carries what the two say to each other and, for a page, a few things it says
to the host itself. A view asks and a program answers: the view starts every exchange, and the
program never speaks first.

### 9.2 The six messages

Every kind of view and its program talk with six messages. Each is a JSON object with one member,
named for what it is, whose value is an object:

| Member      | From the | Meaning                                                      |
| ----------- | -------- | ------------------------------------------------------------ |
| `call`      | view     | Asks the program to do something and to answer once.         |
| `response`  | program  | The answer to a `call`: a `result`, or an `error`.           |
| `subscribe` | view     | Asks to hear a `topic` as it changes, until it is cancelled. |
| `push`      | program  | A value of a subscription.                                   |
| `cancel`    | view     | Gives up a `call` that is still out, or ends a subscription. |
| `progress`  | program  | How far a `call` that takes a while has got.                 |

The members of each, in the order a writer writes them (Appendix A):

- `call`: `id`, `method`, and OPTIONAL `params`;
- `response`: `id`, and either `result` or `error`. A `result` is never `null`: a call with nothing
  to say answers `{}`. An `error` has a `code`, a short name a view can act on, and a `message` in
  words for the person, OPTIONAL;
- `subscribe`: `id`, `topic`, and OPTIONAL `params`;
- `push`: `id` and `value`;
- `cancel`: `id`;
- `progress`: `id`, `done`, and OPTIONAL `total` and `note`.

An `id` is a positive integer that belongs to the side that asked. The view numbers its `call`s and
`subscribe`s, from 1 and upward, and two requests of a view that are out at the same time do not
share one; the `response`, `push`, `progress` and `cancel` that go with a request carry its number. A
`response` ends its `call`; a subscription lasts until a `cancel` ends it; a `cancel` of a request
that is already over, or that the program does not know, changes nothing. A program that is sent a
`cancel` for a `call` it has not answered MAY stop the work and need not answer, and the view ignores
an answer that comes after its `cancel`.

The program and the view each read what the other says as untrusted: an object that is not one of
these, or lacks a member, has one of the wrong kind, has an `id` of 0, or is a `response` with both
a `result` and an `error` or with neither, is ignored. An unknown member is ignored too.

```frame fixtures/to-program/msg-call.frame
<ESC>_pencere;msg;id=3;{"call":{"id":1,"method":"list","params":{"all":true}}}<ESC>\
```

```frame fixtures/to-host/msg-response.frame
<ESC>_pencere;msg;id=4;{"response":{"id":1,"result":{"containers":[{"id":"c1","name":"web","state":"running"}]}}}<ESC>\
```

```frame fixtures/to-program/msg-subscribe.frame
<ESC>_pencere;msg;id=3;{"subscribe":{"id":2,"topic":"stats","params":{"name":"web"}}}<ESC>\
```

```frame fixtures/to-host/msg-push.frame
<ESC>_pencere;msg;id=4;{"push":{"id":3,"value":{"id":"c1","state":"exited"}}}<ESC>\
```

```frame fixtures/to-host/msg-progress.frame
<ESC>_pencere;msg;id=4;{"progress":{"id":4,"done":4,"total":10,"note":"layer 4 of 10"}}<ESC>\
```

```frame fixtures/to-program/msg-cancel.frame
<ESC>_pencere;msg;id=4;{"cancel":{"id":2}}<ESC>\
```

A longer one is split into chunks like any message (Section 4.4), and its non-ASCII text is written as
escapes (Section 4.5):

```frame fixtures/to-program/msg-chunked.frame
<ESC>_pencere;msg;id=4,more=1;{"call":{"id":4,"method":"stop","params":{"id":"c3","reason":"Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis <ESC>\
<ESC>_pencere;msg;id=4,seq=1,more=1;la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. A<ESC>\
<ESC>_pencere;msg;id=4,seq=2;rr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue. Arr\u00eat demand\u00e9 depuis la vue."}}}<ESC>\
```

### 9.3 Carrying them

A message of the six is one JSON object, written as Section 4.5 says, in the payload of a `msg`,
and the host does not read it: it hands the program's messages to the view and the view's to the
program, whole and in order. What the objects mean is between the program and the view. The host
does this only while the view is open: a program's `msg` before the view runs, and any `msg`
that is not an object, is dropped (Section 4.7), and a message over 1 MiB, the limit toward the
program (Section 4.6), is dropped in whichever direction it goes. **Once the view has closed the host
writes nothing the view says**, whatever ended it (the program's `bye` or `dismiss`, the person, the
view's own failure, the end of the program), because whatever reads the tty next would take it for
typed input (Section 4.8). The same holds from the moment the host writes a signal character for the
program: what the view had said and the host had not yet written is dropped, not written after
it. A program therefore does not expect a message of the view after the `closed` that ends its
view, or after its own `bye` or `dismiss`, and it must expect to lose the messages that were waiting
when the person pressed ⌃C. A frame is never cut for any other end: the one under way is written
whole, and what waits behind it is dropped.

A host that shows a page reads one thing in the objects, and only in its page's direction: the name
of the member, to tell the host's own messages (Section 9.4) from the six. It reads no value.

### 9.4 The bridge of a page

A page cannot reach the host except through the web platform's message channel, and the host's end
of that channel is the page's bridge. This section is what the kind `web` means, and its version,
**`1.0`**, is the version of the interface a page is written against (Section 5.5).

The page sends a message to the host with `postMessage` to the window that embeds it, and receives
the host's with a `message` event on its own window; the `data` of each is a JSON object with one
member. A host whose web view has another way of embedding a page gives the page the same two things.
The host takes a message only from its page, and never builds anything from it but JSON.

The members:

| Member                         | Direction                          | Section |
| ------------------------------ | ---------------------------------- | ------- |
| `call`, `subscribe`, `cancel`  | page → host, passed to the program | 9.2     |
| `response`, `push`, `progress` | program → host, passed to the page | 9.2     |
| `context`                      | host → page; page → host, to ask   | 9.5     |
| `close`                        | page → host                        | 9.6     |
| `link`                         | page → host                        | 9.7     |

`context`, `close` and `link` are the host's own messages, and their names are reserved to the host:

- A message of the page that is an object with exactly one member, named `context`, `close` or
  `link`, is the host's to handle and is not passed to the program. Any other object, with whatever
  members, is passed to the program as a `msg` (Section 9.3), where it is the program's business
  to ignore what it does not know. What is not an object is dropped.
- A message of the program whose payload is an object with exactly one member, named `context`,
  `close` or `link`, is dropped: the page must be able to tell the host's `context` from the
  program's words.

A host that handles a message of its own the page sent counts it as read; one it cannot make sense of
it counts as ignored.

```message fixtures/bridge/call.json
{"call":{"id":1,"method":"list"}}
```

```message fixtures/bridge/two-members.json
{"close":{},"call":{"id":1,"method":"list"}}
```

The first is passed to the program; the second has two members, so it is nobody's message of the host's
and is passed as well.

### 9.5 `context`

`context` is what the host tells the page about the terminal (Section 11): the object has these members,
in this order.

- `mode`: `dark` when the pane's default background is darker than its default foreground, by
  relative luminance, `light` otherwise;
- `colors`: the roles of the terminal's theme, each a colour as `#rrggbb` in sRGB: `background`,
  `foreground`, `dim`, `accent`, `border`, `success`, `error`, `warning` and `info`, then `selection`,
  and `ansi`, the sixteen colours of its palette as an array of such strings (Section 11.2);
- `font`: the terminal's monospace font as a CSS `font-family` list. Left out when the host does not
  know it;
- `size`: `width` and `height`, the content area of the pane in CSS pixels;
- `locale`: the person's language and region (BCP 47) and `timeZone`: the person's time zone (IANA).
  Each is left out when the host does not know it. They are the person's, not the program's;
- `reduceMotion`: whether the person asked the system for less motion;
- `access`: what the person allowed of what the page asked for (Section 12.2), in the shape of the
  `access` of the `open`; left out when nothing was allowed.

The host sends `context` once the page has loaded, and again whenever any member changes: the pane is
resized, the theme changes, the person changes language, time zone or the setting for motion. A page that
starts listening only after it has loaded asks for it again by sending `{"context":{}}`, and the host
answers with the current one.

```message fixtures/bridge/context.json
{"context":{"mode":"dark","colors":{"background":"#0d1117","foreground":"#c9d1d9","dim":"#8b949e","accent":"#58a6ff","border":"#30363d","success":"#3fb950","error":"#f85149","warning":"#d29922","info":"#58a6ff","selection":"#1f3d5c","ansi":["#484f58","#ff7b72","#3fb950","#d29922","#58a6ff","#bc8cff","#39c5cf","#b1bac4","#6e7681","#ffa198","#56d364","#e3b341","#79c0ff","#d2a8ff","#56d4dd","#f0f6fc"]},"font":"Menlo, monospace","size":{"width":800,"height":600},"locale":"en-US","timeZone":"Europe/Istanbul","reduceMotion":false,"access":{"connect":["https://api.example.com"]}}}
```

```message fixtures/bridge/context-request.json
{"context":{}}
```

A host that knows only the background and the foreground of its theme sends the other roles, the
selection and the palette made up (Section 11.2) and leaves out the members it does not know:

```message fixtures/bridge/context-defaults.json
{"context":{"mode":"light","colors":{"background":"#ffffff","foreground":"#1f2328","dim":"#6a6c70","accent":"#58a6ff","border":"#d2d3d4","success":"#3fb950","error":"#f85149","warning":"#d29922","info":"#58a6ff","selection":"#c7e1ff","ansi":["#000000","#cd0000","#00cd00","#cdcd00","#0000ee","#cd00cd","#00cdcd","#e5e5e5","#7f7f7f","#ff0000","#00ff00","#ffff00","#5c5cff","#ff00ff","#00ffff","#ffffff"]},"size":{"width":640,"height":480},"reduceMotion":true}}
```

### 9.6 `close`

`{"close":{}}`: the page asks to be closed. The host runs Section 10.2 with reason `view` and tells
the program with `closed` `reason` `view` (Section 10.6). It is a request, like the person's: the
program is told and decides what it does. A `close` while no view is open changes nothing.

```message fixtures/bridge/close.json
{"close":{}}
```

### 9.7 `link`

`{"link":{"url":"…"}}`: the page asks the host to open a link in the person's browser. The only way out of a page
is through here (Section 12.3): the host MUST accept only `http:` and `https:` URLs, of printable
ASCII, at most 8192 bytes, drops any other, SHOULD ask the person, and opens an accepted one in the person's
default browser on the person's machine, never on the machine the program runs on. The host does not
answer the page: whether the person agreed is not the page's to learn. A host MAY bound how many
questions about links are open at once and drop the rest.

```message fixtures/bridge/link.json
{"link":{"url":"https://example.com/docs"}}
```

```message fixtures/bridge/link-not-http.json
{"link":{"url":"file:///etc/passwd"}}
```

The first is put to the person; the second is dropped.

## 10. Lifecycle

### 10.1 States

A pane is idle, or offered (a token is outstanding), or opening (an open
accepted, files pending or the person being asked), or open (the view running). Closing a view is
instant: nothing is waited for (Section 10.2). A `hello` at any time gets its answer and a new token
without affecting a view that is opening or open; an `open` while one is opening or open is `busy`.

### 10.2 Closing

Whatever ends a view, the host:

1. tears the view down and stops serving its bundle. The view is not told and not waited for: it is
   stopped, and what it had in flight is lost with it;
2. if the program is still there and the cause was not the program's own end,
   writes `closed` into its input (Sections 10.5, 10.6 and 10.8);
3. forgets the view's token: the one that opened it (Section 6.1), and no other.

A `hello` that arrives after the view is gone is answered at once (Section 10.1), and it shows that a
program is reading the tty. The token it gets is not the view's: it is still good after step 3, and
the end of the view does not make that program ended, whatever the cause (Section 4.8).

### 10.3 The program ends

A host learns that the program ended from either of two sources and acts on
whichever comes first:

- the program's `bye` message, `reason` `exit`, `signal` or `error`, carrying the
  `token` the view was opened with; a program library SHOULD send it on a normal exit and
  from its SIGINT and SIGTERM handlers; it is best effort. A host ignores a `bye` that has
  no `token`, or another token than the one that opened the view that is opening or open,
  and a `bye` that arrives when no view is: whoever can write the stream can send
  one, only the program that read its token can send this one (Section 14), and a program
  that opened no view has no `bye` to send, the host's own observation (below) covers it;
- the host's own observation where it has one: shell integration reports the
  command finished (OSC 133 `D`), the tty's foreground process group changed, or the
  stream ended.

Either way the host closes the view as Section 10.2 says; its step 2 does not
apply, because the program is gone.

A program that is killed with SIGKILL, or crashes, sends nothing, so a host that
has no observation would leave the view open for ever; every host MUST let the
person close a view regardless. After the program has ended the host does not
write frames (Section 4.8).

```frame fixtures/to-host/bye.frame
<ESC>_pencere;bye;id=6;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","reason":"signal"}<ESC>\
```

**The answer to a `bye`.** A program that says `bye` is still reading, and the host cannot know
when the program has read the last frame the host wrote. A frame the host wrote before it learned
of the `bye` and the program did not read stays in the tty's queue (up to 1022 bytes on macOS,
Appendix B), and the program's exit leaves it to whatever reads the tty next: a shell, which
takes it for typed input. So the host answers a `bye` it believes with a `closed` whose `re` is
the `bye`'s id and whose `reason` is `program` (the shape of the answer to a `dismiss`, Section
10.8). It is the last thing the host writes for the view and the only frame it writes for a
program that has said it ended; every frame it wrote before comes ahead of it in the queue.

A program library that sent `bye` SHOULD therefore **read, and throw away, its input until that
`closed` comes**, for a time it chooses (500 ms is RECOMMENDED), then discard what is left of its
unread input (`tcflush` with `TCIFLUSH`), restore the terminal and exit. Reading up to the
`closed` empties the queue; the flush covers a host that never answers. A host's answer that comes
after the program stopped reading is the one frame this document cannot keep from a shell, which
is why the wait is long enough to make that rare and short enough for a person not to see it.

```frame fixtures/to-program/closed-program.frame
<ESC>_pencere;closed;id=5,re=6;{"reason":"program"}<ESC>\
```

### 10.4 Interrupt

⌃C goes to the program. With `ISIG` on, the tty turns the character into SIGINT
for the foreground process group. A program library that opened a view SHOULD
handle SIGINT by sending `bye` with reason `signal`, waiting for its answer and throwing away what
it has not read (Section 10.3), restoring the terminal (Section 4.9), leaving the alternate screen
(Section 6.5) and exiting. The host writes the character ahead of anything of the view's component
that is waiting, and writes none of it afterwards (Section 4.8): the interrupt does not queue behind
the messages of a component that is saying a lot.

A program that goes on after the signal keeps its view on the screen but hears nothing more from
it: the host cannot tell what the program wrote before it took the signal from what it writes
after, so it does not start writing the component's words again. A program that wants its view
to talk to it again ends it (`dismiss`, Section 10.8) and opens a new one.

### 10.5 The person closes the view

A host MAY offer the person a way to close a view that the program has not ended
(a shortcut or a control of its own). It runs Section 10.2 with reason `user` and
tells the program with `closed` `reason` `user`. A program library ends its use
of the view then, which the call that opened it observes.

```frame fixtures/to-program/closed-user.frame
<ESC>_pencere;closed;id=5;{"reason":"user"}<ESC>\
```

### 10.6 The view asks to close

A page that wants to be closed sends `close` (Section 9.6); the host then runs Section 10.2 and tells
the program with `closed` `reason` `view`. A kit view has no message of the host's own: a view that
wants the program to end sends it a `call`, and the program ends and Section 10.3 follows, or
`dismiss`es the view (Section 10.8).

### 10.7 Restart

Messages are not part of the screen. Nothing that restores a screen — a
reconnection of the stream behind the pane, the hand-over of a pane from one host
process to another — brings them back, and a host that comes up after such a
break knows of a running view only from its own record.

A host in that position MAY restart the view: it writes an unsolicited `welcome`
(no `re`, `restart` `true`, a new token, and in `views` every kind the host will open, with every version of it
the host speaks, in the host's own order, since no `hello` asks) into the pane's input. A program that has
a view open, or believes it has, treats it as a request to start again: it forgets
what it knew of its view, and sends `open` with the new token, which runs Sections 6 to 8 again with the bundle
served from the store and the view starting afresh: it asks its program again for what it shows, as at any first start. A program with no view open
ignores it. A host SHOULD restart only for a pane where its own record says a view was
open, SHOULD apply the checks of Section 6.2, and gives up if no `open` arrives
in a time of its choosing.

```frame fixtures/to-program/welcome-restart.frame
<ESC>_pencere;welcome;id=2;{"version":1,"token":"Zk3mP8rD5uYb1Xe6HqS0vg","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle","dismiss"],"views":[{"kind":"web","versions":["1.0"]}],"restart":true}<ESC>\
```

### 10.8 The program closes its view

A program that is done with its view but not with the terminal — it has a text
interface to show for a while, or it is a tool whose author reloads the view as the
files change — sends `dismiss` and goes on running. A host that supports it lists
`dismiss` in the `features` of its `welcome` (Section 5.2), and a program sends
`dismiss` only to a host that did: any other host ignores a kind it does not know
(Section 4.5) and the view would stay. `dismiss` carries the token the view was opened
with, as `bye` does (Section 10.3):

```frame fixtures/to-host/dismiss.frame
<ESC>_pencere;dismiss;id=6;{"token":"q7Vh2sB0xJm9kLw4TzN1cA"}<ESC>\
```

A host believes it under the same rule as a `bye`: only with that token, and only while
a view is opening or open. A `dismiss` that has no token, another token, or arrives when
no view is opening or open is ignored (Section 14). Otherwise the host runs
Section 10.2 with reason `program` and writes `closed` with `reason` `program` and `re` the
id of the `dismiss`. A view that is still opening is given up.

```frame fixtures/to-program/closed-program.frame
<ESC>_pencere;closed;id=5,re=6;{"reason":"program"}<ESC>\
```

The pane is idle then, with the alternate screen as the program left it. A program that
wants a view again sends `hello` and,
with the token it gets, `open` (Sections 5 and 6) as for any first view, with the same
bundle or another. The host's store (Section 8.5) holds what it verified before, so
`need` lists only what is not there. A program SHOULD read the `closed` before it
sends `hello`, so that the `closed` of the view that ended is not taken for a word about the next.

## 11. Terminal context

### 11.1 Host context

A page is told what the host knows of the person's machine, not the program's, in its `context`
(Section 9.5): the size of the pane, the theme and the font, `locale` and `timeZone`, which are the
person's, and whether the person asked for less motion. A kit view is told the same by the kit.

### 11.2 Theme

`mode` is `dark` when the pane's default background is darker than its default
foreground (by relative luminance), `light` otherwise. The host gives the view the roles of the
terminal's theme, as `#rrggbb` in sRGB:

| Role         | The terminal's                                                                                                                                                                                                                  |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `background` | default background                                                                                                                                                                                                              |
| `foreground` | default foreground                                                                                                                                                                                                              |
| `dim`        | the dimmed foreground the terminal uses; made up as a third of the way from the foreground to the background                                                                                                                    |
| `accent`     | the accent colour; made up as `info`                                                                                                                                                                                            |
| `border`     | the colour of rules and borders; made up as a fifth of the way from the background to the foreground                                                                                                                            |
| `success`    | the success colour; made up as `#3fb950`                                                                                                                                                                                        |
| `error`      | the error colour; made up as `#f85149`                                                                                                                                                                                          |
| `warning`    | the warning colour; made up as `#d29922`                                                                                                                                                                                        |
| `info`       | the information colour; made up as `#58a6ff`                                                                                                                                                                                    |
| `selection`  | the background of what the person selected; made up as a third of the way from the background to the accent                                                                                                                     |
| `ansi`       | the theme's sixteen palette colours, in the order black, red, green, yellow, blue, magenta, cyan, white, then the same eight bright (xterm's names, the order of the palette's numbers 0 to 15); made up as xterm's own palette |

A host sends all of them, whatever it knows of its theme: a role it does not have is made up as the table
says, a colour between two others rounded to the nearest value of each channel (halves away from
zero), and a palette is all sixteen or the made-up one. The made-up palette is `#000000`, `#cd0000`,
`#00cd00`, `#cdcd00`, `#0000ee`, `#cd00cd`, `#00cdcd`, `#e5e5e5`, `#7f7f7f`, `#ff0000`, `#00ff00`,
`#ffff00`, `#5c5cff`, `#ff00ff`, `#00ffff` and `#ffffff`. A page that wants the steps between them — a
surface a little lighter than the background — makes them itself. The roles are plain colours: a
terminal theme has one mode.

### 11.3 Keyboard

The keyboard divides three ways:

- **The host's own shortcuts stay with the host.** The host does not deliver them
  to the view, and it does not tell the view which they are.
- **⌃C goes to the program,** as the byte 0x03 written to the tty (Section 10.4).
- **Every other key is the view's,** ⌘F included.

## 12. Sandbox and network consent

The view runs on the person's machine, so what it can reach is the person's
business and not the program's.

### 12.1 Content security policy

The host builds the policy of a page from what the person allowed. Without any `access` in the
`open`, or when the person allows nothing, the policy is the restrictive one:

```
default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline';
img-src 'self' data:; media-src 'self' data:; connect-src 'none';
```

(one line), where `'self'` is the page's own origin (Section 8.6). What the person allows of each
list of the `access` is added to the directive it is for: `connect` to `connect-src`, `resources` to
`script-src`, `style-src`, `img-src`, `font-src` and `media-src`, `frames` to `frame-src` (which is
`'none'` otherwise) and `base` to `base-uri` (which is `'self'` otherwise). A page never gets a domain
the person did not approve, whatever it asked.

### 12.2 Network

By default a page reaches nothing outside the host's own bridge to its program. Where the `access` of
the `open` names origins (any of `connect`, `resources`, `frames`, `base`) or asks for a
browser capability in `permissions`, the host MUST ask the person before the page can use them, MUST NOT
allow what the person did not approve, and tells the page what it allowed in the `access` of its
`context` (Section 9.5). A view of the other kinds has no `access` to ask about.

The question names the origins and the capabilities, says where the program runs (this machine, or the
host name of the other one) and names the program as it called itself. A host SHOULD say when an origin is a
loopback or private address and MAY refuse such names. An origin that is not of the form Section
6.3 gives is not allowed, and the page is not told why. How long an answer is remembered is the
host's policy; it SHOULD NOT carry from one program to another.

### 12.3 Files and navigation

The view has no access to files and does not navigate to another document. The `link` message of the
page (Section 9.7) is the only way out: the host MUST accept only `http:` and `https:` URLs, SHOULD
ask the person, and opens them in the person's default browser on the person's machine, never on the
machine the program runs on.

### 12.4 Origin, limits, indication

A bundled view's origin is unique to it (Section 8.6). A host SHOULD bound the
CPU and memory of a view and
SHOULD keep an indication outside the view's area that a view is shown and
where its program runs, because a view that covers the pane could imitate the
terminal.

## 13. Fallback

- **stdin or stdout is not a tty**, and the program is not a filter (below). The program is started
  by something that is not a terminal — a script, another program — and has no pane to show a view
  in: it sends no frame of this protocol, and goes on as a program with no view.
- **A tty, and no host.** Section 5.3. The program sends no further frame and
  continues as a program with no view: it offers its own text interface, or says
  what it needs and exits. It leaves the terminal as it found it.
- **A host, and no kind of view in common.** A `welcome` whose `views` is empty
  (Section 5.2). The program falls back at once, with no wait, and sends no `open`.
- **The host refused or ended the open.** `closed` before the view ran gets the same
  treatment.

**A program in a pipeline.** A filter, a program whose stdin and stdout carry a
pipeline's data (`… | tool --select | mail`), speaks this protocol on its controlling
terminal (`/dev/tty`) and not on stdin and stdout: it puts that tty in the modes of
Section 4.9, takes the alternate screen and writes its frames there, reads the host's
replies there, and keeps stdin and stdout for the data. A program MUST decide this
explicitly, with an option or a flag of its own, and MUST NOT infer it from stdin and
stdout: a pipeline and a program started by another program look alike from inside,
both with pipes for stdin and stdout, and a program that took one for the other would
write frames into a terminal another program is using, whose host would answer into that
program's input. One with no controlling terminal, or whose terminal has no host, has no
view and goes on as the second case above says.

The host sees nothing different: the tty is the pane's, and so are its modes. Every
process of the pipeline shares that tty, so one that reads it while the view is open
(a command that asks for a password) can read the host's replies, the token among them.
Such a pipeline is the person's own to arrange.

## 14. Security considerations

**A stream anyone can write.** The program's output may contain frames the person
never intended: a file shown with `cat`, a web page's text, the output of a
program that is not the one the person started. Such a stream can send `hello`.
It cannot read the answer — the answer goes to the tty's input — so it cannot
present a token, and an `open` without one is refused. A program that can read
the pane's input (a process of the same user on the same tty) could take the token,
but it can equally read the keystrokes; this document does not defend against it.

**The answer is typed input.** Whatever reads the tty next receives the answer as
typed characters: the shell, `cat` waiting on stdin, an editor. Frames are one
printable-ASCII line (Section 4.2), so they contain no line terminator, no
signal character and no editing character, and cannot run a command or end a
program by themselves. A line editor may still interpret the characters it is
given, so text from a stale frame can appear at a prompt. The hardening in
Section 6.2 and the rule that a host writes nothing after the program ended (Section 4.8)
limit how often that happens.

**Replay and race.** A token is single use and one per pane; a replaced or spent token
is worthless. An open that fails because of a wrong token leaves the outstanding token
alone, so a hostile stream cannot use up the token of the program that is running.

**Ending a view.** `bye` needs no answer, so a stream anyone can write could send it; it
carries the token that opened the view, which that stream cannot read, and a `bye` without
it or with another is ignored (Section 10.3). `dismiss` carries the same token and is held
to the same rule (Section 10.8). A stream that cannot open a view cannot close
the view of the program that is running either. The token is the view's identity while it
lives, not a secret that outlasts it: a host forgets it when the view has closed.

**Bundles.** A file is kept only under the digest it was verified to have, and only if
it was asked for, so a stream cannot fill the store with files nobody asked for or put a file
under a digest it does not match. Paths are a closed grammar and the host serves only
what the manifest lists, so the manifest cannot name a path outside the bundle. Limits
bound what a manifest can cost.

**The view.** A view is untrusted code from a program the person may not control. A page runs in
the sandbox of Section 12, without network unless the person allows it, without files,
without navigation, in an origin of its own; it reaches the host only through the bridge of
Section 9.4, whose messages the host reads by their names and nothing else. A view can still say anything
on its own surface; the host's indication (Section 12.4) is how the person tells the terminal from a view.

**Icons.** A program's icon is drawn in the terminal's chrome by the terminal's own image
decoder. A host fetches none, so an icon cannot reveal who opened a program; it takes only
images whose bytes it has checked and bounded; and it draws none in a question it asks in its
own name (Section 7.3).

**A view the host draws itself, and the messages.** The component or the script is untrusted code from a program the
person may not control, and what it is given is the host's to decide (Section 8.8): the host runs
it in a sandbox that gives it nothing it was not handed, and bounds its memory and time
(Section 12.4). The messages between it and its program are a stream anyone can write to while the
view is open — the rule of the first paragraph applies, since a process that can write the pane's
output can send a `msg` — so a view treats what its program says as untrusted, and a program what
its view says. What the view says is never written to the program once the view is over. The words
a failed view's `closed` carries (`message`) are the view's own and are printed by the program, not
by the host: a program that prints them treats them as it treats any text from another party.

**Resource use.** Frame, message and bundle limits (Sections 4.6, 8.2) bound what a
receiver buffers; hosts SHOULD also limit how often one pane may ask (answers to `hello`,
`open`).

## 15. Conformance

A **host** conforms if it does everything this document asks of a host with MUST and
MUST NOT, in Sections 4 to 12; a **program library** conforms if it does everything it asks
of a program. SHOULD is for what an implementation has a reason to skip.

There are two levels. **Core** is Sections 4 to 9, 10 without 10.7 and 10.8, 11, 12 and 13. **Full**
adds restart (Section 10.7) and the program closing its view (Section 10.8). A host below Full does not
list `dismiss` in its `features`; a program below Full does not send a `dismiss` and ignores an
unsolicited `welcome`. Each kind of view is optional: a host offers in its `views` the kinds it shows,
the pages (`web`) among them or not, the views it draws itself (Section 8.8) among them or not, and a
program sends an `open` only of a kind, and at a version, that the host offered.

The fixtures of this repository are the conformance suite:

- every `decode` entry of `fixtures/manifest.json` is fed, as a stream in the entry's
  direction and split at every byte boundary, to a receiver, which delivers exactly the
  entry's messages and reports exactly its errors. A receiver that reads raw bytes (a program
  library) takes every entry. A receiver that is handed APC bodies by a terminal parser (a host)
  takes the entries whose scope is `body`, fed with the body of each frame in the file; the
  entries of scope `stream` exercise the framing such a parser has already done;
- every entry marked `canonical` is what a writer produces when asked to send the entry's
  messages;
- every `bridge` entry is fed to a host that shows pages, and it does what the entry's `outcome` says;
  a host that writes a `context` writes the entry of `host-to-page` byte for byte when it is
  configured as the entry's `config` says;
- every `session` entry is a conversation with a host configured as the entry's `config` says
  (its name and version, the tokens it issues in order, the digests its store already holds):
  the bytes the host emits after each `in` step are the next `out` step's file, or nothing when
  the step is `null`.

Two implementations that pass them agree on the wire.

## Appendix A. Payload types

Members are written in the order shown (Section 4.5), except that `supported` of a
`Welcome`, when it is present, is written before `host` (§5.2 and the fixtures show it
so). Unknown members are ignored.

```ts
// An entry of `views` in a Hello and in a Welcome (Sections 5.1, 5.2).
interface ViewOffer {
  kind: string; // a name of Section 5.5
  versions: string[]; // "major.minor" (Section 5.5), in the order of whoever wrote the list
}

interface Hello {
  versions: number[]; // protocol versions: positive integers
  views: ViewOffer[]; // the kinds the program can show, in its order of preference; not empty
}

interface Welcome {
  version?: number; // present with token, absent with supported
  token?: string;
  host: { name: string; version: string };
  features?: string[]; // "bundle" (Section 8.1), "dismiss" (Section 10.8)
  views?: ViewOffer[]; // with token: the program's list cut down to what the host opens, in the program's order; may be empty (Section 5.2)
  restart?: boolean; // true on an unsolicited welcome
  supported?: number[]; // present when no protocol version is shared
}

interface Open {
  token: string;
  view: string; // one of the welcome's kinds (Section 6.3)
  version: string; // one of the welcome's versions of that kind
  program?: string; // what the program calls itself (Sections 6.3, 7.3)
  title?: string; // what the program calls the view
  icons?: Icon[]; // (Section 7.3)
  access?: Access; // "web" only (Section 12.2)
  bundle: Bundle; // the entry has the media type of the kind (Section 6.3)
}

interface Icon {
  src: string; // a data: URI with base64 data
  mimeType?: string;
  sizes?: string[]; // "48x48"
  theme?: "light" | "dark";
}

interface Access {
  connect?: string[]; // origins: scheme://host[:port] with http, https, ws or wss
  resources?: string[];
  frames?: string[];
  base?: string[];
  permissions?: ("camera" | "microphone" | "geolocation" | "clipboardWrite")[];
}

interface Bundle {
  entry: string;
  files: { path: string; sha256: string; size: number; type: string }[];
}

interface Need {
  missing: string[]; // sha256 digests
}

interface Closed {
  reason: "refused" | "error" | "user" | "view" | "program";
  code?: string;
  message?: string; // for people; never parsed. A failed view says why (Section 8.8)
}

interface Bye {
  token: string; // the token the view was opened with (Section 10.3)
  reason?: "exit" | "signal" | "error";
}

interface Dismiss {
  token: string; // the token the view was opened with (Section 10.8)
}
```

The payload of a `msg` is one of the six messages (Section 9.2):

```ts
type ViewMessage =
  | { call: { id: number; method: string; params?: unknown } }
  | {
      response:
        { id: number; result: unknown } | { id: number; error: { code: string; message?: string } };
    }
  | { subscribe: { id: number; topic: string; params?: unknown } }
  | { push: { id: number; value: unknown } }
  | { cancel: { id: number } }
  | { progress: { id: number; done: number; total?: number; note?: string } };
```

The messages of the bridge of a page that are the host's own (Sections 9.4 to 9.7), which are not
carried in a frame:

```ts
interface Context {
  mode: "dark" | "light";
  colors: {
    background: string; // "#rrggbb"
    foreground: string;
    dim: string;
    accent: string;
    border: string;
    success: string;
    error: string;
    warning: string;
    info: string;
  };
  font?: string; // a CSS font-family list
  size: { width: number; height: number }; // CSS pixels
  locale?: string; // BCP 47
  timeZone?: string; // IANA
  reduceMotion: boolean;
  access?: Access; // what the person allowed
}

type HostToPage = { context: Context };
type PageToHost =
  | { context: {} } // asks for the context again
  | { close: {} }
  | { link: { url: string } };
```

Codes of `Closed`: for `refused`, those of Section 6.4; for `error`, `bundle` (Section 8.4), `view`
(Section 8.8), `timeout` and `internal`. A receiver treats a code it does not know as the plain reason.

## Appendix B. The tty's input queue

The host's frames reach the program through the input queue of the pane's tty. What that
queue does with them decides Sections 4.6, 4.8 and 4.9. These are the facts, with where each
comes from. Sources were read as fetched on 2026-10-21: the apple-oss-distributions/xnu
repository at `main` and the torvalds/linux repository at `master`; the machine measured is
macOS 26.4.1 on a kernel built from xnu-12377.101.15.

**1. macOS, non-canonical mode.** The pty master's write stops at `TTYHOG - 2`
bytes outstanding (`bsd/kern/tty_dev.c`, `ptcwrite`; `TTYHOG` is 1024 in `bsd/sys/tty.h`): the
call blocks, or with `O_NONBLOCK` takes fewer bytes or fails with `EWOULDBLOCK`. That is flow
control, not loss. Measured: with the slave in raw mode and unread, the master accepts 1022
bytes, whether written one byte at a time or in 4096-byte writes, and a single write of 1023 to
8192 bytes is accepted for 1022. From it: a host writes without blocking and keeps the rest
(Section 4.8).

**2. macOS, canonical mode.** `ttyinput` discards a character when `MAX_INPUT` bytes
are queued (`bsd/kern/tty.c`; `MAX_INPUT` is 1024 in `bsd/sys/syslimits.h`) and the line is
delivered to a reader only when a terminator arrives. Measured, slave canonical and unread, then
a line terminator written: lines of 255, 511 and 1023 bytes were delivered with their
terminator (256, 512 and 1024 bytes); lines of 2047, 4095 and 8191 bytes delivered nothing, though
the master accepted every byte. A reply that arrives while the slave is canonical — a program
that did not set the modes, or one that exited with a shell reading — vanishes above the limit and
is never delivered without a terminator, which a frame does not have. From it: the modes of
Section 4.9, and a small frame limit in this direction.

**3. Linux.** The line discipline's read buffer is `N_TTY_BUF_SIZE` = 4096 and the room left for
input is that minus the bytes outstanding minus one (`drivers/tty/n_tty.c`, `n_tty_receive_buf_common`);
what does not fit waits in the pty's flip buffers, limited to `TTYB_DEFAULT_MEM_LIMIT` = 640 KiB
(`drivers/tty/tty_buffer.c`; `pty_write_room` in `drivers/tty/pty.c`). In canonical mode with no line
pending, characters are let through without limit "so that erase characters will be handled" and the
excess is lost (`n_tty.c`). Not measured here; this is from the source.

**4. Signal characters flush.** With `ISIG` on, a signal character (ETX, FS, SUB) flushes the tty's
queues unless `NOFLSH` is set (`n_tty.c`, `isig`; `bsd/kern/tty.c`, `ttyinput`). Frames in
flight when the person presses ⌃C are lost, which Section 4.7 expects.

**The POSIX floor.** `_POSIX_MAX_INPUT` and `_POSIX_MAX_CANON` are 255, the least a system must
provide, and Linux still advertises `MAX_INPUT` as 255 (`include/uapi/linux/limits.h`) although the
queue it operates is 4095. This document does not bind to 255: on the systems examined the queue is
four times that or more, and a frame limit of 255 would leave about 230 payload bytes for no gain on
any system measured. A sender may send smaller frames (Section 4.6), so an implementation for a
system with a smaller queue lowers its own frame size without leaving the protocol; its receivers
still accept up to the limit.

**Derivation of 511.** The binding limit is the smallest operating one: macOS discards at 1024
queued bytes in canonical mode and accepts 1022 in non-canonical mode. Two frames of 511 bytes are
1022 bytes, exactly what the master accepts in non-canonical mode, and with a line terminator they
are 1023, below the 1024 at which canonical mode discards. So in the smallest queue examined two
whole frames are never refused or discarded for size, and a single frame can never be larger than
the queue.

## Appendix C. An example exchange

A program starts in a terminal that supports this document. It shows a web page. The host's store holds
`index.html` and `style.css` of the program's bundle already, and lacks `app.js`. Frames are as in the
fixtures; ids number each side's messages, and the exchange is shortened, so the numbers of the
messages left out are skipped.

1. The program puts the tty in non-canonical mode, writes `hello` and the fence:

```frame fixtures/to-host/hello-fence.frame
<ESC>_pencere;hello;id=1;{"versions":[1],"views":[{"kind":"web","versions":["1.0"]}]}<ESC>\
<ESC>[c
```

2. The host answers before it answers `CSI c`:

```frame fixtures/to-program/welcome.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle","dismiss"],"views":[{"kind":"web","versions":["1.0"]}]}<ESC>\
```

3. The program switches to the alternate screen and opens its view with the token, the kind and its
   version, its names, what the page asks to reach, and the manifest:

```frame fixtures/to-host/open-web.frame
<ESC>_pencere;open;id=2;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"web","version":"1.0","program":"containers","title":"Containers","access":{"connect":["https://api.example.com"],"permissions":["clipboardWrite"]},"bundle":{"entry":"index.html","files":[{"path":"index.html","sha256":"aeba6c19a632d2153a9e336a88b385d438c474a996d5be1408f6cd2ca6898e54","size":132,"type":"text/html"},{"path":"app.js","sha256":"bcc81d22a717681517bc99bbad72438377e02c19583fae67522475fd8b3ca62c","size":67,"type":"text/javascript"},{"path":"style.css","sha256":"72735d03f2b7e5a9ddb8ebe1c5fde2da88aa74292ba7ac608490a94f0ebbac0e","size":61,"type":"text/css"}]}}<ESC>\
```

4. The host names what it lacks:

```frame fixtures/to-program/need.frame
<ESC>_pencere;need;id=2,re=2;{"missing":["bcc81d22a717681517bc99bbad72438377e02c19583fae67522475fd8b3ca62c"]}<ESC>\
```

5. The program sends `app.js`:

```frame fixtures/to-host/file-chunked.frame
<ESC>_pencere;file;id=3,more=1,d=bcc81d22a717681517bc99bbad72438377e02c19583fae67522475fd8b3ca62c;ZG9jdW1lbnQuZ2V0RWxlbWVudEJ5SWQoImFwcCIpLnRleHRDb250ZW50ID0g<ESC>\
<ESC>_pencere;file;id=3,seq=1;ImhlbGxvIGZyb20gcGVuY2VyZSI7Cg==<ESC>\
```

6. With the file here, the host asks the person about the origin and the capability the page asked for.
   It starts the page, and when the page has loaded sends it the terminal's context, with the
   origin the person allowed:

```message fixtures/bridge/context.json
{"context":{"mode":"dark","colors":{"background":"#0d1117","foreground":"#c9d1d9","dim":"#8b949e","accent":"#58a6ff","border":"#30363d","success":"#3fb950","error":"#f85149","warning":"#d29922","info":"#58a6ff","selection":"#1f3d5c","ansi":["#484f58","#ff7b72","#3fb950","#d29922","#58a6ff","#bc8cff","#39c5cf","#b1bac4","#6e7681","#ffa198","#56d364","#e3b341","#79c0ff","#d2a8ff","#56d4dd","#f0f6fc"]},"font":"Menlo, monospace","size":{"width":800,"height":600},"locale":"en-US","timeZone":"Europe/Istanbul","reduceMotion":false,"access":{"connect":["https://api.example.com"]}}}
```

7. The page calls the program; the host passes the call on as a `msg`, unread:

```frame fixtures/to-program/msg-call.frame
<ESC>_pencere;msg;id=3;{"call":{"id":1,"method":"list","params":{"all":true}}}<ESC>\
```

8. The program answers; the host passes the answer to the page:

```frame fixtures/to-host/msg-response.frame
<ESC>_pencere;msg;id=4;{"response":{"id":1,"result":{"containers":[{"id":"c1","name":"web","state":"running"}]}}}<ESC>\
```

9. When the person presses ⌃C the program writes `bye`, waits for the host's `closed`, restores the
   terminal and leaves the alternate screen; the host tears the page down:

```frame fixtures/to-host/bye.frame
<ESC>_pencere;bye;id=6;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","reason":"signal"}<ESC>\
```

```frame fixtures/to-program/closed-program.frame
<ESC>_pencere;closed;id=5,re=6;{"reason":"program"}<ESC>\
```

## Appendix D. References

Normative:

- BCP 14: RFC 2119 and RFC 8174.
- RFC 4648, base64 and base64url. RFC 5234, ABNF. RFC 8259, JSON. RFC 2397, the `data:` URL
  scheme.

Informative:

- ECMA-48, Control Functions for Coded Character Sets: APC, ST, CSI.
- The kitty graphics protocol, whose APC framing, `m=` chunking and 4096-byte payload chunks this
  protocol follows in spirit: <https://sw.kovidgoyal.net/kitty/graphics-protocol/>
- The 2 second bound of Section 5.3 is the one the terminal mode of gpuix uses for its
  kitty-graphics probe and `CSI c` fence (`packages/gpui-terminal/src/kitty.rs`,
  `PROBE_TIMEOUT` and `FENCE_TIMEOUT`).
- Kernel sources of Appendix B: `bsd/kern/tty_dev.c`, `bsd/kern/tty.c`, `bsd/sys/tty.h` and
  `bsd/sys/syslimits.h` in <https://github.com/apple-oss-distributions/xnu>; `drivers/tty/n_tty.c`,
  `drivers/tty/pty.c`, `drivers/tty/tty_buffer.c` and `include/uapi/linux/limits.h` in
  <https://github.com/torvalds/linux>.
