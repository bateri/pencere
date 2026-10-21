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
async function project(): Promise<string> {
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
     mkdirSync(${JSON.stringify(join(dir, "dist/view"))}, { recursive: true });
     writeFileSync(${JSON.stringify(join(dir, "dist/view/index.html"))}, "<html>");`,
  );
  await standIn(
    "tsc",
    `mkdirSync(${JSON.stringify(join(dir, "dist/server"))}, { recursive: true });
     writeFileSync(${JSON.stringify(join(dir, "dist/server/main.js"))}, "");`,
  );
  // pencere dev stays up until the view has been built a second time, which is the rebuild.
  await standIn(
    "pencere",
    `const deadline = Date.now() + 15000;
     (function wait() {
       const built = readFileSync(calls, "utf8").split("\\n").filter((l) => l.startsWith("vite ")).length;
       if (built >= 2) process.exit(7);
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

  it("does not show a view whose first build failed", async () => {
    const dir = await project();
    const dev = runDev(dir, { FAIL_VITE: "1" });
    expect(await dev.ended).toBe(1);
    expect(dev.err()).toContain("dev: the view did not build");
    expect(await calls(dir)).toEqual(["vite build"]);
  });
});
