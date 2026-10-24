/**
 * The tty's modes (specification §4.9) and the escape sequences a program uses
 * to take the screen (§5.1, §6.5).
 *
 * Before it sends `hello` a program puts its tty in non-canonical mode, with echo
 * off, and **leaves ISIG on**, so that the terminal's replies are read and ⌃C
 * still reaches the program as SIGINT (§10.4). No runtime can do that by itself:
 * Node's `setRawMode(true)` clears ISIG along with ICANON and ECHO, and none has a
 * call that changes one flag. The modes are therefore set with `stty`, which
 * every system that has a tty has, on the tty that is the program's own stdin,
 * or, for a filter in a pipeline, on the terminal the program opened itself.
 * Only the way `stty` is run differs between runtimes ({@link runtime}).
 *
 * **On Node, `stty` never gets the program's input as one of its standard streams.** Node reads
 * the tty in its event loop and counts on the descriptor not waiting: a read that `poll` promised
 * bytes for can find the queue emptied under it, because ⌃C flushes the tty's input (Appendix B,
 * fact 4), and only a descriptor that does not wait turns that read into nothing instead of a wait
 * for a byte that never comes. But Node turns `O_NONBLOCK` off on the standard streams it hands a
 * child, and the flag is the open file's, which the child shares with the stream the program reads:
 * after one `stty` with the tty as its stdin the program's own reads wait, and a ⌃C that empties
 * the queue the loop was about to read leaves the program in that read, where its SIGINT is never
 * heard. So `stty` gets the tty as descriptor 3, which a shell makes its stdin (see `sttyOn`).
 *
 * This file imports nothing of the library, so that the tests of the tty can run it from source
 * under each runtime.
 */

import { spawnSync } from "node:child_process";
import { readSync } from "node:fs";

/** The runtimes the library runs on. */
export type Runtime = "node" | "deno" | "bun";

/** The runtime of `scope`'s globals: Deno and Bun each have a global of their own, Node has neither. */
export function detectRuntime(scope: object = globalThis): Runtime {
  if ("Deno" in scope) return "deno";
  if ("Bun" in scope) return "bun";
  return "node";
}

/** The runtime this program runs on. */
export const runtime: Runtime = detectRuntime();

/** `CSI c`: primary device attributes, the fence after `hello` (§5.1). */
export const DEVICE_ATTRIBUTES = "\x1b[c";
/** `CSI ? 1049 h`: the alternate screen (§6.5). */
export const ENTER_ALTERNATE_SCREEN = "\x1b[?1049h";
/** `CSI ? 1049 l`: back to the primary screen. */
export const LEAVE_ALTERNATE_SCREEN = "\x1b[?1049l";

/** The state of the tty, taken and given back. */
export interface Terminal {
  /**
   * Puts the tty in the state §4.9 asks of a program that sends `hello`: not
   * canonical, no echo, ISIG on, and a read that waits for one byte. Remembers
   * what it found.
   *
   * @throws when it cannot: a program that cannot is a program with no host.
   */
  enter(): void;
  /**
   * Throws away what the tty has queued for this program and it has not read
   * (`tcflush` with `TCIFLUSH`, §10.3): the last thing a program that said `bye`
   * does with its input, before it gives the modes back, so that what the host
   * wrote and the program did not read is not left for whatever reads the tty next.
   */
  discardInput(): void;
  /**
   * Reads what the tty has for this program, without the event loop, handing each piece to `feed`,
   * until `feed` says it has what it waited for (`true`) or `timeout` milliseconds have passed:
   * what a program that is leaving does where nothing runs after it (an `exit` handler), to read
   * the host's last word (§10.3) instead of leaving it for the shell. Does nothing before `enter`.
   */
  waitFor(feed: (bytes: Uint8Array) => boolean, timeout: number): void;
  /** Gives back what `enter` found. Safe to call again, and without `enter`. */
  restore(): void;
}

/** What `stty` said. */
export interface SttyResult {
  readonly status: number | null;
  readonly stdout: string;
}

/** Runs `stty` with `args` on the program's tty. */
export type RunStty = (args: readonly string[]) => SttyResult;

/** The part of `Deno.Command` that runs a command and waits for it. */
interface DenoCommand {
  new (
    command: string,
    options: {
      args: string[];
      stdin: "inherit" | "null";
      stdout: "piped";
      stderr: "null";
    },
  ): { outputSync(): { success: boolean; code: number; stdout: Uint8Array } };
}

/** How long `stty` has to answer before it is given up on (Deno's call has no limit to give). */
const STTY_TIMEOUT = 2000;

/**
 * The tty that is the program's controlling terminal, for `stty` to read: a program that
 * asks about a descriptor of its own has to hand the descriptor over, and only Node can.
 * Deno and Bun run a shell whose stdin is the terminal, which is the same terminal: the
 * only descriptor this library asks about is one it opened on `/dev/tty` itself.
 */
const ON_CONTROLLING_TERMINAL = ['exec stty "$@" </dev/tty', "stty"] as const;

/**
 * `stty` on the tty that is the child's descriptor 3: how Node runs it, so that the descriptor
 * the program reads its input on keeps the `O_NONBLOCK` Node gave it (the module's comment says
 * why). Node changes the flag only on the child's standard streams; the shell's redirection makes
 * descriptor 3 the stdin of `stty` with the flag as it is, and `stty` only asks the tty about its
 * modes.
 */
const ON_DESCRIPTOR_3 = ['exec stty "$@" <&3', "stty"] as const;

/** `stty` with `args` on the tty that is the program's descriptor `fd`, as Node runs it. */
function sttyOnDescriptor(fd: number, args: readonly string[]): SttyResult {
  const result = spawnSync("sh", ["-c", ...ON_DESCRIPTOR_3, ...args], {
    stdio: ["ignore", "pipe", "ignore", fd],
    encoding: "utf8",
    timeout: STTY_TIMEOUT,
  });
  return { status: result.status, stdout: result.stdout ?? "" };
}

/** Runs `command` with `args`, and `stdin` as its stdin, and waits for it. */
function run(command: string, args: readonly string[], stdin: "inherit" | "null"): SttyResult {
  if (runtime === "deno") {
    // `node:child_process` on Deno never sees `stty` end when `stty` sets the modes (it reads
    // them well), so Deno's own call runs it.
    const { Command } = (globalThis as unknown as { Deno: { Command: DenoCommand } }).Deno;
    const done = new Command(command, {
      args: [...args],
      stdin,
      stdout: "piped",
      stderr: "null",
    }).outputSync();
    return { status: done.success ? 0 : done.code, stdout: new TextDecoder().decode(done.stdout) };
  }
  const result = spawnSync(command, [...args], {
    stdio: [stdin === "inherit" ? "inherit" : "ignore", "pipe", "ignore"],
    encoding: "utf8",
    timeout: STTY_TIMEOUT,
  });
  return { status: result.status, stdout: result.stdout ?? "" };
}

/**
 * `stty` on the tty that is `stdin`: the program's own stdin (`inherit`), or a descriptor of a tty.
 * The descriptor is for a terminal the program opened itself (`/dev/tty`). Node hands either to
 * `stty` as descriptor 3, never as its stdin ({@link ON_DESCRIPTOR_3}): the program's stdin is
 * descriptor 0. Deno and Bun read their input in a way that does not count on the descriptor not
 * waiting; they let `stty` inherit the stdin, and for a descriptor, which they cannot pass to a
 * child, name `/dev/tty`.
 */
export function sttyOn(stdin: "inherit" | number): RunStty {
  return (args) => {
    if (runtime === "node") return sttyOnDescriptor(stdin === "inherit" ? 0 : stdin, args);
    if (stdin === "inherit") return run("stty", args, "inherit");
    return run("sh", ["-c", ...ON_CONTROLLING_TERMINAL, ...args], "null");
  };
}

/** `stty` on the program's own stdin, which the child inherits. */
export const runStty: RunStty = sttyOn("inherit");

/** What `enter` asks of the tty: the modes of §4.9. */
export const PROGRAM_MODES: readonly string[] = [
  "-icanon",
  "-echo",
  "isig",
  "min",
  "1",
  "time",
  "0",
];

/** Reads what is queued on the descriptor `fd` into `into` without waiting: how many bytes, 0 when there are none or it cannot be read. */
export type ReadQueued = (fd: number, into: Uint8Array) => number;

/** {@link ReadQueued} on the file system's `read`: the descriptor must not wait for input (`VMIN` 0, or a descriptor that is not blocking). */
export const readQueued: ReadQueued = (fd, into) => {
  try {
    return readSync(fd, into, 0, into.length, null);
  } catch {
    // `EAGAIN` on a descriptor that does not wait, `EIO` on a tty that is gone: nothing more to read.
    return 0;
  }
};

/** What `discardInput` reads at most: far more than any tty queues (Appendix B), so that it ends. */
const DISCARD_LIMIT = 4 * 1024 * 1024;

/** How long `waitFor` sleeps between two reads of a queue that is empty. */
const POLL_MS = 4;

/** The time, and a wait without the event loop: what `waitFor` is given, so that a test can say what time it is. */
export interface Clock {
  now(): number;
  /** Stops for `ms` milliseconds, blocking. */
  sleep(ms: number): void;
}

const systemClock: Clock = {
  now: () => Date.now(),
  sleep: (ms) => void Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms),
};

/**
 * A {@link Terminal} that reads and sets the modes with `stty`.
 *
 * Node has no `tcflush` either. `discardInput` reads the queue empty instead, on the descriptor
 * `input` (the program's stdin, or the descriptor of the terminal a filter opened): a descriptor of
 * the tty itself, not the stream Node reads it with, so that reading it takes what the stream has
 * not. It sets `VMIN` 0 first, so that a read of an empty queue returns at once and never waits,
 * and gives the descriptor up unread when that cannot be set. Without an `input` there is nothing
 * to read and it does nothing.
 */
export class SttyTerminal implements Terminal {
  readonly #run: RunStty;
  readonly #input: number | undefined;
  readonly #read: ReadQueued;
  readonly #clock: Clock;
  #saved: string | undefined;

  constructor(
    run: RunStty = runStty,
    input?: number,
    read: ReadQueued = readQueued,
    clock: Clock = systemClock,
  ) {
    this.#run = run;
    this.#input = input;
    this.#read = read;
    this.#clock = clock;
  }

  waitFor(feed: (bytes: Uint8Array) => boolean, timeout: number): void {
    const fd = this.#input;
    if (fd === undefined || this.#saved === undefined) return;
    // A read that waits for a byte on an empty queue would not return when the time is up.
    if (this.#run(["min", "0", "time", "0"]).status !== 0) return;
    const into = new Uint8Array(4096);
    const deadline = this.#clock.now() + timeout;
    for (;;) {
      const got = this.#read(fd, into);
      if (got > 0 && feed(into.subarray(0, got))) return;
      const left = deadline - this.#clock.now();
      if (left <= 0) return;
      // An empty queue is asked again after a moment; a full one at once, until the time is up.
      if (got <= 0) this.#clock.sleep(Math.min(POLL_MS, left));
    }
  }

  discardInput(): void {
    const fd = this.#input;
    if (fd === undefined || this.#saved === undefined) return;
    // A read that waits for a byte on an empty queue is a program that never ends.
    if (this.#run(["min", "0", "time", "0"]).status !== 0) return;
    const into = new Uint8Array(4096);
    for (let total = 0; total < DISCARD_LIMIT;) {
      const got = this.#read(fd, into);
      if (got <= 0) return;
      total += got;
    }
  }

  enter(): void {
    if (this.#saved !== undefined) return;
    const found = this.#run(["-g"]);
    const saved = found.stdout.trim();
    if (found.status !== 0 || saved === "") throw new Error("the terminal's modes cannot be read");
    this.#saved = saved;
    if (this.#run(PROGRAM_MODES).status !== 0) {
      this.restore();
      throw new Error("the terminal's modes cannot be set");
    }
  }

  restore(): void {
    const saved = this.#saved;
    if (saved === undefined) return;
    this.#saved = undefined;
    this.#run([saved]);
  }
}
