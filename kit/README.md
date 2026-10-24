# The pencere kit

What a terminal needs to show the views of kind `wasm` and `lua` of
[pencere](../spec/draft/pencere.md) in its own font and theme, and what a page is written with.
A view of these kinds is a WebAssembly component or a Lua script in a sandbox that reaches nothing
but the interface here; the terminal lays out what it hands out and draws it.

| Directory            | Crate                             | What it is                                                                                                   |
| -------------------- | --------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| [`view`](view)       | `pencere-view`                    | The page engine: elements, style, layout (flexbox and grid), the draw list. Depends on the layout crate only |
| [`sandbox`](sandbox) | `pencere-sandbox`                 | Runs a component or a script under limits and hands out pages. The one door to the WebAssembly runtime       |
| [`wit`](wit)         |                                   | The interface a component speaks: `pencere:view`                                                             |
| [`luau`](luau)       |                                   | The sources of the interpreter for scripts: Luau, the bridge to the interface and the Lua page API           |
| [`guest`](guest)     | `pencere-page` and the test pages | Another workspace, built for `wasm32`: the SDK a page module is written with, and modules to try it on       |

The page language and the sandbox know nothing of a window, a font or a renderer. A terminal
measures text for the engine (`pencere_view::Measure`), resolves the colours of a page in its own
theme (a role in the theme, a value in the form that goes with its brightness), keeps text readable
against the grounds the draw list names under it, and turns the draw list into what it draws.

## Images

A page shows pictures with `image(src, fit)` (in Lua, `pencere.image(src):fit(…):tint(…):opacity(…)`):
`src` is the path of a file of the view's own bundle, as its manifest names it, and nothing else is
reachable (no network, no file system). PNG, JPEG and SVG; at most 4096 × 4096 pixels an image and
64 MiB of decoded pixels a view; `contain` (all of it, centred) or `cover` (the box filled, the
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

## Versions

The kit's version is the one a view names in its `open` (specification Sections 5.4 and 5.5):
`wasm` `1.0` is the interface `pencere:view@1.0.0`, `lua` `1.0` is the page API whose
`pencere.version` is `1`.

- An **addition** that no existing view can notice is a new minor version (`1.1`): a new function in
  the component interface, a new element or a new colour role in the page API.
- A **change to something a view already uses** is a new major version (`2.0`): a new kind of element
  (`variant content` of the interface), a new role, a new field of a record, a function that means
  something else. A terminal that adopts it opens the previous major version as well; an older view
  is refused in words that name both sides.
- For `lua` the interpreter is the terminal's, so a new element or a new role is an addition for a
  script. What breaks a script is a function of the page API that means something else.

## Building

```sh
make check        # the gate: needs no toolchain for WebAssembly
make guest        # the page modules (containers, broken, chatty, tints) and the example program
make check-heavy  # what runs a real module and the interpreter; builds `guest` first
make luau         # rebuilds the interpreter in Docker (pinned wasi-sdk and Luau); `make check` fails when it is due
```

`make guest` and `make check-heavy` build with rustup's `stable` toolchain and its
`wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown --toolchain stable`).
