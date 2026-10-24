/**
 * `serve` when the program ends while its view is up: SIGINT, SIGTERM, exit and its own abort
 * (§10.3, §10.4). The program says `bye`, reads and throws away its input until the host's answer
 * (500 ms at most), throws away what is left, and gives the terminal back.
 */

import { describe, expect, it } from "vitest";

import { toWire } from "../src/json.js";
import { callPayload, subscribePayload } from "../src/messages.js";
import { serveWith } from "../src/serve.js";
import type { Subscription } from "../src/serve.js";
import { DA_REPLY, Rig, TOKEN, options, payloadOf, shown, start } from "./support.js";

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
const closed = (reason: string, rest: object = {}) => toWire({ reason, ...rest });
const LEAVE = "\x1b[?1049l";

/** The `bye`s the program wrote. */
const byes = (rig: Rig) => rig.messages().filter((m) => m.kind === "bye");

describe("a signal while the view is up", () => {
  for (const signal of ["SIGINT", "SIGTERM"] as const) {
    it(`${signal} sends bye with the token, reads on until the host's answer, then gives the tty back and lets the signal go on`, async () => {
      const rig = new Rig();
      await shown(rig);
      rig.process.emit(signal);
      const bye = await rig.next("bye");
      expect(bye.payload).toBe(payloadOf("to-host/bye.frame"));
      expect(bye.control).toEqual({});
      await rig.settle();
      // The wait: the modes are still the program's, the screen the view's, the input is read.
      expect(rig.terminal.calls).toEqual(["enter"]);
      expect(rig.text().includes(LEAVE)).toBe(false);
      expect(rig.input.listenerCount("data")).toBeGreaterThan(0);
      expect(rig.process.kills).toEqual([]);
      // What the host wrote before it learned of the bye comes first; the answer is the last frame.
      rig.sayBytes("typed");
      rig.say("need", toWire({ missing: [] }), 2);
      rig.say("closed", closed("program"), bye.id);
      await rig.settle();
      expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
      expect(rig.text().endsWith(LEAVE)).toBe(true);
      expect(rig.input.listenerCount("data")).toBe(0);
      expect(rig.process.listenerCount(signal)).toBe(0);
      expect(rig.process.kills).toEqual([[rig.process.pid, signal]]);
      expect(byes(rig)).toHaveLength(1);
    });
  }

  it("says SIGINT's reason as the fixture has it", async () => {
    const rig = new Rig();
    await shown(rig);
    rig.process.emit("SIGINT");
    const bye = await rig.next("bye");
    expect(JSON.parse(bye.payload)).toEqual({ token: TOKEN, reason: "signal" });
    expect(bye.payload).toBe(payloadOf("to-host/bye.frame"));
  });

  it("goes on when the host does not answer: after the time the unread input is thrown away and the tty given back", async () => {
    const rig = new Rig();
    await shown(rig, { byeTimeout: 60 });
    rig.process.emit("SIGINT");
    await rig.next("bye");
    await sleep(20);
    expect(rig.terminal.calls).toEqual(["enter"]);
    await sleep(100);
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
    expect(rig.process.kills).toEqual([[rig.process.pid, "SIGINT"]]);
  });

  it("is ended by a closed that has no re too: the host's word that the view is gone", async () => {
    const rig = new Rig();
    await shown(rig, { byeTimeout: 5000 });
    rig.process.emit("SIGINT");
    await rig.next("bye");
    rig.say("closed", closed("user"));
    await rig.settle();
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
  });

  it("is not ended by the answer to another message", async () => {
    const rig = new Rig();
    await shown(rig, { byeTimeout: 150 });
    rig.process.emit("SIGINT");
    const bye = await rig.next("bye");
    rig.say("closed", closed("refused", { code: "busy" }), bye.id + 4);
    rig.say("closed", "not json", bye.id);
    await sleep(60);
    expect(rig.terminal.calls).toEqual(["enter"]);
    rig.say("closed", closed("program"), bye.id);
    await rig.settle();
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
  });

  it("keeps listening for the signal until the terminal is given back, so that a forwarded second one cannot end the program between two writes", async () => {
    const rig = new Rig();
    await shown(rig, { byeTimeout: 150 });
    rig.process.emit("SIGINT");
    const bye = await rig.next("bye");
    // The parent forwards the ⌃C the tty already sent: a listener is there, and the second does nothing.
    expect(rig.process.listenerCount("SIGINT")).toBe(1);
    rig.process.emit("SIGINT");
    rig.process.emit("SIGTERM");
    await rig.settle();
    expect(byes(rig)).toHaveLength(1);
    expect(rig.process.kills).toEqual([]);
    rig.say("closed", closed("program"), bye.id);
    await rig.settle();
    expect(rig.process.kills).toEqual([[rig.process.pid, "SIGINT"]]);
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
  });

  it("reads and throws away what comes while it waits, and answers nothing of it", async () => {
    const rig = new Rig();
    let calls = 0;
    let subscription: Subscription | undefined;
    await shown(rig, {
      byeTimeout: 100,
      calls: { list: () => void calls++ },
      topics: { t: (_p, s) => void (subscription = s) },
    });
    rig.say("msg", subscribePayload(2, "t"));
    await rig.settle();
    rig.process.emit("SIGINT");
    const bye = await rig.next("bye");
    // The view's last words, a call, typed characters and the terminal's reply: all of it is read.
    rig.say("msg", callPayload(3, "list"));
    rig.sayBytes("typed" + DA_REPLY);
    await rig.settle();
    expect(calls).toBe(0);
    // Nothing is written for the view after the bye.
    expect(subscription?.push(1)).toBe(false);
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello", "open", "bye"]);
    rig.say("closed", closed("program"), bye.id);
    await rig.settle();
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
  });

  it("leaves the signal to the program's own listener when it has one, and says the view closed by it", async () => {
    const rig = new Rig();
    const { served } = await shown(rig);
    rig.process.on("SIGINT", () => {});
    rig.process.emit("SIGINT");
    const bye = await rig.next("bye");
    rig.say("closed", closed("program"), bye.id);
    expect(await served).toEqual({
      kind: "closed",
      by: "signal",
      view: { kind: "web", version: "1.0" },
    });
    expect(rig.process.kills).toEqual([]);
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
    // The program's listener stays, ours is gone.
    expect(rig.process.listenerCount("SIGINT")).toBe(1);
  });

  it("gives the terminal back without a bye when the signal comes before there is a view", async () => {
    const rig = new Rig();
    const served = serveWith(options({ answerTimeout: 5000 }), rig.environment());
    await rig.next("hello");
    rig.process.on("SIGINT", () => {});
    rig.process.emit("SIGINT");
    expect(await served).toEqual({ kind: "no-view", reason: "aborted" });
    expect(byes(rig)).toEqual([]);
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
  });

  it("says there was no view when the signal comes before the host has taken the open", async () => {
    const rig = new Rig();
    const { served } = await start(rig);
    await rig.next("open");
    rig.process.on("SIGINT", () => {});
    rig.process.emit("SIGINT");
    const bye = await rig.next("bye");
    rig.say("closed", closed("program"), bye.id);
    expect(await served).toEqual({ kind: "no-view", reason: "aborted" });
  });

  it("does not leave a signal listener behind when serve ends", async () => {
    const rig = new Rig();
    const { served } = await shown(rig);
    expect(rig.process.listenerCount("SIGINT")).toBe(1);
    expect(rig.process.listenerCount("SIGTERM")).toBe(1);
    expect(rig.process.listenerCount("exit")).toBe(1);
    rig.say("closed", closed("user"));
    await served;
    expect(rig.process.listenerCount("SIGINT")).toBe(0);
    expect(rig.process.listenerCount("SIGTERM")).toBe(0);
    expect(rig.process.listenerCount("exit")).toBe(0);
  });
});

describe("the process exiting while the view is up", () => {
  it("sends bye with reason exit, throws away the input and gives the tty back at once: nothing runs after exit to wait with", async () => {
    const rig = new Rig();
    await shown(rig);
    rig.process.emit("exit", 0);
    // Synchronously: the handler of `exit` cannot wait.
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
    const bye = await rig.next("bye");
    expect(JSON.parse(bye.payload)).toEqual({ token: TOKEN, reason: "exit" });
    expect(rig.text().endsWith(LEAVE)).toBe(true);
    expect(rig.process.listenerCount("SIGINT")).toBe(0);
  });

  it("sends reason error when it exits with a code", async () => {
    const rig = new Rig();
    await shown(rig);
    rig.process.emit("exit", 1);
    const bye = await rig.next("bye");
    expect(JSON.parse(bye.payload)).toMatchObject({ reason: "error" });
  });

  it("says it only once, however many ways out the program takes", async () => {
    const rig = new Rig();
    await shown(rig);
    rig.process.emit("exit", 0);
    rig.process.emit("exit", 0);
    rig.process.emit("SIGINT");
    await rig.settle();
    expect(byes(rig)).toHaveLength(1);
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
  });

  it("says no bye when no view was ever opened, and does not throw away the input it never asked for", async () => {
    const rig = new Rig();
    const served = serveWith(options({ answerTimeout: 5000 }), rig.environment());
    await rig.next("hello");
    rig.process.emit("exit", 0);
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
    expect(byes(rig)).toEqual([]);
    void served;
  });

  it("does not say bye for a view the host has closed", async () => {
    const rig = new Rig();
    const { served } = await shown(rig);
    rig.say("closed", closed("user"));
    await served;
    rig.process.emit("exit", 0);
    expect(byes(rig)).toEqual([]);
  });
});

describe("the program's own signal", () => {
  it("ends the view with a bye and the wait for its answer, and serve is closed by aborted", async () => {
    const rig = new Rig();
    const controller = new AbortController();
    const { served } = await shown(rig, { signal: controller.signal });
    controller.abort();
    const bye = await rig.next("bye");
    expect(JSON.parse(bye.payload)).toEqual({ token: TOKEN, reason: "exit" });
    expect(rig.terminal.calls).toEqual(["enter"]);
    rig.say("closed", closed("program"), bye.id);
    expect(await served).toEqual({
      kind: "closed",
      by: "aborted",
      view: { kind: "web", version: "1.0" },
    });
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
    expect(rig.process.kills).toEqual([]);
  });

  it("says there was no view when it ends the wait before the host has taken the open", async () => {
    const rig = new Rig();
    const controller = new AbortController();
    const { served } = await start(rig, { signal: controller.signal });
    await rig.next("open");
    controller.abort();
    expect(await served).toEqual({ kind: "no-view", reason: "aborted" });
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
  });

  it("does not run the fallback of a view it ended", async () => {
    const rig = new Rig();
    const controller = new AbortController();
    let ran = 0;
    const { served } = await start(rig, {
      signal: controller.signal,
      fallback: () => void ran++,
    });
    await rig.next("open");
    controller.abort();
    await served;
    expect(ran).toBe(0);
  });
});
