import { execFileSync } from "node:child_process";
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  renameSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { closeSync, constants, openSync, readSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { FileError, Files } from "../src/files.js";
import type { SendFile } from "../src/files.js";
import type { Message } from "../src/frame.js";
import { PipeError, proveStage } from "../src/stage.js";

const TOKEN = "0b5d7c1e9a3f4682d0e1c7a95b3f8d24";

describe("the folder a host offers", () => {
  let root: string;
  beforeAll(() => {
    root = mkdtempSync(join(tmpdir(), "pencere-stage-"));
  });
  afterAll(() => rmSync(root, { recursive: true, force: true }));

  /** A folder as the host makes it: a directory of mode 0700 with a `marker`. */
  const folder = (name: string, marker: string | null = TOKEN): string => {
    const dir = join(root, name);
    mkdirSync(dir, { recursive: true });
    chmodSync(dir, 0o700);
    if (marker !== null) writeFileSync(join(dir, "marker"), marker);
    return dir;
  };

  it("is the host's only when every part of the proof holds", () => {
    const good = folder("good");
    expect(proveStage({ dir: good, token: TOKEN })).toBeDefined();
    // Not an absolute path; not there; a file.
    expect(proveStage({ dir: "good", token: TOKEN })).toBeUndefined();
    expect(proveStage({ dir: join(root, "missing"), token: TOKEN })).toBeUndefined();
    expect(proveStage({ dir: join(good, "marker"), token: TOKEN })).toBeUndefined();
    // Another token in the marker, one that holds more, one that holds less, none at all.
    expect(
      proveStage({ dir: folder("other", "f".repeat(32)), token: TOKEN }),
      "another token",
    ).toBeUndefined();
    expect(proveStage({ dir: folder("more", `${TOKEN}\n`), token: TOKEN })).toBeUndefined();
    expect(proveStage({ dir: folder("less", TOKEN.slice(0, 31)), token: TOKEN })).toBeUndefined();
    expect(proveStage({ dir: folder("bare", null), token: TOKEN })).toBeUndefined();
    // A marker that is a link is not followed; a marker that is a directory is no file.
    const linked = folder("linked", null);
    symlinkSync(join(good, "marker"), join(linked, "marker"));
    expect(proveStage({ dir: linked, token: TOKEN }), "a link").toBeUndefined();
    const hollow = folder("hollow", null);
    mkdirSync(join(hollow, "marker"));
    expect(proveStage({ dir: hollow, token: TOKEN })).toBeUndefined();
    // The group or others can use the folder.
    for (const mode of [0o710, 0o701, 0o750, 0o755, 0o770, 0o707]) {
      chmodSync(good, mode);
      expect(proveStage({ dir: good, token: TOKEN }), mode.toString(8)).toBeUndefined();
    }
    chmodSync(good, 0o700);
    // The folder is a link to a good one: the path is not the folder it was made as.
    const link = join(root, "link");
    symlinkSync(good, link);
    expect(proveStage({ dir: link, token: TOKEN }), "a linked folder").toBeUndefined();
  });

  it("is not used for a pipe once its path names another directory", () => {
    const dir = folder("swapped");
    const stage = proveStage({ dir, token: TOKEN });
    if (stage === undefined) throw new Error("a proved folder");
    const name = "4".repeat(32);
    execFileSync("mkfifo", [join(dir, name)]);
    closeSync(stage.openPipe(name, "read"));
    // The path now names a folder of the same shape, which is not the one proved.
    renameSync(dir, join(root, "swapped-away"));
    const other = folder("swapped");
    execFileSync("mkfifo", [join(other, name)]);
    expect(() => stage.openPipe(name, "read")).toThrowError(PipeError);
    // And a link in its place is not followed.
    rmSync(other, { recursive: true });
    symlinkSync(join(root, "swapped-away"), dir);
    expect(() => stage.openPipe(name, "read")).toThrowError(PipeError);
  });

  it("opens a pipe only when it is a FIFO the host could have named", () => {
    const dir = folder("pipes");
    const stage = proveStage({ dir, token: TOKEN });
    if (stage === undefined) throw new Error("a proved folder");
    const name = "1".repeat(32);
    execFileSync("mkfifo", [join(dir, name)]);
    // Nobody reads it yet: it cannot be opened for writing, and the caller is told why.
    expect(() => stage.openPipe(name, "write")).toThrowError(PipeError);
    try {
      stage.openPipe(name, "write");
    } catch (error) {
      expect((error as PipeError).errno).toBe("ENXIO");
    }
    const reading = stage.openPipe(name, "read");
    const writing = stage.openPipe(name, "write");
    // What is written is read, and a read with nothing to give does not block.
    const buffer = new Uint8Array(8);
    expect(() => readSync(reading, buffer, 0, 8, null)).toThrowError(/EAGAIN/);
    closeSync(writing);
    closeSync(reading);
    // Names that are not 32 lowercase hex digits are not joined to the folder.
    for (const bad of ["", "..", "../good", "A".repeat(32), "1".repeat(31), "marker"]) {
      expect(() => stage.openPipe(bad, "read"), bad).toThrowError(PipeError);
    }
    // A regular file with a good name; a link, even to a pipe.
    writeFileSync(join(dir, "2".repeat(32)), "x");
    expect(() => stage.openPipe("2".repeat(32), "read")).toThrowError(PipeError);
    symlinkSync(join(dir, name), join(dir, "3".repeat(32)));
    expect(() => stage.openPipe("3".repeat(32), "read")).toThrowError(PipeError);
    // The descriptor is the pipe's: a plain open of the same name sees the same FIFO.
    const same = openSync(join(dir, name), constants.O_RDONLY | constants.O_NONBLOCK);
    closeSync(same);
  });
});

describe("the transfers that are out", () => {
  const frame = (re: number, payload: string): Message =>
    ({ kind: "staged", id: 1, control: { re: String(re) }, payload }) as unknown as Message;

  /** Messages the program sent, and the ids they went out with. */
  const sender = (): { sent: string[]; send: SendFile } => {
    const sent: string[] = [];
    return {
      sent,
      send: (kind, payload) => {
        sent.push(`${kind} ${payload}`);
        return 10 + sent.length;
      },
    };
  };

  it("are told the pipe and then the end, and the last answer ends them", async () => {
    const files = new Files();
    const { sent, send } = sender();
    const handle = "0".repeat(32);
    const transfer = files.stage(handle, 5, send);
    expect(sent).toEqual([`stage {"handle":"${handle}","offset":5}`]);
    expect(transfer.id).toBe(11);
    files.onTransfer(frame(11, `{"pipe":"${TOKEN}"}`), false);
    await expect(transfer.pipe).resolves.toBe(TOKEN);
    // A `fetched` is not the answer to a `stage`, and an id that is not out is nobody's.
    files.onTransfer(frame(11, '{"sent":99}'), true);
    files.onTransfer(frame(77, '{"sent":99}'), false);
    files.onTransfer(frame(11, '{"sent":11}'), false);
    await expect(transfer.done).resolves.toBe(11);
    // It is over: another answer is nothing.
    files.onTransfer(frame(11, '{"sent":12}'), false);
  });

  it("fail with the host's code, and a route of a fetch says how long a file the host holds", async () => {
    const files = new Files();
    const { send } = sender();
    const staged = files.stage("0".repeat(32), 0, send);
    files.onTransfer(frame(11, '{"code":"busy"}'), false);
    await expect(staged.pipe).rejects.toMatchObject({ code: "busy" });
    await expect(staged.done).rejects.toBeInstanceOf(FileError);

    const fetched = files.fetch("0".repeat(32), 11, 0, send);
    files.onTransfer(frame(12, `{"pipe":"${TOKEN}"}`), true);
    await expect(fetched.pipe).resolves.toBe(TOKEN);
    files.onTransfer(frame(12, '{"code":"route","at":4}'), true);
    await expect(fetched.done).rejects.toMatchObject({ code: "route", at: 4 });
  });

  it("are taken back by an unstage, once, and the view ending ends the ones that are out", async () => {
    const files = new Files();
    const { sent, send } = sender();
    const handle = "0".repeat(32);
    const first = files.fetch(handle, 11, 0, send);
    first.unstage();
    first.unstage();
    expect(sent).toEqual([
      `fetch {"handle":"${handle}","size":11,"offset":0}`,
      'unstage {"id":11}',
    ]);
    await expect(first.done).rejects.toMatchObject({ code: "cancelled" });
    // The host's own `cancelled` comes late and is nobody's.
    files.onTransfer(frame(11, '{"code":"cancelled"}'), true);

    const second = files.stage(handle, 0, send);
    files.offer({ dir: "/a", token: TOKEN });
    files.forget();
    await expect(second.pipe).rejects.toMatchObject({ code: "ended" });
    await expect(second.done).rejects.toMatchObject({ code: "ended" });
    // The folder is the host's, not the view's.
    expect(files.stageOffer).toEqual({ dir: "/a", token: TOKEN });
  });
});
