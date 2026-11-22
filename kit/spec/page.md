# The Lua page API

Version `0.1` of the kind `lua` ([the protocol](../../spec/draft/pencere.md), Sections 5.5 and 9.8).
This document is normative for what the protocol leaves to the kit: with the prelude
[`luau/prelude`](../luau/prelude) for the page API of `lua` views. The key words are those of Section 2
of the protocol. A statement here that the prelude contradicts is a mistake in this document.

The kit is what a view of the kind `lua` is written against. It does not draw: the view says what
the page is, and the host lays it out, resolves its colours in the theme of the pane and draws it. A
host that keeps the rules of this document and the sandbox floors shows the same page as any other.

A script is Luau. What it can use besides the language is the base functions, `string`, `table`,
`math`, `utf8`, `print` and `warn` (which say a line to the program's log: [`print` and `warn`](program.md#print-and-warn)), `require`
(the other scripts of its bundle and the kit's library: [The other scripts of the bundle](program.md#the-other-scripts-of-the-bundle) and [The kit's library](library.md)), and `pencere`, the
table the prelude fills. It has no files of the system's, no network, no clock but the time `render`
and a wake hand it ([Events](events.md#what-a-page-is-told)) and the time of day its host says ([Layout and scrolling](layout.md)), and no randomness.
(`print` is not the base library's, which has nowhere to write to: it is the prelude's, and a page has
no standard output.)

Nothing of the prelude's own is a name a script can reach: what its parts share among themselves,
and the functions of the host that it wraps, are not fields of `pencere` or of any table a script
is given, so that a script, and a module of the library, can neither read them nor change them
(`pencere.host`, `pencere._keymaps` and the like are nil). The tables `pencere` holds (`pencere.json`,
`pencere.layout`, `pencere.time`, the classes of elements, spans, paths and timers with their
metatables, `pencere.json.null`) are read only, as `pencere` itself is: assigning to one, or adding a
field, is an error of the script at its line, whatever the script is and whichever script ran before
it. The tables a script makes with them are its own. Of an object the page API or a module of the
library gives (an element, a timer, a syntax, a language, a document, the context of a component) only
the fields and methods this document names are the contract: any other field it happens to hold is
not, and may change in every version.

A call of the page API that is given what it refuses (the wrong kind of value, a name it has not, a
box that cannot be what the call needs it to be) fails the page with the error at the line of the
script that made the call, `script.lua:12: …` (at the line of the module of the library when one made
it), and not at a line of the prelude. What the page API finds wrong in what a function of the page
returned (a `render` that gives no element, a reply's `wake`) has no line of the script to name, and
says its words as they are. A mistake that is found only once the tree is made, in a style value or a
colour that is not one, says its words alone: no line of the script is there to name by then.

A script **returns the page**, a table of the functions the host calls:

| Function                                                              | Called when                                                                                                                                                                                                                                                                                      |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `render(size, now, reduce_motion) -> element[, animating[, wake_at]]` | The host draws. `size` is `{ w =, h = }` in points, `now` seconds on the host's clock, `reduce_motion` the person's setting. `animating` asks for the next frame at once; a page at rest costs no frames. `wake_at` (a time on `now`'s clock) asks a page at rest to be rendered once more then. |
| `settle() -> boolean` (optional)                                      | The page was laid out: bring back what depends on the layout. `true` when something changed. (A timer set here is asked for: the kit has the page rendered again.)                                                                                                                               |
| `handle(event) -> reply` (optional)                                   | The person did something, or the time the page asked to be woken at came. See [Events](events.md#what-a-page-is-told)                                                                                                                                                                            |
| `message(message) -> reply` (optional)                                | The program said something that no `pencere.call` or `pencere.subscribe` of the page was waiting for, and that is not an answer to a request the kit numbered ([The page and its program](program.md)). `message` is the decoded JSON.                                                           |
| `room() -> reply` (optional)                                          | The program has read enough that a message `pencere.send` answered `false` to may be sent again.                                                                                                                                                                                                 |
