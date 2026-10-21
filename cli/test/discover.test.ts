import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import {
  DiscoveryError,
  Interrupted,
  MAX_TOOL_PAGES,
  discoverTool,
  viewOf,
} from "../src/discover.js";
import type { DiscoverOptions } from "../src/discover.js";
import { StartError } from "../src/server-process.js";
import { Tail } from "../src/tail.js";
import { alive, node, raw, tools, until } from "./support.js";

const options: DiscoverOptions = {
  version: "0.0.0",
  timeout: 10_000,
  timing: { stdin: 200, term: 200, kill: 2000 },
};

describe("which tool's view to show", () => {
  it("is the one tool that names a view", async () => {
    expect(await discoverTool(tools("one"), new Tail(), options)).toEqual({
      kind: "found",
      tool: "show",
    });
  });

  it("reads the flat key the first drafts of MCP Apps used as well", async () => {
    expect(await discoverTool(tools("legacy"), new Tail(), options)).toEqual({
      kind: "found",
      tool: "show",
    });
  });

  it("asks the person to choose when several tools name a view", async () => {
    expect(await discoverTool(tools("many"), new Tail(), options)).toEqual({
      kind: "many",
      tools: ["first", "second"],
    });
  });

  it("says which tools there were when none names a view, or there are none", async () => {
    expect(await discoverTool(tools("none"), new Tail(), options)).toEqual({
      kind: "none",
      tools: ["plain", "other"],
    });
    expect(await discoverTool(tools("empty"), new Tail(), options)).toEqual({
      kind: "none",
      tools: [],
    });
  });

  it("finds the view of the example tool, started as a chat client starts it", async () => {
    const main = fileURLToPath(new URL("../../examples/containers/dist/main.js", import.meta.url));
    // `tsc -b` builds it; the gate runs that first.
    expect(existsSync(main), `${main} is missing: run "npx tsc -b" first`).toBe(true);
    expect(
      await discoverTool({ command: process.execPath, args: [main] }, new Tail(), options),
    ).toEqual({ kind: "found", tool: "list_containers" });
  });

  it("finds the tool with a view on a later page of the list, as the terminal would", async () => {
    expect(await discoverTool(raw("paged"), new Tail(), options)).toEqual({
      kind: "found",
      tool: "show",
    });
  });

  it("stops at the number of pages the terminal reads, and says so", async () => {
    const failure = await discoverTool(raw("endless"), new Tail(), options).catch(
      (error: unknown) => error,
    );
    expect(failure).toBeInstanceOf(DiscoveryError);
    expect((failure as Error).message).toContain(`more than ${MAX_TOOL_PAGES} pages of tools`);
  });

  it("does not take an address that is not a ui:// resource for a view, as the terminal does not", async () => {
    expect(await discoverTool(raw("foreign"), new Tail(), options)).toEqual({
      kind: "none",
      tools: ["show"],
    });
  });

  it("writes what the server said in a failure as text", async () => {
    const failure = await discoverTool(raw("failing"), new Tail(), options).catch(
      (error: unknown) => error,
    );
    expect(failure).toBeInstanceOf(DiscoveryError);
    const message = (failure as Error).message;
    expect(message).toContain("no tools \\u001b[2Jfor you");
    expect(message).not.toContain("\u001b");
  });

  it("reads a tool's view from either place and from nothing else", () => {
    // The terminal takes a `ui://` resource only.
    expect(viewOf({ _meta: { ui: { resourceUri: "https://example.com/v" } } })).toBeUndefined();
    expect(viewOf({ _meta: { "ui/resourceUri": "file:///etc/passwd" } })).toBeUndefined();
    expect(viewOf({ _meta: { ui: { resourceUri: "ui:" } } })).toBeUndefined();
    expect(viewOf({ _meta: { ui: { resourceUri: "ui://a/b" } } })).toBe("ui://a/b");
    expect(viewOf({ _meta: { "ui/resourceUri": "ui://a/b" } })).toBe("ui://a/b");
    expect(viewOf({ _meta: { ui: { resourceUri: "" } } })).toBeUndefined();
    expect(viewOf({ _meta: { ui: { visibility: ["app"] } } })).toBeUndefined();
    expect(viewOf({ _meta: { ui: 3 } })).toBeUndefined();
    expect(viewOf({ _meta: "x" })).toBeUndefined();
    expect(viewOf({})).toBeUndefined();
  });

  it("fails plainly when the command cannot be started", async () => {
    await expect(
      discoverTool({ command: "/nonexistent/pencere-server", args: [] }, new Tail(), options),
    ).rejects.toBeInstanceOf(StartError);
  });

  it("fails when what started is not an MCP server, with what it said", async () => {
    const tail = new Tail();
    const failure = await discoverTool(
      node(`console.error("Traceback: no module named mcp"); process.exit(1);`),
      tail,
      options,
    ).catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(DiscoveryError);
    expect((failure as Error).message).toContain("did not answer as an MCP server");
    expect(tail.text()).toContain("no module named mcp");
  });

  it("gives up on a server that never answers, and lets it go", async () => {
    const tail = new Tail();
    const silent = node(`console.error(process.pid); setInterval(() => {}, 1000);`);
    const failure = await discoverTool(silent, tail, { ...options, timeout: 300 }).catch(
      (error: unknown) => error,
    );
    expect(failure).toBeInstanceOf(DiscoveryError);
    const pid = Number(tail.text());
    await until(() => !alive(pid), "the server to be let go");
  });

  it("stops asking when the person interrupts, and lets the server go", async () => {
    const tail = new Tail();
    const silent = node(`console.error(process.pid); setInterval(() => {}, 1000);`);
    const controller = new AbortController();
    const asking = discoverTool(silent, tail, { ...options, signal: controller.signal });
    await until(() => !tail.empty, "the server to start");
    controller.abort();
    await expect(asking).rejects.toBeInstanceOf(Interrupted);
    await until(() => !alive(Number(tail.text())), "the server to be let go");

    // An interruption that came before the question is the answer to it.
    await expect(
      discoverTool(silent, new Tail(), { ...options, signal: controller.signal }),
    ).rejects.toBeInstanceOf(Interrupted);
  });
});
