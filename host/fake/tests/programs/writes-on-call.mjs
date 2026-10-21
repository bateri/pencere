// A plain stdio MCP server, one that knows nothing of pencere, for the end-to-end
// tests of `pencere dev`. Its tool has a view, and calling it edits a file: what a
// person's editor does while the view is up, done at a moment the test can name.
//
//   node writes-on-call.mjs <file> <text>
//
// The file is written only when it does not hold the text already. Every view is a
// new server process, so a server that wrote on every call would change the file
// again for each view that opens and `pencere dev` would never be done.
import { readFileSync, writeFileSync } from "node:fs";

import {
  RESOURCE_MIME_TYPE,
  registerAppResource,
  registerAppTool,
} from "@modelcontextprotocol/ext-apps/server";
import { McpServer } from "@modelcontextprotocol/server";
import { StdioServerTransport } from "@modelcontextprotocol/server/stdio";

const [file, text] = process.argv.slice(2);
if (file === undefined || text === undefined) {
  console.error("usage: writes-on-call.mjs <file> <text>");
  process.exit(64);
}

const server = new McpServer({ name: "writes-on-call", version: "0.0.0" });
registerAppTool(
  server,
  "show",
  { description: "Show", _meta: { ui: { resourceUri: "ui://writes-on-call/view.html" } } },
  async () => {
    let held;
    try {
      held = readFileSync(file, "utf8");
    } catch {
      held = undefined;
    }
    if (held !== text) writeFileSync(file, text);
    return { content: [{ type: "text", text: held === text ? "unchanged" : "edited" }] };
  },
);
registerAppResource(server, "View", "ui://writes-on-call/view.html", {}, async (uri) => ({
  contents: [{ uri: uri.href, mimeType: RESOURCE_MIME_TYPE, text: "<!doctype html><p>view" }],
}));
await server.connect(new StdioServerTransport());
