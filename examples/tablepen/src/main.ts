#!/usr/bin/env node
/**
 * tablepen: a table in a pipeline.
 *
 *   tablepen [--select] [FILE]
 *
 * Reads CSV, the first line its header, from FILE or from stdin. In a terminal that
 * hosts pencere views the table opens in the pane, where it can be filtered and
 * sorted. With `--select` the person chooses rows, and they are written to stdout as
 * CSV, header first, for what comes next in the pipeline:
 *
 *   rows=$(tablepen --select < hosts.csv) && printf '%s\n' "$rows" | mail -s hosts ops
 *
 * stdin and stdout are the pipeline's: the view is asked for on the terminal itself
 * (`filter`), so this program is never an MCP client's stdio server. Nothing chosen
 * (the view closed, Esc) writes nothing and ends with 1, so that `&&` stops there.
 */

import { readFileSync } from "node:fs";
import { ViewControl, bundleFromDirectory, serve } from "@pencere/sdk";

import { format, parse } from "./csv.js";
import { VIEW_DIRECTORY, createServer, tableOf, text } from "./tool.js";

const USAGE = "usage: tablepen [--select] [FILE]\n  reads CSV from FILE or from stdin\n";

function quit(code: number, message: string): never {
  process.stderr.write(message.endsWith("\n") ? message : `tablepen: ${message}\n`);
  process.exit(code);
}

async function readAll(stream: NodeJS.ReadableStream): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of stream) {
    chunks.push(typeof chunk === "string" ? Buffer.from(chunk) : chunk);
  }
  return Buffer.concat(chunks).toString("utf8");
}

const args = process.argv.slice(2);
const select = args.includes("--select");
const files = args.filter((arg) => arg !== "--select");
if (args.includes("-h") || args.includes("--help")) {
  process.stdout.write(USAGE);
  process.exit(0);
}
if (files.length > 1 || files.some((arg) => arg.startsWith("-") && arg !== "-")) quit(64, USAGE);

const file = files[0];
// A person who typed `tablepen` alone would wait on a read nobody feeds.
if ((file === undefined || file === "-") && process.stdin.isTTY) quit(64, USAGE);

let input: string;
try {
  input =
    file === undefined || file === "-" ? await readAll(process.stdin) : readFileSync(file, "utf8");
} catch (error) {
  quit(66, `cannot read ${file ?? "stdin"}: ${(error as Error).message}`);
}
const table = tableOf(parse(input));
if (table === undefined) quit(65, "there is no table: the input is empty");

const control = new ViewControl();
const abort = new AbortController();
let chosen: number[] | undefined;
const server = createServer(table, {
  select,
  chosen: (rows) => {
    chosen = rows;
    // After the answer has gone back to the view: the program closes its view and
    // goes on to write. A terminal that cannot close it for the program has the
    // wait ended instead.
    setImmediate(() => {
      void control.close().then((closed) => {
        if (closed.kind === "unsupported") abort.abort();
      });
    });
  },
});

let noView = false;
await serve(server, {
  tool: "show_table",
  bundle: await bundleFromDirectory(VIEW_DIRECTORY),
  filter: true,
  control,
  signal: abort.signal,
  fallback: () => {
    noView = true;
    if (!select) process.stdout.write(`${text(table)}\n`);
  },
});

if (chosen !== undefined) {
  const rows = chosen.map((at) => table.rows[at] ?? []);
  process.stdout.write(format([table.columns, ...rows]));
} else if (select && noView) {
  quit(2, "choosing rows needs a terminal that shows pencere views");
} else if (select) {
  quit(1, "nothing was chosen");
}
