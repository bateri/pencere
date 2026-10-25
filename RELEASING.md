# Releasing

This repository releases three **lines**, each with its own version, changelog and tags. A release
of one line does not release another.

| Line             | What                                    | Channel        | Tag                       |
| ---------------- | --------------------------------------- | -------------- | ------------------------- |
| `sdk-typescript` | `@pencere/sdk` (`sdk/typescript`)       | npm            | `sdk-typescript-v<x.y.z>` |
| `crates`         | six crates, one version for all         | crates.io      | `crates-v<x.y.z>`         |
| `cli`            | the `pencere` command, four executables | GitHub release | `cli-v<x.y.z>`            |

The specification is dated by hand (see [The specification](#the-specification)).

## Versions and changelogs

- [Semantic versioning](https://semver.org/). While a line is `0.x`, a minor release may break.
- The wire has its own version (today `1`, a draft), separate from the packages'.
- Every change a user would notice goes under `## [Unreleased]` in the package's `CHANGELOG.md`, in
  the same commit as the change. A release only moves that section under a version and a date.

## A release

1. `make release-prepare LINE=<line> VERSION=<x.y.z>` sets the version in the manifests and
   lockfiles and dates the changelog(s). Commit it as `Release <line> <version>` and push it to
   `main`, directly or through a pull request.
2. The gate (`check.yml`) runs on the push. When it is green, the release workflow
   (`release.yml`) starts.
3. The workflow finds each line whose version has no tag yet; a commit that moved no version ends
   here. For each one it checks the version and changelog, checks the package (`npm pack`,
   `cargo publish --dry-run`) or builds the four executables with their `SHA256SUMS` and
   provenance, then pushes the tag and writes a **draft** GitHub release.
4. The run waits in the `release` environment. GitHub notifies the reviewers; one of them opens
   the run and chooses **Review deployments → Approve**. Only then is anything published: npm and
   crates.io through trusted publishing (no tokens), and the command's draft made public as
   **Latest**. Rejected, nothing is published.

Check it afterwards: `npm view @pencere/sdk version`, `cargo info pencere@<version>`,
`gh release view cli-v<version>`.

**If something fails.** Before the tag, nothing is public: fix it and push again. A tag is never
moved; a broken release is followed by a new version. A release that stopped after its tag is
finished by running the workflow by hand on `main` (**Run workflow**, or
`gh workflow run release.yml -f line=<line>`); what is published already is skipped.

**Trying it.** `make rehearse LINE=<line>` runs the checks of a prepared line on any branch.
`make dist` builds the four executables locally. **Run workflow** with **rehearse** builds on any
branch and publishes nothing.

### What the checks refuse

- a version that is not after the last tag, or manifests that disagree with the changelog;
- a non-empty `## [Unreleased]`, or an empty release section;
- a commit whose gate is not green, or a tag on another commit;
- a package missing `LICENSE`, `NOTICE`, `README.md` or `CHANGELOG.md`, or one that carries tests
  or build output; a crate missing crates.io metadata;
- command files that are not exactly the four executables of this version with correct sums.

## The crates

They share one version because they ask for each other by version. They are published in this
order, each after the ones it depends on: `pencere-wire`, `pencere-host`, `pencere-view`,
`pencere-sandbox`, `pencere-page`, `pencere` (`tools/lines.mjs`). A crate with no change of its own
gets "Released with the other crates; no change of its own." in its changelog.

Each crate is dry-run on its own, with its dependencies patched to the repository, because none of
them is on crates.io at the new version yet. The sandbox and the page SDK carry a copy of
`kit/wit/view.wit`; `make check` keeps the copies equal.

A crate that is new to crates.io is published the first time by hand (`cargo publish -p <crate>`
from the tag), since a trusted publisher can only be set on an existing crate.

## The command

The release is four files, `pencere-<version>-<target>` for macOS and Linux on arm64 and x86_64,
and `SHA256SUMS`. `cli/install.sh` installs the newest one; `pencere build --target` fetches
runners from the same release.

The command is published after the SDK and the crates, and only once the versions it makes new
projects with are on npm and crates.io, so that `pencere new` never makes a project that cannot
build.

`cli-v0.1.0` belongs to the retired `@pencere/cli`; the command started at `0.2.0`. That package
and `create-pencere` are deprecated on npm.

## Settings the workflow relies on

None of these is a secret.

- **GitHub** → Settings → Environments → `release`: **Required reviewers**, and deployments from
  `main` only. A publishing job refuses to run if the environment has no reviewers, because GitHub
  would otherwise create it empty and publish without asking.
- **npm**, `@pencere/sdk`: trusted publisher GitHub Actions, `bateri/pencere`, `release.yml`,
  environment `release`; tokens disallowed.
- **crates.io**, each of the six crates: trusted publisher GitHub, `bateri/pencere`, `release.yml`,
  environment `release`.

## The specification

When the wire of the draft is settled:

1. Copy `spec/draft/` to `spec/<YYYY-MM-DD>/` and point `tools/check-spec.mjs` and the fixture
   tests at it.
2. Never edit that directory again; a change of the wire is a new version in the draft.
3. Tag the commit `spec-<YYYY-MM-DD>`.
