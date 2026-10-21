// `npm run dev`: builds the view and the server, shows the view in this pane with
// `pencere dev`, and builds again when what they are made of changes. `pencere dev`
// sees the new files and shows the view again.
//
// The first build is made here, before anything watches, and later builds run one at
// a time after a change. A watch mode of Vite's or of the compiler's would build once
// more at the start, and `pencere dev` would open the view while that build was still
// emptying its directory.

import { spawn, spawnSync } from "node:child_process";
import { closeSync, openSync, watch, writeSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const bin = (name) => join(root, "node_modules", ".bin", name);

/** What is built, and what it is built from. */
const BUILDS = [
  {
    name: "view",
    command: bin("vite"),
    args: ["build"],
    sources: [
      { path: "src/view", recursive: true },
      { path: ".", files: ["index.html", "vite.config.ts"] },
    ],
  },
  {
    name: "server",
    command: bin("tsc"),
    args: ["-p", "tsconfig.server.json"],
    sources: [
      { path: "src/server", recursive: true },
      { path: ".", files: ["tsconfig.server.json"] },
    ],
  },
];

// The first build shows its output: nothing is up yet, and what is wrong is easiest to see here.
for (const build of BUILDS) {
  const done = spawnSync(build.command, build.args, { cwd: root, stdio: "inherit" });
  if (done.status !== 0) {
    console.error(
      `dev: the ${build.name} did not build${done.error ? `: ${done.error.message}` : ""}`,
    );
    process.exit(1);
  }
}

// From here on the screen is the view's, so what a build says goes to a file.
const log = openSync(join(root, "dev.log"), "w");
const failed = new Set();
const note = (text) => writeSync(log, `dev: ${text}\n`);

/** Builds `build` after a change, once for a burst of saves and never two at a time. */
function rebuilder(build) {
  let timer;
  let child;
  let again = false;

  function start() {
    again = false;
    child = spawn(build.command, build.args, { cwd: root, stdio: ["ignore", log, log] });
    child.on("error", (error) => {
      child = undefined;
      failed.add(build.name);
      note(`could not run the ${build.name} build: ${error.message}`);
    });
    child.on("close", (code) => {
      child = undefined;
      if (code === 0) failed.delete(build.name);
      else failed.add(build.name);
      if (again) start();
    });
  }

  return {
    changed() {
      clearTimeout(timer);
      timer = setTimeout(() => {
        if (child === undefined) start();
        else again = true;
      }, 100);
    },
    stop() {
      clearTimeout(timer);
      again = false;
      child?.kill();
    },
  };
}

const rebuilders = BUILDS.map(rebuilder);
const watchers = BUILDS.flatMap((build, index) =>
  build.sources.map(({ path, recursive = false, files }) =>
    // A directory is watched, not a file: an editor that saves by writing a new file and
    // renaming it over the old one would end the watch of the file.
    watch(join(root, path), { recursive }, (_event, name) => {
      if (files === undefined || files.includes(String(name))) rebuilders[index].changed();
    }).on("error", (error) => note(`cannot watch ${path}: ${error.message}`)),
  ),
);

function finish() {
  for (const rebuild of rebuilders) rebuild.stop();
  for (const watcher of watchers) watcher.close();
  closeSync(log);
}

// `pencere dev` runs the server, shows its view, and shows it again when the files in
// `dist` change.
const dev = spawn(
  bin("pencere"),
  [
    "dev",
    "--view",
    "dist/view",
    "--watch",
    "dist/server",
    "--",
    process.execPath,
    "dist/server/main.js",
  ],
  { cwd: root, stdio: "inherit" },
);

// ⌃C reaches this process and `pencere dev` together. It is `pencere dev` that takes
// the view down and gives the terminal back before it ends, so this waits for it.
process.on("SIGINT", () => {});
process.on("SIGTERM", () => dev.kill("SIGTERM"));

dev.on("error", (error) => {
  finish();
  console.error(`dev: could not run pencere: ${error.message}`);
  process.exitCode = 1;
});

dev.on("close", (code, signal) => {
  finish();
  if (failed.size > 0) {
    console.error(
      `dev: the last build of the ${[...failed].join(" and ")} failed; dev.log says why`,
    );
  }
  if (signal !== null) {
    // `pencere dev` ended by the signal the person sent, and so does this, for the shell to read.
    process.removeAllListeners(signal);
    process.kill(process.pid, signal);
  } else {
    process.exitCode = code ?? 1;
  }
});
