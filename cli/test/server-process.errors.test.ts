/**
 * What a started process's own events do to `ServerProcess`, with the process lent to the
 * test: an `error` event of a child that is already running comes from a signal that could
 * not be sent or a write that did not go through, and may come more than once.
 */

import type { ChildProcess } from "node:child_process";
import { describe, expect, it, vi } from "vitest";

import { ServerProcess } from "../src/server-process.js";
import { Tail } from "../src/tail.js";
import { node } from "./support.js";

const children = vi.hoisted(() => [] as ChildProcess[]);

vi.mock("node:child_process", async (importOriginal) => {
  const real = await importOriginal<typeof import("node:child_process")>();
  return {
    ...real,
    spawn: (...args: Parameters<typeof real.spawn>) => {
      const child = real.spawn(...args);
      children.push(child);
      return child;
    },
  };
});

describe("the process of a server", () => {
  it("hears every error its child raises after it has started, and is not ended by the second", async () => {
    const server = new ServerProcess(node("setInterval(() => {}, 1000)"), new Tail(), {
      stdin: 100,
      term: 100,
      kill: 2000,
    });
    const errors: string[] = [];
    server.onerror = (error) => errors.push(error.message);
    await server.start();
    const [child] = children;
    // With a listener that is taken off after the first, the second is an error nobody handles.
    expect(() => {
      child?.emit("error", new Error("the first"));
      child?.emit("error", new Error("the second"));
    }).not.toThrow();
    expect(errors).toEqual(["the first", "the second"]);
    await server.close();
  });
});
