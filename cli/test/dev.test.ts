import { Bundle, BundleError, bundleFromDirectory, bundleFromFiles } from "@pencere/sdk";
import type { ReopenOptions, ReopenResult } from "@pencere/sdk";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Reloader } from "../src/dev.js";
import type { ReloadTiming } from "../src/dev.js";

/** Quiet for a few milliseconds is a burst's end; a view that is not up is asked about again at once. */
const TIMING: ReloadTiming = { settle: 10, retry: 0, tries: 3 };

const dirs: string[] = [];
afterEach(async () => {
  for (const dir of dirs.splice(0)) await rm(dir, { recursive: true, force: true });
});

/** A view directory of the test's own. */
async function directory(files: Record<string, string> = { "index.html": "one" }): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "pencere-dev-"));
  dirs.push(dir);
  for (const [name, text] of Object.entries(files)) await writeFile(join(dir, name), text);
  return dir;
}

/** The program's hand on the view, answering as it is told and writing down what it was asked. */
function control(...answers: (ReopenResult | Promise<ReopenResult>)[]) {
  const asked: ReopenOptions[] = [];
  return {
    asked,
    async reopen(next: ReopenOptions): Promise<ReopenResult> {
      asked.push(next);
      return (await answers.shift()) ?? { kind: "reopened" };
    },
  };
}

/** The view the directory holds when `index.html` says `text`: what a view was first opened with. */
async function open(dir: string, text = "one") {
  await writeFile(join(dir, "index.html"), text);
  return bundleFromDirectory(dir);
}

describe("the view's files", () => {
  it("are shown again once, for a burst of changes", async () => {
    const dir = await directory();
    const hand = control();
    const reloader = new Reloader({
      control: hand,
      view: dir,
      bundle: await open(dir),
      timing: TIMING,
    });
    await writeFile(join(dir, "index.html"), "two");
    await writeFile(join(dir, "app.js"), "export {}");
    for (let i = 0; i < 5; i += 1) reloader.viewChanged();
    await reloader.settled();
    expect(hand.asked).toHaveLength(1);
    const bundle = hand.asked[0]?.bundle;
    expect(bundle).toBeInstanceOf(Bundle);
    expect(bundle?.manifest.files.map((file) => file.path)).toEqual(["app.js", "index.html"]);
    expect(reloader.reopened).toBe(1);
    expect(reloader.notes).toEqual([]);
  });

  it("are not shown again when they are what the view shows", async () => {
    const dir = await directory();
    const hand = control();
    const reloader = new Reloader({
      control: hand,
      view: dir,
      bundle: await open(dir),
      timing: TIMING,
    });
    // Saved as it was, and a file that is not part of a view.
    await writeFile(join(dir, "index.html"), "one");
    await writeFile(join(dir, ".DS_Store"), "x");
    reloader.viewChanged();
    reloader.viewChanged();
    await reloader.settled();
    expect(hand.asked).toEqual([]);

    // And what was shown is what is compared with afterwards, not what the first view was.
    await writeFile(join(dir, "index.html"), "two");
    reloader.viewChanged();
    await reloader.settled();
    expect(hand.asked).toHaveLength(1);
    reloader.viewChanged();
    await reloader.settled();
    expect(hand.asked).toHaveLength(1);
  });

  it("are looked at again, once, after a reopening that was under way", async () => {
    const dir = await directory();
    let finish: (result: ReopenResult) => void = () => {};
    let began: () => void = () => {};
    const started = new Promise<void>((resolve) => (began = resolve));
    const asked: ReopenOptions[] = [];
    const answer = new Promise<ReopenResult>((resolve) => (finish = resolve));
    const reloader = new Reloader({
      control: {
        reopen: (next) => {
          asked.push(next);
          began();
          return asked.length === 1 ? answer : Promise.resolve({ kind: "reopened" });
        },
      },
      view: dir,
      bundle: await open(dir),
      timing: TIMING,
    });
    await writeFile(join(dir, "index.html"), "two");
    reloader.viewChanged();
    await started;

    // A save while the host is still tearing the old view down.
    await writeFile(join(dir, "index.html"), "three");
    reloader.viewChanged();
    reloader.viewChanged();
    finish({ kind: "reopened" });
    await reloader.settled();

    expect(asked).toHaveLength(2);
    const [first, second] = asked.map((next) => next.bundle?.manifest.files[0]?.sha256);
    expect(first).not.toBe(second);
  });

  it("wait out their quiet again when it was spent while a reopening was under way", async () => {
    // The change that came during the reopening has had its quiet by the time the
    // reopening ends, and is acted on then, not dropped with the timer that ran out.
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    try {
      let text = "two";
      const load = async (): Promise<Bundle> =>
        bundleFromFiles([{ path: "index.html", data: Buffer.from(text) }]);
      let finish: (result: ReopenResult) => void = () => {};
      let began: () => void = () => {};
      const started = new Promise<void>((resolve) => (began = resolve));
      const asked: ReopenOptions[] = [];
      const answer = new Promise<ReopenResult>((resolve) => (finish = resolve));
      const reloader = new Reloader({
        control: {
          reopen: (next) => {
            asked.push(next);
            began();
            return asked.length === 1 ? answer : Promise.resolve({ kind: "reopened" });
          },
        },
        view: "/not/read",
        bundle: await load(),
        load,
        timing: TIMING,
      });
      text = "three";
      reloader.viewChanged();
      vi.advanceTimersByTime(TIMING.settle);
      await started;

      text = "four";
      reloader.viewChanged();
      vi.advanceTimersByTime(TIMING.settle);
      expect(asked).toHaveLength(1);
      finish({ kind: "reopened" });
      await vi.runAllTimersAsync();
      await reloader.settled();
      expect(asked).toHaveLength(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it("wait for the next change when a build has left something that is not a view", async () => {
    // A build empties its output before it writes: no `index.html`, and then it is there.
    const dir = await directory({});
    const hand = control();
    const reloader = new Reloader({
      control: hand,
      view: dir,
      bundle: await open(await directory(), "before"),
      timing: TIMING,
    });
    reloader.viewChanged();
    await reloader.settled();
    expect(hand.asked).toEqual([]);
    expect(reloader.notes.join("\n")).toContain("could not be shown");

    await writeFile(join(dir, "index.html"), "built");
    reloader.viewChanged();
    await reloader.settled();
    expect(hand.asked).toHaveLength(1);
    expect(reloader.notes).toEqual([]);
  });

  it("wait while a file goes away under the walk, and say what is wrong if it stays so", async () => {
    const dir = await directory();
    let reads = 0;
    const hand = control();
    const reloader = new Reloader({
      control: hand,
      view: dir,
      bundle: await open(dir),
      timing: TIMING,
      load: async (directory) => {
        reads += 1;
        if (reads === 1) {
          const error: NodeJS.ErrnoException = new Error("gone");
          error.code = "ENOENT";
          throw error;
        }
        if (reads === 2) throw new BundleError('the entry "index.html" is not one of the files');
        return bundleFromDirectory(directory);
      },
    });
    await writeFile(join(dir, "index.html"), "two");
    reloader.viewChanged();
    await reloader.settled();
    expect(reloader.notes.join("\n")).toContain("a file went away");
    reloader.viewChanged();
    await reloader.settled();
    expect(reloader.notes.join("\n")).toContain("is not one of the files");
    expect(hand.asked).toEqual([]);
    reloader.viewChanged();
    await reloader.settled();
    expect(hand.asked).toHaveLength(1);
    expect(reloader.notes).toEqual([]);
  });
});

describe("the server's files", () => {
  it("restart the view with no new files when what they hold moved", async () => {
    let print = "a";
    const hand = control();
    const reloader = new Reloader({
      control: hand,
      fingerprint: async () => print,
      timing: TIMING,
    });
    await reloader.start();

    // The late events of a write that was already shown are not another write.
    reloader.serverChanged();
    await reloader.settled();
    expect(hand.asked).toEqual([]);

    print = "b";
    reloader.serverChanged();
    reloader.serverChanged();
    await reloader.settled();
    expect(hand.asked).toEqual([{}]);

    reloader.serverChanged();
    await reloader.settled();
    expect(hand.asked).toEqual([{}]);
  });

  it("restart it for every change when nothing says what they hold", async () => {
    const hand = control();
    const reloader = new Reloader({ control: hand, timing: TIMING });
    reloader.serverChanged();
    await reloader.settled();
    reloader.serverChanged();
    await reloader.settled();
    expect(hand.asked).toEqual([{}, {}]);
  });

  it("restart the view with the new files when the view's own changed with them", async () => {
    const dir = await directory();
    let print = "a";
    const hand = control();
    const reloader = new Reloader({
      control: hand,
      view: dir,
      bundle: await open(dir),
      fingerprint: async () => print,
      timing: TIMING,
    });
    await reloader.start();
    print = "b";
    await writeFile(join(dir, "index.html"), "two");
    reloader.viewChanged();
    reloader.serverChanged();
    await reloader.settled();
    expect(hand.asked).toHaveLength(1);
    expect(hand.asked[0]?.bundle).toBeInstanceOf(Bundle);
  });

  it("are carried along by the next change when the build they came with was half done", async () => {
    const dir = await directory({});
    let print = "a";
    const hand = control();
    const reloader = new Reloader({
      control: hand,
      view: dir,
      bundle: await open(await directory(), "before"),
      fingerprint: async () => print,
      timing: TIMING,
    });
    await reloader.start();
    print = "b";
    reloader.serverChanged();
    reloader.viewChanged();
    await reloader.settled();
    expect(hand.asked).toEqual([]);

    await writeFile(join(dir, "index.html"), "built");
    reloader.viewChanged();
    await reloader.settled();
    expect(hand.asked).toHaveLength(1);
  });
});

describe("the files at the start", () => {
  it("are the view's files as they are when the watching has begun, and are what the reloader compares with", async () => {
    const dir = await directory();
    const hand = control();
    const reloader = new Reloader({
      control: hand,
      view: dir,
      bundle: await open(dir, "one"),
      timing: TIMING,
    });
    // A save between reading the files first and watching them: nothing tells of it.
    await writeFile(join(dir, "index.html"), "two");
    const shown = await reloader.start();
    expect(shown?.manifest.files[0]?.sha256).toBe(
      (await bundleFromDirectory(dir)).manifest.files[0]?.sha256,
    );
    // A change that comes now finds the files as they are shown: nothing to show again.
    reloader.viewChanged();
    await reloader.settled();
    expect(hand.asked).toEqual([]);
  });

  it("stay as they were given when the directory cannot be read just then", async () => {
    const dir = await directory();
    const first = await open(dir, "one");
    const reloader = new Reloader({
      control: control(),
      view: dir,
      bundle: first,
      timing: TIMING,
      load: async () => {
        throw new BundleError("a build is half done");
      },
    });
    expect(await reloader.start()).toBe(first);
  });
});

describe("a terminal", () => {
  it("that cannot reopen a view is told of once, and the files are let be", async () => {
    const hand = control({ kind: "unsupported" });
    const reloader = new Reloader({ control: hand, timing: TIMING });
    reloader.serverChanged();
    await reloader.settled();
    reloader.serverChanged();
    await reloader.settled();
    expect(hand.asked).toHaveLength(1);
    expect(reloader.notes).toHaveLength(1);
    expect(reloader.notes[0]).toContain("does not reopen a view when asked");
    expect(reloader.reopened).toBe(0);
  });

  it("that refuses the new view ends the reopening: serve says why", async () => {
    const hand = control({ kind: "no-view", reason: "refused", code: "policy" });
    const reloader = new Reloader({ control: hand, timing: TIMING });
    reloader.serverChanged();
    await reloader.settled();
    reloader.serverChanged();
    await reloader.settled();
    expect(hand.asked).toHaveLength(1);
    expect(reloader.notes).toEqual([]);
  });

  it("that ended a reopening because the program did (its server fell) leaves the reloader going", async () => {
    const hand = control({ kind: "no-view", reason: "aborted" });
    let print = "a";
    const reloader = new Reloader({
      control: hand,
      fingerprint: async () => print,
      timing: TIMING,
    });
    await reloader.start();
    print = "b";
    reloader.serverChanged();
    await reloader.settled();
    expect(hand.asked).toHaveLength(1);
    // What the files held then is what came of the server, and the same files are not tried again.
    reloader.serverChanged();
    await reloader.settled();
    expect(hand.asked).toHaveLength(1);
    // The next change is: the reloader is not stopped.
    print = "c";
    reloader.serverChanged();
    await reloader.settled();
    expect(hand.asked).toHaveLength(2);
    expect(reloader.reopened).toBe(1);
  });

  it("that has no view yet is asked again, a few times, and then left until the next change", async () => {
    const hand = control({ kind: "unavailable" }, { kind: "unavailable" }, { kind: "reopened" });
    const reloader = new Reloader({ control: hand, timing: TIMING });
    reloader.serverChanged();
    await reloader.settled();
    expect(hand.asked).toHaveLength(3);
    expect(reloader.reopened).toBe(1);
    expect(reloader.notes).toEqual([]);

    const never = control(
      { kind: "unavailable" },
      { kind: "unavailable" },
      { kind: "unavailable" },
      { kind: "unavailable" },
    );
    const waiting = new Reloader({ control: never, timing: TIMING });
    waiting.serverChanged();
    await waiting.settled();
    // The first time, and `tries` more.
    expect(never.asked).toHaveLength(4);
    expect(waiting.notes.join("\n")).toContain("was not shown");

    // The next change carries it, and is shown.
    waiting.serverChanged();
    await waiting.settled();
    expect(never.asked).toHaveLength(5);
    expect(waiting.reopened).toBe(1);
    expect(waiting.notes).toEqual([]);
  });
});

describe("a reloader that is stopped", () => {
  it("acts on nothing that was waiting, and on nothing after", async () => {
    const hand = control();
    const reloader = new Reloader({ control: hand, timing: { ...TIMING, settle: 10_000 } });
    reloader.serverChanged();
    reloader.stop();
    await reloader.settled();
    reloader.serverChanged();
    await reloader.settled();
    reloader.stop();
    expect(hand.asked).toEqual([]);
  });

  it("keeps what a watcher said for the person to read", () => {
    const reloader = new Reloader({ control: control(), timing: TIMING });
    reloader.warn("stopped watching /x: the system has no more watches to give");
    expect(reloader.notes).toEqual(["stopped watching /x: the system has no more watches to give"]);
  });
});
