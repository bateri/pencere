/**
 * The modes on a real tty, on each runtime the library runs on. No runtime can set ISIG apart from
 * ICANON and ECHO (Node's `setRawMode(true)` clears it with them), so `SttyTerminal` has `stty` do
 * it, and the way `stty` is run differs between runtimes: whether the flags that come out are the
 * ones §4.9 asks for, and are given back, can only be seen on a tty, and a pty is what `script` gives
 * a child. The child runs the library's own source (type stripping, no build), under Node, and under
 * Deno and Bun when they are installed; a runtime that is not has its tests skipped.
 *
 * Where to look for them: `PENCERE_DENO` and `PENCERE_BUN` name the executable; otherwise the `PATH`,
 * and for Bun the place its installer puts it.
 */

import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, describe, expect, it } from "vitest";

const terminal = fileURLToPath(new URL("../src/terminal.ts", import.meta.url));

const child = `
import { SttyTerminal, runStty, runtime } from ${JSON.stringify(`file://${terminal}`)};
const modes = new SttyTerminal();
const before = runStty(["-a"]).stdout;
modes.enter();
const inside = runStty(["-a"]).stdout;
modes.restore();
const after = runStty(["-a"]).stdout;
console.log("PENCERE " + JSON.stringify({ runtime, before, inside, after }));
`;

function quote(text: string): string {
  return `'${text.replaceAll("'", `'\\''`)}'`;
}

/** The executable of `name` (`deno` or `bun`), if there is one. */
function find(name: "deno" | "bun"): string | undefined {
  const named = process.env[`PENCERE_${name.toUpperCase()}`];
  if (named !== undefined && named !== "") return named;
  const found = spawnSync("sh", ["-c", `command -v ${name}`], { encoding: "utf8" });
  if (found.status === 0 && found.stdout.trim() !== "") return found.stdout.trim();
  const installed = join(homedir(), ".bun", "bin", name);
  return name === "bun" && existsSync(installed) ? installed : undefined;
}

interface Runtime {
  readonly name: "node" | "deno" | "bun";
  readonly path: string;
}

const scratch = mkdtempSync(join(tmpdir(), "pencere-tty-"));
afterAll(() => rmSync(scratch, { recursive: true, force: true }));
let written = 0;

/** The command that runs `code` as a module on the runtime. */
function command(runtime: Runtime, code: string): string[] {
  if (runtime.name === "node")
    return [runtime.path, "--experimental-strip-types", "--input-type=module", "-e", code];
  // Deno and Bun run a file.
  written += 1;
  const file = join(scratch, `child-${written}.mjs`);
  writeFileSync(file, code);
  return runtime.name === "deno" ? [runtime.path, "run", "-A", file] : [runtime.path, file];
}

/**
 * Runs `code` with a tty as stdin and stdout, and returns what it printed. `script` wants a pipe or
 * a file for its own stdin, not the socket that `spawn` makes, so a shell makes the pipe.
 */
function underATty<T>(runtime: Runtime, code: string, typed = ""): T | undefined {
  const words = command(runtime, code).map(quote).join(" ");
  const script =
    process.platform === "darwin"
      ? `script -q /dev/null ${words}`
      : `script -qec ${quote(words)} /dev/null`;
  const run = spawnSync("sh", ["-c", `printf '%s' ${quote(typed)} | ${script}`], {
    encoding: "utf8",
    timeout: 60_000,
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

const runtimes = (["node", "deno", "bun"] as const).map((name) => {
  const path = name === "node" ? process.execPath : find(name);
  return { runtime: { name, path: path ?? name } satisfies Runtime, present: path !== undefined };
});

/**
 * What a program that said `bye` does with the input the host wrote and it did not read (§10.3):
 * no runtime has `tcflush`, so `discardInput` reads the queue empty. The child is typed three
 * characters before it has entered its modes, as a host writes into a tty, and looks at the queue
 * afterwards; the control child looks at it without discarding, to show the characters were there.
 */
const queued = (discard: boolean) => `
import { SttyTerminal, readQueued, runStty } from ${JSON.stringify(`file://${terminal}`)};
// The stream on the tty makes the descriptor one that does not wait, in a runtime that has one: the
// read then fails with EAGAIN on an empty queue, as it does in a program that reads its stdin.
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
  left += String.fromCharCode(...into.subarray(0, got));
}
modes.restore();
console.log("PENCERE " + JSON.stringify({ left }));
`;

/**
 * What a program does in an `exit` handler, where nothing runs after it: `waitFor` reads the tty
 * without the event loop, hands what it reads to a function, and ends when the function has what it
 * waited for or the time is up. The child is typed three characters before it has entered its modes
 * (`script` adds an end-of-transmission when its input ends, which is not ours to count).
 */
const waited = (timeout: number, wanted: string) => `
import { SttyTerminal, runStty } from ${JSON.stringify(`file://${terminal}`)};
void process.stdin;
const modes = new SttyTerminal(runStty, 0);
modes.enter();
Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 600);
let heard = "";
const started = Date.now();
modes.waitFor((bytes) => {
  heard += String.fromCharCode(...bytes);
  return heard.includes(${JSON.stringify(wanted)});
}, ${timeout});
const elapsed = Date.now() - started;
modes.restore();
console.log("PENCERE " + JSON.stringify({ heard, elapsed }));
`;

for (const { runtime, present } of runtimes) {
  describe.skipIf(!supported || !present)(`SttyTerminal on a tty, on ${runtime.name}`, () => {
    const seen =
      supported && present
        ? underATty<{ runtime: string; before: string; inside: string; after: string }>(
            runtime,
            child,
          )
        : undefined;

    it("is the runtime it runs on", () => {
      expect(seen, "the child printed nothing: script or stty is not usable here").toBeDefined();
      expect(seen?.runtime).toBe(runtime.name);
    });

    it("reads the modes of the tty it runs on", () => {
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

  describe.skipIf(!supported || !present)(
    `SttyTerminal.discardInput on a tty, on ${runtime.name}`,
    () => {
      it("leaves the characters of the queue in it when nothing discards them", () => {
        const seen = underATty<{ left: string }>(runtime, queued(false), "abc");
        expect(seen, "the child printed nothing: script or stty is not usable here").toBeDefined();
        expect(seen?.left).toContain("abc");
      });

      it("reads them away, so that whatever reads the tty next does not find them", () => {
        const seen = underATty<{ left: string }>(runtime, queued(true), "abc");
        expect(seen, "the child printed nothing: script or stty is not usable here").toBeDefined();
        expect(seen?.left).toBe("");
      });
    },
  );

  /**
   * What the child heard, without the byte `script` writes to the tty when its own input ends: ^D
   * on macOS, NUL with util-linux on Linux. Neither is anything the test typed.
   */
  const typed = (heard: string | undefined): string | undefined =>
    heard?.replaceAll("\u0004", "").replaceAll("\u0000", "");

  describe.skipIf(!supported || !present)(
    `SttyTerminal.waitFor on a tty, on ${runtime.name}`,
    () => {
      it("reads what the tty holds and hands it over, and ends at once when the function has what it waited for", () => {
        const seen = underATty<{ heard: string; elapsed: number }>(
          runtime,
          waited(5000, "abc"),
          "abc",
        );
        expect(seen, "the child printed nothing: script or stty is not usable here").toBeDefined();
        expect(typed(seen?.heard)).toBe("abc");
        expect(seen?.elapsed).toBeLessThan(2000);
      });

      it("gives up when the time is up and nothing came, and does not wait for a byte", () => {
        const seen = underATty<{ heard: string; elapsed: number }>(
          runtime,
          waited(300, "never"),
          "",
        );
        expect(seen, "the child printed nothing: script or stty is not usable here").toBeDefined();
        expect(typed(seen?.heard)).toBe("");
        expect(seen?.elapsed).toBeGreaterThanOrEqual(280);
        expect(seen?.elapsed).toBeLessThan(3000);
      });
    },
  );
}
