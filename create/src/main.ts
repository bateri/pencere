#!/usr/bin/env node
/** The `create-pencere` command, which `npm create pencere` runs. */

import { create, systemEnvironment } from "./create.js";

// Not `process.exit`: what was written to a pipe is flushed before the process ends.
process.exitCode = await create(process.argv.slice(2), systemEnvironment());
