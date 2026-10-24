# pencere

The host side of [pencere](https://github.com/bateri/pencere): what a terminal needs to let a
program in one of its panes show a view in that pane, whatever the view is — a WebAssembly
component or a Lua script the terminal runs and draws itself, or a web page in a sandboxed web
view.

The crate does no I/O. A terminal feeds it the bodies of the APC sequences it parses from a
pane's output and writes the replies the crate builds into that pane's input; it supplies the
clock, the bundle store, a source of random bytes and the views themselves. The crate holds the
rules — frames and their limits, the kinds of view and the versions of their interfaces the
terminal opens, the single-use token, the checks an `open` goes through, bundles, carrying the six
messages of a program and its view between the two without reading them, the bridge of a web page
(`context`, `close`, `link`), the network consent, closing and restart — so every terminal that
embeds it follows the same ones.

Every byte the crate reads comes from a stream anyone can write to: it never panics on it and
never buffers more than a limit of the specification allows.

The wire is version 1 of the
[specification](https://github.com/bateri/pencere/blob/main/spec/draft/pencere.md), still a
draft: a `0.x` release may change along with it. [CHANGELOG.md](CHANGELOG.md) says what each
release changed.

## License

Apache-2.0 — see [LICENSE](LICENSE).
