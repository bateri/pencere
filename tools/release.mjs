// Releases a line of this repository: one script for every step that a machine can check, so that
// a release is the same sequence every time (RELEASING.md is the procedure, this file the checks).
//
// The four verbs of a release, as `make` runs them:
//
//   prepare <line> <version>   set the version(s) and move Unreleased into a dated section
//   tag <line>                 the release checks (`check`), then the annotated tag; nothing is pushed
//   push <line>                one atomic push of main and the tag, and nothing else
//   approve <line>             make the line public everywhere (by its kind), then its GitHub release
//
// and what a machine can try before that, or in the release workflow:
//
//   check <line>               everything a release needs, without changing anything
//   rehearse <line>            the same, but not the checks of the repository's state (branch, a clean
//                              tree, origin, the remote tags): on any branch, after a commit
//   package <line>             in the workflow: the checks of each crate's package and its dry run
//   dist <line> [dir]          a binary line: the files of the directory are this version's, and intact
//   draft <line> [dir]         in the workflow: the line's GitHub release as a draft (with the files of
//                              a binary line), made again if a draft is there, never over a public one
//   verify-tag <tag>           in the workflow: the tag names the line's version on main; prints the
//                              line, its kind, package, directory, version and tag
//   describe <line>            the same output without a tag, for a run started by hand
//   registry <line> <version>  in the workflow: is the npm package, and this version, published
//   notes <line> [<version>]   the changelog section(s) of a version, as the release notes
//
// A line is released by its kind (tools/lines.mjs): `npm` is staged by the workflow and approved
// on npm, `crates` are published to crates.io by `approve` from a clean checkout of the tag, and
// `binary` is built by the workflow, which attaches it to a draft that `approve` verifies and
// publishes. Whatever the kind, the workflow writes the GitHub release as a draft and `approve`
// publishes it; only a release of the binary line is ever the repository's Latest.
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
    `next: review, commit ("Release ${line.title} ${version}"), then make release LINE=${line.id}`,
  );
}

// ---- the release checks -------------------------------------------------------

/**
 * Everything a release needs, without changing anything. `rehearse` leaves out what asks about the
 * repository's state (the branch, a clean tree, `origin` and its tags): the rest is the same, so
 * that a prepared line can be tried on any branch.
 */
function check(name, { rehearse = false } = {}) {
  const line = lineOf(name);
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

  if (!rehearse) {
    if (run("git", ["branch", "--show-current"]) !== "main") refuse("not on main");
    if (run("git", ["status", "--porcelain"]) !== "") refuse("the working tree is not clean");
    run("git", ["fetch", "--quiet", "origin", "main", "--tags"]);
    if (!succeeds("git", ["merge-base", "--is-ancestor", "origin/main", "HEAD"])) {
      refuse("main has diverged from origin/main");
    }
  }
  if (succeeds("git", ["rev-parse", "--verify", "--quiet", `refs/tags/${tag}`])) {
    refuse(`the tag ${tag} exists`);
  }
  const last = taggedVersions(line).at(-1);
  if (last !== undefined && compare(version, last) <= 0) {
    refuse(`${version} is not after ${last}, the line's last release`);
  }
  if (!rehearse && run("git", ["ls-remote", "--tags", "origin", `refs/tags/${tag}`]) !== "") {
    refuse(`the tag ${tag} exists on origin`);
  }

  for (const pkg of line.packages) checkPackage(pkg, version);
  console.log(
    `${line.title} ${version} is ready to tag as ${tag}${rehearse ? " (a rehearsal: the state of the repository was not looked at)" : ""}`,
  );
  return { line, version, tag, notes: notesOf(line, version) };
}

/** What a package must be to be released: its files, and for a crate its manifest and a dry run. */
function checkPackage(pkg, version) {
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
  if (pkg.kind === "cargo") dryRun(pkg, version);
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
function dryRun(pkg, version) {
  const crate = crateOf(pkg);
  const gaps = manifestGaps(crate);
  if (gaps.length > 0) refuse(`${pkg.dir}/Cargo.toml lacks: ${gaps.join(", ")}`);
  if (onRegistry(pkg.name, version)) refuse(`crates.io has ${pkg.name} ${version} already`);
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

function tag(name) {
  const { tag: tagName, notes, line, version } = check(name);
  // Verbatim: the default cleanup would take the section's "### Added" lines for comments.
  const message = `${line.title} ${version}\n\n${notes}\n`;
  run("git", ["tag", "--annotate", "--cleanup=verbatim", tagName, "--message", message]);
  console.log(`tagged ${tagName}; nothing is pushed. Next: make publish LINE=${line.id}`);
}

function currentTag(line) {
  const version = lineVersion(line);
  const tagName = tagOf(line, version);
  if (!succeeds("git", ["rev-parse", "--verify", "--quiet", `refs/tags/${tagName}`])) {
    refuse(`no tag ${tagName}: run make release LINE=${line.id} first`);
  }
  if (run("git", ["rev-list", "-n", "1", tagName]) !== run("git", ["rev-parse", "HEAD"])) {
    refuse(`${tagName} is not on HEAD`);
  }
  return { version, tagName };
}

/** Pushes `main` and the tag, together or not at all; the workflow takes it from there. */
function push(name) {
  const line = lineOf(name);
  const { tagName } = currentTag(line);
  run("git", ["push", "--atomic", "origin", "main", tagName], { stdio: "inherit" });
  console.log(`pushed main and ${tagName}; the release workflow takes it from here`);
}

/** The version of the line's newest tag: what `approve` makes public, wherever HEAD has gone. */
function newestRelease(line) {
  const version = taggedVersions(line).at(-1);
  if (version === undefined) refuse(`${line.id} has no tag: make release LINE=${line.id} first`);
  return { version, tagName: tagOf(line, version) };
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

/**
 * The run of the release workflow for a tag must have finished, and well. Several lines can be
 * tagged on one commit, so the run is the one of this tag (a tag's run has the tag for its branch),
 * not the newest of the commit.
 */
function requireGreenRun(tagName) {
  const sha = run("git", ["rev-list", "-n", "1", tagName]);
  const runs = JSON.parse(
    run("gh", [
      "run",
      "list",
      "--workflow",
      "release.yml",
      "--event",
      "push",
      "--branch",
      tagName,
      "--commit",
      sha,
      "--limit",
      "10",
      "--json",
      "databaseId,status,conclusion",
    ]),
  );
  const [latest] = runs;
  if (latest === undefined) {
    refuse(`no run of the release workflow for ${tagName} (${sha.slice(0, 7)}): has it started?`);
  }
  if (latest.status !== "completed") {
    refuse(
      `the release workflow's run ${latest.databaseId} for ${tagName} is ${latest.status}: wait for it`,
    );
  }
  if (latest.conclusion !== "success") {
    refuse(
      `the release workflow's run ${latest.databaseId} for ${tagName} ended in ${latest.conclusion}: gh run view ${latest.databaseId}`,
    );
  }
}

/** `npm stage` needs npm 11.15 or later and Node 22 ships npm 10; the release workflow installs the same one. */
const STAGE_NPM = ["--yes", "npm@11.21.1", "stage"];

function approveStaged(line, version) {
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

/** The files of the draft, checked as they were built (the sums) and by whom (the attestation). */
function verifyDownloads(line, version, tagName) {
  const dir = mkdtempSync(join(tmpdir(), "pencere-approve-"));
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

/** Makes the line public everywhere, by its kind, and then its GitHub release. */
function approve(name, requested) {
  const line = lineOf(name);
  const { version, tagName } =
    requested === undefined
      ? newestRelease(line)
      : { version: requested, tagName: tagOf(line, requested) };
  if (!succeeds("git", ["rev-parse", "--verify", "--quiet", `refs/tags/${tagName}`])) {
    refuse(`no tag ${tagName}: git fetch --tags, if it was made on another machine`);
  }
  if (!succeeds("gh", ["auth", "status"])) refuse("gh is missing or not logged in");
  requireGreenRun(tagName);
  const state = releaseState(tagName);
  if (state === "none") refuse(`${tagName} has no GitHub release: the workflow writes the draft`);
  if (state === "public") refuse(`the GitHub release ${tagName} is public already`);

  if (line.kind === "npm") approveStaged(line, version);
  else if (line.kind === "crates") publishCrates(line, version, tagName);
  else verifyDownloads(line, version, tagName);

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

/** The crates' dry run, as the release workflow runs it for a tag or by hand. */
function packageCrates(name) {
  const line = lineOf(name);
  if (line.kind !== "crates") refuse(`${line.id} is not crates: only crates have a package to try`);
  const version = lineVersion(line);
  for (const pkg of line.packages) {
    checkPackage(pkg, version);
    console.log(`${pkg.name} ${version}: its package builds from its own files`);
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
  // Only the command is ever the repository's Latest, which `approve` decides; the others never are.
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

/** What the workflow reads from the output file of the step: the line, the tag, its version. */
function describeLine(line, version) {
  const first = line.packages[0];
  console.log(`line=${line.id}`);
  console.log(`kind=${line.kind}`);
  console.log(`name=${line.name ?? line.title}`);
  console.log(`dir=${first.dir}`);
  console.log(`version=${version}`);
  console.log(`tag=${tagOf(line, version)}`);
}

function verifyTag(tagName) {
  const match = /^(.+)-v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?)$/.exec(tagName ?? "");
  if (match === null) refuse(`"${tagName}" is not a release tag (<line>-v<version>)`);
  const line = lineOf(match[1]);
  const version = match[2];
  if (lineVersion(line) !== version) {
    refuse(
      `${tagName} names ${version} but ${line.packages.map((p) => p.dir).join(", ")} is at ${lineVersion(line)}`,
    );
  }
  for (const pkg of line.packages) releasedSection(pkg, version);
  run("git", ["fetch", "--quiet", "origin", "main"]);
  if (!succeeds("git", ["merge-base", "--is-ancestor", tagName, "origin/main"])) {
    refuse(`${tagName} is not on main`);
  }
  describeLine(line, version);
}

/** The same output for a run started by hand: the manifests say the version, no tag does. */
function describe(name) {
  const line = lineOf(name);
  const version = lineVersion(line);
  for (const pkg of line.packages) releasedSection(pkg, version);
  describeLine(line, version);
}

function registry(name, version) {
  const line = lineOf(name);
  if (line.kind !== "npm") {
    refuse(`${line.id} is released as ${line.kind}, not staged on npm`);
  }
  const exists = succeeds("npm", ["view", line.name, "name"]);
  const published = exists && succeeds("npm", ["view", `${line.name}@${version}`, "version"]);
  // Read by the workflow from its output file.
  console.log(`exists=${exists}`);
  console.log(`published=${published}`);
}

function notes(name, version) {
  const line = lineOf(name);
  console.log(notesOf(line, version ?? lineVersion(line)));
}

const COMMANDS = {
  prepare,
  check: (name) => check(name),
  rehearse: (name) => check(name, { rehearse: true }),
  tag,
  push,
  approve,
  package: packageCrates,
  dist,
  draft,
  "verify-tag": verifyTag,
  describe,
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
  // A command this script ran failed: what it said is the reason (or was shown, if it ran inheriting).
  if (error !== null && typeof error === "object" && "status" in error && "stderr" in error) {
    const said = String(error.stderr ?? "").trim();
    console.error(`release: ${error.message.split("\n")[0]}${said === "" ? "" : `\n${said}`}`);
    process.exit(1);
  }
  throw error;
}
