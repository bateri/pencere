// The release tooling's rules that no other check sees: `release.mjs prepare` moves a line's
// version, in its manifest and in the lockfile, and dates its changelog; `release.mjs check`
// refuses a release whose version and changelog disagree, before it looks at git or the registry.

import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vitest";

import { LINES } from "./lines.mjs";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const json = (root, ...path) => JSON.parse(readFileSync(join(root, ...path), "utf8"));
const text = (root, ...path) => readFileSync(join(root, ...path), "utf8");
const npmLines = Object.values(LINES).filter((line) => line.kind === "npm");

describe("the repository's lines", () => {
  it("are the packages the lines say they are", () => {
    for (const line of npmLines) {
      expect(json(repo, line.dir, "package.json").name).toBe(line.name);
    }
    expect(text(repo, LINES["host-rust"].dir, "Cargo.toml")).toMatch(
      /^\[package\]\nname = "pencere-host"$/m,
    );
  });

  it("have the lockfile record each npm package at the version of its manifest", () => {
    const lock = json(repo, "package-lock.json");
    for (const line of npmLines) {
      const manifest = json(repo, line.dir, "package.json");
      expect(lock.packages[line.dir]?.version, line.name).toBe(manifest.version);
    }
  });
});

const roots = [];
afterEach(() => {
  while (roots.length > 0) rmSync(roots.pop(), { recursive: true, force: true });
});

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

/** A copy of the repository's shape, with both lines at `0.1.0` and something unreleased in each. */
function fixture() {
  const root = mkdtempSync(join(tmpdir(), "pencere-release-test-"));
  roots.push(root);
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
    `[workspace]\nresolver = "3"\nmembers = ["host/rust"]\n\n[workspace.package]\nversion = "0.0.0"\nedition = "2024"\n`,
  );
  write(
    root,
    "host/rust/Cargo.toml",
    `[package]\nname = "pencere"\nversion = "0.1.0"\nedition.workspace = true\n`,
  );
  write(root, "host/rust/src/lib.rs", "");
  write(root, "host/rust/CHANGELOG.md", CHANGELOG("a change of the crate"));
  return root;
}

/** `tools/release.mjs` of the fixture, which is the script of this repository run on its files. */
function release(root, ...args) {
  mkdirSync(join(root, "tools"), { recursive: true });
  cpSync(join(repo, "tools/release.mjs"), join(root, "tools/release.mjs"));
  cpSync(join(repo, "tools/lines.mjs"), join(root, "tools/lines.mjs"));
  const done = spawnSync("node", [join(root, "tools/release.mjs"), ...args], { encoding: "utf8" });
  return { status: done.status, out: done.stdout, err: done.stderr };
}

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
    expect(text(root, "host/rust/Cargo.toml")).toContain('version = "0.2.0"');
    expect(text(root, "host/rust/Cargo.toml")).not.toContain('version = "0.1.0"');
    expect(text(root, "Cargo.toml")).toContain('version = "0.0.0"');
    expect(text(root, "host/rust/CHANGELOG.md")).toMatch(/## \[0\.2\.0\] - \d{4}-\d{2}-\d{2}/);
    expect(json(root, "sdk/typescript", "package.json").version).toBe("0.1.0");
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
    expect(done.err).toContain("the lines are sdk-typescript, host-rust");
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
});
