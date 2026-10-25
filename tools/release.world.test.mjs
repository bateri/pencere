// The release tooling against a repository with an origin and against stand-ins of the world it
// asks (crates.io through `cargo`, GitHub through `gh`, npm through `npm` and `npx`, see
// `release.fixture.mjs`): the checks of a release that need git, the verbs that tag, push and
// approve, and what the workflow runs (`package`, `draft`, `dist`, `verify-tag`).

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { describe, expect, it, vi } from "vitest";

import { LINES } from "./lines.mjs";
import {
  executable,
  fixture,
  git,
  members,
  nameOf,
  processorOf,
  release,
  scratch,
  systemOf,
  world,
  write,
} from "./release.fixture.mjs";

vi.setConfig({ testTimeout: 30_000 });

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

/** Writes the files of the command the way the workflow gathers them: the binaries and their sums. */
function dist(root, version = "0.1.0", dir = "dist") {
  mkdirSync(join(root, dir), { recursive: true });
  const sums = [];
  for (const target of LINES.cli.targets) {
    const bytes = executable(systemOf(target), processorOf(target));
    writeFileSync(join(root, dir, nameOf(target, version)), bytes);
    sums.push(`${sha256(bytes)}  ${nameOf(target, version)}`);
  }
  write(root, `${dir}/SHA256SUMS`, `${sums.join("\n")}\n`);
  return sums;
}

const TAGS = {
  "sdk-typescript": "sdk-typescript-v0.1.0",
  crates: "crates-v0.1.0",
  cli: "cli-v0.1.0",
};

/** A line that was tagged, whose workflow run is green and whose draft is there. */
function ready(w, id, { draft = true, runs } = {}) {
  w.git("tag", "-a", TAGS[id], "-m", "x");
  w.state.gh.releases[TAGS[id]] = { draft };
  w.state.gh.runs = runs ?? [{ databaseId: 7, status: "completed", conclusion: "success" }];
  return TAGS[id];
}

const real = (w) => realpathSync(w.root);
const dryRuns = (w) => w.said("cargo").filter((said) => said.startsWith("publish --dry-run"));
const published = (w) =>
  w
    .calls()
    .filter(
      (call) =>
        call.tool === "cargo" && call.args[0] === "publish" && !call.args.includes("--dry-run"),
    );
const edits = (w) => w.calls().filter((c) => c.tool === "gh" && c.args[1] === "edit");

describe("release.mjs check of the crates", () => {
  it("dry-runs the publication of all six, each with the crates it asks for taken from the repository", () => {
    const w = world();
    const done = w.run("check", "crates");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    const patch = (name, dir) =>
      `--config patch.crates-io.${name}.path=${JSON.stringify(join(real(w), dir))}`;
    expect(dryRuns(w)).toEqual([
      "publish --dry-run -p pencere-wire",
      `publish --dry-run -p pencere-host ${patch("pencere-wire", "wire")}`,
      "publish --dry-run -p pencere-view",
      `publish --dry-run -p pencere-sandbox ${patch("pencere-view", "kit/view")}`,
      `publish --dry-run -p pencere-page --manifest-path kit/guest/Cargo.toml ${patch("pencere-view", "kit/view")} ${patch("pencere-wire", "wire")}`,
      `publish --dry-run -p pencere ${patch("pencere-wire", "wire")}`,
    ]);
    expect(done.out).toContain("the pencere crates 0.1.0 is ready to tag as crates-v0.1.0");
  });

  it("runs the cargo commands of the page SDK against the page modules' workspace", () => {
    const w = world();
    w.run("check", "crates");
    const mentions = w.said("cargo").filter((said) => said.includes("kit/guest"));
    // Its file list, its manifest and its dry run; the other crates' commands name no other workspace.
    expect(mentions.length).toBe(3);
    for (const said of w.said("cargo").filter((said) => said.includes("-p pencere-page"))) {
      expect(said).toContain("--manifest-path kit/guest/Cargo.toml");
    }
  });

  it("asks crates.io from outside the repository, where cargo does not answer from the workspace", () => {
    const w = world();
    w.run("check", "crates");
    const asked = w.calls().filter((call) => call.tool === "cargo" && call.args[0] === "info");
    expect(asked.map((call) => call.args[1])).toEqual(members.map((m) => `${m.name}@0.1.0`));
    for (const call of asked) expect(call.cwd.startsWith(real(w))).toBe(false);
  });

  it("refuses a crate whose version crates.io has, and tags nothing", () => {
    const w = world();
    w.state.registry = ["pencere-view@0.1.0"];
    const done = w.run("tag", "crates");
    expect(done.status).toBe(1);
    expect(done.err).toContain("crates.io has pencere-view 0.1.0 already");
    expect(w.git("tag", "--list")).toBe("");
  });

  it("refuses a crate whose manifest lacks what crates.io refuses for, and names what", () => {
    const w = world();
    w.state.crates["pencere-sandbox"].manifest = { description: "", keywords: [] };
    const done = w.run("check", "crates");
    expect(done.status).toBe(1);
    expect(done.err).toContain("kit/sandbox/Cargo.toml lacks: description, keywords (one to five)");
    w.state.crates["pencere-sandbox"].manifest = { publish: [] };
    expect(w.run("check", "crates").err).toContain("lacks: publish (it is false)");
  });

  it("refuses a crate that asks by path for a crate that no line publishes, and for one with no version", () => {
    const w = world();
    w.state.crates["pencere-host"].dependencies.push({
      name: "left-pad",
      req: "^1.0.0",
      path: "/x",
      kind: null,
    });
    expect(w.run("check", "crates").err).toContain(
      "pencere-host depends on left-pad by path, and no line of this repository publishes it",
    );
    w.state.crates["pencere-host"].dependencies.pop();
    w.state.crates["pencere-host"].dependencies[0].req = "*";
    expect(w.run("check", "crates").err).toContain(
      'pencere-host asks for pencere-wire by path with the version "*"',
    );
  });

  it("leaves the dependencies the package does not carry (development ones) out of the dry run", () => {
    const w = world();
    w.state.crates["pencere-host"].dependencies.push({
      name: "containers-page",
      req: "*",
      path: "/x",
      kind: "dev",
    });
    const done = w.run("check", "crates");
    expect(done.err).toBe("");
    expect(dryRuns(w)[1]).not.toContain("containers-page");
  });

  it("refuses a package that lacks a file of the crate, or carries tests", () => {
    const w = world();
    const files = w.state.crates["pencere-sandbox"].files;
    w.state.crates["pencere-sandbox"].files = files.filter((file) => file !== "luau/luau.wasm");
    expect(w.run("check", "crates").err).toContain(
      "the package of pencere-sandbox lacks luau/luau.wasm",
    );
    w.state.crates["pencere-sandbox"].files = [
      ...files,
      "tests/session.rs",
      "src/images_tests.rs",
      "node_modules/x/y.js",
    ];
    expect(w.run("check", "crates").err).toContain(
      "the package carries tests or builds: tests/session.rs, src/images_tests.rs, node_modules/x/y.js",
    );
  });

  it("refuses a crate whose licence is not the repository's, or is not there", () => {
    const w = world();
    write(w.root, "kit/view/LICENSE", "another licence\n");
    w.git("add", "-A");
    w.git("commit", "-q", "-m", "another licence");
    expect(w.run("check", "crates").err).toContain(
      "kit/view/LICENSE is not the repository's LICENSE",
    );
    rmSync(join(w.root, "wire/NOTICE"));
    w.git("add", "-A");
    w.git("commit", "-q", "-m", "no notice");
    expect(w.run("check", "crates").err).toContain("wire/NOTICE is not the repository's NOTICE");
  });

  it("says why a dry run that failed did", () => {
    const w = world();
    w.state.failDryRun = ["pencere-page"];
    const done = w.run("check", "crates");
    expect(done.status).toBe(1);
    expect(done.err).toContain("failed to verify package pencere-page");
  });
});

describe("release.mjs check of the command", () => {
  it("checks the directory's files and nothing is built or asked for", () => {
    const w = world();
    const done = w.run("check", "cli");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(done.out).toContain("pencere 0.1.0 is ready to tag as cli-v0.1.0");
    expect(w.calls()).toEqual([]);
  });

  it("refuses a directory that lacks a file or whose licence is not the repository's", () => {
    const w = world();
    git(w.root, "rm", "-q", "cli/README.md");
    git(w.root, "commit", "-q", "-m", "no readme");
    expect(w.run("check", "cli").err).toContain("cli lacks README.md");
    write(w.root, "cli/LICENSE", "another\n");
    git(w.root, "add", "-A");
    git(w.root, "commit", "-q", "-m", "another licence");
    expect(w.run("check", "cli").err).toContain("cli/LICENSE is not the repository's LICENSE");
  });

  it("does not hold the tests of the command's crate against it", () => {
    const w = world();
    write(w.root, "cli/tests/runs.rs", "// a test\n");
    write(w.root, "cli/src/other_tests.rs", "// a test\n");
    git(w.root, "add", "-A");
    git(w.root, "commit", "-q", "-m", "tests");
    expect(w.run("check", "cli").status).toBe(0);
  });
});

describe("release.mjs check of the repository's state", () => {
  it("refuses a branch other than main", () => {
    const w = world();
    w.git("checkout", "-q", "-b", "other");
    expect(w.run("check", "cli").err).toContain("not on main");
  });

  it("refuses a working tree that is not clean", () => {
    const w = world();
    write(w.root, "stray.txt", "x");
    expect(w.run("check", "cli").err).toContain("the working tree is not clean");
  });

  it("refuses a main that has diverged from origin/main", () => {
    const w = world();
    const other = scratch();
    execFileSync("git", ["clone", "-q", w.origin, other]);
    git(other, "commit", "--allow-empty", "-q", "-m", "elsewhere");
    git(other, "push", "-q", "origin", "main");
    expect(w.run("check", "cli").err).toContain("main has diverged from origin/main");
  });

  it("refuses a tag that exists, and a version that is not after the line's last tag", () => {
    const w = world();
    w.git("tag", "cli-v0.1.0");
    expect(w.run("check", "cli").err).toContain("the tag cli-v0.1.0 exists");
    w.git("tag", "-d", "cli-v0.1.0");
    w.git("tag", "cli-v0.2.0");
    expect(w.run("check", "cli").err).toContain(
      "0.1.0 is not after 0.2.0, the line's last release",
    );
  });

  it("is not held up by the tags of other lines", () => {
    const w = world();
    w.git("tag", "crates-v0.5.0");
    w.git("tag", "sdk-typescript-v0.5.0");
    expect(w.run("check", "cli").status).toBe(0);
  });
});

describe("release.mjs rehearse", () => {
  it("runs the checks of a prepared line on any branch and with a tree that is not clean, and tags nothing", () => {
    const w = world();
    w.git("checkout", "-q", "-b", "other");
    write(w.root, "stray.txt", "x");
    const done = w.run("rehearse", "crates");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(done.out).toContain("(a rehearsal: the state of the repository was not looked at)");
    expect(dryRuns(w).length).toBe(6);
    expect(w.git("tag", "--list")).toBe("");
    // What the tag would have refused.
    expect(w.run("tag", "crates").err).toContain("not on main");
  });

  it("still refuses what is wrong with the line itself", () => {
    const w = world();
    write(w.root, "cli/CHANGELOG.md", "# Changelog\n\n## [Unreleased]\n\n- more\n");
    const done = w.run("rehearse", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain("Unreleased is not empty");
  });
});

describe("release.mjs tag and push", () => {
  it("makes the tag with the changelog sections for its message, and says to publish", () => {
    const w = world();
    const done = w.run("tag", "crates");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(done.out).toContain(
      "tagged crates-v0.1.0; nothing is pushed. Next: make publish LINE=crates",
    );
    const message = w.git("tag", "--list", "--format=%(contents)", "crates-v0.1.0");
    expect(message).toContain("the pencere crates 0.1.0");
    expect(message).toContain("## pencere-wire\n\n### Added\n\n- a change");
    expect(w.git("ls-remote", "--tags", "origin")).toBe("");
  });

  it("pushes main and the tag together, and does nothing else", () => {
    const w = world();
    write(w.root, "README.md", "# changed\n");
    w.git("add", "-A");
    w.git("commit", "-q", "-m", "a change");
    expect(w.run("tag", "cli").status).toBe(0);
    w.forget();
    const done = w.run("push", "cli");
    expect(done.status).toBe(0);
    expect(done.out).toContain("pushed main and cli-v0.1.0");
    expect(git(w.origin, "tag", "--list")).toBe("cli-v0.1.0");
    expect(git(w.origin, "rev-parse", "main")).toBe(w.git("rev-parse", "HEAD"));
    // Not a release, not a registry, not a build.
    expect(w.calls()).toEqual([]);
  });

  it.each(["crates", "sdk-typescript"])("pushes the tag of %s the same way", (id) => {
    const w = world();
    w.git("tag", "-a", TAGS[id], "-m", "x");
    w.forget();
    expect(w.run("push", id).status).toBe(0);
    expect(git(w.origin, "tag", "--list")).toBe(TAGS[id]);
    expect(w.calls()).toEqual([]);
  });

  it("refuses to push without a tag, and when the tag is not on HEAD", () => {
    const w = world();
    expect(w.run("push", "cli").err).toContain(
      "no tag cli-v0.1.0: run make release LINE=cli first",
    );
    w.git("tag", "-a", "cli-v0.1.0", "-m", "x");
    w.git("commit", "--allow-empty", "-q", "-m", "after the tag");
    const done = w.run("push", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain("cli-v0.1.0 is not on HEAD");
    expect(git(w.origin, "tag", "--list")).toBe("");
  });
});

describe("release.mjs approve", () => {
  it("refuses a line with no tag, and when gh is not logged in", () => {
    const w = world();
    expect(w.run("approve", "crates").err).toContain("crates has no tag");
    ready(w, "crates");
    w.state.gh.auth = false;
    expect(w.run("approve", "crates").err).toContain("gh is missing or not logged in");
  });

  it("refuses while the workflow's run for the tag is not green, and asks for the tag's own commit", () => {
    const w = world();
    const tag = ready(w, "crates", { runs: [] });
    expect(w.run("approve", "crates").err).toContain(
      "no run of the release workflow for crates-v0.1.0",
    );
    w.state.gh.runs = [{ databaseId: 9, status: "in_progress", conclusion: "" }];
    expect(w.run("approve", "crates").err).toContain(
      "run 9 for crates-v0.1.0 is in_progress: wait for it",
    );
    w.state.gh.runs = [
      { databaseId: 10, status: "completed", conclusion: "failure" },
      { databaseId: 9, status: "completed", conclusion: "success" },
    ];
    expect(w.run("approve", "crates").err).toContain("run 10 for crates-v0.1.0 ended in failure");
    const sha = w.git("rev-list", "-n", "1", tag);
    expect(w.said("gh").find((said) => said.startsWith("run list"))).toContain(
      `--workflow release.yml --event push --branch ${tag} --commit ${sha}`,
    );
    // Nothing was published, and nothing was made public.
    expect(published(w)).toEqual([]);
    expect(edits(w)).toEqual([]);
  });

  it("refuses a tag whose draft is not there, and one that is public already", () => {
    const w = world();
    ready(w, "crates");
    delete w.state.gh.releases["crates-v0.1.0"];
    expect(w.run("approve", "crates").err).toContain("crates-v0.1.0 has no GitHub release");
    w.state.gh.releases["crates-v0.1.0"] = { draft: false };
    expect(w.run("approve", "crates").err).toContain(
      "the GitHub release crates-v0.1.0 is public already",
    );
    expect(published(w)).toEqual([]);
    expect(edits(w)).toEqual([]);
  });

  describe("of an npm line", () => {
    const staged = [{ id: "stg-1", version: "0.1.0", status: "staged" }];

    it("shows what is staged, approves it, and then makes the draft public but not the Latest", () => {
      const w = world();
      ready(w, "sdk-typescript");
      w.state.npm.staged = staged;
      const done = w.run("approve", "sdk-typescript");
      expect(done.err).toBe("");
      expect(done.status).toBe(0);
      const npm = "--yes npm@11.21.1 stage";
      expect(w.said("npx")).toEqual([
        `${npm} list @pencere/sdk --json`,
        `${npm} view stg-1`,
        `${npm} approve stg-1`,
      ]);
      expect(edits(w).map((call) => call.args)).toEqual([
        ["release", "edit", "sdk-typescript-v0.1.0", "--draft=false", "--latest=false"],
      ]);
      expect(w.state.gh.releases["sdk-typescript-v0.1.0"].draft).toBe(false);
      expect(done.out).not.toContain("npm deprecate");
    });

    it("refuses when npm holds nothing staged of the version, or more than one, and the draft stays", () => {
      const w = world();
      ready(w, "sdk-typescript");
      w.state.npm.staged = [{ id: "stg-0", version: "0.0.9", status: "staged" }];
      expect(w.run("approve", "sdk-typescript").err).toContain(
        "npm holds no staged @pencere/sdk 0.1.0",
      );
      w.state.npm.staged = [...staged, { id: "stg-2", version: "0.1.0", status: "staged" }];
      expect(w.run("approve", "sdk-typescript").err).toContain(
        "npm holds more than one staged @pencere/sdk 0.1.0",
      );
      expect(edits(w)).toEqual([]);
      expect(w.state.gh.releases["sdk-typescript-v0.1.0"].draft).toBe(true);
    });
  });

  describe("of the crates", () => {
    it("publishes the six in order from a checkout of the tag, and then makes the draft public but not the Latest", () => {
      const w = world();
      ready(w, "crates");
      const done = w.run("approve", "crates");
      expect(done.err).toBe("");
      expect(done.status).toBe(0);
      const calls = published(w);
      expect(calls.map((call) => call.args)).toEqual([
        ["publish", "-p", "pencere-wire"],
        ["publish", "-p", "pencere-host"],
        ["publish", "-p", "pencere-view"],
        ["publish", "-p", "pencere-sandbox"],
        ["publish", "-p", "pencere-page", "--manifest-path", "kit/guest/Cargo.toml"],
        ["publish", "-p", "pencere"],
      ]);
      // Not from the working tree, which has gone on, but from a checkout of its own.
      for (const call of calls) {
        expect(call.cwd).not.toBe(real(w));
        expect(basename(call.cwd)).toMatch(/^pencere-publish-/);
      }
      expect(new Set(calls.map((call) => call.cwd)).size).toBe(1);
      expect(w.git("worktree", "list").split("\n").length).toBe(1);
      expect(edits(w).map((call) => call.args)).toEqual([
        ["release", "edit", "crates-v0.1.0", "--draft=false", "--latest=false"],
      ]);
      expect(w.state.gh.releases["crates-v0.1.0"].draft).toBe(false);
    });

    it("leaves the crates crates.io has already", () => {
      const w = world();
      ready(w, "crates");
      w.state.registry = ["pencere-wire@0.1.0", "pencere-host@0.1.0"];
      const done = w.run("approve", "crates");
      expect(done.status).toBe(0);
      expect(done.out).toContain("pencere-wire 0.1.0 is on crates.io already");
      expect(published(w).map((call) => call.args[2])).toEqual([
        "pencere-view",
        "pencere-sandbox",
        "pencere-page",
        "pencere",
      ]);
    });

    it("stops at the crate that cannot be published, keeps the draft, and goes on from there the next time", () => {
      const w = world();
      ready(w, "crates");
      w.state.failPublish = "pencere-sandbox";
      const failed = w.run("approve", "crates");
      expect(failed.status).toBe(1);
      expect(failed.err).toContain("failed to publish pencere-sandbox");
      expect(published(w).map((call) => call.args[2])).toEqual([
        "pencere-wire",
        "pencere-host",
        "pencere-view",
        "pencere-sandbox",
      ]);
      expect(edits(w)).toEqual([]);
      expect(w.state.gh.releases["crates-v0.1.0"].draft).toBe(true);
      expect(w.git("worktree", "list").split("\n").length).toBe(1);

      w.forget();
      delete w.state.failPublish;
      expect(w.run("approve", "crates").status).toBe(0);
      expect(published(w).map((call) => call.args[2])).toEqual([
        "pencere-sandbox",
        "pencere-page",
        "pencere",
      ]);
      expect(w.state.gh.releases["crates-v0.1.0"].draft).toBe(false);
    });
  });

  describe("of the command", () => {
    const withAssets = (w) => {
      const assets = join(w.aux, "assets");
      dist(w.aux, "0.1.0", "assets");
      w.state.gh.assets = assets;
      return assets;
    };
    const message = LINES.cli.retired["@pencere/cli"];

    it("checks the draft's files against the sums and their attestations, and makes it public as the Latest", () => {
      const w = world();
      ready(w, "cli");
      withAssets(w);
      const done = w.run("approve", "cli");
      expect(done.err).toBe("");
      expect(done.status).toBe(0);
      const download = w.calls().find((call) => call.args[1] === "download");
      expect(download.args.slice(0, 4)).toEqual(["release", "download", "cli-v0.1.0", "--dir"]);
      const attested = w.calls().filter((call) => call.args[0] === "attestation");
      expect(attested.map((call) => basename(call.args[2])).sort()).toEqual(
        [...LINES.cli.targets.map((target) => nameOf(target)), "SHA256SUMS"].sort(),
      );
      for (const call of attested) {
        expect(call.args.slice(0, 2)).toEqual(["attestation", "verify"]);
        expect(call.args.slice(3)).toEqual([
          "--repo",
          "bateri/pencere",
          "--signer-workflow",
          "bateri/pencere/.github/workflows/release.yml",
        ]);
      }
      expect(edits(w).map((call) => call.args)).toEqual([
        ["release", "edit", "cli-v0.1.0", "--draft=false", "--latest"],
      ]);
      expect(w.state.gh.releases["cli-v0.1.0"].draft).toBe(false);
    });

    it("prints the commands that deprecate the retired packages that are not, and runs none", () => {
      const w = world();
      ready(w, "cli");
      withAssets(w);
      w.state.npm.deprecated = { "@pencere/cli": "", "create-pencere": "" };
      const done = w.run("approve", "cli");
      expect(done.out).toContain(`npm deprecate "@pencere/cli@*" ${JSON.stringify(message)}`);
      expect(done.out).toContain(
        'npm deprecate "create-pencere@*" "Replaced by the pencere command (pencere new):',
      );
      expect(w.said("npm").every((said) => said.startsWith("view "))).toBe(true);
    });

    it("prints the command of the retired package that is not deprecated yet, and none for the rest", () => {
      const w = world();
      ready(w, "cli");
      withAssets(w);
      // One is deprecated, the other npm does not know.
      w.state.npm.deprecated = { "@pencere/cli": message };
      const done = w.run("approve", "cli");
      expect(done.status).toBe(0);
      expect(done.out).not.toContain("npm deprecate");
    });

    it("refuses files that are not the ones the sums have, and the draft stays", () => {
      const w = world();
      ready(w, "cli");
      const assets = withAssets(w);
      const changed = executable(systemOf(LINES.cli.targets[1]), processorOf(LINES.cli.targets[1]));
      changed[40] = 1;
      writeFileSync(join(assets, nameOf(LINES.cli.targets[1])), changed);
      const done = w.run("approve", "cli");
      expect(done.status).toBe(1);
      expect(done.err).toContain(`${nameOf(LINES.cli.targets[1])} is not the file that`);
      expect(edits(w)).toEqual([]);
      expect(w.state.gh.releases["cli-v0.1.0"].draft).toBe(true);
    });

    it("refuses a draft that holds a file no release of the command carries", () => {
      const w = world();
      ready(w, "cli");
      writeFileSync(join(withAssets(w), "extra.txt"), "x");
      const done = w.run("approve", "cli");
      expect(done.status).toBe(1);
      expect(done.err).toContain("extra.txt, which no release of pencere carries");
      expect(edits(w)).toEqual([]);
    });

    it("refuses files whose attestation does not verify, and the draft stays", () => {
      const w = world();
      ready(w, "cli");
      withAssets(w);
      w.state.gh.badAttestation = true;
      const done = w.run("approve", "cli");
      expect(done.status).toBe(1);
      expect(done.err).toContain("verification failed");
      expect(edits(w)).toEqual([]);
      expect(w.state.gh.releases["cli-v0.1.0"].draft).toBe(true);
    });
  });
});

describe("release.mjs draft", () => {
  const notes = "### Added\n\n- a change";

  it("makes the draft of a package from its changelog section, and never as the Latest", () => {
    const w = world();
    const done = w.run("draft", "sdk-typescript");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(w.state.gh.releases["sdk-typescript-v0.1.0"]).toEqual({
      draft: true,
      args: [
        "release",
        "create",
        "sdk-typescript-v0.1.0",
        "--verify-tag",
        "--draft",
        "--title",
        "sdk-typescript-v0.1.0",
        `--notes=${notes}`,
        "--latest=false",
      ],
    });
  });

  it("makes the draft of the crates with a section for each, and never as the Latest", () => {
    const w = world();
    expect(w.run("draft", "crates").status).toBe(0);
    const { args, draft } = w.state.gh.releases["crates-v0.1.0"];
    expect(draft).toBe(true);
    expect(args).toContain("--latest=false");
    const body = args.find((arg) => arg.startsWith("--notes="));
    for (const pkg of members) expect(body).toContain(`## ${pkg.name}\n\n### Added`);
  });

  it("makes the draft of the command with the notes and the five files, and leaves the Latest to approve", () => {
    const w = world();
    const files = join(w.root, "dist");
    const sums = dist(w.root);
    const done = w.run("draft", "cli", files);
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    const { args } = w.state.gh.releases["cli-v0.1.0"];
    expect(args.slice(0, 8)).toEqual([
      "release",
      "create",
      "cli-v0.1.0",
      "--verify-tag",
      "--draft",
      "--title",
      "cli-v0.1.0",
      `--notes=${notes}`,
    ]);
    expect(args.slice(8)).toEqual([
      ...sums.map((sum) => join(files, sum.split("  ")[1])),
      join(files, "SHA256SUMS"),
    ]);
    expect(args).not.toContain("--latest=false");
  });

  it("refuses the command's files when they are not right, before it makes anything", () => {
    const w = world();
    expect(w.run("draft", "cli", join(w.root, "nothing")).err).toContain("nothing/ is missing");
    dist(w.root, "0.0.9");
    expect(w.run("draft", "cli", join(w.root, "dist")).err).toContain("is missing");
    expect(w.state.gh.releases).toEqual({});
  });

  it("makes the draft again when a job that is run again finds the first run's", () => {
    const w = world();
    w.state.gh.releases["crates-v0.1.0"] = { draft: true, args: ["old"] };
    const done = w.run("draft", "crates");
    expect(done.status).toBe(0);
    const gh = w.said("gh").filter((said) => said.startsWith("release "));
    expect(gh[0]).toBe("release view crates-v0.1.0 --json isDraft");
    expect(gh[1]).toBe("release delete crates-v0.1.0 --yes");
    expect(gh[2]).toContain("release create crates-v0.1.0 --verify-tag --draft");
    expect(w.state.gh.releases["crates-v0.1.0"].args).not.toEqual(["old"]);
  });

  it("never touches a release that is public", () => {
    const w = world();
    w.state.gh.releases["crates-v0.1.0"] = { draft: false, args: ["public"] };
    const done = w.run("draft", "crates");
    expect(done.status).toBe(1);
    expect(done.err).toContain("a release that is public is never touched");
    expect(w.said("gh").filter((said) => /^release (delete|create|edit)/.test(said))).toEqual([]);
    expect(w.state.gh.releases["crates-v0.1.0"].args).toEqual(["public"]);
  });

  it("says why GitHub would not make it", () => {
    const w = world();
    w.state.gh.cannotCreate = true;
    const done = w.run("draft", "crates");
    expect(done.status).toBe(1);
    expect(done.err).toContain("HTTP 500");
  });
});

describe("release.mjs package, verify-tag and registry", () => {
  it("tries the package of each crate, as the workflow does for a tag", () => {
    const w = world();
    const done = w.run("package", "crates");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(dryRuns(w).length).toBe(6);
    for (const pkg of members) expect(done.out).toContain(`${pkg.name} 0.1.0: its package builds`);
    expect(w.run("package", "sdk-typescript").err).toContain("sdk-typescript is not crates");
  });

  it("verifies that a tag is on main and names the line, and says what the workflow reads", () => {
    const w = world();
    w.git("tag", "-a", "crates-v0.1.0", "-m", "x");
    const done = w.run("verify-tag", "crates-v0.1.0");
    expect(done.err).toBe("");
    expect(done.out.trim().split("\n")).toEqual([
      "line=crates",
      "kind=crates",
      "name=the pencere crates",
      "dir=wire",
      "version=0.1.0",
      "tag=crates-v0.1.0",
    ]);
  });

  it("refuses a tag that is not on main", () => {
    const w = world();
    w.git("commit", "--allow-empty", "-q", "-m", "not pushed");
    w.git("tag", "-a", "cli-v0.1.0", "-m", "x");
    const done = w.run("verify-tag", "cli-v0.1.0");
    expect(done.status).toBe(1);
    expect(done.err).toContain("cli-v0.1.0 is not on main");
  });

  it("asks npm whether the package, and its version, are there; and only of an npm line", () => {
    const w = world();
    w.state.npm.deprecated = { "@pencere/sdk": "" };
    expect(w.run("registry", "sdk-typescript", "0.1.0").out.trim().split("\n")).toEqual([
      "exists=true",
      "published=false",
    ]);
    expect(w.run("registry", "cli", "0.1.0").err).toContain("cli is released as binary");
  });
});

describe("release.mjs dist", () => {
  it("accepts the files the workflow gathers for the version, and names them", () => {
    const root = fixture();
    dist(root);
    const done = release(root, "dist", "cli");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(done.out.trim().split("\n")).toEqual([
      ...LINES.cli.targets.map((target) => `dist/${nameOf(target)}`),
      "dist/SHA256SUMS",
    ]);
  });

  it("takes the directory it is told, and the licence files `make dist` leaves beside the files", () => {
    const root = fixture();
    dist(root, "0.1.0", "elsewhere");
    write(root, "elsewhere/LICENSE", "l\n");
    write(root, "elsewhere/NOTICE", "n\n");
    const done = release(root, "dist", "cli", join(root, "elsewhere"));
    expect(done.err).toBe("");
    expect(done.out.trim().split("\n").length).toBe(5);
  });

  it("refuses a directory that is not there", () => {
    const done = release(fixture(), "dist", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain("dist/ is missing");
  });

  it.each(LINES.cli.targets)("refuses files without the binary for %s", (target) => {
    const root = fixture();
    dist(root);
    rmSync(join(root, "dist", nameOf(target)));
    const done = release(root, "dist", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain(`dist/${nameOf(target)} is missing`);
  });

  it("refuses files built for another version than the manifest's", () => {
    const root = fixture();
    dist(root, "0.0.9");
    const done = release(root, "dist", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain("dist/pencere-0.1.0-aarch64-apple-darwin is missing");
  });

  it("refuses sums of another version, though the binaries are this one's", () => {
    const root = fixture();
    dist(root);
    write(root, "dist/SHA256SUMS", `${dist(scratch(), "0.0.9").join("\n")}\n`);
    const done = release(root, "dist", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain("dist/SHA256SUMS names pencere-0.0.9-");
    expect(done.err).toContain("is it the sums of another version?");
  });

  it("refuses sums that leave a binary out, and sums that are not there", () => {
    const root = fixture();
    const sums = dist(root);
    write(root, "dist/SHA256SUMS", `${sums.slice(1).join("\n")}\n`);
    expect(release(root, "dist", "cli").err).toContain(
      `dist/SHA256SUMS does not name ${nameOf(LINES.cli.targets[0])}`,
    );
    rmSync(join(root, "dist/SHA256SUMS"));
    expect(release(root, "dist", "cli").err).toContain("dist/SHA256SUMS is missing");
  });

  it("refuses sums that are not those of the files", () => {
    const root = fixture();
    dist(root);
    const target = LINES.cli.targets[1];
    const changed = executable(systemOf(target), processorOf(target));
    changed[40] = 1;
    writeFileSync(join(root, "dist", nameOf(target)), changed);
    const done = release(root, "dist", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain(
      `dist/${nameOf(target)} is not the file that dist/SHA256SUMS has the sum of`,
    );
  });

  it("refuses a line of the sums that is not a sum, and a name twice", () => {
    const root = fixture();
    const sums = dist(root);
    write(root, "dist/SHA256SUMS", `${sums.join("\n")}\nnot a sum\n`);
    expect(release(root, "dist", "cli").err).toContain('"not a sum" is not "<sha256>  <file>"');
    write(root, "dist/SHA256SUMS", `${sums.join("\n")}\n${sums[0]}\n`);
    expect(release(root, "dist", "cli").err).toContain(
      `dist/SHA256SUMS names ${nameOf(LINES.cli.targets[0])} twice`,
    );
  });

  it("refuses an empty file, a file that is not an executable, and a file nothing names", () => {
    const root = fixture();
    dist(root);
    const last = nameOf(LINES.cli.targets[3]);
    writeFileSync(join(root, "dist", last), "");
    expect(release(root, "dist", "cli").err).toContain(`dist/${last} is empty`);
    writeFileSync(join(root, "dist", last), "#!/bin/sh\n");
    expect(release(root, "dist", "cli").err).toContain(`dist/${last} is not an executable`);
    dist(root);
    write(root, "dist/extra", "x");
    expect(release(root, "dist", "cli").err).toContain(
      "dist/ holds extra, which no release of pencere carries",
    );
  });

  it.each([
    ["x86_64-unknown-linux-musl", "linux", "aarch64"],
    ["aarch64-unknown-linux-musl", "macos", "aarch64"],
    ["aarch64-apple-darwin", "macos", "x86_64"],
    ["x86_64-apple-darwin", "linux", "x86_64"],
  ])("refuses the file for %s when it is a %s executable for %s", (target, system, processor) => {
    const root = fixture();
    dist(root);
    writeFileSync(join(root, "dist", nameOf(target)), executable(system, processor));
    const done = release(root, "dist", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain(
      `dist/${nameOf(target)} is a ${system} executable for ${processor}, not for ${target}`,
    );
  });

  it("is a command of the binary line only", () => {
    const done = release(fixture(), "dist", "crates");
    expect(done.status).toBe(1);
    expect(done.err).toContain("crates has no executables");
  });
});
