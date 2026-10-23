/** `serve`, played against a host that the test is: the sequence of §5 to §10 from the program's side. */

import { McpServer } from "@modelcontextprotocol/server";
import { PassThrough } from "node:stream";
import { describe, expect, it } from "vitest";

import { bundleFromFiles } from "../src/bundle.js";
import { ViewControl } from "../src/control.js";
import { toWire } from "../src/json.js";
import { serveWith } from "../src/serve.js";
import type { Connectable, ServeOptions } from "../src/serve.js";
import {
  CONTAINERS,
  EXAMPLE_DIGESTS,
  EXAMPLE_FILES,
  Rig,
  TOKEN,
  TOKEN_2,
  makeServer,
  payloadOf,
  welcomePayload,
} from "./support.js";

const DA_REPLY = "\x1b[?62;4c";
const FAST = { answerTimeout: 40, lateReplyGrace: 40 } as const;

const exampleBundle = (meta?: object) =>
  bundleFromFiles(
    Object.entries(EXAMPLE_FILES).map(([path, text]) => ({ path, data: Buffer.from(text) })),
    meta === undefined ? {} : { meta },
  );

const initialize = (id = 1) => ({
  jsonrpc: "2.0",
  id,
  method: "initialize",
  params: {
    protocolVersion: "2025-11-25",
    capabilities: {
      extensions: { "io.modelcontextprotocol/ui": { mimeTypes: ["text/html;profile=mcp-app"] } },
    },
    clientInfo: { name: "pencere-test", version: "0.0.0" },
  },
});

/** Starts `serve` and answers the capability query the way a host does. */
async function start(
  rig: Rig,
  options: Partial<ServeOptions> = {},
  server: Connectable = makeServer(),
  token = TOKEN,
  features: string[] = ["bundle"],
) {
  const result = serveWith(
    server,
    { tool: "list_containers", ...FAST, ...options },
    rig.environment(),
  );
  const hello = await rig.next("hello");
  rig.say("welcome", welcomePayload(token, features), hello.id);
  rig.sayBytes(DA_REPLY);
  return { result, hello, server };
}

/** The host's MCP start-up of §9.3, up to the call of the tool, answered by the program's server. */
async function mcpStartUp(rig: Rig) {
  rig.sayJson(initialize());
  const initialized = await rig.nextJson();
  expect(initialized.id).toBe(1);
  rig.sayJson({ jsonrpc: "2.0", method: "notifications/initialized" });
  rig.sayJson({ jsonrpc: "2.0", id: 2, method: "tools/list" });
  const tools = await rig.nextJson();
  rig.sayJson({
    jsonrpc: "2.0",
    id: 3,
    method: "tools/call",
    params: { name: "list_containers", arguments: {} },
  });
  const called = await rig.nextJson();
  return { initialized, tools, called };
}

describe("when stdin is not a terminal", () => {
  it("is an ordinary MCP server on stdio, and sends no frame of this protocol", async () => {
    const input = new PassThrough();
    const output = new PassThrough();
    const rig = new Rig();
    const lines: string[] = [];
    output.on("data", (chunk: Buffer) => lines.push(chunk.toString("utf8")));
    const result = serveWith(
      makeServer(),
      { tool: "list_containers", ...FAST },
      {
        ...rig.environment(),
        input: Object.assign(input, { isTTY: false }),
        output: Object.assign(output, { isTTY: false }),
      },
    );
    input.write(`${JSON.stringify(initialize())}\n`);
    await new Promise((resolve) => setTimeout(resolve, 80));
    const answer = JSON.parse(lines.join("").trim().split("\n")[0] ?? "") as {
      id: number;
      result: { serverInfo: { name: string } };
    };
    expect(answer.id).toBe(1);
    expect(answer.result.serverInfo.name).toBe("containers");
    expect(lines.join("")).not.toContain("\x1b");
    input.end();
    expect(await result).toEqual({ kind: "stdio" });
    expect(rig.terminal.calls).toEqual([]);
  });
});

describe("a filter in a pipeline", () => {
  it("is never an MCP server on stdio: its stdin is data, nothing is read or written, and the fallback runs", async () => {
    const rig = new Rig();
    const ran: string[] = [];
    const result = await serveWith(
      makeServer(),
      { tool: "list_containers", ...FAST, filter: true, fallback: () => void ran.push("fallback") },
      {
        ...rig.environment(),
        input: Object.assign(rig.input, { isTTY: false }),
        output: Object.assign(rig.output, { isTTY: false }),
      },
    );
    expect(result).toEqual({ kind: "no-view", reason: "no-host" });
    expect(ran).toEqual(["fallback"]);
    expect(rig.written).toEqual([]);
    expect(rig.terminal.calls).toEqual([]);
    expect(rig.input.listenerCount("data")).toBe(0);
  });

  it("asks on the terminal it is given, and opens its view there as any program does", async () => {
    const rig = new Rig();
    const { result } = await start(rig, { filter: true });
    await rig.next("open");
    rig.say("closed", toWire({ reason: "user" }));
    expect(await result).toEqual({ kind: "closed", by: "user" });
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
  });
});

describe("with no host", () => {
  it("concludes it when the terminal answers CSI c first, and gives the terminal back before the fallback", async () => {
    const rig = new Rig();
    const log = rig.terminal.calls;
    const result = serveWith(
      makeServer(),
      { tool: "list_containers", ...FAST, fallback: () => void log.push("fallback") },
      rig.environment(),
    );
    const hello = await rig.next("hello");
    expect(hello.payload).toBe(payloadOf("to-host/hello.frame"));
    expect(log).toEqual(["enter"]);
    // A terminal answers after it has been asked: the fence is written first.
    await rig.settle();
    rig.sayBytes(DA_REPLY);
    expect(await result).toEqual({ kind: "no-view", reason: "no-host" });
    expect(log).toEqual(["enter", "restore", "fallback"]);
    // Nothing but the query was written: the hello, and the fence behind it.
    await rig.settle();
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello"]);
    expect(rig.text()).toBe("\x1b[c");
    expect(rig.input.listenerCount("data")).toBe(0);
    expect(rig.process.listenerCount("SIGINT") + rig.process.listenerCount("exit")).toBe(0);
  });

  it("does not believe a welcome that does not answer its hello, nor one that comes after the reply", async () => {
    const rig = new Rig();
    const result = serveWith(makeServer(), { tool: "list_containers", ...FAST }, rig.environment());
    const hello = await rig.next("hello");
    rig.say("welcome", welcomePayload(TOKEN), hello.id + 5);
    rig.say("welcome", welcomePayload(TOKEN));
    rig.sayBytes(DA_REPLY);
    rig.say("welcome", welcomePayload(TOKEN), hello.id);
    expect(await result).toEqual({ kind: "no-view", reason: "no-host" });
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello"]);
  });

  it("concludes it after the time, and reads on a while so that a late reply does not reach the shell", async () => {
    const rig = new Rig();
    const result = serveWith(
      makeServer(),
      { tool: "list_containers", answerTimeout: 30, lateReplyGrace: 500 },
      rig.environment(),
    );
    await rig.next("hello");
    const started = Date.now();
    await new Promise((resolve) => setTimeout(resolve, 80));
    // The answer is late, and it is read, and the reading ends with it rather than with the grace.
    expect(rig.input.listenerCount("data")).toBe(1);
    rig.sayBytes(DA_REPLY);
    expect(await result).toEqual({ kind: "no-view", reason: "no-host" });
    expect(Date.now() - started).toBeLessThan(400);
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
  });

  it("gives up reading when the grace is over", async () => {
    const rig = new Rig();
    const result = await serveWith(
      makeServer(),
      { tool: "list_containers", answerTimeout: 20, lateReplyGrace: 20 },
      rig.environment(),
    );
    expect(result).toEqual({ kind: "no-view", reason: "no-host" });
    expect(rig.input.listenerCount("data")).toBe(0);
  });

  it("concludes it from a host that shares no version", async () => {
    const rig = new Rig();
    const ran: string[] = [];
    const result = serveWith(
      makeServer(),
      { tool: "list_containers", ...FAST, fallback: () => void ran.push("fallback") },
      rig.environment(),
    );
    const hello = await rig.next("hello");
    rig.say("welcome", payloadOf("to-program/welcome-unsupported.frame"), hello.id);
    rig.sayBytes(DA_REPLY);
    expect(await result).toEqual({ kind: "no-view", reason: "no-common-version" });
    expect(ran).toEqual(["fallback"]);
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello"]);
  });

  it("concludes it from a host that opens none of the kinds of view it can show, at once", async () => {
    const rig = new Rig();
    const ran: string[] = [];
    const result = serveWith(
      makeServer(),
      // A long wait, because the program must not wait it out.
      {
        tool: "list_containers",
        ...FAST,
        answerTimeout: 60_000,
        fallback: () => void ran.push("fallback"),
      },
      rig.environment(),
    );
    const hello = await rig.next("hello");
    rig.say("welcome", payloadOf("to-program/welcome-no-views.frame"), hello.id);
    rig.sayBytes(DA_REPLY);
    expect(await result).toEqual({ kind: "no-view", reason: "no-common-view" });
    expect(ran).toEqual(["fallback"]);
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello"]);
  });

  it("says in its hello which kinds of view it can show, and names the kind in its open", async () => {
    const rig = new Rig();
    void (await start(rig));
    const hello = rig.messages()[0];
    expect(JSON.parse(hello?.payload ?? "{}")).toEqual({ versions: [1], views: ["web"] });
    const open = await rig.next("open");
    expect(JSON.parse(open.payload)).toMatchObject({ token: TOKEN, view: "web" });
  });

  it("is where the modes cannot be set: nothing is written and the fallback runs", async () => {
    const rig = new Rig();
    rig.terminal.failEnter = true;
    const ran: string[] = [];
    const result = await serveWith(
      makeServer(),
      { tool: "list_containers", ...FAST, fallback: () => void ran.push("fallback") },
      rig.environment(),
    );
    expect(result).toEqual({ kind: "no-view", reason: "no-host" });
    expect(ran).toEqual(["fallback"]);
    expect(rig.written).toEqual([]);
    expect(rig.input.listenerCount("data")).toBe(0);
  });

  it("is where stdout is not a terminal: there is nowhere to ask", async () => {
    const rig = new Rig();
    const env = rig.environment();
    const result = await serveWith(
      makeServer(),
      { tool: "list_containers", ...FAST },
      {
        ...env,
        output: Object.assign(rig.output, { isTTY: false }),
      },
    );
    expect(result).toEqual({ kind: "no-view", reason: "no-host" });
    expect(rig.written).toEqual([]);
    expect(rig.terminal.calls).toEqual([]);
  });

  it("does not run the fallback of a wait that was aborted", async () => {
    const rig = new Rig();
    const controller = new AbortController();
    const ran: string[] = [];
    const result = serveWith(
      makeServer(),
      {
        tool: "list_containers",
        ...FAST,
        answerTimeout: 5000,
        signal: controller.signal,
        fallback: () => void ran.push("fallback"),
      },
      rig.environment(),
    );
    await rig.next("hello");
    controller.abort();
    expect(await result).toEqual({ kind: "no-view", reason: "aborted" });
    expect(ran).toEqual([]);
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
    // An abort that came before anything: not even the terminal is touched.
    const again = await serveWith(
      makeServer(),
      { tool: "x", signal: controller.signal },
      new Rig().environment(),
    );
    expect(again).toEqual({ kind: "no-view", reason: "aborted" });
  });
});

describe("with a host", () => {
  it("takes the modes first, asks, takes the alternate screen, opens with the token, and serves the view's calls", async () => {
    const rig = new Rig();
    const { result, hello } = await start(rig, {
      tool: { name: "list_containers", arguments: { all: true } },
    });
    expect(rig.terminal.calls).toEqual(["enter"]);
    const open = await rig.next("open");
    expect(open.payload).toBe(
      `{"token":"${TOKEN}","view":"web","tool":{"name":"list_containers","arguments":{"all":true}}}`,
    );
    // In the order of §5.1 and §6.5: the hello and its fence, the alternate screen, the open.
    expect(
      rig.written.map((w) => (w.type === "text" ? JSON.stringify(w.text) : w.message.kind)),
    ).toEqual(["hello", JSON.stringify("\x1b[c"), JSON.stringify("\x1b[?1049h"), "open"]);
    expect(open.id).toBeGreaterThan(hello.id);

    const { initialized, tools, called } = await mcpStartUp(rig);
    expect(initialized.result).toMatchObject({ serverInfo: { name: "containers" } });
    expect(JSON.stringify(tools.result)).toContain("ui://containers/view.html");
    expect(called.result).toMatchObject({ structuredContent: { containers: CONTAINERS } });

    rig.say("closed", toWire({ reason: "user" }));
    expect(await result).toEqual({ kind: "closed", by: "user" });
    await rig.settle();
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello", "open", "msg", "msg", "msg"]);
    expect(rig.text()).toBe("\x1b[c\x1b[?1049h\x1b[?1049l");
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
    expect(
      rig.process.listenerCount("SIGINT") +
        rig.process.listenerCount("SIGTERM") +
        rig.process.listenerCount("exit"),
    ).toBe(0);
    expect(rig.input.listenerCount("data")).toBe(0);
    expect(rig.errors).toEqual([]);
    // The person closed the view; the program has nothing to say about it.
    rig.process.emit("exit", 0);
    expect(rig.messages().map((m) => m.kind)).not.toContain("bye");
  });

  it("offers its bundle to a host that takes bundles, and sends only the files the host lacks, in its order", async () => {
    const rig = new Rig();
    const { result } = await start(rig, {
      bundle: exampleBundle({
        csp: { connectDomains: ["https://api.example.com"] },
        prefersBorder: false,
      }),
    });
    const open = await rig.next("open");
    expect(open.payload).toBe(payloadOf("to-host/open-bundle.frame"));
    // The files come in the order of the manifest, not of the host's wish.
    const manifestFiles = (JSON.parse(open.payload) as { bundle: { files: { path: string }[] } })
      .bundle.files;
    expect(manifestFiles.map((f) => f.path)).toEqual(["index.html", "app.js", "style.css"]);
    rig.say(
      "need",
      toWire({ missing: [EXAMPLE_DIGESTS["style.css"], EXAMPLE_DIGESTS["app.js"]] }),
      open.id,
    );
    const first = await rig.next("file");
    const second = await rig.next("file");
    expect(first.control.d).toBe(EXAMPLE_DIGESTS["style.css"]);
    expect(Buffer.from(first.payload, "base64").toString()).toBe(EXAMPLE_FILES["style.css"]);
    expect(second.control.d).toBe(EXAMPLE_DIGESTS["app.js"]);
    expect(second.payload).toBe(payloadOf("to-host/file-chunked.frame"));
    rig.say("closed", toWire({ reason: "user" }));
    await result;
    expect(rig.messages().filter((m) => m.kind === "file")).toHaveLength(2);
  });

  it("sends no file when the host holds them all, or asks for one that is not in the bundle, or answers another open", async () => {
    const rig = new Rig();
    const { result } = await start(rig, { bundle: exampleBundle() });
    const open = await rig.next("open");
    rig.say("need", toWire({ missing: [] }), open.id);
    rig.say("need", toWire({ missing: ["f".repeat(64)] }), open.id);
    rig.say("need", toWire({ missing: [EXAMPLE_DIGESTS["app.js"]] }), open.id + 9);
    rig.say("need", "not json", open.id);
    await rig.settle();
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello", "open"]);
    rig.say("closed", toWire({ reason: "user" }));
    await result;
  });

  it("writes files and the server's messages whole, never one inside the other", async () => {
    const rig = new Rig();
    const big = Buffer.alloc(300_000, 3);
    const bundle = bundleFromFiles([
      { path: "index.html", data: Buffer.from("<p>") },
      { path: "big.bin", data: big },
    ]);
    const { result } = await start(rig, { bundle });
    const open = await rig.next("open");
    const digests = (
      JSON.parse(open.payload) as { bundle: { files: { sha256: string }[] } }
    ).bundle.files.map((f) => f.sha256);
    rig.say("need", toWire({ missing: digests }), open.id);
    // The host starts talking while the files are still being written.
    rig.sayJson(initialize());
    rig.sayJson({ jsonrpc: "2.0", id: 2, method: "tools/list" });
    const files = [await rig.next("file"), await rig.next("file")];
    const replies = [await rig.nextJson(), await rig.nextJson()];
    expect(rig.errors).toEqual([]);
    expect(Buffer.from(files[1]?.payload ?? "", "base64")).toEqual(big);
    expect(replies.map((r) => r.id)).toEqual([1, 2]);
    rig.say("closed", toWire({ reason: "user" }));
    await result;
  });

  it("opens without the manifest when the host does not list the bundle feature", async () => {
    const rig = new Rig();
    const { result } = await start(rig, { bundle: exampleBundle() }, makeServer(), TOKEN, []);
    const open = await rig.next("open");
    expect(open.payload).toBe(payloadOf("to-host/open.frame"));
    rig.say("closed", toWire({ reason: "view" }));
    expect(await result).toEqual({ kind: "closed", by: "view" });
  });

  it("runs the fallback of a refusal, with the code, once the terminal is back", async () => {
    const rig = new Rig();
    const log = rig.terminal.calls;
    const { result } = await start(rig, { fallback: () => void log.push("fallback") });
    const open = await rig.next("open");
    rig.say("closed", payloadOf("to-program/closed-refused.frame"), open.id);
    expect(await result).toEqual({ kind: "no-view", reason: "refused", code: "token" });
    expect(log).toEqual(["enter", "restore", "fallback"]);
    expect(rig.text()).toBe("\x1b[c\x1b[?1049h\x1b[?1049l");
    // The host ended the open: there is no view whose end the program would announce.
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello", "open"]);
  });

  it("takes a failure before the view ran as no view, and one after it as the view's end", async () => {
    const early = new Rig();
    const first = await start(early);
    const open = await early.next("open");
    early.say("closed", payloadOf("to-program/closed-error.frame"), open.id);
    expect(await first.result).toEqual({ kind: "no-view", reason: "failed", code: "bundle" });

    const late = new Rig();
    const second = await start(late);
    await late.next("open");
    await mcpStartUp(late);
    late.say("closed", toWire({ reason: "error", code: "internal" }));
    expect(await second.result).toEqual({ kind: "closed", by: "error" });
  });

  it("ignores a closed that answers an open that is not its own, and a kind it does not know", async () => {
    const rig = new Rig();
    const { result } = await start(rig);
    const open = await rig.next("open");
    rig.say("closed", toWire({ reason: "refused", code: "busy" }), open.id + 1);
    rig.say("zzz", "{}");
    rig.say("welcome", welcomePayload(TOKEN_2), open.id);
    rig.say("closed", "not json");
    await rig.settle();
    expect(rig.terminal.calls).toEqual(["enter"]);
    rig.say("closed", toWire({ reason: "later" }), open.id);
    expect(await result).toEqual({ kind: "no-view", reason: "failed" });
  });

  it("goes on through typed characters, the terminal's replies and a stream cut at every byte", async () => {
    const rig = new Rig();
    const server = makeServer();
    const result = serveWith(server, { tool: "list_containers", ...FAST }, rig.environment());
    const hello = await rig.next("hello");
    const noisy = Buffer.concat([
      Buffer.from("ls\r\x1b[A"),
      Buffer.from("\x1b_Gi=1;AAAA\x1b\\"),
      Buffer.from(`\x1b_pencere;welcome;id=1,re=${hello.id};${welcomePayload(TOKEN)}\x1b\\`),
      Buffer.from(`${DA_REPLY}x`),
    ]);
    for (const byte of noisy) rig.sayBytes(Uint8Array.of(byte));
    const open = await rig.next("open");
    expect(open.payload).toContain(TOKEN);
    rig.say("closed", toWire({ reason: "user" }));
    expect(await result).toEqual({ kind: "closed", by: "user" });
    expect(rig.errors).toEqual([]);
  });
});

describe("a host that starts the view again", () => {
  it("is answered with a new open, a new token and a fresh MCP session", async () => {
    const rig = new Rig();
    const { result } = await start(rig, { bundle: exampleBundle() });
    const first = await rig.next("open");
    rig.say("need", toWire({ missing: [] }), first.id);
    await mcpStartUp(rig);

    // A host that came up after a break: an unsolicited welcome, with a new token.
    rig.say("welcome", welcomePayload(TOKEN_2, ["bundle"], { restart: true }));
    const second = await rig.next("open");
    expect(JSON.parse(second.payload)).toMatchObject({
      token: TOKEN_2,
      tool: { name: "list_containers" },
    });
    expect(second.id).toBeGreaterThan(first.id);
    rig.say("need", toWire({ missing: [EXAMPLE_DIGESTS["app.js"]] }), second.id);
    expect((await rig.next("file")).control.d).toBe(EXAMPLE_DIGESTS["app.js"]);

    // The session is a new one: it starts with `initialize` again.
    const { initialized, called } = await mcpStartUp(rig);
    expect(initialized.result).toMatchObject({ serverInfo: { name: "containers" } });
    expect(called.result).toMatchObject({ structuredContent: { containers: CONTAINERS } });

    // The view is now the second open's: a bye carries its token.
    rig.process.emit("exit", 0);
    const bye = rig.messages().at(-1);
    expect(bye?.kind).toBe("bye");
    expect(JSON.parse(bye?.payload ?? "")).toEqual({ token: TOKEN_2, reason: "exit" });
    void result;
  });

  it("is ignored when the welcome does not say restart, answers something, or comes with no view open", async () => {
    const rig = new Rig();
    const { result } = await start(rig);
    const open = await rig.next("open");
    rig.say("welcome", welcomePayload(TOKEN_2));
    rig.say("welcome", welcomePayload(TOKEN_2, ["bundle"], { restart: true }), open.id);
    await rig.settle();
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello", "open"]);
    rig.say("closed", toWire({ reason: "user" }));
    await result;
    rig.say("welcome", welcomePayload(TOKEN_2, ["bundle"], { restart: true }));
    await rig.settle();
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello", "open"]);
  });
});

describe("a program that closes its view and goes on (§10.8)", () => {
  const FEATURES = ["bundle", "dismiss"];
  const CHANGED_APP = 'document.getElementById("app").textContent = "hello again";\n';
  const changedBundle = () =>
    bundleFromFiles(
      Object.entries({ ...EXAMPLE_FILES, "app.js": CHANGED_APP }).map(([path, text]) => ({
        path,
        data: Buffer.from(text),
      })),
    );
  const digestOf = (bundle: ReturnType<typeof changedBundle>, path: string) =>
    bundle.manifest.files.find((file) => file.path === path)?.sha256;

  /** A view that runs: the open answered, the MCP start-up done and the tool called. */
  async function running(
    options: Partial<ServeOptions> = {},
    features = FEATURES,
    server: Connectable = makeServer(),
  ) {
    const control = new ViewControl();
    const rig = new Rig();
    const started = await start(
      rig,
      { bundle: exampleBundle(), control, ...options },
      server,
      TOKEN,
      features,
    );
    const open = await rig.next("open");
    rig.say("need", toWire({ missing: [] }), open.id);
    await mcpStartUp(rig);
    return { rig, control, open, ...started };
  }

  const kinds = (rig: Rig) => rig.messages().map((m) => m.kind);

  it("reopens: the view is dismissed with its token, then a new token, a new open and a fresh MCP session", async () => {
    const { rig, control, result } = await running();
    const changed = changedBundle();
    const reopened = control.reopen({ bundle: changed });
    const dismiss = await rig.next("dismiss");
    expect(dismiss.payload).toBe(payloadOf("to-host/dismiss.frame"));
    // The view is not gone until the host says so: the program does not ask for another yet.
    await rig.settle();
    expect(kinds(rig).filter((kind) => kind === "hello")).toHaveLength(1);

    rig.say("closed", payloadOf("to-program/closed-program.frame"), dismiss.id);
    const hello = await rig.next("hello");
    // The host is known: no fence behind this hello, and the alternate screen stays.
    expect(rig.text()).toBe("\x1b[c\x1b[?1049h");
    rig.say("welcome", welcomePayload(TOKEN_2, FEATURES), hello.id);
    const open = await rig.next("open");
    expect(JSON.parse(open.payload)).toMatchObject({
      token: TOKEN_2,
      tool: { name: "list_containers" },
      bundle: changed.manifest,
    });
    // Only what changed crosses the stream again.
    rig.say("need", toWire({ missing: [digestOf(changed, "app.js")] }), open.id);
    expect((await rig.next("file")).control.d).toBe(digestOf(changed, "app.js"));

    // A new MCP session: it starts with `initialize` again, and the call of the tool ends the wait.
    const { initialized, called } = await mcpStartUp(rig);
    expect(initialized.result).toMatchObject({ serverInfo: { name: "containers" } });
    expect(called.result).toMatchObject({ structuredContent: { containers: CONTAINERS } });
    expect(await reopened).toEqual({ kind: "reopened" });
    expect(rig.terminal.calls).toEqual(["enter"]);

    // The view is the new one's: a second reopening works, and its dismiss carries the new token.
    void control.reopen();
    const again = await rig.next("dismiss");
    expect(JSON.parse(again.payload)).toEqual({ token: TOKEN_2 });
    rig.process.emit("exit", 0);
    void result;
  });

  it("shows the same files again when no others are given, and the same tool", async () => {
    const { rig, control, open } = await running();
    const reopened = control.reopen();
    rig.say("closed", payloadOf("to-program/closed-program.frame"), (await rig.next("dismiss")).id);
    rig.say("welcome", welcomePayload(TOKEN_2, FEATURES), (await rig.next("hello")).id);
    const second = await rig.next("open");
    expect(JSON.parse(second.payload)).toEqual({ ...JSON.parse(open.payload), token: TOKEN_2 });
    void reopened;
  });

  it("goes on with another tool when the reopening names one", async () => {
    const { rig, control } = await running();
    void control.reopen({ tool: { name: "other", arguments: { a: 1 } } });
    rig.say("closed", payloadOf("to-program/closed-program.frame"), (await rig.next("dismiss")).id);
    rig.say("welcome", welcomePayload(TOKEN_2, FEATURES), (await rig.next("hello")).id);
    expect(JSON.parse((await rig.next("open")).payload)).toMatchObject({
      tool: { name: "other", arguments: { a: 1 } },
    });
  });

  it("does not take the call of the tool by the view that is closing for the new view", async () => {
    const { rig, control } = await running();
    const reopened = control.reopen();
    let settled = false;
    void reopened.then(() => (settled = true));
    const dismiss = await rig.next("dismiss");
    // The old view refreshes while the host is still tearing it down.
    rig.sayJson({
      jsonrpc: "2.0",
      id: 9,
      method: "tools/call",
      params: { name: "list_containers", arguments: {} },
    });
    await rig.settle();
    expect(settled).toBe(false);
    rig.say("closed", payloadOf("to-program/closed-program.frame"), dismiss.id);
    await rig.next("hello");
  });

  it("closes: the terminal is given back and serve ends as closed by the program", async () => {
    const { rig, control, result } = await running();
    const closed = control.close();
    const dismiss = await rig.next("dismiss");
    rig.say("closed", payloadOf("to-program/closed-program.frame"), dismiss.id);
    expect(await closed).toEqual({ kind: "closed" });
    expect(await result).toEqual({ kind: "closed", by: "program" });
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
    expect(rig.text()).toBe("\x1b[c\x1b[?1049h\x1b[?1049l");
    // The view's token is gone with the view: no `bye` follows.
    expect(kinds(rig)).toEqual(["hello", "open", "msg", "msg", "msg", "dismiss"]);
    expect(rig.process.listenerCount("exit")).toBe(0);
    // And a control whose serve has ended has nothing to close.
    expect(await control.close()).toEqual({ kind: "unavailable" });
    expect(await control.reopen()).toEqual({ kind: "unavailable" });
  });

  it("touches nothing when the host does not list dismiss, and says so", async () => {
    const { rig, control, result } = await running({}, ["bundle"]);
    expect(await control.reopen()).toEqual({ kind: "unsupported" });
    expect(await control.close()).toEqual({ kind: "unsupported" });
    await rig.settle();
    expect(kinds(rig)).toEqual(["hello", "open", "msg", "msg", "msg"]);
    rig.say("closed", toWire({ reason: "user" }));
    expect(await result).toEqual({ kind: "closed", by: "user" });
  });

  it("has no view to close before there is one, while one is being closed, or on stdio", async () => {
    const control = new ViewControl();
    expect(await control.reopen()).toEqual({ kind: "unavailable" });
    expect(await control.close()).toEqual({ kind: "unavailable" });

    const rig = new Rig();
    const served = serveWith(
      makeServer(),
      { tool: "list_containers", control, ...FAST },
      rig.environment(),
    );
    const hello = await rig.next("hello");
    // Asking for a host is not showing a view.
    expect(await control.reopen()).toEqual({ kind: "unavailable" });
    rig.say("welcome", welcomePayload(TOKEN, FEATURES), hello.id);
    rig.sayBytes(DA_REPLY);
    await rig.next("open");
    await rig.settle();
    // A view that is opening can be dismissed; one being dismissed cannot.
    const first = control.reopen();
    expect(await control.reopen()).toEqual({ kind: "unavailable" });
    expect(await control.close()).toEqual({ kind: "unavailable" });
    expect(kinds(rig).filter((kind) => kind === "dismiss")).toHaveLength(1);
    rig.say("closed", toWire({ reason: "user" }));
    expect(await served).toEqual({ kind: "closed", by: "user" });
    expect(await first).toEqual({ kind: "unavailable" });
  });

  it("is told unavailable when someone else closes the view before the host answers the dismiss", async () => {
    const { rig, control, result } = await running();
    const reopened = control.reopen();
    await rig.next("dismiss");
    rig.say("closed", toWire({ reason: "user" }));
    expect(await result).toEqual({ kind: "closed", by: "user" });
    expect(await reopened).toEqual({ kind: "unavailable" });
    expect(kinds(rig).filter((kind) => kind === "hello")).toHaveLength(1);
  });

  it("ends as no view, with the fallback, when the host refuses the new open", async () => {
    const log: string[] = [];
    const { rig, control, result } = await running({ fallback: () => void log.push("fallback") });
    const reopened = control.reopen();
    rig.say("closed", payloadOf("to-program/closed-program.frame"), (await rig.next("dismiss")).id);
    rig.say("welcome", welcomePayload(TOKEN_2, FEATURES), (await rig.next("hello")).id);
    const open = await rig.next("open");
    rig.say("closed", payloadOf("to-program/closed-refused.frame"), open.id);
    const ended = { kind: "no-view", reason: "refused", code: "token" };
    expect(await reopened).toEqual(ended);
    expect(await result).toEqual(ended);
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
    expect(log).toEqual(["fallback"]);
  });

  it("ends as no view, for want of an answer and not for want of a host, when the host stops answering the hello", async () => {
    const { rig, control, result } = await running({ reopenTimeout: 40 });
    const reopened = control.reopen();
    rig.say("closed", payloadOf("to-program/closed-program.frame"), (await rig.next("dismiss")).id);
    await rig.next("hello");
    // A host that answered before is there: that it is silent now is a timeout, which `no-host` is not.
    const ended = { kind: "no-view", reason: "timeout" };
    expect(await reopened).toEqual(ended);
    expect(await result).toEqual(ended);
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
    expect(rig.text()).toBe("\x1b[c\x1b[?1049h\x1b[?1049l");
  });

  it("waits for the answer to the hello of a reopening longer than for the first one", async () => {
    // The first question has 40 ms (`FAST`); a host that is busy replacing a view may take longer.
    const { rig, control } = await running({ reopenTimeout: 1000 });
    const reopened = control.reopen();
    rig.say("closed", payloadOf("to-program/closed-program.frame"), (await rig.next("dismiss")).id);
    const hello = await rig.next("hello");
    await new Promise((resolve) => setTimeout(resolve, 200));
    rig.say("welcome", welcomePayload(TOKEN_2, FEATURES), hello.id);
    expect(JSON.parse((await rig.next("open")).payload)).toMatchObject({ token: TOKEN_2 });
    void reopened;
  });

  it("gives up on a host that does not answer the dismiss, and says bye with the token the view has", async () => {
    const { rig, control, result } = await running({ reopenTimeout: 40 });
    const reopened = control.reopen();
    await rig.next("dismiss");
    const ended = { kind: "no-view", reason: "timeout" };
    expect(await reopened).toEqual(ended);
    expect(await result).toEqual(ended);
    // The view may still be up at the host: the program's end is what takes it down.
    const bye = rig.messages().at(-1);
    expect(bye?.kind).toBe("bye");
    expect(JSON.parse(bye?.payload ?? "")).toEqual({ token: TOKEN, reason: "error" });
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
    // A `closed` that comes after is for nothing.
    rig.say("closed", payloadOf("to-program/closed-program.frame"));
    await rig.settle();
    expect(rig.process.listenerCount("exit")).toBe(0);
  });

  it("does not wait on a dismiss the host has answered, once the answer is in", async () => {
    const { rig, control, result } = await running({ reopenTimeout: 80 });
    const reopened = control.reopen();
    rig.say("closed", payloadOf("to-program/closed-program.frame"), (await rig.next("dismiss")).id);
    rig.say("welcome", welcomePayload(TOKEN_2, FEATURES), (await rig.next("hello")).id);
    const open = await rig.next("open");
    rig.say("need", toWire({ missing: [] }), open.id);
    await mcpStartUp(rig);
    expect(await reopened).toEqual({ kind: "reopened" });
    // The bound of the dismiss is over with the dismiss: past it, the view is still there.
    await new Promise((resolve) => setTimeout(resolve, 200));
    let ended = false;
    void result.then(() => (ended = true));
    await rig.settle();
    expect(ended).toBe(false);
    rig.process.emit("exit", 0);
  });

  it("gives the terminal back on a signal between the views, with no bye, as the view has none", async () => {
    const { rig, control, result } = await running();
    const reopened = control.reopen();
    rig.say("closed", payloadOf("to-program/closed-program.frame"), (await rig.next("dismiss")).id);
    await rig.next("hello");
    rig.process.on("SIGINT", () => {});
    rig.process.emit("SIGINT");
    expect(await result).toEqual({ kind: "closed", by: "signal" });
    expect(await reopened).toEqual({ kind: "unavailable" });
    expect(kinds(rig)).not.toContain("bye");
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
    expect(rig.text()).toBe("\x1b[c\x1b[?1049h\x1b[?1049l");
  });

  it("says bye with the view's token when the signal comes before the host has answered the dismiss", async () => {
    const { rig, control } = await running();
    void control.reopen();
    await rig.next("dismiss");
    rig.process.emit("exit", 0);
    const bye = rig.messages().at(-1);
    expect(bye?.kind).toBe("bye");
    expect(JSON.parse(bye?.payload ?? "")).toEqual({ token: TOKEN, reason: "exit" });
  });

  describe("while the host starts the view again (§10.7)", () => {
    const TOKEN_3 = "wwwwwwwwwwwwwwwwwwwwww";
    const restart = (rig: Rig, token: string) =>
      rig.say("welcome", welcomePayload(token, FEATURES, { restart: true }));

    /** The open of a restart is answered like any other, and its session started. */
    async function view(rig: Rig, open: { id: number }) {
      rig.say("need", toWire({ missing: [] }), open.id);
      await mcpStartUp(rig);
    }

    it("ends a reopening whose dismiss is out and shows the view the host asks for, with a token that is alive", async () => {
      const { rig, control } = await running();
      const reopened = control.reopen();
      await rig.next("dismiss");
      // The host knows nothing of the dismiss: it came up after a break and asks for the view.
      restart(rig, TOKEN_2);
      const open = await rig.next("open");
      expect(JSON.parse(open.payload)).toMatchObject({ token: TOKEN_2 });
      expect(await reopened).toEqual({ kind: "unavailable" });
      await view(rig, open);

      // The view is the restarted one's: a dismiss carries its token, and `reopen` is free again.
      void control.reopen();
      expect(JSON.parse((await rig.next("dismiss")).payload)).toEqual({ token: TOKEN_2 });
    });

    it("ends a reopening whose new open is out, and opens again with the new token", async () => {
      const { rig, control } = await running();
      const reopened = control.reopen();
      rig.say(
        "closed",
        payloadOf("to-program/closed-program.frame"),
        (await rig.next("dismiss")).id,
      );
      rig.say("welcome", welcomePayload(TOKEN_2, FEATURES), (await rig.next("hello")).id);
      expect(JSON.parse((await rig.next("open")).payload)).toMatchObject({ token: TOKEN_2 });
      restart(rig, TOKEN_3);
      const open = await rig.next("open");
      expect(JSON.parse(open.payload)).toMatchObject({ token: TOKEN_3 });
      expect(await reopened).toEqual({ kind: "unavailable" });
      await view(rig, open);
    });

    it("takes the offer for the answer to a hello that waits, and goes on with the reopening", async () => {
      const { rig, control } = await running();
      const reopened = control.reopen();
      rig.say(
        "closed",
        payloadOf("to-program/closed-program.frame"),
        (await rig.next("dismiss")).id,
      );
      await rig.next("hello");
      // The host came up as the program asked: what it offers is the answer.
      restart(rig, TOKEN_2);
      const open = await rig.next("open");
      expect(JSON.parse(open.payload)).toMatchObject({ token: TOKEN_2 });
      await view(rig, open);
      expect(await reopened).toEqual({ kind: "reopened" });
    });

    it("sends no dismiss with the token of a view that is gone while the session of the restart starts", async () => {
      // A session that is slow to end holds the restart between ending the old session and opening.
      const server = makeServer();
      let slow = false;
      let release: () => void = () => {};
      const gate = new Promise<void>((resolve) => (release = resolve));
      const connectable: Connectable = {
        connect: (transport) => server.connect(transport),
        close: async () => {
          if (slow) await gate;
          await server.close();
        },
      };
      const { rig, control } = await running({}, FEATURES, connectable);
      slow = true;
      restart(rig, TOKEN_2);
      await rig.settle();
      // The token of the view the host forgot is no one's: there is no view to dismiss yet.
      expect(await control.reopen()).toEqual({ kind: "unavailable" });
      expect(await control.close()).toEqual({ kind: "unavailable" });
      expect(kinds(rig)).not.toContain("dismiss");
      release();
      expect(JSON.parse((await rig.next("open")).payload)).toMatchObject({ token: TOKEN_2 });
    });
  });

  it("reads closed by the program as the view's end, whoever asked", async () => {
    const { rig, result } = await running();
    rig.say("closed", payloadOf("to-program/closed-program.frame"));
    expect(await result).toEqual({ kind: "closed", by: "program" });
  });
});

describe("when the program ends while the view is up", () => {
  async function up(options: Partial<ServeOptions> = {}) {
    const rig = new Rig();
    const started = await start(rig, options);
    await rig.next("open");
    return { rig, ...started };
  }

  const kinds = (rig: Rig) =>
    rig.written.map((w) => (w.type === "text" ? JSON.stringify(w.text) : w.message.kind));

  for (const signal of ["SIGINT", "SIGTERM"] as const) {
    it(`${signal} sends bye with the token, leaves the alternate screen, gives the tty back and lets the signal go on`, async () => {
      const { rig } = await up();
      rig.process.emit(signal);
      const messages = rig.messages();
      expect(messages.at(-1)?.kind).toBe("bye");
      expect(messages.at(-1)?.payload).toBe(`{"token":"${TOKEN}","reason":"signal"}`);
      expect(kinds(rig).slice(-2)).toEqual(["bye", JSON.stringify("\x1b[?1049l")]);
      expect(rig.terminal.calls).toEqual(["enter", "restore"]);
      expect(rig.process.kills).toEqual([[4242, signal]]);
      expect(rig.process.listenerCount(signal) + rig.process.listenerCount("exit")).toBe(0);
      expect(rig.input.listenerCount("data")).toBe(0);
    });

    it(`keeps listening for ${signal} until the terminal is given back, so a forwarded second one cannot end the program between two writes`, async () => {
      // npm, running the program as a script, forwards the ⌃C the tty sent: the
      // program gets it twice, and the second must not meet the default action.
      const { rig } = await up();
      const listening: number[] = [];
      const write = rig.output.write.bind(rig.output);
      rig.output.write = ((...args: Parameters<typeof write>) => {
        listening.push(rig.process.listenerCount(signal));
        return write(...args);
      }) as typeof rig.output.write;
      const restore = rig.terminal.restore.bind(rig.terminal);
      rig.terminal.restore = () => {
        listening.push(rig.process.listenerCount(signal));
        restore();
      };
      rig.process.emit(signal);
      // `bye`, the primary screen and the tty's modes.
      expect(listening).toEqual([1, 1, 1]);
      expect(rig.process.kills).toEqual([[4242, signal]]);
      expect(rig.process.listenerCount(signal)).toBe(0);
    });
  }

  it("leaves the signal to the program's own listener when it has one, and says so", async () => {
    const { rig, result } = await up();
    await mcpStartUp(rig);
    rig.process.on("SIGINT", () => {});
    rig.process.emit("SIGINT");
    expect(await result).toEqual({ kind: "closed", by: "signal" });
    expect(rig.process.kills).toEqual([]);
    expect(rig.messages().at(-1)?.kind).toBe("bye");
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
  });

  it("sends bye with reason exit when the process exits, and error when it exits with a code", async () => {
    const clean = await up();
    clean.rig.process.emit("exit", 0);
    expect(clean.rig.messages().at(-1)?.payload).toBe(`{"token":"${TOKEN}","reason":"exit"}`);
    expect(clean.rig.text().endsWith("\x1b[?1049l")).toBe(true);
    expect(clean.rig.terminal.calls).toEqual(["enter", "restore"]);
    const failing = await up();
    failing.rig.process.emit("exit", 1);
    expect(failing.rig.messages().at(-1)?.payload).toBe(`{"token":"${TOKEN}","reason":"error"}`);
  });

  it("says it only once, however many ways out the program takes", async () => {
    const { rig } = await up();
    rig.process.emit("exit", 0);
    rig.process.emit("exit", 0);
    rig.process.emit("SIGINT");
    expect(rig.messages().filter((m) => m.kind === "bye")).toHaveLength(1);
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
  });

  it("sends bye with reason exit when its own code aborts the view", async () => {
    const controller = new AbortController();
    const { rig, result } = await up({ signal: controller.signal });
    await mcpStartUp(rig);
    controller.abort();
    expect(await result).toEqual({ kind: "closed", by: "aborted" });
    expect(rig.messages().at(-1)?.payload).toBe(`{"token":"${TOKEN}","reason":"exit"}`);
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
    expect(rig.process.kills).toEqual([]);
  });

  it("says there was no view when its own code ends the wait before the host has run it", async () => {
    // `closed` says a view ran; the host has not yet called the tool.
    const controller = new AbortController();
    const { rig, result } = await up({ signal: controller.signal });
    controller.abort();
    expect(await result).toEqual({ kind: "no-view", reason: "aborted" });
    expect(rig.messages().at(-1)?.payload).toBe(`{"token":"${TOKEN}","reason":"exit"}`);

    const own = await up();
    own.rig.process.on("SIGTERM", () => {});
    own.rig.process.emit("SIGTERM");
    expect(await own.result).toEqual({ kind: "no-view", reason: "aborted" });
  });

  it("does not take a view the host restarted for one that ran: its open may be refused", async () => {
    const { rig, result } = await up();
    await mcpStartUp(rig);
    rig.say("welcome", welcomePayload(TOKEN_2, ["bundle"], { restart: true }));
    const second = await rig.next("open");
    expect(second.id).toBeGreaterThan(0);
    rig.say("closed", toWire({ reason: "refused", code: "policy" }), second.id);
    expect(await result).toEqual({ kind: "no-view", reason: "refused", code: "policy" });
  });

  it("gives the terminal back without a bye when the signal comes before there is a view", async () => {
    const rig = new Rig();
    const result = serveWith(
      makeServer(),
      { tool: "list_containers", ...FAST, answerTimeout: 5000 },
      rig.environment(),
    );
    await rig.next("hello");
    await rig.settle();
    rig.process.emit("SIGINT");
    expect(rig.process.kills).toEqual([[4242, "SIGINT"]]);
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
    await rig.settle();
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello"]);
    expect(rig.text()).toBe("\x1b[c");
    // The signal ends the process; where it is caught instead the wait is over.
    rig.process.on("SIGINT", () => {});
    void result;
  });

  it("does not leave a signal listener behind when serve ends", async () => {
    const { rig, result } = await up();
    rig.say("closed", toWire({ reason: "user" }));
    await result;
    expect(rig.process.listenerCount("SIGINT")).toBe(0);
    expect(rig.process.listenerCount("SIGTERM")).toBe(0);
    expect(rig.process.listenerCount("exit")).toBe(0);
  });
});
