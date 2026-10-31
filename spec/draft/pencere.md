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
- how the files of a view are loaded once and kept by content, and the files beside them: what a
  program attaches to its view, what the person gives the view and where the person saves what
  the program gives (Section 8);
- the six messages a program and its view exchange, and the bridge between a web page and its host
  (Section 9);
- when a view starts and ends (Section 10);
- what the terminal tells a view about itself (Section 11), and what a web page may reach (Section 12);
- what happens when there is no host (Section 13).

Non-goals: letting a program choose where its view appears (a view always takes the pane of the
program that asked; a person who wants it elsewhere splits the pane first and starts the program
there); reaching a host through a terminal multiplexer that does not forward the stream untouched
(such a program sees no host and falls back, Section 5.3); the interface of the kit that a `wasm`
or `lua` view is written against, which has a document of its own (`kit/SPEC.md`); the internals of
any platform's web view.

## 2. Conventions and terms

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
"SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY" and "OPTIONAL" are to be
interpreted as described in BCP 14 (RFC 2119, RFC 8174) when, and only when,
they appear in all capitals. Grammar is ABNF (RFC 5234); JSON is RFC 8259.

| Term         | Meaning                                                                                                                             |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------- |
| Host         | A terminal emulator that implements this document: it owns panes, the views and their sandboxes, and the bundle store.              |
| Program      | A process running on a pane's tty that wants to show a view.                                                                        |
| Pane         | A terminal pane: one tty and the screen it draws.                                                                                   |
| View         | What a program shows in its pane: a component, a script or a web page (Section 3).                                                  |
| Kind         | The sort of a view: a name from a closed table, with a version of its interface (Section 5.5).                                      |
| Kit          | What a `wasm` or `lua` view is written against: the component interface `pencere:view` and the Lua page API, the global `pencere`.  |
| Page         | A view of the kind `web`.                                                                                                           |
| Person       | The human looking at the pane. The view runs on the person's machine; the program may run on another.                               |
| `to-host`    | The direction program → host: bytes the program writes to its tty.                                                                  |
| `to-program` | The direction host → program: bytes the host writes to the input side of the pane's tty.                                            |
| Frame        | One APC string in this protocol (Section 4.1).                                                                                      |
| Message      | A kind, control parameters and a payload, carried by one or more frames (Section 4.4).                                              |
| Chunk        | One frame of a message.                                                                                                             |
| View message | One of the six JSON objects a program and its view exchange (Section 9.2), carried in the payload of a `msg`.                       |
| Token        | A single-use secret the host gives a program so that it can open a view (Section 6.1).                                              |
| Bundle       | The files of a view, listed with their SHA-256 digests (Section 8).                                                                 |
| Handle       | A secret the host makes when the person gives a view a file or a place to save in; the program reads or writes by it (Section 8.9). |

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
document of its own, `kit/SPEC.md` in this repository, and sources that are normative with it
(Section 9.8); this one names them and the version of the kit that a kind carries (Section 5.5).

A **web view** — kind `web` — is a web page that the terminal shows in a web view of its own, in a
sandbox with no network, no files and no navigation of its own unless the person allows some
(Section 12). It reaches the terminal only through the bridge of Section 9.4.

The two families are opened the same way (Section 6), arrive as a bundle (Section 8), are sized and
named the same way (Section 7), start and end the same way (Section 10), and talk to their program in
the same six messages (Section 9.2), which the terminal carries between the two and does not read.
What differs is who draws the view and, for a page, the few messages it exchanges with the host
itself (Section 9.4).

A host MAY open one family and not the other: its `welcome` says which (Section 5.2).

**In brief.** Nine kinds of frame (Section 4.5) carry a conversation, and six messages (Section 9.2)
are what a program and its view say to each other while the view runs:

1. The program asks whether there is a host: `hello` and `CSI c` (Section 5.1). The host answers
   with `welcome`: a token and the kinds of view it opens (Section 5.2). Nothing at all, or the answer to
   `CSI c`, means no host (Section 5.3).
2. The program asks for a view: `open`, with the token, the kind and the version, and the manifest of
   the bundle (Section 6). The host checks it and refuses with `closed` (Section 6.4), or answers with
   `need`, the digests of the files it lacks (Section 8.3).
3. The program sends the files, one `file` each (Section 8.4). When the bundle is whole, and the person
   has been asked if the page asked to reach more than its own bundle (Section 12.2), the host starts
   the view.
4. While the view runs, the view starts every exchange (`call`, `subscribe`, `cancel`) and the
   program answers (`response`, `push`, `progress`), in `msg` frames the host carries and does not
   read (Section 9). A page also has a few messages of the host's own (Section 9.4).
5. The view ends: the program says `bye` or `dismiss` (Sections 10.3 and 10.8), the person closes
   it (Section 10.5), the view asks to close (Section 10.6) or fails (Section 9.8), or the host
   restarts it (Section 10.7). The host tells the program with `closed`, unless the host is restarting or
   the program is the one that ended; a program that said `bye` is still answered with one
   (Sections 10.2 and 10.3).

## 4. Transport

Messages travel in the terminal's own stream: from the program in its output, to
the program in its input. There is no port and no second channel. The one thing a host may
carry another way is the bytes of a file, through a folder of its own on the program's machine
(Section 8.11), and the program learns of that only from the messages.

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
| `h`    | 32 lowercase hex digits                                | A handle (`write` messages only, Section 8.10).                    |
| `o`    | decimal, 0 to 2^53 - 1, no leading zero (except `0`)   | Where in the file the bytes go (`write` messages only).            |
| `end`  | `0` or `1`                                             | `1`: these are the file's last bytes (`write` messages only).      |
| `e`    | 1 to 32 of `a-z` and `-`                               | Why a `data` carries no bytes (`data` messages only, Section 8.9). |

A key may appear once; twice is malformed. A receiver MUST ignore keys it does
not know, and MUST NOT treat a value outside the table's range as anything but
malformed. A sender MUST write the keys in the order `id`, `seq`, `more`,
`re`, `d`, `h`, `o`, `end`, `e`, MUST omit `seq`, `more` and `end` when they are 0, and MUST NOT
write any other key in this version, nor a key on a kind the table does not give it to. Receivers accept
any order and the explicit defaults. (The order exists so that two writers
produce the same bytes; the fixtures depend on it.)

### 4.4 Messages and chunks

A message is a kind, the parameters of its first chunk and a payload. A payload
too long for one frame is split into chunks, each a frame.

1. A sender sends the chunks of a message contiguously and in order, with no
   other frame of this protocol between them, numbering them from `seq` 0.
2. The parameters that belong to a message (`re`, `d`, `h`, `o`, `end`, `e`) appear on its
   first chunk only. Later chunks carry `id`, `seq` and `more`; a receiver ignores
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
7. `file`, `write` and `data` payloads are base64 split on four-character groups: every
   chunk but the last has a payload length that is a multiple of 4, so a receiver may
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

| Kind      | Direction    | Payload                                         | First-chunk parameters                            | Section  |
| --------- | ------------ | ----------------------------------------------- | ------------------------------------------------- | -------- |
| `hello`   | `to-host`    | JSON `Hello`                                    | none                                              | 5        |
| `welcome` | `to-program` | JSON `Welcome`                                  | `re`: the hello answered; none when unsolicited   | 5, 10.7  |
| `open`    | `to-host`    | JSON `Open`                                     | none                                              | 6        |
| `need`    | `to-program` | JSON `Need`                                     | `re`: the open or the attach answered             | 8.3, 8.8 |
| `file`    | `to-host`    | base64 (RFC 4648 § 4, with padding) of one file | `d`: its digest                                   | 8.4, 8.8 |
| `attach`  | `to-host`    | JSON `Attach`                                   | none (answered by a `need`)                       | 8.8      |
| `detach`  | `to-host`    | JSON `Detach`                                   | none                                              | 8.8      |
| `read`    | `to-host`    | JSON `Read`                                     | none (answered by a `data`)                       | 8.9      |
| `data`    | `to-program` | base64 of bytes of a file the person gave       | `re`: the read answered; `e` when there are none  | 8.9      |
| `write`   | `to-host`    | base64 of bytes for a place to save in          | `h`: the handle; `o`: the offset; `end`           | 8.10     |
| `wrote`   | `to-program` | JSON `Wrote`                                    | `re`: the write answered                          | 8.10     |
| `stage`   | `to-host`    | JSON `Stage`                                    | none (answered by two `staged`)                   | 8.11     |
| `staged`  | `to-program` | JSON `Staged`                                   | `re`: the stage answered                          | 8.11     |
| `fetch`   | `to-host`    | JSON `Fetch`                                    | none (answered by two `fetched`)                  | 8.11     |
| `fetched` | `to-program` | JSON `Fetched`                                  | `re`: the fetch answered                          | 8.11     |
| `unstage` | `to-host`    | JSON `Unstage`                                  | none (answered by a `staged` or a `fetched`)      | 8.11     |
| `msg`     | both         | one JSON object: a view message (9.2)           | none                                              | 9        |
| `closed`  | `to-program` | JSON `Closed`                                   | `re`: the open, dismiss or bye it answers, if any | 6.4, 10  |
| `bye`     | `to-host`    | JSON `Bye`                                      | none (answered by a `closed`, 10.3)               | 10.3     |
| `dismiss` | `to-host`    | JSON `Dismiss`                                  | none                                              | 10.8     |

The JSON types are in Appendix A. A sender MUST NOT send a kind in a direction
the table does not list. A receiver MUST ignore a message of a kind it does not
know; decoders deliver every well-formed message whatever its kind and leave this
to the receiver. A message of a known kind that arrives in the other direction,
or whose payload is not valid JSON where the kind has JSON, is ignored too.

**JSON text.** A JSON payload is one line of compact JSON (no whitespace outside
strings) in which every character the rule of Section 4.2 forbids is escaped. The one exception is
a message of a view that the host passes on to the program as it came: it keeps the spaces
between its tokens, and loses only the tab, line feed and carriage return (Section 9.3).
A sender writes: `"` as `\"` and `\` as `\\`; U+0008, U+0009, U+000A, U+000C and
U+000D as `\b`, `\t`, `\n`, `\f` and `\r`; every other code point below U+0020,
U+007F and every code point from U+0080 up as `\uXXXX` with lowercase hex
digits, a surrogate pair from U+10000 up; everything else as itself. For the
types of Appendix A the members are written in the order given there. A JSON
encoder that only escapes non-ASCII text still writes DEL raw: escape it
yourself. Receivers parse any valid JSON, with one limit: a text whose arrays and objects
nest **128 levels deep or more**, the outermost counting as the first, is not accepted, and a
sender MUST NOT write one. A receiver drops it as it drops a text that is not JSON, so that no
two receivers need build a tree to agree on what a message is (Section 9.3).

```frame fixtures/to-program/welcome.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["dismiss"],"views":[{"kind":"web","versions":["1.0"]}]}<ESC>\
```

### 4.6 Limits

| Limit                                 | `to-host`           | `to-program`      |
| ------------------------------------- | ------------------- | ----------------- |
| A frame, `ESC _` and `ESC \` included | 4096 bytes          | 511 bytes         |
| A message's payload                   | 16 MiB (16 777 216) | 6 MiB (6 291 456) |

A receiver MUST drop a frame or a message over its direction's limit (errors
`too-long` and `message-too-long`). A sender MAY always send smaller frames.

The two directions differ because the second one passes through the tty's input
queue, which is small and which a program that is not reading does not drain.
511 is derived, not chosen, from the operating limits of that queue on the two
systems examined; the derivation is Appendix B. 4096 is the size kitty's graphics
protocol allows for the payload of a chunk; here it bounds the whole frame. The
message limits are protocol constants, not measurements: they bound what a
receiver has to buffer.

The limit toward the program is not 1 MiB because a message of a view has that size
(Section 9.3) and no more, but six times it: the host writes a view's message in the form of
Section 4.5, where a character that a frame cannot carry becomes an escape of six bytes (a DEL
is `\u007f`), and a message that is within the view's limit must always be writable. The escapes
are the transport's, and they are split into frames like the rest of the message.

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
  A host that writes one (⌃C, ⌃\ or ⌃Z) for the program of a view MUST therefore put it **first**:
  it drops everything of the view's component it has not written yet, including the rest of a
  frame it has begun, and writes nothing of the component's after it. It does not wait for the
  frame that was under way, and it does not write the character behind frames that are waiting.
  The three characters are the program's whatever the view is (Section 11.3).
- Once the host knows the program has ended (Section 10.3) it MUST NOT begin a frame, because
  whatever reads the tty next would receive it as typed input, until a `hello` shows that a program
  is reading again. The one exception is the single `closed` that answers a `bye` it believes
  (Section 10.3). A frame already begun is finished, as the first item asks and as Section 9.3
  says: the rest of a frame left in the input would reach whatever reads next as a frame that
  never ends. A view that has closed is not the end of the program: the host still writes the
  `closed` that tells the program (Sections 10.5, 10.6 and 10.8) and answers a later `hello`.
- A host MUST NOT hold more than 16 MiB (16 777 216 bytes) of what a view says waiting to be
  written to the program (the messages of the view that it passes on, Section 9.3; its own frames
  are few and small and are not counted). A program that does not read leaves everything the tty
  does not take in the host, and a view can say a good deal. The host asks before it holds a
  message: if that message would take the words of the view that wait past the bound, it does not
  hold it, it drops the words of the view that it has not begun to write, as it does for a signal
  character, and it closes the view with `closed`, `reason` `error`, `code` `view` (Section 9.8). (A host that
  draws the view itself holds a well-behaved view back well before the limit: the kit's
  `program.send` answers `full` and `page.room` tells the view when it may send again, so that
  such a view slows down and is not closed.) The `data` that answers a `read` (Section 8.9) is
  what the view brings the program and is held the same way: it counts toward the bound, and a
  signal character drops it as it drops the view's words; it is the one message of the host's own
  that is. A **program author** should know that a host
  MAY stop reading the pane's output while its view is behind: a program that writes a great deal
  of output while its view says a great deal may find its own writes waiting until the program
  reads what the view said.

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
versions of that kind's interface that the program speaks, in the order it prefers them. The two
`versions` are different things with one name: the first is a list of integers and names the
version of this document (the wire), the second is a list of strings and names the version of a
kind's interface (Section 5.5); the names are not to be changed, and a reader keeps them apart by
where they stand.

A host ignores a `hello` that lacks either member, whose `versions` is not a list of integers,
whose `views` is empty, or whose `views` is not a list of objects with a string `kind` and a
`versions` that is a list of strings, as it ignores a message it cannot read. A `versions` that is
an empty list is a list: the program speaks no version that the host does, and the host answers
with `supported` (Section 5.2). A kind that is listed twice counts once, at its first place.

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
token (Section 6.1) and answers with `version`, `token`, `host`, `features`, `views` and, when
there is something to say in it, `opens`, and `re` set to the hello's id. If there is none (the
`versions` of the `hello` is empty, or names none the host speaks), it answers with `supported`
and `host` only.

`features` lists the optional parts of this document the host supports: `dismiss`
(Section 10.8) and `stage` (Section 8.11). A program uses only what is listed and ignores a
name it does not know. (Every view arrives as a bundle, Section 8, so that is no feature.) A host
that lists `stage` says where its folder on the program's machine is in `stage`, a member next to
`features` with the `dir` of the folder and the `token` its `marker` file holds; a `stage` without
the feature, or the feature without a `stage` that is well formed, is no offer.

```frame fixtures/to-program/welcome-stage.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["dismiss","stage"],"stage":{"dir":"/home/alice/.pencere/stage/6d0f9f1c","token":"3f9c2a7be1d84056a9c4e7b1d2f80a65"},"views":[{"kind":"lua","versions":["2.1"]}]}<ESC>\
```

`views` lists what the host will open: the kinds of the `hello` and, for each, the versions of its
interface that the program and the host both speak. For each entry of the `hello`'s `views`, in
order, the answer has an entry with the same `kind` and `versions` cut down to the versions the host
also offers, in the order the program listed them; an entry whose kind the host will not open, or
that shares no version with the host, is left out. The host only eliminates: it never reorders, and
never adds a kind or a version the program did not list. A name that is not in the table of Section
5.5 is a kind the host will not open. `views` is present in every `welcome` that carries a token,
and it is empty when the program and the host share no kind at a version: the host answers all the
same, with a protocol version and a token, so that the program knows at once that there is a host and
nothing it can show, and does not wait.

`opens` says why the host left a kind out when it was only for want of a version in common. For each
entry of the `hello`'s `views`, in order, whose kind the host opens but at none of the versions the
program listed, it has an entry with that `kind` and **the versions the host does open of it**, in the
host's own order. It is left out when there is no such kind (the examples below have none), and it
never names a kind the host will not open. It is for people and for the program's diagnostics: it
changes nothing about which kinds the program may open (`views` alone does that), and a program that
finds nothing to open in `views` but a kind in `opens` SHOULD say so in words that name both sides
(Section 5.3). A restart `welcome` (Section 10.7) lists every version the host speaks in `views`, so it
has no `opens`. The answer of a host that opens web pages at `1.0` and the kit's interface at `1.0`
to a program that listed `wasm` at `2.0` and `web` at `1.0`:

```frame fixtures/to-program/welcome-opens.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["dismiss"],"views":[{"kind":"web","versions":["1.0"]}],"opens":[{"kind":"wasm","versions":["1.0"]}]}<ESC>\
```

```frame fixtures/session/handshake/02-out.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["dismiss"],"views":[{"kind":"web","versions":["1.0"]}]}<ESC>\
```

```frame fixtures/to-program/welcome-versions.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["dismiss"],"views":[{"kind":"wasm","versions":["0.5","0.4"]},{"kind":"web","versions":["1.0"]}]}<ESC>\
```

The host in this example opens `wasm` at `0.4` and `0.5` and `web` at `1.0`; the program's `hello`
(`session/several-versions`) listed `wasm` at `0.6`, `0.5` and `0.4`, `lua` and `web`. The version
`0.6` is left out because the host does not speak it, `lua` because the host does not open it, and the two
versions of `wasm` that remain are in the program's order, not the host's.

```frame fixtures/to-program/welcome-unsupported.frame
<ESC>_pencere;welcome;id=1,re=1;{"supported":[1],"host":{"name":"pencere-test","version":"0.0.0"}}<ESC>\
```

```frame fixtures/to-program/welcome-no-views.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["dismiss"],"views":[]}<ESC>\
```

### 5.3 What the program concludes

| What the program reads first                                              | Conclusion                                                                                                                                                                                                                                                                                                                                                                                                |
| ------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A `welcome` with `version` and `token`, `re` equal to its last hello's id | There is a host. If `views` has a kind, go on with Section 6 and open the first it can show, at the first version listed for it. If `views` is empty, there is a host and nothing to open: fall back at once, as with no host (Section 13); do not wait and do not send `open`. If `views` is empty and `opens` names a kind the program listed, say in words which versions would have served, as below. |
| The terminal's answer to `CSI c` (`ESC [ ? … c`)                          | There is no host. A `welcome` that arrives later MUST be ignored.                                                                                                                                                                                                                                                                                                                                         |
| A `welcome` with `supported` and no `version`                             | There is a host that shares no version with this program. Treat as no host.                                                                                                                                                                                                                                                                                                                               |
| Nothing                                                                   | After a time, no host. The time SHOULD be 2 seconds; a program MAY wait longer on a slow link.                                                                                                                                                                                                                                                                                                            |

2 seconds is a precedent, not a measurement (Appendix D). A program that
concludes "no host" MUST NOT send further frames of this protocol (Section 13).
One that concluded by timeout SHOULD discard a late answer to `CSI c`
(`ESC [ ? … c`) for as long as it still reads the tty, so that it does not reach
a shell as typed text.

A program that falls back because `views` is empty, and finds in `opens` a kind it listed, SHOULD
say what would have served before its fallback runs, in a sentence that names the kind and the
versions of both sides, for instance `this program is written for wasm 2.0; this terminal opens wasm
1.0`; the libraries of this repository print it on the program's standard error as
`pencere: <sentence>`. A program that finds nothing in `opens` says nothing of the kind: the host
does not open what it lists, or there is no host.

A `welcome` is only believed when the program has sent a `hello` and
the `welcome` answers it; an unsolicited `welcome` is handled by Section 10.7 or ignored.

```frame fixtures/to-program/noise.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["dismiss"],"views":[{"kind":"web","versions":["1.0"]}]}<ESC>\
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

The versions of the kit views (`wasm` and `lua`, Section 5.5) follow one rule, and each of the two
kinds is numbered by itself: the number of `wasm` is that of the component interface and the number
of `lua` that of the page API, so `wasm` `1.1` and `lua` `1.0` can be the versions of one kit, and
nothing ties the minor number of the one to the other's. A change that no existing view can notice
is a new **minor** version, `1.1` after `1.0`: a new function in the component interface, a new
element or a new colour role in the page API. A change to something a view already uses is a new
**major** version, `2.0`: a type that gains a variant or a field (a new kind of element, a new role
in the interface's colour type, a new member of a record), or a function that means something else.
A host that adopts a new major version offers it together with the one before, as this section asks
of the protocol's own versions; a host that has stopped opening a version refuses an `open` for it
as `unsupported` (Section 6.4). For `lua` the interpreter is the host's: a new element or a new
role is an addition for a script, and what breaks a script is a function of the page API that means
something else.

A program whose `hello` listed a version the host stopped opening finds out from `opens` (Section
5.2), and its library says so in words that name both sides (Section 5.3). A **component** that is
opened under a version (`open.version`) while its own interface is another one cannot run: the host
MUST close the view with `closed`, `reason` `error`, `code` `view` (Section 9.8) and a `message`
that names both versions, the one the component is written against and the one it was opened as
(for instance "this view is written against `pencere:view@0.5.0`; this terminal opens
`pencere:view@1.0.0`"). The host cannot read the interface out of a file before it has the file, so
this is found out when the component is loaded, after the `open` was accepted.

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
| `wasm` | A WebAssembly component that the host runs in a sandbox and draws itself.                                                              | The component interface of the kit, `pencere:view` (`pencere:view@3.0.0` is `3.0`).        |
| `web`  | A web page, shown in a web view of the host's.                                                                                         | The bridge of Section 9.4 between the page and its host: now `1.0`.                        |

A version is a string of two numbers, `major.minor`, such as `0.5`:

```abnf
version = number "." number
number  = "0" / ( %x31-39 *8DIGIT )
```

Versions are compared for equality and for nothing else; the order of a list is the preference of
whoever wrote it. What a version means is the interface's. For the kit, `wasm` `3.0` is the
interface `pencere:view@3.0.0` and `lua` `2.0` and `2.1` are the page API whose `pencere.version` is
`2` (which have, among what `2.0` added to `1.0`, the editors of text of Section 9.8; `2.1` and `3.0`
add the files beside the bundle of Sections 8.8 to 8.10); the
rule that moves the numbers is in Section 5.4, and a host that updates its kit offers the new major
version and the one before (while this document is a draft, the kit's own sandbox opens `wasm` `3.0`
alone: no view the kit knows of was written against `wasm` `2.0`). The examples in this document use made-up versions (`0.4`, `0.5`,
`0.6`) so that they show several at once. For `web`, a change that only adds an optional member to a message of Section 9.4 moves the
minor number, and one that renames or drops a member or a message moves the major. A host MAY open
several versions of a kind — a host that updates its kit offers the one it now has and the one
before — and the program chooses among those both speak (Section 5.2).

A host does not compare versions for "newer" or "older"; it **lists every version it accepts**. A
host that speaks `M.n` of a kind and accepts the earlier minor versions of the same major, as
every host of the kit should (a minor version only adds, Section 5.4), lists each of them (`M.0`
up to `M.n`) and opens each; a program written against `M.k` and not `M.n` is then opened under
`M.k`, the version it was written for. No rule makes `1.2` stand in for `1.0`: if `1.0` is not
listed, it is not opened.

The table grows with this document; a name in `hello.views` that a host does not know is a kind it
does not open, and it leaves it out of its `welcome`. A kind for which no version is shared is a kind
the host cannot open for this program, and it leaves it out as well (Section 5.2). The program names
the kind and the version again in its `open` (Section 6.3), and an `open` of a kind, or of a version
of it, that the `welcome` of its token did not list is refused as `unsupported` (Section 6.4).

The kinds `wasm` and `lua` are the **kit views**, the views the host draws itself (Section 9.8). They
have a component or a script for an entry and no document. The kind `web` is a page and has an HTML
document for an entry; it is the one kind that reaches the host with messages of its own (Section 9.4).

The interfaces from `lua` `2.1` and `wasm` `3.0` on have **files beside the bundle** (Sections 8.8 to
8.10): a host that lists one of them speaks `attach`, `detach`, `read` and `write`, and a program
learns that it may send them from the version it opened, not from a feature of the `welcome`. (This
is what those versions mean, not an order of versions: a host still lists each version it opens.)
Every other view, a page and a kit view at an earlier version, has none, and the host answers those
messages as Sections 8.8 to 8.10 say it does when no view runs. The host's folder (Section 8.11) is
a feature, not a version: whether a host can reach the program's machine does not depend on the
interface of a view. A program uses it for the files of the views that have files.

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
`https`, `ws` or `wss`, a host that is a DNS name (labels of letters, digits and `-`, up to 63
characters each, 253 in all), an IPv4 address or an IPv6 address in brackets, and a port from 1 to
65535 when there is one; the scheme and the host are read in lower case. The host may be a
**wildcard**, `*.` followed by a name of at least two labels (`*.example.com`, not `*.com`): it
stands for the subdomains of the name, at any depth, and not for the name itself, which is listed
on its own when the page needs it. The question to the person shows a wildcard as it was written
(Section 12.2). A member of another name is ignored, so that a program written against a later
version of this specification can say more than an earlier host knows.

The person is asked about every origin that is listed (Section 12.2), so the lists are bounded to
what a person can read: at most 32 entries in each list of origins and 64 in all of them. The entries are
counted as they are written, whether or not they are origins, and a host MAY enforce lower bounds. An
`open` whose `access` goes over a bound is refused with `limit` (Section 6.4).

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

| `code`        | When                                                                                                                                                                                                                                                                                                                                                          |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `token`       | The payload has no string `token`, or it is not the outstanding token. Checked before anything else in the payload.                                                                                                                                                                                                                                           |
| `busy`        | A view is already opening or open in this pane.                                                                                                                                                                                                                                                                                                               |
| `screen`      | The pane is not on the alternate screen (Section 6.5). A host SHOULD refuse so.                                                                                                                                                                                                                                                                               |
| `manifest`    | The payload or the bundle manifest is malformed (Sections 6.3 and 8.2): no `view`, no `version`, no bundle, an `access` that is not of the shape of Section 6.3, or an entry whose type is not that of the `view`. A `view` that is not in the table of Section 5.5 has no type to compare, so that check is skipped and the `open` goes on to `unsupported`. |
| `limit`       | A limit of Section 8.2 is exceeded, or the `access` lists more origins than Section 6.3 allows.                                                                                                                                                                                                                                                               |
| `unsupported` | The host does not support what was asked: a `view`, or a `version` of it, that the `welcome` of the token did not list (or a kind that is not in the table of Section 5.5).                                                                                                                                                                                   |
| `policy`      | The host's own policy, or the person's, refuses.                                                                                                                                                                                                                                                                                                              |

Everything that makes an `open` malformed comes before every limit: an `open` that is both, an entry of the
wrong type and a file over the size, say, or an `access` that goes over its bound and has an entry that is
not a string further on, is refused with `manifest`.

A refusal MUST NOT disturb a view that is already opening or open. A receiver
treats a code it does not know as a refusal without a reason.

```frame fixtures/to-program/closed-refused.frame
<ESC>_pencere;closed;id=2,re=2;{"reason":"refused","code":"token"}<ESC>\
```

### 6.5 Alternate screen

A program MUST switch to the alternate screen (`CSI ? 1049 h`) before it sends
`open`, and MUST switch back (`CSI ? 1049 l`) when it has finished with the view and
before it exits. The primary screen is left as it was, which is what makes the
pane come back when the program ends. The same rule gives the host a cheap check:
a request from a program that is not on the alternate screen is suspect, and a
host that can observe the screen SHOULD refuse it with `screen`; a program that skips the
rule is refused by such hosts, and is at fault where the host does not check.

A host that sees that the pane is not on the alternate screen refuses the `open` (its token is
spent, as for any refusal), and the program that switches to the alternate screen asks again with a
new `hello`. The `open` of the session `screen-refused`, and the answer to it:

```frame fixtures/session/screen-refused/05-in.frame
<ESC>_pencere;open;id=2;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"web","version":"1.0","bundle":{"entry":"index.html","files":[{"path":"index.html","sha256":"83b177a33228866c079d766c12c4b743f8ec7576ea9558264617992720d123ea","size":66,"type":"text/html"}]}}<ESC>\
```

```frame fixtures/session/screen-refused/06-out.frame
<ESC>_pencere;closed;id=2,re=2;{"reason":"refused","code":"screen"}<ESC>\
```

While a view is shown it covers the pane. The program's output goes on drawing
the alternate screen underneath, and when the view closes that screen is what
the person sees, as after any full-screen program.

## 7. The pane

### 7.1 Placement

A view takes the whole content area of the pane of the program that asked, and
the protocol has no field for another place. A pane shows one view at a time.

### 7.2 Size and focus

The view's container is the content area of the pane: the host draws no border, no padding and no
title around a view inside it. The view is told this size, and told again when the pane is resized
(Section 11.1 says how, for each family). What a page or a script asks for in the way of another
size changes nothing: the container is the pane, and its size is the person's.

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

### 8.1 Every view is a bundle

Every kind of view arrives as a bundle, so there is nothing to ask a host about it: a host that opens
a kind takes the bundle of that kind, and a program sends the manifest to any host that offered the
kind (Section 5.2). The `welcome` lists no feature for it.

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

For an `open` it accepts, the host answers with a `need`
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

The host verifies each file before it keeps it: the decoded length does not exceed the `size` the
manifest gave for that digest (the greatest of them, when several files of the manifest have the same
digest and give different sizes) and the SHA-256 of the bytes equals `d`. The `size` is a bound that
spares the host from decoding more than was announced; the digest decides what the file is, so a file
shorter than its `size` whose digest matches is the file. A file it did not ask for, or one it
already has verified for this open, is ignored and not stored. A file that fails verification ends
the open with `closed` `reason` `error`, `code` `bundle`. The host SHOULD end an open whose files do
not arrive in a time of its choosing with `error`, `timeout`, and an open that waits in vain for the
person's answer to the question of Section 12.2 in the same way.

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

When all missing files have been verified, the host starts the view from the store. A **page**
is served: the entry as the document and the other files at their paths, each with the
`Content-Type` of its `type`. The page's origin is unique to this instance of the
view; it is never the host's and never shared with another view, and `'self'` in
the content security policy means that origin. No path outside the manifest
answers. A kit view is not served and has no origin: the host reads its entry, and the pictures
it names, out of the store (Section 9.8).

### 8.7 The entry

The entry of a bundle is the view's own file, and its media type is that of the kind
(Section 6.3): an HTML document for a page, a component for `wasm`, a script for `lua`. The host does
not decide the kind from the entry, and it does not guess. A page may have other files, which it names
by their paths; the other kinds have the entry only, unless the kit says otherwise, which it does
for the images of the kinds the host draws itself (Section 9.8).

### 8.8 Attached files

A program may add files to the view that runs, beside its bundle, for the view to draw: the pictures
of a project, of a document, of a preview. They are kept by content as the files of a bundle are, so a
file that the host still holds crosses the stream once, whichever view or open it is for.

The program sends an `attach`: the files, each by its digest (`sha256`) and its length in bytes
(`size`). The host answers every `attach` with a `need` whose `re` is the attach's id:

- `missing` lists, in the order of the `attach`, the digests it does not hold, which the program sends
  as `file` messages (Section 8.4);
- `refused` lists the digests it will not attach, each with a `code`; it is left out when it is empty.

A file that is attached already is in neither list. When `missing` is not empty, the host answers the
`attach` a second time once the last of its files has come: with a `need` whose `missing` is empty and
whose `refused` lists the files that came and were not what they said. The view is not told: it learns
the names of the files from its program (Section 9.2), as `sha256:` and the digest, and draws them by
those names (Section 9.8). A program that waits for the last answer before it says the name knows that
the view will find the file.

```frame fixtures/to-host/attach.frame
<ESC>_pencere;attach;id=7;{"files":[{"sha256":"e7f0893138aeb54e20f94006b6522ba8331876b1dd2313f77d6c1861cd7b8472","size":520},{"sha256":"b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9","size":11}]}<ESC>\
```

```frame fixtures/to-program/need-refused.frame
<ESC>_pencere;need;id=3,re=7;{"missing":["b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"],"refused":[{"sha256":"e7f0893138aeb54e20f94006b6522ba8331876b1dd2313f77d6c1861cd7b8472","code":"limit"}]}<ESC>\
```

The codes of `refused`:

- `limit`: the file is over 8 MiB (8 388 608 bytes, the bound of a file of a bundle, Section 8.2), or
  attaching it would take the files attached to the view past 1024 or past 64 MiB together. The
  view's bundle is not counted. A host MAY hold a view to less.
- `view`: no view runs, or the view that runs has no files beside its bundle (Section 5.5). Every
  digest of the `attach` is refused so.
- `size` and `digest`: the file came, and it is longer than its size, its base64 is not well formed, or
  its SHA-256 is not its digest. A host verifies an attached file as it verifies the file of a bundle
  (Section 8.4), and a file that fails is refused, not fatal: the view goes on.

The host keeps the bytes of every file attached to the view for as long as the view runs, whatever its
store keeps (Section 8.5), and counts them against the bounds of `limit`. A view names a digest that its
program attached to it, and nothing else: a digest the host holds for another view, or for the bundle,
is no file of this one, so that a view cannot find out what the person has seen elsewhere. The program
sends a `detach` with digests it attached, and the host lets go of them: the view forgets what it drew
of them, and they no longer count. A file is attached to one view, and when the view ends, whatever
ends it, its files are no longer attached; a program that opens a view again attaches them again, and a
host that still holds one does not ask for it.

```frame fixtures/to-host/detach.frame
<ESC>_pencere;detach;id=9;{"files":["e7f0893138aeb54e20f94006b6522ba8331876b1dd2313f77d6c1861cd7b8472"]}<ESC>\
```

A `file` that comes while the view runs and that no `attach` waits for is ignored.

### 8.9 Files the person gives

The person may give the view that runs a file of the person's machine: by dropping it on the view, by
pasting it, or by choosing it in an Open panel that the view asked for (Section 9.8). Nothing else
gives the view a file. The host makes each file a **handle**: 128 bits of the terminal's randomness,
written as 32 lowercase hex digits, that is the file's for as long as the view runs. A kit view is
handed the handle with the file's name, its size and a media type, and the program is handed none of
them: the view passes on what it wants the program to have, in its own messages (Section 9.2). The
name is the person's and the type is the host's guess; a program reads neither as anything but
words, and never joins a name to a path of its own as it came. A host MAY also hand the view the
file's path, and only when it knows that the program reads the same file system as the person (a
program on the person's machine, with no remote shell between). A program that was offered a
folder (Section 8.11) and finds that the folder is not where it runs does not read the path
either: the host was wrong about where the program is.

The program reads the file by its handle, a piece at a time: it sends a `read` with the handle, an
`offset` and a `length` of at most 1 MiB (1 048 576 bytes). The host answers each `read` with one
`data` whose `re` is the read's id: the bytes, in base64, as many as were asked for unless the file
ends first. Fewer bytes than were asked for is the end of the file.

```frame fixtures/to-host/read.frame
<ESC>_pencere;read;id=4;{"handle":"664de63fcac3e6e61bd577ba1ea4b4be","offset":0,"length":1048576}<ESC>\
```

```frame fixtures/to-program/data.frame
<ESC>_pencere;data;id=4,re=4;aGVsbG8gd29ybGQ=<ESC>\
```

A `data` that carries no bytes says why in its `e`, with an empty payload:

- `handle`: the handle is not one of the view that runs (a view that ended has none, and a handle is no
  longer anything after the view is opened again, Section 10.7);
- `range`: the `length` is 0 or over 1 MiB, or the `offset` is past the end of the file;
- `busy`: two `read`s of the program are out already; one more is answered so, and the program asks
  again when one of the two has been answered;
- `gone`: the file is not what it was when the person gave it (its size, or the time it changed), or
  it can no longer be read;
- `io`: reading it failed otherwise.

```frame fixtures/to-program/data-error.frame
<ESC>_pencere;data;id=4,re=4,e=gone;<ESC>\
```

A piece is 1 MiB at most so that a `data`, whose chunks go out with nothing between them (Section 4.4),
does not hold up for long what the view says to the program behind it. A `data` is held and dropped as
what the view says is (Section 4.8): a signal character drops a `data` that was waiting, and a `read` may
be sent again for the same bytes. The host reads only the file of the handle, and only what was asked
for; it never reads a path that a view or a program wrote, and on the program's machine it touches
nothing but its own folder (Section 8.11).

### 8.10 Saving

A view may ask for the Save panel, with a name it proposes for the file (Section 9.8). When the person
chooses a place, the host makes it a handle, as it does for a file the person gives (Section 8.9), and
hands the view the handle and the name the person chose; the view passes the handle to its program.

The program sends the bytes in `write` messages, each with the handle (`h`), the offset of its first
byte (`o`) and at most 1 MiB, in base64; the last has `end`, and may have no bytes. The writes of a
handle come in order and leave no gap: each `o` is where the one before ended.

```frame fixtures/to-host/write.frame
<ESC>_pencere;write;id=5,h=664de63fcac3e6e61bd577ba1ea4b4be,o=0;aGVsbG8g<ESC>\
```

```frame fixtures/to-host/write-end.frame
<ESC>_pencere;write;id=6,h=664de63fcac3e6e61bd577ba1ea4b4be,o=6,end=1;d29ybGQ=<ESC>\
```

The host writes the bytes to a file of its own beside the place the person chose, named so that a
person recognises it (`.` and the name, then `.pencere-part`), and gives it the person's name when the
write with `end` has been written. It makes that file anew and does not write through a link that is
where it goes. It answers once, with a `wrote` whose `re` is the id of that write;
or, if something goes wrong first, at once, with a `wrote` that has a `code` and whose `re` is the id of
the write that failed, and it throws away what it wrote. The writes for that handle that come after the
failure are ignored. When the view ends before the write with `end`, the host throws away what it wrote.

```frame fixtures/to-program/wrote.frame
<ESC>_pencere;wrote;id=5,re=6;{}<ESC>\
```

```frame fixtures/to-program/wrote-error.frame
<ESC>_pencere;wrote;id=5,re=6;{"code":"order"}<ESC>\
```

The codes of `wrote`:

- `handle`: the handle is not a place to save in of the view that runs, or its file has had its
  last write already;
- `order`: a write's `o` is not where the one before ended, or its base64 is not well formed;
- `size`: a write carries more than 1 MiB;
- `space`: the place has no room;
- `io`: writing failed otherwise.

A process that is killed outright, or a machine that stops, may leave the host's file beside the place
the person chose: that is the price of never leaving a half-written file under the person's name, and
the file's name says what it is. The host writes only to the place of the handle; it never writes a path
that a view or a program wrote, and on the program's machine it touches nothing but its own folder
(Section 8.11).

### 8.11 The host's folder

The bytes of a file cross as `data` and `write` messages (Sections 8.9 and 8.10): base64, a piece of 1 MiB
at a time, through the terminal's stream and, at the program, through the tty. When the program runs on
another machine, behind a remote shell, that stream is the slow way: every piece waits for the terminal,
for the tty's input queue and for the program to read it. A host that has a way of its own to the
program's machine MAY keep a **folder** there, and let the bytes of a file cross through a pipe in it.
Nothing but the bytes goes that way: what the program asks and what it is told stay messages, and a
program learns that the folder is there, and which pipe to open, from the messages alone (Section 4). The
program does not ask where it runs, and uses the one mechanism wherever it is.

**The folder.** It is the host's own: a directory on the program's machine under the account's home
directory, one for each instance of the host, with no permission for the group and for others (mode
`0700`). A file in it called `marker` holds the `token` of the `welcome` (Section 5.2), 32 lowercase hex
digits of the host's randomness. The host makes the folder, its `marker` and the pipes this section
names, removes them, and reads and writes nothing else on the program's machine. How it reaches the machine is its own
business, and this document does not say.

**What the program checks.** A `stage` in a `welcome` is an offer, and a path alone proves nothing: a
remote shell inside a remote shell, or a container, may show another folder, of another account, at the
same path. The program uses the offer only if all of this holds, and checks it again for every `welcome`:

- `dir` is an absolute path;
- `dir`, opened, is a directory that belongs to the effective user of the program, and the group and
  others cannot use it;
- `marker` in it, opened without following a link, is a regular file whose content is exactly `token`.

When anything fails the program uses messages, as it did before. The same care goes for a pipe: the
program joins `dir` and the name it was told, which must be 32 lowercase hex digits, opens the result
without following a link, and reads or writes only when it is a pipe (a FIFO) that belongs to its own
user. The host names its pipes from its randomness, so that no name that a person or a program wrote
is ever part of a command of the host.

**Taking a file: `stage`.** For the file of a handle (Section 8.9) the program sends a `stage`, with the
`handle` and the `offset` it wants the bytes to start at. The host answers it twice, as it answers an
`attach` (Section 8.8):

- first, once the pipe is there, with a `staged` whose `re` is the id of the `stage` and whose `pipe` is
  the name of the pipe in the folder;
- then, when it has ended, with another `staged` that says how: `sent`, the offset of the end of the file
  (the length of the file as the host read it), or a `code`.

```frame fixtures/to-host/stage.frame
<ESC>_pencere;stage;id=4;{"handle":"664de63fcac3e6e61bd577ba1ea4b4be","offset":0}<ESC>\
```

```frame fixtures/to-program/staged-pipe.frame
<ESC>_pencere;staged;id=3,re=4;{"pipe":"0b5d7c1e9a3f4682d0e1c7a95b3f8d24"}<ESC>\
```

```frame fixtures/to-program/staged-sent.frame
<ESC>_pencere;staged;id=4,re=4;{"sent":11}<ESC>\
```

The program opens the pipe for reading and reads to its end. It writes what it reads to a file of its own
beside the place where it keeps the file, as it does for a file that comes in `data` (a name made anew,
never written through a link), with the rights its own user has, and it counts the progress itself:
the host does not tell it how far the file has gone. The file is complete when the program has read to
the end of the pipe and its part, the bytes of earlier tries included, is `sent` bytes long; if it is
not, the file did not come, whatever the host said. The pipe is the host's, and the program does not
remove it.

**Sending a file: `fetch`.** For a place the person chose (Section 8.10) the program sends a `fetch`, with
the `handle`, the `size` of the whole file and the `offset` at which the bytes it sends start. The host
answers it twice, with a `fetched`: the `pipe` first, then `saved`, the length of the file it has saved,
or a `code`. The program opens the pipe for writing, writes its file from `offset` to its end, and closes
the pipe. The host writes what it reads to a file of its own beside the place the person chose, as it
does for the `write`s of Section 8.10 (the same name, made anew, not written through a link), and when
`size` bytes are in it, gives it the person's name and answers `saved`. A pipe that ends before `size`
bytes have come, or from which more would come, is `size`. The file is read once and in order, and is
no more seekable than the `write`s of a handle are.

```frame fixtures/to-host/fetch.frame
<ESC>_pencere;fetch;id=5;{"handle":"664de63fcac3e6e61bd577ba1ea4b4be","size":11,"offset":0}<ESC>\
```

```frame fixtures/to-program/fetched-pipe.frame
<ESC>_pencere;fetched;id=5,re=5;{"pipe":"0b5d7c1e9a3f4682d0e1c7a95b3f8d24"}<ESC>\
```

```frame fixtures/to-program/fetched-saved.frame
<ESC>_pencere;fetched;id=6,re=5;{"saved":11}<ESC>\
```

**The codes** of `staged` and `fetched` are the same set. An answer that carries one is the last
answer to its message: there is no `pipe` before it if the host could not begin.

- `handle`: the handle is not one of the view that runs; for a `stage`, it is not a file the person gave;
  for a `fetch`, it is not a place to save in, or its file has had its last bytes, or has failed, or a
  `fetch` of it is out. A view that has no files (Section 5.5), and no view at all, has no handle;
- `busy`: two transfers of the view are out already (a `stage` and a `fetch`, or two of one), and one
  more is answered so: the program asks again when one of the two has been answered;
- `gone`: the file is not what it was when the person gave it (its size, or the time it changed), or it
  can no longer be read;
- `size`: for a `stage`, the `offset` is past the end of the file. For a `fetch`, the `offset` is not
  where the file stands (the first, 0, or after a `route` the `at`; a `fetch` is in order and leaves no
  gap, as a `write` is), or it is past the `size`, or the bytes that came are not as many as the `size`;
- `space`: the place the person chose has no room;
- `route`: the way of the host to the program's machine has gone, for the time being (a lost
  connection, a remote shell that was restarted). The host closes and removes its pipe, and keeps what it
  has written;
- `io`: reading or writing failed otherwise;
- `cancelled`: the program took the transfer back with an `unstage`.

```frame fixtures/to-program/staged-error.frame
<ESC>_pencere;staged;id=4,re=4;{"code":"gone"}<ESC>\
```

**Going on after a `route`.** The way back to the program's machine may come again, and the file need
not start over. For a `stage` the program knows how long its part is: it sends a new `stage` whose
`offset` is that length, and the host reads the file from there. For a `fetch` it is the host that knows
what it holds, so a `fetched` with the code `route` also has `at`, the length of the file the host has
written: the program sends a new `fetch` whose `offset` is `at`, and the bytes after it. A program that
does not want to go on, or is not sure of its part, starts again at 0.

```frame fixtures/to-program/fetched-route.frame
<ESC>_pencere;fetched;id=6,re=5;{"code":"route","at":4}<ESC>\
```

**A pipe that stays silent.** The host's way to the program's machine may be one that does not carry
the pipe (a home directory that the two machines share over a network is the usual case): the program
opens it and no byte ever comes, and neither side learns why. The program therefore waits a bounded time
(Section 4.7) for the first byte after it was told the `pipe`. When none comes it takes the transfer
back with an `unstage` and goes on with messages (`read` and `write`, Sections 8.9 and 8.10). The same
bound holds the host: a program that is told a `pipe` and never opens it is waited for a bounded time,
and then answered `io`.

**Taking one back: `unstage`.** The program sends an `unstage` with the `id` of the `stage` or the
`fetch` it takes back, and is answered, as if it had gone wrong, with the last answer of that message:
`cancelled`. The host closes the pipe and removes it, and a place that a `fetch` had begun to write is
thrown away, as one whose write failed is (Section 8.10). An `unstage` for a transfer that is not out is
ignored.

```frame fixtures/to-host/unstage.frame
<ESC>_pencere;unstage;id=6;{"id":4}<ESC>\
```

**How many, and how it ends.** A view has at most two transfers out at once, `stage`s and `fetch`es
together; a third is `busy`. While a `fetch` of a place is out, a `write` to it is answered `handle`
(Section 8.10). A transfer ends with its view, whatever ends the view (Section 10.2): the host closes its
pipes and removes them, and throws away the part of a `fetch`. A program that ends, or gives the
terminal back (Section 10.3), with a transfer out closes the pipe it has open and throws away its part of
a `stage`. A process that is killed outright may leave a part or a pipe behind, and the name of each says
what it is.

A host that lists `stage` answers a `stage`, a `fetch` and an `unstage` as this section says. One that does
not list it ignores them (Section 4.5): they are no kinds it knows.

## 9. Messages and the bridge of a page

### 9.1 Roles

The **program** is the process on the pane's tty. The **view** is what the host runs or shows in
the pane. The **host** carries what the two say to each other and, for a page, a few things it says
to the host itself. A view asks and a program answers: the view starts every exchange, and the
program never speaks first. The six messages are the same for every kind of view. Sections 9.4 to
9.7 are about pages, which also have a bridge to the host, and Section 9.8 is about the views the
host draws itself.

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

An `id` belongs to the side that asked. It is a JSON number whose value is a whole number from 1 to
2^53 - 1 (9 007 199 254 740 991), the greatest integer a JSON number holds in the language a page is
written in, so that the same number means the same to every end: `1`, `1.0` and `1e3` are ids, `0`,
`1.5`, `"1"` and `9007199254740992` are not, and a message with an `id` that is not one is ignored. A
sender writes the whole number in its plain form. The
view numbers its `call`s and `subscribe`s, from 1 and upward, and two requests of a view that are out
at the same time do not share one: a receiver keeps the first and ignores a `call` or `subscribe`
whose `id` is one of a request that is still out. The `response`, `push`, `progress` and `cancel` that
go with a request carry its number.

A `response` with a `result` ends its `call`, and it is never the answer to a `subscribe`. A
`response` with an `error` ends the request whose `id` it carries, a `call` or a `subscribe`: a program
that cannot serve a `subscribe` (it has no such topic, or the handler failed) answers it so, and the
subscription is over. Otherwise a subscription lasts until a `cancel` ends it. A `cancel` of a request
that is already over, or that the program does not know, changes nothing. A program that is sent a
`cancel` for a `call` it has not answered MAY stop the work and need not answer, and the view ignores
an answer that comes after its `cancel`.

The `code` of an `error` is the program's to choose and a view acts on the ones it knows. The libraries
of this repository use `not-found` (the program has no method or topic of that name), `busy` (too many
calls are being answered at once), `internal` (the handler failed, or its result cannot be written as
JSON) and `message-too-long` (the answer is over the size of a message, Section 9.3), and a view
SHOULD read any other code as the plain failure.

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

A message of the six is one JSON object in the payload of a `msg`, and the host does not read what
it means: it hands the program's messages to the view and the view's to the program, whole and in
order. What the objects mean is between the program and the view. The host does this only while the
view is open: a program's `msg` while no view runs is dropped (Section 10.1). **Once the view has
closed the host writes nothing the view says**, whatever ended it (the program's `bye` or `dismiss`,
the person, the view's own failure, the end of the program), because whatever reads the tty next
would take it for typed input (Section 4.8). The same holds from the moment the host writes a signal
character for the program: what the view had said and the host had not yet written is dropped, not
written after it. A program therefore does not expect a message of the view after the `closed` that
ends its view, or after its own `bye` or `dismiss`, and it must expect to lose the messages that were
waiting when the person pressed ⌃C. A frame is never cut for any other end, the end of the program
among them (Section 4.8): the one under way is written whole, and what waits behind it is dropped.

**What the host reads.** Two things in the objects, in both directions, and nothing else:

- that the payload is one complete JSON object: a `msg` that is not one is dropped, as any message
  the receiver cannot read is (Section 4.5), and
  so is a text that begins with `{` and goes on as something else;
- for a page, the name of the member, to tell the host's own messages (Section 9.4) from the six: in
  the page's direction to handle `context`, `close` and `link`, in the program's to drop a message
  that is named like one of them, because the page must be able to tell the host's `context` from the
  program's words. A view that the host draws itself has no messages of the host's own, and for it the
  host reads only the first.

It reads no value and builds no tree of a message, however large.

**Order.** A page's messages reach the program in the order the page sent them, each whole. A `close`
is acted on at its place in that order: the `closed` with `reason` `view` (Section 10.6) is written
after every message the page sent before the `close`, and none that it sent after is written. A page
that wants the program to know why it closes says so in a message before it, a `call` say; `close` has
no reason.

**Size.** A message is at most 1 MiB (1 048 576 bytes), counted on its JSON text before the escapes of
Section 4.5, in either direction: what the page posted, or the payload the program wrote with each
escape counted as the UTF-8 bytes of the character it stands for (`\u00e9` counts 2, a surrogate pair
4, `\u007f` 1; a program has to write ASCII, and is not held to less than a page that need not). The escapes the host adds when it writes a page's message into the
program's input are not counted, and the limit toward the program leaves room for them
(Section 4.6): a message within the limit is always written. The
host writes the message otherwise as it came, except that it leaves out the tab, line feed and
carriage return that JSON allows between tokens, because a frame has no byte for them; a space is
kept, and the program parses any valid JSON. A program's message over the limit is dropped. A
message of the view over it, a page's or a component's, is one nobody could answer, and a view that
waited for the answer would wait for ever, so the host does what it does to a view that goes over
any limit it is held to: it ends the view and tells the program `closed` with `reason` `error` and
`code` `view` (Section 9.8), and the message is not written.

A page that writes its message over several lines:

```message fixtures/bridge/call-pretty.json
{"call": {
  "id": 1,
  "method": "list"
}}
```

is written to the program with the line feeds gone and the spaces that indented it kept:

```frame fixtures/to-program/msg-pretty.frame
<ESC>_pencere;msg;id=3;{"call": {  "id": 1,  "method": "list"}}<ESC>\
```

Text that a frame cannot carry becomes escapes (Section 4.5): a call whose text is the word `café`,
a DEL and a grinning face, written by the page as it is, reaches the program as

```frame fixtures/to-program/msg-escapes.frame
<ESC>_pencere;msg;id=3;{"call":{"id":1,"method":"say","params":{"text":"caf\u00e9 \u007f \ud83d\ude00"}}}<ESC>\
```

**Depth.** A message whose arrays and objects nest 128 levels deep or more, the outermost counting as
the first, is no message in either direction (Section 4.5): the host drops it, as it drops a text that
is not a complete JSON object. It is the depth at which the common JSON parsers stop, so that every end
agrees on what a message is without building a tree. A sender MUST NOT write one.

### 9.4 The bridge of a page

A page cannot reach the host except through the web platform's message channel, and the host's end
of that channel is the page's bridge. Sections 9.4 to 9.7 are what the kind `web` means, and its version,
**`1.0`**, is the version of the interface a page is written against (Section 5.5). The kit views
have their own section, 9.8.

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
  `link`, is the host's to handle and is not passed to the program. An object counts as having one
  member when the page's own parser would find one: a name written twice (`{"close":{},"close":1}`) is
  one member, the last of them being its value, and a name written with escapes (`"clos\u0065"`) is
  the name it stands for. Any other object, with whatever members, is passed to the program as a
  `msg` (Section 9.3), where it is the program's business to ignore what it does not know: an
  object with a host name and another member (`{"context":{},"x":1}`) is such an object, and reaches
  the program. What is not an object is dropped. A page SDK sends the host's own messages as the
  only member of an object, so that nothing it sends is mistaken for the program's.
- A message of the program whose payload is an object with exactly one member, named `context`,
  `close` or `link`, is dropped: the page must be able to tell the host's `context` from the
  program's words.

(Informative: a host that handles a message of its own the page sent counts it as read, and one it
cannot make sense of as ignored; the counts are for diagnostics and nothing depends on them.)

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

- `mode`: `dark` or `light`, as Section 11.2 defines it;
- `colors`: the roles of the terminal's theme, each a colour as `#rrggbb` in sRGB: `background`,
  `foreground`, `dim`, `accent`, `cursor`, `border`, `success`, `error`, `warning` and `info`, then
  `selection`, and `ansi`, the sixteen colours of its palette as an array of such strings
  (Section 11.2);
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
{"context":{"mode":"dark","colors":{"background":"#0d1117","foreground":"#c9d1d9","dim":"#8b949e","accent":"#58a6ff","cursor":"#79c0ff","border":"#30363d","success":"#3fb950","error":"#f85149","warning":"#d29922","info":"#58a6ff","selection":"#1f3d5c","ansi":["#484f58","#ff7b72","#3fb950","#d29922","#58a6ff","#bc8cff","#39c5cf","#b1bac4","#6e7681","#ffa198","#56d364","#e3b341","#79c0ff","#d2a8ff","#56d4dd","#f0f6fc"]},"font":"Menlo, monospace","size":{"width":800,"height":600},"locale":"en-US","timeZone":"Europe/Istanbul","reduceMotion":false,"access":{"connect":["https://api.example.com"]}}}
```

```message fixtures/bridge/context-request.json
{"context":{}}
```

A host that knows only the background and the foreground of its theme sends the other roles, the
selection and the palette made up (Section 11.2) and leaves out the members it does not know:

```message fixtures/bridge/context-defaults.json
{"context":{"mode":"light","colors":{"background":"#ffffff","foreground":"#1f2328","dim":"#6a6c70","accent":"#58a6ff","cursor":"#58a6ff","border":"#d2d3d4","success":"#3fb950","error":"#f85149","warning":"#d29922","info":"#58a6ff","selection":"#c7e1ff","ansi":["#000000","#cd0000","#00cd00","#cdcd00","#0000ee","#cd00cd","#00cdcd","#e5e5e5","#7f7f7f","#ff0000","#00ff00","#ffff00","#5c5cff","#ff00ff","#00ffff","#ffffff"]},"size":{"width":640,"height":480},"reduceMotion":true}}
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
is through here (Section 12.3): the host MUST accept only a URL that begins with `http://` or
`https://` (the scheme in either letter case), whose every byte is printable ASCII without the space
(0x21 to 0x7E) and that is at most 8192 bytes long, drops any other, SHOULD ask the person, and opens
an accepted one in the person's
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

### 9.8 Kit views

This section is to the kinds `wasm` and `lua` what Sections 9.4 to 9.7 are to `web`: what the kind means,
and what the host does for it. A kit view has no bridge of the host's own; it talks to its program in the
six messages (Section 9.2) and nothing else. What it asks of the host itself (to be closed, text for the
pasteboard, a link to open, to be woken at a time) is not among those messages: it is in what the view
answers the host, by the rule of the kit, and never reaches the program. A view of the kind `wasm` or
`lua` (Section 5.5) is one the host draws itself, in the terminal's font and theme, without a web view:

- a **WebAssembly view** (`wasm`): the entry of its bundle, whose media type is `application/wasm`,
  is a WebAssembly component (the component model's binary format) that the host runs in a sandbox
  of its own. What the component is given and may ask of the host is the interface `pencere:view`
  of the kit, defined by `kit/wit/view.wit` in this repository, which is not part of this document
  and is normative for `wasm` views;
- a **Lua view** (`lua`): the entry of its bundle, whose media type is `text/x-lua`, is the text of
  a script in the Lua language (Luau). The host runs it in the same kind of sandbox, with an
  interpreter that is itself such a component; what the script is given and may ask of the host is
  the same interface, in the interpreter's terms, as the page API of the global `pencere`, and it is
  not part of this document either: `kit/SPEC.md` in this repository describes the page API (the
  elements, the colour strings, the functions), and the prelude it is written in (`kit/luau/prelude`)
  is normative with it for `lua` views. The language is **Luau at the release the kit pins**
  (`0.740` for `lua` `2.x`, `kit/luau/pins`) with the library the interpreter exposes: a script
  that uses syntax a later Luau added is not a `lua` `2.x` script, and such syntax arrives with a
  minor version of `lua` that raises Luau (Section 5.4). A host SHOULD run the kit's own interpreter
  component (`kit/sandbox/luau/luau.wasm`) rather than another Luau, so that a script means one thing
  on every host. The script is source text: a host does not load compiled code from a program.
  The entry is the script, and it MAY `require` the other files of its bundle whose media type is
  `text/x-lua`, by the rules of Luau for requiring by string, relative to the entry's path in the
  bundle (`kit/SPEC.md`, Section 2.5): `require` reaches nothing outside the bundle, so a path that
  leaves its root, an absolute path, an alias, a file of another media type and a file the bundle does
  not have are errors of the script, and no file of the host is read. The text of the scripts a view
  loads, the entry's included, is held to the same limit as the entry alone (the table below).

What these views may ask of the host, and what the host does with it, is the rule of the kit: the
interface of `kit/wit/view.wit`, the page API of `kit/SPEC.md` and the sandbox floors below. The
versions of the kit are those of Section 5.4. (This repository also carries an implementation of the
kit that a terminal may use under the same licence: a page engine that lays out the elements and
makes the draw list, a sandbox that runs a component or a script and hands out pages, the
interpreter for the scripts, and the SDK a page module is written with. It is one way of keeping the
rule and not a part of it: a host that implements the same rules in its own way shows the same
views.)

A host that shows such views offers `wasm`, `lua` or both in the `views` of its `welcome`
(Section 5.2), when the program listed them at a version the host speaks. A program sends such an
`open` only to a host that offered the kind and the version; a host that did not refuses one with
`unsupported` (Section 6.4). A bundle whose entry is not of the kind's type, sent with that kind, is
refused with `manifest`.

```frame fixtures/to-program/welcome-wasm.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["dismiss"],"views":[{"kind":"wasm","versions":["0.5"]},{"kind":"web","versions":["1.0"]}]}<ESC>\
```

Such a view has no `access`: it reaches nothing that the host does not hand it, so there are no
domains to ask the person about (Section 12.2), and an `access` in its `open` is not read. A network
capability, if one is added, is an extension of the kit.

```frame fixtures/to-host/open-lua.frame
<ESC>_pencere;open;id=2;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"lua","version":"0.5","program":"demopen","title":"Containers","bundle":{"entry":"view.lua","files":[{"path":"view.lua","sha256":"7d06ba2f8d1d231826e549de62e88f14c83997c82437df8b44597470da66d081","size":34,"type":"text/x-lua"}]}}<ESC>\
```

A script of several files lists each of them in its bundle, with the entry as `entry`; the files other
than the entry are the ones the entry may `require`, and they are of the type `text/x-lua` like it
(a file of another type is not a script, whatever it is called):

```frame fixtures/to-host/open-lua-modules.frame
<ESC>_pencere;open;id=2;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"lua","version":"0.5","program":"demopen","title":"Containers","bundle":{"entry":"view.lua","files":[{"path":"view.lua","sha256":"73903e55851bb02507f4f680d5981271ab01bd93e8b95645b558f04220f27ba1","size":91,"type":"text/x-lua"},{"path":"parts/init.luau","sha256":"24a9fedcc8d580f117627acce70ca45ce8505cb199190d3af6f421fd9c4ca8d8","size":32,"type":"text/x-lua"},{"path":"screens/logs.lua","sha256":"52e2cfadfa1fcb40e4323d436653d2acfb47c2528a5cd6672770c46ec4311e12","size":82,"type":"text/x-lua"}]}}<ESC>\
```

The host asks for the files as for any bundle (Sections 8.3 and 8.4) and starts the view when it
holds them. It MAY refuse an entry over a size of its own choosing with `limit` (Section 8.2), so
a program must be ready for that.

**What a host gives a view at least.** A view is held to limits (Section 12.4), and a program that
stays within these opens on every host, so a host MUST give a view at least the following, and MAY
give it more:

| Limit                                                                                    | At least               |
| ---------------------------------------------------------------------------------------- | ---------------------- |
| Memory of the module (and its tables)                                                    | 64 MiB                 |
| One call (a frame, a settle, an event) before it is cut off                              | 250 ms                 |
| The start of the module (its first code to run)                                          | 2 s                    |
| Compiling a component                                                                    | 20 s                   |
| The component itself, or the scripts of a view together (the entry and what it requires) | 4 MiB                  |
| Elements in one frame                                                                    | 20 000                 |
| Depth of the tree of one frame (the root is 1)                                           | 128                    |
| Bytes of one line of text, of a title, of text for the pasteboard                        | 64 KiB                 |
| Bytes a frame says apart from its elements' fixed fields (words, paths…)                 | 4 MiB                  |
| Tracks of a grid's columns or rows                                                       | 64                     |
| Values of a sparkline, steps of a path of a canvas                                       | 4096                   |
| Parts of a text of many looks, paths of a canvas, commands of an element                 | 1024                   |
| Entries of a menu                                                                        | 256                    |
| Messages to the program that one call may send                                           | 32                     |
| Bytes of one message to the program                                                      | 1 MiB (Section 9.3)    |
| Pictures: the sides, the file, the decoded pixels, the paths named                       | as listed under Images |
| The document of one editor of the view (bytes of UTF-8)                                  | 16 MiB                 |
| The documents of all the view's editors together                                         | 64 MiB                 |
| Text one reply hands the view's editors, and text one read of an editor answers with     | 1 MiB                  |
| Decorations one reply sends the view's editors                                           | 20 000                 |
| Layers of decorations of one editor, and decorations of all its layers                   | 16, and 200 000        |
| Names of ranges of one editor's text                                                     | 64                     |
| Versions back a change or a decoration of an editor may be made against                  | 1 024                  |

A view that goes over a limit it is held to is closed as one that fails (below). The limits of an
editor are of two sorts: a reply that hands the editors more text or decorations than it may is not a
reply, and the view fails; a change, a load or decorations that would take a document or an editor
past what it may hold, or that are made against a version older than the last 1 024, are refused by
the editor, which tells the view why and does none of it (`kit/SPEC.md`, Section 2.3).

**Editors.** A kit view of version `2.0` may hold editors of text (`kit/SPEC.md`, Section 2.1): rooms
in which the host puts an editor of its own, which keeps the document, its carets and selections,
undoing and what the system's input method composes, lays out and draws the lines, and scrolls them
as it scrolls a box. The view is told what changed, by version, where the carets are and the lines in
view, and reads the text from a copy the host posts it before it tells it of the version; it asks for
changes against the version it saw (the host's editor carries them over what the person typed since),
for selections, and for the colours, marks and folds of its language, by layers. The language is the
view's or its program's: the host's editor knows none, and does only what the view's options say of
how it is written (how it comments, the rules of its indent, folding by the indent: `kit/SPEC.md`,
Section 2.1). Places in a document are characters (Unicode code points) counted from 0.

**Colours.** A page of the kit says a colour in one of two ways. The way to prefer is a **role**
of the theme, which follows the theme the terminal wears: the nine roles of its theme
(`background`, `foreground`, `dim`, `accent`, `cursor`, `success`, `error`, `info`, `warning`),
`selection`, the sixteen colours of the palette by xterm's names (`black`, `red`, `green`,
`yellow`, `blue`, `magenta`, `cyan`, `white` and the same eight prefixed `bright-`), and the
surfaces the kit derives from them (a ground raised by some steps, quieter text, a hairline, the
colour of a shadow). The other way is a **value**, for a colour that no role says: `#rrggbb` or
`#rrggbbaa` in sRGB, with, if the page wants one, a second value for a dark theme. The host decides
which of the two a pane wears (it is `dark` as in Section 11.2) and takes that one; the kit never
does. Either way a colour has an opacity of its own on top, `0` to `1`, which is **multiplied** with
the alpha of a value: `#80808080` with an opacity of `0.5` is drawn at a quarter of full opacity. The
surfaces derived from the roles (`raised`, `quiet` and `shadow`) are the host's to compute, and `border`
is a role like the rest, with the default of Section 11.2; the kit document gives a recommended formula
for each of the derived ones, which makes the same page look alike on every host that follows it.

A view tells the host what the page said, not what it came to. For each line of text it also
names the **grounds** under it: the fills of the box the line is in and of the boxes that hold it,
nearest first, up to and including the first that is opaque; the page's background (`background`)
is under the last of them when it is not. A host that keeps text readable (a terminal with a
setting for the lowest contrast of text) resolves the grounds in its theme, lays them over each
other, and holds the colour of the line to its contrast with the result. That is a rule of the
host, not of the page: it applies to a line whether its colour is a role or a value, and only to
lines of text; lines, fills, borders and the values of a chart are drawn as the page said them.
What is held is the colour of the line, its own opacity included. The opacity of the element the
line is in and of the boxes that hold it (an element made fainter, one fading as it comes or goes)
is laid over what the colour comes to: a line fades with its box all the way, as the box's fill
does, and a page can make words fainter than the floor.

**Images.** A view of the kit shows pictures. An element of its page names a file of the view's own
bundle by the path the manifest gives it (`icons/stop.svg`), and the host draws that file in the
element's box. It is the one thing of its bundle besides its entry that such a view reaches, and the
view does not reach it: the view's code has no file system and is handed no bytes, the host reads the
file out of the bundle it holds and decodes it. There is no network, so an image is a file of the
bundle and nothing else, and a path the manifest does not name is an image that is not there. A view
that has files beside its bundle (Section 5.5) also names a file its program attached to it, as
`sha256:` and the digest (Section 8.8), and a file the person gave it, as `file:` and the handle
(Section 8.9). A manifest path has no `:`, so the three never meet; the host finds such a name only
among the view's own attached files and handles, and a name that is none of them is an image that is
not there.

- _Formats._ PNG, JPEG and SVG, told from the bytes and not from the name or the `type` of the file.
  An SVG draws its shapes: no text in it is drawn and no font is loaded for it, and an `<image>`
  inside it is never loaded, whether it names a path, a URL or carries its data. Nothing else is read
  (not GIF or WebP, not a compressed SVG), and a PNG that is an animation shows its first picture.
- _Limits._ No image is wider or taller than 4096 pixels (an SVG is drawn at most that size), a file
  is at most 8 MiB as any file of a bundle is (Section 8.2), and the decoded pixels of a view, 4 bytes a
  pixel, are at most 64 MiB at a time; a view names at most 1024 different paths, as many as a bundle
  has files. A host MAY hold a view to less, and every host that
  uses the kit holds it to these. An image that is past a limit, or that cannot be read, leaves its
  box empty and gives the person who wrote the view a diagnostic with the reason; the view is not
  closed for it and nothing is written to the screen the program shares with the shell. A path
  beyond the 1024th that a view names is such an image: its box is empty and the diagnostic says
  that the view names more paths than it may.
- _Size._ The element's size is the style's. A side the style does not say follows the picture's
  proportions, and an element that says neither is the picture's own size in pixels, one point to a
  pixel (for an SVG the width and height it says). The sizes are known only once the files are
  decoded, which is not on the thread that draws: the page is laid out first as it is, with the
  room of the pictures that are not known as the style gives it, and once more when they are. A page
  at rest asks for nothing and so is drawn no more often than before.
- _Sharp pictures._ A page is written in points. A host draws a picture at the pixels it comes to,
  and a PNG or a JPEG has the pixels it has: to be sharp on a display with two pixels to the point it
  is given a size in points and sent with twice the pixels (a 24-point icon is a 48-pixel file).
  An SVG needs nothing of the kind: it is drawn at the size it comes to on the display, and drawn
  again when that changes.
- _Fit._ `contain`, the default, draws all of the picture as large as the box lets it be with its
  proportions kept, in the middle of the box. `cover` fills the box with the proportions kept, and
  what does not fit is cut off evenly on both sides.
- _Tint._ A picture may be given a colour (a role or a value, as for any colour above): it is then a
  shape, and every pixel of it is that colour, with the opacity the picture has at that pixel. It is
  how an icon of one colour follows the theme. A tint is not a line of text, and the rule for the
  readability of text does not apply to it. A picture may also be given an opacity of its own, `0`
  to `1`.

A view of the kit at `1.0` of the interface, with a picture beside its entry. The manifest lists the
picture like any file, and the view names it by its path (`icons/stop.svg`); the picture is sent when the
host asks for it, like the rest:

```frame fixtures/to-host/open-wasm-image.frame
<ESC>_pencere;open;id=2;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"wasm","version":"1.0","program":"demopen","title":"Containers","bundle":{"entry":"view.wasm","files":[{"path":"view.wasm","sha256":"24eaf3439a60c96764c4f2c92a0a81b52bd4fe5d5b91f090184a0f6486b1bf29","size":8,"type":"application/wasm"},{"path":"icons/stop.svg","sha256":"2b965986578b523e5024f97f2cf9a3b1d4fc74ca149eeef172dfc74c0c84f4af","size":112,"type":"image/svg+xml"}]}}<ESC>\
```

```frame fixtures/to-host/file-image.frame
<ESC>_pencere;file;id=3,d=2b965986578b523e5024f97f2cf9a3b1d4fc74ca149eeef172dfc74c0c84f4af;PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIyNCIgaGVpZ2h0PSIyNCI+PHJlY3Qgd2lkdGg9IjI0IiBoZWlnaHQ9IjI0IiBmaWxsPSIjODg4Ii8+PC9zdmc+Cg==<ESC>\
```

**Links.** A kit view asks the host to open a link in the person's browser in what it answers the
host (the `link` of a reply: `kit/SPEC.md`, Section 2.3), not in a message, and the host does with it
what Section 9.7 says of a page's `link`. It MUST accept only a URL that begins with `http://` or
`https://` (the scheme in either letter case), whose every byte is printable ASCII without the space
(0x21 to 0x7E) and that is at most 8192 bytes long, drops any other, SHOULD ask the person, and opens
an accepted one in the person's default browser on the person's machine, never on the machine the
program runs on. A link it drops is not a failure of the view: the view is not closed for it, and it
is not told, as it is not told whether the person agreed. A host MAY bound how many questions about
links are open at once and drop the rest. The 8192 bytes are a part of that rule and not a limit of
the table above: a link past them is dropped, the view goes on. (Informative: the addresses of the
fixtures of a page's `link`, `fixtures/bridge/link*.json`, come to the same outcome as the link of a
kit view's reply.)

**Ending.** Section 10.2 runs as for any view: the view is not told and not waited for, it is stopped.
A view that fails (a component traps, goes over a limit the host holds it to, a message of the view
among them (Section 9.3), or hands the host something it cannot use; a script makes an error) is closed as if the person had
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

## 10. Lifecycle

### 10.1 States

A pane is idle, or offered (a token is outstanding), or opening (an open
accepted, files pending or the person being asked), or open (the view running). Closing a view is
instant: nothing is waited for (Section 10.2). A `hello` at any time gets its answer and a new token
without affecting a view that is opening or open; an `open` while one is opening or open is `busy`.
A `msg` of the program in any state but open is dropped, and so is what a page or a script says
before it runs or after it has closed (Section 9.3).

### 10.2 Closing

Whatever ends a view, the host:

1. tears the view down and stops serving its bundle. The view is not told and not waited for: it is
   stopped, and what it had in flight is lost with it;
2. if the program is still there and the cause was neither the program's own end nor a restart
   (Section 10.7), writes `closed` into its input (Sections 10.5, 10.6 and 10.8);
3. forgets the view's token: the one that opened it (Section 6.1), and no other.

A `hello` that arrives after the view is gone is answered at once (Section 10.1). It shows that a
program is reading the tty. The token it gets is not the view's, so step 3 does not forget it. And
the end of a view, whatever the cause, is not the end of the program (Section 4.8).

**The `re` of a `closed`.** A `closed` carries `re` when it answers something that is outstanding.
One that answers a `dismiss` or a `bye` has the id of that message. Any other `closed` that ends a
view that has not yet run (it is still opening: refused, its files wrong or late, the person's
refusal, a failure, the person closing it) answers the `open` and has that `open`'s id as `re`. One
that ends a view that is running (the person closing it, the page's `close`, a failure of the view)
answers nothing and has no `re`. A program library therefore takes a `closed` with `re` equal to its `open`
as the end of an opening view, and one without `re` as the end of the view that runs, and ignores a
`closed` whose `re` is another message's.

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

Either way the host closes the view as Section 10.2 says. For the host's own observation its step 2
does not apply, because the program is gone; for a `bye` the host believes, the answer below takes the
place of step 2.

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
`closed` empties the queue; the flush covers a host that never answers. A `closed` that has no `re`
ends the wait as well: if the person closed the view as the `bye` went out, the host has no view
left that the `bye` could end, ignores it (above), and answers nothing; the `closed` it wrote for the
person is the last frame, and the wait is over when it has been read. A host's answer that comes
after the program stopped reading is the one frame this document cannot keep from a shell, which
is why the wait is long enough to make that rare and short enough for a person not to see it.

```frame fixtures/to-program/closed-program.frame
<ESC>_pencere;closed;id=5,re=6;{"reason":"program"}<ESC>\
```

### 10.4 Interrupt

⌃C goes to the program, and so do ⌃\ and ⌃Z (Section 11.3). With `ISIG` on, the tty turns
the characters into SIGINT, SIGQUIT and SIGTSTP for the foreground process group. A program library that opened a view SHOULD
handle SIGINT by sending `bye` with reason `signal`, waiting for its answer and throwing away what
it has not read (Section 10.3), restoring the terminal (Section 4.9), leaving the alternate screen
(Section 6.5) and exiting. The host writes the character ahead of anything of the view's component
that is waiting, and writes none of it afterwards (Section 4.8): the interrupt does not queue behind
the messages of a component that is saying a lot.

A program that goes on after the signal keeps its view on the screen but hears nothing more from
it: the host cannot tell what the program wrote before it took the signal from what it writes
after, so it does not start writing the component's words again. A program that wants its view
to talk to it again ends it (`dismiss`, Section 10.8) and opens a new one.

SIGTSTP (⌃Z) stops the program, and a stopped program cannot take the view's words or answer them.
A program library that has a view up SHOULD therefore handle SIGTSTP: it ends the view (`dismiss`,
or `bye` to a host that lists no `dismiss`, and waits for the answer as it does for SIGINT),
throws away what it has not read, gives the terminal back (the tty's modes, the alternate screen)
and stops. When it is continued it takes the terminal again, sends a new `hello` and opens its view
anew (Section 10.8). SIGQUIT is the program's as for any program: a program that does not handle it
ends, and the host learns of that as of any end (Section 10.3).

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
the program with `closed` `reason` `view`, after every message the page sent before the `close`
(Section 9.3). A kit view has no message of the host's own: a view that
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
<ESC>_pencere;welcome;id=2;{"version":1,"token":"Zk3mP8rD5uYb1Xe6HqS0vg","host":{"name":"pencere-test","version":"0.0.0"},"features":["dismiss"],"views":[{"kind":"web","versions":["1.0"]}],"restart":true}<ESC>\
```

A running view that the host restarts is torn down at once and without a `closed` (Section 10.2, step
2); the host offers the new token, and the program opens again with it. The session `restart` shows the
offer and what becomes of the token the view had, which is no more:

```frame fixtures/session/restart/06-out.frame
<ESC>_pencere;welcome;id=3;{"version":1,"token":"Zk3mP8rD5uYb1Xe6HqS0vg","host":{"name":"pencere-test","version":"0.0.0"},"features":["dismiss"],"views":[{"kind":"web","versions":["1.0"]}],"restart":true}<ESC>\
```

```frame fixtures/session/restart/07-in.frame
<ESC>_pencere;open;id=3;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"web","version":"1.0","bundle":{"entry":"index.html","files":[{"path":"index.html","sha256":"83b177a33228866c079d766c12c4b743f8ec7576ea9558264617992720d123ea","size":66,"type":"text/html"}]}}<ESC>\
```

```frame fixtures/session/restart/08-out.frame
<ESC>_pencere;closed;id=4,re=3;{"reason":"refused","code":"token"}<ESC>\
```

and of the `open` with the new token:

```frame fixtures/session/restart/09-in.frame
<ESC>_pencere;open;id=4;{"token":"Zk3mP8rD5uYb1Xe6HqS0vg","view":"web","version":"1.0","bundle":{"entry":"index.html","files":[{"path":"index.html","sha256":"83b177a33228866c079d766c12c4b743f8ec7576ea9558264617992720d123ea","size":66,"type":"text/html"}]}}<ESC>\
```

```frame fixtures/session/restart/10-out.frame
<ESC>_pencere;need;id=5,re=4;{"missing":[]}<ESC>\
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

A program that lends the terminal to another program for a while (a shell the person asked
for, an editor on a file) closes its view this way first. It writes what it has for the view
before the `dismiss`, the answer to the call that asked for the shell among it, since a `msg` it
writes after is dropped (Section 10.1). When the `closed` has come it gives the terminal back
as it does at its end (Sections 4.9 and 6.5), and the other program has it, the foreground of the
tty with it, as a shell gives it to a command. When that program ends, the program takes the
terminal again and asks for a view as above. Between the two the pane is idle, and what is typed
and written there is the other program's, as for any program; to the host it is the end of one
view and, later, the first open of another.

## 11. Terminal context

### 11.1 Host context

A page is told what the host knows of the person's machine, not the program's, in its `context`
(Section 9.5): the size of the pane (Section 7.2), the theme and the font, `locale` and `timeZone`,
which are the person's, and whether the person asked for less motion.

A kit view is told less, and by the kit's own interface: each time it is asked to draw, the size of
the pane (in points), the time and whether the person asked for less motion (`kit/wit/view.wit`,
`render`). It is **not** told the person's language or time zone, the font or the `mode` of the
theme, and it has no `context` message: it names a colour as a role of the theme, which the host
resolves in the theme the pane wears (Section 9.8), and the host draws its text in the terminal's
font. Where the two families have the same thing they have the same name: the roles of Section 11.2.

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
| `cursor`     | the cursor's colour; made up as `accent`                                                                                                                                                                                        |
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

These are the roles of both families, under the same names: a kit view says `border`, `cursor`,
`selection` and the sixteen colours of `ansi` (by xterm's names) as a page reads them in its
`context`. The kit has three more, which are derived from the others and which a page makes for itself
(`raised`, `quiet`, `shadow`): `kit/SPEC.md` says how they are derived.

### 11.3 Keyboard

The keyboard divides three ways:

- **The host's own shortcuts stay with the host.** The host does not deliver them
  to the view, and it does not tell the view which they are.
- **The tty's signal characters go to the program:** INTR (⌃C), QUIT (⌃\) and SUSP (⌃Z), as the
  bytes the tty is set to take for them (0x03, 0x1C and 0x1A unless the program changed them),
  written ahead of anything the view has said (Sections 4.8 and 10.4). The kernel makes of them the
  signals the program has a handler for, whatever the view is showing.
- **Every other key is the view's,** ⌘F included.

## 12. Sandbox and network consent

The view runs on the person's machine, so what it can reach is the person's
business and not the program's.

### 12.1 Content security policy

The host builds the policy of a page from what the person allowed. Without any `access` in the
`open`, or when the person allows nothing, the policy is the restrictive one:

```
default-src 'none'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline';
connect-src 'self'; img-src 'self' data:; font-src 'self'; media-src 'self' data:;
frame-src 'none'; object-src 'none'; base-uri 'self'; form-action 'none';
```

(one line), where `'self'` is the page's own origin (Section 8.6). The host MUST deliver the policy
as the `Content-Security-Policy` header of the response to every file it serves for the page, the
entry included: a `<meta>` element in the page is the page's own writing and does not stand for it.
`form-action` has no fallback in `default-src`, so it is written out: a form would otherwise submit
the page to any address. A page can always read its own
bundle, with `fetch` and `XMLHttpRequest` as well as with elements, whatever the person allowed;
that is why `connect-src` and `font-src` have `'self'`, and it reaches no network. What the person
allows of each list of the `access` is added to the directive it is for, and the policy is that one
with those origins and nothing else changed: `connect` to `connect-src`, `resources` to `script-src`,
`style-src`, `img-src`, `font-src` and `media-src`, `frames` to `frame-src` (in place of `'none'`) and
`base` to `base-uri` (in place of `'self'`). A page never gets a domain the person did not approve,
whatever it asked.

### 12.2 Network

By default a page reaches nothing outside its own bundle (Section 8.6) and the host's own bridge to its
program. Where the `access` of
the `open` names origins (any of `connect`, `resources`, `frames`, `base`) or asks for a
browser capability in `permissions`, the host MUST ask the person before the page can use them, MUST NOT
allow what the person did not approve, and tells the page what it allowed in the `access` of its
`context` (Section 9.5). A view of the other kinds has no `access` to ask about.

The question names the origins and the capabilities, says where the program runs (this machine, or the
host name of the other one) and names the program as it called itself. A host SHOULD say when an origin is a
loopback or private address and MAY refuse such names. An origin that is not of the form Section
6.3 gives is not allowed, and the page is not told why. How long an answer is remembered is the
host's policy; it SHOULD NOT carry from one program to another. A host SHOULD end an open that waits
for the answer for longer than a time of its choosing with `closed`, `reason` `error`, `code`
`timeout` (Section 8.4), as it does for files that do not arrive.

### 12.3 Files and navigation

The view has no access to files of its own (a kit view reaches only the files the person gives it, by
their handles, Section 8.9) and does not navigate to another document: after the load of its
entry, the host MUST cancel every top-level navigation of the web view to another document, whatever
starts it (a link the page follows, a change of `location`, a form, a script that opens a window). A
navigation that only changes the fragment (`#…`) of the same document is not one to another document
and is allowed, and the loading of a frame that the policy lets the page nest (Section 12.1) is not a
top-level navigation. `form-action 'none'` in the
policy (Section 12.1) says the same of forms to the web view itself. The `link` message of the
page is the only way out; what the host accepts and what it does with it is in Section 9.7. A view the
host draws itself has no document to leave, and its only way out is the `link` of its reply, held to
the same rule (Section 9.8).

A file moves between the person's machine and a program only by the person's own act: a drop, a paste,
a choice in an Open or a Save panel. A view asks for a panel only in what it answers something the
person did (Section 9.8), and one panel is open at a time. The host reads and writes only the files of
the handles it made, never a path that a view or a program wrote, and on the program's machine only its
own folder (Section 8.11); a handle is nothing once its view has ended. A **page** is given files the web platform's way (a drop, an `<input type="file">`), which is
the person's act too, and reaches their bytes in its own script, which may pass them on to its program
in its messages; the messages of Sections 8.8 to 8.10 are not a page's.

### 12.4 Origin, limits, indication

A page's origin is unique to it (Section 8.6); a view the host draws itself has none. A host SHOULD bound the
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
person may not control, and what it is given is the host's to decide (Section 9.8): the host runs
it in a sandbox that gives it nothing it was not handed, and bounds its memory and time
(Section 12.4). The messages between it and its program are a stream anyone can write to while the
view is open — the rule of the first paragraph applies, since a process that can write the pane's
output can send a `msg` — so a view treats what its program says as untrusted, and a program what
its view says. What the view says is never written to the program once the view is over. Its only way
out is a link it asks the host to open, which the host holds to the rule of a page's `link` (Sections
9.7 and 9.8). The words
a failed view's `closed` carries (`message`) are the view's own and are printed by the program, not
by the host: a program that prints them treats them as it treats any text from another party.

**Resource use.** Frame, message and bundle limits (Sections 4.6, 8.2) bound what a
receiver buffers, and the limit on what waits for a program that does not read (Section 4.8) bounds
what a view can make the host keep; hosts SHOULD also limit how often one pane may ask (answers to
`hello`, `open`).

## 15. Conformance

A **host** conforms if it does everything this document asks of a host with MUST and
MUST NOT, in Sections 4 to 12; a **program library** conforms if it does everything it asks
of a program. SHOULD is for what an implementation has a reason to skip.

There are two levels. **Core** is Sections 4 to 9, 10 without 10.7 and 10.8, 11, 12 and 13. **Full**
adds restart (Section 10.7) and the program closing its view (Section 10.8). A host below Full does not
list `dismiss` in its `features`; a program below Full does not send a `dismiss` and ignores an
unsolicited `welcome`. Each kind of view is optional: a host offers in its `views` the kinds it shows,
the pages (`web`) among them or not, the views it draws itself (Section 9.8) among them or not, and a
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
  (its name and version, the tokens it issues in order, the digests its store already holds, the
  kinds it opens): the bytes the host emits after each `in` step, or after each `event` step
  (something on the terminal's side: the person closing the view, a wait running out, a restart, a
  page's message), are the next `out` step's file, or nothing when the step is `null`; an `out` step
  may also name the other actions the host takes (`acted`) and what it tells the view (`to-view`).

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
  features?: string[]; // "dismiss" (Section 10.8), "stage" (Section 8.11)
  stage?: { dir: string; token: string }; // with "stage" in features: the host's folder and what its marker file holds (Section 8.11)
  views?: ViewOffer[]; // with token (always present then): the program's list cut down to what the host opens, in the program's order; may be empty (Section 5.2)
  opens?: ViewOffer[]; // the kinds of the program's list that the host opens at none of its versions, with the versions it does open (Section 5.2); left out when none
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
  refused?: { sha256: string; code: string }[]; // an attach's only (Section 8.8); left out when empty
}

interface Attach {
  files: { sha256: string; size: number }[]; // Section 8.8
}

interface Detach {
  files: string[]; // sha256 digests the program attached
}

interface Read {
  handle: string; // 32 lowercase hex digits (Section 8.9)
  offset: number; // 0 to 2^53 - 1
  length: number; // 1 to 1048576
}

interface Wrote {
  code?: string; // absent when the file was written (Section 8.10)
}

interface Stage {
  handle: string; // 32 lowercase hex digits (Section 8.9)
  offset: number; // 0 to 2^53 - 1: where in the file the bytes start (Section 8.11)
}

interface Fetch {
  handle: string; // 32 lowercase hex digits (Section 8.10)
  size: number; // the length of the whole file
  offset: number; // where in the file the bytes sent start
}

interface Unstage {
  id: number; // the id of the stage or the fetch taken back (Section 8.11)
}

// A staged has one of pipe, sent and code; a fetched, one of pipe, saved and code (Section 8.11).
interface Staged {
  pipe?: string; // 32 lowercase hex digits: the name of the pipe in the host's folder
  sent?: number; // the file went through the pipe and ended at this offset
  code?: string; // why not
}

interface Fetched {
  pipe?: string;
  saved?: number; // the file is saved, and this long
  code?: string;
  at?: number; // with code "route": the length of the file the host holds
}

interface Closed {
  reason: "refused" | "error" | "user" | "view" | "program";
  code?: string;
  message?: string; // for people; never parsed. A failed view says why (Section 9.8)
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
    cursor: string;
    border: string;
    success: string;
    error: string;
    warning: string;
    info: string;
    selection: string; // the background of what the person selected (Section 11.2)
    ansi: string[]; // the palette's sixteen colours, as "#rrggbb", in the order of Section 11.2
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

Codes of `Need.refused`: `limit`, `view`, `size`, `digest` (Section 8.8). Codes of the `e` of a `data`:
`handle`, `range`, `busy`, `gone`, `io` (Section 8.9). Codes of `Wrote`: `handle`, `order`, `size`,
`space`, `io` (Section 8.10). Codes of `Staged` and `Fetched`: `handle`, `busy`, `gone`, `size`, `space`,
`route`, `io`, `cancelled` (Section 8.11).

Codes of `Closed`: for `refused`, those of Section 6.4; for `error`, `bundle` (Section 8.4), `view`
(Section 9.8) and `timeout` (Sections 8.4 and 12.2). A receiver treats a code it does not know as the
plain reason. (The codes of the `error` of a `response`, Section 9.2, are another thing.)

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
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["dismiss"],"views":[{"kind":"web","versions":["1.0"]}]}<ESC>\
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
{"context":{"mode":"dark","colors":{"background":"#0d1117","foreground":"#c9d1d9","dim":"#8b949e","accent":"#58a6ff","cursor":"#79c0ff","border":"#30363d","success":"#3fb950","error":"#f85149","warning":"#d29922","info":"#58a6ff","selection":"#1f3d5c","ansi":["#484f58","#ff7b72","#3fb950","#d29922","#58a6ff","#bc8cff","#39c5cf","#b1bac4","#6e7681","#ffa198","#56d364","#e3b341","#79c0ff","#d2a8ff","#56d4dd","#f0f6fc"]},"font":"Menlo, monospace","size":{"width":800,"height":600},"locale":"en-US","timeZone":"Europe/Istanbul","reduceMotion":false,"access":{"connect":["https://api.example.com"]}}}
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
