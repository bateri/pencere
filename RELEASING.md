# Releasing

What this repository releases is a **line**: one thing with one version, its changelog and its
tags. A release of one line does not release another. There are three lines, and the
specification, which is editorial.

| Line             | What                                            | Channel        | Tag                       |
| ---------------- | ----------------------------------------------- | -------------- | ------------------------- |
| `sdk-typescript` | `@pencere/sdk` (`sdk/typescript`)               | npm            | `sdk-typescript-v<x.y.z>` |
| `crates`         | six crates, **one version for all** (see below) | crates.io      | `crates-v<x.y.z>`         |
| `cli`            | the `pencere` command: four executables + sums  | GitHub release | `cli-v<x.y.z>`            |
| specification    | `spec/<date>/`, an editorial step (see below)   | —              | `spec-<YYYY-MM-DD>`       |

The six crates, in the order they are published: `pencere-wire` (`wire`), `pencere-host`
(`host/rust`), `pencere-view` (`kit/view`), `pencere-sandbox` (`kit/sandbox`), `pencere-page`
(`kit/guest/sdk`, in the page modules' workspace) and `pencere` (`program`). The four executables of
the command are for `aarch64-apple-darwin`, `x86_64-apple-darwin`, `aarch64-unknown-linux-musl` and
`x86_64-unknown-linux-musl`, named `pencere-<version>-<target>`.

Every step a machine can check is in `tools/release.mjs`, behind the `make` targets below. This
file is the order of the steps and the reasons for them.

## Versions and changelogs

- A line follows [semantic versioning](https://semver.org/). While it is `0.x`, a minor release may
  break what the previous one offered.
- The wire has its own version (the specification's `versions`, today `1`), separate from any
  package's. A package says in its README and changelog which wire versions it speaks. Version 1 of
  the wire is a draft: a `0.x` release may change it together with the specification. Once the
  specification publishes a dated version, its fixtures never change again (specification §5.4).
- Every change a user of a package would notice goes under `## [Unreleased]` in that package's
  `CHANGELOG.md` ([Keep a Changelog](https://keepachangelog.com/en/1.1.0/)), in the same commit as
  the change. A release only moves that section under a version and a date; it does not write it.

## One rhythm

Every line is released with the same four verbs, from a clean `main`, and the line's kind decides
only what each verb does. Nothing is public before the last one, which is the maintainer's.

1. **`make release-prepare LINE=… VERSION=…`** moves the version and the changelog(s), for one
   commit, `Release <name> <version>`: read the diff, then commit it.
   - npm: the version in `package.json` and `package-lock.json`.
   - crates: the version of all six crates, every `version = "…"` that sits beside a `path` to one
     of them (in every manifest of the repository, the command's and the page modules' included),
     and both lockfiles (`Cargo.lock`, `kit/guest/Cargo.lock`). `## [Unreleased]` becomes
     `## [<version>] - <date>` in each of the six changelogs. A crate with nothing of its own under
     Unreleased gets the line "Released with the other crates; no change of its own." (at least
     one crate must have something).
   - cli: the version in `cli/Cargo.toml` and `Cargo.lock`, and `cli/CHANGELOG.md`.
2. **`make release LINE=…`** runs the gate (`make check`), then the release checks, then makes the
   annotated tag with the changelog section as its message. Nothing is pushed.
   - crates: it also dry-runs the publication of all six (see [The crates](#the-crates)).
3. **`make publish LINE=…`** is one atomic push of `main` and the tag, and nothing else. The
   release workflow (`.github/workflows/release.yml`) takes it from there:
   - it checks that the tag names the version the manifests and the changelogs carry, on `main`,
     and runs the gate on Linux;
   - npm: it **stages** the package on npm through trusted publishing, which also signs where it
     was built (provenance);
   - cli: it builds the four executables, each on a native runner, tries each where its runner can
     run it, and writes `SHA256SUMS` and an attestation of the build (provenance);
   - crates: nothing is staged, for crates.io has no staging; it only runs the dry run again,
     from the tag;
   - every line: it writes the line's GitHub release as a **draft**, with the changelog section as
     the notes (for the crates, the six sections under the crates' names) and, for the command,
     the files. A draft is seen by maintainers only.
4. **`make approve LINE=…`** is the maintainer's one act that makes the line public everywhere. It
   first requires the tag's workflow run to be green and the draft to be there.
   - npm: shows the staged version (who staged it, its shasum) and approves it; npm asks for the
     second factor, so run it in a terminal. `npm stage` needs npm 11.15 or later and Node 22 ships
     npm 10, so the target runs the npm the workflow uses through `npx`. The same can be done on
     npmjs.com, under the package's **Staged Packages** tab. Then it publishes the draft.
   - crates: `cargo publish`es the six, in order, from a clean checkout at the tag (so what is
     published is what was tagged, not what the working tree has become). A crate that crates.io
     already has is skipped, so a run that stopped half way is run again. It needs `cargo login`.
     Then it publishes the draft.
   - cli: downloads the draft's files, checks them against `SHA256SUMS` and with
     `gh attestation verify`, then publishes the draft as the repository's **Latest** release. If
     the retired packages are not deprecated yet it prints the commands that do (see [Packages
     that are retired](#packages-that-are-retired)); it runs none.

Afterwards, check it: `npm view @pencere/sdk version`, `cargo info pencere@<version>` or
`gh release view cli-v<version>`.

If a check fails, nothing has left the machine: fix it, amend or add a commit, and run the step
again. A tag that was made but not pushed can be deleted with `git tag -d <tag>`. A pushed tag is
never moved; a broken release is followed by a new version. A job of the workflow that failed for
a reason outside the repository is run again (`gh run rerun <run-id> --failed`): the draft is made
again, and a release that is public is never touched.

**Latest.** Only a release of the command is marked Latest, so that the repository's front page
offers what a visitor downloads. Every other line's release is made with `--latest=false`, as a
draft and again when it is published.

**Rehearsals.** `make rehearse LINE=…` runs the release checks of a prepared line on any branch,
without those of the repository's state (the branch, a clean tree, `origin`, the tags): it changes
nothing. `make dist` builds the four executables on this machine (Docker, and the Apple targets of
rustup's toolchain) and tries them, into `dist/`: a rehearsal of what the workflow builds, not what
is released. The workflow can also be started by hand, on any branch (`gh workflow run release.yml
-f line=cli`, or **Run workflow** in the Actions tab; `cli` or `crates`): it builds, tries and sums,
uploads the files as workflow artifacts, and makes no release and no attestation.

### Why the workflow builds the command

The command's executables used to be built on the maintainer's machine, because an executable that
a workflow built was one that a pushed tag changed without a maintainer's decision. The decision is
now `make approve`: the workflow only writes a draft, and nothing it built is public before the
maintainer has verified it. What a workflow gives that one machine could not is the provenance (a
signed statement of which commit and which workflow built each file) and a native runner for each
system, with no emulation.

### What the release checks refuse

- a version of `0.0.0`, one that is not after the line's last tag (for the crates: one that is
  before a crate's own), or a manifest that disagrees with the newest changelog section;
- an `## [Unreleased]` section that still holds changes, or an empty release section;
- a branch other than `main`, a working tree that is not clean, a `main` that has diverged from
  `origin/main`;
- a tag that already exists, here or on `origin`;
- a package without its `LICENSE` and `NOTICE` (byte for byte the repository's), `README.md`,
  `CHANGELOG.md` or entry point, or one that carries tests (a `tests/` directory, a `…_tests.rs`
  module of `src/`), a build (`target/`) or installed packages (`node_modules/`). The npm package is
  checked from `npm pack`, which runs its clean build first; a crate from `cargo package --list`;
  the command's directory from the files git holds;
- a crate whose manifest lacks what crates.io refuses for (a description, a licence) or what every
  crate here says (the repository, a readme, keywords, a category), or whose version crates.io
  already has.

`make check` compares the `LICENSE` and `NOTICE` copies at every run, so they cannot drift between
releases. `make approve LINE=cli` refuses files that are not the four executables of the version for
the system and processor their names say, or whose `SHA256SUMS` does not name exactly these four or
has a wrong sum.

## The crates

The six crates are one line with one version because they are one thing to the person who uses
them: a program uses `pencere`, a terminal `pencere-host`, a page `pencere-page`, and they ask for
each other by version. A release of one used to move the others' requirements and make them the next
releases, one after the other; now a release moves all six, and each crate's changelog says what
changed in it, or that nothing did. The cost is a crate released with no change of its own, and the
line in its changelog is for that.

A crate is published after the crates it depends on, because its package asks for them by version
and crates.io must have them: the wire first; then `pencere-host` and `pencere-view`; then
`pencere-sandbox`, which depends on `pencere-view`; then `pencere-page` and `pencere`.
`tools/lines.mjs` lists them in this order and `make approve` publishes in it, each after the
previous one is in the index (cargo waits for that).

The version starts at `0.2.0`: `pencere` already has a `0.1.0` on crates.io, the host crate under
the name the program library has now, and the other five have no `0.1.0`.

**The dry run.** A dry run (`cargo publish --dry-run`) builds the package from its own files against
the registry, so it fails for a crate whose dependency crates.io does not have at that version, and
in lockstep none of the six has it before the first is published. Cargo 1.88, the toolchain this
repository is gated with, cannot package several members and check them against each other (that is
cargo 1.90's); so each crate is dry-run on its own, with the crates it depends on taken from the
repository (`--config patch.crates-io.<crate>.path=…`). Each package is still built from its own
extracted files, which is the proof that it reads nothing outside itself. The page SDK is in another
workspace (`kit/guest`, with its own toolchain and lockfile), so its commands run against
`kit/guest/Cargo.toml` (the member's `manifest` in `tools/lines.mjs`).

**What a crate carries.** The package carries the library, its README, changelog, `LICENSE` and
`NOTICE`, and not the tests: they read the specification's fixtures, the scripts and the page
modules of the repository. The interface the sandbox and the page SDK implement is
`kit/wit/view.wit`, which a crate cannot read from outside itself: each carries a copy,
`kit/sandbox/wit/view.wit` and `kit/guest/sdk/wit/view.wit`, that `make check` keeps byte for byte
equal. After a change of the interface: `cp kit/wit/view.wit kit/sandbox/wit/view.wit && cp
kit/wit/view.wit kit/guest/sdk/wit/view.wit`, and `make luau` for the interpreter.

crates.io could take the publishing over from the maintainer's machine through its trusted
publishing, behind a GitHub environment that requires a reviewer. Until then `make approve` is the
only thing that runs `cargo publish`.

## The command

The `pencere` command is the `cli` line: the crate `pencere-cli`, whose binary is `pencere`. It is
not on crates.io (the crate is `publish = false`); its release is the four executables and
`SHA256SUMS` as files of the GitHub release `cli-v<x.y.z>`.

- **A person installs it** with `cli/install.sh`, one line (in the command's README). It takes the
  newest `cli-v*` release, not the repository's Latest (that is right only while nothing else is
  marked so), checks the file against `SHA256SUMS` and puts it in `~/.local/bin`. `pencere build
--target` fetches the runner of another system from the same release, by the same names.
- **The workflow builds** the macOS executables on a macOS runner (both targets, cross-built by one
  compiler) and each Linux executable on a runner of its own architecture, in the image of
  `tools/linux/Dockerfile`. Each is tried where its runner can run it, as `make dist` tries them:
  `--version`; `codesign --verify` for the Mac one; and for the Linux ones the example app made with
  the executable and run against the stand-in terminal. The log says whether the Intel executable
  could be run on the Mac runner (through Rosetta) or was not.
- **The command is released after the others.** `pencere new --lang ts` makes a project that
  asks npm for `@pencere/sdk` (`cli/templates/ts/deno.json`), and `--lang rust` one that asks
  crates.io for `pencere` (`cli/templates/rust/Cargo.toml.tmpl`), by the versions the command was
  built with. A release of the command that needs a newer SDK or crates than the registries have
  makes projects that do not build: release `sdk-typescript` and `crates` first, approve them, and
  then the command. A Lua project needs nothing from either.
- **`cli-v0.1.0` is not the command's.** That tag is the retired TypeScript `@pencere/cli`'s, so
  the command's first release is `0.2.0`, and a tag is never reused.

## Packages that are retired

`@pencere/cli` and `create-pencere` ran and generated programs for an earlier draft of the protocol,
and this repository no longer has them: a program now shows its view with `@pencere/sdk` directly,
and the `pencere` command makes and builds apps. Any version of either that is on npm is deprecated
once the command is released (`make approve LINE=cli` prints these two commands while they still
have to be run). It is a manual step, done by a maintainer with 2FA, and nothing in this repository
runs it:

```sh
npm deprecate "@pencere/cli@*" "Replaced by the pencere command (pencere new, pencere build): install it from the GitHub releases of bateri/pencere. A TypeScript program uses @pencere/sdk."
npm deprecate "create-pencere@*" "Replaced by the pencere command (pencere new): install it from the GitHub releases of bateri/pencere. A TypeScript program uses @pencere/sdk."
```

`@*` is every version. `npm view @pencere/cli deprecated` and `npm view create-pencere deprecated`
show the message afterwards.

## npm settings the workflow relies on

On npmjs.com, in the settings of `@pencere/sdk`: **Publishing access** requires two-factor
authentication and disallows bypass tokens; the **trusted publisher** is GitHub Actions,
organization `bateri`, repository `pencere`, workflow `release.yml`, no environment, with "Allow npm
publish" and "Allow npm dist-tag" left off, so the workflow can only stage and never make a version
public by itself. npm stops a trusted publisher that has not published within two days of being
made, and one that has stopped cannot be edited: it is deleted and made again.

## The specification

A dated version of the specification is an editorial decision, not a check. When the wire of a draft
is settled:

1. Copy `spec/draft/` to `spec/<YYYY-MM-DD>/`, and point `tools/check-spec.mjs` and the fixture
   tests at it as well.
2. From then on, never edit that directory. A change of the wire is a new version of the wire, with
   new fixtures, in the draft.
3. Tag the commit `spec-<YYYY-MM-DD>`. The packages that speak it say so in their next release's
   changelog.

## Who releases

`make publish` and `make approve` make something happen under the project's name: only a maintainer
runs them, and `make approve` is a decision of its own, taken once the workflow's work is there to be
looked at. Everything before them (prepare, commit, `make release`, `make rehearse`, `make dist`)
can be undone locally.
