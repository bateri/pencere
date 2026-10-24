// The lines this repository releases: each published package has its own version,
// CHANGELOG.md and tags (`<line>-v<version>`). `release.mjs` is the one that reads this table.

/**
 * The lines. `dir` holds the package, `kind` is the manager that publishes it, and `entries`
 * are the files a package of the line is useless without.
 */
export const LINES = {
  "sdk-typescript": {
    dir: "sdk/typescript",
    kind: "npm",
    name: "@pencere/sdk",
    entries: ["dist/index.js", "dist/index.d.ts"],
  },
  "host-rust": {
    dir: "host/rust",
    kind: "cargo",
    name: "pencere-host",
    entries: ["src/lib.rs"],
  },
};
