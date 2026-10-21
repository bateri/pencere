/**
 * The containers tool started the way a chat client starts an MCP server: with
 * a pipe for stdin. It must be an ordinary stdio server then, and the official
 * MCP client must be able to list its tools and read the view it offers.
 */

import { Client } from "@modelcontextprotocol/client";
import { StdioClientTransport } from "@modelcontextprotocol/client/stdio";
import { RESOURCE_MIME_TYPE } from "@modelcontextprotocol/ext-apps/server";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { VIEW_URI } from "../src/tool.js";

const MAIN = fileURLToPath(new URL("../dist/main.js", import.meta.url));

let client: Client;

beforeAll(async () => {
  // `tsc -b` builds it; the gate runs that first.
  expect(existsSync(MAIN), `${MAIN} is missing: run "npx tsc -b" first`).toBe(true);
  client = new Client({ name: "stdio-test", version: "0.0.0" });
  await client.connect(new StdioClientTransport({ command: process.execPath, args: [MAIN] }));
});

afterAll(async () => {
  await client.close();
});

describe("the tool over stdio", () => {
  it("lists its tools, and says which one has a view", async () => {
    const { tools } = await client.listTools();
    expect(tools.map((tool) => tool.name).sort()).toEqual([
      "list_containers",
      "start_container",
      "stop_container",
    ]);
    const list = tools.find((tool) => tool.name === "list_containers");
    expect(list?._meta).toMatchObject({ ui: { resourceUri: VIEW_URI } });
    expect(list?.title).toBe("Containers");
  });

  it("serves the view as one self-contained document", async () => {
    const { contents } = await client.readResource({ uri: VIEW_URI });
    const view = contents[0];
    expect(view?.mimeType).toBe(RESOURCE_MIME_TYPE);
    const html = view !== undefined && "text" in view ? view.text : "";
    expect(html).toContain("<style>");
    expect(html).toContain("<script>");
    expect(html).not.toMatch(/href="style\.css"|src="app\.js"/);
    expect(html).toContain("pencere/pane");
  });

  it("answers its tools with text a reader can use without the view", async () => {
    const before = await client.callTool({ name: "list_containers", arguments: {} });
    expect(JSON.stringify(before.content)).toContain("web");
    expect(before.structuredContent).toMatchObject({ containers: expect.any(Array) });

    const stopped = await client.callTool({ name: "stop_container", arguments: { id: "web" } });
    expect(JSON.stringify(stopped.content)).toContain("Stopped web");
    const after = await client.callTool({ name: "list_containers", arguments: {} });
    expect(JSON.stringify(after.structuredContent)).toContain('"id":"web","name":"web"');
    expect(JSON.stringify(after.structuredContent)).toMatch(/"id":"web"[^}]*"state":"stopped"/);
  });

  it("refuses a container that does not exist", async () => {
    const result = await client.callTool({ name: "stop_container", arguments: { id: "nope" } });
    expect(result.isError).toBe(true);
  });
});
