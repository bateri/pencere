// The lines this repository releases (RELEASING.md): a line has one version, its CHANGELOG.md
// file(s) and its tags (`<line>-v<version>`). `release.mjs` is the one that reads this table.

/**
 * The lines. A line is released with the same four verbs whatever it is (`make release-prepare`,
 * `release`, `publish`, `approve`); its `kind` decides what each does:
 *
 * - `npm`: a package, staged by the release workflow and approved on npm. `dir` holds it.
 * - `crates`: crates published to crates.io, **one version for all of them**. `members` are the
 *   crates in the order they are published: a crate comes after every crate it depends on by path.
 *   Each member has a `dir`, a `name`, the `entries` a package of it is useless without, and a
 *   `manifest` when it is not a member of the repository's own cargo workspace.
 * - `binary`: an executable, built for each of `targets` by the release workflow and attached as
 *   `<binary>-<version>-<target>` to a GitHub release with `SHA256SUMS`. `dir` holds its crate.
 *
 * `title` is what a release commit names (`Release <title> <version>`). `retired` are the npm
 * packages a line replaces, each with the message `npm deprecate` gives it.
 */
export const LINES = {
  "sdk-typescript": {
    dir: "sdk/typescript",
    kind: "npm",
    name: "@pencere/sdk",
    title: "@pencere/sdk",
    entries: ["dist/index.js", "dist/index.d.ts"],
  },
  crates: {
    kind: "crates",
    title: "the pencere crates",
    members: [
      {
        dir: "wire",
        name: "pencere-wire",
        entries: ["src/lib.rs"],
      },
      {
        dir: "host/rust",
        name: "pencere-host",
        entries: ["src/lib.rs"],
      },
      {
        dir: "kit/view",
        name: "pencere-view",
        entries: ["src/lib.rs"],
      },
      {
        dir: "kit/sandbox",
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
      {
        dir: "kit/guest/sdk",
        name: "pencere-page",
        // The page modules have a workspace of their own, built for another target.
        manifest: "kit/guest/Cargo.toml",
        entries: ["src/lib.rs", "wit/view.wit"],
      },
      {
        dir: "program",
        name: "pencere",
        entries: ["src/lib.rs"],
      },
    ],
  },
  cli: {
    dir: "cli",
    kind: "binary",
    name: "pencere-cli",
    title: "pencere",
    binary: "pencere",
    entries: ["src/main.rs"],
    targets: [
      "aarch64-apple-darwin",
      "x86_64-apple-darwin",
      "aarch64-unknown-linux-musl",
      "x86_64-unknown-linux-musl",
    ],
    // The TypeScript `@pencere/cli` and the generator `create-pencere`, which the command replaces.
    retired: {
      "@pencere/cli":
        "Replaced by the pencere command (pencere new, pencere build): install it from the GitHub releases of bateri/pencere. A TypeScript program uses @pencere/sdk.",
      "create-pencere":
        "Replaced by the pencere command (pencere new): install it from the GitHub releases of bateri/pencere. A TypeScript program uses @pencere/sdk.",
    },
  },
};
