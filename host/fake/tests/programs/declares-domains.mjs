// A plain stdio MCP server, one that knows nothing of pencere, whose view declares a
// domain it wants to reach: the person is asked before the view starts, for the
// end-to-end test of what the question says. The tool and the program have names of
// their own, so the question can be seen to carry them.
//
//   node declares-domains.mjs
import {
  RESOURCE_MIME_TYPE,
  registerAppResource,
  registerAppTool,
} from "@modelcontextprotocol/ext-apps/server";
import { McpServer } from "@modelcontextprotocol/server";
import { StdioServerTransport } from "@modelcontextprotocol/server/stdio";

const server = new McpServer({ name: "declares-domains", version: "0.0.0", title: "Domain Desk" });
registerAppTool(
  server,
  "show",
  {
    title: "Show it",
    description: "Show",
    _meta: { ui: { resourceUri: "ui://declares-domains/view.html" } },
  },
  async () => ({ content: [{ type: "text", text: "shown" }] }),
);
registerAppResource(server, "View", "ui://declares-domains/view.html", {}, async (uri) => ({
  contents: [
    {
      uri: uri.href,
      mimeType: RESOURCE_MIME_TYPE,
      text: "<!doctype html><p>view",
      _meta: { ui: { csp: { connectDomains: ["https://api.example.com"] } } },
    },
  ],
}));
await server.connect(new StdioServerTransport());
