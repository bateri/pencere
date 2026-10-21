#!/usr/bin/env node
/** The `pencere` command. */

import { run, systemEnvironment } from "./run.js";

// A terminal that is gone (ssh dropped, a window closed) fails what is written to it and read
// from it, as an `error` of the stream. Nothing listens for it by default, and an `error` nobody
// listens for ends the process in the middle of letting go of the server, which is then left running.
for (const stream of [process.stdin, process.stdout, process.stderr]) stream.on("error", () => {});

const finish = await run(process.argv.slice(2), systemEnvironment());
if ("signal" in finish) {
  // What the person sent (or the terminal, going away) ended the view and the server, and now
  // ends this process as it would have ended any other: the shell reads the signal in the
  // status (130, 143, 129).
  process.kill(process.pid, finish.signal);
  // The signal is delivered at once; this is for a platform where it is not.
  await new Promise((resolve) => setTimeout(resolve, 1000));
  process.exit({ SIGINT: 130, SIGTERM: 143, SIGHUP: 129 }[finish.signal]);
}
// A server's pipes or a stream the terminal left open must not keep the process.
process.exit(finish.code);
