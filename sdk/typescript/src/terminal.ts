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

/** A {@link Terminal} that reads and sets the modes with `stty`. */
export class SttyTerminal implements Terminal {
  readonly #run: RunStty;
  #saved: string | undefined;

  constructor(run: RunStty = runStty) {
    this.#run = run;
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
