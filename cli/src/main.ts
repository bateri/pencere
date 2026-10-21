#!/usr/bin/env node
/** The `pencere` command. */

import { run, systemEnvironment } from "./run.js";

const finish = await run(process.argv.slice(2), systemEnvironment());
if ("signal" in finish) {
  // What the person sent ended the view and the server, and now ends this process as
  // it would have ended any other: the shell reads the signal in the status (130, 143).
  process.kill(process.pid, finish.signal);
  // The signal is delivered at once; this is for a platform where it is not.
  await new Promise((resolve) => setTimeout(resolve, 1000));
  process.exit(finish.signal === "SIGINT" ? 130 : 143);
}
// A server's pipes or a stream the terminal left open must not keep the process.
process.exit(finish.code);
