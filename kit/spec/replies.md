# Replies

A reply's **`fill`** and its **`keyboard = { field = name }`** may name a field that the very page the
reply comes with brings: a reply that opens a field (a find bar, a palette, a dialog) fills it and
sends the keyboard to it in the same reply, as an element is focused when it comes. A field is the
host's only once a frame has laid it out ([What a host does for a kit view](host.md)), so what a reply asks of a field the host does
not have yet waits for the next frame that lays the page out, and is done once the host has that
frame's fields: in the order it was asked (a reply's texts, then its keyboard), and what is asked
after it waits behind it, though it names a field the host has. A name that frame still does not
have is passed over, as any name that is no field of the page is: its text is dropped and the
keyboard goes to the page. A frame that is not drawn (the view is hidden) leaves it waiting.

A reply's **`link`** asks the host to open an address in the person's browser, by the rule of a web
page's `link` (protocol, Sections 9.7 and 9.8): only an `http` or `https` address, every byte of it
printable ASCII without the space, at most 8192 bytes. The host SHOULD ask the person first (as
Section 9.7 of the protocol says), opens it on
the person's machine, and never tells the page what came of it. Any other address is **dropped**, as a
page's is: the page is not closed for it and not told, and the rest of the reply stands. A `link` that
is not a string is an error of the script. The link is the host's, not a message to the program. It
is not a span's `link(id)` either, which names a part of a text for the pointer: a page opens what
such a part stands for by answering its press with `{ link = url }`.

The **files the person gives** a page (protocol, Section 8.9) come as
`files`, each with the `handle` the host made of it (32 lowercase hex digits; none for a `kind` of
`"folder"`, which is not read), its `name`, its `size` in bytes, its media `type` (the host's guess)
and, only where the host knows the program reads the same files as the person, its `path`. The name
is the person's and the type a guess: words, never a path to join to another. A page draws such a
file as `image("file:" .. handle)` and passes the handle on to its program, which reads the file by
it; the handle is nothing once the view has ended.

A reply's **`pick`** (`true`, or `{ multiple = true }`) asks the host for the Open panel, and its
**`save`** (the name to propose, or `{ name =, type = }`) for the Save panel: what the person chooses
comes as `files` with `source` `"pick"`, or as `saving`. A reply asks for one panel; where replies are
merged (`pencere.merge`, the timers) the last one's stands. **A panel is shown only for a reply to
something the person did**: a press or its release (a button acts on the release), a key, a pick
in a menu, a command. Asked for in a reply to
anything else (the pointer moving, a wake, a message of the program, the room to send), it is passed
over, so that a page cannot open panel after panel by itself; and the host shows one panel at a time,
dropping one asked for while another is up. **A menu is shown by the same rule**: a menu asked for in
a reply to a message of the program, a wake, the room to send or the pointer moving is passed over, so
that a program cannot put a menu, and with it a line that pastes, under a person who did not ask for one;
so is a menu asked for in the reply to the editor's account of what it did (`edit`, `selection`,
`viewport`, `focus`), which the page is told and nobody asked, without a word to the page: it asks from
the press or the key. **A Paste line is on only in the menu a context press opened**: a press of the right button (a Mac's ⌃
with the main button too, where the drive tells it the page as the right button's, below), in an editor,
in a gutter or elsewhere. In a menu that a key (Shift F10, which is a command a key did), a press of
the main button (the second click of a double click too), a release, a command of the host's list
or the pick of another menu asked for, the line stays in its place, so that the page's count of its
lines and the number of a pick are what they were, and is off: nothing picks it. A person who typed a
letter and a space, or clicked twice, has not asked to paste; the cost is that the editor's menu from the
keyboard has Paste greyed, and ⌘V still pastes.

A reply's **`wake`** asks the host to wake the page that many seconds from now (0 or more; 0 for the
next frame). Once the time has come the host tells `handle` `{ type = "wake", now = }`, `now` the
time of the frame it is woken at on `render`'s clock, and the page answers as it answers any event:
the work a page does over time (a long document read a piece at a time, a save once the person stops
typing, a search that waits for the typing to stop) goes on with no event of the person's, and what
it finds reaches the editors, the keyboard or the pasteboard with the wake's reply (not a menu or a panel: those are the person's). Of the times a
page asks, in its replies and (for a timer set as it is drawn) its frames, the earliest stands, so
that what one part of a page asks is not put off by another; a wake that came is gone, and a page
that wants another asks again. The host wakes a page at most once a frame: a page that asks
`wake = 0` each time it is woken is woken once a frame, as one that is `animating` is drawn. A
`wake` that is not a number of seconds, 0 or more, is an error of the script. It is not `render`'s
`wake_at`, which draws a page at rest again at a time: a wake calls `handle`, and its reply can ask
what a frame cannot. In Lua the kit's timers ask it for the page ([Layout and scrolling](layout.md)).

A reply's **`text_size`** asks the host to draw this view that many points larger than its own text
(a whole number: 0 is as large as the host's text, below 0 smaller). It is the size of the view, not of
the host: the host puts it into the size it draws the view at, within what it can draw, and leaves its
own text as it was, so that nothing of the person's terminal stays larger when the view is gone. The
page says it when it wants it, and again to change it (a code editor made larger by a person who reads
small print, kept in its settings and said when it opens), and is not told what came of it. It is not
the person's act: any reply may carry it, a wake's or a program's message's as well as a press's. A host
that has no such size ignores it. A `text_size` that is not a whole number is an error of the script. It
is not the element's `text_size` (the size in points of that element's text).

**Replies put together.** A page made of parts that answer apart (its timers, a module of the
library, its own `handle`) answers with one reply: `pencere.merge(first, second, …)` is that reply,
the replies (each nil, `true`, `false` or a table) put together in the order given and none of them
changed. It asks everything they ask: the asks of an editor or a browser, of each in the order of
the replies; the texts of `fill`, by name, the later's where two name one field; a frame where any
wants one (`true`, `redraw`) and the page closed where any asks; of what says one thing (`keyboard`,
`copy`, `cursor`, `menu`, `link`, `text_size`) the last that says it; and the earliest `wake`. Of replies none of
which is a table, it is `true` where any is `true`, nil where every one is nil, and `false`
otherwise. This is how the kit puts together what its timers and the page answer ([Layout and scrolling](layout.md)).
