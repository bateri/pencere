# pencere

The terminal side of [MCP Apps](https://github.com/modelcontextprotocol/ext-apps).
A program running in a terminal — on your machine, or on a server over plain
ssh with no GPU — shows a real web interface in its own terminal pane, the way
a full-screen program like vim takes the screen and gives it back when it ends.

- **The view is an MCP App.** HTML and JavaScript (React, Vue, Svelte, anything
  that builds to the web), sandboxed, talking JSON-RPC exactly as the MCP Apps
  extension defines. pencere does not define a second view protocol.
- **pencere adds the part MCP Apps does not have: the terminal.** MCP messages
  ride in the terminal's own stream — no port, no extra connection, nothing to
  set up on the server. The view opens in the program's own pane and closes
  when the program ends. It takes the terminal's theme, leaves the terminal's
  own shortcuts alone, loads its files once and keeps them by content, and a
  program in a terminal that does not support pencere is told so and can fall
  back to text.
- **A pencere tool is an ordinary MCP server.** Started over stdio by an MCP
  Apps host instead of from a terminal, the same tool shows the same view
  there.

The view runs on the machine of the person looking at it, so it is kept in a
sandbox: it reaches nothing but its own program unless the program names a
domain and the person allows it.

## Status

Early: nothing here is stable yet. The repository holds

- the specification and its fixtures, the one truth both libraries are tested from
  ([`spec/draft`](spec/draft/pencere.md));
- a TypeScript SDK for tool authors ([`@pencere/sdk`](sdk/typescript));
- a Rust library for terminals that host views, doing no I/O (crate `pencere`,
  [`host/rust`](host/rust));
- a stand-in terminal that plays the host, for trying a tool and for tests
  ([`host/fake`](host/fake));
- an example tool to try it all with ([`examples/containers`](examples/containers)).

[bateri](https://github.com/bateri/bateri) will be the first host.

## License

GPL-3.0-or-later — see [LICENSE](LICENSE). Anyone may implement the protocol;
the license covers this repository's code.
