import { describe, expect, it } from "vitest";

import * as page from "../src/page/index.js";
import { CallError, createBridge, windowChannel } from "../src/page/index.js";
import type { Channel, Context } from "../src/page/index.js";

const CONTEXT: Context = {
  mode: "dark",
  colors: {
    background: "#0d1117",
    foreground: "#c9d1d9",
    dim: "#8b949e",
    accent: "#58a6ff",
    cursor: "#79c0ff",
    border: "#30363d",
    success: "#3fb950",
    error: "#f85149",
    warning: "#d29922",
    info: "#58a6ff",
    selection: "#1f3d5c",
    ansi: [
      "#484f58",
      "#ff7b72",
      "#3fb950",
      "#d29922",
      "#58a6ff",
      "#bc8cff",
      "#39c5cf",
      "#b1bac4",
      "#6e7681",
      "#ffa198",
      "#56d364",
      "#e3b341",
      "#79c0ff",
      "#d2a8ff",
      "#56d4dd",
      "#f0f6fc",
    ],
  },
  size: { width: 800, height: 600 },
  reduceMotion: false,
};

/** A page and its host: what the page posts, and a way for the host to post to the page. */
function rig(options: { connected?: boolean } = {}) {
  const posted: object[] = [];
  let listener: ((data: unknown) => void) | undefined;
  let listening = 0;
  let stopped = 0;
  const channel: Channel = {
    connected: options.connected ?? true,
    post: (message) => posted.push(JSON.parse(JSON.stringify(message))),
    listen: (next) => {
      listener = next;
      listening += 1;
      return () => {
        stopped += 1;
        listener = undefined;
      };
    },
  };
  return {
    bridge: createBridge(channel),
    posted,
    host: (data: unknown) => listener?.(data),
    get listening() {
      return listening;
    },
    get stopped() {
      return stopped;
    },
  };
}

describe("call", () => {
  it("posts a call with the next id and resolves with the result of the response", async () => {
    const { bridge, posted, host } = rig();
    const first = bridge.call("list");
    const second = bridge.call("stop", { id: "web" });
    expect(posted).toEqual([
      // The first use of the bridge asks the host for the context, once.
      { context: {} },
      { call: { id: 1, method: "list" } },
      { call: { id: 2, method: "stop", params: { id: "web" } } },
    ]);
    host({ response: { id: 2, result: { stopped: "web" } } });
    host({ response: { id: 1, result: { containers: [] } } });
    expect(await second).toEqual({ stopped: "web" });
    expect(await first).toEqual({ containers: [] });
  });

  it("rejects with the error of the response: its code, and its words or else the code", async () => {
    const { bridge, host } = rig();
    const failed = bridge.call("stop");
    host({ response: { id: 1, error: { code: "no-such-container", message: "There is none." } } });
    const error = await failed.catch((e: unknown) => e);
    expect(error).toBeInstanceOf(CallError);
    expect(error).toMatchObject({ code: "no-such-container", message: "There is none." });

    const bare = bridge.call("stop");
    host({ response: { id: 2, error: { code: "internal" } } });
    expect(await bare.catch((e: unknown) => e)).toMatchObject({
      code: "internal",
      message: "internal",
    });
  });

  it("is answered once: a second response, or one for a call nobody made, changes nothing", async () => {
    const { bridge, host } = rig();
    const pending = bridge.call("list");
    host({ response: { id: 1, result: { n: 1 } } });
    host({ response: { id: 1, result: { n: 2 } } });
    host({ response: { id: 9, result: {} } });
    expect(await pending).toEqual({ n: 1 });
  });

  it("takes no response that is both a result and an error, or neither (§9.2)", async () => {
    const { bridge, host } = rig();
    let settled = false;
    const pending = bridge.call("list").then(() => (settled = true));
    host({ response: { id: 1, result: {}, error: { code: "x" } } });
    host({ response: { id: 1 } });
    await Promise.resolve();
    expect(settled).toBe(false);
    host({ response: { id: 1, result: {} } });
    await pending;
    expect(settled).toBe(true);
  });

  it("reports progress to the call it belongs to", async () => {
    const { bridge, host } = rig();
    const seen: unknown[] = [];
    const other: unknown[] = [];
    const slow = bridge.call("pull", undefined, { onProgress: (p) => seen.push(p) });
    void bridge.call("push", undefined, { onProgress: (p) => other.push(p) });
    host({ progress: { id: 1, done: 4, total: 10, note: "layer 4 of 10" } });
    host({ progress: { id: 1, done: 5 } });
    host({ progress: { id: 1, done: "many" } });
    host({ progress: { id: 7, done: 1 } });
    host({ response: { id: 1, result: {} } });
    await slow;
    host({ progress: { id: 1, done: 6 } });
    expect(seen).toEqual([{ done: 4, total: 10, note: "layer 4 of 10" }, { done: 5 }]);
    expect(other).toEqual([]);
  });

  it("is given up with a signal: the cancel goes out, the promise rejects, an answer after it is ignored", async () => {
    const { bridge, posted, host } = rig();
    const controller = new AbortController();
    const given = bridge.call("slow", undefined, { signal: controller.signal });
    controller.abort(new Error("enough"));
    expect(await given.catch((e: unknown) => e)).toEqual(new Error("enough"));
    expect(posted.slice(1)).toEqual([{ call: { id: 1, method: "slow" } }, { cancel: { id: 1 } }]);
    host({ response: { id: 1, result: {} } });
    expect(posted).toHaveLength(3);
  });

  it("does not post a call whose signal is already aborted, and does not cancel a call that is over", async () => {
    const { bridge, posted, host } = rig();
    const done = new AbortController();
    const finished = bridge.call("list", undefined, { signal: done.signal });
    host({ response: { id: 1, result: {} } });
    await finished;
    done.abort();
    const before = new AbortController();
    before.abort(new Error("already"));
    await expect(bridge.call("list", undefined, { signal: before.signal })).rejects.toThrow(
      "already",
    );
    expect(posted.slice(1)).toEqual([{ call: { id: 1, method: "list" } }]);
  });
});

describe("subscribe", () => {
  it("posts a subscribe, hears the pushes of its id in order and cancels", () => {
    const { bridge, posted, host } = rig();
    const heard: unknown[] = [];
    const stop = bridge.subscribe("stats", undefined, (value) => heard.push(value));
    bridge.subscribe("other", { every: 200 }, () => heard.push("wrong"));
    expect(posted.slice(1)).toEqual([
      { subscribe: { id: 1, topic: "stats" } },
      { subscribe: { id: 2, topic: "other", params: { every: 200 } } },
    ]);
    host({ push: { id: 1, value: { n: 1 } } });
    host({ push: { id: 1, value: { n: 2 } } });
    host({ push: { id: 9, value: { n: 3 } } });
    stop();
    stop();
    host({ push: { id: 1, value: { n: 4 } } });
    expect(heard).toEqual([{ n: 1 }, { n: 2 }]);
    expect(posted.slice(3)).toEqual([{ cancel: { id: 1 } }]);
  });

  it("is a function that ends it, which is also called cancel: the shape a page written against the hand-made module has", () => {
    const { bridge, posted, host } = rig();
    const heard: unknown[] = [];
    const subscription = bridge.subscribe("stats", undefined, { onPush: (v) => heard.push(v) });
    expect(typeof subscription).toBe("function");
    expect(subscription.cancel).toBeTypeOf("function");
    subscription.cancel();
    host({ push: { id: 1, value: 1 } });
    expect(heard).toEqual([]);
    expect(posted.slice(2)).toEqual([{ cancel: { id: 1 } }]);
  });

  it("tells onError once when the program ends it with an error, then onEnd; nothing is pushed after, and ending it is no cancel", () => {
    const { bridge, posted, host } = rig();
    const events: unknown[] = [];
    const stop = bridge.subscribe("stats", undefined, {
      onPush: (value) => events.push(["push", value]),
      onError: (error) => events.push(["error", error.code, error.message]),
      onEnd: () => events.push(["end"]),
    });
    host({ push: { id: 1, value: "first" } });
    host({ response: { id: 1, error: { code: "not-found", message: "No such topic." } } });
    host({ response: { id: 1, error: { code: "again" } } });
    host({ push: { id: 1, value: "late" } });
    stop();
    expect(events).toEqual([["push", "first"], ["error", "not-found", "No such topic."], ["end"]]);
    expect(posted.slice(1)).toEqual([{ subscribe: { id: 1, topic: "stats" } }]);
  });

  it("tells onEnd, and not onError, when the page cancels it, once", () => {
    const { bridge, host } = rig();
    const events: string[] = [];
    const stop = bridge.subscribe("stats", undefined, {
      onPush: () => events.push("push"),
      onError: () => events.push("error"),
      onEnd: () => events.push("end"),
    });
    stop();
    stop();
    host({ response: { id: 1, error: { code: "late" } } });
    expect(events).toEqual(["end"]);
  });

  it("is not ended by a response that is a result", () => {
    const { bridge, host } = rig();
    const heard: unknown[] = [];
    bridge.subscribe("stats", undefined, (value) => heard.push(value));
    host({ response: { id: 1, result: {} } });
    host({ push: { id: 1, value: "still" } });
    expect(heard).toEqual(["still"]);
  });
});

describe("the context", () => {
  it("is asked for by the first listener, and given to a listener at once when the page has it", () => {
    const { bridge, posted, host } = rig();
    expect(bridge.context()).toBeUndefined();
    const first: Context[] = [];
    const second: Context[] = [];
    bridge.onContext((context) => first.push(context));
    expect(posted).toEqual([{ context: {} }]);
    expect(first).toEqual([]);
    host({ context: CONTEXT });
    expect(first).toEqual([CONTEXT]);
    expect(bridge.context()).toEqual(CONTEXT);

    // The second asks for nothing: the bridge asks once, and has the context already.
    bridge.onContext((context) => second.push(context));
    expect(posted).toHaveLength(1);
    expect(second).toEqual([CONTEXT]);

    const dark = { ...CONTEXT, mode: "light" as const };
    host({ context: dark });
    expect(first).toEqual([CONTEXT, dark]);
    expect(second).toEqual([CONTEXT, dark]);
  });

  it("stops a listener that asks to, and no other", () => {
    const { bridge, host } = rig();
    const kept: Context[] = [];
    const dropped: Context[] = [];
    bridge.onContext((context) => kept.push(context));
    const stop = bridge.onContext((context) => dropped.push(context));
    stop();
    host({ context: CONTEXT });
    expect(kept).toHaveLength(1);
    expect(dropped).toEqual([]);
  });

  it("is asked for by the first use of the bridge, whatever it is, and only then", () => {
    for (const use of [
      (bridge: ReturnType<typeof rig>["bridge"]) => bridge.context(),
      (bridge: ReturnType<typeof rig>["bridge"]) => void bridge.call("a"),
      (bridge: ReturnType<typeof rig>["bridge"]) => bridge.subscribe("t", undefined, () => {}),
    ]) {
      const { bridge, posted } = rig();
      use(bridge);
      expect(posted[0]).toEqual({ context: {} });
      bridge.context();
      expect(posted.filter((m) => "context" in m)).toHaveLength(1);
    }
  });

  it("is read as far as it is of the right kind: members of the wrong kind are left out, missing numbers are 0", () => {
    const { bridge, host } = rig();
    host({});
    bridge.onContext(() => {});
    host({
      context: {
        mode: "light",
        colors: CONTEXT.colors,
        font: 12,
        size: { width: "wide", height: 40 },
        locale: "tr-TR",
        timeZone: 3,
        reduceMotion: "yes",
        access: { connect: ["https://api.example.com", 5] },
      },
    });
    expect(bridge.context()).toEqual({
      mode: "light",
      colors: CONTEXT.colors,
      size: { width: 0, height: 40 },
      locale: "tr-TR",
      reduceMotion: false,
      access: { connect: ["https://api.example.com"] },
    });
  });

  it("is no context without a mode, the roles of the theme and a size", () => {
    const { bridge, host } = rig();
    const heard: Context[] = [];
    bridge.onContext((context) => heard.push(context));
    host({ context: { colors: CONTEXT.colors, size: CONTEXT.size } });
    host({ context: { mode: "sepia", colors: CONTEXT.colors, size: CONTEXT.size } });
    host({ context: { mode: "dark", size: CONTEXT.size } });
    host({ context: { mode: "dark", colors: CONTEXT.colors } });
    expect(heard).toEqual([]);
  });

  it("is a word of the host only as an object with that one member", () => {
    const { bridge, host } = rig();
    const heard: Context[] = [];
    bridge.onContext((context) => heard.push(context));
    host({ context: CONTEXT, close: {} });
    host({ context: "dark" });
    host({ context: null });
    host({ context: [CONTEXT] });
    host({});
    host("context");
    host(null);
    host([{ context: CONTEXT }]);
    expect(heard).toEqual([]);
    expect(bridge.context()).toBeUndefined();
  });
});

describe("the host's own messages", () => {
  it("close and link are single-member objects (§9.6, §9.7)", () => {
    const { bridge, posted } = rig();
    bridge.close();
    bridge.openLink("https://example.com/docs");
    expect(posted).toEqual([{ close: {} }, { link: { url: "https://example.com/docs" } }]);
  });

  it("listen for nothing when a page only closes or opens a link", () => {
    const harness = rig();
    harness.bridge.close();
    harness.bridge.openLink("https://example.com");
    expect(harness.listening).toBe(0);
  });
});

describe("the channel", () => {
  it("is listened to once, on first use", () => {
    const harness = rig();
    expect(harness.listening).toBe(0);
    void harness.bridge.call("a");
    void harness.bridge.call("b");
    harness.bridge.subscribe("t", undefined, () => {});
    harness.bridge.onContext(() => {});
    expect(harness.listening).toBe(1);
  });

  it("ignores what is not one of the six, or has an id that is not a positive integer", async () => {
    const { bridge, host } = rig();
    const heard: unknown[] = [];
    bridge.subscribe("stats", undefined, (value) => heard.push(value));
    host({ push: { id: 0, value: "zero" } });
    host({ push: { id: 1.5, value: "half" } });
    host({ push: { id: "1", value: "text" } });
    host({ push: { value: "none" } });
    host({ unknown: { id: 1 } });
    host({ push: { id: 1, value: "ok" }, extra: {} });
    host({ push: 1 });
    expect(heard).toEqual([]);
    host({ push: { id: 1, value: "ok" } });
    expect(heard).toEqual(["ok"]);
  });
});

describe("windowChannel", () => {
  /** A window that embeds nothing but a parent, which the test plays. */
  function windows() {
    const parent = { posted: [] as { message: unknown; target: string }[] } as {
      posted: { message: unknown; target: string }[];
      postMessage(message: unknown, target: string): void;
    };
    parent.postMessage = (message, target) => parent.posted.push({ message, target });
    const listeners = new Set<(event: { source: unknown; data: unknown }) => void>();
    const win = {
      parent,
      addEventListener: (_type: "message", listener: never) => listeners.add(listener),
      removeEventListener: (_type: "message", listener: never) => listeners.delete(listener),
    };
    const deliver = (source: unknown, data: unknown) => {
      for (const listener of [...listeners]) listener({ source, data });
    };
    return { win, parent, listeners, deliver };
  }

  it("posts to the window that embeds the page", () => {
    const { win, parent } = windows();
    windowChannel(win).post({ close: {} });
    expect(parent.posted).toEqual([{ message: { close: {} }, target: "*" }]);
  });

  it("hears only that window, and stops when asked", () => {
    const { win, parent, listeners, deliver } = windows();
    const heard: unknown[] = [];
    const stop = windowChannel(win).listen((data) => heard.push(data));
    deliver(parent, { a: 1 });
    deliver({}, { b: 2 });
    deliver(undefined, { c: 3 });
    expect(heard).toEqual([{ a: 1 }]);
    stop();
    expect(listeners.size).toBe(0);
    deliver(parent, { d: 4 });
    expect(heard).toHaveLength(1);
  });
});

describe("a page that nothing embeds", () => {
  it("rejects a call with no-host, and posts nothing", async () => {
    const { bridge, posted } = rig({ connected: false });
    const error = await bridge.call("list").catch((e: unknown) => e);
    expect(error).toBeInstanceOf(CallError);
    expect(error).toMatchObject({
      code: "no-host",
      message: "This page is not shown by a terminal",
    });
    expect(posted).toEqual([]);
  });

  it("refuses a subscription with no-host, once, to onError and then onEnd; cancelling it changes nothing", async () => {
    const { bridge, posted } = rig({ connected: false });
    const events: unknown[] = [];
    const stop = bridge.subscribe("stats", undefined, {
      onPush: () => events.push("push"),
      onError: (error) => events.push(["error", error.code]),
      onEnd: () => events.push("end"),
    });
    expect(events).toEqual([]);
    await Promise.resolve();
    expect(events).toEqual([["error", "no-host"], "end"]);
    stop();
    expect(events).toHaveLength(2);
    expect(posted).toEqual([]);
  });

  it("is cancelled before it is refused without being told anything", async () => {
    const { bridge } = rig({ connected: false });
    const events: unknown[] = [];
    bridge
      .subscribe("stats", undefined, {
        onPush: () => {},
        onError: () => events.push("error"),
        onEnd: () => events.push("end"),
      })
      .cancel();
    await Promise.resolve();
    expect(events).toEqual(["end"]);
  });

  it("has no context, closes nothing and opens no link", () => {
    const { bridge, posted } = rig({ connected: false });
    const heard: unknown[] = [];
    bridge.onContext((context) => heard.push(context));
    bridge.close();
    bridge.openLink("https://example.com");
    expect(bridge.context()).toBeUndefined();
    expect(heard).toEqual([]);
    expect(posted).toEqual([]);
  });

  it("is a window that is its own parent", () => {
    const top = {
      parent: undefined as unknown,
      addEventListener: () => {},
      removeEventListener: () => {},
    };
    top.parent = top;
    expect(windowChannel(top as never).connected).toBe(false);
    const embedded = {
      parent: { postMessage: () => {} },
      addEventListener: () => {},
      removeEventListener: () => {},
    };
    expect(windowChannel(embedded as never).connected).toBe(true);
  });
});

describe("the module", () => {
  it("has what a page written against a hand-made bridge uses: call, subscribe, onContext, context, close, openLink and CallError", () => {
    for (const name of ["call", "subscribe", "onContext", "context", "close", "openLink"])
      expect(page, name).toHaveProperty(name);
    expect(new page.CallError("code", "words")).toMatchObject({
      name: "CallError",
      code: "code",
      message: "words",
    });
    expect(new page.CallError("code").message).toBe("code");
  });

  it("is the host's window for the functions it exports: nothing is posted before they are used", () => {
    // Importing this module in a runtime with no window changes nothing, and does not throw.
    expect(page.createBridge).toBeTypeOf("function");
  });
});
