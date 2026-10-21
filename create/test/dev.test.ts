/**
 * The generated project's dev script, run with stand-ins for vite, tsc and
 * pencere: the order of what it does, what it hands `pencere dev`, and that a
 * change builds again while the view is up, with the build's output in a file
 * and not on the screen.
 */

import { spawn } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";

import { plan, write } from "../src/scaffold.js";

const dirs: string[] = [];
afterEach(async () => {
  for (const dir of dirs.splice(0)) await rm(dir, { recursive: true, force: true });
});

/** A project of the React template with a stand-in for each tool the script runs. */
async function project(options: { pencere?: boolean } = {}): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "create-pencere-dev-"));
  dirs.push(dir);
  await write(
    dir,
    await plan(new URL("../templates/", import.meta.url), { name: "my-tool", template: "react" }),
  );
  const bin = join(dir, "node_modules/.bin");
  await mkdir(bin, { recursive: true });
  // Each stand-in notes that it ran, with its arguments, and makes what the real one makes.
  const standIn = async (name: string, body: string) => {
    const path = join(bin, name);
    await writeFile(
      path,
      `#!${process.execPath}
const { appendFileSync, mkdirSync, readFileSync, writeFileSync } = require("node:fs");
const calls = ${JSON.stringify(join(dir, "calls.log"))};
appendFileSync(calls, ${JSON.stringify(name)} + " " + process.argv.slice(2).join(" ") + "\\n");
${body}
`,
    );
    await chmod(path, 0o755);
  };
  await standIn(
    "vite",
    `console.log("vite built the view");
     if (process.env.FAIL_VITE) process.exit(1);
     // A rebuild, after a change: it can fail, or be ended by a signal (^C reaches it with the rest).
     const builds = readFileSync(calls, "utf8").split("\\n").filter((l) => l.startsWith("vite ")).length;
     if (builds >= 2 && process.env.VITE_REBUILD === "fail") process.exit(1);
     if (builds >= 2 && process.env.VITE_REBUILD === "signal") process.kill(process.pid, "SIGINT");
     mkdirSync(${JSON.stringify(join(dir, "dist/view"))}, { recursive: true });
     writeFileSync(${JSON.stringify(join(dir, "dist/view/index.html"))}, "<html>");`,
  );
  await standIn(
    "tsc",
    `mkdirSync(${JSON.stringify(join(dir, "dist/server"))}, { recursive: true });
     writeFileSync(${JSON.stringify(join(dir, "dist/server/main.js"))}, "");`,
  );
  // pencere dev stays up until the view has been built a second time, which is the rebuild, and
  // then, for as long as the test says, a while longer: it takes the view down before it ends.
  if (options.pencere !== false)
    await standIn(
      "pencere",
      `const deadline = Date.now() + 15000;
       (function wait() {
         const built = readFileSync(calls, "utf8").split("\\n").filter((l) => l.startsWith("vite ")).length;
         if (built >= 2) return setTimeout(() => process.exit(7), Number(process.env.PENCERE_LINGER ?? 0));
         if (Date.now() > deadline) process.exit(99);
         setTimeout(wait, 20);
       })();`,
    );
  return dir;
}

async function until(condition: () => Promise<boolean>, what: string): Promise<void> {
  const deadline = Date.now() + 10_000;
  while (!(await condition())) {
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
}

const calls = async (dir: string): Promise<string[]> =>
  (await readFile(join(dir, "calls.log"), "utf8").catch(() => ""))
    .split("\n")
    .filter((line) => line !== "");

function runDev(dir: string, env: Record<string, string> = {}) {
  const child = spawn(process.execPath, [join(dir, "scripts/dev.mjs")], {
    cwd: dir,
    env: { ...process.env, ...env },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let stdout = "";
  let stderr = "";
  child.stdout.on("data", (chunk) => (stdout += chunk));
  child.stderr.on("data", (chunk) => (stderr += chunk));
  const ended = new Promise<number | null>((resolve) => child.on("close", resolve));
  return { ended, out: () => stdout, err: () => stderr };
}

describe("the generated project's dev script", () => {
  it("builds once, shows the view, and builds again when a source changes", async () => {
    const dir = await project();
    const dev = runDev(dir);
    await until(
      async () => (await calls(dir)).some((line) => line.startsWith("pencere ")),
      "pencere",
    );
    // The watchers are up before pencere starts: this change is the one to build again.
    await writeFile(join(dir, "src/view/App.tsx"), "// changed\n");
    expect(await dev.ended).toBe(7);

    const ran = await calls(dir);
    expect(ran.slice(0, 3)).toEqual([
      "vite build",
      "tsc -p tsconfig.server.json",
      `pencere dev --view dist/view --watch dist/server -- ${process.execPath} dist/server/main.js`,
    ]);
    expect(ran.slice(3)).toEqual(["vite build"]);
    // The first build is on the screen, and the build that came with the view up is in the file.
    expect(dev.out().match(/vite built the view/g)).toHaveLength(1);
    expect(await readFile(join(dir, "dev.log"), "utf8")).toContain("vite built the view");
    expect(dev.err()).toBe("");
  });

  it("says that pencere could not be run, once, and ends", async () => {
    const dir = await project({ pencere: false });
    const dev = runDev(dir);
    expect(await dev.ended).toBe(1);
    expect(dev.err()).toContain("dev: could not run pencere");
    // The script ends by what it says, not by an error of its own: the log it had open is
    // let go of once.
    expect(dev.err()).not.toMatch(/EBADF|at |Error:/);
  });

  describe("a build that comes with the view up", () => {
    /** Changes a source once pencere is up, and returns the script's end. */
    async function rebuilt(env: Record<string, string>) {
      const dir = await project();
      const dev = runDev(dir, env);
      await until(
        async () => (await calls(dir)).some((line) => line.startsWith("pencere ")),
        "pencere",
      );
      await writeFile(join(dir, "src/view/App.tsx"), "// changed\n");
      expect(await dev.ended).toBe(7);
      return dev;
    }

    it("is reported when it failed", async () => {
      const dev = await rebuilt({ VITE_REBUILD: "fail" });
      expect(dev.err()).toContain("dev: the last build of the view failed; dev.log says why");
    });

    it("is not reported when a signal ended it: that is the person's ^C, not a build that failed", async () => {
      // The build is ended by the signal while pencere takes the view down, which takes longer.
      const dev = await rebuilt({ VITE_REBUILD: "signal", PENCERE_LINGER: "400" });
      expect(dev.err()).toBe("");
    });
  });

  it("does not show a view whose first build failed", async () => {
    const dir = await project();
    const dev = runDev(dir, { FAIL_VITE: "1" });
    expect(await dev.ended).toBe(1);
    expect(dev.err()).toContain("dev: the view did not build");
    expect(await calls(dir)).toEqual(["vite build"]);
  });
});
