/**
 * The containers tool: an ordinary MCP server whose `list_containers` tool has
 * a view. The containers are made up, so nothing here needs Docker.
 *
 * Nothing in this file knows about pencere. The same server, started by a chat
 * client over stdio, shows the same view there; `main.ts` is the only place
 * that asks a terminal to show it.
 */

import { McpServer } from "@modelcontextprotocol/server";
import {
  RESOURCE_MIME_TYPE,
  registerAppResource,
  registerAppTool,
} from "@modelcontextprotocol/ext-apps/server";
import { readFile } from "node:fs/promises";
import { z } from "zod";

export const VIEW_URI = "ui://containers/view";

/** Where the view's files are: `index.html`, `style.css` and `app.js`. */
export const VIEW_DIRECTORY = new URL("../view/", import.meta.url);

export interface Container {
  id: string;
  name: string;
  image: string;
  state: "running" | "stopped";
  ports: string;
}

export function sampleContainers(): Container[] {
  return [
    { id: "web", name: "web", image: "nginx:1.27", state: "running", ports: "8080→80" },
    { id: "api", name: "api", image: "node:22-alpine", state: "running", ports: "3000" },
    { id: "db", name: "db", image: "postgres:17", state: "running", ports: "5432" },
    { id: "cache", name: "cache", image: "redis:7", state: "stopped", ports: "" },
    { id: "worker", name: "worker", image: "python:3.13", state: "stopped", ports: "" },
  ];
}

/** The containers as text: what a tool result says where there is no view to show it. */
export function table(containers: readonly Container[]): string {
  const width = (pick: (container: Container) => string) =>
    Math.max(...containers.map((container) => pick(container).length));
  const [name, image] = [width((c) => c.name), width((c) => c.image)];
  return containers
    .map(
      (c) =>
        `${c.state === "running" ? "●" : "○"} ${c.name.padEnd(name)}  ${c.image.padEnd(image)}  ` +
        `${c.state.padEnd(7)}  ${c.ports}`.trimEnd(),
    )
    .join("\n");
}

/**
 * The view as one self-contained document: the stylesheet and the script are put
 * into the page. A host that takes bundles reads the three files instead; one
 * that does not (every MCP Apps host that knows nothing of pencere) reads this
 * from `resources/read`.
 */
export async function inlineDocument(directory: URL = VIEW_DIRECTORY): Promise<string> {
  const read = (name: string) => readFile(new URL(name, directory), "utf8");
  const [page, style, script] = await Promise.all([
    read("index.html"),
    read("style.css"),
    read("app.js"),
  ]);
  const link = /<link rel="stylesheet" href="style\.css"\s*\/?>/;
  const tag = /<script src="app\.js"><\/script>/;
  if (!link.test(page) || !tag.test(page)) {
    throw new Error("index.html no longer links style.css and app.js the way inlineDocument reads");
  }
  return page
    .replace(link, () => `<style>\n${style}</style>`)
    .replace(tag, () => `<script>\n${script}</script>`);
}

/** The server, over `containers`; the tools change them in place. */
export function createServer(containers: Container[] = sampleContainers()): McpServer {
  const server = new McpServer({ name: "containers", version: "1.0.0" });

  const listed = (text: string) => ({
    content: [{ type: "text" as const, text }],
    structuredContent: { containers },
  });

  registerAppTool(
    server,
    "list_containers",
    {
      description: "List the containers and whether each is running",
      _meta: { ui: { resourceUri: VIEW_URI } },
    },
    async () => listed(table(containers)),
  );

  const change = (state: Container["state"], verb: string) => async (args: { id: string }) => {
    const container = containers.find((candidate) => candidate.id === args.id);
    if (container === undefined) {
      return {
        isError: true,
        content: [{ type: "text" as const, text: `There is no container "${args.id}".` }],
      };
    }
    container.state = state;
    return listed(`${verb} ${container.name} (${container.image})`);
  };
  const input = z.object({ id: z.string().describe("The container's id") });
  server.registerTool(
    "stop_container",
    { description: "Stop a container", inputSchema: input },
    change("stopped", "Stopped"),
  );
  server.registerTool(
    "start_container",
    { description: "Start a container", inputSchema: input },
    change("running", "Started"),
  );

  registerAppResource(server, "Containers", VIEW_URI, {}, async (uri) => ({
    contents: [{ uri: uri.href, mimeType: RESOURCE_MIME_TYPE, text: await inlineDocument() }],
  }));
  return server;
}
