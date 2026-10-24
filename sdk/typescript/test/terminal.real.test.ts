/**
 * The modes on a real tty. Node cannot set ISIG apart from ICANON and ECHO, so
 * `SttyTerminal` has `stty` do it; whether the flags that come out are the ones
 * §4.9 asks for can only be seen on a tty, and a pty is what `script` gives a
 * child. The child runs the library's own source (type stripping, no build).
 */

import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const terminal = fileURLToPath(new URL("../src/terminal.ts", import.meta.url));

const child = `
import { SttyTerminal, runStty } from ${JSON.stringify(`file://${terminal}`)};
const modes = new SttyTerminal();
const before = runStty(["-a"]).stdout;
modes.enter();
const inside = runStty(["-a"]).stdout;
modes.restore();
const after = runStty(["-a"]).stdout;
console.log("PENCERE " + JSON.stringify({ before, inside, after }));
`;

function quote(text: string): string {
  return `'${text.replaceAll("'", `'\\''`)}'`;
}

/**
 * Runs `child` with a tty as stdin and stdout, and returns what it printed. `script`
 * wants a pipe or a file for its own stdin, not the socket that `spawn` makes, so
 * a shell makes the pipe.
 */
function underATty<T>(code: string, typed = ""): T | undefined {
  const flags = ["--experimental-strip-types", "--input-type=module", "-e", code];
  const command = [process.execPath, ...flags].map(quote).join(" ");
  const script =
    process.platform === "darwin"
      ? `script -q /dev/null ${command}`
      : `script -qec ${quote(command)} /dev/null`;
  const run = spawnSync("sh", ["-c", `printf '%s' ${quote(typed)} | ${script}`], {
    encoding: "utf8",
    timeout: 30_000,
  });
  const line = run.stdout?.split(/\r?\n/).find((l) => l.includes("PENCERE {"));
  return line === undefined ? undefined : JSON.parse(line.slice(line.indexOf("PENCERE ") + 8));
}

/** The state of a flag in the output of `stty -a`: `true` when it is on, `false` when it is `-flag`. */
function flag(output: string, name: string): boolean | undefined {
  const words = output.split(/[\s;]+/);
  if (words.includes(name)) return true;
  if (words.includes(`-${name}`)) return false;
  return undefined;
}

const supported =
  (process.platform === "darwin" || process.platform === "linux") &&
  spawnSync("sh", ["-c", "command -v script >/dev/null && command -v stty >/dev/null"]).status ===
    0;

describe.skipIf(!supported)("SttyTerminal on a tty", () => {
  const seen = supported
    ? underATty<{ before: string; inside: string; after: string }>(child)
    : undefined;

  it("reads the modes of the tty it runs on", () => {
    expect(seen, "the child printed nothing: script or stty is not usable here").toBeDefined();
    expect(flag(seen?.before ?? "", "icanon")).toBe(true);
    expect(flag(seen?.before ?? "", "echo")).toBe(true);
    expect(flag(seen?.before ?? "", "isig")).toBe(true);
  });

  it("leaves ISIG on and turns canonical mode and echo off (§4.9)", () => {
    expect(flag(seen?.inside ?? "", "icanon")).toBe(false);
    expect(flag(seen?.inside ?? "", "echo")).toBe(false);
    expect(flag(seen?.inside ?? "", "isig")).toBe(true);
  });

  it("gives them back", () => {
    for (const name of ["icanon", "echo", "isig"])
      expect(flag(seen?.after ?? "", name), name).toBe(flag(seen?.before ?? "", name));
  });
});

/**
 * What a program that said `bye` does with the input the host wrote and it did not read (§10.3):
 * Node has no `tcflush`, so `discardInput` reads the queue empty. The child is typed three
 * characters before it has entered its modes, as a host writes into a tty, and looks at the queue
 * afterwards; the control child looks at it without discarding, to show the characters were there.
 */
const queued = (discard: boolean) => `
import { SttyTerminal, readQueued, runStty } from ${JSON.stringify(`file://${terminal}`)};
// Node's stream on the tty makes the descriptor one that does not wait: the read then fails
// with EAGAIN on an empty queue, as it does in a program that reads its stdin.
void process.stdin;
const modes = new SttyTerminal(runStty, 0);
modes.enter();
// The characters arrive while the child is starting; they are in the queue by now.
Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 600);
${discard ? "modes.discardInput();" : ""}
runStty(["min", "0", "time", "0"]);
const into = new Uint8Array(64);
let left = "";
for (;;) {
  const got = readQueued(0, into);
  if (got <= 0) break;
  left += Buffer.from(into.subarray(0, got)).toString("latin1");
}
modes.restore();
console.log("PENCERE " + JSON.stringify({ left }));
`;

describe.skipIf(!supported)("SttyTerminal.discardInput on a tty", () => {
  it("leaves the characters of the queue in it when nothing discards them", () => {
    const seen = underATty<{ left: string }>(queued(false), "abc");
    expect(seen, "the child printed nothing: script or stty is not usable here").toBeDefined();
    expect(seen?.left).toContain("abc");
  });

  it("reads them away, so that whatever reads the tty next does not find them", () => {
    const seen = underATty<{ left: string }>(queued(true), "abc");
    expect(seen, "the child printed nothing: script or stty is not usable here").toBeDefined();
    expect(seen?.left).toBe("");
  });
});
