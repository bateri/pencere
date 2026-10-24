// The release tooling's rules that no other check sees: `release.mjs prepare` moves a line's
// version, in its manifest and in the lockfile, and dates its changelog; `release.mjs check`
// refuses a release whose version and changelog disagree, and what is wrong with a package, a
// crate or the files of a binary; `push` and `local` do what the kind of the line is released by.
// The parts that ask the world (crates.io, GitHub) run against stand-in `cargo` and `gh`.

import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  chmodSync,
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, afterEach, describe, expect, it, vi } from "vitest";

import { LINES } from "./lines.mjs";

// Each test runs the script as a process, and some make a repository: on a busy machine that is
// more than the default five seconds.
vi.setConfig({ testTimeout: 30_000 });

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const json = (root, ...path) => JSON.parse(readFileSync(join(root, ...path), "utf8"));
const text = (root, ...path) => readFileSync(join(root, ...path), "utf8");
const npmLines = Object.entries(LINES).filter(([, line]) => line.kind === "npm");
const rustLines = Object.entries(LINES).filter(([, line]) => line.kind !== "npm");
const cargoLines = Object.entries(LINES).filter(([, line]) => line.kind === "cargo");

/** The version of a crate in its own `[package]`, not the workspace's. */
const crateVersion = (root, dir) =>
  /^\[package\][^[]*?^version = "([^"]+)"$/ms.exec(text(root, dir, "Cargo.toml"))?.[1];

describe("the repository's lines", () => {
  it("are the packages the lines say they are", () => {
    for (const [id, line] of npmLines) {
      expect(json(repo, line.dir, "package.json").name, id).toBe(line.name);
    }
    for (const [id, line] of rustLines) {
      expect(text(repo, line.dir, "Cargo.toml"), id).toMatch(
        new RegExp(`^\\[package\\]\\nname = "${line.name}"$`, "m"),
      );
      expect(crateVersion(repo, line.dir), `${id} has a version of its own`).toMatch(
        /^\d+\.\d+\.\d+/,
      );
    }
  });

  it("have the lockfile record each npm package at the version of its manifest", () => {
    const lock = json(repo, "package-lock.json");
    for (const [, line] of npmLines) {
      const manifest = json(repo, line.dir, "package.json");
      expect(lock.packages[line.dir]?.version, line.name).toBe(manifest.version);
    }
  });

  it("have the lockfile of their workspace record each crate at the version of its manifest", () => {
    for (const [, line] of rustLines) {
      const lock = text(repo, line.manifest === undefined ? "Cargo.lock" : "kit/guest/Cargo.lock");
      const entry = new RegExp(`^name = "${line.name}"\\nversion = "([^"]+)"$`, "m").exec(lock);
      expect(entry?.[1], `${line.name} in the lockfile`).toBe(crateVersion(repo, line.dir));
    }
  });

  it("carry what a release checks for: the repository's licence, a readme, a changelog, the entries", () => {
    for (const [id, line] of rustLines) {
      for (const name of ["LICENSE", "NOTICE"]) {
        expect(text(repo, line.dir, name), `${id}/${name}`).toBe(text(repo, name));
      }
      for (const file of ["README.md", "CHANGELOG.md", ...line.entries]) {
        expect(existsSync(join(repo, line.dir, file)), `${id}/${file}`).toBe(true);
      }
      expect(text(repo, line.dir, "CHANGELOG.md"), id).toMatch(/^## \[Unreleased\]$/m);
    }
  });

  it("are listed in the order the crates are published: each after the crates it depends on", () => {
    const names = cargoLines.map(([, line]) => line.name);
    for (const [, line] of cargoLines) {
      const own = text(repo, line.dir, "Cargo.toml");
      const section = /^\[dependencies\]\n([\s\S]*?)(?=^\[|(?![\s\S]))/m.exec(own)?.[1] ?? "";
      for (const dependency of names.filter((name) =>
        new RegExp(`^${name} *=`, "m").test(section),
      )) {
        expect(names.indexOf(dependency), `${dependency} before ${line.name}`).toBeLessThan(
          names.indexOf(line.name),
        );
      }
    }
  });

  it("have the crates that read the interface read their own copy of it", () => {
    for (const dir of ["kit/sandbox", "kit/guest/sdk"]) {
      expect(text(repo, dir, "wit/view.wit"), dir).toBe(text(repo, "kit/wit/view.wit"));
      expect(text(repo, dir, "Cargo.toml"), dir).toMatch(/"wit\/\*\*"/);
    }
    expect(text(repo, "kit/sandbox/src/world.rs")).toContain('path: "wit"');
    expect(text(repo, "kit/guest/sdk/src/lib.rs")).toContain('path: "wit"');
  });
});

const roots = [];
afterEach(() => {
  while (roots.length > 0) rmSync(roots.pop(), { recursive: true, force: true });
});

const scratch = () => {
  const dir = mkdtempSync(join(tmpdir(), "pencere-release-test-"));
  roots.push(dir);
  return dir;
};

const write = (root, path, value) => {
  mkdirSync(dirname(join(root, path)), { recursive: true });
  writeFileSync(
    join(root, path),
    typeof value === "string" ? value : `${JSON.stringify(value, null, 2)}\n`,
  );
};

const CHANGELOG = (what) => `# Changelog

## [Unreleased]

### Added

- ${what}
`;

const RELEASED = `# Changelog

## [Unreleased]

## [0.1.0] - 2026-01-01

### Added

- a change
`;

/**
 * A copy of the repository's shape: every line of `lines.mjs` at `0.1.0` with something
 * unreleased in its changelog, the crates of the workspace asking for each other by path and
 * version, and the workspace of the page modules of its own beside it.
 */
function fixture() {
  const root = scratch();
  const sdk = { name: "@pencere/sdk", version: "0.1.0", devDependencies: { zod: "^4.6.7" } };
  write(root, "sdk/typescript/package.json", sdk);
  write(root, "package-lock.json", {
    lockfileVersion: 3,
    packages: { "": { name: "root", workspaces: ["sdk/*"] }, "sdk/typescript": sdk },
  });
  write(root, "sdk/typescript/CHANGELOG.md", CHANGELOG("a change of the SDK"));
  // The crate's version is its own line in [package], apart from the workspace's.
  write(
    root,
    "Cargo.toml",
    `[workspace]\nresolver = "3"\nmembers = ["wire", "host/rust", "kit/view", "kit/sandbox", "program", "cli"]\nexclude = ["kit/guest"]\n\n[workspace.package]\nversion = "0.0.0"\nedition = "2024"\n`,
  );
  const crate = (dir, name, dependencies = {}) => {
    const asks = Object.entries(dependencies)
      .map(([other, path]) => `${other} = { path = "${path}", version = "0.1.0" }\n`)
      .join("");
    write(
      root,
      `${dir}/Cargo.toml`,
      `[package]\nname = "${name}"\nversion = "0.1.0"\nedition.workspace = true\n\n[dependencies]\n${asks}`,
    );
    write(root, `${dir}/src/${name === "pencere-cli" ? "main" : "lib"}.rs`, "");
    write(root, `${dir}/CHANGELOG.md`, CHANGELOG(`a change of ${name}`));
  };
  crate("wire", "pencere-wire");
  crate("host/rust", "pencere-host", { "pencere-wire": "../../wire" });
  crate("kit/view", "pencere-view");
  crate("kit/sandbox", "pencere-sandbox", { "pencere-view": "../view" });
  crate("program", "pencere", { "pencere-wire": "../wire" });
  crate("cli", "pencere-cli", { pencere: "../program", "pencere-wire": "../wire" });
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

/** The script of this repository, put into the fixture to run on its files. */
function install(root) {
  mkdirSync(join(root, "tools"), { recursive: true });
  cpSync(join(repo, "tools/release.mjs"), join(root, "tools/release.mjs"));
  cpSync(join(repo, "tools/lines.mjs"), join(root, "tools/lines.mjs"));
}

/** `tools/release.mjs` of the fixture, with `env` besides the environment of the test. */
function releaseWith(root, env, ...args) {
  install(root);
  const done = spawnSync("node", [join(root, "tools/release.mjs"), ...args], {
    encoding: "utf8",
    env: { ...process.env, ...env },
  });
  return { status: done.status, out: done.stdout, err: done.stderr };
}

const release = (root, ...args) => releaseWith(root, {}, ...args);

describe("release.mjs prepare", () => {
  it("moves the version of an npm line in its manifest and the lockfile, and dates the changelog", () => {
    const root = fixture();
    const done = release(root, "prepare", "sdk-typescript", "0.2.0");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(json(root, "sdk/typescript", "package.json").version).toBe("0.2.0");
    expect(json(root, "package-lock.json").packages["sdk/typescript"].version).toBe("0.2.0");
    const changelog = text(root, "sdk/typescript/CHANGELOG.md");
    expect(changelog).toMatch(/## \[Unreleased\]\n\n## \[0\.2\.0\] - \d{4}-\d{2}-\d{2}\n/);
    expect(changelog).toContain("- a change of the SDK");
    // The other line is not moved with it.
    expect(text(root, "host/rust/Cargo.toml")).toContain('version = "0.1.0"');
    expect(text(root, "host/rust/CHANGELOG.md")).not.toContain("0.2.0");
  });

  it("moves the crate's own version, not the workspace's", () => {
    const root = fixture();
    const done = release(root, "prepare", "host-rust", "0.2.0");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(crateVersion(root, "host/rust")).toBe("0.2.0");
    expect(text(root, "Cargo.toml")).toContain('version = "0.0.0"');
    expect(text(root, "host/rust/CHANGELOG.md")).toMatch(/## \[0\.2\.0\] - \d{4}-\d{2}-\d{2}/);
    expect(json(root, "sdk/typescript", "package.json").version).toBe("0.1.0");
  });

  it.each(["wire", "view", "sandbox", "program"])(
    "moves the version of the crate line %s in its manifest and the lockfile",
    (id) => {
      const root = fixture();
      const { dir, name } = LINES[id];
      const done = release(root, "prepare", id, "0.1.1");
      expect(done.err).toBe("");
      expect(done.status).toBe(0);
      expect(crateVersion(root, dir)).toBe("0.1.1");
      expect(text(root, "Cargo.lock")).toContain(`name = "${name}"\nversion = "0.1.1"`);
      for (const [other, line] of cargoLines) {
        if (other !== id) expect(crateVersion(root, line.dir), other).toBe("0.1.0");
      }
    },
  );

  it("moves the version of the page SDK, whose workspace is the page modules'", () => {
    const root = fixture();
    const done = release(root, "prepare", "page", "0.2.0");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(crateVersion(root, "kit/guest/sdk")).toBe("0.2.0");
    expect(text(root, "kit/guest/Cargo.lock")).toContain(
      'name = "pencere-page"\nversion = "0.2.0"',
    );
    expect(text(root, "kit/guest/Cargo.toml")).toContain('version = "0.1.0"');
    expect(text(root, "kit/guest/sdk/CHANGELOG.md")).toMatch(/## \[0\.2\.0\] - \d{4}-\d{2}-\d{2}/);
    // The root workspace does not hold it.
    expect(text(root, "Cargo.lock")).not.toContain("pencere-page");
  });

  it("moves the version of the binary line in its crate's manifest, and tells what comes next", () => {
    const root = fixture();
    const done = release(root, "prepare", "cli", "0.2.0");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(crateVersion(root, "cli")).toBe("0.2.0");
    expect(text(root, "Cargo.lock")).toContain('name = "pencere-cli"\nversion = "0.2.0"');
    expect(done.out).toContain("make dist, then make release LINE=cli");
  });

  it("moves what the crates that ask for a crate ask for, when its new version is past it", () => {
    const root = fixture();
    const done = release(root, "prepare", "wire", "0.2.0");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    for (const file of [
      "host/rust/Cargo.toml",
      "program/Cargo.toml",
      "cli/Cargo.toml",
      "kit/guest/Cargo.toml",
    ]) {
      expect(text(root, file), file).toMatch(
        /pencere-wire = \{ path = "[./]+wire", version = "0\.2\.0" \}/,
      );
    }
    // What is asked of another crate is left as it is.
    expect(text(root, "cli/Cargo.toml")).toContain(
      'pencere = { path = "../program", version = "0.1.0" }',
    );
    expect(text(root, "kit/sandbox/Cargo.toml")).toContain(
      'pencere-view = { path = "../view", version = "0.1.0" }',
    );
    expect(done.out).toContain("program/Cargo.toml asked for pencere-wire 0.1.0");
    expect(text(root, "Cargo.lock")).toContain('name = "pencere-wire"\nversion = "0.2.0"');
    expect(text(root, "kit/guest/Cargo.lock")).toContain(
      'name = "pencere-wire"\nversion = "0.2.0"',
    );
  });

  it("moves what the crates ask for to a pre-release, which no plain requirement meets", () => {
    const root = fixture();
    const done = release(root, "prepare", "wire", "0.1.1-rc.1");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(text(root, "program/Cargo.toml")).toContain(
      'pencere-wire = { path = "../wire", version = "0.1.1-rc.1" }',
    );
  });

  it("leaves what the crates ask for alone when the new version still meets it", () => {
    const root = fixture();
    const done = release(root, "prepare", "wire", "0.1.1");
    expect(done.status).toBe(0);
    expect(text(root, "program/Cargo.toml")).toContain(
      'pencere-wire = { path = "../wire", version = "0.1.0" }',
    );
    expect(text(root, "kit/guest/Cargo.toml")).toContain(
      'pencere-wire = { path = "../../wire", version = "0.1.0" }',
    );
    expect(done.out).not.toContain("asked for");
  });

  it("refuses a version that is not after the line's, and changes nothing", () => {
    const root = fixture();
    const done = release(root, "prepare", "sdk-typescript", "0.1.0");
    expect(done.status).toBe(1);
    expect(done.err).toContain("0.1.0 is not after 0.1.0");
    expect(json(root, "sdk/typescript", "package.json").version).toBe("0.1.0");
    expect(text(root, "sdk/typescript/CHANGELOG.md")).toBe(CHANGELOG("a change of the SDK"));
  });

  it("refuses a line with nothing unreleased", () => {
    const root = fixture();
    write(root, "sdk/typescript/CHANGELOG.md", "# Changelog\n\n## [Unreleased]\n");
    const done = release(root, "prepare", "sdk-typescript", "0.2.0");
    expect(done.status).toBe(1);
    expect(done.err).toContain("Unreleased is empty, nothing to release");
    expect(json(root, "sdk/typescript", "package.json").version).toBe("0.1.0");
  });
});

describe("release.mjs check", () => {
  const released = (version, date = "2026-01-01") =>
    `# Changelog\n\n## [Unreleased]\n\n## [${version}] - ${date}\n\n### Added\n\n- a change\n`;
  const at = (root, version) =>
    write(root, "sdk/typescript/package.json", {
      ...json(root, "sdk/typescript", "package.json"),
      version,
    });

  it("refuses a line that was never prepared", () => {
    const root = fixture();
    at(root, "0.0.0");
    const done = release(root, "check", "sdk-typescript");
    expect(done.status).toBe(1);
    expect(done.err).toContain("@pencere/sdk is at 0.0.0");
  });

  it("refuses an Unreleased section that still holds changes", () => {
    const done = release(fixture(), "check", "sdk-typescript");
    expect(done.status).toBe(1);
    expect(done.err).toContain("Unreleased is not empty");
  });

  it("refuses an Unreleased section that still holds changes, in a crate and in the binary too", () => {
    for (const id of ["wire", "page", "cli"]) {
      const done = release(fixture(), "check", id);
      expect(done.status, id).toBe(1);
      expect(done.err, id).toContain("Unreleased is not empty");
    }
  });

  it("refuses a line whose changelog is not there", () => {
    const root = fixture();
    rmSync(join(root, "cli/CHANGELOG.md"));
    const done = release(root, "check", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain("cli/CHANGELOG.md is missing");
  });

  it("refuses a manifest whose version is not the changelog's newest section", () => {
    const root = fixture();
    write(root, "sdk/typescript/CHANGELOG.md", released("0.1.0"));
    at(root, "0.2.0");
    const done = release(root, "check", "sdk-typescript");
    expect(done.status).toBe(1);
    expect(done.err).toContain("the newest section is not 0.2.0");
  });

  it("refuses a manifest with no released section at all", () => {
    const root = fixture();
    write(root, "sdk/typescript/CHANGELOG.md", "# Changelog\n\n## [Unreleased]\n");
    at(root, "0.2.0");
    const done = release(root, "check", "sdk-typescript");
    expect(done.status).toBe(1);
    expect(done.err).toContain("the newest section is not 0.2.0");
  });

  it("refuses an empty section of the version", () => {
    const root = fixture();
    write(
      root,
      "sdk/typescript/CHANGELOG.md",
      "# Changelog\n\n## [Unreleased]\n\n## [0.2.0] - 2026-01-01\n",
    );
    at(root, "0.2.0");
    const done = release(root, "check", "sdk-typescript");
    expect(done.status).toBe(1);
    expect(done.err).toContain("the section of 0.2.0 is empty");
  });

  it("knows the lines and refuses others", () => {
    const done = release(fixture(), "check", "nothing");
    expect(done.status).toBe(1);
    expect(done.err).toContain(`the lines are ${Object.keys(LINES).join(", ")}`);
    expect(Object.keys(LINES)).toEqual([
      "sdk-typescript",
      "wire",
      "host-rust",
      "view",
      "sandbox",
      "page",
      "program",
      "cli",
    ]);
  });
});

describe("release.mjs notes", () => {
  it("prints the changelog section of the version the manifest is at", () => {
    const root = fixture();
    expect(release(root, "prepare", "sdk-typescript", "0.2.0").status).toBe(0);
    const done = release(root, "notes", "sdk-typescript");
    expect(done.status).toBe(0);
    expect(done.out.trim()).toBe("### Added\n\n- a change of the SDK");
  });

  it("prints the section of a crate and of the binary the same way", () => {
    const root = fixture();
    for (const id of ["wire", "page", "cli"]) {
      expect(release(root, "prepare", id, "0.1.1").status, id).toBe(0);
      const done = release(root, "notes", id);
      expect(done.status, id).toBe(0);
      expect(done.out.trim(), id).toMatch(/^### Added\n\n- a change of /);
    }
  });
});

describe("release.mjs verify-tag", () => {
  it.each(Object.keys(LINES))("reads the tag of the line %s and refuses another version", (id) => {
    const done = release(fixture(), "verify-tag", `${id}-v9.9.9`);
    expect(done.status).toBe(1);
    expect(done.err).toContain(`${id}-v9.9.9 names 9.9.9 but ${LINES[id].dir} is at 0.1.0`);
  });

  it("refuses a tag of no line, and a name that is not a tag", () => {
    const root = fixture();
    expect(release(root, "verify-tag", "nothing-v0.1.0").err).toContain('unknown line "nothing"');
    expect(release(root, "verify-tag", "wire-0.1.0").err).toContain("is not a release tag");
  });
});

describe("release.mjs by the kind of the line", () => {
  it("leaves a crate to the maintainer's machine: no push of the workflow's kind, no approval, no registry", () => {
    const root = fixture();
    expect(release(root, "push", "wire").err).toContain("make publish-local LINE=wire");
    expect(release(root, "approve", "wire").err).toContain("crates.io publishes at once");
    expect(release(root, "registry", "wire", "0.1.0").err).toContain(
      "not published by the workflow",
    );
  });

  it("does not publish a binary like a package, nor approve or look it up", () => {
    const root = fixture();
    expect(release(root, "local", "cli").err).toContain("make publish LINE=cli");
    expect(release(root, "approve", "cli").err).toContain("no registry to approve in");
    expect(release(root, "registry", "cli", "0.1.0").err).toContain(
      "not published by the workflow",
    );
  });

  it("pushes a binary only when it is tagged", () => {
    const done = release(fixture(), "push", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain("no tag cli-v0.1.0: run make release LINE=cli first");
  });
});

// ---- the binary's files ------------------------------------------------------

/** The first bytes of an executable of a system and a processor, as the loader reads them. */
const executable = (system, processor) => {
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

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const systemOf = (target) => (target.includes("apple") ? "macos" : "linux");
const processorOf = (target) => (target.startsWith("aarch64") ? "aarch64" : "x86_64");
const nameOf = (target, version = "0.1.0") => `pencere-${version}-${target}`;

/** Writes dist/ the way `make dist` leaves it for a version: the binaries and their sums. */
function dist(root, version = "0.1.0") {
  mkdirSync(join(root, "dist"), { recursive: true });
  const sums = [];
  for (const target of LINES.cli.targets) {
    const bytes = executable(systemOf(target), processorOf(target));
    writeFileSync(join(root, "dist", nameOf(target, version)), bytes);
    sums.push(`${sha256(bytes)}  ${nameOf(target, version)}`);
  }
  write(root, "dist/SHA256SUMS", `${sums.join("\n")}\n`);
  return sums;
}

describe("release.mjs dist", () => {
  it("accepts the files `make dist` leaves for the version, and names them", () => {
    const root = fixture();
    dist(root);
    const done = release(root, "dist", "cli");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(done.out.trim().split("\n")).toEqual([
      ...LINES.cli.targets.map((target) => `dist/${nameOf(target)}`),
      "dist/SHA256SUMS",
    ]);
  });

  it("refuses a dist/ that is not there", () => {
    const done = release(fixture(), "dist", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain("dist/ is missing");
  });

  it.each(LINES.cli.targets)("refuses a dist/ without the binary for %s", (target) => {
    const root = fixture();
    dist(root);
    rmSync(join(root, "dist", nameOf(target)));
    const done = release(root, "dist", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain(`dist/${nameOf(target)} is missing`);
  });

  it("refuses a dist/ built for another version than the manifest's", () => {
    const root = fixture();
    dist(root, "0.0.9");
    const done = release(root, "dist", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain("dist/pencere-0.1.0-aarch64-apple-darwin is missing");
  });

  it("refuses sums of another version, though the binaries are this one's", () => {
    const root = fixture();
    dist(root);
    write(root, "dist/SHA256SUMS", `${dist(scratch(), "0.0.9").join("\n")}\n`);
    const done = release(root, "dist", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain("dist/SHA256SUMS names pencere-0.0.9-");
    expect(done.err).toContain("is it the sums of another version?");
  });

  it("refuses sums that leave a binary out, and sums that are not there", () => {
    const root = fixture();
    const sums = dist(root);
    write(root, "dist/SHA256SUMS", `${sums.slice(1).join("\n")}\n`);
    expect(release(root, "dist", "cli").err).toContain(
      `dist/SHA256SUMS does not name ${nameOf(LINES.cli.targets[0])}`,
    );
    rmSync(join(root, "dist/SHA256SUMS"));
    expect(release(root, "dist", "cli").err).toContain("dist/SHA256SUMS is missing");
  });

  it("refuses sums that are not those of the files", () => {
    const root = fixture();
    dist(root);
    const target = LINES.cli.targets[1];
    const changed = executable(systemOf(target), processorOf(target));
    changed[40] = 1;
    writeFileSync(join(root, "dist", nameOf(target)), changed);
    const done = release(root, "dist", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain(
      `dist/${nameOf(target)} is not the file that dist/SHA256SUMS has the sum of`,
    );
  });

  it("refuses a line of the sums that is not a sum, and a name twice", () => {
    const root = fixture();
    const sums = dist(root);
    write(root, "dist/SHA256SUMS", `${sums.join("\n")}\nnot a sum\n`);
    expect(release(root, "dist", "cli").err).toContain('"not a sum" is not "<sha256>  <file>"');
    write(root, "dist/SHA256SUMS", `${sums.join("\n")}\n${sums[0]}\n`);
    expect(release(root, "dist", "cli").err).toContain(
      `dist/SHA256SUMS names ${nameOf(LINES.cli.targets[0])} twice`,
    );
  });

  it("refuses an empty file, and a file that is not an executable", () => {
    const root = fixture();
    dist(root);
    writeFileSync(join(root, "dist", nameOf(LINES.cli.targets[3])), "");
    expect(release(root, "dist", "cli").err).toContain(
      `dist/${nameOf(LINES.cli.targets[3])} is empty`,
    );
    writeFileSync(join(root, "dist", nameOf(LINES.cli.targets[3])), "#!/bin/sh\n");
    expect(release(root, "dist", "cli").err).toContain(
      `dist/${nameOf(LINES.cli.targets[3])} is not an executable`,
    );
  });

  it.each([
    ["x86_64-unknown-linux-musl", "linux", "aarch64"],
    ["aarch64-unknown-linux-musl", "macos", "aarch64"],
    ["aarch64-apple-darwin", "macos", "x86_64"],
    ["x86_64-apple-darwin", "linux", "x86_64"],
  ])("refuses the file for %s when it is a %s executable for %s", (target, system, processor) => {
    const root = fixture();
    dist(root);
    writeFileSync(join(root, "dist", nameOf(target)), executable(system, processor));
    const done = release(root, "dist", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain(
      `dist/${nameOf(target)} is a ${system} executable for ${processor}, not for ${target}`,
    );
  });

  it("is a command of a binary line only", () => {
    const done = release(fixture(), "dist", "wire");
    expect(done.status).toBe(1);
    expect(done.err).toContain("wire has no dist/");
  });
});

// ---- check, tag, push and local against stand-ins of the world ---------------

/** What git is told in every run, so that no user's configuration (identity, signing) is read. */
const GIT_ENV = {
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

const git = (cwd, ...args) =>
  execFileSync("git", args, {
    cwd,
    encoding: "utf8",
    env: { ...process.env, ...GIT_ENV },
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();

/**
 * The repository and its `origin` that every `world()` starts from, made once: every line at
 * `0.1.0` and released in its changelog, the licence files in place, committed and pushed.
 * Stand-in `cargo` and `gh` stay beside them: they answer from a state file and write what they
 * were asked to a log, and tell nothing else about the machine they run on.
 */
let template;
function worldTemplate() {
  if (template !== undefined) return template;
  const root = fixture();
  roots.splice(roots.indexOf(root), 1);
  const origin = mkdtempSync(join(tmpdir(), "pencere-release-test-"));
  const bin = mkdtempSync(join(tmpdir(), "pencere-release-test-"));
  template = { root, origin, bin };
  write(root, ".gitignore", "dist/\ntarget/\n");
  write(root, "LICENSE", "Apache License\n");
  write(root, "NOTICE", "pencere\n");
  for (const line of Object.values(LINES)) {
    write(root, `${line.dir}/LICENSE`, "Apache License\n");
    write(root, `${line.dir}/NOTICE`, "pencere\n");
    write(root, `${line.dir}/README.md`, "# readme\n");
    write(root, `${line.dir}/CHANGELOG.md`, RELEASED);
  }
  for (const entry of LINES.cli.entries) write(root, `cli/${entry}`, "fn main() {}\n");
  install(root);
  git(root, "init", "-q", "-b", "main");
  git(root, "add", "-A");
  git(root, "commit", "-q", "-m", "fixture");
  git(root, "init", "-q", "--bare", origin);
  git(root, "remote", "add", "origin", origin);
  git(root, "push", "-q", "origin", "main");

  // cargo answers `metadata` (the path dependencies in `dependencies`), `package --list` (the
  // `files`) and `info` (found when the spec is in `registry`), and writes everything down.
  write(
    bin,
    "cargo",
    `#!/usr/bin/env node
const fs = require("node:fs");
const args = process.argv.slice(2);
fs.appendFileSync(process.env.FAKE_LOG, JSON.stringify({ tool: "cargo", args, cwd: process.cwd() }) + "\\n");
const state = JSON.parse(fs.readFileSync(process.env.FAKE_STATE, "utf8"));
if (args[0] === "metadata") {
  console.log(JSON.stringify({ packages: [{ name: state.name, dependencies: state.dependencies, ...state.manifest }] }));
} else if (args[0] === "package" && args.includes("--list")) {
  console.log(state.files.join("\\n"));
} else if (args[0] === "info") {
  if (state.registry.includes(args[1])) console.log(args[1]);
  else {
    console.error("error: could not find \`" + args[1] + "\` in registry \`crates-io\`");
    process.exit(101);
  }
}
`,
  );
  // gh is logged in, knows no release unless told, and makes one unless it is told it cannot.
  write(
    bin,
    "gh",
    `#!/usr/bin/env node
const fs = require("node:fs");
const args = process.argv.slice(2);
fs.appendFileSync(process.env.FAKE_LOG, JSON.stringify({ tool: "gh", args }) + "\\n");
if (args[0] === "release" && args[1] === "view" && !process.env.FAKE_GH_HAS_RELEASE) process.exit(1);
if (args[0] === "release" && args[1] === "create" && process.env.FAKE_GH_CANNOT_CREATE) process.exit(1);
`,
  );
  chmodSync(join(bin, "cargo"), 0o755);
  chmodSync(join(bin, "gh"), 0o755);
  return template;
}
afterAll(() => {
  for (const dir of template === undefined ? [] : Object.values(template)) {
    rmSync(dir, { recursive: true, force: true });
  }
});

/**
 * A copy of the template: a git repository on `main` with its own `origin`, to commit to and
 * release from, and the stand-ins (`crate` says what `cargo` answers for the line under test).
 */
function world() {
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
  const state = { name: "", dependencies: [], files: [], registry: [], manifest };
  const runWith = (env, ...args) => {
    write(aux, "state.json", state);
    return releaseWith(
      root,
      {
        ...GIT_ENV,
        PATH: `${bin}:${process.env.PATH}`,
        FAKE_LOG: join(aux, "log"),
        FAKE_STATE: join(aux, "state.json"),
        ...env,
      },
      ...args,
    );
  };
  const calls = () =>
    readFileSync(join(aux, "log"), "utf8")
      .split("\n")
      .filter((line) => line !== "")
      .map((line) => JSON.parse(line));
  /** A crate line as cargo would package it: the files it must have, and the siblings it asks for. */
  const crate = (id, { dependencies = {}, extra = [] } = {}) => {
    const line = LINES[id];
    state.name = line.name;
    state.files = [
      "LICENSE",
      "NOTICE",
      "README.md",
      "CHANGELOG.md",
      "Cargo.toml",
      ...line.entries,
      ...extra,
    ];
    state.dependencies = Object.entries(dependencies).map(([name, req]) => ({
      name,
      req,
      path: `/somewhere/${name}`,
      kind: null,
    }));
  };
  return {
    root,
    origin,
    git: (...args) => git(root, ...args),
    state,
    crate,
    calls,
    run: (...args) => runWith({}, ...args),
    runWith,
    forget: () => write(aux, "log", ""),
  };
}

const cargoCalls = (calls) =>
  calls.filter((call) => call.tool === "cargo").map((call) => call.args.join(" "));

describe("release.mjs check of a crate", () => {
  it("dry-runs the publication of a crate that asks for no crate of this repository", () => {
    const w = world();
    w.crate("wire");
    const done = w.run("check", "wire");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(cargoCalls(w.calls())).toEqual([
      "package --list -p pencere-wire",
      "metadata --no-deps --format-version 1",
      "publish --dry-run -p pencere-wire",
    ]);
    expect(done.out).toContain("pencere-wire 0.1.0 is ready to tag as wire-v0.1.0");
  });

  it("dry-runs the publication once the crates it asks for are on crates.io", () => {
    const w = world();
    w.crate("program", { dependencies: { "pencere-wire": "^0.1.0" } });
    w.state.registry = ["pencere-wire@0.1.0"];
    const done = w.run("check", "program");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(cargoCalls(w.calls())).toContain("publish --dry-run -p pencere");
    expect(done.out).not.toContain("not on crates.io yet");
  });

  it("does not dry-run a crate, and says so, while a crate it asks for is not on crates.io", () => {
    const w = world();
    w.crate("program", { dependencies: { "pencere-wire": "^0.1.0" } });
    const done = w.run("check", "program");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    // The files were listed and the manifest read; cargo cannot package it without the crate.
    expect(cargoCalls(w.calls())).toEqual([
      "package --list -p pencere",
      "metadata --no-deps --format-version 1",
      "info pencere-wire@0.1.0",
    ]);
    expect(done.out).toContain("pencere-wire 0.1.0 is not on crates.io yet");
    expect(done.out).toContain("pencere was not dry-run");
    expect(done.out).toContain("its files and its manifest are checked, and nothing else");
  });

  it("asks crates.io from outside the repository, where cargo does not answer from the workspace", () => {
    const w = world();
    w.crate("program", { dependencies: { "pencere-wire": "^0.1.0" } });
    w.run("check", "program");
    const asks = w.calls().filter((call) => call.args[0] === "info");
    expect(asks.map((call) => call.args[1])).toEqual(["pencere-wire@0.1.0"]);
    expect(realpathSync(asks[0].cwd)).toBe(realpathSync(tmpdir()));
    expect(realpathSync(asks[0].cwd).startsWith(realpathSync(w.root))).toBe(false);
  });

  it("runs the cargo commands of the page SDK against the page modules' workspace", () => {
    const w = world();
    w.crate("page", { dependencies: { "pencere-view": "^0.1.0", "pencere-wire": "^0.1.0" } });
    w.state.registry = ["pencere-view@0.1.0", "pencere-wire@0.1.0"];
    const done = w.run("check", "page");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    const asked = cargoCalls(w.calls()).filter((call) => !call.startsWith("info"));
    expect(asked).toEqual([
      "package --list -p pencere-page --manifest-path kit/guest/Cargo.toml",
      "metadata --no-deps --format-version 1 --manifest-path kit/guest/Cargo.toml",
      "publish --dry-run -p pencere-page --manifest-path kit/guest/Cargo.toml",
    ]);
  });

  it("lists each crate it asks for that is not on crates.io, and only those", () => {
    const w = world();
    w.crate("page", { dependencies: { "pencere-view": "^0.1.0", "pencere-wire": "^0.1.0" } });
    w.state.registry = ["pencere-wire@0.1.0"];
    const done = w.run("check", "page");
    expect(done.status).toBe(0);
    expect(done.out).toContain("pencere-view 0.1.0 is not on crates.io yet");
    expect(done.out).not.toContain("pencere-wire 0.1.0 is not");
  });

  it("refuses a crate that asks by path for a crate that no line publishes", () => {
    const w = world();
    w.crate("program", { dependencies: { "containers-page": "^0.1.0" } });
    const done = w.run("check", "program");
    expect(done.status).toBe(1);
    expect(done.err).toContain("pencere depends on containers-page by path, and no line");
  });

  it("refuses a path dependency that asks for no version", () => {
    const w = world();
    w.crate("program", { dependencies: { "pencere-wire": "*" } });
    const done = w.run("check", "program");
    expect(done.status).toBe(1);
    expect(done.err).toContain('asks for pencere-wire by path with the version "*"');
  });

  it.each([
    [{ description: "" }, "description"],
    [{ license: null }, "license"],
    [{ repository: null }, "repository"],
    [{ readme: null }, "readme"],
    [{ keywords: [] }, "keywords (one to five)"],
    [{ keywords: ["a", "b", "c", "d", "e", "f"] }, "keywords (one to five)"],
    [{ categories: [] }, "categories"],
    [{ publish: [] }, "publish (it is false)"],
  ])(
    "refuses a crate whose manifest says %j, whether or not the crates below it are out",
    (gap, what) => {
      for (const registry of [[], ["pencere-wire@0.1.0"]]) {
        const w = world();
        w.crate("program", { dependencies: { "pencere-wire": "^0.1.0" } });
        Object.assign(w.state.manifest, gap);
        w.state.registry = registry;
        const done = w.run("check", "program");
        expect(done.status).toBe(1);
        expect(done.err).toContain(`program/Cargo.toml lacks: ${what}`);
        expect(cargoCalls(w.calls()).some((call) => call.startsWith("publish"))).toBe(false);
      }
    },
  );

  it("refuses a package that lacks a file of the line", () => {
    const w = world();
    w.crate("sandbox");
    w.state.files = w.state.files.filter((file) => file !== "wit/view.wit");
    const done = w.run("check", "sandbox");
    expect(done.status).toBe(1);
    expect(done.err).toContain("the package lacks wit/view.wit");
  });

  it.each([
    "tests/it.rs",
    "src/images_tests.rs",
    "src/tests.rs",
    "target/x",
    "node_modules/x/y.js",
  ])("refuses a package that carries %s", (stray) => {
    const w = world();
    w.crate("sandbox", { extra: [stray] });
    const done = w.run("check", "sandbox");
    expect(done.status).toBe(1);
    expect(done.err).toContain("the package carries tests or builds");
    expect(done.err).toContain(stray);
  });

  it("refuses a crate whose licence is not the repository's, or is not there", () => {
    const w = world();
    w.crate("wire");
    writeFileSync(join(w.root, "wire/LICENSE"), "another\n");
    w.git("commit", "-q", "-am", "another licence");
    expect(w.run("check", "wire").err).toContain("wire/LICENSE is not the repository's LICENSE");
    w.git("rm", "-q", "wire/NOTICE");
    w.git("commit", "-q", "-m", "no notice");
    writeFileSync(join(w.root, "wire/LICENSE"), "Apache License\n");
    w.git("commit", "-q", "-am", "the licence back");
    expect(w.run("check", "wire").err).toContain("wire/NOTICE is not the repository's NOTICE");
  });
});

describe("release.mjs tag and local of a crate", () => {
  it("makes the tag with the changelog section as its message, and says to publish from here", () => {
    const w = world();
    w.crate("wire");
    const done = w.run("tag", "wire");
    expect(done.status).toBe(0);
    expect(done.out).toContain(
      "tagged wire-v0.1.0; nothing is pushed. Next: make publish-local LINE=wire",
    );
    expect(w.git("tag", "--list", "wire-v*")).toBe("wire-v0.1.0");
    expect(w.git("tag", "--list", "--format=%(contents)", "wire-v0.1.0")).toContain("- a change");
  });

  it("publishes a crate with cargo and pushes main and the tag", () => {
    const w = world();
    w.crate("wire");
    expect(w.run("tag", "wire").status).toBe(0);
    w.forget();
    const done = w.run("local", "wire");
    expect(done.status).toBe(0);
    expect(cargoCalls(w.calls())).toContain("publish -p pencere-wire");
    expect(w.git("ls-remote", "--tags", "origin")).toContain("refs/tags/wire-v0.1.0");
  });

  it("publishes the page SDK from the page modules' workspace", () => {
    const w = world();
    w.crate("page", { dependencies: { "pencere-view": "^0.1.0" } });
    w.state.registry = ["pencere-view@0.1.0"];
    expect(w.run("tag", "page").status).toBe(0);
    w.forget();
    expect(w.run("local", "page").status).toBe(0);
    expect(cargoCalls(w.calls())).toContain(
      "publish -p pencere-page --manifest-path kit/guest/Cargo.toml",
    );
  });

  it("publishes nothing, and pushes nothing, while a crate it asks for is not on crates.io", () => {
    const w = world();
    w.crate("program", { dependencies: { "pencere-wire": "^0.1.0" } });
    expect(w.run("tag", "program").status).toBe(0);
    w.forget();
    const done = w.run("local", "program");
    expect(done.status).toBe(1);
    expect(done.err).toContain(
      "pencere depends on pencere-wire 0.1.0, which crates.io does not have yet",
    );
    expect(cargoCalls(w.calls()).some((call) => call.startsWith("publish"))).toBe(false);
    expect(w.git("ls-remote", "--tags", "origin")).toBe("");
  });
});

describe("release.mjs for the binary line", () => {
  it("checks the directory's files and the binaries, and makes the tag", () => {
    const w = world();
    dist(w.root);
    const done = w.run("tag", "cli");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(done.out).toContain("dist/ holds the 4 binaries of 0.1.0 and their sums");
    expect(done.out).toContain("tagged cli-v0.1.0; nothing is pushed. Next: make publish LINE=cli");
    expect(w.git("tag", "--list", "cli-v*")).toBe("cli-v0.1.0");
    // A binary has no package: nothing of cargo.
    expect(cargoCalls(w.calls())).toEqual([]);
  });

  it("refuses to tag without its dist/", () => {
    const w = world();
    const done = w.run("tag", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain("dist/ is missing");
    expect(w.git("tag", "--list")).toBe("");
  });

  it("refuses to tag when the directory lacks a file of the line or its licence is not the repository's", () => {
    const w = world();
    dist(w.root);
    w.git("rm", "-q", "cli/README.md");
    w.git("commit", "-q", "-m", "no readme");
    expect(w.run("check", "cli").err).toContain("cli lacks README.md");
    w.git("rm", "-q", "cli/src/main.rs");
    w.git("commit", "-q", "-m", "no entry");
    expect(w.run("check", "cli").err).toContain("cli lacks");
    writeFileSync(join(w.root, "cli/NOTICE"), "another\n");
    w.git("commit", "-q", "-am", "another notice");
    expect(w.run("check", "cli").err).toContain("cli/NOTICE is not the repository's NOTICE");
  });

  it("does not hold the tests of the binary's crate against it", () => {
    const w = world();
    dist(w.root);
    write(w.root, "cli/tests/run.rs", "");
    w.git("add", "-A");
    w.git("commit", "-q", "-m", "tests");
    expect(w.run("check", "cli").status).toBe(0);
  });

  it("pushes main and the tag, then makes the GitHub release with the changelog and the five files", () => {
    const w = world();
    dist(w.root);
    expect(w.run("tag", "cli").status).toBe(0);
    const done = w.run("push", "cli");
    expect(done.status).toBe(0);
    expect(w.git("ls-remote", "--tags", "origin")).toContain("refs/tags/cli-v0.1.0");

    const made = w.calls().find((call) => call.tool === "gh" && call.args[1] === "create");
    const files = made.args.slice(7).map((file) => basename(file));
    expect(made.args.slice(0, 7)).toEqual([
      "release",
      "create",
      "cli-v0.1.0",
      "--verify-tag",
      "--title",
      "cli-v0.1.0",
      "--notes=### Added\n\n- a change",
    ]);
    expect(files).toEqual([...LINES.cli.targets.map((target) => nameOf(target)), "SHA256SUMS"]);
    // gh is asked whether it can, and whether the release is there, before the release is made.
    const asked = w.calls().map((call) => `${call.tool} ${call.args.slice(0, 2).join(" ")}`);
    expect(asked.indexOf("gh auth status")).toBeLessThan(asked.indexOf("gh release create"));
    expect(asked.indexOf("gh release view")).toBeLessThan(asked.indexOf("gh release create"));
  });

  it("pushes nothing when the files of dist/ are not the version's any more", () => {
    const w = world();
    dist(w.root);
    expect(w.run("tag", "cli").status).toBe(0);
    rmSync(join(w.root, "dist", nameOf(LINES.cli.targets[2])));
    const done = w.run("push", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain("is missing");
    expect(w.git("ls-remote", "--tags", "origin")).toBe("");
    expect(w.calls().some((call) => call.args[1] === "create")).toBe(false);
  });

  it("pushes nothing when the GitHub release is already there", () => {
    const w = world();
    dist(w.root);
    expect(w.run("tag", "cli").status).toBe(0);
    const done = w.runWith({ FAKE_GH_HAS_RELEASE: "1" }, "push", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain("the GitHub release cli-v0.1.0 exists");
    expect(w.git("ls-remote", "--tags", "origin")).toBe("");
  });

  it("says what to run when the push went and the release could not be made, and makes it the next time", () => {
    const w = world();
    dist(w.root);
    expect(w.run("tag", "cli").status).toBe(0);
    const failed = w.runWith({ FAKE_GH_CANNOT_CREATE: "1" }, "push", "cli");
    expect(failed.status).toBe(1);
    expect(failed.err).toContain(
      "pushed main and cli-v0.1.0, but the GitHub release was not made: run make publish LINE=cli again",
    );
    expect(w.git("ls-remote", "--tags", "origin")).toContain("refs/tags/cli-v0.1.0");
    const next = w.run("push", "cli");
    expect(next.status).toBe(0);
    expect(next.out).toContain("made its GitHub release with 5 files");
  });
});
