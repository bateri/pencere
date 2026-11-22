# The pencere kit

What a terminal needs to show the views of kind `lua` of [pencere](../spec/draft/pencere.md) in its
own font and theme, and what a page is written with. A view of this kind is a Lua script that the
kit's interpreter runs in a sandbox that reaches nothing but what the page API gives it; the
terminal lays out what it hands out and draws it. The one contract outside the kit is
[`SPEC.md`](SPEC.md).

| Directory                            | Crate             | What it is                                                                                                       |
| ------------------------------------ | ----------------- | ---------------------------------------------------------------------------------------------------------------- |
| [`view`](view)                       | `pencere-view`    | The page engine: elements, style, layout (flexbox and grid), the draw list. Depends on the layout crate only     |
| [`editor`](editor)                   | `pencere-editor`  | The editor of text a host puts in an editor's room: document, carets, undo, input method, layout and drawing     |
| [`sandbox`](sandbox)                 | `pencere-sandbox` | Runs a script under limits and hands out pages. The one door to the WebAssembly runtime the interpreter runs on  |
| [`drive`](drive)                     | `pencere-drive`   | Drives a page as every host does: what the person does in, its frames, editors, scrolling, replies and wakes out |
| [`wit`](wit)                         |                   | The interface between the interpreter and the sandbox that runs it: `pencere:view`, internal to the kit          |
| [`SPEC.md`](SPEC.md)                 |                   | What the kit's views are written against: the Lua page API, the colours, the sandbox floors and the versions     |
| [`luau`](luau)                       |                   | The sources of the interpreter for scripts: Luau, the bridge to the interface and the Lua page API               |
| [`sandbox/library`](sandbox/library) |                   | The kit's library: modules of Luau a page requires as `@pencere/<name>`                                          |
| [`guest`](guest)                     |                   | Another workspace: the tools that make the interpreter's component (`cbindgen`, `componentize`)                  |

The page language, the sandbox and the drive know nothing of a window, a font or a renderer. The
drive does what the specification ([`spec/host.md`](spec/host.md)) asks of every host but what only a host can do; a
terminal measures text for the engine (`pencere_view::Measure`), resolves the colours of a page in its own
theme (a role in the theme, a value in the form that goes with its brightness), keeps text readable
against the grounds the draw list names under it, and turns the draw list into what it draws.

## Where a part of the kit goes

The kit is made of four layers. Every part of it, and every part added to it later, takes its place
by the rule of a layer, not by where its first user would like it to be.

| Layer                                         | What it is                                                                                                                                                                          | What goes there                                                                                                                                                                                                          |
| --------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 0. The host                                   | Rust: the wire, the engine (the layout and the draw list), the editor of text (`pencere-editor`), the sandbox, the drive (`pencere-drive`)                                          | What only a host can do. A change to it is a change to [`SPEC.md`](SPEC.md), the one contract outside the kit; the interface between the host and the interpreter ([`wit/view.wit`](wit/view.wit)) is the kit's own link |
| 1. The prelude (`pencere.*`, in every page)   | The thin binding of what the host can do, and the loop of a page: the elements, `Element:editor`, the events and the replies, `pencere.time`, `pencere.json`, what a page remembers | Only what needs the bridge's inner places (the prelude's own, out of a script's reach), or must be there before the page starts                                                                                          |
| 2. The library (`require("@pencere/<name>")`) | Pure Luau, written on the public API of the prelude alone, and loaded when a page asks for it ([`sandbox/library`](sandbox/library))                                                | What an app could have written itself on the page API. It comes with the kit, at the kit's version, with tests of its own                                                                                                |
| 3. The app (`require("./…")`)                 | The app's own scripts                                                                                                                                                               | The app's policy                                                                                                                                                                                                         |

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

The modules, and what each does, are in [`spec/library.md`](spec/library.md), one section each.

## Images

A page shows pictures with `image(src, fit)` (in Lua, `pencere.image(src):fit(…):tint(…):opacity(…)`;
`:page(n)` is the page of a PDF shown). What a `src` may name, the formats, the limits, and how a
picture is sized, fitted and tinted are the protocol's ([Section 9.8](../spec/draft/pencere.md#98-kit-views), Images).

The sandbox crate does the decoding, so that every terminal takes the same formats and the same
limits. A terminal gives it the files (`Assets`, a path to bytes: its store of the bundle), makes
one `Images` for a view, and uses three of its calls:

- `Measure::image` of the engine is `Images::size`: the size of an image in pixels, `None` while it
  is not known. The first ask starts the decoding on a thread of its own; the layout goes on
  without it. It is asked with the page of the file (`Measure::image(src, page)`: from 1, and 1 for
  a file that has no pages, which is everything but a PDF).
- `Paint::Image` of the draw list is drawn with `Images::picture(&picture)`: premultiplied RGBA with
  an `id` to key the texture by, and `Bitmap::part`, what of the whole image the pixels hold (all of
  it for a PNG, a JPEG or a small SVG; for a large SVG or a page of a PDF the part that is seen,
  `Picture::shown`, and a margin: the picture's rectangle is drawn with `Bitmap::crop_of(&picture)`
  for the texture's crop, which reaches past the pixels only where the clip cuts the rectangle). An
  SVG or a page of a PDF is drawn at the size asked, again for every other, and a large one in steps
  of it. A terminal that draws the pictures of its draw list asks for no other (`Images::bitmap` is
  the whole image at a size).
- The `wake` given to `Images::new` is called when something arrived. The terminal then calls
  `Images::poll`: `relayout` says a size is known and the page is to be laid out again (once),
  `redraw` that pixels are there, and `diagnostics` why an image was refused. Nothing is polled
  and nothing woken at rest.

A terminal that opens `lua` answers `Assets::read` for
a `sha256:` name from the files the program attached to the view (`Session::attached` of the host
crate, or `AttachedFiles::get` from a thread that has no session: `Session::attached_files` gives
one table for the life of the session, which the view's end empties) and for a `file:` name from its own table of the handles it made, never from a path, and
calls `Images::forget` with the names the program let go of (`Action::ForgetFiles`). The panels a
page asks for are the drive's `Host::panel`, which the terminal hands the session
(`Session::module_pick`, `module_save`) before it shows one.

## Versions

The kit's version is the one a view names in its `open`: `lua` `0.1`, the page API whose
`pencere.version` is the string `"0.1"`. The rules the versions follow, in the draft and from the first
general release on, are the protocol's ([Sections 5.4 and 5.5](../spec/draft/pencere.md#54-versions)), and
[`spec/versions.md`](spec/versions.md) says what is the kit's own. The interface `pencere:view` has no
version of the kit's: it is the link between the interpreter and the sandbox, and a change of it is a
new build of the interpreter (`make luau`).

## Building

```sh
make check        # the gate: needs no toolchain for WebAssembly
make check-heavy  # what runs the interpreter against the sandbox, the drive and the examples
make luau         # rebuilds the interpreter in Docker (pinned wasi-sdk and Luau); `make check` fails when it is due
```

`make luau` builds the tools that make the component (`kit/guest`) with rustup's `stable`
toolchain.
