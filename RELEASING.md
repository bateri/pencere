# Releasing

Each package this repository publishes is a **line**: its own version, its own
`CHANGELOG.md`, its own tags. A release of one line does not release another.

| Line             | Package                                     | Directory        | Tag                       | Published by                                      |
| ---------------- | ------------------------------------------- | ---------------- | ------------------------- | ------------------------------------------------- |
| `sdk-typescript` | `@pencere/sdk`                              | `sdk/typescript` | `sdk-typescript-v<x.y.z>` | the release workflow, approved on npm             |
| `wire`           | crate `pencere-wire`                        | `wire`           | `wire-v<x.y.z>`           | the maintainer's machine (`cargo`)                |
| `host-rust`      | crate `pencere-host`                        | `host/rust`      | `host-rust-v<x.y.z>`      | the maintainer's machine (`cargo`)                |
| `view`           | crate `pencere-view`                        | `kit/view`       | `view-v<x.y.z>`           | the maintainer's machine (`cargo`)                |
| `sandbox`        | crate `pencere-sandbox`                     | `kit/sandbox`    | `sandbox-v<x.y.z>`        | the maintainer's machine (`cargo`)                |
| `page`           | crate `pencere-page`                        | `kit/guest/sdk`  | `page-v<x.y.z>`           | the maintainer's machine (`cargo`)                |
| `program`        | crate `pencere`                             | `program`        | `program-v<x.y.z>`        | the maintainer's machine (`cargo`)                |
| `cli`            | the `pencere` command (crate `pencere-cli`) | `cli`            | `cli-v<x.y.z>`            | the maintainer's machine (`make dist`, then `gh`) |
| specification    | —                                           | `spec/`          | `spec-<YYYY-MM-DD>`       | an editorial step, see below                      |

Every step a machine can check is in `tools/release.mjs`, behind the `make` targets below. This
file is the order of the steps and the reasons for them.

## Versions

- A line follows [semantic versioning](https://semver.org/). While it is `0.x`, a minor release
  may break what the previous one offered.
- The wire has its own version (the specification's `versions`, today `1`), separate from
  any package's. A package says in its README and changelog which wire versions it speaks.
- Version 1 of the wire is a draft. A `0.x` release may change it together with the
  specification. Once the specification publishes a dated version, its fixtures never change
  again (specification §5.4). A package that reaches `1.0` speaks only published wire versions.

## Changelogs

Every change a user of a package would notice goes under `## [Unreleased]` in that package's
`CHANGELOG.md` ([Keep a Changelog](https://keepachangelog.com/en/1.1.0/)), in the same commit
as the change. A release only moves that section under a version and a date; it does not write
it.

## A release

From a clean `main`:

1. `make release-prepare LINE=sdk-typescript VERSION=0.2.0` sets the version in the manifest
   and the lockfile, turns `## [Unreleased]` into `## [0.2.0] - <today>` with a new, empty
   `## [Unreleased]` above it. Read the diff, then commit it as `Release @pencere/sdk 0.2.0`.
2. `make release LINE=sdk-typescript` runs the gate (`make check`), then the release checks,
   then makes the annotated tag `sdk-typescript-v0.2.0` with the changelog section as its
   message. Nothing is pushed.
3. `make publish LINE=sdk-typescript` pushes `main` and the tag in one atomic push. From here the
   release workflow (`.github/workflows/release.yml`) takes over:
   - it checks that the tag names the version the manifest and the changelog carry, on `main`;
   - it runs the gate on Linux;
   - it stages the package on npm through trusted publishing, which also signs where it was
     built (provenance);
   - it writes the GitHub release from the changelog section.
4. `make approve LINE=sdk-typescript` once the workflow has staged the version. It is not public
   until a maintainer approves it with 2FA: the target shows the staged version (who staged it,
   its shasum) and approves it, and npm asks for the second factor, so run it in a terminal.
   `npm stage` needs npm 11.15 or later and Node 22 ships npm 10, so the target runs the npm
   the workflow uses through `npx`. The same can be done on npmjs.com, under the package's
   **Staged Packages** tab.
5. Check it: `npm view @pencere/sdk version`.

If a check fails, nothing has left the machine: fix it, amend or add a commit, and run the step
again. A tag that was made but not pushed can be deleted with `git tag -d <tag>`. A pushed tag is
never moved; a broken release is followed by a new version.

The steps above are those of an npm line. A line's kind (`tools/lines.mjs`) decides what comes
after step 2: a crate is published from the maintainer's machine ([The crates](#the-crates)), the
command goes to GitHub with its files ([The command](#the-command)), and in both the workflow
only verifies the tag and runs the gate, and for a crate writes the GitHub release.

### What the release checks refuse

- a version of `0.0.0`, a version that is not after the last, or a manifest that disagrees with
  the newest changelog section;
- an `## [Unreleased]` section that still holds changes, or an empty release section;
- a branch other than `main`, a working tree that is not clean, a `main` that has diverged from
  `origin/main`;
- a tag that already exists, here or on `origin`;
- a package without its `LICENSE` and `NOTICE` (byte for byte the repository's), `README.md`,
  `CHANGELOG.md` or entry point, or one that carries tests (a `tests/` directory, a
  `…_tests.rs` module of `src/`), a build (`target/`) or installed packages (`node_modules/`).
  The npm package is checked from `npm pack`, which runs its clean build first. A crate is
  checked from `cargo package --list` and a dry run of its publication (see [The
  crates](#the-crates) for when that is the whole dry run).
- for the command, a `dist/` that is not there, that lacks one of the four executables of the
  version, whose executables are not the system and processor their names say, or whose
  `SHA256SUMS` does not name exactly these four files or has the wrong sum of one. The directory
  of the command is checked from the files git holds: its `LICENSE`, `NOTICE`, `README.md`,
  `CHANGELOG.md`, `Cargo.toml` and `src/main.rs`.

`make check` compares the `LICENSE` and `NOTICE` copies at every run, so they cannot drift between
releases.

## A line's first release

npm learns about a package from its first publish, and a trusted publisher can only be set up
for a package that exists. Staging a package npm does not know would publish a public
placeholder, so the workflow refuses to. A line's first release is therefore published from the
maintainer's machine, with the same steps:

1. `make release-prepare LINE=sdk-typescript VERSION=0.1.0`, review, commit.
2. `make release LINE=sdk-typescript`.
3. `make publish-local LINE=sdk-typescript` runs `npm publish` (npm asks for the second factor,
   so run it in a terminal), waits until npm lists the version, then pushes `main` and the tag.
   The workflow sees that the version is already on npm, stages nothing, and writes the GitHub
   release.

npm processes a new package before it lists it ("Your package is being processed and may take a
few minutes to become available"), and the workflow's `npm` job asks npm whether the version is
there, so the push waits for the listing, up to 15 minutes. If the job still finds nothing, it
fails with "not on npm yet" and the GitHub release is not written: once
`npm view <package> version` answers, run what failed again (`gh run rerun <run-id> --failed`).

`@pencere/sdk` was released this way once, at `0.1.0`. From its second release on, the workflow
stages.

Then, on npmjs.com, in the settings of `@pencere/sdk`:

- **Publishing access**, once the first release is out: require two-factor authentication and
  disallow bypass tokens. It does not expire.
- **Trusted publisher**, on the day of the line's second release, just before `make publish`:
  GitHub Actions, organization `bateri`, repository `pencere`, workflow `release.yml`, no
  environment, with "Allow npm publish" and "Allow npm dist-tag" left off, so the workflow can
  only stage and never make a version public by itself. npm stops a trusted publisher that has
  not published within two days of being made, and one that has stopped cannot be edited: it is
  deleted and made again. So it is made when a release is about to use it, not after the first.

From then on a version becomes public only through the workflow and a maintainer's approval.

## Packages that are retired

`@pencere/cli` and `create-pencere` ran and generated programs for an earlier draft of the
protocol, and this repository no longer has them: a program now shows its view with `@pencere/sdk`
directly, and the `pencere` command makes and builds apps. Any version of either that is on npm is
deprecated on the day the `pencere` command is released (the `cli` line's first release, once
`make publish LINE=cli` has made its GitHub release). It is a manual step of that release, done by
a maintainer with 2FA, and nothing in this repository runs it or checks it:

```sh
npm deprecate "@pencere/cli@*" "Replaced by the pencere command (pencere new, pencere build): install it from the GitHub releases of bateri/pencere. A TypeScript program uses @pencere/sdk."
npm deprecate "create-pencere@*" "Replaced by the pencere command (pencere new): install it from the GitHub releases of bateri/pencere. A TypeScript program uses @pencere/sdk."
```

`@*` is every version. `npm view @pencere/cli deprecated` and `npm view create-pencere deprecated`
show the message afterwards.

## The crates

Six crates are published to crates.io, each on its own line, from the maintainer's machine:
`make release-prepare LINE=… VERSION=…`, commit, `make release LINE=…`, then
`make publish-local LINE=…`, which runs `cargo publish` and pushes `main` and the tag. crates.io
has no staging: a workflow that published a crate would make it public the moment a tag is
pushed, with no approval in between. When the crates have users outside this repository's
organization, the workflow can take them over through crates.io's trusted publishing, behind a
GitHub environment that requires a reviewer. Until then the workflow only writes the GitHub
release of a crate's tag.

### The order

A crate is published after the crates it depends on, because its package asks for them by
version and crates.io must have them:

1. `pencere-wire` (line `wire`): the wire, with no crate of this repository below it.
2. `pencere-host` (`host-rust`) and `pencere-view` (`view`): the first depends on the wire, the
   second on nothing. Either may go first.
3. `pencere-sandbox` (`sandbox`): depends on `pencere-view`.
4. `pencere-page` (`page`) and `pencere` (`program`): the first depends on `pencere-view` and
   `pencere-wire`, the second on `pencere-wire`. Either may go first.

`make publish-local` refuses a crate whose dependency crates.io does not have yet, and names it.
`tools/lines.mjs` lists the lines in this order.

`pencere` already has a release on crates.io: `0.1.0`, the host crate under its old name. The
`program` line continues that name at `0.2.0`; `pencere-host` is a new name, and its first release
is `0.1.0`.

When a crate's release takes its version past what the crates that depend on it ask for (a `0.2.0`
after `0.1.0`), `make release-prepare` moves their `version = "…"` beside the `path` too, in the
same commit, and says which: those crates are the next releases, since their last release asks
for a version that no longer exists next to this one's.

### What is checked, and when

A dry run (`cargo publish --dry-run`) builds the package against the registry, so it can only run
when the crates this one depends on are on crates.io: cargo cannot even make the package of a
crate that asks for one the registry does not have. `make release` asks crates.io for each of
them (`cargo info`, from outside the repository, where it would answer from the workspace):

- all of them are there: the full dry run, as for `pencere-wire`, which has none to wait for;
- one is not: the list of files the package would carry (`cargo package --list`) and the
  manifest (cargo's report of it: what crates.io refuses a crate for, no description or licence,
  and the repository, readme, keywords and category every crate here has) are checked, nothing
  else, and the output names the crate and says so.

Following the order above, every crate gets the full dry run: each is released and published
before the crates that depend on it are tagged. The lighter check is for a tag made before the crate below
it has been published. What it cannot see is a package that does not build from its own files (a
file read from outside the crate, a dependency it does not name). For `pencere-sandbox` and
`pencere-page` the full dry run is the first build of the package apart from the repository, and
the proof that the copy of the interface in `wit/` is all they read.

### What a crate carries

The package carries the library, its README, changelog, `LICENSE` and `NOTICE`, and not the tests:
they read the specification's fixtures, the scripts and the page modules of the repository.
`pencere-view` and `pencere-sandbox` keep their tests in `src/` (`…_tests.rs`, `tests.rs`), which
their `include` leaves out, and the release check refuses a package that carries one.

The interface the sandbox and the page SDK implement is `kit/wit/view.wit`, which a crate cannot
read from outside itself: each carries a copy, `kit/sandbox/wit/view.wit` and
`kit/guest/sdk/wit/view.wit`, that `make check` keeps byte for byte equal. After a change of the
interface: `cp kit/wit/view.wit kit/sandbox/wit/view.wit && cp kit/wit/view.wit
kit/guest/sdk/wit/view.wit`, and `make luau` for the interpreter.

`pencere-page` is in the page modules' workspace (`kit/guest`, another toolchain, its own
lockfile), so its commands run against `kit/guest/Cargo.toml` (the line's `manifest` in
`tools/lines.mjs`). Its release moves the version in that lockfile.

## The command

The `pencere` command is the `cli` line: the crate `pencere-cli`, whose binary is `pencere`. It is
not on crates.io (the crate is `publish = false`). Its release is the executable, as a file on the
line's GitHub release `cli-v<x.y.z>`, for four targets (`aarch64-apple-darwin`,
`x86_64-apple-darwin`, `aarch64-unknown-linux-musl`, `x86_64-unknown-linux-musl`), named
`pencere-<version>-<target>`, with `SHA256SUMS` naming the four.

They are built on the maintainer's machine, by `make dist` into `dist/`, and the workflow does not
build them: an executable that a workflow built is one that a pushed tag changes without a
maintainer's decision, and the machine that has the toolchains for the four targets is this one.
For a `cli-v*` tag the workflow verifies that the tag names the version the manifest and the
changelog carry, on `main`, and runs the gate; it writes no GitHub release, because the release
needs the files.

From a clean `main`:

1. `make release-prepare LINE=cli VERSION=…`, review, commit.
2. `make dist`, from that commit, on a clean tree. The file names carry the version, so a `dist/`
   built before the version moved is refused; nothing can tell which source a `dist/` of the right
   version was built from, so build it after the commit and not before.
3. `make release LINE=cli`: the gate, the release checks (including those of `dist/`), the tag.
   Nothing is pushed.
4. `make publish LINE=cli`: the checks of `dist/` again, then one atomic push of `main` and the
   tag, then `gh release create cli-v<x.y.z>` with the changelog section as the notes and the four
   executables and `SHA256SUMS` as the files. It needs `gh`, logged in, and refuses when the
   release already exists.
5. Check it: `gh release view cli-v<x.y.z>`.

If the push went and `gh` failed, the tag is on `origin` and the release is not: run
`make publish LINE=cli` again. It pushes nothing new and makes the release.

On the first release of the command, deprecate the packages it replaces
([Packages that are retired](#packages-that-are-retired)).

## The specification

A dated version of the specification is an editorial decision, not a check. When the wire of a
draft is settled:

1. Copy `spec/draft/` to `spec/<YYYY-MM-DD>/`, and point `tools/check-spec.mjs` and the fixture
   tests at it as well.
2. From then on, never edit that directory. A change of the wire is a new version of the wire,
   with new fixtures, in the draft.
3. Tag the commit `spec-<YYYY-MM-DD>`. The packages that speak it say so in their next release's
   changelog.

## Who releases

`make publish`, `make publish-local` publish a package, a crate or a GitHub release under the
project's name. Only a maintainer runs them. The approval on npm (`make approve`) is a second,
separate decision. Everything before them (prepare, commit, `make dist`, tag) can be undone
locally.
