// Releases a package of this repository: one script for every step that can be
// checked by a machine, so a release is the same sequence every time.
//
// Each published package is a line with its own version, CHANGELOG.md and tags
// (`<line>-v<version>`); RELEASING.md is the procedure, this file the checks.
//
//   prepare <line> <version>  set the version and move Unreleased into a dated section
//   check <line>              everything a release needs, without changing anything
//   tag <line>                check, then create the annotated tag
//   push <line>               push main and the tag together (what starts the workflow)
//   local <line>              publish from this machine, then push (a line's first release)
//   verify-tag <tag>          in the workflow: the tag names the line's version on main
//   registry <line> <version> in the workflow: is the package, and this version, published
//   notes <line> [<version>]  the CHANGELOG section of a version
//
// It uses nothing but Node, git and the package managers already needed by the gate.

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const LINES = {
  "sdk-typescript": { dir: "sdk/typescript", kind: "npm", name: "@pencere/sdk" },
  "host-rust": { dir: "host/rust", kind: "cargo", name: "pencere" },
};

/** The files a package must carry, beyond what its manager adds by itself. */
const REQUIRED = {
  npm: ["LICENSE", "README.md", "CHANGELOG.md", "package.json", "dist/index.js", "dist/index.d.ts"],
  cargo: ["LICENSE", "README.md", "CHANGELOG.md", "Cargo.toml", "src/lib.rs"],
};

const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z.-]+))?$/;
const RELEASED = /^## \[(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)\] - (\d{4}-\d{2}-\d{2})$/;

class Refusal extends Error {}
const refuse = (what) => {
  throw new Refusal(what);
};

/** Runs a command and returns its output; one run with `stdio: "inherit"` returns "". */
function run(command, args, options = {}) {
  const output = execFileSync(command, args, {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    ...options,
  });
  return (output ?? "").trim();
}

function succeeds(command, args, options = {}) {
  try {
    run(command, args, options);
    return true;
  } catch {
    return false;
  }
}

function lineOf(name) {
  const line = LINES[name];
  if (line === undefined) {
    refuse(`unknown line "${name}"; the lines are ${Object.keys(LINES).join(", ")}`);
  }
  return { id: name, ...line };
}

const tagOf = (line, version) => `${line.id}-v${version}`;

// ---- the manifest's version ------------------------------------------------

function manifestPath(line) {
  return join(root, line.dir, line.kind === "npm" ? "package.json" : "Cargo.toml");
}

function versionOf(line) {
  const text = readFileSync(manifestPath(line), "utf8");
  if (line.kind === "npm") return JSON.parse(text).version;
  // The crate's version is its own line in [package], not the workspace's.
  const match = /^\[package\][^[]*?^version = "([^"]+)"$/ms.exec(text);
  if (match === null) refuse(`${line.dir}/Cargo.toml: no version of its own in [package]`);
  return match[1];
}

function setVersion(line, version) {
  const path = manifestPath(line);
  const text = readFileSync(path, "utf8");
  if (line.kind === "npm") {
    const manifest = JSON.parse(text);
    manifest.version = version;
    writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`);
    // The workspace's lockfile records the package's version too.
    const lockPath = join(root, "package-lock.json");
    const lock = JSON.parse(readFileSync(lockPath, "utf8"));
    const entry = lock.packages?.[line.dir];
    if (entry === undefined) refuse(`package-lock.json has no entry for ${line.dir}`);
    entry.version = version;
    writeFileSync(lockPath, `${JSON.stringify(lock, null, 2)}\n`);
  } else {
    const next = text.replace(
      /^(\[package\][^[]*?^version = ")[^"]+(")$/ms,
      (_, before, after) => `${before}${version}${after}`,
    );
    writeFileSync(path, next);
    run("cargo", ["update", "--workspace", "--offline"]);
  }
}

// ---- the changelog ---------------------------------------------------------

function changelogPath(line) {
  return join(root, line.dir, "CHANGELOG.md");
}

/** The changelog as its heading lines and the sections below them. */
function sections(line) {
  const lines = readFileSync(changelogPath(line), "utf8").split("\n");
  const found = [];
  lines.forEach((text, index) => {
    if (text.startsWith("## ")) found.push({ heading: text, start: index });
  });
  return found.map((section, i) => ({
    ...section,
    end: i + 1 < found.length ? found[i + 1].start : lines.length,
    body: lines
      .slice(section.start + 1, i + 1 < found.length ? found[i + 1].start : lines.length)
      .join("\n")
      .trim(),
  }));
}

function unreleased(line) {
  const first = sections(line)[0];
  if (first === undefined || first.heading !== "## [Unreleased]") {
    refuse(`${line.dir}/CHANGELOG.md: the first section must be "## [Unreleased]"`);
  }
  return first;
}

function releasedSection(line, version) {
  const section = sections(line).find((s) => RELEASED.exec(s.heading)?.[1] === version);
  if (section === undefined) {
    refuse(`${line.dir}/CHANGELOG.md: no "## [${version}] - YYYY-MM-DD" section`);
  }
  if (section.body === "") refuse(`${line.dir}/CHANGELOG.md: the section of ${version} is empty`);
  return section;
}

function compare(a, b) {
  const pa = SEMVER.exec(a);
  const pb = SEMVER.exec(b);
  for (let i = 1; i <= 3; i++) {
    const d = Number(pa[i]) - Number(pb[i]);
    if (d !== 0) return d;
  }
  // A pre-release sorts before the release it leads to.
  if (pa[4] === undefined) return pb[4] === undefined ? 0 : 1;
  if (pb[4] === undefined) return -1;
  return pa[4] < pb[4] ? -1 : pa[4] > pb[4] ? 1 : 0;
}

// ---- commands ----------------------------------------------------------------

function prepare(name, version) {
  const line = lineOf(name);
  if (!SEMVER.test(version ?? "")) refuse(`"${version}" is not a version (x.y.z)`);
  const current = versionOf(line);
  if (compare(version, current) <= 0) refuse(`${version} is not after ${current}`);
  const top = unreleased(line);
  if (top.body === "") refuse(`${line.dir}/CHANGELOG.md: Unreleased is empty, nothing to release`);
  if (sections(line).some((s) => RELEASED.exec(s.heading)?.[1] === version)) {
    refuse(`${line.dir}/CHANGELOG.md already has a section for ${version}`);
  }
  const date = new Date().toISOString().slice(0, 10);
  const lines = readFileSync(changelogPath(line), "utf8").split("\n");
  lines.splice(top.start, 1, "## [Unreleased]", "", `## [${version}] - ${date}`);
  writeFileSync(changelogPath(line), lines.join("\n"));
  setVersion(line, version);
  console.log(`${line.name} ${current} -> ${version}`);
  console.log(
    `next: review, commit ("Release ${line.name} ${version}"), then make release LINE=${line.id}`,
  );
}

function check(name) {
  const line = lineOf(name);
  const version = versionOf(line);
  const tag = tagOf(line, version);

  if (version === "0.0.0") refuse(`${line.name} is at 0.0.0: run make release-prepare first`);
  if (unreleased(line).body !== "") {
    refuse(`${line.dir}/CHANGELOG.md: Unreleased is not empty; prepare moves it into the release`);
  }
  const section = sections(line).find((s) => RELEASED.test(s.heading));
  if (section === undefined || RELEASED.exec(section.heading)[1] !== version) {
    refuse(`${line.dir}/CHANGELOG.md: the newest section is not ${version}`);
  }
  releasedSection(line, version);

  if (run("git", ["branch", "--show-current"]) !== "main") refuse("not on main");
  if (run("git", ["status", "--porcelain"]) !== "") refuse("the working tree is not clean");
  run("git", ["fetch", "--quiet", "origin", "main", "--tags"]);
  if (!succeeds("git", ["merge-base", "--is-ancestor", "origin/main", "HEAD"])) {
    refuse("main has diverged from origin/main");
  }
  if (succeeds("git", ["rev-parse", "--verify", "--quiet", `refs/tags/${tag}`])) {
    refuse(`the tag ${tag} exists`);
  }
  const released = run("git", ["tag", "--list", `${line.id}-v*`])
    .split("\n")
    .map((name) => name.slice(line.id.length + 2))
    .filter((v) => SEMVER.test(v));
  const last = released.sort(compare).at(-1);
  if (last !== undefined && compare(version, last) <= 0) {
    refuse(`${version} is not after ${last}, the line's last release`);
  }
  if (run("git", ["ls-remote", "--tags", "origin", `refs/tags/${tag}`]) !== "") {
    refuse(`the tag ${tag} exists on origin`);
  }
  if (
    readFileSync(join(root, line.dir, "LICENSE"), "utf8") !==
    readFileSync(join(root, "LICENSE"), "utf8")
  ) {
    refuse(`${line.dir}/LICENSE is not the repository's LICENSE`);
  }

  const files = contents(line);
  for (const file of REQUIRED[line.kind]) {
    if (!files.includes(file)) refuse(`the package lacks ${file}`);
  }
  const stray = files.filter((file) => file.startsWith("test/") || file.startsWith("tests/"));
  if (stray.length > 0) refuse(`the package carries tests: ${stray.join(", ")}`);
  if (line.kind === "cargo") {
    run("cargo", ["publish", "--dry-run", "-p", line.name]);
  }
  console.log(`${line.name} ${version} is ready to tag as ${tag}`);
  return { line, version, tag, section };
}

/** The files the package manager would publish, as paths inside the package. */
function contents(line) {
  if (line.kind === "npm") {
    // `npm pack` runs the package's clean build (`prepack`) first, even where npm is
    // told to skip scripts, so this is what would ship.
    const report = JSON.parse(
      run("npm", ["pack", "--dry-run", "--json", "--ignore-scripts=false"], {
        cwd: join(root, line.dir),
      }),
    );
    return report[0].files.map((file) => file.path);
  }
  return run("cargo", ["package", "--list", "-p", line.name]).split("\n");
}

function tag(name) {
  const { tag: tagName, section, line, version } = check(name);
  // Verbatim: the default cleanup would take the section's "### Added" lines for comments.
  const message = `${line.name} ${version}\n\n${section.body}\n`;
  run("git", ["tag", "--annotate", "--cleanup=verbatim", tagName, "--message", message]);
  console.log(`tagged ${tagName}; nothing is pushed. Next: make publish LINE=${line.id}`);
}

function currentTag(line) {
  const version = versionOf(line);
  const tagName = tagOf(line, version);
  if (!succeeds("git", ["rev-parse", "--verify", "--quiet", `refs/tags/${tagName}`])) {
    refuse(`no tag ${tagName}: run make release LINE=${line.id} first`);
  }
  if (run("git", ["rev-list", "-n", "1", tagName]) !== run("git", ["rev-parse", "HEAD"])) {
    refuse(`${tagName} is not on HEAD`);
  }
  return { version, tagName };
}

function push(name) {
  const line = lineOf(name);
  if (line.kind !== "npm") {
    refuse(`${line.id} is published from this machine: make publish-local LINE=${line.id}`);
  }
  const { tagName } = currentTag(line);
  run("git", ["push", "--atomic", "origin", "main", tagName], { stdio: "inherit" });
  console.log(`pushed main and ${tagName}; the release workflow takes it from here`);
}

function local(name) {
  const line = lineOf(name);
  const { tagName } = currentTag(line);
  const cwd = join(root, line.dir);
  if (line.kind === "npm") {
    run("npm", ["publish"], { cwd, stdio: "inherit" });
  } else {
    run("cargo", ["publish", "-p", line.name], { stdio: "inherit" });
  }
  run("git", ["push", "--atomic", "origin", "main", tagName], { stdio: "inherit" });
  console.log(`published ${line.name} and pushed ${tagName}`);
}

function verifyTag(tagName) {
  const match = /^(.+)-v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)$/.exec(tagName ?? "");
  if (match === null) refuse(`"${tagName}" is not a release tag (<line>-v<version>)`);
  const line = lineOf(match[1]);
  const version = match[2];
  if (versionOf(line) !== version) {
    refuse(`${tagName} names ${version} but ${line.dir} is at ${versionOf(line)}`);
  }
  releasedSection(line, version);
  run("git", ["fetch", "--quiet", "origin", "main"]);
  if (!succeeds("git", ["merge-base", "--is-ancestor", tagName, "origin/main"])) {
    refuse(`${tagName} is not on main`);
  }
  // Read by the workflow from its output file.
  console.log(`line=${line.id}`);
  console.log(`version=${version}`);
}

function registry(name, version) {
  const line = lineOf(name);
  if (line.kind !== "npm") refuse(`${line.id} is not published by the workflow`);
  const exists = succeeds("npm", ["view", line.name, "name"]);
  const published = exists && succeeds("npm", ["view", `${line.name}@${version}`, "version"]);
  // Read by the workflow from its output file.
  console.log(`exists=${exists}`);
  console.log(`published=${published}`);
}

function notes(name, version) {
  const line = lineOf(name);
  console.log(releasedSection(line, version ?? versionOf(line)).body);
}

const COMMANDS = {
  prepare,
  check,
  tag,
  push,
  local,
  "verify-tag": verifyTag,
  registry,
  notes,
};

const [command, ...args] = process.argv.slice(2);
const handler = COMMANDS[command];
if (handler === undefined) {
  console.error(`release: commands are ${Object.keys(COMMANDS).join(", ")}`);
  process.exit(2);
}
try {
  handler(...args);
} catch (error) {
  if (error instanceof Refusal) {
    console.error(`release: ${error.message}`);
    process.exit(1);
  }
  throw error;
}
