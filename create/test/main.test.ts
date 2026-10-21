/** The built command, run as `npm create` runs it: a process with no terminal on its input. */

import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vitest";

const MAIN = fileURLToPath(new URL("../dist/main.js", import.meta.url));

const dirs: string[] = [];
afterEach(async () => {
  for (const dir of dirs.splice(0)) await rm(dir, { recursive: true, force: true });
});

async function directory(): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "create-pencere-main-"));
  dirs.push(dir);
  return dir;
}

function command(cwd: string, ...args: string[]) {
  const done = spawnSync(process.execPath, [MAIN, ...args], { cwd, encoding: "utf8", input: "" });
  return { code: done.status, out: done.stdout, err: done.stderr };
}

describe("the create-pencere command", () => {
  it("writes the project, from the templates that ship next to it, and ends with 0", async () => {
    const cwd = await directory();
    const done = command(cwd, "my-tool", "--template", "vue");
    expect(done).toMatchObject({ code: 0, err: "" });
    expect(done.out).toContain("Created my-tool (Vue)");
    const manifest = JSON.parse(await readFile(join(cwd, "my-tool/package.json"), "utf8"));
    expect(manifest.name).toBe("my-tool");
  });

  it("does not ask when there is no terminal to ask, and says what it needs", async () => {
    const cwd = await directory();
    const done = command(cwd);
    expect(done.code).toBe(2);
    expect(done.err).toContain("name the directory");
    expect(await readdir(cwd)).toEqual([]);
  });

  it("ends with 1 and leaves a directory with something in it alone", async () => {
    const cwd = await directory();
    expect(command(cwd, "again").code).toBe(0);
    const second = command(cwd, "again");
    expect(second.code).toBe(1);
    expect(second.err).toContain("is not empty");
  });
});
