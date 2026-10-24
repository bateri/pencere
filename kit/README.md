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
measures text for the engine (`pencere_view::Measure`), resolves the colour roles of a page in its
own theme, and turns the draw list into what it draws.

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
make guest        # the page modules (containers, broken, chatty) and the example program
make check-heavy  # what runs a real module and the interpreter; builds `guest` first
make luau         # rebuilds the interpreter in Docker (pinned wasi-sdk and Luau); `make check` fails when it is due
```

`make guest` and `make check-heavy` build with rustup's `stable` toolchain and its
`wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown --toolchain stable`).
