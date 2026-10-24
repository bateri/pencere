# Releasing

Each package this repository publishes is a **line**: its own version, its own
`CHANGELOG.md`, its own tags. A release of one line does not release the other.

| Line             | Package        | Directory        | Tag                       | Published by                          |
| ---------------- | -------------- | ---------------- | ------------------------- | ------------------------------------- |
| `sdk-typescript` | `@pencere/sdk` | `sdk/typescript` | `sdk-typescript-v<x.y.z>` | the release workflow, approved on npm |
| `host-rust`      | `pencere`      | `host/rust`      | `host-rust-v<x.y.z>`      | the maintainer's machine (`cargo`)    |
| specification    | —              | `spec/`          | `spec-<YYYY-MM-DD>`       | an editorial step, see below          |

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

### What the release checks refuse

- a version of `0.0.0`, a version that is not after the last, or a manifest that disagrees with
  the newest changelog section;
- an `## [Unreleased]` section that still holds changes, or an empty release section;
- a branch other than `main`, a working tree that is not clean, a `main` that has diverged from
  `origin/main`;
- a tag that already exists, here or on `origin`;
- a package without its `LICENSE` and `NOTICE` (byte for byte the repository's), `README.md`,
  `CHANGELOG.md` or entry point, or one that carries tests.
  The npm package is checked from `npm pack`, which runs its clean build first. The crate is
  checked from `cargo package` and `cargo publish --dry-run`.

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
directly. Any version of either that is on npm is deprecated (`npm deprecate`, with a message that
says what replaced it) on the day the tools that take their place are released. It is a step of
that release, done by a maintainer with 2FA, and no check here covers it.

## The crate

`pencere` is published from the maintainer's machine:
`make release-prepare LINE=host-rust VERSION=…`, commit, `make release LINE=host-rust`, then
`make publish-local LINE=host-rust`, which runs `cargo publish`. crates.io has no staging: a
workflow that published the crate would make it public the moment a tag is pushed, with no
approval in between. When the crate has users outside this repository's organization, the
workflow can take it over through crates.io's trusted publishing, behind a GitHub environment
that requires a reviewer. Until then the workflow only writes the GitHub release of a
`host-rust-v*` tag.

The crate's tests read the specification's fixtures from the repository, so the package
carries the library, its README, changelog and license, and not the tests.

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

`make publish` and `make publish-local` publish a package under the project's name. Only a
maintainer runs them. The approval on npm (`make approve`) is a second, separate decision.
Everything before them (prepare, commit, tag) can be undone locally.
