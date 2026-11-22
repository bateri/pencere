# The page and its program

What the page says to its program is the messages of the protocol (Section 9.2):
`pencere.call(method, params, done, progress)` asks and is answered once, `pencere.subscribe(topic,
params, on_value, on_error)` asks to hear a topic, `pencere.cancel(id)` gives either up (and `print` and
`warn`, below, are calls too), `pencere.json` encodes
and decodes JSON (arrays and objects nested up to 127 levels, the outermost counting as the first: protocol, Section 4.5).
`pencere.json.decode(text)` is the reader the program's own messages are read with, and gives the
value, or nil and why (`a comma or a brace was expected at 12`, the place counted from 1; nil alone for
the text `null`); it reads a message of any length the limits of the protocol ([Section 9.3](../../spec/draft/pencere.md#93-carrying-them)) allow. A member that is `null` holds nothing in what is decoded, as one that is not there does, so that
`if changes.git then` reads both alike; where they mean different things, `pencere.json.has(object,
key)` says whether the object had the key (`null` too) and `pencere.json.is_null(object, key)`
whether it had it as `null`, for every object decoded, those of the program's messages included,
while the page has set nothing at that key. What is decoded is encoded again as it was: its `null`
members, and an empty list as a list. A message the program cannot take yet waits its turn (`pencere.send`, the raw word,
says whether it took the message), and the host holds the page to the limits of [What a host does for a kit view](host.md).

`method` and `topic` are strings, `params` is nil or a table, `done`, `progress`, `on_value` and
`on_error` are nil or functions, and the `id` of `pencere.cancel` is the number a request was
given (a whole number from 1); anything else is a mistake of the script, at its line ([The Lua page API](page.md)). `pencere.call` and
`pencere.subscribe` give back the number of the request, which the kit chooses, from 1 and up.

A subscription the program refuses, or ends with an error (protocol, Section 9.2), calls `on_error`
once with the error, `{ code =, message = }` as a call's; the subscription is then over, and no
value follows. Without an `on_error` it ends without a word. A subscription is answered by an
error and by nothing else: a `response` with a `result` to a `subscribe` is ignored.

What the program says after the page gave a request up (`pencere.cancel`), or after it ended, is
not told to the page: a `response`, `push` or `progress` whose `id` is a whole number from 1 that the kit
has given out and has no request for is dropped, and so is one of a kind that request does not take. Only a
message with another `id` reaches `message`: a page that numbers its own messages (`pencere.send`)
takes its numbers from beyond those the kit gives out (the kit takes the next one for each request,
and for each line of `print` and `warn`), as the protocol has it that no two requests of a page
share one.

## `print` and `warn`

`print(...)` and `warn(...)` are global functions of the page: the script has them, and so has every
script it requires and every module of the kit's library, as it has `pencere`. A script that assigns
a name `print` or `warn` of its own has its own, in its own globals, as it does for any name. They
return nothing.

A page has no screen to print to, no standard output and no files, and the program that shows it
may be on another machine. The line goes to the program, which writes it to its log: not to the
screen, and not to the page. Each call sends one `call` (protocol, Section 9.2) of the reserved
method `pencere.log` with the params `{ text = <the line>, level = "info" | "warn" }`: `"info"` for
`print`, `"warn"` for `warn`. It is an ordinary call of the page's, which the program's library
answers itself, with `{}`, without a handler of the application's; it goes through the same queue as
the page's other messages, in order, and counts toward the messages to the program a call may say
([What a host does for a kit view](host.md)).
A page of any kind may send it (a `web` view's page as well), and nothing in the interface, the
bridge or the host changes for it. The answer is the prelude's: it is never delivered to the
page's `message`.

- **The line** is each argument made text as `tostring` makes it, joined with a tab, so
  `print("a", 1, nil, true)` says `a<TAB>1<TAB>nil<TAB>true`; a line may hold line breaks. A line of
  more than 8192 bytes is cut at that many, never inside a character (the cut goes back to the
  character before it), and ends with `… (N more bytes)`, `N` the bytes that were left out.
- **At most 64 lines are out at once**: waiting to be sent, or sent and not yet answered. A line
  printed while 64 are out is dropped, and counted; when there is room again (for it and for what
  follows), one `warn` line, `N lines of print were dropped`, is sent before the next line, and the
  count starts again. A program that never answers costs the page the 64 lines and nothing more:
  what the page remembers of its lines does not grow.
- **A program that has no `pencere.log`** answers `not-found`; from then on, for as long as the
  page lives, `print` and `warn` say nothing (they are no-ops, and what was waiting to be sent is let
  go). A program whose library serves it need not keep what it is told: a program MAY cut a line or
  leave one out (protocol, Section 9.2).
- **A page that fails** says the last 16 lines that were waiting for the end of its call before it is
  closed, so that a page that printed and then made an error has said what it printed; it says
  nothing else it was going to say (a `pencere.call` made in the call that failed is not made). A
  script that fails in its first run, the one that starts it, says nothing: no page starts.
- **The line is what the page says**, untrusted as everything the page says is: a program writes
  the control characters in it as a visible escape before it writes it to a place a terminal may read
  (protocol, Section 9.2).

Neither `pencere.version` nor the version of `lua` changes for `print` and `warn`: a page that does
not use them cannot tell that they are there.

## The other scripts of the bundle

The entry of the bundle is the script, and it may split itself over the other files of its bundle
whose media type is `text/x-lua` with `require`, by Luau's own rules for requiring by string, so
that the editors and the language server of Luau read the paths as they are meant:

- A path starts with `./` or `../` and is relative to the requiring script's own path in the bundle
  (`require("./screens/logs")` in `app/main.lua` is `app/screens/logs`; the script's path is the
  `path` its manifest names). It has no extension: `name` is the script `name.luau` or `name.lua`,
  or the script `name/init.luau` or `name/init.lua`, and it is one of them, or an error. An `init`
  script is the module of its directory; within it `./` is relative to the directory's parent, and
  `@self/` is the directory itself (`require("@self/child")`).
- The bundle is the whole world, with the kit's library beside it ([The kit's library](library.md)). A path that goes
  out of the root of the bundle with `../`, an absolute path, a path with no `./` or `../`, an alias
  other than `@self` and `@pencere` (there is no configuration file to define one), a file that is
  in the bundle but is not typed `text/x-lua`, and a name the bundle has nothing at are all errors,
  raised at the line of the `require`, that say the path. `require` reads no file of the system.
- A script runs once per page, in a thread with globals of its own like the entry's, when it is
  first required, and what it returns (exactly one value) is what every `require` of it returns.
  A script that requires itself through others is an error that names the chain
  (`require cycle: a.lua -> b.lua -> a.lua`). The entry is not a module: it cannot be required.
- An error in a required script, a syntax error included, says the script's own path and line
  (`screens/logs.lua:12: ...`); the entry's say its path as well.
- The text of the scripts a page loads, the entry's too, is held to the size of a script together
  (the floor of [What a host does for a kit view](host.md): 4 MiB): a `require` past it is an error that says so.
