// A filter in a pipeline: it reads rows on stdin, asks the terminal for a view where the
// person chooses among them, and writes the chosen rows to stdout. stdin and stdout are
// the pipeline's, so the view is asked for on the terminal itself (`filter`). Nothing
// chosen (the view closed) writes nothing and ends with 1, so that `&&` stops there.
//
//   printf 'web\napi\ndb\n' | node filter.mjs > chosen.txt
import { readFileSync } from "node:fs";

import { ViewControl, bundleFromFiles, serve } from "@pencere/sdk";

const rows = readFileSync(0, "utf8")
  .split("\n")
  .filter((row) => row !== "");
const control = new ViewControl();
let chosen;

await serve({
  filter: true,
  control,
  views: [
    {
      kind: "web",
      bundle: bundleFromFiles([
        { path: "index.html", data: Buffer.from("<!doctype html><p>rows\n") },
      ]),
    },
  ],
  program: "filter",
  title: "Choose rows",
  calls: {
    rows: () => ({ rows }),
    // After the answer has gone back to the view, the program closes its view and goes
    // on to write.
    choose: (params) => {
      chosen = (params?.rows ?? []).map((index) => rows[index]).filter((row) => row !== undefined);
      setImmediate(() => void control.close());
      return { chosen: chosen.length };
    },
  },
});
if (chosen === undefined) process.exit(1);
process.stdout.write(`${chosen.join("\n")}\n`);
