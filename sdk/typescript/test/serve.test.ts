/** `serve`, played against a host that the test is: the sequence of §5 to §10 from the program's side. */

import { describe, expect, it } from "vitest";

import { BundleError, bundleFromFiles } from "../src/bundle.js";
import { encode } from "../src/frame.js";
import { toWire } from "../src/json.js";
import { serveWith } from "../src/serve.js";
import type { NoView, ServeOptions, ServeResult } from "../src/serve.js";
import {
  DA_REPLY,
  EXAMPLE_DIGESTS,
  EXAMPLE_FILES,
  FAST,
  Rig,
  TOKEN,
  TOKEN_2,
  fixture,
  luaBundle,
  offers,
  options,
  payloadOf,
  shown,
  start,
  wasmBundle,
  webBundle,
  welcomePayload,
} from "./support.js";

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const closed = (reason: string, rest: object = {}) => toWire({ reason, ...rest });
const need = (...missing: string[]) => toWire({ missing });

describe("when there is no tty", () => {
  it("sends no frame and takes nothing from the terminal when stdin is not one, and runs the fallback", async () => {
    const rig = new Rig();
    rig.input.isTTY = false;
    const why: NoView[] = [];
    const result = await serveWith(
      options({ fallback: (w) => void why.push(w) }),
      rig.environment(),
    );
    expect(result).toEqual({ kind: "no-view", reason: "no-tty" });
    expect(why).toEqual([{ kind: "no-view", reason: "no-tty" }]);
    expect(rig.written).toEqual([]);
    expect(rig.terminal.calls).toEqual([]);
  });

  it("is a filter's case too: its stdin is data, whatever the option says", async () => {
    const rig = new Rig();
    rig.input.isTTY = false;
    const result = await serveWith(options({ filter: true }), rig.environment());
    expect(result).toMatchObject({ kind: "no-view", reason: "no-tty" });
    expect(rig.written).toEqual([]);
  });

  it("asks on the terminal it is given, and opens its view there as any program does", async () => {
    const rig = new Rig();
    const { served } = await shown(rig, { filter: true });
    rig.say("closed", closed("user"));
    expect(await served).toMatchObject({ kind: "closed", by: "user" });
  });

  it("has nowhere to ask when stdout is not one", async () => {
    const rig = new Rig();
    rig.output.isTTY = false;
    expect(await serveWith(options(), rig.environment())).toEqual({
      kind: "no-view",
      reason: "no-tty",
    });
    expect(rig.terminal.calls).toEqual([]);
  });

  it("is where the modes cannot be set: nothing is written and the fallback runs", async () => {
    const rig = new Rig();
    rig.terminal.failEnter = true;
    let ran = 0;
    const result = await serveWith(options({ fallback: () => void ran++ }), rig.environment());
    expect(result).toEqual({ kind: "no-view", reason: "no-tty" });
    expect(ran).toBe(1);
    expect(rig.written).toEqual([]);
    expect(rig.terminal.calls).toEqual(["enter"]);
  });

  it("does not run the fallback of a wait that was aborted", async () => {
    const rig = new Rig();
    let ran = 0;
    const result = await serveWith(
      options({ signal: AbortSignal.abort(), fallback: () => void ran++ }),
      rig.environment(),
    );
    expect(result).toEqual({ kind: "no-view", reason: "aborted" });
    expect(ran).toBe(0);
    expect(rig.written).toEqual([]);

    const waiting = new Rig();
    const controller = new AbortController();
    const served = serveWith(
      options({ signal: controller.signal, fallback: () => void ran++, answerTimeout: 5000 }),
      waiting.environment(),
    );
    await waiting.next("hello");
    controller.abort();
    expect(await served).toEqual({ kind: "no-view", reason: "aborted" });
    expect(ran).toBe(0);
    // The host has not answered: what it may still write is thrown away (§5.3).
    expect(waiting.terminal.calls).toEqual(["enter", "discard", "restore"]);
  });
});

describe("what a program may offer", () => {
  it("is checked once: the list is read once, whether serve starts on the system or on an environment", async () => {
    const rig = new Rig();
    const base = options({ signal: AbortSignal.abort() });
    let reads = 0;
    const counted = Object.defineProperty({ ...base }, "views", {
      enumerable: true,
      get: () => {
        reads += 1;
        return base.views;
      },
    });
    await serveWith(counted, rig.environment());
    expect(reads).toBe(1);
  });

  const rejected = async (
    views: ServeOptions["views"],
    error: ErrorConstructor | typeof BundleError,
  ) => {
    const rig = new Rig();
    await expect(serveWith(options({ views }), rig.environment())).rejects.toThrow(error);
    // Nothing was asked of the terminal.
    expect(rig.terminal.calls).toEqual([]);
    expect(rig.written).toEqual([]);
  };

  it("is at least one kind", async () => {
    await rejected([], TypeError);
  });

  it("is a kind of the table, listed once", async () => {
    await rejected([{ kind: "html" as never, bundle: webBundle() }], TypeError);
    await rejected(
      [
        { kind: "web", bundle: webBundle() },
        { kind: "web", bundle: webBundle() },
      ],
      TypeError,
    );
  });

  it("names the versions of a kit view, in the form of §5.5, each once", async () => {
    await rejected([{ kind: "wasm", bundle: wasmBundle() }], TypeError);
    await rejected([{ kind: "wasm", versions: [], bundle: wasmBundle() }], TypeError);
    await rejected([{ kind: "wasm", versions: ["0.5.1"], bundle: wasmBundle() }], TypeError);
    await rejected([{ kind: "wasm", versions: ["0.5", "0.5"], bundle: wasmBundle() }], TypeError);
  });

  it("has an entry of the type of its kind", async () => {
    await rejected([{ kind: "wasm", versions: ["0.5"], bundle: webBundle() }], BundleError);
    await rejected([{ kind: "web", bundle: wasmBundle() }], BundleError);
    await rejected([{ kind: "lua", versions: ["0.5"], bundle: wasmBundle() }], BundleError);
  });

  it("asks to reach something only for a page", async () => {
    await rejected(
      [{ kind: "wasm", versions: ["0.5"], bundle: wasmBundle(), access: { connect: [] } }],
      TypeError,
    );
  });

  it("is checked when a function makes the bundle, for the version the host chose", async () => {
    const rig = new Rig();
    const served = serveWith(
      options({ views: [{ kind: "wasm", versions: ["0.5"], bundle: () => webBundle() }] }),
      rig.environment(),
    );
    const hello = await rig.next("hello");
    rig.say("welcome", welcomePayload(TOKEN, undefined, offers("wasm")), hello.id);
    rig.sayBytes(DA_REPLY);
    await expect(served).rejects.toThrow(BundleError);
    // The terminal is given back, and no open was written.
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello"]);
  });
});

describe("with no host", () => {
  it("concludes it when the terminal answers CSI c first, and gives the terminal back before the fallback", async () => {
    const rig = new Rig();
    let calls: string[] = [];
    const served = serveWith(
      options({ fallback: () => void (calls = [...rig.terminal.calls]) }),
      rig.environment(),
    );
    await rig.next("hello");
    await rig.settle();
    expect(rig.text()).toBe("\x1b[c");
    rig.sayBytes(DA_REPLY);
    expect(await served).toEqual({ kind: "no-view", reason: "no-host" });
    expect(calls).toEqual(["enter", "restore"]);
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello"]);
    expect(rig.process.listenerCount("SIGINT")).toBe(0);
  });

  it("does not believe a welcome that does not answer its hello, nor one that comes after the reply", async () => {
    const rig = new Rig();
    const served = serveWith(options(), rig.environment());
    const hello = await rig.next("hello");
    rig.say("welcome", welcomePayload(TOKEN), hello.id + 5);
    rig.say("welcome", welcomePayload(TOKEN));
    await rig.settle();
    rig.sayBytes(DA_REPLY);
    rig.say("welcome", welcomePayload(TOKEN), hello.id);
    expect(await served).toEqual({ kind: "no-view", reason: "no-host" });
    await rig.settle();
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello"]);
  });

  it("concludes it after the time, and reads on a while so that a late reply does not reach the shell", async () => {
    const rig = new Rig();
    const served = serveWith(
      options({ answerTimeout: 30, lateReplyGrace: 2000 }),
      rig.environment(),
    );
    await rig.next("hello");
    await sleep(100);
    // The time is up and the terminal is not given back yet: the reply may still come.
    expect(rig.terminal.calls).toEqual(["enter"]);
    rig.sayBytes(DA_REPLY);
    expect(await served).toEqual({ kind: "no-view", reason: "no-host" });
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
  });

  it("gives up reading when the grace is over", async () => {
    const rig = new Rig();
    const served = serveWith(options({ answerTimeout: 20, lateReplyGrace: 30 }), rig.environment());
    expect(await served).toEqual({ kind: "no-view", reason: "no-host" });
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
  });

  it("concludes it from a host that shares no version", async () => {
    const rig = new Rig();
    const served = serveWith(options(), rig.environment());
    const hello = await rig.next("hello");
    rig.say("welcome", payloadOf("to-program/welcome-unsupported.frame"), hello.id);
    expect(await served).toEqual({ kind: "no-view", reason: "no-common-version" });
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello"]);
  });

  it("concludes it at once from a host that opens none of the kinds it can show, and does not wait", async () => {
    const rig = new Rig();
    const { served } = await start(rig, { answerTimeout: 5000 }, { views: [] });
    const began = Date.now();
    expect(await served).toEqual({ kind: "no-view", reason: "no-common-view" });
    expect(Date.now() - began).toBeLessThan(1000);
    // No open, and the screen is the program's own: the alternate screen was not taken.
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello"]);
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
  });

  it("concludes it from a host that opens a kind the program did not offer", async () => {
    const rig = new Rig();
    const { served } = await start(rig, {}, { views: offers(["wasm", "0.5"], "lua") });
    expect(await served).toEqual({ kind: "no-view", reason: "no-common-view" });
  });

  it("concludes it from a host that does not take bundles: every kind arrives as one (§8.1)", async () => {
    const rig = new Rig();
    const { served } = await start(rig, {}, { features: [] });
    expect(await served).toEqual({ kind: "no-view", reason: "no-common-view" });
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello"]);
  });
});

describe("the hello", () => {
  it("lists the one kind of a page, with its version", async () => {
    const rig = new Rig();
    const { served, hello } = await start(rig);
    expect(hello.payload).toBe(payloadOf("to-host/hello.frame"));
    rig.say("closed", closed("user"), 2);
    await served;
  });

  it("lists the kinds in the program's order, each with its versions in the program's order", async () => {
    const rig = new Rig();
    const { served, hello } = await start(rig, {
      views: [
        { kind: "wasm", versions: ["0.5", "0.4"], bundle: wasmBundle() },
        { kind: "lua", versions: ["0.5"], bundle: luaBundle() },
        { kind: "web", bundle: webBundle() },
      ],
    });
    expect(hello.payload).toBe(payloadOf("to-host/hello-views.frame"));
    expect(hello.control).toEqual({});
    // (The host here answers with the web page alone; the program's order is what the hello says.)
    rig.say("closed", closed("user"), 2);
    await served;
  });

  it("is followed by CSI c, in the same turn, and by nothing else before the answer", async () => {
    const rig = new Rig();
    const served = serveWith(options(), rig.environment());
    await rig.next("hello");
    await rig.settle();
    expect(rig.text()).toBe("\x1b[c");
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello"]);
    rig.sayBytes(DA_REPLY);
    await served;
  });
});

describe("the open", () => {
  it("takes the modes first, asks, takes the alternate screen, and opens with the token and the names", async () => {
    const rig = new Rig();
    const { served } = await start(rig);
    const open = await rig.next("open");
    expect(rig.terminal.calls).toEqual(["enter"]);
    expect(rig.text()).toBe("\x1b[c\x1b[?1049h");
    const order = rig.written.map((w) => (w.type === "message" ? w.message.kind : "text"));
    expect(order[0]).toBe("hello");
    expect(order.at(-1)).toBe("open");
    // The bytes of the specification's own example (§6.3).
    expect(open.payload).toBe(payloadOf("to-host/open-web.frame"));
    expect(open.control).toEqual({});
    rig.say("closed", closed("user"));
    await served;
  });

  it("opens the first kind of its list the host opens, at the first version of it both speak", async () => {
    const rig = new Rig();
    const asked: string[] = [];
    const { served } = await start(
      rig,
      {
        program: "demopen",
        views: [
          {
            kind: "wasm",
            versions: ["0.6", "0.5", "0.4"],
            bundle: (version) => (asked.push(version), wasmBundle()),
          },
          { kind: "lua", versions: ["0.5"], bundle: luaBundle() },
          { kind: "web", bundle: webBundle() },
        ],
      },
      // The host of session/several-versions: wasm at 0.5 and 0.4, no lua, web.
      { views: offers(["wasm", "0.5", "0.4"], "web") },
    );
    const open = await rig.next("open");
    expect(open.payload).toBe(payloadOf("to-host/open-wasm.frame"));
    // The bundle is made for the version chosen, and for no other.
    expect(asked).toEqual(["0.5"]);
    rig.say("closed", closed("user"));
    expect(await served).toEqual({
      kind: "closed",
      by: "user",
      view: { kind: "wasm", version: "0.5" },
    });
  });

  it("goes on to the next kind when the host has none of a version the program speaks", async () => {
    const rig = new Rig();
    const { served } = await start(
      rig,
      {
        views: [
          { kind: "wasm", versions: ["0.9"], bundle: wasmBundle() },
          { kind: "lua", versions: ["0.5"], bundle: luaBundle() },
          { kind: "web", bundle: webBundle() },
        ],
      },
      // session/no-common-version: the host's wasm is at 0.4 and 0.5.
      { views: offers(["wasm", "0.4", "0.5"], "lua", "web") },
    );
    const open = await rig.next("open");
    expect(JSON.parse(open.payload)).toMatchObject({ view: "lua", version: "0.5" });
    rig.say("closed", closed("user"));
    expect(await served).toMatchObject({ view: { kind: "lua", version: "0.5" } });
  });

  it("opens a page without the access of a kit view, and a kit view without any", async () => {
    const rig = new Rig();
    const { served } = await start(
      rig,
      { views: [{ kind: "lua", versions: ["0.5"], bundle: luaBundle() }] },
      { views: offers("lua") },
    );
    const open = await rig.next("open");
    expect(Object.keys(JSON.parse(open.payload) as object)).toEqual([
      "token",
      "view",
      "version",
      "program",
      "title",
      "bundle",
    ]);
    rig.say("closed", closed("user"));
    await served;
  });

  it("writes the names and the icons it was given, in the wire's order", async () => {
    const rig = new Rig();
    const src =
      "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg==";
    const { served } = await start(
      rig,
      {
        program: "demopen",
        views: [{ kind: "wasm", versions: ["0.5"], bundle: wasmBundle() }],
        icons: [{ src, mimeType: "image/png", sizes: ["1x1"], theme: "dark" }],
      },
      { views: offers("wasm") },
    );
    const open = await rig.next("open");
    expect(open.payload).toBe(payloadOf("to-host/open-icons.frame"));
    rig.say("closed", closed("user"));
    await served;
  });

  it("leaves out the names it was not given", async () => {
    const rig = new Rig();
    const { program: _program, title: _title, ...bare } = options();
    const served = serveWith(bare, rig.environment());
    const hello = await rig.next("hello");
    rig.say("welcome", welcomePayload(TOKEN), hello.id);
    const open = await rig.next("open");
    const body = JSON.parse(open.payload) as Record<string, unknown>;
    expect("program" in body || "title" in body).toBe(false);
    rig.say("closed", closed("user"));
    await served;
  });
});

describe("the files", () => {
  const digest = EXAMPLE_DIGESTS;
  const text = (m: { payload: string }) => Buffer.from(m.payload, "base64").toString();

  it("sends only the files the host lacks, in the order it names them, each as a file with its digest", async () => {
    const rig = new Rig();
    const { served } = await start(rig);
    const open = await rig.next("open");
    rig.say("need", need(digest["app.js"], digest["style.css"]), open.id);
    const first = await rig.next("file");
    const second = await rig.next("file");
    expect(first.control).toEqual({ d: digest["app.js"] });
    expect(text(first)).toBe(EXAMPLE_FILES["app.js"]);
    expect(second.control).toEqual({ d: digest["style.css"] });
    expect(text(second)).toBe(EXAMPLE_FILES["style.css"]);
    await rig.settle();
    expect(rig.messages().filter((m) => m.kind === "file")).toHaveLength(2);
    rig.say("closed", closed("user"));
    await served;
  });

  it("answers the need of the fixtures with the file of the fixtures", async () => {
    const rig = new Rig();
    const { served } = await start(rig);
    const open = await rig.next("open");
    expect(open.id).toBe(2);
    // `need` of the specification: id 2, re 2, the digest of app.js.
    rig.sayBytes(fixture("to-program/need.frame"));
    const file = await rig.next("file");
    const chunked = payloadOf("to-host/file-chunked.frame");
    expect(file.control).toEqual({ d: digest["app.js"] });
    expect(file.payload).toBe(chunked);
    rig.say("closed", closed("user"));
    await served;
  });

  it("sends no file when the host holds them all, or asks for one that is not in the bundle, or answers another open", async () => {
    const rig = new Rig();
    const { served } = await start(rig);
    const open = await rig.next("open");
    rig.say("need", need(digest["app.js"]), open.id + 7);
    rig.say("need", need("0".repeat(64), "not a digest"), open.id);
    await rig.settle();
    rig.say("need", need(), open.id);
    rig.say("need", need(digest["app.js"]), open.id);
    await rig.settle();
    expect(rig.messages().filter((m) => m.kind === "file")).toEqual([]);
    rig.say("closed", closed("user"));
    await served;
  });

  it("sends a file once however many times it is named, and answers an open once", async () => {
    const rig = new Rig();
    const { served } = await start(rig);
    const open = await rig.next("open");
    rig.say("need", need(digest["app.js"], digest["app.js"]), open.id);
    rig.say("need", need(digest["style.css"]), open.id);
    await rig.settle();
    expect(rig.messages().filter((m) => m.kind === "file")).toHaveLength(1);
    rig.say("closed", closed("user"));
    await served;
  });

  it("writes a file longer than a frame in frames of the host's limit, whole and in order", async () => {
    const rig = new Rig();
    const big = Buffer.alloc(100_000);
    for (let i = 0; i < big.length; i++) big[i] = (i * 7) % 251;
    const bundle = bundleFromFiles([
      { path: "index.html", data: Buffer.from("<p>") },
      { path: "big.bin", data: big },
    ]);
    const { served } = await start(rig, { views: [{ kind: "web", bundle }] });
    const open = await rig.next("open");
    const wanted = bundle.manifest.files[1]?.sha256 ?? "";
    rig.say("need", need(wanted), open.id);
    const file = await rig.next("file");
    // The rig decodes with the limit of 4096 bytes a frame: a longer one would have thrown.
    expect(Buffer.from(file.payload, "base64").equals(big)).toBe(true);
    rig.say("closed", closed("user"));
    await served;
  });
});

describe("when the host ends the view", () => {
  const view = { kind: "web", version: "1.0" };

  it("takes the person closing it as the view's end, gives the terminal back, and says no bye", async () => {
    const rig = new Rig();
    const { served } = await shown(rig);
    rig.sayBytes(fixture("to-program/closed-user.frame"));
    expect(await served).toEqual({ kind: "closed", by: "user", view });
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
    expect(rig.text()).toBe("\x1b[c\x1b[?1049h\x1b[?1049l");
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello", "open"]);
    expect(rig.process.listenerCount("SIGINT")).toBe(0);
    expect(rig.process.listenerCount("SIGTERM")).toBe(0);
    expect(rig.process.listenerCount("exit")).toBe(0);
  });

  it("takes the page asking to be closed, and the program's own dismiss, as the end", async () => {
    const page = new Rig();
    const first = await shown(page);
    page.say("closed", closed("view"));
    expect(await first.served).toEqual({ kind: "closed", by: "view", view });

    const dismissed = new Rig();
    const second = await shown(dismissed);
    dismissed.say("closed", closed("program"));
    expect(await second.served).toEqual({ kind: "closed", by: "program", view });
  });

  it("tells what a view the host draws itself failed with: the words, and that it ran", async () => {
    const rig = new Rig();
    const { served } = await shown(
      rig,
      {
        views: [{ kind: "lua", versions: ["0.5"], bundle: luaBundle() }],
      },
      { views: offers("lua") },
    );
    rig.sayBytes(fixture("to-program/closed-error-view.frame"));
    expect(await served).toEqual({
      kind: "closed",
      by: "error",
      view: { kind: "lua", version: "0.5" },
      code: "view",
      message: "script:12: attempt to index nil with 'x'",
    });
  });

  it("takes a failure before the view ran as no view, and runs the fallback with the code", async () => {
    const rig = new Rig();
    const why: NoView[] = [];
    const { served } = await shown(rig, { fallback: (w) => void why.push(w) });
    rig.sayBytes(fixture("to-program/closed-error.frame"));
    expect(await served).toEqual({ kind: "no-view", reason: "failed", code: "bundle" });
    expect(why).toEqual([{ kind: "no-view", reason: "failed", code: "bundle" }]);
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
  });

  it("takes a failure after the view asked something as the view's end", async () => {
    const rig = new Rig();
    const { served } = await shown(rig, { calls: { list: () => ({}) } });
    rig.say("msg", toWire({ call: { id: 1, method: "list" } }));
    await rig.next("msg");
    rig.say("closed", closed("error", { code: "internal" }), 2);
    expect(await served).toEqual({ kind: "closed", by: "error", view, code: "internal" });
  });

  it("runs the fallback of a refusal, with the code, once the terminal is back", async () => {
    const rig = new Rig();
    let calls: string[] = [];
    let seen: NoView | undefined;
    const { served } = await start(rig, {
      fallback: (why) => {
        calls = [...rig.terminal.calls];
        seen = why;
      },
    });
    const open = await rig.next("open");
    rig.sayBytes(fixture("to-program/closed-refused.frame"));
    expect(open.id).toBe(2);
    expect(await served).toEqual({ kind: "no-view", reason: "refused", code: "token" });
    expect(calls).toEqual(["enter", "restore"]);
    expect(seen).toEqual({ kind: "no-view", reason: "refused", code: "token" });
    expect(rig.text().endsWith("\x1b[?1049l")).toBe(true);
  });

  it("reads a reason it does not know as an error", async () => {
    const rig = new Rig();
    const { served } = await shown(rig);
    rig.say("closed", closed("someday"));
    expect(await served).toMatchObject({ kind: "no-view", reason: "failed" });
  });

  it("ignores a closed that answers an open that is not its own, and a kind it does not know", async () => {
    const rig = new Rig();
    const { served, open } = await shown(rig);
    rig.say("closed", closed("refused", { code: "busy" }), open.id + 3);
    rig.say("zzz", "{}");
    rig.say("closed", "not json");
    rig.say("welcome", welcomePayload(TOKEN_2));
    await rig.settle();
    expect(rig.terminal.calls).toEqual(["enter"]);
    rig.say("closed", closed("user"));
    expect(await served).toMatchObject({ kind: "closed", by: "user" });
  });

  it("goes on through typed characters, the terminal's replies and a stream cut at every byte", async () => {
    const rig = new Rig();
    const { served } = await start(rig, { calls: { list: () => ({ ok: true }) } });
    const open = await rig.next("open");
    rig.sayBytes("ls\r");
    const bytes = Buffer.concat([
      Buffer.from(DA_REPLY),
      fixture("to-program/need-empty.frame"),
      Buffer.from("typed"),
      encode("to-program", {
        kind: "msg",
        id: 900,
        payload: toWire({ call: { id: 1, method: "list" } }),
      }),
    ]);
    for (const byte of bytes) rig.sayBytes(Uint8Array.of(byte));
    const answer = await rig.next("msg");
    expect(JSON.parse(answer.payload)).toEqual({ response: { id: 1, result: { ok: true } } });
    expect(open.id).toBe(2);
    rig.say("closed", closed("user"));
    await served;
  });
});

describe("the result", () => {
  it("says which kind and version of view ran", async () => {
    const rig = new Rig();
    const { served } = await shown(rig);
    rig.say("closed", closed("user"));
    const result: ServeResult = await served;
    expect(result).toEqual({ kind: "closed", by: "user", view: { kind: "web", version: "1.0" } });
  });

  it("names a kit view by its kind and the version chosen", async () => {
    const rig = new Rig();
    const { served } = await shown(
      rig,
      { views: [{ kind: "wasm", versions: ["0.5"], bundle: wasmBundle() }] },
      { views: offers("wasm") },
    );
    rig.say("closed", closed("user"));
    expect(await served).toMatchObject({ view: { kind: "wasm", version: "0.5" } });
  });
});
