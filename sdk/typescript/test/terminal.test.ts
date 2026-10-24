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

describe("SttyTerminal.waitFor", () => {
  /** A clock that moves only when the terminal sleeps or a read says it took time. */
  function clockAt() {
    let now = 1000;
    const sleeps: number[] = [];
    return {
      sleeps,
      advance: (ms: number) => void (now += ms),
      clock: {
        now: () => now,
        sleep: (ms: number) => {
          sleeps.push(ms);
          now += ms;
        },
      },
    };
  }

  /** A tty that holds `chunks`, one for each read; empty after them. */
  function holding(...chunks: string[]) {
    const left = chunks.map((chunk) => Buffer.from(chunk, "latin1"));
    return (_fd: number, into: Uint8Array): number => {
      const next = left.shift();
      if (next === undefined) return 0;
      into.set(next);
      return next.length;
    };
  }

  it("lets a read return at once, then hands each piece to feed until feed has what it wanted", () => {
    const { calls, run } = fake();
    const { clock } = clockAt();
    const seen: string[] = [];
    const terminal = new SttyTerminal(run, 7, holding("typed", "an answer", "more"), clock);
    terminal.enter();
    terminal.waitFor((bytes) => {
      seen.push(Buffer.from(bytes).toString("latin1"));
      return seen.length === 2;
    }, 500);
    expect(calls[2]).toEqual(["min", "0", "time", "0"]);
    // It stops at the answer: the piece after it is left for the discard.
    expect(seen).toEqual(["typed", "an answer"]);
  });

  it("asks an empty queue again after a moment, and gives up when the time is up", () => {
    const { run } = fake();
    const { clock, sleeps } = clockAt();
    let reads = 0;
    const terminal = new SttyTerminal(
      run,
      7,
      () => {
        reads += 1;
        return 0;
      },
      clock,
    );
    terminal.enter();
    let fed = 0;
    terminal.waitFor(() => (fed += 1) > 0, 30);
    expect(fed).toBe(0);
    // 30 ms in sleeps of at most a few: the time is the clock's, and it ends.
    expect(sleeps.reduce((a, b) => a + b, 0)).toBe(30);
    expect(Math.max(...sleeps)).toBeLessThanOrEqual(10);
    expect(reads).toBe(sleeps.length + 1);
  });

  it("ends even for a queue that never empties and never has the answer", () => {
    const { run } = fake();
    const { clock, advance } = clockAt();
    let reads = 0;
    const terminal = new SttyTerminal(
      run,
      7,
      (_fd, into) => {
        reads += 1;
        advance(1);
        return into.length;
      },
      clock,
    );
    terminal.enter();
    terminal.waitFor(() => false, 25);
    expect(reads).toBeGreaterThanOrEqual(25);
    expect(reads).toBeLessThanOrEqual(26);
  });

  it("does nothing before enter, without a descriptor to read, and when VMIN 0 cannot be set", () => {
    const first = fake();
    let reads = 0;
    const read = () => {
      reads += 1;
      return 0;
    };
    const { clock } = clockAt();
    new SttyTerminal(first.run, 3, read, clock).waitFor(() => true, 10);
    expect(first.calls).toEqual([]);
    const second = fake();
    const noFd = new SttyTerminal(second.run, undefined, read, clock);
    noFd.enter();
    noFd.waitFor(() => true, 10);
    expect(second.calls).toHaveLength(2);
    const third = fake({ "min 0 time 0": { status: 1, stdout: "" } });
    const refused = new SttyTerminal(third.run, 3, read, clock);
    refused.enter();
    refused.waitFor(() => true, 10);
    expect(reads).toBe(0);
  });
});
