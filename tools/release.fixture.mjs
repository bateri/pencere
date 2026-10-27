// What the tests of the release tooling run on: a copy of the repository's shape to release from,
// `release.mjs` run as a process on it, and stand-ins for the world it asks (`cargo`, `gh` and
// `npm`), which answer from a state file and write what they were asked to a log. Nothing here
// reaches the network, the user's configuration or anything outside a temporary directory.

import { execFileSync, spawnSync } from "node:child_process";
import {
  chmodSync,
  cpSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, afterEach } from "vitest";

import { LINES } from "./lines.mjs";

export const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const json = (root, ...path) => JSON.parse(readFileSync(join(root, ...path), "utf8"));
export const text = (root, ...path) => readFileSync(join(root, ...path), "utf8");

/** The crates in the order they are published, the packages of every line, and the cargo ones. */
export const members = LINES.crates.members;
export const packages = [LINES["sdk-typescript"], ...members, LINES.cli];
export const cargoPackages = [...members, LINES.cli];

/** The version of a crate in its own `[package]`, not the workspace's. */
export const crateVersion = (root, dir) =>
  /^\[package\][^[]*?^version = "([^"]+)"$/ms.exec(text(root, dir, "Cargo.toml"))?.[1];

const roots = [];
afterEach(() => {
  while (roots.length > 0) rmSync(roots.pop(), { recursive: true, force: true });
});

export const scratch = () => {
  const dir = mkdtempSync(join(tmpdir(), "pencere-release-test-"));
  roots.push(dir);
  return dir;
};

export const write = (root, path, value) => {
  mkdirSync(dirname(join(root, path)), { recursive: true });
  writeFileSync(
    join(root, path),
    typeof value === "string" ? value : `${JSON.stringify(value, null, 2)}\n`,
  );
};

/** A changelog with something unreleased. */
export const CHANGELOG = (what) => `# Changelog

## [Unreleased]

### Added

- ${what}
`;

/** A changelog whose newest section is 0.1.0 and nothing is unreleased. */
export const RELEASED = `# Changelog

## [Unreleased]

## [0.1.0] - 2026-01-01

### Added

- a change
`;

/**
 * A copy of the repository's shape: the SDK, the seven crates and the command at `0.1.0` (the
 * program library at `programVersion`, as it is on crates.io's side of the story) with something
 * unreleased in every changelog, the crates asking for each other by path and version, and the
 * workspace of the page modules of its own beside them.
 */
export function fixture({ programVersion = "0.2.0" } = {}) {
  const root = scratch();
  const sdk = { name: "@pencere/sdk", version: "0.1.0", devDependencies: { zod: "^4.6.7" } };
  write(root, "sdk/typescript/package.json", sdk);
  write(root, "package-lock.json", {
    lockfileVersion: 3,
    packages: { "": { name: "root", workspaces: ["sdk/*"] }, "sdk/typescript": sdk },
  });
  write(root, "sdk/typescript/CHANGELOG.md", CHANGELOG("a change of the SDK"));
  write(
    root,
    "Cargo.toml",
    `[workspace]\nresolver = "3"\nmembers = ["wire", "host/rust", "kit/view", "kit/editor", "kit/sandbox", "program", "cli"]\nexclude = ["kit/guest"]\n\n[workspace.package]\nversion = "0.0.0"\nedition = "2024"\n`,
  );
  const versions = { "pencere-wire": "0.1.0", "pencere-view": "0.1.0", pencere: programVersion };
  const crate = (dir, name, version, dependencies = {}) => {
    const asks = Object.entries(dependencies)
      .map(([other, path]) => `${other} = { path = "${path}", version = "${versions[other]}" }\n`)
      .join("");
    write(
      root,
      `${dir}/Cargo.toml`,
      `[package]\nname = "${name}"\nversion = "${version}"\nedition.workspace = true\n\n[dependencies]\n${asks}`,
    );
    write(root, `${dir}/src/${name === "pencere-cli" ? "main" : "lib"}.rs`, "");
    write(root, `${dir}/CHANGELOG.md`, CHANGELOG(`a change of ${name}`));
  };
  crate("wire", "pencere-wire", "0.1.0");
  crate("host/rust", "pencere-host", "0.1.0", { "pencere-wire": "../../wire" });
  crate("kit/view", "pencere-view", "0.1.0");
  crate("kit/editor", "pencere-editor", "0.1.0", { "pencere-view": "../view" });
  crate("kit/sandbox", "pencere-sandbox", "0.1.0", { "pencere-view": "../view" });
  crate("program", "pencere", programVersion, { "pencere-wire": "../wire" });
  crate("cli", "pencere-cli", "0.1.0", { pencere: "../program", "pencere-wire": "../wire" });
  // The page modules: the SDK asks for the engine and the wire by path and version too.
  write(
    root,
    "kit/guest/Cargo.toml",
    `[workspace]\nresolver = "3"\nmembers = ["sdk"]\n\n[workspace.package]\nversion = "0.1.0"\nedition = "2024"\n\n[workspace.dependencies]\npencere-view = { path = "../view", version = "0.1.0" }\npencere-wire = { path = "../../wire", version = "0.1.0" }\n`,
  );
  write(
    root,
    "kit/guest/sdk/Cargo.toml",
    `[package]\nname = "pencere-page"\nversion = "0.1.0"\nedition.workspace = true\n\n[dependencies]\npencere-view = { workspace = true }\npencere-wire = { workspace = true }\n`,
  );
  write(root, "kit/guest/sdk/src/lib.rs", "");
  write(root, "kit/guest/sdk/CHANGELOG.md", CHANGELOG("a change of the page SDK"));
  return root;
}

/** The scripts of this repository, put into the fixture to run on its files. */
export function install(root) {
  mkdirSync(join(root, "tools"), { recursive: true });
  cpSync(join(repo, "tools/release.mjs"), join(root, "tools/release.mjs"));
  cpSync(join(repo, "tools/lines.mjs"), join(root, "tools/lines.mjs"));
}

/** `tools/release.mjs` of the fixture, with `env` besides the environment of the test. */
export function releaseWith(root, env, ...args) {
  install(root);
  const done = spawnSync("node", [join(root, "tools/release.mjs"), ...args], {
    encoding: "utf8",
    env: { ...process.env, ...env },
  });
  return { status: done.status, out: done.stdout, err: done.stderr };
}

export const release = (root, ...args) => releaseWith(root, {}, ...args);

// ---- a repository with an origin, and stand-ins of the world -------------------

/** What git is told in every run, so that no user's configuration (identity, signing) is read. */
export const GIT_ENV = {
  GIT_AUTHOR_NAME: "test",
  GIT_AUTHOR_EMAIL: "test@example.com",
  GIT_COMMITTER_NAME: "test",
  GIT_COMMITTER_EMAIL: "test@example.com",
  GIT_CONFIG_COUNT: "2",
  GIT_CONFIG_KEY_0: "commit.gpgsign",
  GIT_CONFIG_VALUE_0: "false",
  GIT_CONFIG_KEY_1: "tag.gpgsign",
  GIT_CONFIG_VALUE_1: "false",
};

export const git = (cwd, ...args) =>
  execFileSync("git", args, {
    cwd,
    encoding: "utf8",
    env: { ...process.env, ...GIT_ENV },
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();

/**
 * One program that stands in for `cargo`, `gh` and `npm` (it is told which by the name it
 * is run as). It answers from the state file (`FAKE_STATE`), keeps what it changes there, and
 * writes every call, with the directory it was made in, to `FAKE_LOG`.
 */
const FAKE = `#!/usr/bin/env node
const fs = require("node:fs");
const path = require("node:path");
const tool = path.basename(process.argv[1]);
const args = process.argv.slice(2);
fs.appendFileSync(process.env.FAKE_LOG, JSON.stringify({ tool, args, cwd: process.cwd() }) + "\\n");
const state = JSON.parse(fs.readFileSync(process.env.FAKE_STATE, "utf8"));
const save = () => fs.writeFileSync(process.env.FAKE_STATE, JSON.stringify(state));
const fail = (said, code = 1) => {
  console.error(said);
  process.exit(code);
};
const flag = (name) => {
  const i = args.indexOf(name);
  return i < 0 ? undefined : args[i + 1];
};
if (tool === "cargo") {
  if (args[0] === "metadata") {
    const crates = Object.entries(state.crates).map(([name, c]) => ({
      name,
      dependencies: c.dependencies,
      ...state.manifest,
      ...(c.manifest || {}),
    }));
    console.log(JSON.stringify({ packages: crates }));
  } else if (args[0] === "package") {
    console.log(state.crates[flag("-p")].files.join("\\n"));
  } else if (args[0] === "info") {
    if (state.registry.includes(args[1])) console.log(args[1]);
    else fail("error: could not find \\u0060" + args[1] + "\\u0060 in registry \\u0060crates-io\\u0060", 101);
  } else if (args[0] === "publish") {
    const name = flag("-p");
    if (args.includes("--dry-run")) {
      if ((state.failDryRun || []).includes(name)) fail("error: failed to verify package " + name, 101);
    } else {
      if (state.failPublish === name) fail("error: failed to publish " + name, 101);
      state.registry.push(name + "@" + state.version);
      save();
    }
  }
} else if (tool === "gh") {
  const gh = state.gh;
  const [a, b, tag] = args;
  if (a === "auth") process.exit(gh.auth === false ? 1 : 0);
  else if (a === "release" && b === "view") {
    if (!gh.releases[tag]) fail("release not found");
    console.log(JSON.stringify({ isDraft: gh.releases[tag].draft }));
  } else if (a === "release" && b === "create") {
    if (gh.cannotCreate) fail("HTTP 500");
    gh.releases[tag] = { draft: args.includes("--draft"), args };
    save();
  } else if (a === "release" && b === "delete") {
    delete gh.releases[tag];
    save();
  } else if (a === "release" && b === "edit") {
    gh.releases[tag].edit = args;
    if (args.includes("--draft=false")) gh.releases[tag].draft = false;
    save();
  } else if (a === "release" && b === "download") {
    for (const file of fs.readdirSync(gh.assets)) {
      fs.copyFileSync(path.join(gh.assets, file), path.join(flag("--dir"), file));
    }
  } else if (a === "api") {
    if (gh.environment === undefined) fail("gh: Not Found (HTTP 404)");
    console.log(JSON.stringify(gh.environment));
  } else if (a === "run") console.log(JSON.stringify(gh.runs));
  else if (a === "repo") console.log("bateri/pencere");
  else if (a === "attestation" && gh.badAttestation) fail("verification failed");
} else if (tool === "npm") {
  if (args[0] === "view" && args[2] === "version") {
    if (!state.npm.published.includes(args[1])) fail("npm error code E404");
    console.log(args[1].slice(args[1].lastIndexOf("@") + 1));
  } else if (args[0] === "view") {
    if (!(args[1] in state.npm.deprecated)) fail("npm error code E404");
    console.log(state.npm.deprecated[args[1]]);
  } else if (args[0] === "pack") {
    console.log(JSON.stringify([{ files: state.npm.files.map((p) => ({ path: p })) }]));
  } else if (args[0] === "publish") {
    if (state.npm.failPublish) fail("npm error code E403");
    state.npm.published.push("@pencere/sdk@" + state.version);
    save();
  }
}
`;

/**
 * The repository and its `origin` that every `world()` starts from, made once: every line at
 * `0.1.0` and released in its changelog, the licence files in place, committed and pushed, and the
 * stand-ins beside it.
 */
let template;
function worldTemplate() {
  if (template !== undefined) return template;
  const root = fixture({ programVersion: "0.1.0" });
  roots.splice(roots.indexOf(root), 1);
  const origin = mkdtempSync(join(tmpdir(), "pencere-release-test-"));
  const bin = mkdtempSync(join(tmpdir(), "pencere-release-test-"));
  template = { root, origin, bin };
  write(root, ".gitignore", "dist/\ntarget/\n");
  write(root, "LICENSE", "Apache License\n");
  write(root, "NOTICE", "pencere\n");
  for (const pkg of packages) {
    write(root, `${pkg.dir}/LICENSE`, "Apache License\n");
    write(root, `${pkg.dir}/NOTICE`, "pencere\n");
    write(root, `${pkg.dir}/README.md`, "# readme\n");
    write(root, `${pkg.dir}/CHANGELOG.md`, RELEASED);
  }
  for (const entry of LINES.cli.entries) write(root, `cli/${entry}`, "fn main() {}\n");
  install(root);
  git(root, "init", "-q", "-b", "main");
  git(root, "add", "-A");
  git(root, "commit", "-q", "-m", "fixture");
  git(root, "init", "-q", "--bare", "-b", "main", origin);
  git(root, "remote", "add", "origin", origin);
  git(root, "push", "-q", "origin", "main");
  for (const tool of ["cargo", "gh", "npm"]) {
    write(bin, tool, FAKE);
    chmodSync(join(bin, tool), 0o755);
  }
  return template;
}
afterAll(() => {
  for (const dir of template === undefined ? [] : Object.values(template)) {
    rmSync(dir, { recursive: true, force: true });
  }
});

/** The files a package of a crate carries, as `cargo package --list` says them. */
const filesOf = (pkg) => [
  "LICENSE",
  "NOTICE",
  "README.md",
  "CHANGELOG.md",
  "Cargo.toml",
  ...pkg.entries,
];

/** The crates each crate asks for by path, as the fixture has them. */
const ASKS = {
  "pencere-host": ["pencere-wire"],
  "pencere-editor": ["pencere-view"],
  "pencere-sandbox": ["pencere-view"],
  "pencere-page": ["pencere-view", "pencere-wire"],
  pencere: ["pencere-wire"],
};

/**
 * A copy of the template: a git repository on `main` with its own `origin`, to commit to and
 * release from, and the stand-ins, which answer from `state`: the crates as cargo reports them,
 * what crates.io has, what npm has and what its package carries, and what GitHub holds (releases,
 * runs of the gate, the files a draft carries). A run reads the state the stand-ins left.
 */
export function world() {
  const { root: from, origin: fromOrigin, bin } = worldTemplate();
  const root = scratch();
  const origin = scratch();
  const aux = scratch();
  cpSync(from, root, { recursive: true });
  cpSync(fromOrigin, origin, { recursive: true });
  git(root, "remote", "set-url", "origin", origin);
  write(aux, "log", "");

  // The fields of a manifest that cargo reports, complete unless a test takes one away.
  const manifest = {
    description: "a crate",
    license: "Apache-2.0",
    repository: "https://example.com/r",
    readme: "README.md",
    keywords: ["pencere"],
    categories: ["gui"],
    publish: null,
  };
  const state = {
    version: "0.1.0",
    manifest,
    registry: [],
    failDryRun: [],
    crates: Object.fromEntries(
      members.map((pkg) => [
        pkg.name,
        {
          files: filesOf(pkg),
          dependencies: (ASKS[pkg.name] ?? []).map((name) => ({
            name,
            req: "^0.1.0",
            path: `/somewhere/${name}`,
            kind: null,
          })),
        },
      ]),
    ),
    npm: {
      published: [],
      deprecated: {},
      // What `npm pack` says the package carries.
      files: [
        "LICENSE",
        "NOTICE",
        "README.md",
        "CHANGELOG.md",
        "package.json",
        ...LINES["sdk-typescript"].entries,
      ],
    },
    gh: { auth: true, releases: {}, runs: [], assets: null },
  };
  const statePath = join(aux, "state.json");
  const runWith = (env, ...args) => {
    write(aux, "state.json", state);
    const done = releaseWith(
      root,
      {
        ...GIT_ENV,
        PATH: `${bin}:${process.env.PATH}`,
        FAKE_LOG: join(aux, "log"),
        FAKE_STATE: statePath,
        ...env,
      },
      ...args,
    );
    // What the stand-ins changed (a published crate, a release made) is what the next run finds.
    Object.assign(state, JSON.parse(readFileSync(statePath, "utf8")));
    return done;
  };
  const calls = () =>
    readFileSync(join(aux, "log"), "utf8")
      .split("\n")
      .filter((line) => line !== "")
      .map((line) => JSON.parse(line));
  return {
    root,
    origin,
    aux,
    state,
    git: (...args) => git(root, ...args),
    calls,
    /** The calls of one tool, each as the words of its command line. */
    said: (tool) =>
      calls()
        .filter((call) => call.tool === tool)
        .map((call) => call.args.join(" ")),
    run: (...args) => runWith({}, ...args),
    runWith,
    forget: () => write(aux, "log", ""),
  };
}

// ---- the command's files ----------------------------------------------------------

/** The first bytes of an executable of a system and a processor, as the loader reads them. */
export const executable = (system, processor) => {
  const bytes = Buffer.alloc(64);
  if (system === "linux") {
    bytes.set([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1], 0);
    bytes.writeUInt16LE(processor === "aarch64" ? 183 : 62, 18);
  } else {
    bytes.writeUInt32LE(0xfeedfacf, 0);
    bytes.writeUInt32LE(processor === "aarch64" ? 0x0100000c : 0x01000007, 4);
  }
  return bytes;
};

export const systemOf = (target) => (target.includes("apple") ? "macos" : "linux");
export const processorOf = (target) => (target.startsWith("aarch64") ? "aarch64" : "x86_64");
export const nameOf = (target, version = "0.1.0") => `pencere-${version}-${target}`;
