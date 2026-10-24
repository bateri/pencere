import { describe, expect, it } from "vitest";

import { MAX_VIEW_MESSAGE } from "../src/messages.js";
import { ViewError, createRouter } from "../src/router.js";
import type { RouterHandlers } from "../src/router.js";
import { Rig, shown } from "./support.js";

/** A router and the program's side of it: what it sent, parsed, and whether the view still listens. */
function rig(handlers: RouterHandlers, listening = () => true) {
  const sent: Record<string, unknown>[] = [];
  const payloads: string[] = [];
  const router = createRouter(handlers, (payload) => {
    if (!listening()) return false;
    payloads.push(payload);
    sent.push(JSON.parse(payload));
    return true;
  });
  const later = () => new Promise((resolve) => setTimeout(resolve, 5));
  return { router, sent, payloads, later };
}

const idOf = (message: Record<string, unknown>): number =>
  (Object.values(message)[0] as { id: number }).id;

describe("calls", () => {
  it("runs the handler of the method with the params and answers with its result", async () => {
    const seen: unknown[] = [];
    const { router, sent, later } = rig({
      calls: {
        list: (params, call) => {
          seen.push(params, call.method);
          return { items: [1, 2] };
        },
      },
    });
    expect(router.handle({ call: { id: 1, method: "list", params: { all: true } } })).toBe(true);
    await later();
    expect(seen).toEqual([{ all: true }, "list"]);
    expect(sent).toEqual([{ response: { id: 1, result: { items: [1, 2] } } }]);
  });

  it("takes the message as JSON text too, as the host passes it", async () => {
    const { router, sent, later } = rig({ calls: { ping: () => ({ pong: true }) } });
    router.handle('{"call":{"id":4,"method":"ping"}}');
    await later();
    expect(sent).toEqual([{ response: { id: 4, result: { pong: true } } }]);
  });

  it("answers `{}` for a call with nothing to say: a result is never null", async () => {
    const { router, sent, later } = rig({ calls: { a: () => undefined, b: () => null } });
    router.handle({ call: { id: 1, method: "a" } });
    router.handle({ call: { id: 2, method: "b" } });
    await later();
    expect(sent).toEqual([
      { response: { id: 1, result: {} } },
      { response: { id: 2, result: {} } },
    ]);
  });

  it("answers an error as `serve` does: a ViewError with its code and words, any other with `internal`", async () => {
    const { router, sent, later } = rig({
      calls: {
        refuse: () => {
          throw new ViewError("no-such-container", "There is no such container.");
        },
        returned: () => new ViewError("busy"),
        broken: () => {
          throw new Error("disk is full");
        },
        awaited: async () => {
          throw new ViewError("slow");
        },
      },
    });
    for (const [id, method] of [
      [1, "refuse"],
      [2, "returned"],
      [3, "broken"],
      [4, "awaited"],
    ] as const)
      router.handle({ call: { id, method } });
    await later();
    // Each handler answers when it is done; the order is theirs.
    sent.sort((a, b) => idOf(a) - idOf(b));
    expect(sent).toEqual([
      {
        response: {
          id: 1,
          error: { code: "no-such-container", message: "There is no such container." },
        },
      },
      { response: { id: 2, error: { code: "busy" } } },
      { response: { id: 3, error: { code: "internal", message: "disk is full" } } },
      { response: { id: 4, error: { code: "slow" } } },
    ]);
  });

  it("answers not-found for a method nobody serves, and for a name that is only inherited", async () => {
    const { router, sent, later } = rig({ calls: { list: () => ({}) } });
    router.handle({ call: { id: 1, method: "nothing" } });
    router.handle({ call: { id: 2, method: "constructor" } });
    await later();
    expect(sent.map((m) => (m["response"] as { error: { code: string } }).error.code)).toEqual([
      "not-found",
      "not-found",
    ]);
  });

  it("reports progress until the call is answered, and no more", async () => {
    let report: ((done: number, total?: number, note?: string) => void) | undefined;
    const { router, sent, later } = rig({
      calls: {
        pull: (_params, call) => {
          report = call.progress;
          call.progress(1, 4, "layer 1");
          return new Promise(() => {});
        },
      },
    });
    router.handle({ call: { id: 7, method: "pull" } });
    await later();
    report?.(2);
    report?.(Number.NaN);
    expect(sent).toEqual([
      { progress: { id: 7, done: 1, total: 4, note: "layer 1" } },
      { progress: { id: 7, done: 2 } },
    ]);
    router.handle({ cancel: { id: 7 } });
    report?.(3);
    expect(sent).toHaveLength(2);
  });

  it("aborts the signal of a call the view cancels, and does not answer it", async () => {
    let signal: AbortSignal | undefined;
    let finish: (() => void) | undefined;
    const { router, sent, later } = rig({
      calls: {
        slow: (_params, call) => {
          signal = call.signal;
          return new Promise((resolve) => (finish = () => resolve({ late: true })));
        },
      },
    });
    router.handle({ call: { id: 1, method: "slow" } });
    await later();
    expect(signal?.aborted).toBe(false);
    router.handle({ cancel: { id: 1 } });
    expect(signal?.aborted).toBe(true);
    finish?.();
    await later();
    expect(sent).toEqual([]);
  });

  it("does not take an id that is out for a second request", async () => {
    let calls = 0;
    const { router, later } = rig({
      calls: {
        slow: () => {
          calls += 1;
          return new Promise(() => {});
        },
      },
      topics: { t: () => {} },
    });
    router.handle({ call: { id: 1, method: "slow" } });
    router.handle({ call: { id: 1, method: "slow" } });
    router.handle({ subscribe: { id: 1, topic: "t" } });
    await later();
    expect(calls).toBe(1);
  });

  it("counts the limit of a message before the escapes of the wire: nearly 1 MiB of Turkish text goes through", async () => {
    // Two bytes a letter as text, six characters a letter as the wire writes it: 3 MB on the wire.
    const text = "ş".repeat(500_000);
    const { router, sent, payloads, later } = rig({
      calls: { turkish: () => ({ text }) },
      topics: { stats: (_params, subscription) => void subscription.push({ text }) },
    });
    router.handle({ call: { id: 1, method: "turkish" } });
    router.handle({ subscribe: { id: 2, topic: "stats" } });
    await later();
    expect(payloads.every((payload) => payload.length > 2 * MAX_VIEW_MESSAGE)).toBe(true);
    expect(sent).toHaveLength(2);
    expect(sent.map((m) => Object.keys(m)[0]).sort()).toEqual(["push", "response"]);
    const response = sent.find((m) => "response" in m) as {
      response: { result: { text: string } };
    };
    expect(response.response.result.text).toBe(text);
  });

  it("refuses one byte over: the limit is the same 1 MiB, in bytes, on text with escapes in it", async () => {
    // The JSON around the text is `{"text":""}` inside a response: the text may be the rest.
    const wrapper = `{"response":{"id":1,"result":{"text":""}}}`.length;
    const { router, sent, later } = rig({
      calls: {
        fits: () => ({ text: "ş".repeat((MAX_VIEW_MESSAGE - wrapper) / 2) }),
        over: () => ({ text: "ş".repeat((MAX_VIEW_MESSAGE - wrapper) / 2 + 1) }),
      },
    });
    router.handle({ call: { id: 1, method: "fits" } });
    router.handle({ call: { id: 2, method: "over" } });
    await later();
    sent.sort((a, b) => idOf(a) - idOf(b));
    expect(Object.keys((sent[0] as { response: object }).response)).toContain("result");
    expect(sent[1]).toEqual({
      response: {
        id: 2,
        error: { code: "message-too-long", message: "the result is over the limit of a message" },
      },
    });
  });

  it("answers a result that is too large with message-too-long, and one that is not JSON with internal", async () => {
    const { router, sent, later } = rig({
      calls: {
        big: () => ({ text: "x".repeat(MAX_VIEW_MESSAGE) }),
        circular: () => {
          const loop: Record<string, unknown> = {};
          loop["self"] = loop;
          return loop;
        },
      },
    });
    router.handle({ call: { id: 1, method: "big" } });
    router.handle({ call: { id: 2, method: "circular" } });
    await later();
    expect(sent).toEqual([
      {
        response: {
          id: 1,
          error: { code: "message-too-long", message: "the result is over the limit of a message" },
        },
      },
      {
        response: {
          id: 2,
          error: { code: "internal", message: "the result cannot be written as JSON" },
        },
      },
    ]);
  });
});

describe("subscriptions", () => {
  it("pushes what the handler pushes, until the view cancels, and runs the cleanup once", async () => {
    let push: ((value: unknown) => boolean) | undefined;
    let cleaned = 0;
    let signal: AbortSignal | undefined;
    const { router, sent, later } = rig({
      topics: {
        stats: (params, subscription) => {
          push = subscription.push;
          signal = subscription.signal;
          subscription.push({ params });
          return () => void (cleaned += 1);
        },
      },
    });
    router.handle({ subscribe: { id: 2, topic: "stats", params: { every: 1 } } });
    await later();
    expect(push?.({ n: 1 })).toBe(true);
    router.handle({ cancel: { id: 2 } });
    router.handle({ cancel: { id: 2 } });
    expect(signal?.aborted).toBe(true);
    expect(cleaned).toBe(1);
    expect(push?.({ n: 2 })).toBe(false);
    expect(sent).toEqual([
      { push: { id: 2, value: { params: { every: 1 } } } },
      { push: { id: 2, value: { n: 1 } } },
    ]);
  });

  it("ends a subscription its handler refuses with an error, and stops it", async () => {
    let signal: AbortSignal | undefined;
    const { router, sent, later } = rig({
      topics: {
        refused: (_params, subscription) => {
          signal = subscription.signal;
          throw new ViewError("forbidden", "Not for you.");
        },
        missing: () => {},
      },
    });
    router.handle({ subscribe: { id: 1, topic: "refused" } });
    router.handle({ subscribe: { id: 2, topic: "nothing" } });
    await later();
    expect(signal?.aborted).toBe(true);
    expect(sent).toEqual([
      { response: { id: 1, error: { code: "forbidden", message: "Not for you." } } },
      {
        response: {
          id: 2,
          error: { code: "not-found", message: "there is nothing to subscribe to by that name" },
        },
      },
    ]);
  });

  it("stops the cleanup of a handler that was cancelled while it started", async () => {
    let cleaned = 0;
    const { router, later } = rig({
      topics: {
        late: async () => {
          await new Promise((resolve) => setTimeout(resolve, 10));
          return () => void (cleaned += 1);
        },
      },
    });
    router.handle({ subscribe: { id: 1, topic: "late" } });
    router.handle({ cancel: { id: 1 } });
    await new Promise((resolve) => setTimeout(resolve, 30));
    await later();
    expect(cleaned).toBe(1);
  });

  it("pushes nothing that does not go: a value that is not JSON, or that the view no longer listens to", async () => {
    let push: ((value: unknown) => boolean) | undefined;
    let open = true;
    const { router, later } = rig(
      {
        topics: {
          t: (_params, subscription) => {
            push = subscription.push;
          },
        },
      },
      () => open,
    );
    router.handle({ subscribe: { id: 1, topic: "t" } });
    await later();
    const loop: Record<string, unknown> = {};
    loop["self"] = loop;
    expect(push?.(loop)).toBe(false);
    expect(push?.({ ok: true })).toBe(true);
    open = false;
    expect(push?.({ ok: true })).toBe(false);
  });
});

describe("end", () => {
  it("cancels every call and stops every subscription: the view is over", async () => {
    const signals: AbortSignal[] = [];
    let cleaned = 0;
    const { router, sent, later } = rig({
      calls: {
        slow: (_params, call) => {
          signals.push(call.signal);
          return new Promise(() => {});
        },
      },
      topics: {
        t: (_params, subscription) => {
          signals.push(subscription.signal);
          return () => void (cleaned += 1);
        },
      },
    });
    router.handle({ call: { id: 1, method: "slow" } });
    router.handle({ subscribe: { id: 2, topic: "t" } });
    await later();
    router.end();
    expect(signals.map((s) => s.aborted)).toEqual([true, true]);
    expect(cleaned).toBe(1);
    expect(sent).toEqual([]);
    // Nothing is left out: the same ids can be used by the next view.
    router.handle({ call: { id: 1, method: "slow" } });
    await later();
    expect(signals).toHaveLength(3);
  });
});

describe("what it ignores (§9.2)", () => {
  it("is not a request: anything but a call, a subscribe or a cancel, however it is malformed", async () => {
    const { router, sent, later } = rig({ calls: { list: () => ({}) } });
    for (const message of [
      "",
      "not json",
      "[]",
      "null",
      '{"response":{"id":1,"result":{}}}',
      '{"call":{"id":0,"method":"list"}}',
      '{"call":{"id":1.5,"method":"list"}}',
      '{"call":{"id":1}}',
      '{"call":{"id":1,"method":"list"},"cancel":{"id":1}}',
      '{"call":3}',
      '{"cancel":{}}',
    ])
      expect(router.handle(message), message).toBe(false);
    await later();
    expect(sent).toEqual([]);
  });

  it("answers a cancel of an id that is not out with nothing", () => {
    const { router, sent } = rig({});
    expect(router.handle({ cancel: { id: 99 } })).toBe(true);
    expect(sent).toEqual([]);
  });
});

describe("serve", () => {
  it("answers through the same router: what serve writes is what the router writes", async () => {
    const handlers: RouterHandlers = {
      calls: {
        refuse: () => {
          throw new ViewError("no-such-container", "There is no such container.");
        },
        list: () => ({ containers: ["web"] }),
        broken: () => {
          throw new Error("disk is full");
        },
      },
    };
    const { router, payloads, later } = rig(handlers);
    const requests = [
      { call: { id: 1, method: "refuse" } },
      { call: { id: 2, method: "list" } },
      { call: { id: 3, method: "broken" } },
      { call: { id: 4, method: "nothing" } },
    ];
    for (const request of requests) router.handle(request);
    await later();

    const host = new Rig();
    await shown(host, handlers);
    for (const request of requests) host.sayView(request);
    await host.settle();
    const written = host
      .messages()
      .filter((m) => m.kind === "msg")
      .map((m) => m.payload);
    expect([...written].sort()).toEqual([...payloads].sort());
  });
});
