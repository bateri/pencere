# pencere

The host side of [pencere](https://github.com/bateri/pencere), the terminal side of
[MCP Apps](https://github.com/modelcontextprotocol/ext-apps): what a terminal needs to let a
program in one of its panes show an MCP App view in that pane.

The crate does no I/O. A terminal feeds it the bodies of the APC sequences it parses from a
pane's output and writes the replies the crate builds into that pane's input; it supplies the
clock, the bundle store, a source of random bytes and the web view. The crate holds the rules —
frames and their limits, the single-use token, the checks an `open` goes through, bundles, the
MCP client toward the program and the MCP Apps host toward the view, closing and restart — so
every terminal that embeds it follows the same ones.

Every byte the crate reads comes from a stream anyone can write to: it never panics on it and
never buffers more than a limit of the specification allows.

The wire is version 1 of the
[specification](https://github.com/bateri/pencere/blob/main/spec/draft/pencere.md), still a
draft: a `0.x` release may change along with it. [CHANGELOG.md](CHANGELOG.md) says what each
release changed.

## License

GPL-3.0-or-later — see [LICENSE](LICENSE). Anyone may implement the protocol; the license covers
this crate's code.
