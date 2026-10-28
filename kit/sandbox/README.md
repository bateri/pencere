# pencere-sandbox

The sandbox of [pencere](https://github.com/bateri/pencere)'s native views: a WebAssembly component
(the interface `pencere:view`) or a Lua script that runs on a thread of its own with no file, no
network, no clock and no environment, held to a memory limit and a time for every call, and that
answers with the page to draw — a tree `pencere-view` lays out and paints in the terminal's own
font and theme.

A script may `require` the other scripts of its bundle (Luau's require-by-string, from the path of
the script that asks, over the files the bundle's manifest types `text/x-lua`) and the modules of
the kit's library (`require("@pencere/syntax")`), and nothing else: `Script` is the entry, its path
in the bundle and the bundle (`Assets`), which is all the door a script has to anything. The
library is Luau written on the page API alone, kept in this crate (`library/`, `library::MODULES`)
and given to every script at the kit's version; it does not count toward what a page's own scripts
may come to, and mending a module of it needs no new interpreter.

A module or a script that traps, goes past a limit or hands out something that is not a page is
closed, and the terminal is told why. The interpreter for scripts (Luau, with the bridge to the
page interface) is a component of its own, kept in this crate as `luau/luau.wasm`; `make luau`
rebuilds it from the sources in `kit/luau` and `make check` fails while it is not.

It also decodes the images of a page (`Images`): the PNG, JPEG and SVG files of the view's bundle, off
the thread that draws, held to the same limits in every terminal that uses it, with a diagnostic for
the image it refuses. An SVG draws its shapes and reads nothing it names. And it draws the paths of
a canvas (`draw_figure`), so that a figure is the same pixels in every terminal.

The interface it implements is `wit/view.wit`, the crate's own copy of the kit's (`kit/wit`), which
`make check` keeps equal.

It depends on the engine, on the wire (`pencere-wire`: the size of a message, the rule of a link a
page asks to open), on the WebAssembly runtime (`wasmtime`), on the image decoders (`png`,
`zune-jpeg`, `resvg`), on regular expressions (`regex`: the rules of an editor's indent, checked as
the editor compiles them) and on nothing of a terminal.
See [`kit/README.md`](../README.md) for the rest.

## License

Apache-2.0 — see [LICENSE](LICENSE). The interpreter carries the code of Luau (MIT), of Lua (MIT),
of LLVM's runtime for WebAssembly and of the C library it is built with; their licences are in
[`licenses`](licenses), a copy of [`kit/luau/licenses`](../luau/licenses) that `make check` keeps equal.
