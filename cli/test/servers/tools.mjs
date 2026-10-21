// An MCP server on stdio to ask which of its tools name a view. The mode says which
// tools it has:
//
//   node tools.mjs one      a tool without a view and one with
//   node tools.mjs many     two tools with a view and one without
//   node tools.mjs legacy   one tool, with the flat `ui/resourceUri` of the first drafts
//   node tools.mjs none     tools, none with a view
//   node tools.mjs empty    no tools
import { McpServer } from "@modelcontextprotocol/server";
import { StdioServerTransport } from "@modelcontextprotocol/server/stdio";

const [mode = "one"] = process.argv.slice(2);
const server = new McpServer({ name: "stand-in", version: "0.0.0" });
const answer = async () => ({ content: [{ type: "text", text: "ok" }] });
const nested = { ui: { resourceUri: "ui://stand-in/view" } };

const tools = {
  one: [["plain"], ["show", nested]],
  many: [["plain"], ["first", nested], ["second", nested]],
  legacy: [["show", { "ui/resourceUri": "ui://stand-in/view" }]],
  none: [["plain"], ["other"]],
  empty: [],
}[mode];
for (const [name, meta] of tools ?? []) {
  server.registerTool(name, { description: name, ...(meta ? { _meta: meta } : {}) }, answer);
}
await server.connect(new StdioServerTransport());
