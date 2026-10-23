# pencere

**MCP Apps in the terminal** — specification draft

|                     |                               |
| ------------------- | ----------------------------- |
| Status              | Draft                         |
| Protocol version    | 1                             |
| Draft of            | 2026-10-21                    |
| Normative reference | MCP Apps, revision 2026-01-26 |

## 1. Introduction

A program running in a terminal pane — on the person's machine, or on a
server reached over plain ssh with no GPU — can show a web interface in that
pane, the way a full-screen program takes the screen and gives it back when it
ends. The interface is an MCP App view: HTML and JavaScript in a sandbox,
talking JSON-RPC to its host. The program is an MCP server.

MCP Apps defines the view, its sandbox, its messages and its theming. It has
no word for the terminal. This document defines only that part:

- how MCP messages travel between a program and the terminal that hosts it,
  inside the terminal's own byte stream (Section 4);
- how a program finds out the terminal can host a view, and which version they
  speak and which kinds of view (Section 5);
- who may open a view (Section 6) and where it appears (Section 7);
- how a view with many files is loaded once and kept by content (Section 8);
- how the pieces bridge to an MCP session (Section 9) and when a view starts
  and ends (Section 10);
- what the terminal tells the view about itself (Section 11);
- what happens when there is no host (Section 13).

It does not define a second view protocol. A view that is valid MCP Apps opens
unchanged in a pencere host; a pencere program is an ordinary MCP server and
starts unchanged as one (Section 13).

Non-goals: letting a program choose where its view appears (a view always takes
the pane of the program that asked; a person who wants it elsewhere splits the
pane first and starts the program there); reaching a host through a terminal
multiplexer that does not forward the stream untouched (such a program sees no
host and falls back, Section 5.3); the internals of any platform's web view.

## 2. Conventions and terms

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
"SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY" and "OPTIONAL" are to be
interpreted as described in BCP 14 (RFC 2119, RFC 8174) when, and only when,
they appear in all capitals. Grammar is ABNF (RFC 5234); JSON is RFC 8259.

**MCP Apps** is "SEP-1865: MCP Apps", revision 2026-01-26
(`specification/2026-01-26/apps.mdx` in modelcontextprotocol/ext-apps). It is
the normative reference. "MCP Apps § X" names the section headed X in that
document. Where this document and MCP Apps disagree about something MCP Apps
defines, MCP Apps is right and this document is wrong. Nothing here is a
requirement of a later MCP Apps revision unless the text says so. **MCP** is the
Model Context Protocol, revision 2025-11-25, and "MCP § X" names the section of its
specification headed X.

| Term         | Meaning                                                                                                           |
| ------------ | ----------------------------------------------------------------------------------------------------------------- |
| Host         | A terminal emulator that implements this document: it owns panes, the view and its sandbox, and the bundle store. |
| Program      | A process running on a pane's tty that wants to show a view. It is an MCP server.                                 |
| Pane         | A terminal pane: one tty and the screen it draws.                                                                 |
| View         | An MCP App view (MCP Apps § UI Resource Format) shown in a pane.                                                  |
| Person       | The human looking at the pane. The view runs on the person's machine; the program may run on another.             |
| `to-host`    | The direction program → host: bytes the program writes to its tty.                                                |
| `to-program` | The direction host → program: bytes the host writes to the input side of the pane's tty.                          |
| Frame        | One APC string in this protocol (Section 4.1).                                                                    |
| Message      | A kind, control parameters and a payload, carried by one or more frames (Section 4.4).                            |
| Chunk        | One frame of a message.                                                                                           |
| Token        | A single-use secret the host gives a program so that it can open a view (Section 6.1).                            |
| Bundle       | The files of a view, listed with their SHA-256 digests (Section 8).                                               |

Examples of frames are written with `<ESC>` for the byte 0x1B, one frame per
line. The lines of a block are concatenated with nothing between them, so a
block is the exact stream. The repository's `fixtures/` directory holds the same
bytes as files; every example here names its file in the block's heading and a
tool checks that they match.

## 3. Relationship to MCP Apps

**Taken unchanged**, by reference only:

- the UI resource and its metadata, and the restrictive default content
  security policy (MCP Apps § UI Resource Format);
- the link from a tool to its view, and tool visibility (MCP Apps § Resource
  Discovery);
- the messages between a view and its host, and the handshake (MCP Apps §
  Standard MCP Messages, § App Capabilities in `ui/initialize`, § MCP Apps
  Specific Messages, § Lifecycle, § Data Passing);
- the host context, container dimensions and theming variables (MCP Apps §
  Host Context in `McpUiInitializeResult`, § Container Dimensions, § Theming);
- how content security policy is built from the declared domains (MCP Apps §
  Security Implications → Content Security Policy Enforcement);
- negotiation of the MCP Apps extension between an MCP client and server (MCP
  Apps § Client\<\>Server Capability Negotiation).

**Added by this document**: the transport (Section 4), the capability query
(5), the token and the open request (6), the way a view is placed in a pane and named there (7),
bundles (8), the mapping to an MCP session (9), including the program's progress
passed to the view (9.6), the lifecycle of a view in a terminal (10), the terminal's context (11), consent for network access (12) and
the fallback (13).

**Different for a terminal host**, because there is no conversation and no web
page: the host has no model, so `ui/message` and `ui/update-model-context` are
refused (Section 9.5); a native host needs no sandbox proxy, while a host that
is itself a web page MUST follow MCP Apps § Sandbox proxy.

A host MUST NOT refuse a view merely because it is valid MCP Apps and knows
nothing of this document, and a program that is a valid MCP Apps server MUST
stay one when it uses this document (Section 13).

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
<ESC>_pencere;hello;id=1;{"versions":[1],"views":["web"]}<ESC>\
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
<ESC>_pencere;msg;id=5,more=1;{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"3 containers"}],"structuredConte<ESC>\
<ESC>_pencere;msg;id=5,seq=1,more=1;nt":{"containers":[{"id":"c1","name":"web","state":"running"},{"id":"c2","name":"db","state":"runnin<ESC>\
<ESC>_pencere;msg;id=5,seq=2;g"},{"id":"c3","name":"cache","state":"exited"}]}}}<ESC>\
```

### 4.5 Message kinds

| Kind      | Direction    | Payload                                          | First-chunk parameters                           | Section |
| --------- | ------------ | ------------------------------------------------ | ------------------------------------------------ | ------- |
| `hello`   | `to-host`    | JSON `Hello`                                     | none                                             | 5       |
| `welcome` | `to-program` | JSON `Welcome`                                   | `re`: the hello answered; none when unsolicited  | 5, 10.7 |
| `open`    | `to-host`    | JSON `Open`                                      | none                                             | 6       |
| `need`    | `to-program` | JSON `Need`                                      | `re`: the open answered                          | 8.3     |
| `file`    | `to-host`    | base64 (RFC 4648 § 4, with padding) of one file  | `d`: its digest                                  | 8.4     |
| `msg`     | both         | one JSON-RPC 2.0 message, or a JSON object (8.8) | none                                             | 9, 8.8  |
| `closed`  | `to-program` | JSON `Closed`                                    | `re`: the open or the dismiss it answers, if any | 6.4, 10 |
| `bye`     | `to-host`    | JSON `Bye`                                       | none                                             | 10.3    |
| `dismiss` | `to-host`    | JSON `Dismiss`                                   | none                                             | 10.8    |

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
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle","dismiss"],"views":["web"]}<ESC>\
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
<ESC>_pencere;msg;id=4,more=1;{"jsonrpc":<ESC>\
<ESC>_pencere;msg;id=4,seq=2;"2.0"}<ESC>\
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
  meanwhile waits.
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
program speaks (Section 5.4), and `views`, the kinds of view it can show (Section 5.5), in
the order it prefers them. A host ignores a `hello` that lacks either, whose `views` is empty,
or that is not a list of strings, as it ignores a message it cannot read.

```frame fixtures/to-host/hello-fence.frame
<ESC>_pencere;hello;id=1;{"versions":[1],"views":["web"]}<ESC>\
<ESC>[c
```

### 5.2 The answer

A terminal that supports this document reads the frames in its output in
stream order, so it handles `hello` before it handles the `CSI c` that follows.
It answers by writing a `welcome` frame into the pane's input **before** it
writes the reply to that `CSI c`. A host MAY decline to answer (Section 6.2); the
program then sees only the reply to `CSI c`.

The host chooses the greatest version both support. If there is one, it issues a
token (Section 6.1) and answers with `version`, `token`, `host`, `features` and `views`,
and `re` set to the hello's id. If there is none, it answers with `supported`
and `host` only.

`features` lists the optional parts of this document the host supports: `bundle`
(Section 8.1) and `dismiss` (Section 10.8). A program uses only what is listed and ignores a
name it does not know.

`views` lists the kinds of view the host will open (Section 5.5): the list of the `hello`,
with every kind the host will not open taken out, and in the program's order. The host only
eliminates; it never reorders and never adds a kind the program did not list, and it writes each
kind once. A name that is not in the table of Section 5.5 is one the host will not open. `views`
is always present, and it is empty when the program and the host share no kind: the host
answers all the same, with a version and a token, so that the program knows at once that there
is a host and nothing it can show, and does not wait.

```frame fixtures/session/handshake/02-out.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle","dismiss"],"views":["web"]}<ESC>\
```

```frame fixtures/to-program/welcome-unsupported.frame
<ESC>_pencere;welcome;id=1,re=1;{"supported":[1],"host":{"name":"pencere-test","version":"0.0.0"}}<ESC>\
```

```frame fixtures/to-program/welcome-no-views.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle","dismiss"],"views":[]}<ESC>\
```

### 5.3 What the program concludes

| What the program reads first                                              | Conclusion                                                                                                                                                                                                                                  |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A `welcome` with `version` and `token`, `re` equal to its last hello's id | There is a host. If `views` has a kind, go on with Section 6 and open the first it can show. If `views` is empty, there is a host and nothing to open: fall back at once, as with no host (Section 13); do not wait and do not send `open`. |
| The terminal's answer to `CSI c` (`ESC [ ? … c`)                          | There is no host. A `welcome` that arrives later MUST be ignored.                                                                                                                                                                           |
| A `welcome` with `supported` and no `version`                             | There is a host that shares no version with this program. Treat as no host.                                                                                                                                                                 |
| Nothing                                                                   | After a time, no host. The time SHOULD be 2 seconds; a program MAY wait longer on a slow link.                                                                                                                                              |

2 seconds is a precedent, not a measurement (Appendix D). A program that
concludes "no host" MUST NOT send further frames of this protocol (Section 13).
One that concluded by timeout SHOULD discard a late answer to `CSI c`
(`ESC [ ? … c`) for as long as it still reads the tty, so that it does not reach
a shell as typed text.

A `welcome` is only believed when the program has sent a `hello` and
the `welcome` answers it; an unsolicited `welcome` is handled by Section 10.7 or ignored.

```frame fixtures/to-program/noise.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle","dismiss"],"views":["web"]}<ESC>\
<ESC>[?62;4cls<CR>
```

(The example is a `welcome` followed by the terminal's answer to `CSI c` and a
typed `ls` with its carriage return; a decoder delivers the one `welcome`.)

### 5.4 Versions

A version is a positive integer. `hello` lists the versions the program speaks.
This document defines version 1. An implementation of version N MUST also
implement version N−1 (when N is greater than 1) and a program SHOULD list both,
so that a program built against one release and a host built against the next
or the previous still meet. A published version's fixtures never change; a
change of the wire is a new version with new fixtures.

A program MAY send `hello` again at any time; each answered `hello` gets a new
token (Section 6.1).

### 5.5 Kinds of view

What a view is made of is the program's to say and the host's to accept: it is not inferred
from the files of a bundle. A **kind** of view is named by a string in a closed table that this
document defines:

| Name   | The view                                                                                                                              |
| ------ | ------------------------------------------------------------------------------------------------------------------------------------- |
| `wasm` | A WebAssembly component that the host runs in a sandbox and draws itself, and that exchanges messages with the program (Section 8.8). |
| `web`  | A web page, shown in a web view of the host's, that is an MCP Apps view and bridges to an MCP session of the program (Section 9).     |

The table grows with this document; a name in `hello.views` that a host does not know is a
kind it does not open, and it leaves it out of its `welcome` (Section 5.2). The program names
the kind again in its `open` (Section 6.3), and an `open` of a kind that the `welcome` of its
token did not list is refused as `unsupported` (Section 6.4).

For now `web` is the MCP bridge of Section 9 and nothing else: the messages of Section 8.8
(a view's and its program's own `msg` objects) do not apply to it, and the `msg` of a web view
is the JSON-RPC message of Section 9.2.

## 6. Opening a view

### 6.1 The token

A token is 16 bytes from a cryptographically secure random generator, written as
base64url without padding (RFC 4648 § 5), 22 characters. A host keeps at most one
outstanding token per pane.

- A token is spent by the first `open` that carries it, whatever the outcome of
  that `open`. A token that opened a view stays that view's identity for as long as the
  view is opening, open or closing: the program's `bye` (Section 10.3) is believed only
  when it carries it, and no `open` can spend it again.
- A token is replaced when the host issues another for the same pane (another
  answered `hello`, or Section 10.7). The replaced token is no longer valid.
- A token ends when the pane's program ends, and a host MAY end it sooner. It does
  not end with a view: a token issued while a view was opening, open or closing is
  still good when the view is gone (Section 10.2).
- An `open` with a token that does not match has no effect on the outstanding one.

A program that merely writes to the stream — a file shown with `cat`, a
program in the background — can send `hello` but cannot read the answer, so it
cannot open a view (Section 14).

```frame fixtures/session/token-single-use/05-in.frame
<ESC>_pencere;open;id=3;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"web","tool":{"name":"list_containers","arguments":{}},"bundle":{"entry":"index.html","files":[{"path":"index.html","sha256":"aeba6c19a632d2153a9e336a88b385d438c474a996d5be1408f6cd2ca6898e54","size":132,"type":"text/html"},{"path":"app.js","sha256":"bcc81d22a717681517bc99bbad72438377e02c19583fae67522475fd8b3ca62c","size":67,"type":"text/javascript"},{"path":"style.css","sha256":"72735d03f2b7e5a9ddb8ebe1c5fde2da88aa74292ba7ac608490a94f0ebbac0e","size":61,"type":"text/css"}],"meta":{"csp":{"connectDomains":["https://api.example.com"]},"prefersBorder":false}}}<ESC>\
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
having switched to the alternate screen (Section 6.5):

- `token`: the token from the `welcome`;
- `view`: the kind of view (Section 5.5), one of the `views` of the `welcome` that issued the
  token. REQUIRED. The host does not work it out from the bundle;
- `tool`: the name of the tool whose `_meta.ui.resourceUri` names the view (MCP
  Apps § Resource Discovery), and the `arguments` to call it with; REQUIRED for a `web` view.
  A `wasm` view has no tool (Section 8.8);
- `bundle`: OPTIONAL for a `web` view, the bundle of Section 8; REQUIRED for a `wasm` view.
  The `type` of its entry has to be that of the kind: `text/html` for `web`, `application/wasm`
  for `wasm`.

```frame fixtures/to-host/open.frame
<ESC>_pencere;open;id=2;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"web","tool":{"name":"list_containers","arguments":{}}}<ESC>\
```

### 6.4 Host checks and refusal

The host checks an `open` in this order and stops at the first that fails; it
answers a failure with a `closed` message whose `re` is the open's id,
`reason` is `refused` and `code` is:

| `code`        | When                                                                                                                                                                                               |
| ------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `token`       | The payload has no string `token`, or it is not the outstanding token. Checked before anything else in the payload.                                                                                |
| `busy`        | A view is already opening or open in this pane.                                                                                                                                                    |
| `screen`      | The pane is not on the alternate screen (Section 6.5). A host SHOULD refuse so.                                                                                                                    |
| `manifest`    | The payload or the bundle manifest is malformed (Section 8.2): no `view`, a `web` view with no tool, a `wasm` view with no bundle, or an entry whose type is not that of the `view` (Section 6.3). |
| `limit`       | A limit of Section 8.2 is exceeded.                                                                                                                                                                |
| `unsupported` | The host does not support what was asked: a `view` that the `welcome` of the token did not list (or that is not in the table of Section 5.5), or a bundle without the `bundle` feature.            |
| `policy`      | The host's own policy, or the person's, refuses.                                                                                                                                                   |

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

### 7.2 Display mode

MCP Apps names three display modes (MCP Apps § Display Modes). A terminal pane is
none of them exactly, and the reference implementations of MCP Apps validate the
mode of a view's `ui/initialize` and of the host's answer against exactly those
three values, so a fourth value would make an unmodified host reject a view that
asks for it and an unmodified view reject a host that names it. This document
therefore adds no display mode. It places the view in the pane with the
mechanism MCP Apps provides for extensions, the `experimental` capabilities.
(The reference implementation's schema, `src/generated/schema.ts` in
modelcontextprotocol/ext-apps at the time of writing, is a closed union for the
modes and an open record for `experimental`.)

The host tells every view its container: `containerDimensions` is fixed
(`width` and `height` both set, in CSS pixels, the content area of the pane) and
`displayMode` is `inline`, or, when the view's `availableDisplayModes` does not
list `inline`, the first of `fullscreen` and `pip` it lists; the host context's
`availableDisplayModes` lists that one mode only. The host MUST NOT use a mode
the view did not list (MCP Apps § Display Modes). A view that does not know this
document therefore opens in a fixed-size container like any MCP App in a host
whose container has a fixed size, and fills it.

A view that is written for the terminal says so by listing
`appCapabilities.experimental["pencere/pane"]` as `{}` in `ui/initialize`. A host
that supports this document answers with `hostCapabilities.experimental["pencere/pane"]`
set to `{}`, and only to such a view. A view that sees it may assume that its container is
the whole pane, that nothing of the host is drawn around it, and that the
keyboard is entirely its own except for what Section 11.3 reserves. For such a
view the host SHOULD draw no border or padding of its own whatever `prefersBorder`
says; for any other view it treats `prefersBorder` as MCP Apps § UI Resource Format
defines.

`ui/request-display-mode` is answered with the current mode, which MCP Apps §
Display Modes asks of a host that does not switch. When the pane is resized the host
sends `ui/notifications/host-context-changed` with the new `containerDimensions`.
`ui/notifications/size-changed`, with which a view asks a host whose container follows
its content to resize it (MCP Apps § Container Dimensions), changes nothing: the
container is the pane, and its size is the person's.

### 7.3 Names and icons

A host names a view in its own chrome (a tab, the header of a pane, a list of panes), never
inside the view's container (Section 7.2). What it draws there comes from the program's MCP
session (Section 9.3) and not from a message of this document, so a program names itself
and its tools to a terminal as it does to any other MCP client (MCP 2025-11-25):

- the tool's name for people is its `title` in `tools/list`, else its `annotations.title`;
  the program's is `serverInfo.title` of its `initialize` result, else `serverInfo.name`.
  The program wrote them, so a host draws them as text, without control characters,
  bidirectional overrides or invisible format characters, cut to a length of its choosing;
- the icons are the tool's `icons` and `serverInfo.icons` (MCP 2025-11-25 § Basic → General
  fields → `icons`).

MCP asks a consumer of icons to treat their metadata and their bytes as untrusted, to fetch
them without credentials and only from the server's origin, and to bound what they cost and
what they can run. A terminal host does so by fetching nothing:

- A host MUST NOT fetch an icon. It uses only an icon whose `src` is a `data:` URI with
  base64 data (RFC 2397). The host draws on the person's machine, where a view has no
  network unless the person allows it (Section 12.2), and a program at the other end of an
  ssh connection has no origin an icon could be checked against: fetching an icon would tell
  whoever serves it who opened the program, from where and when.
- A host uses an icon only when its bytes are, by their own first bytes, an image of a type
  it allows, and MUST NOT draw an SVG icon in a way that runs what it contains. A `mimeType`,
  or a media type in the `data:` URI, that names another type than the bytes makes the icon
  unusable (MCP: the media type is advisory, a mismatch is a reason to reject).
- A host bounds an icon's bytes, its dimensions and its frames before it decodes the image.
- A host MUST NOT draw a program's icon in a question it asks the person in its own name
  (Section 12.2), where the picture could pass for the terminal's own or another
  application's.
- An icon the host does not use, and an `icons` member that is not a list of icons, are no
  reason to refuse the view.

The reference host (`host/rust` in this document's repository) takes PNG, JPEG and WebP, at
most 64 KiB, at most 512 pixels a side and only a still image, at most four icons of the
first sixteen entries of a list, and passes the tool's and the program's icons to the
terminal apart, as it passes their names.

## 8. Bundles

A view that is one HTML document needs no bundle: the host reads it with
`resources/read` (MCP Apps § UI Resource Format) and Section 8 is not involved.
A bundle is for a view of many files, so that they cross the stream once and are
kept by content. It is an extension of this document; it is not MCP Apps.

### 8.1 Support

A host that supports bundles lists `bundle` in `features` of its `welcome`. A
program sends a manifest only to such a host.

### 8.2 The manifest

`open.bundle` has an `entry` (a path), a list of `files`, and an OPTIONAL `meta`.
Each file has:

- `path`: relative, `/`-separated, 1 to 255 bytes, every segment one or more of
  `A–Z a–z 0–9 . _ ~ -` and not `.` or `..`; no two files share a path;
- `sha256`: the SHA-256 of the file's bytes, 64 lowercase hex digits;
- `size`: the length of the file in bytes;
- `type`: a media type `type/subtype`, lowercase, without parameters.

`entry` MUST be the path of a file. `meta` is the `_meta.ui` object of MCP Apps §
UI Resource Format (`csp`, `permissions`, `prefersBorder`; `domain` is
host-dependent and a host MAY ignore it) and is where the host takes them from when
it uses the bundle. A path is the only way a view names a file; a host MUST
serve nothing that is not in the manifest.

Limits: 1024 files, 8 MiB (8 388 608 bytes) per file, 64 MiB (67 108 864 bytes)
for the sum of the sizes. These are constants of the protocol, like the message
limits; a host MAY enforce lower ones and then refuses with `limit`.

```frame fixtures/to-host/open-bundle.frame
<ESC>_pencere;open;id=2;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"web","tool":{"name":"list_containers","arguments":{}},"bundle":{"entry":"index.html","files":[{"path":"index.html","sha256":"aeba6c19a632d2153a9e336a88b385d438c474a996d5be1408f6cd2ca6898e54","size":132,"type":"text/html"},{"path":"app.js","sha256":"bcc81d22a717681517bc99bbad72438377e02c19583fae67522475fd8b3ca62c","size":67,"type":"text/javascript"},{"path":"style.css","sha256":"72735d03f2b7e5a9ddb8ebe1c5fde2da88aa74292ba7ac608490a94f0ebbac0e","size":61,"type":"text/css"}],"meta":{"csp":{"connectDomains":["https://api.example.com"]},"prefersBorder":false}}}<ESC>\
```

### 8.3 What is missing

For an `open` it accepts that has a manifest, the host answers with a `need`
message whose `re` is the open's id and whose `missing` lists the digests it
does not hold, each once, in the order of their first appearance in `files`.
An empty list means everything is held. Then, without waiting, it goes on with
Section 9 if nothing is missing, or waits for the files.

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

### 8.7 With and without `resources/read`

If it uses a bundle, the host MUST NOT fetch the view's document with
`resources/read`; it takes the document and its `meta` from the bundle. A program
that offers a bundle SHOULD also serve a single self-contained document at the
same `ui://` resource, so that MCP Apps hosts that know nothing of bundles — the
same tool started over stdio by a chat client — can show the view (Section 13).

### 8.8 WebAssembly views

A view need not be a web page. A view of the kind `wasm` (Section 5.5) is a **WebAssembly
view**: the entry of its bundle, whose media type is `application/wasm`, is a WebAssembly
component (the component model's binary format) that the host runs in a sandbox of its own and
draws itself, in the terminal's font and theme, without a web view. What the component is given and
may ask of the host is an interface the host and the component share, and it is not part of this
document. This document defines how such a view is opened, how it and its program exchange
messages, and when it ends.

A host that shows WebAssembly views offers `wasm` in the `views` of its `welcome` (Section 5.2),
when the program listed it and the host takes bundles, because the component arrives as a
bundle. A program sends such an `open` only to a host that offered the kind; a host that did not
refuses one with `unsupported` (Section 6.4). A bundle that is not a component, sent with the
kind `wasm`, and a component sent with the kind `web`, are refused with `manifest`: the host does not
decide the kind from the entry, and it does not guess.

```frame fixtures/to-program/welcome-wasm.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle","dismiss"],"views":["wasm","web"]}<ESC>\
```

The `open` of a WebAssembly view differs from that of a web view in three ways:

- there is no `tool`, and one that is present is not read. There is no MCP session: Sections 9.3
  to 9.6 do not apply;
- `program` and `title` are OPTIONAL strings: what the program calls itself and what it calls the
  view. They take the place of what the host of a web view reads from MCP (`serverInfo` and the
  tool's `title`) and, like those, are text the program wrote, so the host cuts them and leaves out
  control characters and bidirectional overrides before it draws them in its own interface
  (Section 12.4);
- `bundle.meta` is not read. The component reaches nothing that the host does not hand it, so there
  are no domains to ask the person about (Section 12.2). A network capability, if one is added, is
  an extension of this document.

```frame fixtures/to-host/open-wasm.frame
<ESC>_pencere;open;id=2;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"wasm","program":"demopen","title":"Containers","bundle":{"entry":"view.wasm","files":[{"path":"view.wasm","sha256":"93a44bbb96c751218e4c00d479e4c14358122a389acca16205b1e4d0dc5f9476","size":8,"type":"application/wasm"}]}}<ESC>\
```

The host asks for the files as for any bundle (Sections 8.3 and 8.4) and starts the view when it
holds them. It MAY refuse an entry over a size of its own choosing with `limit` (Section 8.2), so
a program must be ready for that.

**Messages.** A `msg` of a WebAssembly view is one JSON object, written as Section 4.5 says, and the
host does not read it: it hands the program's messages to the component and the component's to the
program, whole and in order. What the objects mean is between the program and the component. The
host does this only while the view is open: a program's `msg` before the view runs, and any `msg`
that is not an object, is dropped (Section 4.7), and **once the view has closed the host writes
nothing the component says**, whatever ended it (the program's `bye` or `dismiss`, the person, the
view's own failure, the end of the program), because whatever reads the tty next would take it for
typed input (Section 4.8). A program therefore does not expect a message of the component after the
`closed` that ends its view, or after its own `bye` or `dismiss`.

**Ending.** Section 10.2 runs without its first step: the component is not told and not waited for,
it is stopped. A component that fails (it traps, goes over a limit the host holds it to, or hands
the host something it cannot use) is closed as if the person had closed it, except that the
program is told `closed` with `reason` `error` and `code` `view`. In every other way the lifecycle
is that of Section 10: the program's `bye` and `dismiss`, the person closing the view and a restart
(Section 10.7) end the view in the same way, and ⌃C goes to the program (Section 11.3).

## 9. MCP bridge

This section is what a view of the kind `web` (Section 5.5) is. It is the bridge to MCP and,
for now, the only thing `web` means: the `msg` objects of a WebAssembly view (Section 8.8) are not
messages of a web view, and a web view does not exchange them with its program.

### 9.1 Roles

The host is an MCP client of the program and an MCP Apps host of the view. The
program is an ordinary MCP server; it does not know the view exists except
through the tool calls it receives.

### 9.2 `msg`

A `msg` payload is exactly one JSON-RPC 2.0 message as MCP defines it, a request,
a response or a notification, with the escaping of Section 4.5. It is the unit
MCP's stdio transport carries, minus the newline that ends it there. Both sides
send them in both directions as MCP requires. The MCP session lasts from the
first `initialize` until a `closed` or `bye` ends the view.

```frame fixtures/to-program/msg-initialize.frame
<ESC>_pencere;msg;id=3;{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{"extensions":{"io.modelcontextprotocol/ui":{"mimeTypes":["text/html;profile=mcp-app"]}}},"clientInfo":{"name":"pencere-test","version":"0.0.0"}}}<ESC>\
```

### 9.3 Start-up

When the open is accepted and the bundle, if there is one, is complete, the host:

1. sends `initialize` as an MCP client, advertising the MCP Apps extension (MCP
   Apps § Client (Host) Capabilities), and `notifications/initialized` after the
   answer;
2. sends `tools/list`, finds the tool of `open.tool`, and takes its
   `_meta.ui.resourceUri`; a tool without it, or one that does not exist, ends the open
   with `closed` `error`, `mcp`;
3. gets the view's document: from the bundle, or with `resources/read` of the
   resource (and its `_meta.ui`);
4. starts the view and follows MCP Apps § Lifecycle: the view sends `ui/initialize`;
   the host answers; the view sends `ui/notifications/initialized`; the host sends
   `ui/notifications/tool-input` with `open.tool.arguments`, calls the tool on the
   program with them, and sends `ui/notifications/tool-result` with the result (or
   `ui/notifications/tool-cancelled`, MCP Apps § MCP Apps Specific Messages). The
   `toolInfo` of the host context is the tool as listed.

The program's answer to the `initialize` of step 1:

```frame fixtures/to-host/msg.frame
<ESC>_pencere;msg;id=4;{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"containers","version":"0.0.0"}}}<ESC>\
```

### 9.4 What the view may reach

The host forwards these requests of a view to the program:

- `tools/call` and `resources/read`, the two MCP Apps § Standard MCP Messages lists. A
  tool the view calls MUST have `app` in its visibility, which the host knows from
  `tools/list` (MCP Apps § Resource Discovery → Visibility);
- `tools/list`, `resources/list`, `resources/templates/list` and `prompts/list`, as they
  are. A view lists what the program has to learn what a notification of Section 9.6
  announced. MCP Apps lets a host forward any message of a view whose method does not
  begin with `ui/` (MCP Apps § Sandbox proxy); these four are the ones its reference
  implementation forwards, and the draft of MCP Apps adds `tools/list` to the standard
  messages.

`ping` and `notifications/message` the host answers or records itself and need not
forward. Messages whose method begins with `ui/` belong to the host and are never
forwarded, and nor is any other request of the view. The view's request ids and the
host's request ids to the program are two spaces; the host gives each forwarded request
an id of its own and maps the answer back. A request the program does not answer in time,
or an answer after the view is gone, fails the way MCP defines.

A view gives up a request it forwarded with `notifications/cancelled`, naming the
request by its own id (MCP § Cancellation). The host sends the program
`notifications/cancelled` with the id it gave the request, and the view's `reason`
when it is text, forgets the request, and drops the program's answer if one still
comes; the view is not answered. A cancellation that names no request of the view in
flight is ignored.

### 9.5 No conversation

A terminal has no conversation with a model. The host answers `ui/message` and
`ui/update-model-context` with a JSON-RPC error, code −32000, the denial MCP Apps §
MCP Apps Specific Messages gives each. Its host
capabilities in the `ui/initialize` answer name what it does: `serverTools` and
`serverResources` (it proxies; each with `listChanged` `true` when Section 9.6 passes
that list's changes on), `openLinks` if it opens links, `logging` if it
takes log messages, and `sandbox` with what the person approved (Section 12).

### 9.6 What reaches the view from the program

The program's notifications go to the host, its MCP client. The host passes these on to
the view, as the program wrote them, and drops every other:

- `notifications/tools/list_changed`, `notifications/resources/list_changed` and
  `notifications/prompts/list_changed`, when the program declared `listChanged` `true` for
  its `tools`, `resources` or `prompts` in the capabilities of its `initialize` result
  (MCP § Lifecycle). A capability of another shape, or a `listChanged` that is not `true`,
  declares nothing, and does not stop the open. The host says which of tools and resources
  it passes on with `hostCapabilities.serverTools.listChanged` and
  `serverResources.listChanged` (MCP Apps § Host Capabilities); MCP Apps names no host
  capability for prompts. A view that hears of a change lists again (Section 9.4).

  Before it tells the view that the tools changed, the host reads `tools/list` again,
  every page, as in Section 9.3, so that a tool the view learns of is one the host lets
  it call (Section 9.4). One reading is out at a time: the program's word that its tools
  changed while one is out makes the host read once more after it, and the view is told
  once, after the last. A reading that fails, or that the program does not answer in
  time, leaves the host with the tools it knew, and the view is still told.

- `notifications/progress` (MCP § Progress), when its `progressToken` is the token a
  request of the view still in flight carried in `params._meta.progressToken`: the host
  forwards a request's parameters as they are, so the program sees the view's token. MCP
  Apps does not name this; it is an addition of this document, and a view in a host that
  does not pass progress on is never called with it and loses nothing else.

A view that has not sent `ui/notifications/initialized` is sent no change of a list: it
lists what it needs once it has. The host still reads the tools again.

## 10. Lifecycle

### 10.1 States

A pane is idle, or offered (a token is outstanding), or opening (an open
accepted, files or MCP start-up pending), or open (the view running), or closing.
A `hello` at any time gets its answer and a new token without affecting a view
that is opening, open or closing; an `open` while one is opening or open is `busy`.

### 10.2 Closing

Whatever ends a view, the host:

1. sends `ui/resource-teardown` to the view with a reason and waits for its answer
   for a time of its choosing (MCP Apps § Lifecycle → Cleanup);
2. tears the view down and stops serving its bundle;
3. if the program is still there and the cause was not the program's own end,
   writes `closed` into its input (Sections 10.5, 10.6 and 10.8);
4. forgets the view's token: the one that opened it (Section 6.1), and no other.

A request the view had in flight fails; if the call of `open.tool` was in flight
the view gets `ui/notifications/tool-cancelled`. The reason given to the view is
the host's to choose.

A `hello` that arrives while the view is closing is answered at once (Section 10.1),
and it shows that a program is reading the tty. The token it gets is not the view's:
it is still good after step 4, and the end of the view does not make that program
ended, whatever the cause (Section 4.8). The answer is written ahead of the `closed` of step 3.

### 10.3 The program ends

A host learns that the program ended from either of two sources and acts on
whichever comes first:

- the program's `bye` message, `reason` `exit`, `signal` or `error`, carrying the
  `token` the view was opened with; a program library SHOULD send it on a normal exit and
  from its SIGINT and SIGTERM handlers; it is best effort. A host ignores a `bye` that has
  no `token`, or another token than the one that opened the view that is opening, open or
  closing, and a `bye` that arrives when no view is: whoever can write the stream can send
  one, only the program that read its token can send this one (Section 14), and a program
  that opened no view has no `bye` to send, the host's own observation (below) covers it;
- the host's own observation where it has one: shell integration reports the
  command finished (OSC 133 `D`), the tty's foreground process group changed, or the
  stream ended.

Either way the host closes the view as Section 10.2 says; its step 3 does not
apply, because the program is gone.

A program that is killed with SIGKILL, or crashes, sends nothing, so a host that
has no observation would leave the view open for ever; every host MUST let the
person close a view regardless. After the program has ended the host does not
write frames (Section 4.8).

```frame fixtures/to-host/bye.frame
<ESC>_pencere;bye;id=6;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","reason":"signal"}<ESC>\
```

### 10.4 Interrupt

⌃C goes to the program. With `ISIG` on, the tty turns the character into SIGINT
for the foreground process group. A program library that opened a view SHOULD
handle SIGINT by sending `bye` with reason `signal`, restoring the terminal
(Section 4.9), leaving the alternate screen (Section 6.5) and exiting.

### 10.5 The person closes the view

A host MAY offer the person a way to close a view that the program has not ended
(a shortcut or a control of its own). It runs Section 10.2 with reason `user` and
tells the program with `closed` `reason` `user`. A program library ends its use
of the view then, which a tool author's `serve` call observes.

```frame fixtures/to-program/closed-user.frame
<ESC>_pencere;closed;id=5;{"reason":"user"}<ESC>\
```

### 10.6 The view asks to close

A view that wants the program to end calls a tool, as any MCP App does when it
needs something done; the program ends and Section 10.3 follows. A host MAY also
accept the notification `ui/notifications/request-teardown`, which exists in the
draft of MCP Apps and not in revision 2026-01-26; it then runs Section 10.2
and tells the program with `closed` `reason` `view`. A view that must work with
hosts that do not accept it does not rely on it.

### 10.7 Restart

Messages are not part of the screen. Nothing that restores a screen — a
reconnection of the stream behind the pane, the hand-over of a pane from one host
process to another — brings them back, and a host that comes up after such a
break knows of a running view only from its own record.

A host in that position MAY restart the view: it writes an unsolicited `welcome`
(no `re`, `restart` `true`, a new token, and in `views` every kind the host will open, in the
host's own order, since no `hello` asks) into the pane's input. A program that has
a view open, or believes it has, treats it as a request to start again: it drops
its MCP session, and sends `open` with the new token, which runs Sections 6 to 9
again with a fresh MCP session, the bundle served from the store, and its state
rebuilt from the tool results as at any first start. A program with no view open
ignores it. A host SHOULD restart only for a pane where its own record says a view was
open, SHOULD apply the checks of Section 6.2, and gives up if no `open` arrives
in a time of its choosing.

```frame fixtures/to-program/welcome-restart.frame
<ESC>_pencere;welcome;id=2;{"version":1,"token":"Zk3mP8rD5uYb1Xe6HqS0vg","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle","dismiss"],"views":["web"],"restart":true}<ESC>\
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
no view is opening or open is ignored (Section 14); so is one that arrives while the view
is already closing, because the `closed` that ends it is on its way. Otherwise the host runs
Section 10.2 with reason `program` and writes `closed` with `reason` `program` and `re` the
id of the `dismiss`. A view that is still opening is given up with no teardown to send:
step 1 of Section 10.2 is for a view that runs.

```frame fixtures/to-program/closed-program.frame
<ESC>_pencere;closed;id=5,re=6;{"reason":"program"}<ESC>\
```

The pane is idle then, with the alternate screen as the program left it. A program that
wants a view again ends the MCP session of the one that closed and sends `hello` and,
with the token it gets, `open` (Sections 5 and 6) as for any first view, with the same
bundle or another. The host's store (Section 8.5) holds what it verified before, so
`need` lists only what is not there. A program SHOULD wait for the `closed` before it
sends `hello`: until the `closed` is written the view is still closing, and the answer
to the `hello` would come ahead of it (Section 10.2).

## 11. Terminal context

### 11.1 Host context

The host fills the host context of the `ui/initialize` answer (MCP Apps § Host
Context in `McpUiInitializeResult`) from what it knows of the person's machine,
not the program's: `locale` and `timeZone` are the person's, `platform` is
`desktop`, `userAgent` names the host. It sends changes with
`ui/notifications/host-context-changed`. `containerDimensions` and
`displayMode` are as in Section 7.2.

### 11.2 Theme

`theme` is `dark` when the pane's default background is darker than its default
foreground (by relative luminance), `light` otherwise. The host SHOULD give the
view the colours and font of the terminal, in `styles.variables`, from these
sources (MCP Apps § Theming defines the variables):

| Terminal                                        | MCP Apps variables                                                                    |
| ----------------------------------------------- | ------------------------------------------------------------------------------------- |
| default background                              | `--color-background-primary`                                                          |
| default foreground                              | `--color-text-primary`                                                                |
| the dimmed foreground the terminal uses         | `--color-text-secondary`                                                              |
| the background a step toward the foreground     | `--color-background-secondary`                                                        |
| the colour of rules and borders                 | `--color-border-primary`                                                              |
| the accent colour                               | `--color-ring-primary`                                                                |
| success, error, warning and information colours | the `success`, `danger`, `warning` and `info` variants of text, background and border |
| the monospace font                              | `--font-mono`                                                                         |

A host that provides one colour variable SHOULD provide the whole family it
belongs to, because MCP Apps § Theming warns that views fall back on their own
values for the rest. Values are plain colours; a terminal theme has one mode, so
`light-dark()` is not needed.

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

The host builds the policy from the declared domains as MCP Apps § Security
Implications → Content Security Policy Enforcement requires, with the
restrictive default of MCP Apps § UI Resource Format when none are declared. A
host that is a web page follows MCP Apps § Sandbox proxy.

### 12.2 Network

By default the view reaches nothing outside the host's own bridge to its program. Where `csp` names
domains (any of `connectDomains`, `resourceDomains`, `frameDomains`,
`baseUriDomains`) or `permissions` asks for a browser capability, the host MUST ask
the person before the view can use them, MUST NOT allow what the person did not
approve, and reports what it allowed in `hostCapabilities.sandbox`. MCP Apps
asks a host only to warn; here the person is asked, because a view in a terminal can
reach the person's own network.

The question names the domains and says where the program runs (this machine, or
the host name of the other one). A host SHOULD say when a domain is a loopback
or private address and MAY refuse such names. How long an answer is
remembered is the host's policy; it SHOULD NOT carry from one program to
another.

### 12.3 Files and navigation

The view has no access to files and does not navigate to another document.
`ui/open-link` (MCP Apps § MCP Apps Specific Messages) is the only way out: the
host MUST accept only `http:` and `https:` URLs, SHOULD ask the person, and opens
them in the person's default browser on the person's machine, never on the machine
the program runs on.

### 12.4 Origin, limits, indication

A bundled view's origin is unique to it (Section 8.6). A host SHOULD bound the
CPU and memory of a view (MCP Apps § Security Implications → Other risks) and
SHOULD keep an indication outside the view's area that a view is shown and
where its program runs, because a view that covers the pane could imitate the
terminal.

## 13. Fallback

- **stdin is not a tty**, and the program is not a filter (below). The program is
  started by something that is not a terminal — an MCP client, a chat application —
  and is an ordinary MCP server on stdio: it sends no frame of this protocol, and its
  tool and `ui://` resource work in any MCP Apps host.
- **A tty, and no host.** Section 5.3. The program sends no further frame and
  continues as a program with no view: it offers its own text interface, or says
  what it needs and exits. It leaves the terminal as it found it.
- **A host, and no kind of view in common.** A `welcome` whose `views` is empty
  (Section 5.2). The program falls back at once, with no wait, and sends no `open`.
- **The host refused or ended the open.** `closed` before the view ran gets the same
  treatment.

A tool of a pencere program SHOULD always return a useful `content` (MCP Apps §
Client\<\>Server Capability Negotiation → Graceful Degradation), so that its
results read without a view.

**A program in a pipeline.** A filter, a program whose stdin and stdout carry a
pipeline's data (`… | tool --select | mail`), speaks this protocol on its controlling
terminal (`/dev/tty`) and not on stdin and stdout: it puts that tty in the modes of
Section 4.9, takes the alternate screen and writes its frames there, reads the host's
replies there, and keeps stdin and stdout for the data. A program MUST decide this
explicitly, with an option or a flag of its own, and MUST NOT infer it from stdin and
stdout: a pipeline and an MCP client that started the program look alike from inside,
both with pipes for stdin and stdout, and a program that took one for the other would
write frames into a terminal an MCP client is using, whose host would answer into that
client's input. A filter is never an MCP server on stdio. One with no controlling
terminal, or whose terminal has no host, has no view and goes on as the second case
above says.

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

**The view.** A view is untrusted code from a program the person may not control. It runs in
the sandbox of Section 12, without network unless the person allows it, without files,
without navigation, in an origin of its own, with its own requests audited as MCP Apps §
Security Implications requires. A view can still say anything on its own surface; the
host's indication (Section 12.4) is how the person tells the terminal from a view.

**Icons.** A program's icon is drawn in the terminal's chrome by the terminal's own image
decoder. A host fetches none, so an icon cannot reveal who opened a program; it takes only
images whose bytes it has checked and bounded; and it draws none in a question it asks in its
own name (Section 7.3).

**A WebAssembly view.** The component is untrusted code from a program the person may not
control, and what it is given is the host's to decide (Section 8.8): the host runs it in a sandbox
that gives it nothing it was not handed, and bounds its memory and time (Section 12.4). The
messages between it and its program are a stream anyone can write to while the view is open — the
rule of the first paragraph applies, since a process that can write the pane's output can send a
`msg` — so a component treats what its program says as untrusted, and a program what its component
says. What the component says is never written to the program once the view is over.

**Resource use.** Frame, message and bundle limits (Sections 4.6, 8.2) bound what a
receiver buffers; hosts SHOULD also limit how often one pane may ask (answers to `hello`,
`open`).

## 15. Conformance

A **host** conforms if it does everything this document asks of a host with MUST and
MUST NOT, in Sections 4 to 12; a **program library** conforms if it does everything it asks
of a program. SHOULD is for what an implementation has a reason to skip.

There are two levels. **Core** is Sections 4 to 7, 9, 10 without 10.7 and 10.8, 11 without 11.2, 12 and 13. **Full**
adds bundles (Section 8), restart (Section 10.7), the program closing its view (Section 10.8) and
the theme of Section 11.2. A host below Full does not list `bundle` or `dismiss` in its `features`
for the part it lacks; a program below Full does not send a manifest or a `dismiss` and
ignores an unsolicited `welcome`. WebAssembly views (Section 8.8) are on top of Full and optional:
a host that shows them offers `wasm` in its `views`, and a program sends one only to a host that did.

The fixtures of this repository are the conformance suite:

- every `decode` entry of `fixtures/manifest.json` is fed, as a stream in the entry's
  direction and split at every byte boundary, to a receiver, which delivers exactly the
  entry's messages and reports exactly its errors. A receiver that reads raw bytes (a program
  library) takes every entry. A receiver that is handed APC bodies by a terminal parser (a host)
  takes the entries whose scope is `body`, fed with the body of each frame in the file; the
  entries of scope `stream` exercise the framing such a parser has already done;
- every entry marked `canonical` is what a writer produces when asked to send the entry's
  messages;
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
interface Hello {
  versions: number[]; // positive integers
  views: string[]; // the kinds of view the program can show, in its order of preference; not empty (Section 5.5)
}

interface Welcome {
  version?: number; // present with token, absent with supported
  token?: string;
  host: { name: string; version: string };
  features?: string[]; // "bundle" (Section 8.1), "dismiss" (Section 10.8)
  views?: string[]; // with token: the program's list without the kinds the host will not open, in the program's order; may be empty (Section 5.2)
  restart?: boolean; // true on an unsolicited welcome
  supported?: number[]; // present when no version is shared
}

interface Open {
  token: string;
  view: string; // one of the welcome's views (Section 6.3)
  tool?: { name: string; arguments?: Record<string, unknown> }; // required for "web", not read for "wasm"
  program?: string; // "wasm" only: what the program calls itself (Section 8.8)
  title?: string; // "wasm" only: what the program calls the view
  bundle?: Bundle; // required for "wasm", whose entry has the type application/wasm; for "web" the entry is text/html
}

interface Bundle {
  entry: string;
  files: { path: string; sha256: string; size: number; type: string }[];
  meta?: UIResourceMeta; // MCP Apps § UI Resource Format
}

interface Need {
  missing: string[]; // sha256 digests
}

interface Closed {
  reason: "refused" | "error" | "user" | "view" | "program";
  code?: string;
  message?: string; // for people; never parsed
}

interface Bye {
  token: string; // the token the view was opened with (Section 10.3)
  reason?: "exit" | "signal" | "error";
}

interface Dismiss {
  token: string; // the token the view was opened with (Section 10.8)
}
```

Codes of `Closed`: for `refused`, those of Section 6.4; for `error`, `bundle` (Section 8.4),
`mcp` (Section 9.3), `view` (Section 8.8), `timeout` and `internal`. A receiver treats a code it does not know as
the plain reason.

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

A program starts in a terminal that supports this document. The host's store holds `index.html` and
`style.css` of the program's bundle already, and lacks `app.js`. Frames are as in the fixtures; ids
number each side's messages.

1. The program puts the tty in non-canonical mode, writes `hello` and the fence:

```frame fixtures/to-host/hello-fence.frame
<ESC>_pencere;hello;id=1;{"versions":[1],"views":["web"]}<ESC>\
<ESC>[c
```

2. The host answers before it answers `CSI c`:

```frame fixtures/to-program/welcome.frame
<ESC>_pencere;welcome;id=1,re=1;{"version":1,"token":"q7Vh2sB0xJm9kLw4TzN1cA","host":{"name":"pencere-test","version":"0.0.0"},"features":["bundle","dismiss"],"views":["web"]}<ESC>\
```

3. The program switches to the alternate screen and opens its view with the token and the manifest:

```frame fixtures/to-host/open-bundle.frame
<ESC>_pencere;open;id=2;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","view":"web","tool":{"name":"list_containers","arguments":{}},"bundle":{"entry":"index.html","files":[{"path":"index.html","sha256":"aeba6c19a632d2153a9e336a88b385d438c474a996d5be1408f6cd2ca6898e54","size":132,"type":"text/html"},{"path":"app.js","sha256":"bcc81d22a717681517bc99bbad72438377e02c19583fae67522475fd8b3ca62c","size":67,"type":"text/javascript"},{"path":"style.css","sha256":"72735d03f2b7e5a9ddb8ebe1c5fde2da88aa74292ba7ac608490a94f0ebbac0e","size":61,"type":"text/css"}],"meta":{"csp":{"connectDomains":["https://api.example.com"]},"prefersBorder":false}}}<ESC>\
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

6. The host starts an MCP session with the program:

```frame fixtures/to-program/msg-initialize.frame
<ESC>_pencere;msg;id=3;{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{"extensions":{"io.modelcontextprotocol/ui":{"mimeTypes":["text/html;profile=mcp-app"]}}},"clientInfo":{"name":"pencere-test","version":"0.0.0"}}}<ESC>\
```

7. The program answers; the host lists tools, starts the view, calls `list_containers`, forwards the
   view's calls, and so on, all in `msg` messages. When the person presses ⌃C the program writes
   `bye`, restores the terminal and leaves the alternate screen; the host tears the view down:

```frame fixtures/to-host/msg.frame
<ESC>_pencere;msg;id=4;{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"containers","version":"0.0.0"}}}<ESC>\
```

```frame fixtures/to-host/bye.frame
<ESC>_pencere;bye;id=6;{"token":"q7Vh2sB0xJm9kLw4TzN1cA","reason":"signal"}<ESC>\
```

## Appendix D. References

Normative:

- BCP 14: RFC 2119 and RFC 8174.
- RFC 4648, base64 and base64url. RFC 5234, ABNF. RFC 8259, JSON. RFC 2397, the `data:` URL
  scheme.
- MCP Apps (SEP-1865), revision 2026-01-26:
  <https://github.com/modelcontextprotocol/ext-apps/blob/main/specification/2026-01-26/apps.mdx>

Informative:

- The Model Context Protocol: <https://modelcontextprotocol.io/specification>
- ECMA-48, Control Functions for Coded Character Sets: APC, ST, CSI.
- The kitty graphics protocol, whose APC framing, `m=` chunking and 4096-byte payload chunks this
  protocol follows in spirit: <https://sw.kovidgoyal.net/kitty/graphics-protocol/>
- The 2 second bound of Section 5.3 is the one the terminal mode of gpuix uses for its
  kitty-graphics probe and `CSI c` fence (`packages/gpui-terminal/src/kitty.rs`,
  `PROBE_TIMEOUT` and `FENCE_TIMEOUT`).
- The Model Context Protocol, revision 2025-11-25, the revision the host speaks as an MCP
  client (Section 9.3). Its sections named here: Lifecycle (the capabilities of `initialize`),
  Cancellation and Progress (Sections 9.4 and 9.6), Tools and Basic → General fields →
  `icons` (Section 7.3):
  <https://modelcontextprotocol.io/specification/2025-11-25>
- MCP Apps draft, `ui/notifications/request-teardown` (Section 10.6):
  <https://github.com/modelcontextprotocol/ext-apps/blob/main/specification/draft/apps.mdx>
- Kernel sources of Appendix B: `bsd/kern/tty_dev.c`, `bsd/kern/tty.c`, `bsd/sys/tty.h` and
  `bsd/sys/syslimits.h` in <https://github.com/apple-oss-distributions/xnu>; `drivers/tty/n_tty.c`,
  `drivers/tty/pty.c`, `drivers/tty/tty_buffer.c` and `include/uapi/linux/limits.h` in
  <https://github.com/torvalds/linux>.
