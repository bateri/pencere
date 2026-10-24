/**
 * `serve` when the host starts the view again (§10.7) and when the program closes its view and
 * goes on, or shows another (§10.8).
 */

import { describe, expect, it } from "vitest";

import { toWire } from "../src/json.js";
import { ViewControl } from "../src/control.js";
import { subscribePayload } from "../src/messages.js";
import { serveWith } from "../src/serve.js";
import type { NoView, ServeOptions, Subscription } from "../src/serve.js";
import {
  Rig,
  TOKEN,
  TOKEN_2,
  fixture,
  luaBundle,
  offers,
  options,
  payloadOf,
  shown,
  welcomePayload,
} from "./support.js";
import type { Answer } from "./support.js";

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
const closed = (reason: string, rest: object = {}) => toWire({ reason, ...rest });
const need = (...missing: string[]) => toWire({ missing });
const CSI_C = "\x1b[c";

const count = (text: string, part: string) => text.split(part).length - 1;

/** A view that runs, and the program's hand on it. */
async function withControl(more: Partial<ServeOptions> = {}, answer: Answer = {}) {
  const rig = new Rig();
  const control = new ViewControl();
  const { served, open } = await shown(rig, { control, ...more }, answer);
  return { rig, control, served, open };
}

/** The host takes the dismiss, shows the pane idle, and answers the hello after it with a new token. */
async function replaced(rig: Rig, token = TOKEN_2, views = offers("web")) {
  const dismiss = await rig.next("dismiss");
  rig.say("closed", closed("program"), dismiss.id);
  const hello = await rig.next("hello");
  rig.say("welcome", welcomePayload(token, undefined, views), hello.id);
  const open = await rig.next("open");
  return { dismiss, hello, open };
}

describe("a program that closes its view and goes on (§10.8)", () => {
  it("reopens: the view is dismissed with its token, then a new token, a new open and the host's need", async () => {
    const { rig, control, served } = await withControl();
    const reopening = control.reopen();
    const dismiss = await rig.next("dismiss");
    expect(dismiss.payload).toBe(payloadOf("to-host/dismiss.frame"));
    rig.say("closed", closed("program"), dismiss.id);
    const hello = await rig.next("hello");
    rig.say("welcome", welcomePayload(TOKEN_2), hello.id);
    const open = await rig.next("open");
    expect(JSON.parse(open.payload)).toMatchObject({ token: TOKEN_2, view: "web", version: "1.0" });
    // It is not reopened until the host has taken the open.
    const state = await Promise.race([reopening, sleep(30).then(() => "waiting")]);
    expect(state).toBe("waiting");
    rig.say("need", need(), open.id);
    expect(await reopening).toEqual({ kind: "reopened" });
    // The alternate screen was taken once, and the fence is only for the first question.
    await rig.settle();
    expect(count(rig.text(), "\x1b[?1049h")).toBe(1);
    expect(count(rig.text(), CSI_C)).toBe(1);
    expect(rig.terminal.calls).toEqual(["enter"]);
    rig.say("closed", closed("user"));
    expect(await served).toMatchObject({ kind: "closed", by: "user" });
  });

  it("shows the same files again when no others are given: the host's store answers with an empty need", async () => {
    const { rig, control, served, open: first } = await withControl();
    const reopening = control.reopen();
    const { open } = await replaced(rig);
    expect(JSON.parse(open.payload).bundle).toEqual(JSON.parse(first.payload).bundle);
    rig.say("need", need(), open.id);
    await reopening;
    rig.say("closed", closed("user"));
    await served;
  });

  it("goes on with other kinds of view when the reopening names them", async () => {
    const { rig, control, served } = await withControl();
    const reopening = control.reopen({
      views: [{ kind: "lua", versions: ["0.5"], bundle: luaBundle() }],
    });
    const { hello, open } = await replaced(rig, TOKEN_2, offers("lua"));
    expect(JSON.parse(hello.payload).views).toEqual([{ kind: "lua", versions: ["0.5"] }]);
    expect(JSON.parse(open.payload)).toMatchObject({ token: TOKEN_2, view: "lua", version: "0.5" });
    rig.say("need", need(), open.id);
    await reopening;
    rig.say("closed", closed("user"));
    expect(await served).toMatchObject({ by: "user", view: { kind: "lua", version: "0.5" } });
  });

  it("refuses a list of views it may not offer, and the view is as it was", async () => {
    const { rig, control, served } = await withControl();
    await expect(control.reopen({ views: [] })).rejects.toThrow(TypeError);
    await rig.settle();
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello", "open"]);
    rig.say("closed", closed("user"));
    await served;
  });

  it("stops what the old view had asked: calls and subscriptions end with it", async () => {
    let subscription: Subscription | undefined;
    const { rig, control, served } = await withControl({
      topics: { t: (_p, s) => void (subscription = s) },
    });
    rig.say("msg", subscribePayload(1, "t"));
    await rig.settle();
    const reopening = control.reopen();
    const dismiss = await rig.next("dismiss");
    rig.say("closed", closed("program"), dismiss.id);
    await rig.settle();
    expect(subscription?.signal.aborted).toBe(true);
    const hello = await rig.next("hello");
    rig.say("welcome", welcomePayload(TOKEN_2), hello.id);
    const open = await rig.next("open");
    rig.say("need", need(), open.id);
    await reopening;
    rig.say("closed", closed("user"));
    await served;
  });

  it("closes: the terminal is given back, and serve ends closed by the program", async () => {
    const { rig, control, served } = await withControl();
    const closing = control.close();
    const dismiss = await rig.next("dismiss");
    rig.say("closed", closed("program"), dismiss.id);
    expect(await closing).toEqual({ kind: "closed" });
    expect(await served).toEqual({
      kind: "closed",
      by: "program",
      view: { kind: "web", version: "1.0" },
    });
    // No bye: the host has torn the view down and forgotten its token.
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello", "open", "dismiss"]);
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
    expect(rig.text().endsWith("\x1b[?1049l")).toBe(true);
  });

  it("touches nothing when the host does not list dismiss, and says so", async () => {
    const { rig, control, served } = await withControl({}, { features: ["bundle"] });
    expect(await control.reopen()).toEqual({ kind: "unsupported" });
    expect(await control.close()).toEqual({ kind: "unsupported" });
    await rig.settle();
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello", "open"]);
    rig.say("closed", closed("user"));
    await served;
  });

  it("has no view to close before there is one, while one is being closed, or when serve has ended", async () => {
    const early = new ViewControl();
    expect(await early.reopen()).toEqual({ kind: "unavailable" });
    expect(await early.close()).toEqual({ kind: "unavailable" });

    const { rig, control, served } = await withControl();
    const closing = control.close();
    expect(await control.close()).toEqual({ kind: "unavailable" });
    expect(await control.reopen()).toEqual({ kind: "unavailable" });
    const dismiss = await rig.next("dismiss");
    rig.say("closed", closed("program"), dismiss.id);
    await closing;
    await served;
    expect(await control.close()).toEqual({ kind: "unavailable" });
  });

  it("is told unavailable when someone else closes the view before the host answers the dismiss", async () => {
    const { rig, control, served } = await withControl();
    const reopening = control.reopen();
    await rig.next("dismiss");
    rig.say("closed", closed("user"));
    expect(await reopening).toEqual({ kind: "unavailable" });
    expect(await served).toMatchObject({ kind: "closed", by: "user" });
  });

  it("ends as no view, with the fallback, when the host refuses the new open", async () => {
    const why: NoView[] = [];
    const { rig, control, served } = await withControl({ fallback: (w) => void why.push(w) });
    const reopening = control.reopen();
    const { open } = await replaced(rig);
    rig.say("closed", closed("refused", { code: "policy" }), open.id);
    const refused = { kind: "no-view", reason: "refused", code: "policy" };
    expect(await reopening).toEqual(refused);
    expect(await served).toEqual(refused);
    expect(why).toEqual([refused]);
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
  });

  it("ends as no view, for want of an answer and not for want of a host, when the host stops answering the hello", async () => {
    const why: NoView[] = [];
    const { rig, control, served } = await withControl({
      reopenTimeout: 60,
      fallback: (w) => void why.push(w),
    });
    const reopening = control.reopen();
    const dismiss = await rig.next("dismiss");
    rig.say("closed", closed("program"), dismiss.id);
    await rig.next("hello");
    expect(await reopening).toEqual({ kind: "no-view", reason: "timeout" });
    expect(await served).toEqual({ kind: "no-view", reason: "timeout" });
    expect(why).toEqual([{ kind: "no-view", reason: "timeout" }]);
    // The host had closed the view: there is nothing to say bye to.
    expect(rig.messages().map((m) => m.kind)).toEqual(["hello", "open", "dismiss", "hello"]);
  });

  it("ends as no view when the host has nothing to open of what the new views offer", async () => {
    const { rig, control, served } = await withControl();
    const reopening = control.reopen({
      views: [{ kind: "lua", versions: ["0.5"], bundle: luaBundle() }],
    });
    const dismiss = await rig.next("dismiss");
    rig.say("closed", closed("program"), dismiss.id);
    const hello = await rig.next("hello");
    rig.say("welcome", welcomePayload(TOKEN_2, undefined, []), hello.id);
    expect(await reopening).toEqual({ kind: "no-view", reason: "no-common-view" });
    expect(await served).toEqual({ kind: "no-view", reason: "no-common-view" });
  });

  it("waits for the answer to the hello of a reopening longer than for the first one", async () => {
    const { rig, control, served } = await withControl({ answerTimeout: 30, reopenTimeout: 2000 });
    const reopening = control.reopen();
    const dismiss = await rig.next("dismiss");
    rig.say("closed", closed("program"), dismiss.id);
    const hello = await rig.next("hello");
    await sleep(120);
    rig.say("welcome", welcomePayload(TOKEN_2), hello.id);
    const open = await rig.next("open");
    rig.say("need", need(), open.id);
    expect(await reopening).toEqual({ kind: "reopened" });
    rig.say("closed", closed("user"));
    await served;
  });

  it("gives up on a host that does not answer the dismiss, and says bye with the token the view has", async () => {
    const { rig, control, served } = await withControl({ reopenTimeout: 50, byeTimeout: 30 });
    const reopening = control.reopen();
    await rig.next("dismiss");
    const bye = await rig.next("bye");
    expect(JSON.parse(bye.payload)).toEqual({ token: TOKEN, reason: "error" });
    expect(await reopening).toEqual({ kind: "no-view", reason: "timeout" });
    expect(await served).toEqual({ kind: "no-view", reason: "timeout" });
    expect(rig.terminal.calls).toEqual(["enter", "discard", "restore"]);
  });

  it("does not wait on a dismiss the host has answered, once the answer is in", async () => {
    const { rig, control, served } = await withControl({ reopenTimeout: 60 });
    const reopening = control.reopen();
    const dismiss = await rig.next("dismiss");
    rig.say("closed", closed("program"), dismiss.id);
    const hello = await rig.next("hello");
    rig.say("welcome", welcomePayload(TOKEN_2), hello.id);
    const open = await rig.next("open");
    rig.say("need", need(), open.id);
    await reopening;
    await sleep(120);
    expect(rig.messages().filter((m) => m.kind === "bye")).toEqual([]);
    rig.say("closed", closed("user"));
    await served;
  });

  it("gives the terminal back on a signal between the views, with no bye, as the view has none", async () => {
    const { rig, control, served } = await withControl();
    rig.process.on("SIGINT", () => {});
    void control.reopen();
    const dismiss = await rig.next("dismiss");
    rig.say("closed", closed("program"), dismiss.id);
    await rig.next("hello");
    rig.process.emit("SIGINT");
    await served;
    expect(rig.messages().filter((m) => m.kind === "bye")).toEqual([]);
    expect(rig.terminal.calls).toEqual(["enter", "restore"]);
  });

  it("says bye with the view's token when the signal comes before the host has answered the dismiss", async () => {
    const { rig, control } = await withControl();
    rig.process.on("SIGINT", () => {});
    void control.reopen();
    await rig.next("dismiss");
    rig.process.emit("SIGINT");
    const bye = await rig.next("bye");
    expect(JSON.parse(bye.payload)).toEqual({ token: TOKEN, reason: "signal" });
  });

  it("says bye when the process exits with a dismiss out", async () => {
    const { rig, control } = await withControl();
    void control.reopen();
    await rig.next("dismiss");
    rig.process.emit("exit", 0);
    const bye = await rig.next("bye");
    expect(JSON.parse(bye.payload)).toEqual({ token: TOKEN, reason: "exit" });
  });

  it("reads closed by the program as the view's end, whoever asked", async () => {
    const { rig, served } = await withControl();
    rig.say("closed", closed("program"), 99);
    await rig.settle();
    rig.say("closed", closed("program"));
    expect(await served).toMatchObject({ kind: "closed", by: "program" });
  });
});

describe("a host that starts the view again (§10.7)", () => {
  it("is answered with a new open and a new token, and what the old view asked is over", async () => {
    let subscription: Subscription | undefined;
    const { rig, served, open } = await withControl({
      topics: { t: (_p, s) => void (subscription = s) },
    });
    rig.say("msg", subscribePayload(1, "t"));
    await rig.settle();
    rig.sayBytes(fixture("to-program/welcome-restart.frame"));
    const again = await rig.next("open");
    expect(again.id).toBeGreaterThan(open.id);
    expect(JSON.parse(again.payload)).toMatchObject({
      token: TOKEN_2,
      view: "web",
      version: "1.0",
    });
    expect(subscription?.signal.aborted).toBe(true);
    rig.say("need", need(), again.id);
    await rig.settle();
    // The alternate screen is the one already taken; and the new view's token is the one a bye carries.
    expect(count(rig.text(), "\x1b[?1049h")).toBe(1);
    rig.process.emit("exit", 0);
    const bye = await rig.next("bye");
    expect(JSON.parse(bye.payload).token).toBe(TOKEN_2);
    void served;
  });

  it("serves the new view: its calls are answered", async () => {
    const { rig, served } = await withControl({ calls: { list: () => "again" } });
    rig.sayBytes(fixture("to-program/welcome-restart.frame"));
    const again = await rig.next("open");
    rig.say("need", need(), again.id);
    rig.say("msg", toWire({ call: { id: 1, method: "list" } }));
    const answer = await rig.next("msg");
    expect(JSON.parse(answer.payload)).toEqual({ response: { id: 1, result: "again" } });
    rig.say("closed", closed("user"));
    expect(await served).toMatchObject({ by: "user" });
  });

  it("is ignored when the welcome does not say restart, answers something, or comes with no view open", async () => {
    const { rig, served } = await withControl();
    rig.say("welcome", welcomePayload(TOKEN_2));
    rig.say("welcome", welcomePayload(TOKEN_2, undefined, undefined, { restart: true }), 7);
    await rig.settle();
    expect(rig.messages().filter((m) => m.kind === "open")).toHaveLength(1);
    rig.say("closed", closed("user"));
    await served;

    const idle = new Rig();
    const waiting = serveWith(options({ answerTimeout: 150 }), idle.environment());
    await idle.next("hello");
    idle.sayBytes(fixture("to-program/welcome-restart.frame"));
    await idle.settle();
    expect(idle.messages().map((m) => m.kind)).toEqual(["hello"]);
    expect(await waiting).toEqual({ kind: "no-view", reason: "no-host" });
  });

  it("is ignored when the host opens nothing of what the program shows", async () => {
    const { rig, served } = await withControl();
    rig.say(
      "welcome",
      welcomePayload(TOKEN_2, undefined, offers(["wasm", "0.5"]), { restart: true }),
    );
    await rig.settle();
    expect(rig.messages().filter((m) => m.kind === "open")).toHaveLength(1);
    rig.say("closed", closed("user"));
    await served;
  });

  it("ends a reopening whose dismiss is out and shows the view the host asks for, with a token that is alive", async () => {
    const { rig, control, served } = await withControl();
    const reopening = control.reopen();
    await rig.next("dismiss");
    rig.sayBytes(fixture("to-program/welcome-restart.frame"));
    const again = await rig.next("open");
    expect(JSON.parse(again.payload).token).toBe(TOKEN_2);
    expect(await reopening).toEqual({ kind: "unavailable" });
    rig.say("closed", closed("user"));
    await served;
  });

  it("takes the offer for the answer to a hello that waits, and goes on with the reopening", async () => {
    const { rig, control, served } = await withControl();
    const reopening = control.reopen();
    const dismiss = await rig.next("dismiss");
    rig.say("closed", closed("program"), dismiss.id);
    await rig.next("hello");
    // The host came up after a break: its unsolicited welcome is the answer the hello was for.
    rig.sayBytes(fixture("to-program/welcome-restart.frame"));
    const open = await rig.next("open");
    expect(JSON.parse(open.payload).token).toBe(TOKEN_2);
    rig.say("need", need(), open.id);
    expect(await reopening).toEqual({ kind: "reopened" });
    rig.say("closed", closed("user"));
    await served;
  });

  it("sends no dismiss with the token of a view that is gone", async () => {
    const { rig, control, served } = await withControl();
    rig.sayBytes(fixture("to-program/welcome-restart.frame"));
    const again = await rig.next("open");
    // The old token is the host's to have forgotten: nothing can be dismissed until the new open is out.
    rig.say("need", need(), again.id);
    await rig.settle();
    const closing = control.close();
    const dismiss = await rig.next("dismiss");
    expect(JSON.parse(dismiss.payload)).toEqual({ token: TOKEN_2 });
    rig.say("closed", closed("program"), dismiss.id);
    await closing;
    await served;
  });
});
