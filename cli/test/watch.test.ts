import { mkdir, mkdtemp, rename, rm, symlink, utimes, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";

import { WatchError, fingerprintOf, isNoise, watchRoots } from "../src/watch.js";
import type { Role } from "../src/watch.js";

const dirs: string[] = [];
afterEach(async () => {
  for (const dir of dirs.splice(0)) await rm(dir, { recursive: true, force: true });
});

async function directory(): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "pencere-watch-"));
  dirs.push(dir);
  return dir;
}

describe("what is nothing to edit", () => {
  it("is a name that begins with a dot, the packages a project installs and compiled bytecode", () => {
    for (const noise of [
      ".git/HEAD",
      ".DS_Store",
      "a/.hidden/b",
      "node_modules/x/index.js",
      "src/__pycache__/m.pyc",
      "__pycache__",
    ])
      expect(isNoise(noise), noise).toBe(true);
    for (const edited of ["index.html", "src/server.py", "a.b/c", "not_node_modules/x"])
      expect(isNoise(edited), edited).toBe(false);
  });
});

describe("what files hold", () => {
  it("changes with a file's size or modification time, and with a file that goes away", async () => {
    const dir = await directory();
    const file = join(dir, "server.py");
    await writeFile(file, "print(1)");
    const first = await fingerprintOf([file, join(dir, "lib")]);
    expect(await fingerprintOf([file, join(dir, "lib")])).toBe(first);
    expect(first).toContain("lib missing");

    await writeFile(file, "print(12)");
    const longer = await fingerprintOf([file, join(dir, "lib")]);
    expect(longer).not.toBe(first);

    // The same length, written later.
    await utimes(file, new Date(Date.now() + 5000), new Date(Date.now() + 5000));
    const later = await fingerprintOf([file, join(dir, "lib")]);
    expect(later).not.toBe(longer);

    await mkdir(join(dir, "lib"));
    await writeFile(join(dir, "lib", "a.py"), "x");
    const withLib = await fingerprintOf([file, join(dir, "lib")]);
    expect(withLib).not.toBe(later);
    await rm(file);
    expect(await fingerprintOf([file, join(dir, "lib")])).toContain("server.py missing");
  });

  it("leaves out what is nothing to edit, and a link to a directory", async () => {
    const dir = await directory();
    await writeFile(join(dir, "a.py"), "x");
    const before = await fingerprintOf([dir]);
    await mkdir(join(dir, "__pycache__"));
    await writeFile(join(dir, "__pycache__", "a.pyc"), "bytecode");
    await writeFile(join(dir, ".DS_Store"), "x");
    await symlink(dir, join(dir, "loop"));
    expect(await fingerprintOf([dir])).toBe(before);
  });
});

/**
 * Writes until the watch has heard, which is as soon as it is listening: a
 * watch says nothing of when it began, and a write before then is not heard.
 */
async function heardAfter(write: (attempt: number) => Promise<void>, heard: Promise<unknown>) {
  let done = false;
  void heard.then(() => (done = true));
  for (let attempt = 0; attempt < 40 && !done; attempt += 1) {
    await write(attempt);
    await Promise.race([heard, new Promise((resolve) => setTimeout(resolve, 100))]);
  }
  expect(done, "the watch heard nothing").toBe(true);
}

describe("watching", () => {
  it("hears a change anywhere under a directory, and says whose it was", async () => {
    const dir = await directory();
    await mkdir(join(dir, "deep"));
    const roles: Role[] = [];
    let hear: () => void = () => {};
    const heard = new Promise<void>((resolve) => (hear = resolve));
    const watching = watchRoots(
      [{ path: dir, role: "view" }],
      (role) => {
        roles.push(role);
        hear();
      },
      (message) => {
        throw new Error(message);
      },
    );
    try {
      await heardAfter((attempt) => writeFile(join(dir, "deep", "app.js"), `${attempt}`), heard);
      expect(roles[0]).toBe("view");
    } finally {
      watching.close();
    }
  });

  it("hears a file replaced the way an editor saves it, and its own change", async () => {
    const dir = await directory();
    const file = join(dir, "server.py");
    await writeFile(file, "print(1)");
    let hear: () => void = () => {};
    let heard = new Promise<void>((resolve) => (hear = resolve));
    const roles: Role[] = [];
    const watching = watchRoots(
      [{ path: file, role: "server" }],
      (role) => {
        roles.push(role);
        hear();
      },
      (message) => {
        throw new Error(message);
      },
    );
    try {
      // Written beside it and renamed over it: the file watched is a new file.
      await heardAfter(async (attempt) => {
        await writeFile(join(dir, "server.py.tmp"), `print(${attempt})`);
        await rename(join(dir, "server.py.tmp"), file);
      }, heard);
      expect(roles[0]).toBe("server");

      heard = new Promise<void>((resolve) => (hear = resolve));
      await heardAfter((attempt) => writeFile(file, `print(${attempt + 100})`), heard);
    } finally {
      watching.close();
    }
  });

  it("refuses a path that is not there, and says so", async () => {
    const dir = await directory();
    const missing = join(dir, "nothing");
    let thrown: unknown;
    try {
      // What was started before the path that failed is ended, so that it leaves nothing behind.
      watchRoots(
        [
          { path: dir, role: "view" },
          { path: missing, role: "server" },
        ],
        () => {},
        () => {},
      );
    } catch (error) {
      thrown = error;
    }
    expect(thrown).toBeInstanceOf(WatchError);
    expect((thrown as WatchError).missing).toBe(true);
    expect((thrown as WatchError).message).toBe(
      `cannot watch ${missing}: no such file or directory`,
    );
  });
});
