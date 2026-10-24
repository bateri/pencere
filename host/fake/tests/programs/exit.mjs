// A program that ends with `process.exit` while its view is up: the exit handler of the SDK says
// `bye`, reads the host's last word from the tty and throws away what is left, with no event loop
// to wait with. `quit` is the call that ends it; it answers first, so the view has its answer.
//
// A second `exit` listener, which runs after the SDK's, looks at what the tty has queued for
// whatever reads it next (the shell) after the host has had time to answer, and prints it as
// `LEFT:"…"`: nothing, when the SDK read the host's last word itself.
//
//   node exit.mjs
import { spawnSync } from "node:child_process";
import { readSync, writeSync } from "node:fs";

import { bundleFromFiles, serve } from "@pencere/sdk";

void serve({
  views: [
    {
      kind: "web",
      bundle: bundleFromFiles([
        { path: "index.html", data: new TextEncoder().encode("<!doctype html><p>exit\n") },
      ]),
    },
  ],
  program: "exit",
  title: "Exit",
  calls: {
    quit: () => {
      setImmediate(() => process.exit(0));
      return {};
    },
  },
});

/** `stty` with the tty as its stdin. Deno's `node:child_process` does not return from the call that sets the modes. */
function stty(...args) {
  if (typeof Deno !== "undefined") {
    new Deno.Command("stty", {
      args,
      stdin: "inherit",
      stdout: "null",
      stderr: "null",
    }).outputSync();
  } else {
    spawnSync("stty", args, { stdio: ["inherit", "ignore", "ignore"] });
  }
}

process.on("exit", () => {
  // The host answers within a few milliseconds; the shell would find the answer if it were there.
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 200);
  stty("-icanon", "-echo", "min", "0", "time", "0");
  const into = new Uint8Array(4096);
  let left = "";
  for (;;) {
    let got = 0;
    try {
      got = readSync(0, into, 0, into.length, null);
    } catch {
      // Nothing to read on a descriptor that does not wait.
    }
    if (got <= 0) break;
    left += String.fromCharCode(...into.subarray(0, got));
  }
  writeSync(1, `LEFT:${JSON.stringify(left)}\n`);
});
