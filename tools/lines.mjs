// The lines this repository releases: each published package has its own version,
// CHANGELOG.md and tags (`<line>-v<version>`). `release.mjs` is the one that reads this table.

/**
 * The lines, the crates in the order they go to crates.io (RELEASING.md): a crate comes after
 * every crate it depends on by path.
 *
 * - `dir` holds the package and `kind` says how it is released: `npm` (staged by the release
 *   workflow, made public by a maintainer's approval), `cargo` (a crate, published from the
 *   maintainer's machine) or `binary` (an executable, attached to its GitHub release from the
 *   maintainer's machine).
 * - `entries` are the files a package of the line is useless without.
 * - `manifest` is the workspace manifest the cargo commands of a crate run against, for a crate
 *   that is not a member of the repository's own workspace.
 * - `binary` is the name of the executable of a binary line and `targets` the platforms it is
 *   built for: its files in `dist/` are `<binary>-<version>-<target>`.
 */
export const LINES = {
  "sdk-typescript": {
    dir: "sdk/typescript",
    kind: "npm",
    name: "@pencere/sdk",
    entries: ["dist/index.js", "dist/index.d.ts"],
  },
  wire: {
    dir: "wire",
    kind: "cargo",
    name: "pencere-wire",
    entries: ["src/lib.rs"],
  },
  "host-rust": {
    dir: "host/rust",
    kind: "cargo",
    name: "pencere-host",
    entries: ["src/lib.rs"],
  },
  view: {
    dir: "kit/view",
    kind: "cargo",
    name: "pencere-view",
    entries: ["src/lib.rs"],
  },
  sandbox: {
    dir: "kit/sandbox",
    kind: "cargo",
    name: "pencere-sandbox",
    // The interpreter, the licences of what is built into it and the interface the crate reads
    // (a copy of kit/wit/view.wit: a crate cannot read outside itself) are part of the crate.
    entries: [
      "src/lib.rs",
      "luau/luau.wasm",
      "luau/inputs.sha256",
      "wit/view.wit",
      "licenses/llvm.txt",
      "licenses/lua.txt",
      "licenses/luau.txt",
      "licenses/wasi-libc.txt",
    ],
  },
  page: {
    dir: "kit/guest/sdk",
    kind: "cargo",
    name: "pencere-page",
    // The page modules have a workspace of their own, built for another target.
    manifest: "kit/guest/Cargo.toml",
    entries: ["src/lib.rs", "wit/view.wit"],
  },
  program: {
    dir: "program",
    kind: "cargo",
    name: "pencere",
    entries: ["src/lib.rs"],
  },
  cli: {
    dir: "cli",
    kind: "binary",
    name: "pencere-cli",
    binary: "pencere",
    entries: ["src/main.rs"],
    targets: [
      "aarch64-apple-darwin",
      "x86_64-apple-darwin",
      "aarch64-unknown-linux-musl",
      "x86_64-unknown-linux-musl",
    ],
  },
};
