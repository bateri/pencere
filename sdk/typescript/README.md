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

A program that wants to close its view and go on — to show its own interface for a while,
or, while a tool is developed, to show the view again with the files that changed — gives
`serve` a `ViewControl` and calls it while `serve` runs. A host that lists `dismiss` in its
`welcome` takes it; with another the view is left as it is and the call says `unsupported`.

```ts
const control = new ViewControl();
const served = serve(server, { tool: "list_containers", bundle, control });
// when the view's files changed:
await control.reopen({ bundle: await bundleFromDirectory(directory) });
// or to leave the view and show the program's own interface:
await control.close(); // `serve` then resolves with `{ kind: "closed", by: "program" }`
```

The terminal's modes are set with `stty`, not with `process.stdin.setRawMode`: raw mode
also turns off ISIG, and ⌃C would arrive as a byte instead of a SIGINT. A system with no
`stty`, such as Windows, has no host as far as this library is concerned.

A bundle is for a view of many files. The server should also serve one self-contained
document at the view's `ui://` resource, which is what an MCP Apps host that knows nothing
of pencere reads.

The terminal names the view as any MCP client would, from the server's own metadata: the
tool's `title` and the server's `title` (else its `name`), and their `icons`. A terminal
draws an icon only from the bytes it is given and never fetches one, so give it as a PNG,
JPEG or WebP in a `data:` URI, small and not animated; an `https:` icon is shown by other
clients and left out in the pane.

```ts
const server = new McpServer({
  name: "containers",
  title: "Containers",
  version: "1.0.0",
  icons: [{ src: "data:image/png;base64,iVBORw0KGgo…", mimeType: "image/png" }],
});
```

## Versions

This release speaks version 1 of the pencere wire, which is still a draft: until the
specification publishes a dated version, a `0.x` release of this package may change the wire
along with it. The package follows semantic versioning; [CHANGELOG.md](CHANGELOG.md) says what
each release changed.

## License

Apache-2.0 — see [LICENSE](LICENSE).
