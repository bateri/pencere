// The release tooling against a repository with an origin and against stand-ins of the world it
// asks (crates.io through `cargo`, GitHub through `gh`, npm through `npm` and `npx`, see
// `release.fixture.mjs`): the checks of a release that need git, and what the release workflow runs
// (`plan`, `package`, `dist`, `tag`, `draft`, `publish`).

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

/** A line that the workflow tagged on this commit, and whose draft is there. */
function ready(w, id, { draft = true } = {}) {
  w.git("tag", "-a", TAGS[id], "-m", "x");
  w.state.gh.releases[TAGS[id]] = { draft };
  return TAGS[id];
}

/** What `plan` says, as the workflow reads it from its output file. */
const outputs = (done) =>
  Object.fromEntries(
    done.out
      .trim()
      .split("\n")
      .map((line) => [line.slice(0, line.indexOf("=")), line.slice(line.indexOf("=") + 1)]),
  );

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

describe("release.mjs rehearse of the crates", () => {
  it("dry-runs the publication of all seven, each with the crates it asks for taken from the repository", () => {
    const w = world();
    const done = w.run("rehearse", "crates");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    const patch = (name, dir) =>
      `--config patch.crates-io.${name}.path=${JSON.stringify(join(real(w), dir))}`;
    expect(dryRuns(w)).toEqual([
      "publish --dry-run -p pencere-wire",
      `publish --dry-run -p pencere-host ${patch("pencere-wire", "wire")}`,
      "publish --dry-run -p pencere-view",
      `publish --dry-run -p pencere-editor ${patch("pencere-view", "kit/view")}`,
      `publish --dry-run -p pencere-sandbox ${patch("pencere-view", "kit/view")}`,
      `publish --dry-run -p pencere-page --manifest-path kit/guest/Cargo.toml ${patch("pencere-view", "kit/view")} ${patch("pencere-wire", "wire")}`,
      `publish --dry-run -p pencere ${patch("pencere-wire", "wire")}`,
    ]);
    expect(done.out).toContain("the pencere crates 0.1.0 is ready to be released as crates-v0.1.0");
  });

  it("runs the cargo commands of the page SDK against the page modules' workspace", () => {
    const w = world();
    w.run("rehearse", "crates");
    const mentions = w.said("cargo").filter((said) => said.includes("kit/guest"));
    // Its file list, its manifest and its dry run; the other crates' commands name no other workspace.
    expect(mentions.length).toBe(3);
    for (const said of w.said("cargo").filter((said) => said.includes("-p pencere-page"))) {
      expect(said).toContain("--manifest-path kit/guest/Cargo.toml");
    }
  });

  it("asks crates.io from outside the repository, where cargo does not answer from the workspace", () => {
    const w = world();
    w.run("rehearse", "crates");
    const asked = w.calls().filter((call) => call.tool === "cargo" && call.args[0] === "info");
    expect(asked.map((call) => call.args[1])).toEqual(members.map((m) => `${m.name}@0.1.0`));
    for (const call of asked) expect(call.cwd.startsWith(real(w))).toBe(false);
  });

  it("refuses a crate whose version crates.io has, and tags nothing", () => {
    const w = world();
    w.state.registry = ["pencere-view@0.1.0"];
    const done = w.run("rehearse", "crates");
    expect(done.status).toBe(1);
    expect(done.err).toContain("crates.io has pencere-view 0.1.0 already");
    expect(w.git("tag", "--list")).toBe("");
  });

  it("refuses a crate whose manifest lacks what crates.io refuses for, and names what", () => {
    const w = world();
    w.state.crates["pencere-sandbox"].manifest = { description: "", keywords: [] };
    const done = w.run("rehearse", "crates");
    expect(done.status).toBe(1);
    expect(done.err).toContain("kit/sandbox/Cargo.toml lacks: description, keywords (one to five)");
    w.state.crates["pencere-sandbox"].manifest = { publish: [] };
    expect(w.run("rehearse", "crates").err).toContain("lacks: publish (it is false)");
  });

  it("refuses a crate that asks by path for a crate that no line publishes, and for one with no version", () => {
    const w = world();
    w.state.crates["pencere-host"].dependencies.push({
      name: "left-pad",
      req: "^1.0.0",
      path: "/x",
      kind: null,
    });
    expect(w.run("rehearse", "crates").err).toContain(
      "pencere-host depends on left-pad by path, and no line of this repository publishes it",
    );
    w.state.crates["pencere-host"].dependencies.pop();
    w.state.crates["pencere-host"].dependencies[0].req = "*";
    expect(w.run("rehearse", "crates").err).toContain(
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
    const done = w.run("rehearse", "crates");
    expect(done.err).toBe("");
    expect(dryRuns(w)[1]).not.toContain("containers-page");
  });

  it("refuses a package that lacks a file of the crate, or carries tests", () => {
    const w = world();
    const files = w.state.crates["pencere-sandbox"].files;
    w.state.crates["pencere-sandbox"].files = files.filter((file) => file !== "luau/luau.wasm");
    expect(w.run("rehearse", "crates").err).toContain(
      "the package of pencere-sandbox lacks luau/luau.wasm",
    );
    w.state.crates["pencere-sandbox"].files = [
      ...files,
      "tests/session.rs",
      "src/images_tests.rs",
      "node_modules/x/y.js",
    ];
    expect(w.run("rehearse", "crates").err).toContain(
      "the package carries tests or builds: tests/session.rs, src/images_tests.rs, node_modules/x/y.js",
    );
  });

  it("refuses a crate whose licence is not the repository's, or is not there", () => {
    const w = world();
    write(w.root, "kit/view/LICENSE", "another licence\n");
    w.git("add", "-A");
    w.git("commit", "-q", "-m", "another licence");
    expect(w.run("rehearse", "crates").err).toContain(
      "kit/view/LICENSE is not the repository's LICENSE",
    );
    rmSync(join(w.root, "wire/NOTICE"));
    w.git("add", "-A");
    w.git("commit", "-q", "-m", "no notice");
    expect(w.run("rehearse", "crates").err).toContain("wire/NOTICE is not the repository's NOTICE");
  });

  it("says why a dry run that failed did", () => {
    const w = world();
    w.state.failDryRun = ["pencere-page"];
    const done = w.run("rehearse", "crates");
    expect(done.status).toBe(1);
    expect(done.err).toContain("failed to verify package pencere-page");
  });
});

describe("release.mjs rehearse of the command", () => {
  it("checks the directory's files and nothing is built or asked for", () => {
    const w = world();
    const done = w.run("rehearse", "cli");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(done.out).toContain("pencere 0.1.0 is ready to be released as cli-v0.1.0");
    expect(w.calls()).toEqual([]);
  });

  it("refuses a directory that lacks a file or whose licence is not the repository's", () => {
    const w = world();
    git(w.root, "rm", "-q", "cli/README.md");
    git(w.root, "commit", "-q", "-m", "no readme");
    expect(w.run("rehearse", "cli").err).toContain("cli lacks README.md");
    write(w.root, "cli/LICENSE", "another\n");
    git(w.root, "add", "-A");
    git(w.root, "commit", "-q", "-m", "another licence");
    expect(w.run("rehearse", "cli").err).toContain("cli/LICENSE is not the repository's LICENSE");
  });

  it("does not hold the tests of the command's crate against it", () => {
    const w = world();
    write(w.root, "cli/tests/runs.rs", "// a test\n");
    write(w.root, "cli/src/other_tests.rs", "// a test\n");
    git(w.root, "add", "-A");
    git(w.root, "commit", "-q", "-m", "tests");
    expect(w.run("rehearse", "cli").status).toBe(0);
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
    expect(dryRuns(w).length).toBe(7);
    expect(w.git("tag", "--list")).toBe("");
  });

  it("still refuses what is wrong with the line itself", () => {
    const w = world();
    write(w.root, "cli/CHANGELOG.md", "# Changelog\n\n## [Unreleased]\n\n- more\n");
    const done = w.run("rehearse", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain("Unreleased is not empty");
  });

  it("refuses a tag that exists, and a version that is not after the line's last tag", () => {
    const w = world();
    w.git("tag", "cli-v0.1.0");
    expect(w.run("rehearse", "cli").err).toContain("the tag cli-v0.1.0 exists");
    w.git("tag", "-d", "cli-v0.1.0");
    w.git("tag", "cli-v0.2.0");
    expect(w.run("rehearse", "cli").err).toContain(
      "0.1.0 is not after 0.2.0, the line's last release",
    );
  });

  it("is not held up by the tags of other lines", () => {
    const w = world();
    w.git("tag", "crates-v0.5.0");
    w.git("tag", "sdk-typescript-v0.5.0");
    expect(w.run("rehearse", "cli").status).toBe(0);
  });
});

describe("release.mjs plan", () => {
  it("names every line whose version has no tag, for the workflow to read", () => {
    const w = world();
    const done = w.run("plan");
    expect(done.status).toBe(0);
    expect(done.out.trim().split("\n")).toEqual([
      'lines=["sdk-typescript","crates","cli"]',
      "sdk-typescript=true",
      "crates=true",
      "cli=true",
    ]);
    expect(done.err).toContain("pencere 0.1.0 is to be released as cli-v0.1.0");
    // Nothing is asked of the world, nothing is tagged.
    expect(w.calls()).toEqual([]);
    expect(w.git("tag", "--list")).toBe("");
  });

  it("leaves the lines whose version is tagged, whatever their changelogs have gone on to", () => {
    const w = world();
    w.git("tag", "cli-v0.1.0");
    w.git("tag", "crates-v0.1.0");
    write(w.root, "cli/CHANGELOG.md", "# Changelog\n\n## [Unreleased]\n\n- the next one\n");
    const done = w.run("plan");
    expect(done.status).toBe(0);
    expect(outputs(done)).toEqual({
      lines: '["sdk-typescript"]',
      "sdk-typescript": "true",
      crates: "false",
      cli: "false",
    });
  });

  it("names nothing when every version is tagged", () => {
    const w = world();
    for (const tag of Object.values(TAGS)) w.git("tag", tag);
    const done = w.run("plan");
    expect(done.status).toBe(0);
    expect(outputs(done).lines).toBe("[]");
    expect(done.err).toContain("nothing to release");
  });

  it("refuses a version with no tag that was not prepared, and names nothing", () => {
    const w = world();
    write(w.root, "sdk/typescript/CHANGELOG.md", "# Changelog\n\n## [Unreleased]\n\n- more\n");
    const done = w.run("plan");
    expect(done.status).toBe(1);
    expect(done.err).toContain("sdk/typescript/CHANGELOG.md: Unreleased is not empty");
    expect(done.out).toBe("");
  });

  it("names the line it is told to, and finishes one tagged on this commit whose release is a draft", () => {
    const w = world();
    expect(outputs(w.run("plan", "crates"))).toMatchObject({ lines: '["crates"]', cli: "false" });
    ready(w, "cli");
    expect(outputs(w.run("plan", "cli")).lines).toBe('["cli"]');
    w.state.gh.releases["cli-v0.1.0"].draft = false;
    expect(w.run("plan", "cli").err).toContain("the GitHub release cli-v0.1.0 is public already");
  });

  it("finishes a release tagged on an earlier commit of the branch from that commit", () => {
    const w = world();
    w.git("tag", "-a", "cli-v0.1.0", "-m", "x");
    w.state.gh.releases["cli-v0.1.0"] = { draft: true };
    const tagged = w.git("rev-parse", "HEAD");
    w.git("commit", "--allow-empty", "-q", "-m", "after the tag");
    // What main has gone on to is not asked: it is not what is released.
    write(w.root, "cli/CHANGELOG.md", "# Changelog\n\n## [Unreleased]\n\n- the next one\n");
    w.git("commit", "-q", "-am", "the next one");
    const done = w.run("plan", "cli");
    expect(done.status).toBe(0);
    expect(outputs(done)).toMatchObject({ lines: '["cli"]', cli: "true", sha: tagged });
    expect(done.err).toContain("is to be finished from cli-v0.1.0");
    // Released on the commit it was started on, it names none.
    expect(outputs(w.run("plan", "crates")).sha).toBeUndefined();
    w.state.gh.releases["cli-v0.1.0"].draft = false;
    expect(w.run("plan", "cli").err).toContain("the GitHub release cli-v0.1.0 is public already");
  });

  it("refuses to finish a release whose tag is on a commit that is not one of the branch's", () => {
    const w = world();
    w.git("checkout", "-q", "-b", "aside");
    w.git("commit", "--allow-empty", "-q", "-m", "aside");
    w.git("tag", "-a", "cli-v0.1.0", "-m", "x");
    w.git("checkout", "-q", "main");
    w.git("commit", "--allow-empty", "-q", "-m", "on main");
    const done = w.run("plan", "cli");
    expect(done.status).toBe(1);
    expect(done.err).toContain("not one of this branch's: a tag is never moved");
    expect(done.out).toBe("");
  });

  it("rehearsing, names the line whatever its tags, and asks nothing", () => {
    const w = world();
    w.git("tag", "cli-v0.1.0");
    const done = w.runWith({ RELEASE_REHEARSE: "true" }, "plan", "cli");
    expect(done.status).toBe(0);
    expect(outputs(done).lines).toBe('["cli"]');
    expect(w.calls()).toEqual([]);
  });
});

describe("release.mjs tag", () => {
  it("makes the tag with the changelog sections for its message, and pushes it alone", () => {
    const w = world();
    const done = w.run("tag", "crates");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(done.out).toContain("tagged crates-v0.1.0 and pushed it");
    const message = w.git("tag", "--list", "--format=%(contents)", "crates-v0.1.0");
    expect(message).toContain("the pencere crates 0.1.0");
    expect(message).toContain("## pencere-wire\n\n### Added\n\n- a change");
    expect(git(w.origin, "tag", "--list")).toBe("crates-v0.1.0");
    expect(git(w.origin, "rev-parse", "crates-v0.1.0^{commit}")).toBe(w.git("rev-parse", "HEAD"));
    expect(w.calls()).toEqual([]);
  });

  it("leaves a tag that is on this commit, and refuses one on another", () => {
    const w = world();
    expect(w.run("tag", "cli").status).toBe(0);
    const again = w.run("tag", "cli");
    expect(again.status).toBe(0);
    expect(again.out).toContain("cli-v0.1.0 is on this commit already");
    w.git("commit", "--allow-empty", "-q", "-m", "after the tag");
    expect(w.run("tag", "cli").err).toContain("cli-v0.1.0 is on another commit");
  });

  it("refuses a line that is not prepared, and tags nothing", () => {
    const w = world();
    write(w.root, "cli/CHANGELOG.md", "# Changelog\n\n## [Unreleased]\n\n- more\n");
    expect(w.run("tag", "cli").status).toBe(1);
    expect(w.git("tag", "--list")).toBe("");
  });
});

describe("release.mjs publish", () => {
  it("refuses a line with no tag, no draft, a release that is public, and a tag on another commit", () => {
    const w = world();
    expect(w.run("publish", "crates").err).toContain("no tag crates-v0.1.0");
    ready(w, "crates");
    delete w.state.gh.releases["crates-v0.1.0"];
    expect(w.run("publish", "crates").err).toContain("crates-v0.1.0 has no GitHub release");
    w.state.gh.releases["crates-v0.1.0"] = { draft: false };
    expect(w.run("publish", "crates").err).toContain(
      "the GitHub release crates-v0.1.0 is public already",
    );
    w.state.gh.releases["crates-v0.1.0"] = { draft: true };
    w.git("commit", "--allow-empty", "-q", "-m", "after the tag");
    expect(w.run("publish", "crates").err).toContain("crates-v0.1.0 is not on this commit");
    expect(published(w)).toEqual([]);
    expect(edits(w)).toEqual([]);
  });

  it("in the workflow, refuses an environment with nobody to approve, and publishes nothing", () => {
    const w = world();
    ready(w, "crates");
    const inWorkflow = { RELEASE_ENVIRONMENT: "release", GITHUB_REPOSITORY: "bateri/pencere" };
    expect(w.runWith(inWorkflow, "publish", "crates").err).toContain(
      "cannot ask GitHub about the environment release",
    );
    // What GitHub makes of an environment a job names that is not there: no rule.
    w.state.gh.environment = { name: "release", protection_rules: [] };
    expect(w.runWith(inWorkflow, "publish", "crates").err).toContain(
      "the environment release has no required reviewers, so nobody approved this",
    );
    w.state.gh.environment = {
      name: "release",
      protection_rules: [{ type: "branch_policy" }, { type: "required_reviewers", reviewers: [] }],
    };
    expect(w.runWith(inWorkflow, "publish", "crates").status).toBe(1);
    expect(published(w)).toEqual([]);
    expect(edits(w)).toEqual([]);
    expect(w.said("gh")).toContain("api repos/bateri/pencere/environments/release");

    w.state.gh.environment = {
      name: "release",
      protection_rules: [
        {
          type: "required_reviewers",
          reviewers: [{ type: "User", reviewer: { login: "kalaomer" } }],
        },
      ],
    };
    expect(w.runWith(inWorkflow, "publish", "crates").status).toBe(0);
    expect(w.state.gh.releases["crates-v0.1.0"].draft).toBe(false);
  });

  describe("of an npm line", () => {
    it("publishes the package with its provenance, and then makes the draft public but not the Latest", () => {
      const w = world();
      ready(w, "sdk-typescript");
      const done = w.run("publish", "sdk-typescript");
      expect(done.err).toBe("");
      expect(done.status).toBe(0);
      const publishing = w.calls().filter((c) => c.tool === "npm" && c.args[0] === "publish");
      expect(publishing.map((c) => c.args)).toEqual([
        ["publish", "--provenance", "--access", "public"],
      ]);
      expect(publishing[0].cwd).toBe(join(real(w), "sdk/typescript"));
      expect(w.state.npm.published).toEqual(["@pencere/sdk@0.1.0"]);
      expect(edits(w).map((call) => call.args)).toEqual([
        ["release", "edit", "sdk-typescript-v0.1.0", "--draft=false", "--latest=false"],
      ]);
      expect(done.out).not.toContain("npm deprecate");
    });

    it("keeps the draft when npm refuses, and leaves a version npm has the next time", () => {
      const w = world();
      ready(w, "sdk-typescript");
      w.state.npm.failPublish = true;
      expect(w.run("publish", "sdk-typescript").status).toBe(1);
      expect(w.state.gh.releases["sdk-typescript-v0.1.0"].draft).toBe(true);
      w.forget();
      w.state.npm.failPublish = false;
      w.state.npm.published = ["@pencere/sdk@0.1.0"];
      const done = w.run("publish", "sdk-typescript");
      expect(done.status).toBe(0);
      expect(done.out).toContain("@pencere/sdk 0.1.0 is on npm already");
      expect(w.said("npm").some((said) => said.startsWith("publish"))).toBe(false);
      expect(w.state.gh.releases["sdk-typescript-v0.1.0"].draft).toBe(false);
    });
  });

  describe("of the crates", () => {
    it("publishes the seven in order from a checkout of the tag, and then makes the draft public but not the Latest", () => {
      const w = world();
      ready(w, "crates");
      const done = w.run("publish", "crates");
      expect(done.err).toBe("");
      expect(done.status).toBe(0);
      const calls = published(w);
      expect(calls.map((call) => call.args)).toEqual([
        ["publish", "-p", "pencere-wire"],
        ["publish", "-p", "pencere-host"],
        ["publish", "-p", "pencere-view"],
        ["publish", "-p", "pencere-editor"],
        ["publish", "-p", "pencere-sandbox"],
        ["publish", "-p", "pencere-page", "--manifest-path", "kit/guest/Cargo.toml"],
        ["publish", "-p", "pencere"],
      ]);
      // Not from the working tree, but from a checkout of its own.
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
      const done = w.run("publish", "crates");
      expect(done.status).toBe(0);
      expect(done.out).toContain("pencere-wire 0.1.0 is on crates.io already");
      expect(published(w).map((call) => call.args[2])).toEqual([
        "pencere-view",
        "pencere-editor",
        "pencere-sandbox",
        "pencere-page",
        "pencere",
      ]);
    });

    it("stops at the crate that cannot be published, keeps the draft, and goes on from there the next time", () => {
      const w = world();
      ready(w, "crates");
      w.state.failPublish = "pencere-sandbox";
      const failed = w.run("publish", "crates");
      expect(failed.status).toBe(1);
      expect(failed.err).toContain("failed to publish pencere-sandbox");
      expect(published(w).map((call) => call.args[2])).toEqual([
        "pencere-wire",
        "pencere-host",
        "pencere-view",
        "pencere-editor",
        "pencere-sandbox",
      ]);
      expect(edits(w)).toEqual([]);
      expect(w.state.gh.releases["crates-v0.1.0"].draft).toBe(true);
      expect(w.git("worktree", "list").split("\n").length).toBe(1);

      w.forget();
      delete w.state.failPublish;
      expect(w.run("publish", "crates").status).toBe(0);
      expect(published(w).map((call) => call.args[2])).toEqual([
        "pencere-sandbox",
        "pencere-page",
        "pencere",
      ]);
      expect(w.state.gh.releases["crates-v0.1.0"].draft).toBe(false);
    });

    it("tries the packages of a release that stopped half way, leaving the crates it published", () => {
      const w = world();
      ready(w, "crates");
      w.state.registry = ["pencere-wire@0.1.0"];
      const done = w.run("package", "crates");
      expect(done.err).toBe("");
      expect(done.status).toBe(0);
      expect(dryRuns(w).length).toBe(6);
    });
  });

  describe("of the command", () => {
    const withAssets = (w) => {
      const assets = join(w.aux, "assets");
      dist(w.aux, "0.1.0", "assets");
      w.state.gh.assets = assets;
      return assets;
    };
    /** The command's draft, with the SDK and the program library of its commit released. */
    const readyCommand = (w) => {
      ready(w, "cli");
      w.state.npm.published = ["@pencere/sdk@0.1.0"];
      w.state.registry = ["pencere@0.1.0"];
      return withAssets(w);
    };
    const message = LINES.cli.retired["@pencere/cli"];

    it("checks the draft's files against the sums and their attestations, and makes it public as the Latest", () => {
      const w = world();
      readyCommand(w);
      const done = w.run("publish", "cli");
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

    it("refuses while the SDK or the program library of its commit is not on its registry", () => {
      const w = world();
      readyCommand(w);
      w.state.npm.published = [];
      expect(w.run("publish", "cli").err).toContain(
        "@pencere/sdk 0.1.0 is not on npm: the command is released after it",
      );
      w.state.npm.published = ["@pencere/sdk@0.1.0"];
      w.state.registry = [];
      expect(w.run("publish", "cli").err).toContain(
        "pencere 0.1.0 is not on crates.io: the command is released after it",
      );
      expect(edits(w)).toEqual([]);
      expect(w.state.gh.releases["cli-v0.1.0"].draft).toBe(true);
    });

    it("prints the commands that deprecate the retired packages that are not, and runs none", () => {
      const w = world();
      readyCommand(w);
      w.state.npm.deprecated = { "@pencere/cli": "", "create-pencere": "" };
      const done = w.run("publish", "cli");
      expect(done.out).toContain(`npm deprecate "@pencere/cli@*" ${JSON.stringify(message)}`);
      expect(done.out).toContain(
        'npm deprecate "create-pencere@*" "Replaced by the pencere command (pencere new):',
      );
      expect(w.said("npm").every((said) => said.startsWith("view "))).toBe(true);
    });

    it("prints the command of the retired package that is not deprecated yet, and none for the rest", () => {
      const w = world();
      readyCommand(w);
      // One is deprecated, the other npm does not know.
      w.state.npm.deprecated = { "@pencere/cli": message };
      const done = w.run("publish", "cli");
      expect(done.status).toBe(0);
      expect(done.out).not.toContain("npm deprecate");
    });

    it("refuses files that are not the ones the sums have, and the draft stays", () => {
      const w = world();
      const assets = readyCommand(w);
      const changed = executable(systemOf(LINES.cli.targets[1]), processorOf(LINES.cli.targets[1]));
      changed[40] = 1;
      writeFileSync(join(assets, nameOf(LINES.cli.targets[1])), changed);
      const done = w.run("publish", "cli");
      expect(done.status).toBe(1);
      expect(done.err).toContain(`${nameOf(LINES.cli.targets[1])} is not the file that`);
      expect(edits(w)).toEqual([]);
      expect(w.state.gh.releases["cli-v0.1.0"].draft).toBe(true);
    });

    it("refuses a draft that holds a file no release of the command carries", () => {
      const w = world();
      writeFileSync(join(readyCommand(w), "extra.txt"), "x");
      const done = w.run("publish", "cli");
      expect(done.status).toBe(1);
      expect(done.err).toContain("extra.txt, which no release of pencere carries");
      expect(edits(w)).toEqual([]);
    });

    it("refuses files whose attestation does not verify, and the draft stays", () => {
      const w = world();
      readyCommand(w);
      w.state.gh.badAttestation = true;
      const done = w.run("publish", "cli");
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

describe("release.mjs package", () => {
  it("tries the package of each crate, as the workflow does before it tags", () => {
    const w = world();
    const done = w.run("package", "crates");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(dryRuns(w).length).toBe(7);
    for (const pkg of members) {
      expect(done.out).toContain(`${pkg.name} 0.1.0: its package is what a release of it carries`);
    }
  });

  it("checks what npm would publish of the SDK", () => {
    const w = world();
    expect(w.run("package", "sdk-typescript").status).toBe(0);
    expect(w.said("npm")).toEqual(["pack --dry-run --json --ignore-scripts=false"]);
    w.state.npm.files = w.state.npm.files.filter((file) => file !== "README.md");
    expect(w.run("package", "sdk-typescript").err).toContain("the package lacks README.md");
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
