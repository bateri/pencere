# @pencere/sdk

Show an MCP App view in the terminal pane your MCP server runs in.

A program running in a terminal pane — on your machine, or on a server reached
over plain ssh — can show a web interface in that pane, the way a full-screen
program takes the screen and gives it back when it ends. The interface is an MCP
App view, the program is an MCP server, and the two meet through the terminal's
own byte stream. This package is the program's side of that, as the
[specification](https://github.com/bateri/pencere/blob/main/spec/draft/pencere.md) defines it.

```ts
import { McpServer } from "@modelcontextprotocol/server";
import { registerAppResource, registerAppTool } from "@modelcontextprotocol/ext-apps/server";
import { bundleFromDirectory, serve } from "@pencere/sdk";

const server = new McpServer({ name: "containers", version: "1.0.0" });
registerAppTool(
  server,
  "list_containers",
  { _meta: { ui: { resourceUri: "ui://containers/view" } } },
  list,
);
registerAppResource(server, "Containers", "ui://containers/view", {}, readView);

const result = await serve(server, {
  tool: "list_containers",
  bundle: await bundleFromDirectory(new URL("./view/", import.meta.url)),
  fallback: () => console.log(table()),
});
```

`serve` does what the situation asks:

- **stdin is not a terminal.** The server is an ordinary MCP server on stdio, and its
  tool and `ui://` resource work in any MCP Apps host. No frame of this protocol is sent.
- **A terminal that hosts pencere.** The terminal's modes are set, the host is asked, the
  alternate screen is taken, and the view is opened with the token the host gave. The files
  of the bundle the host does not already hold are sent, the server answers the view's
  calls, and when the view closes the terminal is given back. ⌃C ends the program as it
  always does: `bye` is sent, the screen and the modes are restored, and the signal goes on.
- **A terminal that does not.** `serve` returns `{ kind: "no-view" }` after the `fallback`,
  if there is one, has run: the program offers its own text interface.

The terminal's modes are set with `stty`, not with `process.stdin.setRawMode`: raw mode
also turns off ISIG, and ⌃C would arrive as a byte instead of a SIGINT. A system with no
`stty`, such as Windows, has no host as far as this library is concerned.

A bundle is for a view of many files. The server should also serve one self-contained
document at the view's `ui://` resource, which is what an MCP Apps host that knows nothing
of pencere reads.

## Versions

This release speaks version 1 of the pencere wire, which is still a draft: until the
specification publishes a dated version, a `0.x` release of this package may change the wire
along with it. The package follows semantic versioning; [CHANGELOG.md](CHANGELOG.md) says what
each release changed.

## License

GPL-3.0-or-later — see [LICENSE](LICENSE). Anyone may implement the protocol; the license covers
this package's code.
