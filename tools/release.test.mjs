// The release tooling's one rule that no other check sees: a package that names another
// package of this repository names exactly `^<that package's version>`, in its manifest and
// in the lockfile, and `release.mjs prepare` keeps it so. When it does not hold, `npm ci`
// looks for the version on the registry instead of linking the workspace's, and fails.

import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vitest";

import { LINES, moveRanges, needs, rangeFor, ranges, strayRanges } from "./lines.mjs";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const json = (root, ...path) => JSON.parse(readFileSync(join(root, ...path), "utf8"));
const versionOf = (root, line) => json(root, line.dir, "package.json").version;
const npmLines = Object.values(LINES).filter((line) => line.kind === "npm");

describe("the repository's packages", () => {
  it("name each other by the version each is at", () => {
    for (const line of npmLines) {
      expect(strayRanges(repo, line.name, versionOf(repo, line)), line.name).toEqual([]);
    }
  });

  it("have the same ranges in the lockfile as in their manifests", () => {
    const lock = json(repo, "package-lock.json");
    for (const { dir, field, name, range } of ranges(repo)) {
      // Only the workspace's members are in the lockfile; the templates are not.
      const member = lock.packages[dir];
      if (member !== undefined) expect(member[field]?.[name], `${dir} ${name}`).toBe(range);
    }
  });

  it("are the ones the lines say they are", () => {
    for (const line of npmLines) {
      expect(json(repo, line.dir, "package.json").name).toBe(line.name);
      expect(line.shipped).toContain(line.dir);
    }
  });

  it("need, of the other lines, exactly what their manifests name", () => {
    const sdk = rangeFor(versionOf(repo, LINES["sdk-typescript"]));
    const cli = rangeFor(versionOf(repo, LINES.cli));
    expect(needs(repo, LINES["sdk-typescript"])).toEqual([]);
    expect(needs(repo, LINES.cli)).toEqual([{ name: "@pencere/sdk", range: sdk }]);
    expect(needs(repo, LINES.create)).toEqual([
      { name: "@pencere/sdk", range: sdk },
      { name: "@pencere/cli", range: cli },
    ]);
    expect(needs(repo, LINES["host-rust"])).toEqual([]);
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

/** A copy of the repository's shape, with every package at `0.1.0` and each naming the others at `^0.1.0`. */
function fixture() {
  const root = mkdtempSync(join(tmpdir(), "pencere-release-test-"));
  roots.push(root);
  const manifest = (name, dependencies = {}, devDependencies = {}) => ({
    name,
    version: "0.1.0",
    dependencies,
    devDependencies,
  });
  const lines = {
    "sdk/typescript": manifest("@pencere/sdk", {}, { zod: "^4.6.7" }),
    cli: manifest("@pencere/cli", {
      "@pencere/sdk": "^0.1.0",
      "@modelcontextprotocol/client": "^2.3.1",
    }),
    create: manifest("create-pencere"),
    "create/templates/react": manifest(
      "{{name}}",
      { "@pencere/sdk": "^0.1.0", react: "^19.3.1" },
      {
        "@pencere/cli": "^0.1.0",
      },
    ),
    "create/templates/vue": manifest(
      "{{name}}",
      { "@pencere/sdk": "^0.1.0" },
      { "@pencere/cli": "^0.1.0" },
    ),
  };
  const packages = {};
  for (const [dir, content] of Object.entries(lines)) {
    write(root, `${dir}/package.json`, content);
    if (!dir.includes("templates")) {
      packages[dir] = {
        name: content.name,
        version: content.version,
        dependencies: content.dependencies,
        devDependencies: content.devDependencies,
      };
    }
  }
  write(root, "package-lock.json", { lockfileVersion: 3, packages });
  for (const dir of ["sdk/typescript", "cli", "create"]) {
    write(root, `${dir}/CHANGELOG.md`, CHANGELOG(`a change of ${dir}`));
  }
  return root;
}

describe("moveRanges", () => {
  it("makes every range that names the package say the new version, in manifests and lockfile", () => {
    const root = fixture();
    const written = moveRanges(root, "@pencere/sdk", "0.2.0");
    expect(written.sort()).toEqual([
      "cli/package.json",
      "create/templates/react/package.json",
      "create/templates/vue/package.json",
      "package-lock.json",
    ]);
    expect(json(root, "cli", "package.json").dependencies["@pencere/sdk"]).toBe("^0.2.0");
    expect(json(root, "create/templates/react/package.json").dependencies["@pencere/sdk"]).toBe(
      "^0.2.0",
    );
    expect(json(root, "create/templates/vue/package.json").dependencies["@pencere/sdk"]).toBe(
      "^0.2.0",
    );
    expect(json(root, "package-lock.json").packages.cli.dependencies["@pencere/sdk"]).toBe(
      "^0.2.0",
    );
  });

  it("leaves what does not name the package as it was", () => {
    const root = fixture();
    moveRanges(root, "@pencere/sdk", "0.2.0");
    const cli = json(root, "cli", "package.json");
    expect(cli.dependencies["@modelcontextprotocol/client"]).toBe("^2.3.1");
    expect(json(root, "create/templates/react/package.json").devDependencies["@pencere/cli"]).toBe(
      "^0.1.0",
    );
    expect(json(root, "create/templates/react/package.json").name).toBe("{{name}}");
    // The package's own version is `prepare`'s, not a range's.
    expect(json(root, "sdk/typescript", "package.json").version).toBe("0.1.0");
  });

  it("writes nothing when the ranges already say it", () => {
    const root = fixture();
    expect(moveRanges(root, "@pencere/sdk", "0.1.0")).toEqual([]);
    expect(moveRanges(root, "create-pencere", "0.2.0")).toEqual([]);
  });

  it("is what strayRanges asks for", () => {
    const root = fixture();
    expect(strayRanges(root, "@pencere/sdk", "0.2.0")).toHaveLength(3);
    moveRanges(root, "@pencere/sdk", "0.2.0");
    expect(strayRanges(root, "@pencere/sdk", "0.2.0")).toEqual([]);
  });
});

/** `tools/release.mjs` of the fixture, which is the script of this repository run on its files. */
function release(root, ...args) {
  mkdirSync(join(root, "tools"), { recursive: true });
  cpSync(join(repo, "tools/release.mjs"), join(root, "tools/release.mjs"));
  cpSync(join(repo, "tools/lines.mjs"), join(root, "tools/lines.mjs"));
  const done = spawnSync("node", [join(root, "tools/release.mjs"), ...args], { encoding: "utf8" });
  return { status: done.status, out: done.stdout, err: done.stderr };
}

describe("release.mjs prepare", () => {
  it("moves the version of a line and the ranges that name it, together", () => {
    const root = fixture();
    const done = release(root, "prepare", "sdk-typescript", "0.2.0");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(done.out).toContain("the range ^0.2.0 of @pencere/sdk is now also in: cli/package.json");
    expect(json(root, "sdk/typescript", "package.json").version).toBe("0.2.0");
    expect(json(root, "package-lock.json").packages["sdk/typescript"].version).toBe("0.2.0");
    expect(json(root, "cli", "package.json").dependencies["@pencere/sdk"]).toBe("^0.2.0");
    expect(json(root, "package-lock.json").packages.cli.dependencies["@pencere/sdk"]).toBe(
      "^0.2.0",
    );
    expect(json(root, "create/templates/vue/package.json").dependencies["@pencere/sdk"]).toBe(
      "^0.2.0",
    );
    // The other line's own version is not moved with it.
    expect(json(root, "cli", "package.json").version).toBe("0.1.0");
    expect(readFileSync(join(root, "sdk/typescript/CHANGELOG.md"), "utf8")).toMatch(
      /## \[0\.2\.0\] - \d{4}-\d{2}-\d{2}/,
    );
  });

  it("names the command's own line's package in the templates, one line at a time", () => {
    const root = fixture();
    expect(release(root, "prepare", "cli", "0.2.0").status).toBe(0);
    expect(json(root, "create/templates/react/package.json").devDependencies["@pencere/cli"]).toBe(
      "^0.2.0",
    );
    expect(json(root, "create/templates/vue/package.json").devDependencies["@pencere/cli"]).toBe(
      "^0.2.0",
    );
    // The SDK's ranges are the SDK line's to move.
    expect(json(root, "cli", "package.json").dependencies["@pencere/sdk"]).toBe("^0.1.0");
    expect(json(root, "package-lock.json").packages.cli.version).toBe("0.2.0");
  });

  it("moves nothing for a line that no other package names", () => {
    const root = fixture();
    const done = release(root, "prepare", "create", "0.2.0");
    expect(done.status).toBe(0);
    expect(done.out).not.toContain("is now also in");
    expect(json(root, "cli", "package.json").dependencies["@pencere/sdk"]).toBe("^0.1.0");
  });

  it("refuses a version that is not after the line's, and changes nothing", () => {
    const root = fixture();
    const done = release(root, "prepare", "sdk-typescript", "0.1.0");
    expect(done.status).toBe(1);
    expect(done.err).toContain("0.1.0 is not after 0.1.0");
    expect(json(root, "cli", "package.json").dependencies["@pencere/sdk"]).toBe("^0.1.0");
  });
});

describe("release.mjs check", () => {
  it("refuses a package whose range names a version its line is not at", () => {
    const root = fixture();
    // A version moved by hand, which `prepare` would have moved the ranges with.
    write(root, "sdk/typescript/package.json", {
      ...json(root, "sdk/typescript", "package.json"),
      version: "0.2.0",
    });
    write(
      root,
      "sdk/typescript/CHANGELOG.md",
      `# Changelog\n\n## [Unreleased]\n\n## [0.2.0] - 2026-01-01\n\n### Added\n\n- a change\n`,
    );
    const done = release(root, "check", "sdk-typescript");
    expect(done.status).toBe(1);
    expect(done.err).toContain(
      "cli/package.json names @pencere/sdk as ^0.1.0 and the line is at 0.2.0",
    );
  });

  it("refuses a line that was never prepared", () => {
    const root = fixture();
    write(root, "cli/package.json", { ...json(root, "cli", "package.json"), version: "0.0.0" });
    const done = release(root, "check", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain("@pencere/cli is at 0.0.0");
  });

  it("knows the lines and refuses others", () => {
    const done = release(fixture(), "check", "nothing");
    expect(done.status).toBe(1);
    expect(done.err).toContain("the lines are sdk-typescript, cli, create, host-rust");
  });
});
