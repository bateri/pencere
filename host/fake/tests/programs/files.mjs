// A program that moves files with its view through the SDK's `ViewControl` (§8.8 to §8.10), for
// the end-to-end tests against the stand-in host: `attach` attaches the bytes of `--attach` and
// answers the name the view draws them by, `read {handle}` reads the file the person gave a piece
// at a time and answers its length, `write {handle}` writes two pieces to the place the person
// chose, and `gap {handle}` writes with a gap, which the host refuses.
//
//   node files.mjs [--attach PATH]
import { readFileSync } from "node:fs";
import { FileError, ViewControl, ViewError, bundleFromFiles, serve } from "@pencere/sdk";

const attach = process.argv[2] === "--attach" ? readFileSync(process.argv[3]) : undefined;
const control = new ViewControl();

const failed = (error) => {
  if (error instanceof FileError) throw new ViewError(error.code, error.message);
  throw error;
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
    gap: async ({ handle }) => {
      await control.write(handle, 5, Buffer.from("late")).catch(failed);
      await control.write(handle, 9, Buffer.from("!"), true).catch(failed);
      return {};
    },
  },
});
console.log(`serve: ${JSON.stringify(result)}`);
