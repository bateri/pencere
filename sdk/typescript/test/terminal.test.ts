import { describe, expect, it } from "vitest";

import { PROGRAM_MODES, SttyTerminal } from "../src/terminal.js";
import type { SttyResult } from "../src/terminal.js";

function fake(answers: Record<string, SttyResult> = {}) {
  const calls: string[][] = [];
  const run = (args: readonly string[]): SttyResult => {
    calls.push([...args]);
    return (
      answers[args.join(" ")] ??
      (args[0] === "-g" ? { status: 0, stdout: "500:5:cb00:8a3b\n" } : { status: 0, stdout: "" })
    );
  };
  return { calls, run };
}

describe("the program's modes", () => {
  it("are non-canonical, without echo, with ISIG on and a read that waits for one byte", () => {
    expect(PROGRAM_MODES.join(" ")).toBe("-icanon -echo isig min 1 time 0");
    // ISIG is never turned off: that is what makes ⌃C a SIGINT (§10.4).
    expect(PROGRAM_MODES).not.toContain("-isig");
    expect(PROGRAM_MODES).not.toContain("raw");
  });
});

describe("SttyTerminal", () => {
  it("takes the modes, sets those of §4.9, and gives the first back", () => {
    const { calls, run } = fake();
    const terminal = new SttyTerminal(run);
    terminal.enter();
    expect(calls).toEqual([["-g"], [...PROGRAM_MODES]]);
    terminal.restore();
    expect(calls[2]).toEqual(["500:5:cb00:8a3b"]);
  });

  it("restores once, and not at all when it never entered", () => {
    const { calls, run } = fake();
    const terminal = new SttyTerminal(run);
    terminal.restore();
    expect(calls).toEqual([]);
    terminal.enter();
    terminal.restore();
    terminal.restore();
    expect(calls).toHaveLength(3);
  });

  it("enters once: a second enter does not take the already changed modes as the ones to give back", () => {
    const { calls, run } = fake();
    const terminal = new SttyTerminal(run);
    terminal.enter();
    terminal.enter();
    expect(calls).toHaveLength(2);
  });

  it("throws when the modes cannot be read, and changes nothing", () => {
    const { calls, run } = fake({ "-g": { status: 1, stdout: "" } });
    expect(() => new SttyTerminal(run).enter()).toThrow(/read/);
    expect(calls).toEqual([["-g"]]);
    const empty = fake({ "-g": { status: 0, stdout: "\n" } });
    expect(() => new SttyTerminal(empty.run).enter()).toThrow(/read/);
  });

  it("gives the modes back when it cannot set the new ones", () => {
    const { calls, run } = fake({ [PROGRAM_MODES.join(" ")]: { status: 1, stdout: "" } });
    expect(() => new SttyTerminal(run).enter()).toThrow(/set/);
    expect(calls).toEqual([["-g"], [...PROGRAM_MODES], ["500:5:cb00:8a3b"]]);
  });
});

describe("SttyTerminal.discardInput", () => {
  /** A queue of chunks: what a tty holds that the program has not read. */
  function queue(...chunks: number[]) {
    const reads: number[] = [];
    const left = [...chunks];
    return {
      reads,
      read: (fd: number, into: Uint8Array): number => {
        reads.push(fd);
        return Math.min(left.shift() ?? 0, into.length);
      },
    };
  }

  it("lets a read of an empty queue return at once, then reads the queue empty", () => {
    const { calls, run } = fake();
    const { reads, read } = queue(5, 4096, 12);
    const terminal = new SttyTerminal(run, 7, read);
    terminal.enter();
    terminal.discardInput();
    // VMIN 0 first: a read that waits for a byte of an empty queue would never end.
    expect(calls[2]).toEqual(["min", "0", "time", "0"]);
    expect(reads).toEqual([7, 7, 7, 7]);
    terminal.restore();
    expect(calls[3]).toEqual(["500:5:cb00:8a3b"]);
  });

  it("reads nothing when the queue is empty", () => {
    const { run } = fake();
    const { reads, read } = queue();
    const terminal = new SttyTerminal(run, 3, read);
    terminal.enter();
    terminal.discardInput();
    expect(reads).toEqual([3]);
  });

  it("does not read when VMIN 0 cannot be set: a read that may wait is not taken", () => {
    const { run } = fake({ "min 0 time 0": { status: 1, stdout: "" } });
    const { reads, read } = queue(10);
    const terminal = new SttyTerminal(run, 3, read);
    terminal.enter();
    terminal.discardInput();
    expect(reads).toEqual([]);
  });

  it("does nothing before enter, and without a descriptor to read", () => {
    const first = fake();
    const { reads, read } = queue(10);
    new SttyTerminal(first.run, 3, read).discardInput();
    expect(first.calls).toEqual([]);
    expect(reads).toEqual([]);
    const second = fake();
    const terminal = new SttyTerminal(second.run, undefined, read);
    terminal.enter();
    terminal.discardInput();
    expect(second.calls).toHaveLength(2);
    expect(reads).toEqual([]);
  });

  it("ends even for a queue that never empties", () => {
    const { run } = fake();
    let total = 0;
    const terminal = new SttyTerminal(run, 3, (_fd, into) => {
      total += into.length;
      return into.length;
    });
    terminal.enter();
    terminal.discardInput();
    expect(total).toBeGreaterThan(0);
    expect(total).toBeLessThanOrEqual(5 * 1024 * 1024);
  });
});
