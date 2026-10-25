// Releases a line of this repository: one script for every step that a machine can check, so that
// a release is the same sequence every time (RELEASING.md is the procedure, this file the checks).
//
// On a maintainer's machine:
//
//   prepare <line> <version>   set the version(s) and move Unreleased into a dated section; the
//                              commit of it, pushed to main, is what starts a release
//   rehearse <line>            everything a release of the prepared line checks, without changing
//                              anything, on any branch
//
// In the release workflow (.github/workflows/release.yml), which runs after the gate is green on
// main, or when it is started by hand:
//
//   plan [<line>]              the lines whose version has no tag yet (or the one named, which may be
//                              tagged on this commit already, to finish its release): the checks of
//                              each, and what the workflow reads
//   package <line>             the checks of each package of the line (for a crate, its dry run)
//   dist <line> [dir]          a binary line: the files of the directory are this version's, and intact
//   tag <line>                 the annotated tag of the line's version on this commit, pushed
//   draft <line> [dir]         the line's GitHub release as a draft (with the files of a binary line),
//                              made again if a draft is there, never over a public one
//   publish <line>             once a maintainer has approved: the line on its registry (by its kind),
//                              then its GitHub release made public
//   notes <line> [<version>]   the changelog section(s) of a version, as the release notes
//
// A line is published by its kind (tools/lines.mjs): `npm` with `npm publish`, `crates` to
// crates.io in order from a checkout of the tag, and `binary` by verifying the files of its draft
// (the sums and the attestations). npm and crates.io take the workflow's word for who it is
// (trusted publishing): no token is kept anywhere. Only a release of the binary line is ever the
// repository's Latest.
//
// It uses nothing but Node, git and the package managers already needed by the gate (and `gh`,
// `cargo` and `npm` for the verbs that publish).

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { LINES } from "./lines.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** The files a package must carry, beyond what its manager adds by itself and its own entries. */
const REQUIRED = {
  npm: ["LICENSE", "NOTICE", "README.md", "CHANGELOG.md", "package.json"],
  cargo: ["LICENSE", "NOTICE", "README.md", "CHANGELOG.md", "Cargo.toml"],
  // A binary has no package to list: these are the files of its directory that git holds.
  binary: ["LICENSE", "NOTICE", "README.md", "CHANGELOG.md", "Cargo.toml"],
};

/** What the changelog of a crate says when it was released with the others and changed in nothing. */
const LOCKSTEP = "Released with the other crates; no change of its own.";

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

/** The last line of what a failed command said. */
const lastSaid = (error) =>
  String(error.stderr ?? error.message)
    .trim()
    .split("\n")
    .at(-1);

/**
 * A line, with its packages: the line itself when it is one package, the crates in the order they
 * are published when it is `crates`. A package has a `dir`, a `kind` (`npm`, `cargo` or `binary`),
 * a `name` and the `entries` it is useless without.
 */
function lineOf(name) {
  const found = LINES[name];
  if (found === undefined) {
    refuse(`unknown line "${name}"; the lines are ${Object.keys(LINES).join(", ")}`);
  }
  const line = { id: name, ...found };
  line.packages =
    found.kind === "crates"
      ? found.members.map((member) => ({ ...member, id: name, kind: "cargo" }))
      : [line];
  return line;
}

const tagOf = (line, version) => `${line.id}-v${version}`;

// ---- the manifest's version ------------------------------------------------

function manifestPath(pkg) {
  return join(root, pkg.dir, pkg.kind === "npm" ? "package.json" : "Cargo.toml");
}

function versionOf(pkg) {
  const text = readFileSync(manifestPath(pkg), "utf8");
  if (pkg.kind === "npm") return JSON.parse(text).version;
  // The crate's version is its own line in [package], not the workspace's.
  const match = /^\[package\][^[]*?^version = "([^"]+)"$/ms.exec(text);
  if (match === null) refuse(`${pkg.dir}/Cargo.toml: no version of its own in [package]`);
  return match[1];
}

/** The version of a line: the one of its package, or the one all its crates are at. */
function lineVersion(line) {
  const versions = line.packages.map((pkg) => ({ pkg, version: versionOf(pkg) }));
  if (versions.some(({ version }) => version !== versions[0].version)) {
    const said = versions.map(({ pkg, version }) => `${pkg.name} ${version}`).join(", ");
    refuse(`${line.id} are not at one version: ${said}`);
  }
  return versions[0].version;
}

function setVersion(pkg, version) {
  const path = manifestPath(pkg);
  const text = readFileSync(path, "utf8");
  if (pkg.kind === "npm") {
    const manifest = JSON.parse(text);
    manifest.version = version;
    writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`);
    // The workspace's lockfile records the package's version too.
    const lockPath = join(root, "package-lock.json");
    const lock = JSON.parse(readFileSync(lockPath, "utf8"));
    const entry = lock.packages?.[pkg.dir];
    if (entry === undefined) refuse(`package-lock.json has no entry for ${pkg.dir}`);
    entry.version = version;
    writeFileSync(lockPath, `${JSON.stringify(lock, null, 2)}\n`);
  } else {
    writeFileSync(
      path,
      text.replace(
        /^(\[package\][^[]*?^version = ")[^"]+(")$/ms,
        (_, before, after) => `${before}${version}${after}`,
      ),
    );
  }
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

/**
 * A crate that another asks for by path and version (`name = { path = "…", version = "0.1.0" }`)
 * is not found by that other once its version has gone past what the requirement allows, and cargo
 * then fails to resolve the workspace. The crates move together, so every requirement of a crate of
 * the line, in any manifest of the repository (the command's, the page modules' workspace's),
 * moves to the new version. Returns the files that changed.
 */
function moveRequirements(line, version) {
  const names = line.packages.map((pkg) => pkg.name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  const pattern = new RegExp(
    `^(\\s*(?:${names.join("|")})\\s*=\\s*\\{[^}\\n]*\\bversion\\s*=\\s*")([^"]+)(")`,
    "gm",
  );
  const moved = [];
  for (const path of cargoManifests()) {
    const text = readFileSync(path, "utf8");
    const next = text.replace(pattern, (whole, before, req, after) =>
      req === version ? whole : `${before}${version}${after}`,
    );
    if (next !== text) {
      writeFileSync(path, next);
      moved.push(path.slice(root.length + 1));
    }
  }
  return moved;
}

/** The lockfile of each cargo workspace records its crates' versions (the page modules' too). */
function updateLockfiles() {
  for (const manifest of ["Cargo.toml", "kit/guest/Cargo.toml"]) {
    if (existsSync(join(root, manifest))) {
      run("cargo", ["update", "--workspace", "--offline", "--manifest-path", manifest]);
    }
  }
}

// ---- the changelog ---------------------------------------------------------

function changelogPath(pkg) {
  return join(root, pkg.dir, "CHANGELOG.md");
}

/** The changelog as its heading lines and the sections below them. */
function sections(pkg) {
  if (!existsSync(changelogPath(pkg))) refuse(`${pkg.dir}/CHANGELOG.md is missing`);
  const lines = readFileSync(changelogPath(pkg), "utf8").split("\n");
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

function unreleased(pkg) {
  const first = sections(pkg)[0];
  if (first === undefined || first.heading !== "## [Unreleased]") {
    refuse(`${pkg.dir}/CHANGELOG.md: the first section must be "## [Unreleased]"`);
  }
  return first;
}

function releasedSection(pkg, version) {
  const section = sections(pkg).find((s) => RELEASED.exec(s.heading)?.[1] === version);
  if (section === undefined) {
    refuse(`${pkg.dir}/CHANGELOG.md: no "## [${version}] - YYYY-MM-DD" section`);
  }
  if (section.body === "") refuse(`${pkg.dir}/CHANGELOG.md: the section of ${version} is empty`);
  return section;
}

/**
 * What a release says of itself: the changelog section of a package, or for the crates a section
 * for each crate that changed under its name, and the names of those released with no change.
 */
function notesOf(line, version) {
  if (line.kind !== "crates") return releasedSection(line, version).body;
  const own = [];
  const same = [];
  for (const pkg of line.packages) {
    const { body } = releasedSection(pkg, version);
    if (body === LOCKSTEP) same.push(pkg.name);
    else own.push(`## ${pkg.name}\n\n${body}`);
  }
  if (same.length > 0) {
    own.push(
      `Released with no change of their own: ${same.map((name) => `\`${name}\``).join(", ")}.`,
    );
  }
  return own.join("\n\n");
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

/** The versions the line has a tag for, here, oldest first. */
function taggedVersions(line) {
  return run("git", ["tag", "--list", `${line.id}-v*`])
    .split("\n")
    .map((name) => name.slice(line.id.length + 2))
    .filter((v) => SEMVER.test(v))
    .sort(compare);
}

// ---- prepare -----------------------------------------------------------------

function prepare(name, version) {
  const line = lineOf(name);
  if (!SEMVER.test(version ?? "")) refuse(`"${version}" is not a version (x.y.z)`);
  // The crates are at different versions to begin with (a crate that was released before the
  // others, the first release of the rest), and they meet at the new one; a line of one package
  // moves forward from its own.
  const lockstep = line.kind === "crates";
  const highest = line.packages.map(versionOf).sort(compare).at(-1);
  if (lockstep ? compare(version, highest) < 0 : compare(version, highest) <= 0) {
    refuse(
      lockstep
        ? `${version} is before ${highest}, the version of one of the crates`
        : `${version} is not after ${highest}`,
    );
  }
  // Everything is looked at before anything is written.
  let own = 0;
  for (const pkg of line.packages) {
    const top = unreleased(pkg);
    if (sections(pkg).some((s) => RELEASED.exec(s.heading)?.[1] === version)) {
      refuse(`${pkg.dir}/CHANGELOG.md already has a section for ${version}`);
    }
    if (top.body !== "") own++;
    else if (!lockstep) refuse(`${pkg.dir}/CHANGELOG.md: Unreleased is empty, nothing to release`);
  }
  if (own === 0) refuse(`no crate has anything under Unreleased: nothing to release`);

  const date = new Date().toISOString().slice(0, 10);
  for (const pkg of line.packages) {
    const top = unreleased(pkg);
    const lines = readFileSync(changelogPath(pkg), "utf8").split("\n");
    const dated = `## [${version}] - ${date}`;
    if (top.body === "") {
      lines.splice(top.start, top.end - top.start, "## [Unreleased]", "", dated, "", LOCKSTEP, "");
    } else {
      lines.splice(top.start, 1, "## [Unreleased]", "", dated);
    }
    writeFileSync(changelogPath(pkg), lines.join("\n"));
    setVersion(pkg, version);
  }
  const moved = lockstep ? moveRequirements(line, version) : [];
  if (line.kind !== "npm") updateLockfiles();

  console.log(`${line.title} ${highest} -> ${version}`);
  for (const file of moved)
    console.log(`${file}: the requirements of the crates moved to ${version}`);
  console.log(
    `next: review, commit ("Release ${line.title} ${version}") and push to main: the release workflow takes it from there`,
  );
}

// ---- the release checks -------------------------------------------------------

/**
 * What the line's version must be to be released: prepared (a dated section in every changelog,
 * nothing left under Unreleased) and after the line's last tag. Asks nothing of anything but the
 * files and the tags.
 */
function lineChecks(line) {
  const version = lineVersion(line);
  const tag = tagOf(line, version);
  if (version === "0.0.0") refuse(`${line.title} is at 0.0.0: run make release-prepare first`);
  for (const pkg of line.packages) {
    if (unreleased(pkg).body !== "") {
      refuse(`${pkg.dir}/CHANGELOG.md: Unreleased is not empty; prepare moves it into the release`);
    }
    const section = sections(pkg).find((s) => RELEASED.test(s.heading));
    if (section === undefined || RELEASED.exec(section.heading)[1] !== version) {
      refuse(`${pkg.dir}/CHANGELOG.md: the newest section is not ${version}`);
    }
    releasedSection(pkg, version);
  }
  const last = taggedVersions(line)
    .filter((v) => v !== version)
    .at(-1);
  if (last !== undefined && compare(version, last) <= 0) {
    refuse(`${version} is not after ${last}, the line's last release`);
  }
  return { version, tag };
}

const tagExists = (tag) =>
  succeeds("git", ["rev-parse", "--verify", "--quiet", `refs/tags/${tag}`]);
const tagIsHead = (tag) =>
  run("git", ["rev-list", "-n", "1", tag]) === run("git", ["rev-parse", "HEAD"]);

/** Everything a release of the line checks, on any branch: what `make rehearse` runs. */
function rehearse(name) {
  const line = lineOf(name);
  const { version, tag } = lineChecks(line);
  if (tagExists(tag)) refuse(`the tag ${tag} exists`);
  for (const pkg of line.packages) checkPackage(pkg, version);
  console.log(`${line.title} ${version} is ready to be released as ${tag}`);
}

/** What a package must be to be released: its files, and for a crate its manifest and a dry run. */
function checkPackage(pkg, version, { resuming = false } = {}) {
  for (const name of ["LICENSE", "NOTICE"]) {
    const copy = join(root, pkg.dir, name);
    if (
      !existsSync(copy) ||
      readFileSync(copy, "utf8") !== readFileSync(join(root, name), "utf8")
    ) {
      refuse(`${pkg.dir}/${name} is not the repository's ${name}`);
    }
  }
  const files = contents(pkg);
  for (const file of [...REQUIRED[pkg.kind], ...pkg.entries]) {
    if (!files.includes(file)) {
      refuse(
        pkg.kind === "binary"
          ? `${pkg.dir} lacks ${file}`
          : pkg.kind === "cargo"
            ? `the package of ${pkg.name} lacks ${file}`
            : `the package lacks ${file}`,
      );
    }
  }
  // A binary's directory is a crate that is not packaged: its tests are not shipped.
  const stray = pkg.kind === "binary" ? [] : files.filter(isStray);
  if (stray.length > 0) refuse(`the package carries tests or builds: ${stray.join(", ")}`);
  if (pkg.kind === "cargo") dryRun(pkg, version, { resuming });
}

/** What a package must not carry: tests (also the modules of a crate's `src/`), builds and installs. */
const isStray = (file) =>
  /^(test|tests|target)\//.test(file) ||
  /(^|\/)node_modules\//.test(file) ||
  /(^|\/)(tests|[^/]*_tests)\.rs$/.test(file);

/** The files the package manager would publish, as paths inside the package. */
function contents(pkg) {
  if (pkg.kind === "npm") {
    // `npm pack` runs the package's clean build (`prepack`) first, even where npm is
    // told to skip scripts, so this is what would ship.
    const report = JSON.parse(
      run("npm", ["pack", "--dry-run", "--json", "--ignore-scripts=false"], {
        cwd: join(root, pkg.dir),
      }),
    );
    return report[0].files.map((file) => file.path);
  }
  if (pkg.kind === "binary") {
    // Nothing is packaged: what the line ships is the files of its directory that git holds.
    return run("git", ["ls-files", "--", pkg.dir])
      .split("\n")
      .filter((file) => file !== "")
      .map((file) => file.slice(pkg.dir.length + 1));
  }
  return run("cargo", ["package", "--list", "-p", pkg.name, ...cargoArgs(pkg)]).split("\n");
}

/** The workspace a crate's cargo commands run against, when it is not the repository's own. */
const cargoArgs = (pkg) => (pkg.manifest === undefined ? [] : ["--manifest-path", pkg.manifest]);

// ---- the crates ---------------------------------------------------------------

/** What cargo knows of a crate from its manifest, in its workspace. */
function crateOf(pkg) {
  const metadata = JSON.parse(
    run("cargo", ["metadata", "--no-deps", "--format-version", "1", ...cargoArgs(pkg)]),
  );
  const crate = metadata.packages.find((p) => p.name === pkg.name);
  if (crate === undefined) {
    refuse(`cargo does not list ${pkg.name} in ${pkg.manifest ?? "the workspace"}`);
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

/**
 * A dry run of the publication builds the package from its own files against what crates.io has,
 * and in lockstep it has none of the crates this one asks for at this version. Cargo 1.88 (the
 * toolchain of the gate) cannot package several crates and check them against one another (cargo
 * 1.90 can), so each crate is dry-run on its own, with the crates it asks for taken from the
 * repository (`--config patch.crates-io.<crate>.path`): the package is still built from its own
 * extracted files, which is what the dry run is for.
 */
function dryRun(pkg, version, { resuming = false } = {}) {
  const crate = crateOf(pkg);
  const gaps = manifestGaps(crate);
  if (gaps.length > 0) refuse(`${pkg.dir}/Cargo.toml lacks: ${gaps.join(", ")}`);
  if (onRegistry(pkg.name, version)) {
    // A release that stopped half way, tagged on this commit, published this crate already.
    if (resuming) return;
    refuse(`crates.io has ${pkg.name} ${version} already`);
  }
  const patches = pathDependencies(crate).flatMap((dep) => {
    const sibling = LINES.crates.members.find((member) => member.name === dep.name);
    if (sibling === undefined) {
      refuse(
        `${pkg.name} depends on ${dep.name} by path, and no line of this repository publishes it`,
      );
    }
    return [
      "--config",
      `patch.crates-io.${dep.name}.path=${JSON.stringify(join(root, sibling.dir))}`,
    ];
  });
  run("cargo", ["publish", "--dry-run", "-p", pkg.name, ...cargoArgs(pkg), ...patches]);
}

// ---- the files of a binary line --------------------------------------------

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
 * The files of a directory (`dist/` after `make dist`, what the workflow gathered, what the draft
 * release holds) and that they are what they say: one executable for each target of the line
 * (the right system and processor for its name), SHA256SUMS naming these and no others, with the
 * sums of the files as they are, and nothing else but the licence files `make dist` adds. Returns
 * the paths the GitHub release is made of.
 */
function checkDist(line, version, dir = join(root, "dist")) {
  const shown = dir.startsWith(`${root}/`) ? dir.slice(root.length + 1) : dir;
  if (!existsSync(dir)) refuse(`${shown}/ is missing: make dist builds it`);
  const names = line.targets.map((target) => `${line.binary}-${version}-${target}`);
  const bytes = new Map();
  for (const [index, name] of names.entries()) {
    const path = join(dir, name);
    if (!existsSync(path) || !statSync(path).isFile())
      refuse(`${shown}/${name} is missing: make dist builds it`);
    const content = readFileSync(path);
    if (content.length === 0) refuse(`${shown}/${name} is empty`);
    const target = line.targets[index];
    const { system, processor } = executableFor(content);
    if (system === undefined)
      refuse(`${shown}/${name} is not an executable (neither ELF nor Mach-O)`);
    const want = [target.includes("apple-darwin") ? "macos" : "linux", target.split("-")[0]];
    if (system !== want[0] || processor !== want[1]) {
      refuse(
        `${shown}/${name} is a ${system} executable for ${processor ?? "an unknown processor"}, not for ${target}`,
      );
    }
    bytes.set(name, content);
  }

  const sums = join(dir, "SHA256SUMS");
  if (!existsSync(sums)) refuse(`${shown}/SHA256SUMS is missing: make dist writes it`);
  const listed = new Map();
  for (const text of readFileSync(sums, "utf8").split("\n")) {
    if (text === "") continue;
    const match = /^([0-9a-f]{64}) [ *](\S+)$/.exec(text);
    if (match === null) refuse(`${shown}/SHA256SUMS: "${text}" is not "<sha256>  <file>"`);
    if (listed.has(match[2])) refuse(`${shown}/SHA256SUMS names ${match[2]} twice`);
    listed.set(match[2], match[1]);
  }
  for (const name of listed.keys()) {
    if (!names.includes(name)) {
      refuse(
        `${shown}/SHA256SUMS names ${name}, which is not a file of ${line.binary} ${version}: is it the sums of another version?`,
      );
    }
  }
  for (const name of names) {
    const sum = listed.get(name);
    if (sum === undefined) refuse(`${shown}/SHA256SUMS does not name ${name}`);
    if (createHash("sha256").update(bytes.get(name)).digest("hex") !== sum) {
      refuse(`${shown}/${name} is not the file that ${shown}/SHA256SUMS has the sum of`);
    }
  }
  const known = [...names, "SHA256SUMS", "LICENSE", "NOTICE"];
  const extra = readdirSync(dir).filter((file) => !known.includes(file));
  if (extra.length > 0)
    refuse(`${shown}/ holds ${extra.join(", ")}, which no release of ${line.binary} carries`);
  return [...names.map((name) => join(dir, name)), sums];
}

function dist(name, dir) {
  const line = lineOf(name);
  if (line.kind !== "binary")
    refuse(`${line.id} has no executables: it is released as ${line.kind}`);
  const version = lineVersion(line);
  for (const path of checkDist(line, version, resolve(dir ?? join(root, "dist")))) {
    console.log(path.startsWith(`${root}/`) ? path.slice(root.length + 1) : path);
  }
}

// ---- the verbs ---------------------------------------------------------------

// ---- in the workflow: what to release, the tag, and the publication -----------------

/**
 * The lines to release on this commit, for the workflow to read from its output file: every line
 * whose version has no tag yet, or the one line named (a run started by hand), which may be tagged
 * on this commit already, so that a release that stopped half way is finished. Each is checked
 * (`lineChecks`) before it is named. With `rehearse`, the line named is built and tried, and
 * nothing is tagged or published.
 */
function plan(name) {
  const rehearsing = process.env.RELEASE_REHEARSE === "true";
  if (process.env.RELEASE_REQUIRE_CHECK === "true" && !rehearsing) {
    requireGreenCheck(run("git", ["rev-parse", "HEAD"]));
  }
  const ids = name === undefined || name === "" ? Object.keys(LINES) : [lineOf(name).id];
  const chosen = [];
  for (const id of ids) {
    const line = lineOf(id);
    if (rehearsing) {
      chosen.push(id);
      continue;
    }
    // Every release has a tag: a line whose version has one, and is not named, is released already
    // (and its changelog may have gone on to the next one's Unreleased).
    if (ids.length > 1 && tagExists(tagOf(line, lineVersion(line)))) continue;
    const { version, tag } = lineChecks(line);
    if (tagExists(tag)) {
      if (!tagIsHead(tag)) refuse(`${tag} is on another commit: a tag is never moved`);
      if (releaseState(tag) === "public") refuse(`the GitHub release ${tag} is public already`);
    }
    console.error(`${line.title} ${version} is to be released as ${tag}`);
    chosen.push(id);
  }
  if (chosen.length === 0) console.error("no line has a version without a tag: nothing to release");
  // Read by the workflow from its output file.
  console.log(`lines=${JSON.stringify(chosen)}`);
  for (const id of Object.keys(LINES)) console.log(`${id}=${chosen.includes(id)}`);
}

/** The run of the gate (check.yml) for the commit, on main, must have finished, and well. */
function requireGreenCheck(sha) {
  const runs = JSON.parse(
    run("gh", [
      "run",
      "list",
      "--workflow",
      "check.yml",
      "--event",
      "push",
      "--branch",
      "main",
      "--commit",
      sha,
      "--limit",
      "10",
      "--json",
      "databaseId,status,conclusion",
    ]),
  );
  const [latest] = runs;
  if (latest === undefined) refuse(`no run of the gate for ${sha.slice(0, 7)} on main`);
  if (latest.status !== "completed") {
    refuse(
      `the gate's run ${latest.databaseId} for ${sha.slice(0, 7)} is ${latest.status}: wait for it`,
    );
  }
  if (latest.conclusion !== "success") {
    refuse(
      `the gate's run ${latest.databaseId} for ${sha.slice(0, 7)} ended in ${latest.conclusion}`,
    );
  }
}

/** The annotated tag of the line's version on this commit, pushed; left as it is if it is there. */
function tag(name) {
  const line = lineOf(name);
  const { version, tag: tagName } = lineChecks(line);
  if (tagExists(tagName)) {
    if (!tagIsHead(tagName)) refuse(`${tagName} is on another commit: a tag is never moved`);
    console.log(`${tagName} is on this commit already`);
    return;
  }
  // Verbatim: the default cleanup would take the section's "### Added" lines for comments.
  const message = `${line.title} ${version}\n\n${notesOf(line, version)}\n`;
  run("git", ["tag", "--annotate", "--cleanup=verbatim", tagName, "--message", message]);
  run("git", ["push", "--quiet", "origin", `refs/tags/${tagName}`], { stdio: "inherit" });
  console.log(`tagged ${tagName} and pushed it`);
}

/** `isDraft` of the GitHub release of a tag, or null when there is none. */
function releaseState(tagName) {
  try {
    return JSON.parse(run("gh", ["release", "view", tagName, "--json", "isDraft"])).isDraft
      ? "draft"
      : "public";
  } catch (error) {
    if (String(error.stderr ?? "").includes("release not found")) return "none";
    refuse(`cannot ask GitHub about the release ${tagName}: ${lastSaid(error)}`);
  }
}

/** Is this version of the npm package on the registry? */
const onNpm = (name, version) => succeeds("npm", ["view", `${name}@${version}`, "version"]);

/**
 * The package on npm. The workflow is the package's trusted publisher, so npm takes its word for
 * who it is and signs where it was built (provenance). A version that npm has is left: a job that
 * is run again goes on to the GitHub release.
 */
function publishNpm(line, version) {
  if (onNpm(line.name, version)) {
    console.log(`${line.name} ${version} is on npm already`);
    return;
  }
  run("npm", ["publish", "--provenance", "--access", "public"], {
    cwd: join(root, line.dir),
    stdio: "inherit",
  });
}

/**
 * The crates, in order, from a clean checkout of the tag: what is published is what was tagged. A
 * crate that crates.io has already is left, so that a run that stopped half way can be run again.
 */
function publishCrates(line, version, tagName) {
  const dir = mkdtempSync(join(tmpdir(), "pencere-publish-"));
  run("git", ["worktree", "add", "--detach", dir, tagName]);
  try {
    // The builds of the verification share the target directory of the repository, which has the
    // dependencies built already.
    const env = {
      ...process.env,
      CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR ?? join(root, "target"),
    };
    for (const pkg of line.packages) {
      if (onRegistry(pkg.name, version)) {
        console.log(`${pkg.name} ${version} is on crates.io already`);
        continue;
      }
      console.log(`publishing ${pkg.name} ${version}`);
      run("cargo", ["publish", "-p", pkg.name, ...cargoArgs(pkg)], {
        cwd: dir,
        env,
        stdio: "inherit",
      });
    }
  } finally {
    succeeds("git", ["worktree", "remove", "--force", dir]);
  }
}

/**
 * The command makes projects that ask npm for the SDK and crates.io for the program library
 * (`pencere new --lang ts`, `--lang rust`): both must be released at the versions of this commit
 * before the command is.
 */
function requireDependencies() {
  const sdk = lineOf("sdk-typescript");
  const sdkVersion = lineVersion(sdk);
  if (!onNpm(sdk.name, sdkVersion)) {
    refuse(`${sdk.name} ${sdkVersion} is not on npm: the command is released after it`);
  }
  const crates = lineOf("crates");
  const cratesVersion = lineVersion(crates);
  const program = crates.packages.at(-1).name;
  if (!onRegistry(program, cratesVersion)) {
    refuse(`${program} ${cratesVersion} is not on crates.io: the command is released after it`);
  }
}

/** The files of the draft, checked as they were built (the sums) and by whom (the attestation). */
function verifyDownloads(line, version, tagName) {
  const dir = mkdtempSync(join(tmpdir(), "pencere-publish-files-"));
  try {
    run("gh", ["release", "download", tagName, "--dir", dir, "--pattern", "*"], {
      stdio: "inherit",
    });
    const files = checkDist(line, version, dir);
    const repo = run("gh", ["repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"]);
    for (const file of files) {
      run(
        "gh",
        [
          "attestation",
          "verify",
          file,
          "--repo",
          repo,
          "--signer-workflow",
          `${repo}/.github/workflows/release.yml`,
        ],
        { stdio: "inherit" },
      );
    }
    console.log(`${files.length} files checked against SHA256SUMS and their attestations`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/** The commands that deprecate the packages the line replaces, for those that are not deprecated yet. */
function deprecations(line) {
  return Object.entries(line.retired ?? {})
    .filter(([name]) => {
      try {
        return run("npm", ["view", name, "deprecated"]) === "";
      } catch (error) {
        return !String(error.stderr ?? "").includes("E404");
      }
    })
    .map(([name, message]) => `npm deprecate "${name}@*" ${JSON.stringify(message)}`);
}

/**
 * Makes the line public everywhere, by its kind, and then its GitHub release: what the workflow
 * does once a maintainer has approved it. The tag must be this commit's and its draft there.
 */
function publish(name) {
  const line = lineOf(name);
  const version = lineVersion(line);
  const tagName = tagOf(line, version);
  if (!tagExists(tagName)) refuse(`no tag ${tagName}: the workflow tags before it publishes`);
  if (!tagIsHead(tagName)) refuse(`${tagName} is not on this commit`);
  const state = releaseState(tagName);
  if (state === "none") refuse(`${tagName} has no GitHub release: the workflow writes the draft`);
  if (state === "public") refuse(`the GitHub release ${tagName} is public already`);

  if (line.kind === "npm") publishNpm(line, version);
  else if (line.kind === "crates") publishCrates(line, version, tagName);
  else {
    requireDependencies();
    verifyDownloads(line, version, tagName);
  }

  // Only the command is the repository's Latest; a draft carries no choice, so it is made here.
  const latest = line.kind === "binary" ? "--latest" : "--latest=false";
  run("gh", ["release", "edit", tagName, "--draft=false", latest], { stdio: "inherit" });
  console.log(`${line.title} ${version} is public: ${tagName}`);
  const pending = deprecations(line);
  if (pending.length > 0) {
    console.log(`\nThe packages this replaces are not deprecated yet. With 2FA, run:\n`);
    for (const command of pending) console.log(command);
  }
}

// ---- in the workflow --------------------------------------------------------

/** The checks of each package of the line, as the release workflow runs them before it tags. */
function packageLine(name) {
  const line = lineOf(name);
  const version = lineVersion(line);
  const tagName = tagOf(line, version);
  const resuming = tagExists(tagName) && tagIsHead(tagName);
  for (const pkg of line.packages) {
    checkPackage(pkg, version, { resuming });
    console.log(`${pkg.name} ${version}: its package is what a release of it carries`);
  }
}

/** The line's GitHub release as a draft: with the notes, and the files of a binary line. */
function draft(name, dir) {
  const line = lineOf(name);
  const version = lineVersion(line);
  const tagName = tagOf(line, version);
  const files =
    line.kind === "binary" ? checkDist(line, version, resolve(dir ?? join(root, "dist"))) : [];
  const notes = notesOf(line, version);
  const state = releaseState(tagName);
  if (state === "public") {
    refuse(
      `the GitHub release ${tagName} is public already: a release that is public is never touched`,
    );
  }
  // A job that is run again finds the draft of the first run: it is made again.
  if (state === "draft") run("gh", ["release", "delete", tagName, "--yes"]);
  // Only the command is ever the repository's Latest, which `publish` decides; the others never are.
  const latest = line.kind === "binary" ? [] : ["--latest=false"];
  run(
    "gh",
    [
      "release",
      "create",
      tagName,
      "--verify-tag",
      "--draft",
      "--title",
      tagName,
      `--notes=${notes}`,
      ...latest,
      ...files,
    ],
    { stdio: "inherit" },
  );
  console.log(
    `made the draft of ${tagName}${files.length > 0 ? ` with ${files.length} files` : ""}`,
  );
}

function notes(name, version) {
  const line = lineOf(name);
  console.log(notesOf(line, version ?? lineVersion(line)));
}

const COMMANDS = {
  prepare,
  rehearse,
  plan,
  package: packageLine,
  dist,
  tag,
  draft,
  publish,
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
  // A command this script ran failed: what it said is the reason (or was shown, if it ran inheriting).
  if (error !== null && typeof error === "object" && "status" in error && "stderr" in error) {
    const said = String(error.stderr ?? "").trim();
    console.error(`release: ${error.message.split("\n")[0]}${said === "" ? "" : `\n${said}`}`);
    process.exit(1);
  }
  throw error;
}
