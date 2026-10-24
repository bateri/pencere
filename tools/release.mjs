// Releases a package of this repository: one script for every step that can be
// checked by a machine, so a release is the same sequence every time.
//
// Each published package is a line with its own version, CHANGELOG.md and tags
// (`<line>-v<version>`); RELEASING.md is the procedure, this file the checks.
//
//   prepare <line> <version>  set the version and move Unreleased into a dated section
//   check <line>              everything a release needs, without changing anything
//   tag <line>                check, then create the annotated tag
//   push <line>               npm: push main and the tag together (what starts the workflow);
//                             binary: the same, then make the GitHub release with its files
//   local <line>              npm and cargo: publish from this machine, then push
//   approve <line>            make public the version of the line that the workflow staged on npm
//   dist <line>               a binary line: the files in dist/ are this version's, and intact
//   verify-tag <tag>          in the workflow: the tag names the line's version on main; prints
//                             the line, its package, directory and kind, and the version
//   registry <line> <version> in the workflow: is the package, and this version, published
//   notes <line> [<version>]  the CHANGELOG section of a version
//
// A line is released by its kind (tools/lines.mjs): `npm` is staged by the workflow and approved
// on npm, `cargo` is published from this machine with `cargo publish`, `binary` is built into
// dist/ by `make dist` and attached to its GitHub release from this machine. In the last two the
// workflow only verifies the tag and runs the gate; it writes the GitHub release of a crate, not
// of a binary.
//
// It uses nothing but Node, git and the package managers already needed by the gate (and `gh`
// for a binary line).

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { LINES } from "./lines.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** The files a package must carry, beyond what its manager adds by itself and the line's own entries. */
const REQUIRED = {
  npm: ["LICENSE", "NOTICE", "README.md", "CHANGELOG.md", "package.json"],
  cargo: ["LICENSE", "NOTICE", "README.md", "CHANGELOG.md", "Cargo.toml"],
  // A binary has no package to list: these are the files of its directory that git holds.
  binary: ["LICENSE", "NOTICE", "README.md", "CHANGELOG.md", "Cargo.toml"],
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
    const moved = moveRequirements(line, version);
    // The lockfile of each workspace records the crate's version (the page modules' workspace
    // depends on the engine and the wire by path).
    for (const manifest of ["Cargo.toml", "kit/guest/Cargo.toml"]) {
      if (existsSync(join(root, manifest))) {
        run("cargo", ["update", "--workspace", "--offline", "--manifest-path", manifest]);
      }
    }
    return moved;
  }
  return [];
}

/** The `Cargo.toml` files of the repository, apart from the builds' and the installs'. */
function cargoManifests(dir = root) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    if (entry.isDirectory()) {
      return ["target", "node_modules", ".git"].includes(entry.name)
        ? []
        : cargoManifests(join(dir, entry.name));
    }
    return entry.name === "Cargo.toml" ? [join(dir, entry.name)] : [];
  });
}

/** Does `version` meet the requirement `req`, which cargo reads as `^req` when it is a bare version? */
function meets(version, req) {
  const match = /^\^?(\d+)\.(\d+)\.(\d+)$/.exec(req);
  if (match === null) return true; // a form this does not read is left as it is
  // A pre-release meets no requirement that does not name a pre-release, whatever its numbers.
  if (version.includes("-")) return false;
  const [major, minor, patch] = match.slice(1).map(Number);
  const [vMajor, vMinor, vPatch] = SEMVER.exec(version).slice(1, 4).map(Number);
  if (compare(version, `${major}.${minor}.${patch}`) < 0) return false;
  if (major > 0) return vMajor === major;
  if (minor > 0) return vMajor === 0 && vMinor === minor;
  return vMajor === 0 && vMinor === 0 && vPatch === patch;
}

/**
 * A crate that another asks for by path and version (`name = { path = "…", version = "0.1.0" }`)
 * is not found by that other once its version has gone past what the requirement allows (a `0.2.0`
 * after `0.1.0`), and cargo then fails to resolve the workspace. The requirement moves with the
 * release, and the crates that ask are listed: they are the next releases.
 */
function moveRequirements(line, version) {
  const name = line.name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const pattern = new RegExp(
    `^(\\s*${name}\\s*=\\s*\\{[^}\\n]*\\bversion\\s*=\\s*")([^"]+)(")`,
    "gm",
  );
  const moved = [];
  for (const path of cargoManifests()) {
    const text = readFileSync(path, "utf8");
    const next = text.replace(pattern, (whole, before, req, after) => {
      if (meets(version, req)) return whole;
      moved.push({ file: path.slice(root.length + 1), from: req });
      return `${before}${version}${after}`;
    });
    if (next !== text) writeFileSync(path, next);
  }
  return moved;
}

// ---- the changelog ---------------------------------------------------------

function changelogPath(line) {
  return join(root, line.dir, "CHANGELOG.md");
}

/** The changelog as its heading lines and the sections below them. */
function sections(line) {
  if (!existsSync(changelogPath(line))) refuse(`${line.dir}/CHANGELOG.md is missing`);
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
  const moved = setVersion(line, version);
  console.log(`${line.name} ${current} -> ${version}`);
  for (const { file, from } of moved) {
    console.log(
      `${file} asked for ${line.name} ${from}: it asks for ${version} now (its release is next)`,
    );
  }
  const next =
    line.kind === "binary"
      ? `make dist, then make release LINE=${line.id}`
      : `make release LINE=${line.id}`;
  console.log(`next: review, commit ("Release ${line.name} ${version}"), then ${next}`);
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
  for (const name of ["LICENSE", "NOTICE"]) {
    const copy = join(root, line.dir, name);
    if (
      !existsSync(copy) ||
      readFileSync(copy, "utf8") !== readFileSync(join(root, name), "utf8")
    ) {
      refuse(`${line.dir}/${name} is not the repository's ${name}`);
    }
  }

  const files = contents(line);
  for (const file of [...REQUIRED[line.kind], ...line.entries]) {
    if (!files.includes(file))
      refuse(`${line.kind === "binary" ? line.dir : "the package"} lacks ${file}`);
  }
  // A binary's directory is a crate that is not packaged: its tests are not shipped.
  const stray = line.kind === "binary" ? [] : files.filter(isStray);
  if (stray.length > 0) refuse(`the package carries tests or builds: ${stray.join(", ")}`);
  if (line.kind === "cargo") dryRun(line);
  if (line.kind === "binary") {
    checkDist(line, version);
    console.log(`dist/ holds the ${line.targets.length} binaries of ${version} and their sums`);
  }
  console.log(`${line.name} ${version} is ready to tag as ${tag}`);
  return { line, version, tag, section };
}

/** What a package must not carry: tests (also the modules of a crate's `src/`), builds and installs. */
const isStray = (file) =>
  /^(test|tests|target)\//.test(file) ||
  /(^|\/)node_modules\//.test(file) ||
  /(^|\/)(tests|[^/]*_tests)\.rs$/.test(file);

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
  if (line.kind === "binary") {
    // Nothing is packaged: what the line ships is the files of its directory that git holds.
    return run("git", ["ls-files", "--", line.dir])
      .split("\n")
      .filter((file) => file !== "")
      .map((file) => file.slice(line.dir.length + 1));
  }
  return run("cargo", ["package", "--list", "-p", line.name, ...cargoArgs(line)]).split("\n");
}

/** The workspace a crate's cargo commands run against, when it is not the repository's own. */
const cargoArgs = (line) => (line.manifest === undefined ? [] : ["--manifest-path", line.manifest]);

// ---- the crates a crate depends on -----------------------------------------

/** What cargo knows of a crate from its manifest, in its workspace. */
function crateOf(line) {
  const metadata = JSON.parse(
    run("cargo", ["metadata", "--no-deps", "--format-version", "1", ...cargoArgs(line)]),
  );
  const crate = metadata.packages.find((p) => p.name === line.name);
  if (crate === undefined) {
    refuse(`cargo does not list ${line.name} in ${line.manifest ?? "the workspace"}`);
  }
  return crate;
}

/**
 * What crates.io refuses a crate for (no description, no licence, publishing turned off) and what
 * this repository's crates all say (where the code is, a readme, keywords and a category).
 */
function manifestGaps(crate) {
  const gaps = [];
  if (Array.isArray(crate.publish) && crate.publish.length === 0)
    gaps.push("publish (it is false)");
  if (!crate.description) gaps.push("description");
  if (!crate.license && !crate.license_file) gaps.push("license");
  if (!crate.repository) gaps.push("repository");
  if (!crate.readme) gaps.push("readme");
  if (crate.keywords.length === 0 || crate.keywords.length > 5) gaps.push("keywords (one to five)");
  if (crate.categories.length === 0) gaps.push("categories");
  return gaps;
}

/**
 * The crates that a crate depends on by path (normal and build dependencies), each with the
 * version it asks for. A published crate asks for them by that version.
 */
function pathDependencies(crate) {
  return crate.dependencies
    .filter((dep) => dep.path != null && dep.kind !== "dev")
    .map((dep) => {
      const version = dep.req.replace(/^[\^=]/, "");
      if (!SEMVER.test(version)) {
        refuse(
          `${crate.name} asks for ${dep.name} by path with the version "${dep.req}": a published crate needs one version, x.y.z`,
        );
      }
      return { name: dep.name, version };
    });
}

/** Is this version of the crate on crates.io? */
function onRegistry(name, version) {
  try {
    // From outside the repository: inside it, `cargo info` answers from the workspace's own crate.
    run("cargo", ["info", `${name}@${version}`], { cwd: tmpdir() });
    return true;
  } catch (error) {
    const said = String(error.stderr ?? error.message).trim();
    if (said.includes("could not find")) return false;
    refuse(`cannot ask crates.io about ${name} ${version}: ${said.split("\n").at(-1)}`);
  }
}

/** The crates a crate depends on that crates.io does not have yet: they are published first. */
function unpublished(line, crate = crateOf(line)) {
  const crates = Object.values(LINES)
    .filter((other) => other.kind === "cargo")
    .map((other) => other.name);
  const missing = [];
  for (const dep of pathDependencies(crate)) {
    if (!crates.includes(dep.name)) {
      refuse(
        `${line.name} depends on ${dep.name} by path, and no line of this repository publishes it`,
      );
    }
    if (!onRegistry(dep.name, dep.version)) missing.push(dep);
  }
  return missing;
}

/**
 * A dry run of the publication builds the package against what crates.io has, and cargo cannot
 * make the package of a crate that asks for one crates.io does not have (not even without the
 * build), so the full dry run can only be made once the crates this one depends on are there:
 * RELEASING.md orders the crates for that. Until then what is checked is the list of files the
 * package would carry (made before this) and the manifest, and the output says that is all.
 */
function dryRun(line) {
  const crate = crateOf(line);
  const gaps = manifestGaps(crate);
  if (gaps.length > 0) refuse(`${line.dir}/Cargo.toml lacks: ${gaps.join(", ")}`);
  const missing = unpublished(line, crate);
  if (missing.length === 0) {
    run("cargo", ["publish", "--dry-run", "-p", line.name, ...cargoArgs(line)]);
    return;
  }
  const names = missing.map((dep) => `${dep.name} ${dep.version}`).join(", ");
  console.log(
    `note: ${names} is not on crates.io yet, so ${line.name} was not dry-run (cargo publish --dry-run builds the package against crates.io): its files and its manifest are checked, and nothing else. Publish ${names} first to have the full dry run.`,
  );
}

// ---- the binaries of a binary line -----------------------------------------

/** What the first bytes of an executable say: its system and processor, or nothing if it is neither kind. */
function executableFor(bytes) {
  if (bytes.length >= 20 && bytes.readUInt32BE(0) === 0x7f454c46) {
    // ELF: 64 bits (4) and little-endian (5), the machine at 18.
    const machine = bytes.readUInt16LE(18);
    const known = bytes[4] === 2 && bytes[5] === 1;
    return {
      system: "linux",
      processor: known ? { 183: "aarch64", 62: "x86_64" }[machine] : undefined,
    };
  }
  if (bytes.length >= 8 && bytes.readUInt32LE(0) === 0xfeedfacf) {
    // Mach-O, 64 bits, little-endian: the processor at 4.
    const cpu = bytes.readUInt32LE(4);
    return { system: "macos", processor: { 0x0100000c: "aarch64", 0x01000007: "x86_64" }[cpu] };
  }
  return { system: undefined, processor: undefined };
}

/**
 * The files `make dist` leaves in dist/ for the version, and that they are what they say: one
 * executable for each target of the line (the right system and processor for its name),
 * SHA256SUMS naming these and no others, with the sums of the files as they are. Returns the
 * paths the GitHub release is made of.
 */
function checkDist(line, version) {
  const dir = join(root, "dist");
  if (!existsSync(dir)) refuse("dist/ is missing: make dist builds it");
  const names = line.targets.map((target) => `${line.binary}-${version}-${target}`);
  const bytes = new Map();
  for (const [index, name] of names.entries()) {
    const path = join(dir, name);
    if (!existsSync(path) || !statSync(path).isFile())
      refuse(`dist/${name} is missing: make dist builds it`);
    const content = readFileSync(path);
    if (content.length === 0) refuse(`dist/${name} is empty`);
    const target = line.targets[index];
    const { system, processor } = executableFor(content);
    if (system === undefined) refuse(`dist/${name} is not an executable (neither ELF nor Mach-O)`);
    const want = [target.includes("apple-darwin") ? "macos" : "linux", target.split("-")[0]];
    if (system !== want[0] || processor !== want[1]) {
      refuse(
        `dist/${name} is a ${system} executable for ${processor ?? "an unknown processor"}, not for ${target}`,
      );
    }
    bytes.set(name, content);
  }

  const sums = join(dir, "SHA256SUMS");
  if (!existsSync(sums)) refuse("dist/SHA256SUMS is missing: make dist writes it");
  const listed = new Map();
  for (const text of readFileSync(sums, "utf8").split("\n")) {
    if (text === "") continue;
    const match = /^([0-9a-f]{64}) [ *](\S+)$/.exec(text);
    if (match === null) refuse(`dist/SHA256SUMS: "${text}" is not "<sha256>  <file>"`);
    if (listed.has(match[2])) refuse(`dist/SHA256SUMS names ${match[2]} twice`);
    listed.set(match[2], match[1]);
  }
  for (const name of listed.keys()) {
    if (!names.includes(name)) {
      refuse(
        `dist/SHA256SUMS names ${name}, which is not a file of ${line.binary} ${version}: is it the sums of another version?`,
      );
    }
  }
  for (const name of names) {
    const sum = listed.get(name);
    if (sum === undefined) refuse(`dist/SHA256SUMS does not name ${name}`);
    if (createHash("sha256").update(bytes.get(name)).digest("hex") !== sum) {
      refuse(`dist/${name} is not the file that dist/SHA256SUMS has the sum of`);
    }
  }
  return [...names.map((name) => join(dir, name)), sums];
}

function dist(name) {
  const line = lineOf(name);
  if (line.kind !== "binary") refuse(`${line.id} has no dist/: it is released as ${line.kind}`);
  const version = versionOf(line);
  for (const path of checkDist(line, version)) console.log(path.slice(root.length + 1));
}

function tag(name) {
  const { tag: tagName, section, line, version } = check(name);
  // Verbatim: the default cleanup would take the section's "### Added" lines for comments.
  const message = `${line.name} ${version}\n\n${section.body}\n`;
  run("git", ["tag", "--annotate", "--cleanup=verbatim", tagName, "--message", message]);
  // The workflow cannot stage a package the registry does not know yet, nor publish a crate, and
  // a binary's release is made from this machine with its files.
  const next =
    line.kind === "binary" || (line.kind === "npm" && succeeds("npm", ["view", line.name, "name"]))
      ? "publish"
      : "publish-local";
  console.log(`tagged ${tagName}; nothing is pushed. Next: make ${next} LINE=${line.id}`);
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
  if (line.kind === "cargo") {
    refuse(`${line.id} is published from this machine: make publish-local LINE=${line.id}`);
  }
  const { version, tagName } = currentTag(line);
  if (line.kind === "binary") return pushBinary(line, version, tagName);
  run("git", ["push", "--atomic", "origin", "main", tagName], { stdio: "inherit" });
  console.log(`pushed main and ${tagName}; the release workflow takes it from here`);
}

/**
 * The release of a binary line: the tag is pushed (the workflow then verifies it and runs the
 * gate, and writes nothing), and the GitHub release is made here, from the changelog section,
 * with the files of dist/. Everything that can refuse does so before anything is pushed. If the
 * release itself fails after the push, running this again pushes nothing new and makes it.
 */
function pushBinary(line, version, tagName) {
  const assets = checkDist(line, version);
  const notes = releasedSection(line, version).body;
  if (!succeeds("gh", ["auth", "status"])) {
    refuse("gh is missing or not logged in: the GitHub release is made with it");
  }
  if (succeeds("gh", ["release", "view", tagName])) {
    refuse(`the GitHub release ${tagName} exists`);
  }
  run("git", ["push", "--atomic", "origin", "main", tagName], { stdio: "inherit" });
  try {
    // `--notes=` takes the text as it is, whatever it starts with.
    run(
      "gh",
      [
        "release",
        "create",
        tagName,
        "--verify-tag",
        "--title",
        tagName,
        `--notes=${notes}`,
        ...assets,
      ],
      { stdio: "inherit" },
    );
  } catch {
    refuse(
      `pushed main and ${tagName}, but the GitHub release was not made: run make publish LINE=${line.id} again, which pushes nothing new and makes it`,
    );
  }
  console.log(
    `pushed main and ${tagName}, and made its GitHub release with ${assets.length} files`,
  );
}

/** How long `local` waits for npm to list what it published, and how often it asks. */
const LISTING = { minutes: 15, every: 15_000 };

function local(name) {
  const line = lineOf(name);
  if (line.kind === "binary") {
    refuse(`${line.id} is a binary, released with its files: make publish LINE=${line.id}`);
  }
  const { version, tagName } = currentTag(line);
  const cwd = join(root, line.dir);
  if (line.kind === "npm") {
    run("npm", ["publish"], { cwd, stdio: "inherit" });
    // npm processes a new package for minutes before it lists it, and the workflow the push
    // starts asks npm whether the version is there: one that asks too early fails.
    console.log(`waiting for npm to list ${line.name} ${version} before pushing`);
    const deadline = Date.now() + LISTING.minutes * 60_000;
    while (!succeeds("npm", ["view", `${line.name}@${version}`, "version"])) {
      if (Date.now() > deadline) {
        refuse(
          `npm has not listed ${line.name} ${version} in ${LISTING.minutes} minutes; once it does: git push --atomic origin main ${tagName}`,
        );
      }
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, LISTING.every);
    }
  } else {
    // cargo publish would stop on its own at a crate the registry does not know; this says which.
    const missing = unpublished(line);
    if (missing.length > 0) {
      const names = missing.map((dep) => `${dep.name} ${dep.version}`).join(", ");
      refuse(
        `${line.name} depends on ${names}, which crates.io does not have yet: publish it first (RELEASING.md, The crates)`,
      );
    }
    run("cargo", ["publish", "-p", line.name, ...cargoArgs(line)], { stdio: "inherit" });
  }
  run("git", ["push", "--atomic", "origin", "main", tagName], { stdio: "inherit" });
  console.log(`published ${line.name} and pushed ${tagName}`);
}

/** `npm stage` needs npm 11.15 or later and Node 22 ships npm 10; the release workflow installs the same one. */
const STAGE_NPM = ["--yes", "npm@11.21.1", "stage"];

function approve(name) {
  const line = lineOf(name);
  if (line.kind === "cargo") refuse(`${line.id} is not staged: crates.io publishes at once`);
  if (line.kind === "binary") refuse(`${line.id} is a binary: there is no registry to approve in`);
  const version = versionOf(line);
  const staged = JSON.parse(run("npx", [...STAGE_NPM, "list", line.name, "--json"])).filter(
    (item) => item.version === version,
  );
  if (staged.length === 0) {
    refuse(`npm holds no staged ${line.name} ${version}: has the release workflow staged it?`);
  }
  if (staged.length > 1) {
    const ids = staged.map((item) => `${item.id} (${item.status})`).join(", ");
    refuse(`npm holds more than one staged ${line.name} ${version}: ${ids}`);
  }
  // What is approved is shown first: who staged it and the shasum of what becomes public.
  run("npx", [...STAGE_NPM, "view", staged[0].id], { stdio: "inherit" });
  run("npx", [...STAGE_NPM, "approve", staged[0].id], { stdio: "inherit" });
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
  console.log(`kind=${line.kind}`);
  console.log(`name=${line.name}`);
  console.log(`dir=${line.dir}`);
  console.log(`version=${version}`);
}

function registry(name, version) {
  const line = lineOf(name);
  if (line.kind !== "npm") {
    refuse(`${line.id} is released as ${line.kind}, not published by the workflow`);
  }
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
  approve,
  dist,
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
