// An MCP server on stdio written by hand, for what `McpServer` will not do: pages of tools, a
// tool list that never ends, errors with text of the server's choosing. The mode says which:
//
//   node raw.mjs paged      three pages of tools, the one with a view on the last
//   node raw.mjs endless    a tool list with another page after every page
//   node raw.mjs foreign    one tool whose view is an address that is not `ui://`
//   node raw.mjs hostile    two tools with a view whose names carry terminal commands
//   node raw.mjs failing    `tools/list` fails with a message that carries a terminal command
import { createInterface } from "node:readline";

const [mode = "paged"] = process.argv.slice(2);
const ui = (uri) => ({ ui: { resourceUri: uri } });
const tool = (name, meta) => ({
  name,
  inputSchema: { type: "object" },
  ...(meta ? { _meta: meta } : {}),
});

/** The page of tools asked for, by its cursor, and the cursor of the next one. */
function page(cursor) {
  const n = cursor === undefined ? 1 : Number(cursor);
  switch (mode) {
    case "paged":
      return {
        tools: [n === 3 ? tool("show", ui("ui://raw/view")) : tool(`plain-${n}`)],
        ...(n < 3 ? { nextCursor: String(n + 1) } : {}),
      };
    case "endless":
      return { tools: [tool(`plain-${n}`)], nextCursor: String(n + 1) };
    case "foreign":
      return { tools: [tool("show", ui("https://example.com/view"))] };
    case "hostile":
      return {
        tools: [
          tool("first\u001b[2J", ui("ui://raw/view")),
          tool("second\u001b]0;pwned\u0007", ui("ui://raw/view")),
        ],
      };
    default:
      return { tools: [] };
  }
}

const send = (message) =>
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", ...message })}\n`);

createInterface({ input: process.stdin }).on("line", (line) => {
  const message = JSON.parse(line);
  if (message.id === undefined) return;
  if (message.method === "initialize") {
    send({
      id: message.id,
      result: {
        protocolVersion: message.params.protocolVersion,
        capabilities: { tools: {} },
        serverInfo: { name: "raw", version: "0.0.0" },
      },
    });
  } else if (message.method === "tools/list" && mode === "failing") {
    send({ id: message.id, error: { code: -32603, message: "no tools \u001b[2Jfor you" } });
  } else if (message.method === "tools/list") {
    send({ id: message.id, result: page(message.params?.cursor) });
  } else {
    send({ id: message.id, error: { code: -32601, message: "method not found" } });
  }
});
