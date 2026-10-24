/**
 * The tty's modes (specification §4.9) and the escape sequences a program uses
 * to take the screen (§5.1, §6.5).
 *
 * Before it sends `hello` a program puts its tty in non-canonical mode, with echo
 * off, and **leaves ISIG on**, so that the terminal's replies are read and ⌃C
 * still reaches the program as SIGINT (§10.4). Node cannot do that by itself:
 * `setRawMode(true)` clears ISIG along with ICANON and ECHO, and the runtime has
 * no call that changes one flag. The modes are therefore set with `stty`, which
 * every system that has a tty has, on the tty that is the program's own stdin,
 * or, for a filter in a pipeline, on the terminal the program opened itself.
 */

import { spawnSync } from "node:child_process";
import { readSync } from "node:fs";

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

/** `stty` on the tty that is `stdin`: the program's own stdin (`inherit`), or a descriptor of a tty. */
export function sttyOn(stdin: "inherit" | number): RunStty {
  return (args) => {
    const result = spawnSync("stty", [...args], {
      stdio: [stdin, "pipe", "ignore"],
      encoding: "utf8",
      timeout: 2000,
    });
    return { status: result.status, stdout: result.stdout ?? "" };
  };
}

/** `stty`, as `spawnSync` runs it with the tty inherited as its stdin. */
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
  #saved: string | undefined;

  constructor(run: RunStty = runStty, input?: number, read: ReadQueued = readQueued) {
    this.#run = run;
    this.#input = input;
    this.#read = read;
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
