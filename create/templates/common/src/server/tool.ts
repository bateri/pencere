/**
 * The tool: an ordinary MCP server whose `show_counter` tool has a view.
 *
 * Nothing in this file knows about pencere. The same server, started by a chat
 * client over stdio, shows the same view there; `main.ts` is the only place that
 * asks a terminal to show it.
 */

import { McpServer } from "@modelcontextprotocol/server";
import {
  RESOURCE_MIME_TYPE,
  registerAppResource,
  registerAppTool,
} from "@modelcontextprotocol/ext-apps/server";
import { readFile } from "node:fs/promises";

export const VIEW_URI = "ui://{{name}}/view";

/**
 * The view as the files `vite build` writes: what a terminal that shows pencere
 * views is given, so that they cross its stream once and are kept by content.
 */
export const VIEW_DIRECTORY = new URL("../view/", import.meta.url);

/**
 * The view as one document, which `vite build --mode single` writes: what every
 * other MCP Apps host reads. It is read when it is asked for, so that a server
 * that is only shown in a terminal does not need it.
 */
const VIEW_DOCUMENT = new URL("../single/index.html", import.meta.url);

export function createServer(): McpServer {
  const server = new McpServer({ name: "{{name}}", version: "0.1.0" });
  let count = 0;

  const counted = () => ({
    // For the model, and for a host that shows no view.
    content: [{ type: "text" as const, text: `The count is ${count}.` }],
    // For the view.
    structuredContent: { count },
  });

  registerAppTool(
    server,
    "show_counter",
    {
      // What a terminal writes above the view, in place of `show_counter`.
      title: "{{title}}",
      description: "Show the counter",
      _meta: { ui: { resourceUri: VIEW_URI } },
    },
    async () => counted(),
  );

  registerAppTool(
    server,
    "add_one",
    {
      title: "Add one",
      description: "Add one to the counter",
      // Only the view calls it: the model is not offered a tool that is a button.
      _meta: { ui: { visibility: ["app"] } },
    },
    async () => {
      count += 1;
      return counted();
    },
  );

  registerAppResource(server, "{{title}}", VIEW_URI, {}, async (uri) => ({
    contents: [
      { uri: uri.href, mimeType: RESOURCE_MIME_TYPE, text: await readFile(VIEW_DOCUMENT, "utf8") },
    ],
  }));
  return server;
}
