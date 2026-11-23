# The Lua page API

This topic says what a script of a page can use, and which of its functions the host calls.

Version `0.1` of the kind `lua` ([the protocol](../../spec/draft/pencere.md), Sections 5.5 and 9.8).

The kit's document is normative for what the protocol leaves to the kit, with the prelude [`luau/prelude`](../luau/prelude) for the page API of `lua` views.[^topic] The key words are those of Section 2 of the protocol. A statement in it that the prelude contradicts is a mistake in the document.

[^topic]: This is one topic of [the kit's document](../SPEC.md).

## What the kit is

The kit is what a view of the kind `lua` is written against. It does not draw. The view says what the page is. The host lays it out, resolves its colours in the theme of the pane and draws it.

A host that keeps the rules of the kit's document and the sandbox floors shows the same page as any other.

## What a script can use

A script is Luau. Besides the language it can use:

- the base functions, `string`, `table`, `math` and `utf8`;
- `print` and `warn`, which say a line to the program's log ([`print` and `warn`](program.md#print-and-warn));
- `require`, for the other scripts of its bundle and the kit's library ([The other scripts of the bundle](program.md#the-other-scripts-of-the-bundle) and [The kit's library](library.md));
- `pencere`, the table the prelude fills.

Beyond that, it has:

- no files of the system's;
- no network;
- no clock but the time `render` and a wake hand it ([Events](events.md#what-a-page-is-told)) and the time of day its host says ([Layout and scrolling](layout.md));
- no randomness.[^print]

[^print]: `print` is not the base library's, which has nowhere to write to. It is the prelude's, and a page has no standard output.

## What the prelude keeps to itself

Nothing of the prelude's own is a name a script can reach. What its parts share among themselves, and the functions of the host that it wraps, are not fields of `pencere` or of any table a script is given. So a script, and a module of the library, can neither read them nor change them: `pencere.host`, `pencere._keymaps` and the like are nil.

The tables `pencere` holds are these:

- `pencere.json`, `pencere.layout` and `pencere.time`;
- the classes of elements, spans, paths and timers with their metatables;
- `pencere.json.null`.

They are read only, as `pencere` itself is. Assigning to one, or adding a field, is an error of the script at its line, whatever the script is and whichever script ran before it. The tables a script makes with them are its own.

Of an object the page API or a module of the library gives, only the fields and methods the kit's document names are the contract.[^objects] Any other field it holds is not, and may change in every version.

[^objects]: An object is an element, a timer, a syntax, a language, a document or the context of a component.

## When a call goes wrong

A call of the page API that is given what it refuses fails the page. It refuses, for example, the wrong kind of value, a name it has not, and a box that cannot be what the call needs it to be.

The error is at the line of the script that made the call, `script.lua:12: …`. It is at the line of the module of the library when one made it, and not at a line of the prelude.

What the page API finds wrong in what a function of the page returned has no line of the script to name. It says its words as they are.[^returned]

A mistake that is found only once the tree is made says its words alone. It is in a style value or a colour that is not one. No line of the script is there to name by then.

[^returned]: A `render` that gives no element is one, and so is a reply's `wake`.

## What a script returns

A script **returns the page**, a table of the functions the host calls.

- `render(size, now, reduce_motion) -> element[, animating[, wake_at]]` is called when the host draws.
  - `size` is `{ w =, h = }` in points, `now` is seconds on the host's clock, and `reduce_motion` is the person's setting.
  - `animating` asks for the next frame at once. A page at rest costs no frames.
  - `wake_at` is a time on the clock of `now`. It asks a page at rest to be rendered once more then.
- `settle() -> boolean` (optional) is called when the page was laid out. It brings back what depends on the layout. `true` when something changed. A timer set here is asked for: the kit has the page rendered again.
- `handle(event) -> reply` (optional) is called when the person did something, or the time the page asked to be woken at came. See [Events](events.md#what-a-page-is-told).
- `message(message) -> reply` (optional) is called when the program said something that no `pencere.call` or `pencere.subscribe` of the page was waiting for, and that is not an answer to a request the kit numbered ([The page and its program](program.md)). `message` is the decoded JSON.
- `room() -> reply` (optional) is called when the program has read enough that a message `pencere.send` answered `false` to may be sent again.
