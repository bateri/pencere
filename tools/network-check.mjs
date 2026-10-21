// The part of the gate that needs the network, and so is not in `make check`: the two
// templates `create-pencere` writes are installed, built, type-checked and opened, and so
// is the Python example, each under the stand-in host.
//
//   node tools/network-check.mjs [react] [vue] [python]     (all three without arguments)
//
// What is tried is what the repository holds, not what is on npm: the packages are packed
// with `npm pack` and the generated projects depend on those tarballs, so a change to the SDK,
// the CLI or a template is tried together with the rest. The Python example needs the
// official `mcp` package from PyPI, which is why this is not part of `make check`.
//
// It needs Node, npm, cargo and python3 (with `venv`). Everything it makes is in one
// temporary directory, which it removes when every check has passed and keeps, naming it,
// when one has not. It touches nothing else of the machine except what npm and pip keep in
// their own caches.

import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fakeHost = join(root, "target", "debug", "pencere-fake-host");
const cli = join(root, "cli", "dist", "main.js");
const python = process.env.PENCERE_PYTHON ?? "python3";

class Failure extends Error {}

const github = process.env.GITHUB_ACTIONS === "true";
const group = (title) => console.log(github ? `::group::${title}` : `\n==> ${title}`);
const endGroup = () => github && console.log("::endgroup::");

/** Runs a command with its output on ours, and fails unless it ends well. */
function run(command, args, options = {}) {
  console.log(`$ ${[command, ...args].join(" ")}`);
  const done = spawnSync(command, args, { cwd: root, stdio: "inherit", ...options });
  if (done.error !== undefined) throw new Failure(`${command}: ${done.error.message}`);
  if (done.status !== 0) throw new Failure(`${command} ended with ${done.status ?? done.signal}`);
}

/** Runs a command and returns what it wrote to stdout. */
function capture(command, args, options = {}) {
  const done = spawnSync(command, args, {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
    ...options,
  });
  if (done.error !== undefined) throw new Failure(`${command}: ${done.error.message}`);
  if (done.status !== 0) throw new Failure(`${command} ended with ${done.status ?? done.signal}`);
  return done.stdout;
}

/** The tarball `npm pack` makes of the package in `dir`, the one `npm publish` would send. */
function pack(dir, into) {
  const [report] = JSON.parse(
    capture("npm", ["pack", "--json", "--pack-destination", into], { cwd: join(root, dir) }),
  );
  return join(into, report.filename);
}

/**
 * Starts `program` in the pane of the stand-in host, where a scripted view makes `calls` and then
 * the person presses ⌃C. The host's log (what the program wrote, what the host and the view did)
 * must hold the `expected` lines, in that order.
 */
function opens(label, cwd, calls, program, expected) {
  const options = calls.flatMap((call) => ["--call", call]);
  console.log(`$ pencere-fake-host ${options.join(" ")} -- ${program.join(" ")}`);
  const done = spawnSync(
    fakeHost,
    [...options, "--then", "interrupt", "--limit", "90", "--", ...program],
    { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
  );
  if (done.error !== undefined) throw new Failure(`${label}: ${done.error.message}`);
  const lines = done.stderr.split("\n");
  let from = 0;
  for (const step of expected) {
    const matches = (line) => (typeof step === "string" ? line === step : step.test(line));
    const found = lines.findIndex((line, index) => index >= from && matches(line));
    if (found < 0) {
      console.error(done.stderr);
      throw new Failure(`${label}: the host's log has no line ${step} after line ${from}`);
    }
    from = found + 1;
  }
  if (done.status !== 0) {
    console.error(done.stderr);
    throw new Failure(`${label}: the stand-in host ended with ${done.status ?? done.signal}`);
  }
  console.log(`ok: ${label}`);
}

// Where a view comes from: the files of a directory, which cross the terminal's stream, or
// the server's own `ui://` resource, which the terminal reads.
const FILES = {
  opened: "bundle of \\d+ files",
  started: /^view: started from the bundle: entry index\.html, \d+ files$/,
};
const RESOURCE = {
  opened: "without a bundle",
  started: /^view: started from resources\/read: a document of \d+ bytes$/,
};

/** The log of a view that opened with the pane's capability, made `calls` and was ended by ⌃C. */
function session(tool, program, source, calls) {
  return [
    new RegExp(`^program: open id=\\d+ tool=${tool} ${source.opened}$`),
    source.started,
    new RegExp(`^view: title="[^"]+" program="${program}"$`),
    // The pane's capability is named in the view's `ui/initialize`; the host says it saw it.
    /^view: <- ui\/initialize result: .* pane=true$/,
    ...calls,
    "host: the person presses ^C",
    /^program: bye id=\d+ reason=signal$/,
    "view: destroyed",
    "screen: alternate screen off",
  ];
}

/** A template: written by the packed `create-pencere`, built, and opened three ways. */
function template(name, packs, work) {
  const project = join(work, `${name}-tool`);
  // The package as it is published, run the way `npm create pencere` runs it.
  run(
    "npm",
    ["exec", "--yes", `--package=${packs.create}`, "--", "create-pencere", project, "-t", name],
    { cwd: work },
  );

  // The project depends on the repository's packages, not on the registry's.
  const manifestPath = join(project, "package.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  manifest.dependencies["@pencere/sdk"] = `file:${packs.sdk}`;
  manifest.devDependencies["@pencere/cli"] = `file:${packs.cli}`;
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

  run("npm", ["install", "--no-audit", "--no-fund"], { cwd: project });
  run("npm", ["run", "build"], { cwd: project });
  run("npm", ["run", "typecheck"], { cwd: project });

  const program = `${name}-tool`;
  const calls = ["view: -> tools/call add_one {}", 'view: <- tool result "The count is 1."'];
  const pencere = join(project, "node_modules", ".bin", "pencere");
  const server = ["node", "dist/server/main.js"];

  opens(
    `${name}: pencere run --view dist/view`,
    project,
    ["add_one"],
    [pencere, "run", "--view", "dist/view", "--", ...server],
    session("show_counter", program, FILES, calls),
  );
  opens(
    `${name}: pencere run`,
    project,
    ["add_one"],
    [pencere, "run", "--", ...server],
    session("show_counter", program, RESOURCE, calls),
  );
  // `npm run dev` is the real Vite build, and then `pencere dev` showing what it wrote.
  opens(
    `${name}: npm run dev`,
    project,
    ["add_one"],
    ["npm", "run", "dev"],
    session("show_counter", program, FILES, calls),
  );
}

/** The Python example: its one dependency installed from PyPI, and opened by the repository's CLI. */
function pythonExample(work) {
  const example = join(root, "examples", "python");
  const venv = join(work, "venv");
  run(python, ["-m", "venv", venv]);
  run(join(venv, "bin", "pip"), [
    "install",
    "--disable-pip-version-check",
    "--requirement",
    join(example, "requirements.txt"),
  ]);

  const server = [join(venv, "bin", "python"), "server.py"];
  const toggle = ['toggle_task={"id":"view"}'];
  const expected = session("show_tasks", "tasks", RESOURCE, [
    'view: -> tools/call toggle_task {"id":"view"}',
    'view: <- tool result "Finished: Show its view in a terminal pane"',
  ]);
  opens("python: pencere run", example, toggle, ["node", cli, "run", "--", ...server], expected);
  // The command the example's README gives for working on it.
  opens(
    "python: pencere dev --watch server.py",
    example,
    toggle,
    ["node", cli, "dev", "--watch", "server.py", "--", ...server],
    expected,
  );
}

const TARGETS = {
  react: (packs, work) => template("react", packs(), work),
  vue: (packs, work) => template("vue", packs(), work),
  python: (_packs, work) => pythonExample(work),
};

const requested = process.argv.slice(2);
const unknown = requested.filter((name) => !(name in TARGETS));
if (unknown.length > 0) {
  console.error(
    `network-check: no target ${unknown.join(", ")}; the targets are ${Object.keys(TARGETS).join(", ")}`,
  );
  process.exit(2);
}
const targets = requested.length > 0 ? requested : Object.keys(TARGETS);

// Python writes its bytecode next to the example otherwise.
process.env.PYTHONDONTWRITEBYTECODE = "1";

const work = mkdtempSync(join(tmpdir(), "pencere-network-check-"));
let packed;
/** The repository's three packages as tarballs, made once and only when a target needs them. */
const packs = () => {
  if (packed === undefined) {
    const into = join(work, "packs");
    mkdirSync(into);
    packed = {
      sdk: pack("sdk/typescript", into),
      cli: pack("cli", into),
      create: pack("create", into),
    };
  }
  return packed;
};

try {
  group("build what is tried");
  run("npx", ["tsc", "-b"]);
  run("cargo", ["build", "-p", "pencere-fake-host"]);
  endGroup();
  for (const name of targets) {
    group(name);
    TARGETS[name](packs, work);
    endGroup();
  }
  rmSync(work, { recursive: true, force: true });
  console.log(`\nnetwork-check: ${targets.join(", ")} passed`);
} catch (error) {
  endGroup();
  if (!(error instanceof Failure)) throw error;
  console.error(`\nnetwork-check: ${error.message}\nwhat it made is kept in ${work}`);
  process.exit(1);
}
