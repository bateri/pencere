// Runs the tests that `cargo test` would, as `cargo test` does not run them: cargo builds
// them (`--no-run`), then this script runs each test binary itself, so that no cargo is alive
// while a test runs.
//
// It matters because some tests start cargo themselves (the stand-in host's end-to-end tests build
// the example program and the `pencere` command, and the templates' test checks a project it makes).
// A `cargo test` that is still running holds the locks of cargo's package cache and of the build
// directory while its tests run, so a cargo that a test starts waits for it for ever, and with it
// every other checkout's cargo, which shares the package cache. Run so, the nested cargo is the only
// one: nothing waits, nothing is left out.
//
// Usage: node tools/cargo-test.mjs <what `cargo test` takes before `--`> [-- <what the test
// binaries take>]. The binaries run one after another, in cargo's order, from their package's
// directory with the environment cargo gives them, and the first to fail stops the run with its
// status, as `cargo test` does. Their standard input is none.

import { spawnSync } from "node:child_process";
import { dirname } from "node:path";

const cargo = process.env.CARGO ?? "cargo";
const args = process.argv.slice(2);
const split = args.indexOf("--");
const cargoArgs = split < 0 ? args : args.slice(0, split);
const testArgs = split < 0 ? [] : args.slice(split + 1);

const built = spawnSync(
  cargo,
  ["test", ...cargoArgs, "--no-run", "--message-format=json-render-diagnostics"],
  {
    encoding: "utf8",
    maxBuffer: 1 << 30,
    stdio: ["ignore", "pipe", "inherit"],
  },
);
if (built.status !== 0) {
  process.exit(built.status ?? 1);
}

/** The test binaries cargo built, in the order it built them, each once. */
const tests = new Map();
for (const line of built.stdout.split("\n")) {
  if (!line.startsWith("{")) {
    continue;
  }
  const message = JSON.parse(line);
  if (
    message.reason === "compiler-artifact" &&
    message.profile?.test === true &&
    message.executable &&
    !tests.has(message.executable)
  ) {
    tests.set(message.executable, message);
  }
}
if (tests.size === 0) {
  console.error("cargo-test: cargo built no test binary for", cargoArgs);
  process.exit(1);
}

for (const [executable, artifact] of tests) {
  const directory = dirname(artifact.manifest_path);
  console.error(`\n     Running ${artifact.target.name} (${executable})`);
  const run = spawnSync(executable, testArgs, {
    cwd: directory,
    stdio: ["ignore", "inherit", "inherit"],
    env: {
      ...process.env,
      CARGO: process.env.CARGO ?? "cargo",
      CARGO_MANIFEST_DIR: directory,
    },
  });
  if (run.status !== 0) {
    console.error(
      `cargo-test: ${executable} ${run.signal ? `ended by ${run.signal}` : `failed with status ${run.status}`}`,
    );
    process.exit(run.status ?? 1);
  }
}
