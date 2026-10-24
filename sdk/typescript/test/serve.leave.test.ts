/**
 * `serve` when the program ends while its view is up: SIGINT, SIGTERM, exit and its own abort
 * (§10.3, §10.4). The program says `bye`, reads and throws away its input until the host's answer
 * (500 ms at most), throws away what is left, and gives the terminal back.
 */

import { constants } from "node:os";
import { describe, expect, it } from "vitest";

import { toWire } from "../src/json.js";
import { callPayload, subscribePayload } from "../src/messages.js";
import { serveWith } from "../src/serve.js";
import type { Subscription } from "../src/serve.js";
import { encode } from "../src/frame.js";
import {
  DA_REPLY,
  Rig,
  TOKEN,
  options,
  payloadOf,
  shown,
  start,
  welcomePayload,
} from "./support.js";

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

  it("ends the process with 128 and the signal's number when it may not signal itself", async () => {
    // Deno refuses `kill` to a program that may not run programs; the program still ends, with the
    // status a shell reports for a program that a signal ended.
    for (const [signal, code] of [
      ["SIGINT", 130],
      ["SIGTERM", 143],
    ] as const) {
      const rig = new Rig();
      rig.process.failKill = true;
      await shown(rig, { byeTimeout: 60 });
      rig.process.emit(signal);
      await rig.next("bye");
      await sleep(120);
      expect(rig.process.kills).toEqual([]);
      expect(rig.process.exits).toEqual([code]);
      expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
    }
  });

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

  it("gives the terminal back without a bye when the signal comes before there is a view, after it has thrown away what the host is still answering (§5.3)", async () => {
    const rig = new Rig();
    const served = serveWith(options({ answerTimeout: 5000, byeTimeout: 5000 }), rig.environment());
    const hello = await rig.next("hello");
    rig.process.on("SIGINT", () => {});
    rig.process.emit("SIGINT");
    await rig.settle();
    // The terminal's answer has not come: the program reads on, and nothing is given back yet.
    expect(rig.terminal.calls).toEqual(["enter"]);
    // A late `welcome` and the reply to `CSI c` are what it waits for, and throws away.
    rig.say("welcome", welcomePayload(TOKEN), hello.id);
    rig.sayBytes(DA_REPLY);
    expect(await served).toEqual({ kind: "no-view", reason: "aborted" });
    expect(byes(rig)).toEqual([]);
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello"]);
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
  });

  it("goes on when the terminal does not answer: after the time the unread input is thrown away and the tty given back", async () => {
    const rig = new Rig();
    const served = serveWith(options({ answerTimeout: 5000, byeTimeout: 60 }), rig.environment());
    await rig.next("hello");
    rig.process.on("SIGINT", () => {});
    const started = Date.now();
    rig.process.emit("SIGINT");
    expect(await served).toEqual({ kind: "no-view", reason: "aborted" });
    expect(Date.now() - started).toBeGreaterThanOrEqual(50);
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
  });

  it("does not take the terminal's answer to CSI c for the end of the wait for a host: the program is leaving", async () => {
    const rig = new Rig();
    const served = serveWith(options({ answerTimeout: 5000, byeTimeout: 5000 }), rig.environment());
    await rig.next("hello");
    rig.process.on("SIGINT", () => {});
    rig.process.emit("SIGINT");
    rig.sayBytes(DA_REPLY);
    // Aborted, not "no host": the signal ended it.
    expect(await served).toEqual({ kind: "no-view", reason: "aborted" });
  });

  it("does not wait for an answer when the terminal has answered already", async () => {
    const rig = new Rig();
    const { served } = await shown(rig);
    rig.process.on("SIGINT", () => {});
    rig.process.emit("SIGINT");
    const bye = await rig.next("bye");
    rig.say("closed", closed("program"), bye.id);
    await served;
    // Only the wait for the answer to the bye: the reply to CSI c was in long ago.
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
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
  it("sends bye with reason exit, reads the tty for the host's answer without the event loop, throws away the input and gives the tty back, all at once: nothing runs after exit", async () => {
    const rig = new Rig();
    await shown(rig);
    rig.process.emit("exit", 0);
    // Synchronously: the handler of `exit` cannot wait for the event loop.
    expect(rig.terminal.calls).toEqual(["enter", "wait", "discard", "restore"]);
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
    expect(rig.terminal.calls).toEqual(["enter", "wait", "discard", "restore"]);
  });

  it("says no bye when no view was ever opened, and reads and throws away the host's answer to the hello that is still on its way (§5.3)", async () => {
    const rig = new Rig();
    const served = serveWith(options({ answerTimeout: 5000 }), rig.environment());
    await rig.next("hello");
    const read: string[] = [];
    rig.terminal.onWait = (feed) => {
      // The host's answer, then the terminal's reply to `CSI c`: the program stops at the reply.
      const welcome = encode("to-program", {
        kind: "welcome",
        id: 1,
        re: 1,
        payload: welcomePayload(TOKEN),
      });
      read.push(String(feed(welcome)), String(feed(Buffer.from(DA_REPLY, "latin1"))));
    };
    rig.process.emit("exit", 0);
    expect(read).toEqual(["false", "true"]);
    expect(rig.terminal.calls).toEqual(["enter", "wait", "discard", "restore"]);
    expect(byes(rig)).toEqual([]);
    void served;
  });

  it("ends the wait at the closed that answers the bye, and at no other word of the host", async () => {
    const rig = new Rig();
    await shown(rig);
    // The ids the program used so far; the bye takes the next.
    const byeId = Math.max(...rig.messages().map((m) => m.id)) + 1;
    const closedWith = (id: number, re: number) =>
      encode("to-program", { kind: "closed", id, re, payload: closed("program") });
    const ends: boolean[] = [];
    rig.terminal.onWait = (feed) => {
      ends.push(feed(Buffer.from("typed"))); // typed ahead: thrown away
      ends.push(feed(closedWith(40, byeId + 3))); // the answer to some other message
      ends.push(
        feed(
          encode("to-program", { kind: "need", id: 41, re: 2, payload: toWire({ missing: [] }) }),
        ),
      );
      ends.push(feed(closedWith(42, byeId))); // the answer
    };
    rig.process.emit("exit", 0);
    expect(ends).toEqual([false, false, false, true]);
    expect(rig.terminal.calls).toEqual(["enter", "wait", "discard", "restore"]);
  });

  it("ends the wait at a closed that has no re too: the host's own word that the view is gone", async () => {
    const rig = new Rig();
    await shown(rig);
    const ends: boolean[] = [];
    rig.terminal.onWait = (feed) => {
      ends.push(feed(encode("to-program", { kind: "closed", id: 40, payload: closed("user") })));
    };
    rig.process.emit("exit", 0);
    expect(ends).toEqual([true]);
  });

  it("gives the terminal back in the end when the host never answers: the wait is the tty's", async () => {
    const rig = new Rig();
    await shown(rig);
    rig.terminal.onWait = undefined;
    rig.process.emit("exit", 1);
    expect(rig.terminal.calls).toEqual(["enter", "wait", "discard", "restore"]);
    expect(rig.text().endsWith(LEAVE)).toBe(true);
  });

  it("waits for the time bye is given, which the options set", async () => {
    const rig = new Rig();
    await shown(rig, { byeTimeout: 123 });
    const waited: number[] = [];
    const wait = rig.terminal.waitFor.bind(rig.terminal);
    rig.terminal.waitFor = (feed, timeout) => {
      waited.push(timeout);
      wait(feed, timeout);
    };
    rig.process.emit("exit", 0);
    expect(waited).toEqual([123]);
  });

  it("does not wait at all when the terminal has answered the hello already: there is nothing on its way", async () => {
    const rig = new Rig();
    const { served } = await shown(rig);
    rig.say("closed", closed("user"));
    await served;
    rig.process.emit("exit", 0);
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
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

const ENTER = "\x1b[?1049h";
/** The signal that cannot be caught, by its number: what the program stops itself with. */
const SIGSTOP = constants.signals.SIGSTOP;
const count = (text: string, part: string) => text.split(part).length - 1;

describe("⌃Z while the view is up", () => {
  /** The program, stopped by ⌃Z: the host has answered its `dismiss` or `bye`, and the program is continued. */
  async function stopped(
    rig: Rig,
    answer: Parameters<typeof shown>[2] = {},
    more: Parameters<typeof shown>[1] = {},
  ) {
    const started = await shown(rig, more, answer);
    rig.process.emit("SIGTSTP");
    return started;
  }

  it("closes the view with a dismiss, gives the terminal back, stops, and asks for the view again when it is continued", async () => {
    const rig = new Rig();
    const { served } = await stopped(rig);
    const dismiss = await rig.next("dismiss");
    expect(JSON.parse(dismiss.payload)).toEqual({ token: TOKEN });
    await rig.settle();
    // Until the host answers the program is as it was: the view is the host's, the modes the program's.
    expect(rig.terminal.calls).toEqual(["enter"]);
    expect(rig.process.kills).toEqual([]);
    expect(count(rig.text(), LEAVE)).toBe(0);

    rig.say("closed", closed("program"), dismiss.id);
    const second = await rig.next("hello");
    // The screen and the modes are the shell's, what the host wrote and was not read is not left for
    // it, the program stopped (the signal that cannot be caught, returning when continued), and
    // took the modes again.
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore", "enter"]);
    expect(rig.text().endsWith(LEAVE)).toBe(true);
    expect(rig.process.kills).toEqual([[rig.process.pid, SIGSTOP]]);

    // It asks for a view as it did the first time, and runs it.
    rig.say("welcome", welcomePayload("Zk3mP8rD5uYb1Xe6HqS0vg"), second.id);
    const open = await rig.next("open");
    expect(JSON.parse(open.payload)).toMatchObject({
      token: "Zk3mP8rD5uYb1Xe6HqS0vg",
      view: "web",
    });
    rig.say("need", toWire({ missing: [] }), open.id);
    await rig.settle();
    expect(count(rig.text(), ENTER)).toBe(2);
    rig.say("closed", closed("user"));
    expect(await served).toEqual({
      kind: "closed",
      by: "user",
      view: { kind: "web", version: "1.0" },
    });
    expect(rig.terminal.calls.at(-1)).toBe("restore");
    // Only the one view was ended by the program's word; no bye was ever said.
    expect(byes(rig)).toEqual([]);
  });

  it("says bye with the reason signal to a host that does not list dismiss", async () => {
    const rig = new Rig();
    await stopped(rig, { features: ["bundle"] });
    const bye = await rig.next("bye");
    expect(JSON.parse(bye.payload)).toEqual({ token: TOKEN, reason: "signal" });
    expect(rig.messages().some((m) => m.kind === "dismiss")).toBe(false);
    rig.say("closed", closed("program"), bye.id);
    await rig.next("hello");
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore", "enter"]);
    expect(rig.process.kills).toEqual([[rig.process.pid, SIGSTOP]]);
  });

  it("goes on when the host does not answer: after the time of a bye the terminal is given back", async () => {
    const rig = new Rig();
    await stopped(rig, {}, { byeTimeout: 50 });
    await rig.next("dismiss");
    await sleep(120);
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore", "enter"]);
    expect(rig.process.kills).toEqual([[rig.process.pid, SIGSTOP]]);
    await rig.next("hello");
  });

  it("ends what the view asked: a call is cancelled and a subscription stops", async () => {
    const rig = new Rig();
    let call: AbortSignal | undefined;
    let sub: AbortSignal | undefined;
    let cleaned = 0;
    await shown(rig, {
      calls: {
        slow: (_params, context) => {
          call = context.signal;
          return new Promise(() => {});
        },
      },
      topics: {
        stats: (_params, subscription) => {
          sub = subscription.signal;
          return () => void (cleaned += 1);
        },
      },
    });
    rig.sayView({ call: { id: 1, method: "slow" } });
    rig.sayView({ subscribe: { id: 2, topic: "stats" } });
    await rig.settle();
    rig.process.emit("SIGTSTP");
    expect(call?.aborted).toBe(true);
    expect(sub?.aborted).toBe(true);
    expect(cleaned).toBe(1);
  });

  it("gives the modes back and takes them again, and says nothing to the host, before there is a view", async () => {
    const rig = new Rig();
    const served = serveWith(options({ answerTimeout: 5000 }), rig.environment());
    const hello = await rig.next("hello");
    rig.process.emit("SIGTSTP");
    expect(rig.terminal.calls).toEqual(["enter", "restore", "enter"]);
    expect(rig.process.kills).toEqual([[rig.process.pid, SIGSTOP]]);
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello"]);
    // The wait for the host goes on.
    rig.say("welcome", welcomePayload(TOKEN), hello.id);
    const open = await rig.next("open");
    rig.say("need", toWire({ missing: [] }), open.id);
    rig.say("closed", closed("user"));
    expect((await served).kind).toBe("closed");
  });

  it("does not stop, and asks for the view again, when the program may not signal itself", async () => {
    const rig = new Rig();
    rig.process.failKill = true;
    await stopped(rig);
    const dismiss = await rig.next("dismiss");
    rig.say("closed", closed("program"), dismiss.id);
    await rig.next("hello");
    expect(rig.process.kills).toEqual([]);
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore", "enter"]);
  });

  it("ends as no-view when it is continued where it cannot take the tty (in the background)", async () => {
    const rig = new Rig();
    const { served } = await stopped(rig);
    const dismiss = await rig.next("dismiss");
    rig.terminal.failEnter = true;
    rig.say("closed", closed("program"), dismiss.id);
    expect(await served).toEqual({ kind: "no-view", reason: "no-tty" });
    expect(rig.messages().filter((m) => m.kind === "hello")).toHaveLength(1);
  });

  it("is not heard while the program is on its way out, nor twice", async () => {
    const rig = new Rig();
    await shown(rig);
    rig.process.on("SIGINT", () => {});
    rig.process.emit("SIGINT");
    rig.process.emit("SIGTSTP");
    await rig.next("bye");
    expect(rig.messages().some((m) => m.kind === "dismiss")).toBe(false);
    expect(rig.process.kills).toEqual([]);

    const again = new Rig();
    await shown(again);
    again.process.emit("SIGTSTP");
    again.process.emit("SIGTSTP");
    await again.settle();
    expect(again.messages().filter((m) => m.kind === "dismiss")).toHaveLength(1);
  });

  it("is a listener of the session like the others: there while the view runs, gone when serve ends", async () => {
    const rig = new Rig();
    const { served } = await shown(rig);
    expect(rig.process.listenerCount("SIGTSTP")).toBe(1);
    rig.say("closed", closed("user"));
    await served;
    expect(rig.process.listenerCount("SIGTSTP")).toBe(0);
  });

  it("is left to the runtime that cannot catch it: serve works without the listener", async () => {
    const rig = new Rig();
    const on = rig.process.on.bind(rig.process);
    rig.process.on = (event: string, listener: (...args: never[]) => void) => {
      if (event === "SIGTSTP") throw new Error("this runtime has no SIGTSTP");
      return on(event, listener);
    };
    const { served } = await shown(rig);
    expect(rig.process.listenerCount("SIGTSTP")).toBe(0);
    rig.say("closed", closed("user"));
    expect((await served).kind).toBe("closed");
    expect(rig.process.listenerCount("SIGINT")).toBe(0);
  });
});
