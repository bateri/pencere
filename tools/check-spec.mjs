// Checks that the specification and its fixtures say the same thing.
//
// Every example frame in a specification (a fenced block whose heading is
// `frame <path>`) must equal the fixture file it names, byte for byte, and the
// fixture must be one the manifest lists, so each example is also a conformance
// case. The manifest, the files on disk and the fixtures README must agree on
// which files exist. Run by `make check`; it uses nothing but Node.
//
// In an example `<ESC>` is the byte 0x1B, `<CR>` and `<LF>` are 0x0D and 0x0A,
// and every other byte is printable ASCII. The lines of a block are joined with
// nothing between them.

import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const specRoot = join(root, "spec");

const problems = [];
const fail = (where, what) => problems.push(`${where}: ${what}`);

const NOTATION = new Map([
  ["<ESC>", 0x1b],
  ["<CR>", 0x0d],
  ["<LF>", 0x0a],
]);

/** The bytes a block of example lines stands for. */
function bytesOf(lines) {
  const out = [];
  const text = lines.join("");
  for (let i = 0; i < text.length;) {
    if (text[i] === "<") {
      const token = [...NOTATION.keys()].find((t) => text.startsWith(t, i));
      if (token === undefined) throw new Error(`unknown notation at "${text.slice(i, i + 8)}"`);
      out.push(NOTATION.get(token));
      i += token.length;
      continue;
    }
    const code = text.charCodeAt(i);
    if (code < 0x20 || code > 0x7e) throw new Error(`byte ${code} has no notation`);
    out.push(code);
    i += 1;
  }
  return Buffer.from(out);
}

/** The `frame <path>` blocks of a markdown file: { path, lines, at }. */
function frameBlocks(markdown) {
  const blocks = [];
  const lines = markdown.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const open = /^```frame\s+(\S+)\s*$/.exec(lines[i] ?? "");
    if (open === null) continue;
    const body = [];
    let j = i + 1;
    while (j < lines.length && !/^```\s*$/.test(lines[j] ?? "")) {
      body.push(lines[j] ?? "");
      j += 1;
    }
    if (j >= lines.length) fail(`line ${i + 1}`, "a frame block is never closed");
    blocks.push({ path: open[1], lines: body, at: i + 1 });
    i = j;
  }
  return blocks;
}

function filesUnder(dir) {
  const found = [];
  for (const name of readdirSync(dir).sort()) {
    const full = join(dir, name);
    if (statSync(full).isDirectory()) found.push(...filesUnder(full));
    else found.push(full);
  }
  return found;
}

function checkSpecDir(dir) {
  const specFile = join(dir, "pencere.md");
  const fixtures = join(dir, "fixtures");
  const name = relative(root, dir);
  if (!existsSync(specFile)) return fail(name, "pencere.md is missing");
  if (!existsSync(fixtures)) return fail(name, "fixtures/ is missing");

  const manifestPath = join(fixtures, "manifest.json");
  if (!existsSync(manifestPath)) return fail(name, "fixtures/manifest.json is missing");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));

  // The files the manifest knows.
  const listed = new Set();
  for (const entry of manifest.decode ?? []) listed.add(entry.file);
  for (const session of manifest.session ?? []) {
    for (const step of session.steps ?? []) {
      for (const file of [step.in, step.out]) if (typeof file === "string") listed.add(file);
    }
  }
  const onDisk = new Map(
    filesUnder(fixtures).map((f) => [relative(fixtures, f).split("\\").join("/"), f]),
  );
  for (const file of listed) {
    if (!onDisk.has(file)) fail(`${name}/fixtures`, `the manifest lists ${file}, which is missing`);
  }
  const readme = readFileSync(join(fixtures, "README.md"), "utf8");
  for (const file of onDisk.keys()) {
    if (file === "manifest.json" || file === "README.md") continue;
    if (!listed.has(file)) fail(`${name}/fixtures`, `${file} is not in the manifest`);
    const inSession = file.startsWith("session/");
    const mention = inSession ? file.split("/").slice(0, 2).join("/") + "/" : file;
    if (!readme.includes(mention))
      fail(`${name}/fixtures/README.md`, `does not mention ${mention}`);
  }
  for (const entry of manifest.decode ?? []) {
    if (!["to-host", "to-program"].includes(entry.direction))
      fail(`${name}/fixtures`, `${entry.file} has no direction`);
    if (!Array.isArray(entry.messages) || !Array.isArray(entry.errors))
      fail(`${name}/fixtures`, `${entry.file} has no expected messages and errors`);
  }

  // Every example in the specification is a fixture, byte for byte.
  const markdown = readFileSync(specFile, "utf8");
  const blocks = frameBlocks(markdown);
  if (blocks.length === 0) fail(name, "pencere.md has no frame example");
  for (const block of blocks) {
    const where = `${name}/pencere.md:${block.at}`;
    const file = onDisk.get(block.path.replace(/^fixtures\//, ""));
    if (!block.path.startsWith("fixtures/") || file === undefined) {
      fail(where, `${block.path} is not a fixture`);
      continue;
    }
    if (!listed.has(block.path.slice("fixtures/".length)))
      fail(where, `${block.path} is not in the manifest`);
    let expected;
    try {
      expected = bytesOf(block.lines);
    } catch (error) {
      fail(where, error instanceof Error ? error.message : String(error));
      continue;
    }
    const actual = readFileSync(file);
    if (!expected.equals(actual)) {
      let at = 0;
      while (at < expected.length && at < actual.length && expected[at] === actual[at]) at += 1;
      fail(
        where,
        `differs from ${block.path} at byte ${at} (${expected.length} vs ${actual.length} bytes)`,
      );
    }
  }
  return blocks.length;
}

const dirs = readdirSync(specRoot)
  .map((n) => join(specRoot, n))
  .filter((p) => statSync(p).isDirectory());
if (dirs.length === 0) fail("spec", "no specification directory");
let examples = 0;
for (const dir of dirs) examples += checkSpecDir(dir) ?? 0;

if (problems.length > 0) {
  for (const p of problems) console.error(`check-spec: ${p}`);
  process.exit(1);
}
console.log(`check-spec: ${examples} examples match their fixtures`);
