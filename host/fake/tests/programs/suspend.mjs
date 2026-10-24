// A program that is stopped by ⌃Z while its view is up, and continued. The call `suspend` stands in
// for the tty's ⌃Z: it sends the program SIGTSTP, as the tty would, and starts a helper that waits
// until the program is stopped, writes the modes of the tty to the file named on the command line
// (the shell's view of it), and continues the program. The second time the view calls `suspend` (it is
// the same page, opened again) the program sends itself SIGINT, which ends it.
//
//   node suspend.mjs /tmp/modes
import { spawn } from "node:child_process";

import { bundleFromFiles, serve } from "@pencere/sdk";

let suspended = false;

/** Starts the helper with the tty as its stdin, so that it can read its modes. */
function startHelper(script) {
  const args = ["-c", script, "sh", process.argv[2] ?? "/dev/null"];
  // Deno's `node:child_process` asks for the environment, which the program has no use for.
  if (typeof Deno !== "undefined") {
    new Deno.Command("sh", { args, stdin: "inherit", stdout: "null", stderr: "null" })
      .spawn()
      .unref();
  } else {
    spawn("sh", args, { stdio: ["inherit", "ignore", "ignore"] }).unref();
  }
}

await serve({
  views: [
    {
      kind: "web",
      bundle: bundleFromFiles([
        { path: "index.html", data: new TextEncoder().encode("<!doctype html><p>suspend\n") },
      ]),
    },
  ],
  program: "suspend",
  title: "Suspend",
  calls: {
    suspend: () => {
      if (suspended) {
        // The view is shown again: this is the end of the test, as the person's ⌃C would be.
        setImmediate(() => process.kill(process.pid, "SIGINT"));
        return { again: true };
      }
      suspended = true;
      const script = `
        pid=${process.pid}
        n=0
        while [ $n -lt 100 ]; do
          case "$(ps -o stat= -p $pid)" in *T*) break;; esac
          n=$((n + 1)); sleep 0.05
        done
        stty -a >"$1" 2>&1
        kill -CONT $pid
      `;
      startHelper(script);
      setImmediate(() => process.kill(process.pid, "SIGTSTP"));
      return {};
    },
  },
});
