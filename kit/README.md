# The pencere kit

What a terminal needs to show the views of kind `wasm` and `lua` of
[pencere](../spec/draft/pencere.md) in its own font and theme, and what a page is written with.
A view of these kinds is a WebAssembly component or a Lua script in a sandbox that reaches nothing
but the interface here; the terminal lays out what it hands out and draws it.

| Directory                            | Crate                             | What it is                                                                                                       |
| ------------------------------------ | --------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| [`view`](view)                       | `pencere-view`                    | The page engine: elements, style, layout (flexbox and grid), the draw list. Depends on the layout crate only     |
| [`editor`](editor)                   | `pencere-editor`                  | The editor of text a host puts in an editor's room: document, carets, undo, input method, layout and drawing     |
| [`sandbox`](sandbox)                 | `pencere-sandbox`                 | Runs a component or a script under limits and hands out pages. The one door to the WebAssembly runtime           |
| [`drive`](drive)                     | `pencere-drive`                   | Drives a page as every host does: what the person does in, its frames, editors, scrolling, replies and wakes out |
| [`wit`](wit)                         |                                   | The interface a component speaks: `pencere:view`                                                                 |
| [`SPEC.md`](SPEC.md)                 |                                   | What the kit's views are written against: the Lua page API, the colours and the sandbox floors                   |
| [`luau`](luau)                       |                                   | The sources of the interpreter for scripts: Luau, the bridge to the interface and the Lua page API               |
| [`sandbox/library`](sandbox/library) |                                   | The kit's library: modules of Luau a page requires as `@pencere/<name>`                                          |
| [`guest`](guest)                     | `pencere-page` and the test pages | Another workspace, built for `wasm32`: the SDK a page module is written with, and modules to try it on           |

The page language, the sandbox and the drive know nothing of a window, a font or a renderer. The
drive does what the specification (Section 4) asks of every host but what only a host can do; a
terminal measures text for the engine (`pencere_view::Measure`), resolves the colours of a page in its own
theme (a role in the theme, a value in the form that goes with its brightness), keeps text readable
against the grounds the draw list names under it, and turns the draw list into what it draws.

## Where a part of the kit goes

The kit is made of four layers. Every part of it, and every part added to it later, takes its place
by the rule of a layer, not by where its first user would like it to be.

| Layer                                         | What it is                                                                                                                                                                          | What goes there                                                                                                                   |
| --------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| 0. The host                                   | Rust: the wire, the engine (the layout and the draw list), the editor of text (`pencere-editor`), the sandbox, the drive (`pencere-drive`)                                          | What only a host can do. A change to it is a change to the interface ([`wit/view.wit`](wit/view.wit)) and to [`SPEC.md`](SPEC.md) |
| 1. The prelude (`pencere.*`, in every page)   | The thin binding of what the host can do, and the loop of a page: the elements, `Element:editor`, the events and the replies, `pencere.time`, `pencere.json`, what a page remembers | Only what needs the bridge's inner places (`pencere._*`), or must be there before the page starts                                 |
| 2. The library (`require("@pencere/<name>")`) | Pure Luau, written on the public API of the prelude alone, and loaded when a page asks for it ([`sandbox/library`](sandbox/library))                                                | What an app could have written itself on the page API. It comes with the kit, at the kit's version, with tests of its own         |
| 3. The app (`require("./…")`)                 | The app's own scripts                                                                                                                                                               | The app's policy                                                                                                                  |

- **What a layer uses is below it, never above.** A module of the library may require another (a
  part that stands by an editor requires `@pencere/keys`), and never an app's script. The prelude
  requires no module of the library, and the host knows none.
- **A part goes in the layer furthest from the host that can hold it.** The library comes before the
  prelude, and the prelude before the host. A part goes lower only for the reason the lower layer's
  rule gives: into the prelude only what cannot be written on its public API, into the host only
  what a page cannot do.
- **The editor of text is the host's, and stays so.** Its engine is Rust, and its element is the
  prelude's (`Element:editor`). What is around it is the library's: the languages it reads
  (`@pencere/languages`), the reader of a line at a time that colours it (`@pencere/syntax`), and
  the parts of a page that stand by it.
- **A part goes into the library when** it is built of the kit's own ideas (elements, an editor and
  its language, the roles of the theme, events), its interface carries no app's policy, and the kit
  can keep it right better than each app can on its own. How many apps use it today only breaks a
  tie. What it costs is weighed alike for every part: the interface it adds, what it asks of the
  prelude, the interface and the interpreter, and its size.

## The library

A page gets its host's library, at the kit's version, as it gets the prelude. The sandbox gives a
script that asks for a module its source (`pencere_sandbox::library`), from the crate itself. A
module is mended without a new interpreter, and a page that asks for none pays nothing for it. The
library does not count toward the size of a page's scripts (4 MiB); the crate is not built when its
modules come to more than 1 MiB together. An error at a line of a module says the module and its
line (`@pencere/syntax:12: …`); one a module raises for what its caller gave it says the caller's
line. The language server of Luau reads the requires where a `.luaurc` gives the alias `pencere`
the directory [`sandbox/library`](sandbox/library).

| Module               | What it is                                                                                                                                                                                                                                               |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `@pencere/syntax`    | A language that colours an editor as it changes, read a line at a time from a stream, as CodeMirror's `StreamLanguage` reads one                                                                                                                         |
| `@pencere/languages` | The languages an editor reads, as CodeMirror's `language-data` knows them: a description of each (its names, how a file in it is told), and the language (its colours for `@pencere/syntax` and what an editor is told of it) built when a page loads it |
| `@pencere/fuzzy`     | What a person types found in texts as fzf and VS Code find it: scored and ordered, a list at once or a slice a frame, the matched characters as parts of a rich text                                                                                     |
| `@pencere/keys`      | Keys a page writes (`"cmd+shift+k"`) as a Mac writes them (`⇧⌘K`) or another platform (`Ctrl+Shift+K`), as key caps, and matched against a key event as the host matches a command's                                                                     |
| `@pencere/markdown`  | Markdown (CommonMark and GitHub's extensions) read into a tree and drawn as elements: a document kept for a text that changes is read again only where it changed, and a long one is drawn a view at a time                                              |

[`SPEC.md`](SPEC.md), Section 2.6, says what each module does.

## Images

A page shows pictures with `image(src, fit)` (in Lua, `pencere.image(src):fit(…):tint(…):opacity(…)`):
`src` is the path of a file of the view's own bundle, as its manifest names it, or a file beside
the bundle: `sha256:` and the digest of one its program attached, `file:` and the handle of one the
person gave the page; nothing else is reachable (no network, no file system). PNG, JPEG and SVG; at most 4096 × 4096 pixels an image and
64 MiB of pixels a view holds (a PNG or a JPEG is kept at the size it is drawn at, not the file's); `contain` (all of it, centred) or `cover` (the box filled, the
rest cut off); `tint` paints the image's opacity in a colour, so that an icon of one colour follows
the theme. The size is the element's, and where it says a side only the image's proportions give the
other; where it says neither the image's own size in pixels, a point to a pixel.

**Give the size in points and send twice the pixels**: a 64-pixel PNG shown at 32 points is sharp
on a display with two pixels to the point, and shown at its own size (64 points) it is blurry.
An SVG is drawn at the size it comes to on the display and needs nothing of the kind.

The sandbox crate does the decoding, so that every terminal takes the same formats and the same
limits. A terminal gives it the files (`Assets`, a path to bytes: its store of the bundle), makes
one `Images` for a view, and uses three of its calls:

- `Measure::image` of the engine is `Images::size`: the size of an image in pixels, `None` while it
  is not known. The first ask starts the decoding on a thread of its own; the layout goes on
  without it.
- `Paint::Image` of the draw list is drawn with `Images::bitmap(src, picture.full())`: premultiplied
  RGBA with an `id` to key the texture by. An SVG is drawn at the size asked, again for every other.
- The `wake` given to `Images::new` is called when something arrived. The terminal then calls
  `Images::poll`: `relayout` says a size is known and the page is to be laid out again (once),
  `redraw` that pixels are there, and `diagnostics` why an image was refused. Nothing is polled
  and nothing woken at rest.

A terminal that hosts files beside the bundle (`lua` `2.1`, `wasm` `3.0`) answers `Assets::read` for
a `sha256:` name from the files the program attached to the view (`Session::attached` of the host
crate, or `AttachedFiles::get` from a thread that has no session: `Session::attached_files` gives
one table for the life of the session, which the view's end empties) and for a `file:` name from its own table of the handles it made, never from a path, and
calls `Images::forget` with the names the program let go of (`Action::ForgetFiles`). The panels a
page asks for are the drive's `Host::panel`, which the terminal hands the session
(`Session::module_pick`, `module_save`) before it shows one.

## Versions

The kit's version is the one a view names in its `open` (specification Sections 5.4 and 5.5):
`wasm` `3.0` is the interface `pencere:view@3.0.0`, `lua` `2.1` (and `2.0`) is the page API whose
`pencere.version` is `2`. (What `2.0` added to `1.0`, from overlays to editors of text, their
languages and a page's timers, and what `2.1` and `3.0` add, the files beside the bundle, are listed
in [`SPEC.md`](SPEC.md), Section 1. The sandbox here opens `wasm` `3.0` alone.)

- An **addition** that no existing view can notice is a new minor version (`1.1`): a new function in
  the component interface, a new element or a new colour role in the page API.
- A **change to something a view already uses** is a new major version (`2.0`): a new kind of element
  (`variant content` of the interface), a new role, a new field of a record, a function that means
  something else. A terminal that adopts it opens the previous major version as well; an older view
  is refused in words that name both sides.
- For `lua` the interpreter is the terminal's, so a new element or a new role is an addition for a
  script. What breaks a script is a function of the page API that means something else.
- The library is a part of the page API. A new module, or a new function of a module, is an
  addition; a function of a module that means something else is a change.

## Building

```sh
make check        # the gate: needs no toolchain for WebAssembly
make guest        # the page modules (containers, broken, chatty, tints) and the example program
make check-heavy  # what runs a real module and the interpreter; builds `guest` first
make luau         # rebuilds the interpreter in Docker (pinned wasi-sdk and Luau); `make check` fails when it is due
```

`make guest` and `make check-heavy` build with rustup's `stable` toolchain and its
`wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown --toolchain stable`).
