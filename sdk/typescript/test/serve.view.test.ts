/** `serve` with a view that runs: the calls, the subscriptions and the rest of the six messages (§9). */

import { describe, expect, it } from "vitest";

import { toWire } from "../src/json.js";
import { callPayload, cancelPayload, subscribePayload } from "../src/messages.js";
import { ViewError } from "../src/serve.js";
import type { CallContext, ServeOptions, Subscription } from "../src/serve.js";
import { Rig, fixture, payloadOf, shown } from "./support.js";

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const CONTAINERS = [{ id: "c1", name: "web", state: "running" }];

/** The messages of the six that the program has written so far, in order, parsed. */
const spoken = (rig: Rig): Record<string, unknown>[] =>
  rig
    .messages()
    .filter((m) => m.kind === "msg")
    .map((m) => JSON.parse(m.payload) as Record<string, unknown>);

/** A view that runs, with the handlers given; `end` closes it as the person does. */
async function running(more: Partial<ServeOptions>) {
  const rig = new Rig();
  const { served } = await shown(rig, more);
  return {
    rig,
    served,
    end: async () => {
      rig.say("closed", toWire({ reason: "user" }));
      await served;
    },
  };
}

describe("a call of the view", () => {
  it("is answered with the result of the handler, as the fixtures write it", async () => {
    let seen: unknown;
    const { rig, end } = await running({
      calls: {
        list: (params) => {
          seen = params;
          return { containers: CONTAINERS };
        },
      },
    });
    rig.say("msg", payloadOf("to-program/msg-call.frame"));
    const answer = await rig.next("msg");
    expect(answer.payload).toBe(payloadOf("to-host/msg-response.frame"));
    expect(seen).toEqual({ all: true });
    await end();
  });

  it("is answered after the handler, whether it answers at once or later", async () => {
    const { rig, end } = await running({
      calls: {
        now: () => 1,
        later: async () => {
          await sleep(20);
          return 2;
        },
      },
    });
    rig.say("msg", callPayload(1, "later"));
    rig.say("msg", callPayload(2, "now"));
    const first = await rig.next("msg");
    const second = await rig.next("msg");
    expect(JSON.parse(first.payload)).toEqual({ response: { id: 2, result: 1 } });
    expect(JSON.parse(second.payload)).toEqual({ response: { id: 1, result: 2 } });
    await end();
  });

  it("is answered {} when the handler has nothing to say: a result is never null", async () => {
    const { rig, end } = await running({
      calls: { nothing: () => undefined, none: () => null, zero: () => 0 },
    });
    for (const [id, method] of [
      [1, "nothing"],
      [2, "none"],
      [3, "zero"],
    ] as const)
      rig.say("msg", callPayload(id, method));
    await rig.settle();
    expect(spoken(rig)).toEqual([
      { response: { id: 1, result: {} } },
      { response: { id: 2, result: {} } },
      { response: { id: 3, result: 0 } },
    ]);
    await end();
  });

  it("is answered not-found when no handler is named for it, whatever the name is", async () => {
    const { rig, end } = await running({ calls: { list: () => ({}) } });
    const names = ["stop", "constructor", "__proto__", "toString", "hasOwnProperty", ""];
    names.forEach((name, at) => rig.say("msg", callPayload(at + 1, name)));
    await rig.settle();
    const answers = spoken(rig) as { response: { id: number; error: { code: string } } }[];
    expect(answers.map((a) => a.response.error.code)).toEqual(names.map(() => "not-found"));
    expect(answers.map((a) => a.response.id)).toEqual([1, 2, 3, 4, 5, 6]);
    await end();
  });

  it("is answered not-found when the program gave no handlers at all", async () => {
    const { rig, end } = await running({});
    rig.say("msg", callPayload(1, "list"));
    rig.say("msg", subscribePayload(2, "stats"));
    await rig.settle();
    expect(spoken(rig).map((m) => Object.values(m)[0])).toEqual([
      { id: 1, error: { code: "not-found", message: "there is nothing to call by that name" } },
      {
        id: 2,
        error: { code: "not-found", message: "there is nothing to subscribe to by that name" },
      },
    ]);
    await end();
  });

  it("is answered with the error a handler throws: a code, and words when it gave some", async () => {
    const { rig, end } = await running({
      calls: {
        gone: () => {
          throw new ViewError("gone", "no such container");
        },
        bare: () => {
          throw new ViewError("denied");
        },
        later: async () => {
          await sleep(5);
          throw new ViewError("late", "too late");
        },
        returned: () => new ViewError("returned", "not thrown"),
        plain: () => {
          throw new Error("disk is full");
        },
        odd: () => {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw "a string";
        },
      },
    });
    ["gone", "bare", "later", "returned", "plain", "odd"].forEach((m, at) =>
      rig.say("msg", callPayload(at + 1, m)),
    );
    await sleep(60);
    const byId = new Map(
      (spoken(rig) as { response: { id: number; error: object } }[]).map((m) => [
        m.response.id,
        m.response.error,
      ]),
    );
    expect(byId.get(1)).toEqual({ code: "gone", message: "no such container" });
    expect(byId.get(2)).toEqual({ code: "denied" });
    expect(byId.get(3)).toEqual({ code: "late", message: "too late" });
    expect(byId.get(4)).toEqual({ code: "returned", message: "not thrown" });
    expect(byId.get(5)).toEqual({ code: "internal", message: "disk is full" });
    expect(byId.get(6)).toEqual({ code: "internal" });
    await end();
  });

  it("is answered an error when the result cannot be written, or is over the limit of a message", async () => {
    const { rig, end } = await running({
      calls: {
        big: () => ({ text: "x".repeat(1024 * 1024) }),
        cyclic: () => {
          const loop: Record<string, unknown> = {};
          loop.me = loop;
          return loop;
        },
        bigint: () => 10n,
      },
    });
    rig.say("msg", callPayload(1, "big"));
    rig.say("msg", callPayload(2, "cyclic"));
    rig.say("msg", callPayload(3, "bigint"));
    await rig.settle();
    await sleep(100);
    const codes = (spoken(rig) as { response: { id: number; error: { code: string } } }[])
      .map((m) => [m.response.id, m.response.error.code])
      .sort();
    expect(codes).toEqual([
      [1, "message-too-long"],
      [2, "internal"],
      [3, "internal"],
    ]);
    await end();
  });

  it("is answered in frames of the host's limit when it is longer than one, and arrives whole", async () => {
    const rows = Array.from({ length: 400 }, (_, i) => ({ id: `c${i}`, note: "é".repeat(5) }));
    const { rig, end } = await running({ calls: { list: () => ({ rows }) } });
    rig.say("msg", callPayload(1, "list"));
    // The rig decodes with the limit of 4096 bytes a frame, and would have thrown on a longer one.
    const answer = await rig.next("msg");
    expect(answer.payload.length).toBeGreaterThan(4096);
    expect(JSON.parse(answer.payload)).toEqual({ response: { id: 1, result: { rows } } });
    expect(/^[\x20-\x7e]*$/.test(answer.payload)).toBe(true);
    await end();
  });

  it("can tell how far it has got, and the progress comes before the answer", async () => {
    const { rig, end } = await running({
      calls: {
        pull: async (_params, call: CallContext) => {
          call.progress(4, 10, "layer 4 of 10");
          call.progress(5);
          await sleep(5);
          return { pulled: true };
        },
      },
    });
    rig.say("msg", callPayload(4, "pull"));
    const first = await rig.next("msg");
    expect(first.payload).toBe(payloadOf("to-host/msg-progress.frame"));
    await rig.settle();
    expect(spoken(rig)).toEqual([
      { progress: { id: 4, done: 4, total: 10, note: "layer 4 of 10" } },
      { progress: { id: 4, done: 5 } },
      { response: { id: 4, result: { pulled: true } } },
    ]);
    await end();
  });

  it("is told to stop by a cancel, and its answer is then not written; its later progress neither", async () => {
    let signal: AbortSignal | undefined;
    let context: CallContext | undefined;
    let finish: (() => void) | undefined;
    const { rig, end } = await running({
      calls: {
        slow: (_params, call) => {
          signal = call.signal;
          context = call;
          return new Promise((resolve) => (finish = () => resolve("done")));
        },
      },
    });
    rig.say("msg", callPayload(7, "slow"));
    await rig.settle();
    expect(signal?.aborted).toBe(false);
    rig.say("msg", cancelPayload(7));
    await rig.settle();
    expect(signal?.aborted).toBe(true);
    context?.progress(1, 2);
    finish?.();
    await rig.settle();
    expect(spoken(rig)).toEqual([]);
    await end();
  });

  it("does not use an id that is out again, and a cancel of an id that is not out changes nothing", async () => {
    let calls = 0;
    let finish: (() => void) | undefined;
    const { rig, end } = await running({
      calls: {
        slow: () => {
          calls++;
          return new Promise((resolve) => (finish = () => resolve(1)));
        },
      },
      topics: { stats: () => undefined },
    });
    rig.say("msg", callPayload(1, "slow"));
    rig.say("msg", callPayload(1, "slow"));
    rig.say("msg", subscribePayload(1, "stats"));
    rig.say("msg", cancelPayload(99));
    await rig.settle();
    expect(calls).toBe(1);
    finish?.();
    await rig.settle();
    expect(spoken(rig)).toEqual([{ response: { id: 1, result: 1 } }]);
    // The id is free again once the call is answered.
    rig.say("msg", callPayload(1, "slow"));
    await rig.settle();
    expect(calls).toBe(2);
    await end();
  });

  it("ignores what is not a message of the view: nothing is called and nothing is written", async () => {
    let calls = 0;
    const { rig, end } = await running({
      calls: { list: () => void calls++ },
      topics: { stats: () => void calls++ },
    });
    for (const text of [
      "not json",
      "[]",
      '{"call":{"id":0,"method":"list"}}',
      '{"call":{"id":1}}',
      '{"call":{"id":1,"method":"list"},"cancel":{"id":1}}',
      '{"response":{"id":1,"result":{}}}',
      '{"close":{}}',
      '{"subscribe":{"id":1,"topic":5}}',
      '{"cancel":{"id":"1"}}',
    ])
      rig.say("msg", text);
    await rig.settle();
    expect(calls).toBe(0);
    expect(spoken(rig)).toEqual([]);
    await end();
  });

  it("is read from a message that came in several chunks, with its text unescaped", async () => {
    let params: { reason?: string } | undefined;
    const { rig, end } = await running({
      calls: {
        stop: (p) => {
          params = p as { reason?: string };
          return "stopped";
        },
      },
    });
    // The three chunks of the specification's example, as the host writes them.
    rig.sayBytes(fixture("to-program/msg-chunked.frame"));
    await rig.next("msg");
    expect(params?.reason?.startsWith("Arrêt demandé depuis la vue. ")).toBe(true);
    await end();
  });
});

describe("a subscription of the view", () => {
  it("is fed by the topic's handler, and what it pushes reaches the view as the fixtures write it", async () => {
    let subscription: Subscription | undefined;
    let seen: unknown;
    const { rig, end } = await running({
      topics: {
        stats: (params, s) => {
          seen = params;
          subscription = s;
        },
      },
    });
    rig.say("msg", payloadOf("to-program/msg-subscribe.frame"));
    await rig.settle();
    expect(seen).toEqual({ name: "web" });
    expect(subscription?.topic).toBe("stats");
    expect(subscription?.push({ id: "c1", state: "exited" })).toBe(true);
    const pushed = await rig.next("msg");
    // The subscription of the example is number 2; its push in the example is number 3.
    expect(JSON.parse(pushed.payload)).toEqual({
      push: { id: 2, value: { id: "c1", state: "exited" } },
    });
    expect(payloadOf("to-host/msg-push.frame")).toBe(
      '{"push":{"id":3,"value":{"id":"c1","state":"exited"}}}',
    );
    await end();
  });

  it("pushes values in the order they are pushed, null for nothing, and nothing over the limit of a message", async () => {
    let subscription: Subscription | undefined;
    const { rig, end } = await running({
      topics: { t: (_p, s) => void (subscription = s) },
    });
    rig.say("msg", subscribePayload(5, "t"));
    await rig.settle();
    expect(subscription?.push(1)).toBe(true);
    expect(subscription?.push(undefined)).toBe(true);
    expect(subscription?.push("x".repeat(1024 * 1024))).toBe(false);
    expect(subscription?.push(10n)).toBe(false);
    expect(subscription?.push({ n: 2 })).toBe(true);
    await rig.settle();
    expect(spoken(rig)).toEqual([
      { push: { id: 5, value: 1 } },
      { push: { id: 5, value: null } },
      { push: { id: 5, value: { n: 2 } } },
    ]);
    await end();
  });

  it("ends with a cancel: what feeds it is stopped, and a push after that is not written", async () => {
    let subscription: Subscription | undefined;
    let stopped = 0;
    const { rig, end } = await running({
      topics: {
        t: (_p, s) => {
          subscription = s;
          return () => void stopped++;
        },
      },
    });
    rig.say("msg", subscribePayload(2, "t"));
    await rig.settle();
    expect(subscription?.signal.aborted).toBe(false);
    rig.say("msg", payloadOf("to-program/msg-cancel.frame"));
    await rig.settle();
    expect(stopped).toBe(1);
    expect(subscription?.signal.aborted).toBe(true);
    expect(subscription?.push(1)).toBe(false);
    // A second cancel of the same subscription changes nothing.
    rig.say("msg", cancelPayload(2));
    await rig.settle();
    expect(stopped).toBe(1);
    expect(spoken(rig)).toEqual([]);
    // Its number may be used again.
    rig.say("msg", subscribePayload(2, "t"));
    await rig.settle();
    expect(subscription?.signal.aborted).toBe(false);
    await end();
  });

  it("is stopped, with a cleanup that came late, as soon as the handler's promise ends", async () => {
    let stopped = 0;
    const { rig, end } = await running({
      topics: {
        t: async () => {
          await sleep(30);
          return () => void stopped++;
        },
      },
    });
    rig.say("msg", subscribePayload(1, "t"));
    rig.say("msg", cancelPayload(1));
    await sleep(80);
    expect(stopped).toBe(1);
    await end();
  });

  it("is answered with an error, and is over, when the topic's handler throws", async () => {
    let signal: AbortSignal | undefined;
    const { rig, end } = await running({
      topics: {
        t: (_p, s) => {
          signal = s.signal;
          throw new ViewError("denied", "not for you");
        },
        late: async () => {
          await sleep(5);
          throw new Error("broke");
        },
      },
    });
    rig.say("msg", subscribePayload(1, "t"));
    rig.say("msg", subscribePayload(2, "late"));
    await sleep(60);
    expect(
      new Set(
        (spoken(rig) as { response: { id: number; error: object } }[]).map((m) =>
          JSON.stringify(m),
        ),
      ),
    ).toEqual(
      new Set([
        '{"response":{"id":1,"error":{"code":"denied","message":"not for you"}}}',
        '{"response":{"id":2,"error":{"code":"internal","message":"broke"}}}',
      ]),
    );
    expect(signal?.aborted).toBe(true);
    await end();
  });

  it("is a topic by its own name only", async () => {
    const { rig, end } = await running({ topics: { stats: () => undefined } });
    rig.say("msg", subscribePayload(1, "constructor"));
    await rig.settle();
    expect(spoken(rig)).toMatchObject([{ response: { id: 1, error: { code: "not-found" } } }]);
    await end();
  });
});

describe("when the view is over", () => {
  it("every call is cancelled and every subscription stopped, and nothing is written for them", async () => {
    const signals: AbortSignal[] = [];
    let stopped = 0;
    let finish: (() => void) | undefined;
    const { rig, served } = await running({
      calls: {
        slow: (_p, call) => {
          signals.push(call.signal);
          return new Promise((resolve) => (finish = () => resolve(1)));
        },
      },
      topics: {
        t: (_p, s) => {
          signals.push(s.signal);
          return () => void stopped++;
        },
      },
    });
    rig.say("msg", callPayload(1, "slow"));
    rig.say("msg", subscribePayload(2, "t"));
    await rig.settle();
    expect(signals.map((s) => s.aborted)).toEqual([false, false]);
    rig.say("closed", toWire({ reason: "user" }));
    await served;
    expect(signals.map((s) => s.aborted)).toEqual([true, true]);
    expect(stopped).toBe(1);
    finish?.();
    await rig.settle();
    expect(spoken(rig)).toEqual([]);
  });

  it("a handler that fails on stopping does not fail the program", async () => {
    const { rig, served } = await running({
      topics: {
        t: () => () => {
          throw new Error("cleanup failed");
        },
      },
    });
    rig.say("msg", subscribePayload(1, "t"));
    await rig.settle();
    rig.say("closed", toWire({ reason: "view" }));
    expect(await served).toMatchObject({ kind: "closed", by: "view" });
  });
});
