/**
 * What the view may call and subscribe to: the handlers on their own, without a terminal.
 * (The program and the page together, in a pane, are the stand-in host's end-to-end test.)
 */

import { ViewError } from "@pencere/sdk";
import type { CallContext, Subscription } from "@pencere/sdk";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createProgram, sampleContainers, stats, table } from "../src/containers.js";

const call = (method: string): CallContext => ({
  method,
  signal: new AbortController().signal,
  progress: () => {},
});

describe("the calls", () => {
  it("lists the containers", async () => {
    const { calls } = createProgram();
    const listed = (await calls["list"]?.(undefined, call("list"))) as {
      containers: { id: string }[];
    };
    expect(listed.containers.map((container) => container.id)).toEqual([
      "web",
      "api",
      "db",
      "cache",
      "worker",
    ]);
  });

  it("stops and starts a container, and answers with the list and the words for the person", async () => {
    const { calls } = createProgram();
    const stopped = (await calls["stop"]?.({ id: "web" }, call("stop"))) as {
      containers: { id: string; state: string }[];
      message: string;
    };
    expect(stopped.message).toBe("Stopped web (nginx:1.27)");
    expect(stopped.containers.find((container) => container.id === "web")?.state).toBe("stopped");

    const started = (await calls["start"]?.({ id: "web" }, call("start"))) as {
      containers: { id: string; state: string }[];
      message: string;
    };
    expect(started.message).toBe("Started web (nginx:1.27)");
    expect(started.containers.find((container) => container.id === "web")?.state).toBe("running");
  });

  it("refuses a container that does not exist, and a call that does not say which", () => {
    const { calls } = createProgram();
    for (const params of [{ id: "nope" }, {}, null, undefined, "web", { id: 3 }]) {
      expect(() => calls["stop"]?.(params, call("stop"))).toThrow(ViewError);
    }
    expect(() => calls["stop"]?.({ id: "nope" }, call("stop"))).toThrow(
      'There is no container "nope".',
    );
  });
});

describe("the stats", () => {
  it("are those of the running containers only", () => {
    const containers = sampleContainers();
    expect(stats(containers, 0).map((stat) => stat.id)).toEqual(["web", "api", "db"]);
    const [web] = containers;
    if (web !== undefined) web.state = "stopped";
    expect(stats(containers, 0).map((stat) => stat.id)).toEqual(["api", "db"]);
  });

  it("move with the second, and are the same at the same second", () => {
    const containers = sampleContainers();
    expect(stats(containers, 0)).toEqual(stats(containers, 0));
    expect(stats(containers, 0)).not.toEqual(stats(containers, 1));
    for (const tick of [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]) {
      for (const stat of stats(containers, tick)) {
        expect(stat.cpu).toBeGreaterThanOrEqual(0);
        expect(stat.memory).toBeGreaterThan(0);
      }
    }
  });
});

describe("the topic stats", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  const subscription = (): { pushed: unknown[]; subscription: Subscription } => {
    const pushed: unknown[] = [];
    return {
      pushed,
      subscription: {
        topic: "stats",
        signal: new AbortController().signal,
        push: (value) => {
          pushed.push(value);
          return true;
        },
      },
    };
  };

  it("pushes at once, then once a second, until the subscription ends", async () => {
    const { topics } = createProgram();
    const { pushed, subscription: feed } = subscription();
    const stop = await topics["stats"]?.(undefined, feed);
    expect(pushed).toHaveLength(1);
    expect(pushed[0]).toMatchObject({ stats: [{ id: "web" }, { id: "api" }, { id: "db" }] });

    vi.advanceTimersByTime(2500);
    expect(pushed).toHaveLength(3);
    expect(typeof stop).toBe("function");
    if (typeof stop === "function") stop();
    vi.advanceTimersByTime(5000);
    expect(pushed).toHaveLength(3);
  });

  it("keeps to the running containers as they change", async () => {
    const containers = sampleContainers();
    const { calls, topics } = createProgram(containers);
    const { pushed, subscription: feed } = subscription();
    const stop = await topics["stats"]?.({ every: 100 }, feed);
    await calls["stop"]?.({ id: "api" }, call("stop"));
    vi.advanceTimersByTime(100);
    expect(pushed).toHaveLength(2);
    expect(pushed[1]).toMatchObject({ stats: [{ id: "web" }, { id: "db" }] });
    if (typeof stop === "function") stop();
  });
});

describe("the text", () => {
  it("is a row a container, with its state", () => {
    const text = table(sampleContainers());
    expect(text.split("\n")).toHaveLength(5);
    expect(text).toContain("● web");
    expect(text).toContain("○ cache");
    expect(text).toContain("8080→80");
  });
});
