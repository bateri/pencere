// The lines this repository releases, and the versions they name of each other.
//
// A package of a line may depend on another line's package: `@pencere/cli` runs the SDK, and
// the templates `create-pencere` writes depend on both. What a published package names is a
// range, and the ranges are kept equal to `^<the version of the line they name>`. That is not
// a taste: npm links a workspace package into another only when the other's range admits its
// version, so a range left behind when a version moves sends `npm ci` to the registry for a
// version that is not there, and every commit after it fails to install. `release.mjs prepare`
// moves the ranges together with the version; `release.mjs check` and the tests refuse any
// that do not agree.

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

/**
 * The lines. `dir` holds the package, `entries` are the files a package of the line is
 * useless without, and `shipped` are the directories of the `package.json` files that go out
 * with it (the templates are manifests `create-pencere` writes into other people's projects).
 */
export const LINES = {
  "sdk-typescript": {
    dir: "sdk/typescript",
    kind: "npm",
    name: "@pencere/sdk",
    entries: ["dist/index.js", "dist/index.d.ts"],
    shipped: ["sdk/typescript"],
  },
  cli: {
    dir: "cli",
    kind: "npm",
    name: "@pencere/cli",
    entries: ["dist/main.js"],
    shipped: ["cli"],
  },
  create: {
    dir: "create",
    kind: "npm",
    name: "create-pencere",
    // `templates/` is what a bad `files` list would leave out, and `_gitignore` what npm itself
    // would, when it is called `.gitignore`.
    entries: [
      "dist/main.js",
      "templates/common/_gitignore",
      "templates/common/src/server/main.ts",
      "templates/react/package.json",
      "templates/vue/package.json",
    ],
    shipped: ["create", "create/templates/react", "create/templates/vue"],
  },
  "host-rust": {
    dir: "host/rust",
    kind: "cargo",
    name: "pencere",
    entries: ["src/lib.rs"],
    shipped: [],
  },
};

const DEPENDENCY_FIELDS = [
  "dependencies",
  "devDependencies",
  "peerDependencies",
  "optionalDependencies",
];

/** The range a package names of a line at `version`. */
export const rangeFor = (version) => `^${version}`;

const npmNames = () =>
  new Set(
    Object.values(LINES)
      .filter((line) => line.kind === "npm")
      .map((line) => line.name),
  );

const shippedDirs = () => Object.values(LINES).flatMap((line) => line.shipped);

const manifestPath = (root, dir) => join(root, dir, "package.json");

const readJson = (path) => JSON.parse(readFileSync(path, "utf8"));
const writeJson = (path, value) => writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);

/** Every range a shipped manifest gives to a package of a line: where it is and what it says. */
export function ranges(root) {
  const names = npmNames();
  const found = [];
  for (const dir of shippedDirs()) {
    const manifest = readJson(manifestPath(root, dir));
    for (const field of DEPENDENCY_FIELDS) {
      for (const [name, range] of Object.entries(manifest[field] ?? {})) {
        if (names.has(name)) found.push({ dir, field, name, range });
      }
    }
  }
  return found;
}

/** The ranges that name `name` and are not the one for `version`. */
export function strayRanges(root, name, version) {
  return ranges(root).filter((r) => r.name === name && r.range !== rangeFor(version));
}

/** What the line's packages need of other lines: each package and range once. */
export function needs(root, line) {
  const wanted = ranges(root).filter((r) => line.shipped.includes(r.dir) && r.name !== line.name);
  const unique = new Map(
    wanted.map((r) => [`${r.name}@${r.range}`, { name: r.name, range: r.range }]),
  );
  return [...unique.values()];
}

/**
 * Makes every range that names `name` say `^version`: in the manifests, and in the
 * lockfile's copy of the manifests of its workspace members. Returns the files it wrote,
 * relative to `root`.
 */
export function moveRanges(root, name, version) {
  const written = [];
  const lockPath = join(root, "package-lock.json");
  const lock = readJson(lockPath);
  let lockMoved = false;
  for (const dir of shippedDirs()) {
    const path = manifestPath(root, dir);
    const manifest = readJson(path);
    let moved = false;
    for (const field of DEPENDENCY_FIELDS) {
      if (manifest[field]?.[name] === undefined || manifest[field][name] === rangeFor(version)) {
        continue;
      }
      manifest[field][name] = rangeFor(version);
      moved = true;
      // Only the workspace's members are in the lockfile; the templates are not.
      const member = lock.packages?.[dir];
      if (member?.[field]?.[name] !== undefined) {
        member[field][name] = rangeFor(version);
        lockMoved = true;
      }
    }
    if (moved) {
      writeJson(path, manifest);
      written.push(`${dir}/package.json`);
    }
  }
  if (lockMoved) {
    writeJson(lockPath, lock);
    written.push("package-lock.json");
  }
  return written;
}
