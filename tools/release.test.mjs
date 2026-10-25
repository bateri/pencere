// The release tooling's rules that no other check sees. This file: the table of lines against the
// repository itself, `release.mjs prepare` (it moves a line's version(s) in the manifests and the
// lockfiles, and dates the changelog(s)), the refusals of `rehearse` that come before anything is
// asked of git, and the release notes (`notes`). What needs a repository with an origin
// and the world's answers (crates.io, GitHub, npm) is in `release.world.test.mjs`.

import { execFileSync } from "node:child_process";
import { existsSync, rmSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it, vi } from "vitest";

import { LINES } from "./lines.mjs";
import {
  CHANGELOG,
  RELEASED,
  cargoPackages,
  crateVersion,
  fixture,
  json,
  members,
  packages,
  release,
  repo,
  text,
  write,
} from "./release.fixture.mjs";

// Each test runs the script as a process, and some make a repository: on a busy machine that is
// more than the default five seconds.
vi.setConfig({ testTimeout: 30_000 });

const lockOf = (pkg) =>
  text(repo, pkg.manifest === undefined ? "Cargo.lock" : "kit/guest/Cargo.lock");

describe("the repository's lines", () => {
  it("are the three, and the crates are the six in the order they are published", () => {
    expect(Object.keys(LINES)).toEqual(["sdk-typescript", "crates", "cli"]);
    expect(members.map((pkg) => pkg.name)).toEqual([
      "pencere-wire",
      "pencere-host",
      "pencere-view",
      "pencere-sandbox",
      "pencere-page",
      "pencere",
    ]);
  });

  it("are the packages the lines say they are", () => {
    expect(json(repo, LINES["sdk-typescript"].dir, "package.json").name).toBe("@pencere/sdk");
    for (const pkg of cargoPackages) {
      expect(text(repo, pkg.dir, "Cargo.toml"), pkg.name).toMatch(
        new RegExp(`^\\[package\\]\\nname = "${pkg.name}"$`, "m"),
      );
      expect(crateVersion(repo, pkg.dir), `${pkg.name} has a version of its own`).toMatch(
        /^\d+\.\d+\.\d+/,
      );
    }
  });

  it("have the lockfile record each npm package at the version of its manifest", () => {
    const line = LINES["sdk-typescript"];
    const manifest = json(repo, line.dir, "package.json");
    expect(json(repo, "package-lock.json").packages[line.dir]?.version, line.name).toBe(
      manifest.version,
    );
  });

  it("have the lockfile of their workspace record each crate at the version of its manifest", () => {
    for (const pkg of cargoPackages) {
      const entry = new RegExp(`^name = "${pkg.name}"\\nversion = "([^"]+)"$`, "m").exec(
        lockOf(pkg),
      );
      expect(entry?.[1], `${pkg.name} in the lockfile`).toBe(crateVersion(repo, pkg.dir));
    }
  });

  it("carry what a release checks for: the repository's licence, a readme, a changelog, the entries", () => {
    for (const pkg of cargoPackages) {
      for (const name of ["LICENSE", "NOTICE"]) {
        expect(text(repo, pkg.dir, name), `${pkg.name}/${name}`).toBe(text(repo, name));
      }
      for (const file of ["README.md", "CHANGELOG.md", ...pkg.entries]) {
        expect(existsSync(join(repo, pkg.dir, file)), `${pkg.name}/${file}`).toBe(true);
      }
      expect(text(repo, pkg.dir, "CHANGELOG.md"), pkg.name).toMatch(/^## \[Unreleased\]$/m);
    }
  });

  it("are listed in the order the crates are published: each after the crates it depends on", () => {
    const names = members.map((pkg) => pkg.name);
    for (const pkg of members) {
      const own = text(repo, pkg.dir, "Cargo.toml");
      const section = /^\[dependencies\]\n([\s\S]*?)(?=^\[|(?![\s\S]))/m.exec(own)?.[1] ?? "";
      for (const dependency of names.filter((name) =>
        new RegExp(`^${name} *=`, "m").test(section),
      )) {
        expect(names.indexOf(dependency), `${dependency} before ${pkg.name}`).toBeLessThan(
          names.indexOf(pkg.name),
        );
      }
    }
  });

  it("have the crates ask for each other by the version each is at", () => {
    const manifests = execFileSync("git", ["ls-files", "*Cargo.toml"], {
      cwd: repo,
      encoding: "utf8",
    })
      .split("\n")
      .filter((file) => file !== "");
    for (const pkg of members) {
      const at = crateVersion(repo, pkg.dir);
      const asks = new RegExp(
        `^\\s*${pkg.name}\\s*=\\s*\\{[^}\\n]*\\bversion\\s*=\\s*"([^"]+)"`,
        "gm",
      );
      for (const file of manifests) {
        for (const [, wanted] of text(repo, file).matchAll(asks)) {
          expect(wanted, `${file} asks for ${pkg.name}`).toBe(at);
        }
      }
    }
  });

  it("have the crates that read the interface read their own copy of it", () => {
    for (const dir of ["kit/sandbox", "kit/guest/sdk"]) {
      expect(text(repo, dir, "wit/view.wit"), dir).toBe(text(repo, "kit/wit/view.wit"));
      expect(text(repo, dir, "Cargo.toml"), dir).toMatch(/"wit\/\*\*"/);
    }
    expect(text(repo, "kit/sandbox/src/world.rs")).toContain('path: "wit"');
    expect(text(repo, "kit/guest/sdk/src/lib.rs")).toContain('path: "wit"');
  });

  it("are the lines the release workflow can be started for, and the verbs the Makefile has", () => {
    const workflow = text(repo, ".github/workflows/release.yml");
    const options = /^\s+options:\n((?:\s+- .+\n)+)/m.exec(workflow)[1];
    const choices = [...options.matchAll(/- (\S+)/g)].map((match) => match[1]);
    expect(choices.sort()).toEqual(Object.keys(LINES).sort());
    // Each line has a job that publishes it, behind the environment a maintainer approves in.
    expect(workflow.match(/^ {4}environment: release$/gm)?.length).toBe(Object.keys(LINES).length);
    const makefile = text(repo, "Makefile");
    for (const verb of ["release-prepare", "rehearse"]) {
      expect(makefile, verb).toMatch(new RegExp(`^${verb}:`, "m"));
    }
    // A release is published by the workflow alone.
    for (const verb of ["release", "publish", "approve"]) {
      expect(makefile, verb).not.toMatch(new RegExp(`^${verb}:`, "m"));
    }
  });

  it("are the ones RELEASING.md names, with the commands it gives for the retired packages", () => {
    const releasing = text(repo, "RELEASING.md");
    for (const id of Object.keys(LINES)) expect(releasing, id).toContain(`${id}-v<x.y.z>`);
    for (const [name, message] of Object.entries(LINES.cli.retired)) {
      expect(releasing).toContain(`npm deprecate "${name}@*" ${JSON.stringify(message)}`);
    }
  });
});

describe("release.mjs prepare", () => {
  it("moves the version of an npm line in its manifest and the lockfile, and dates the changelog", () => {
    const root = fixture();
    const done = release(root, "prepare", "sdk-typescript", "0.2.0");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(json(root, "sdk/typescript", "package.json").version).toBe("0.2.0");
    expect(json(root, "package-lock.json").packages["sdk/typescript"].version).toBe("0.2.0");
    const changelog = text(root, "sdk/typescript/CHANGELOG.md");
    expect(changelog).toMatch(/## \[Unreleased\]\n\n## \[0\.2\.0\] - \d{4}-\d{2}-\d{2}\n/);
    expect(changelog).toContain("- a change of the SDK");
    expect(done.out).toContain('commit ("Release @pencere/sdk 0.2.0")');
    // The other lines are not moved with it.
    expect(crateVersion(root, "host/rust")).toBe("0.1.0");
    expect(text(root, "host/rust/CHANGELOG.md")).not.toContain("0.2.0");
  });

  describe("of the crates", () => {
    it("moves all six to one version, though one of them is there already, and nothing else", () => {
      const root = fixture();
      const done = release(root, "prepare", "crates", "0.2.0");
      expect(done.err).toBe("");
      expect(done.status).toBe(0);
      for (const pkg of members) {
        expect(crateVersion(root, pkg.dir), pkg.name).toBe("0.2.0");
        expect(text(root, pkg.dir, "CHANGELOG.md"), pkg.name).toMatch(
          /## \[Unreleased\]\n\n## \[0\.2\.0\] - \d{4}-\d{2}-\d{2}\n/,
        );
        expect(text(root, pkg.dir, "CHANGELOG.md"), pkg.name).toContain("- a change of");
      }
      // The workspace's version is the unpublished crates', and the command is its own line.
      expect(text(root, "Cargo.toml")).toContain('version = "0.0.0"');
      expect(crateVersion(root, "cli")).toBe("0.1.0");
      expect(text(root, "cli/CHANGELOG.md")).not.toContain("0.2.0");
      expect(done.out).toContain('commit ("Release the pencere crates 0.2.0")');
    });

    it("moves what the crates ask for of each other, in every manifest that asks", () => {
      const root = fixture();
      release(root, "prepare", "crates", "0.2.0");
      const asks = (name, path) => `${name} = { path = "${path}", version = "0.2.0" }`;
      expect(text(root, "host/rust/Cargo.toml")).toContain(asks("pencere-wire", "../../wire"));
      expect(text(root, "program/Cargo.toml")).toContain(asks("pencere-wire", "../wire"));
      expect(text(root, "kit/sandbox/Cargo.toml")).toContain(asks("pencere-view", "../view"));
      // The command's, which is not one of the crates, and the page modules' workspace.
      expect(text(root, "cli/Cargo.toml")).toContain(asks("pencere-wire", "../wire"));
      expect(text(root, "cli/Cargo.toml")).toContain(asks("pencere", "../program"));
      expect(text(root, "kit/guest/Cargo.toml")).toContain(asks("pencere-view", "../view"));
      expect(text(root, "kit/guest/Cargo.toml")).toContain(asks("pencere-wire", "../../wire"));
    });

    it("moves the lockfile of each workspace, the page modules' too", () => {
      const root = fixture();
      release(root, "prepare", "crates", "0.2.0");
      for (const name of [
        "pencere-wire",
        "pencere-host",
        "pencere-view",
        "pencere-sandbox",
        "pencere",
      ]) {
        expect(text(root, "Cargo.lock"), name).toContain(`name = "${name}"\nversion = "0.2.0"`);
      }
      expect(text(root, "Cargo.lock")).toContain('name = "pencere-cli"\nversion = "0.1.0"');
      for (const name of ["pencere-page", "pencere-view", "pencere-wire"]) {
        expect(text(root, "kit/guest/Cargo.lock"), name).toContain(
          `name = "${name}"\nversion = "0.2.0"`,
        );
      }
      // The root workspace does not hold the page SDK.
      expect(text(root, "Cargo.lock")).not.toContain("pencere-page");
    });

    it("gives a crate with nothing of its own the line that says so, and asks for nothing more", () => {
      const root = fixture();
      write(root, "host/rust/CHANGELOG.md", "# Changelog\n\n## [Unreleased]\n");
      write(
        root,
        "kit/view/CHANGELOG.md",
        "# Changelog\n\n## [Unreleased]\n\n## [0.1.0] - 2026-01-01\n\n- old\n",
      );
      const done = release(root, "prepare", "crates", "0.2.0");
      expect(done.err).toBe("");
      expect(done.status).toBe(0);
      const line = "Released with the other crates; no change of its own.";
      expect(text(root, "host/rust/CHANGELOG.md")).toMatch(
        new RegExp(
          `^# Changelog\\n\\n## \\[Unreleased\\]\\n\\n## \\[0\\.2\\.0\\] - \\d{4}-\\d{2}-\\d{2}\\n\\n${line}\\n$`,
        ),
      );
      expect(text(root, "kit/view/CHANGELOG.md")).toMatch(
        new RegExp(
          `## \\[0\\.2\\.0\\] - \\d{4}-\\d{2}-\\d{2}\\n\\n${line}\\n\\n## \\[0\\.1\\.0\\]`,
        ),
      );
      expect(text(root, "wire/CHANGELOG.md")).not.toContain(line);
    });

    it("refuses a version before the one a crate is at, and changes nothing", () => {
      const root = fixture();
      const done = release(root, "prepare", "crates", "0.1.5");
      expect(done.status).toBe(1);
      expect(done.err).toContain("0.1.5 is before 0.2.0, the version of one of the crates");
      expect(crateVersion(root, "wire")).toBe("0.1.0");
      expect(text(root, "wire/CHANGELOG.md")).toBe(CHANGELOG("a change of pencere-wire"));
    });

    it("refuses when no crate has anything unreleased, and when one already has the version", () => {
      const root = fixture();
      for (const pkg of members) {
        write(root, `${pkg.dir}/CHANGELOG.md`, "# Changelog\n\n## [Unreleased]\n");
      }
      const done = release(root, "prepare", "crates", "0.2.0");
      expect(done.status).toBe(1);
      expect(done.err).toContain("no crate has anything under Unreleased");
      expect(crateVersion(root, "wire")).toBe("0.1.0");

      const again = fixture();
      write(again, "kit/view/CHANGELOG.md", RELEASED.replace("0.1.0", "0.2.0"));
      const refused = release(again, "prepare", "crates", "0.2.0");
      expect(refused.status).toBe(1);
      expect(refused.err).toContain("kit/view/CHANGELOG.md already has a section for 0.2.0");
      expect(crateVersion(again, "wire")).toBe("0.1.0");
    });
  });

  it("moves the version of the command in its crate's manifest and the lockfile, and tells what comes next", () => {
    const root = fixture();
    const done = release(root, "prepare", "cli", "0.2.0");
    expect(done.err).toBe("");
    expect(done.status).toBe(0);
    expect(crateVersion(root, "cli")).toBe("0.2.0");
    expect(text(root, "Cargo.lock")).toContain('name = "pencere-cli"\nversion = "0.2.0"');
    expect(text(root, "cli/CHANGELOG.md")).toMatch(/## \[0\.2\.0\] - \d{4}-\d{2}-\d{2}/);
    expect(done.out).toContain('commit ("Release pencere 0.2.0") and push to main');
    // No crate moved with it.
    expect(crateVersion(root, "wire")).toBe("0.1.0");
    expect(text(root, "program/Cargo.toml")).toContain('version = "0.1.0" }');
  });

  it("refuses a version that is not after the line's, and changes nothing", () => {
    const root = fixture();
    const done = release(root, "prepare", "sdk-typescript", "0.1.0");
    expect(done.status).toBe(1);
    expect(done.err).toContain("0.1.0 is not after 0.1.0");
    expect(json(root, "sdk/typescript", "package.json").version).toBe("0.1.0");
    expect(text(root, "sdk/typescript/CHANGELOG.md")).toBe(CHANGELOG("a change of the SDK"));
  });

  it("refuses a line with nothing unreleased, and a version that is not one", () => {
    const root = fixture();
    write(root, "sdk/typescript/CHANGELOG.md", "# Changelog\n\n## [Unreleased]\n");
    const done = release(root, "prepare", "sdk-typescript", "0.2.0");
    expect(done.status).toBe(1);
    expect(done.err).toContain("Unreleased is empty, nothing to release");
    expect(json(root, "sdk/typescript", "package.json").version).toBe("0.1.0");
    expect(release(root, "prepare", "cli", "two").err).toContain('"two" is not a version');
  });
});

describe("release.mjs rehearse", () => {
  const refused = (root, id, said) => {
    const done = release(root, "rehearse", id);
    expect(done.status).toBe(1);
    expect(done.err).toContain(said);
  };

  it("refuses a line that was never prepared", () => {
    const root = fixture();
    write(
      root,
      "cli/Cargo.toml",
      text(root, "cli/Cargo.toml").replace('version = "0.1.0"', 'version = "0.0.0"'),
    );
    refused(root, "cli", "pencere is at 0.0.0: run make release-prepare first");
  });

  it.each(["sdk-typescript", "cli"])(
    "refuses an Unreleased section that still holds changes (%s)",
    (id) => {
      refused(fixture(), id, "Unreleased is not empty; prepare moves it into the release");
    },
  );

  it("refuses an Unreleased section that still holds changes in any of the crates", () => {
    const root = fixture({ programVersion: "0.1.0" });
    for (const pkg of members.slice(0, -1)) write(root, `${pkg.dir}/CHANGELOG.md`, RELEASED);
    refused(root, "crates", "program/CHANGELOG.md: Unreleased is not empty");
  });

  it("refuses crates that are not at one version, and names them", () => {
    const root = fixture();
    refused(root, "crates", "crates are not at one version: pencere-wire 0.1.0");
    expect(release(root, "rehearse", "crates").err).toContain("pencere 0.2.0");
  });

  it("refuses a line whose changelog is not there", () => {
    const root = fixture();
    rmSync(join(root, "sdk/typescript/CHANGELOG.md"));
    refused(root, "sdk-typescript", "sdk/typescript/CHANGELOG.md is missing");
  });

  it("refuses a manifest whose version is not the changelog's newest section", () => {
    const root = fixture();
    write(
      root,
      "cli/CHANGELOG.md",
      "# Changelog\n\n## [Unreleased]\n\n## [0.2.0] - 2026-01-01\n\n- x\n",
    );
    refused(root, "cli", "cli/CHANGELOG.md: the newest section is not 0.1.0");
  });

  it("refuses a manifest with no released section at all", () => {
    const root = fixture();
    write(root, "cli/CHANGELOG.md", "# Changelog\n\n## [Unreleased]\n");
    refused(root, "cli", "the newest section is not 0.1.0");
  });

  it("refuses an empty section of the version", () => {
    const root = fixture();
    write(root, "cli/CHANGELOG.md", "# Changelog\n\n## [Unreleased]\n\n## [0.1.0] - 2026-01-01\n");
    refused(root, "cli", "the section of 0.1.0 is empty");
  });

  it("knows the lines and refuses others", () => {
    const root = fixture();
    for (const old of ["wire", "view", "host-rust", "program", "page", "sandbox"]) {
      const done = release(root, "rehearse", old);
      expect(done.status, old).toBe(1);
      expect(done.err).toContain(
        `unknown line "${old}"; the lines are sdk-typescript, crates, cli`,
      );
    }
  });

  it("has no command to publish from here", () => {
    for (const old of ["local", "publish-local", "push", "approve", "check", "verify-tag"]) {
      const done = release(fixture(), old, "crates");
      expect(done.status).toBe(2);
      expect(done.err).toContain("commands are");
    }
  });
});

describe("release.mjs notes", () => {
  /** The crates released at 0.1.0, all with something to say but the ones named. */
  function released(silent = []) {
    const root = fixture({ programVersion: "0.1.0" });
    for (const pkg of packages) {
      const quiet = silent.includes(pkg.name);
      write(
        root,
        `${pkg.dir}/CHANGELOG.md`,
        quiet
          ? "# Changelog\n\n## [Unreleased]\n\n## [0.1.0] - 2026-01-01\n\nReleased with the other crates; no change of its own.\n"
          : RELEASED,
      );
    }
    return root;
  }

  it("print the changelog section of the version the manifest is at", () => {
    const root = released();
    const done = release(root, "notes", "sdk-typescript");
    expect(done.status).toBe(0);
    expect(done.out).toBe("### Added\n\n- a change\n");
    expect(release(root, "notes", "cli", "0.1.0").out).toBe("### Added\n\n- a change\n");
  });

  it("print the crates' as a section for each under its name, and name the ones released with no change", () => {
    const root = released(["pencere-host", "pencere-view"]);
    const done = release(root, "notes", "crates");
    expect(done.status).toBe(0);
    expect(done.out).toContain("## pencere-wire\n\n### Added\n\n- a change\n");
    expect(done.out).toContain("## pencere-sandbox\n\n### Added");
    expect(done.out).not.toContain("## pencere-host");
    expect(done.out).not.toContain("## pencere-view");
    expect(done.out.trim().split("\n").at(-1)).toBe(
      "Released with no change of their own: `pencere-host`, `pencere-view`.",
    );
  });
});
