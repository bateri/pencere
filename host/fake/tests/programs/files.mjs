// A program that moves files with its view through the SDK's `ViewControl` (§8.8 to §8.11), for
// the end-to-end tests against the stand-in host: `attach` attaches the bytes of `--attach` and
// answers the name the view draws them by, `read {handle}` reads the file the person gave a piece
// at a time and answers its length, `write {handle}` writes two pieces to the place the person
// chose, and `gap {handle}` writes with a gap, which the host refuses. `stagedRead {handle, to}` and
// `stagedWrite {handle, from}` move a whole file the way a program of the library does: through a
// pipe in the host's folder when it has one that the program proves, and in `read`s and `write`s
// when it has not or the pipe stays silent; they answer `{ bytes, via }` with `via` `pipe` or
// `messages`.
//
//   node files.mjs [--attach PATH]
import { closeSync, openSync, readFileSync, readSync, writeSync } from "node:fs";
import { setTimeout as sleep } from "node:timers/promises";
import { FileError, ViewControl, ViewError, bundleFromFiles, serve } from "@pencere/sdk";

const attach = process.argv[2] === "--attach" ? readFileSync(process.argv[3]) : undefined;
const control = new ViewControl();

const failed = (error) => {
  if (error instanceof FileError) throw new ViewError(error.code, error.message);
  throw error;
};

/** How long a pipe may stay silent before the program takes the transfer back (§8.11). */
const SILENT = 3000;

/** Reads `handle` into the file at `to`, through a pipe if it can. */
const stagedRead = async (handle, to) => {
  const out = openSync(to, "w");
  let length = 0;
  let via = "messages";
  try {
    const folder = control.stageFolder();
    if (folder !== undefined) {
      let transfer;
      try {
        transfer = await control.stage(handle, 0);
        let end;
        let failure;
        transfer.done.then(
          (sent) => (end = sent),
          (error) => (failure = error),
        );
        const fd = folder.openPipe(await transfer.pipe, "read");
        const buffer = Buffer.alloc(1 << 16);
        const opened = Date.now();
        try {
          while (end === undefined || length < end) {
            if (failure !== undefined) throw failure;
            let read = 0;
            try {
              read = readSync(fd, buffer, 0, buffer.length, null);
            } catch (error) {
              if (error.code !== "EAGAIN") throw error;
            }
            if (read > 0) {
              writeSync(out, buffer, 0, read);
              length += read;
            } else if (length === 0 && end === undefined && Date.now() - opened > SILENT) {
              transfer.unstage();
              throw new Error("the pipe is silent");
            } else {
              await sleep(2);
            }
          }
          via = "pipe";
        } finally {
          closeSync(fd);
        }
      } catch {
        // The messages carry the rest, from where the part ends. A transfer that is still out is
        // taken back first, so that the host's pipe does not wait for a reader that is gone.
        transfer?.unstage();
      }
    }
    if (via !== "pipe") {
      for (;;) {
        const piece = await control.read(handle, length, 1 << 20).catch(failed);
        writeSync(out, piece);
        length += piece.length;
        if (piece.length < 1 << 20) break;
      }
    }
  } finally {
    closeSync(out);
  }
  return { bytes: length, via };
};

/** Writes the file at `from` to the place of `handle`, through a pipe if it can. */
const stagedWrite = async (handle, from) => {
  const data = readFileSync(from);
  const folder = control.stageFolder();
  if (folder !== undefined) {
    let transfer;
    try {
      transfer = await control.fetch(handle, data.length, 0);
      const fd = await (async () => {
        const name = await transfer.pipe;
        const began = Date.now();
        for (;;) {
          try {
            return folder.openPipe(name, "write");
          } catch (error) {
            if (error.errno !== "ENXIO" || Date.now() - began > SILENT) throw error;
            await sleep(5);
          }
        }
      })();
      try {
        let at = 0;
        let moved = Date.now();
        while (at < data.length) {
          try {
            at += writeSync(fd, data, at, Math.min(1 << 16, data.length - at));
            moved = Date.now();
          } catch (error) {
            if (error.code !== "EAGAIN") throw error;
            if (Date.now() - moved > SILENT) {
              transfer.unstage();
              throw new Error("the pipe is not read");
            }
            await sleep(1);
          }
        }
      } finally {
        closeSync(fd);
      }
      const saved = await transfer.done;
      return { bytes: saved, via: "pipe" };
    } catch {
      // The place is as it was before the `fetch` once it is taken back, if it was still out: the
      // file goes in `write`s from its start.
      transfer?.unstage();
    }
  }
  let at = 0;
  do {
    const end = Math.min(data.length, at + (1 << 20));
    await control.write(handle, at, data.subarray(at, end), end === data.length).catch(failed);
    at = end;
  } while (at < data.length);
  return { bytes: data.length, via: "messages" };
};

const result = await serve({
  views: [
    {
      kind: "lua",
      versions: ["2.1"],
      bundle: bundleFromFiles(
        [{ path: "files.lua", data: Buffer.from("return { render = function() end }") }],
        { entry: "files.lua" },
      ),
    },
  ],
  program: "files",
  control,
  calls: {
    attach: async () => ({ name: await control.attach(attach).catch(failed) }),
    read: async ({ handle }) => {
      let offset = 0;
      for (;;) {
        const piece = await control.read(handle, offset, 4).catch(failed);
        offset += piece.length;
        if (piece.length < 4) return { bytes: offset };
      }
    },
    write: async ({ handle }) => {
      await control.write(handle, 0, Buffer.from("hello ")).catch(failed);
      await control.write(handle, 6, Buffer.from("world"), true).catch(failed);
      return { bytes: 11 };
    },
    stagedRead: ({ handle, to }) => stagedRead(handle, to),
    stagedWrite: ({ handle, from }) => stagedWrite(handle, from),
    gap: async ({ handle }) => {
      await control.write(handle, 5, Buffer.from("late")).catch(failed);
      await control.write(handle, 9, Buffer.from("!"), true).catch(failed);
      return {};
    },
  },
});
console.log(`serve: ${JSON.stringify(result)}`);
