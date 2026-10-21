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
- a command that runs an MCP server written in any language as a pencere program, so its view
  opens in the terminal pane: `pencere run -- python server.py`, and `pencere dev` for the same
  while the files change, which shows the view again each time ([`@pencere/cli`](cli));
- a generator for a new tool, with a React or Vue view built by Vite, so that a first view is
  one command away: `npm create pencere@latest my-tool` ([`create-pencere`](create));
- a Rust library for terminals that host views, doing no I/O (crate `pencere`,
  [`host/rust`](host/rust));
- a stand-in terminal that plays the host, for trying a tool and for tests
  ([`host/fake`](host/fake));
- an example tool to try it all with ([`examples/containers`](examples/containers));
- the same kind of tool in Python, with the official MCP SDK and no pencere library, to show that a
  server in any language gets its view in the pane from `pencere run`
  ([`examples/python`](examples/python)).

[bateri](https://github.com/bateri/bateri) will be the first host. Nothing is on npm or
crates.io yet; how a package is released is in [RELEASING.md](RELEASING.md).

## License

Apache-2.0 — see [LICENSE](LICENSE). The specification, the libraries and the tools are
all under it, so any terminal or tool, open or closed, can implement the protocol or embed
this code. Copyright belongs to the pencere Authors ([NOTICE](NOTICE), [AUTHORS](AUTHORS)):
everyone who contributes keeps the copyright of their contribution.
