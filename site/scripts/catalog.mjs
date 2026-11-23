// The catalogue the site is built from, read from the repository itself: the elements from the
// kit's specification, the components from the specification's list and the
// comments the library's code says them in, and for each the page script under demos/ that pictures
// it (public/shots/, made by shots.sh). It writes src/data/catalog.json, and fails where the three
// do not agree: a component of the specification the code does not say, one of the code the
// specification does not list, or one with no picture.

import { readFileSync, readdirSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { html } from "./words.mjs";

const site = join(dirname(fileURLToPath(import.meta.url)), "..");
const repo = join(site, "..");
const kit = join(repo, "kit");
const library = join(kit, "sandbox/library/ui");
const demos = join(site, "demos");
const shots = JSON.parse(readFileSync(join(site, "public/shots/shots.json"), "utf8"));
// The topics of the page API that say what the catalogue shows: the library's and the elements'.
const spec = readFileSync(join(kit, "spec/library.md"), "utf8");
const elements_spec = readFileSync(join(kit, "spec/elements.md"), "utf8");

const wrong = [];

// --- The components -------------------------------------------------------------------------

// The specification lists the components in groups, a semicolon between two.
const GROUPS = ["Controls", "Fields and choices", "Layers", "Lists and data", "Layout", "Editing"];
// The components the host draws: a page asks for them, and they have no picture of the kit's.
const HOST = new Set(["menu", "editor_menu"]);

const listed = spec.match(/^- `button`, `icon_button`[^]*?(?=`ui\.commands)/m);
if (!listed) throw new Error("kit/spec/library.md: the list of the components is not where it was");
const groups = listed[0]
  .replace(/\(the host's\)/g, "")
  .split(";")
  .map((part) => [...part.matchAll(/`(\w+)`/g)].map((match) => match[1]))
  .filter((names) => names.length > 0);

// What the code says of each component: the comment above where it is made.
const said = new Map();
const made = new Set();
// The components that draw an icon the page gives them: those whose code asks for one to be drawn.
const takes_icon = new Set();
for (const file of readdirSync(library).filter((name) => name.endsWith(".luau"))) {
  const lines = readFileSync(join(library, file), "utf8").split("\n");
  let current = null;
  lines.forEach((line, index) => {
    if (current && line.includes("look.icon(")) takes_icon.add(current);
    const found =
      line.match(/core\.component\("(\w+)"/) ??
      line.match(/^function core\.Ctx:(\w+)\(/) ??
      line.match(/^function \w+\.(\w+)\(u\b/);
    if (!found) return;
    const name = found[1];
    current = name;
    if (line.includes("core.component(")) made.add(name);
    const comment = [];
    for (let at = index - 1; at >= 0 && lines[at].startsWith("--"); at--) {
      comment.unshift(lines[at].replace(/^--\s?/, ""));
    }
    // A line of the comment with nothing on it ends a paragraph.
    const words = comment
      .join("\n")
      .split(/\n\s*\n/)
      .map((paragraph) => paragraph.replace(/\s*\n\s*/g, " ").trim())
      .filter(Boolean)
      .join("\n\n");
    if (!words) return;
    const before = said.get(name);
    said.set(name, {
      words: before ? `${before.words}\n\n${words}` : words,
      source: before?.source ?? { file: `kit/sandbox/library/ui/${file}`, line: index + 1 },
    });
  });
}

const components = [];
groups.forEach((names, group) => {
  for (const name of names) {
    const doc = said.get(name);
    if (!doc)
      wrong.push(`${name}: the specification lists it, and no comment of the library's says it`);
    const demo = picture(`components/${name}`);
    if (!demo && !HOST.has(name))
      wrong.push(`${name}: no page under demos/components/ pictures it`);
    components.push({
      name,
      group: GROUPS[group] ?? `Group ${group + 1}`,
      host: HOST.has(name),
      summary: doc ? summary(doc.words) : "",
      doc: doc ? doc.words.split("\n\n").map(html) : [],
      source: doc?.source ?? null,
      demo,
    });
  }
});
for (const name of made) {
  if (!components.some((component) => component.name === name)) {
    wrong.push(`${name}: the library makes it, and the specification does not list it`);
  }
}
for (const file of readdirSync(join(demos, "components"))) {
  const name = file.replace(/\.lua$/, "");
  if (!file.startsWith("_") && !components.some((component) => component.name === name)) {
    wrong.push(`demos/components/${file}: no component is named ${name}`);
  }
}

// --- The glyphs -----------------------------------------------------------------------------

// The names of the library's glyphs, as its code has them, and as the specification names them
// where it says what an icon is: the two are the same names.
const look = readFileSync(join(library, "look.luau"), "utf8").match(
  /\nlocal GLYPHS = \{\n([^]*?)\n\}\n/,
);
if (!look) throw new Error("look.luau: the table of the glyphs is not where it was");
const glyphs = [...look[1].matchAll(/^\t(\w+) = function/gm)].map((match) => match[1]).sort();
const named = spec.match(/the name of one of the library's glyphs:([^.]*)\./);
if (!named)
  throw new Error("kit/spec/library.md: the sentence that names the glyphs is not where it was");
const said_glyphs = [...named[1].matchAll(/`(\w+)`/g)].map((match) => match[1]).sort();
for (const name of glyphs)
  if (!said_glyphs.includes(name))
    wrong.push(`${name}: a glyph of the library, and the specification does not name it`);
for (const name of said_glyphs)
  if (!glyphs.includes(name))
    wrong.push(`${name}: the specification names it as a glyph, and the library has none`);
// What the specification says an icon is, for the pages of the components that take one.
const icon_said = spec.match(/^- (An `icon` that a component[^]*?)\n(?=- )/m);
if (!icon_said) throw new Error("kit/spec/library.md: what an icon is, is not where it was");
const icon = html(icon_said[1].replace(/\s*\n\s*/g, " "));
for (const component of components) {
  component.icon = takes_icon.has(component.name);
}

// --- The elements ---------------------------------------------------------------------------

const elements = [];
// The elements the host draws in a view of its own, which no picture of the drive shows: a browser is
// the host's web view. Their page says what they are, with no picture.
const DRAWN_BY_HOST = new Set(["browser"]);
const table = elements_spec.match(/\| Constructor[^\n]*\n\|[-| ]+\n((?:\|[^\n]*\n)+)/);
if (!table) throw new Error("kit/spec/elements.md: the table of the elements is not where it was");
for (const row of table[1].trim().split("\n")) {
  const [, made, words] = row
    .replace(/\\\|/g, "\u0000")
    .split("|")
    .map((cell) => cell.trim().replace(/\u0000/g, "|"));
  const name = made.match(/pencere\.(\w+)/)?.[1];
  if (!name) continue;
  const demo = picture(`elements/${name}`);
  if (!demo && !DRAWN_BY_HOST.has(name))
    wrong.push(`${name}: no page under demos/elements/ pictures it`);
  // Further pictures of the same element, as `elements/<name>-<what>.lua`, are shown after the first.
  const more = readdirSync(join(demos, "elements"))
    .filter((file) => file.startsWith(`${name}-`) && file.endsWith(".lua"))
    .map((file) => ({
      name: `elements/${file.replace(/\.lua$/, "")}`,
      ...picture(`elements/${file.replace(/\.lua$/, "")}`),
    }))
    .filter((extra) => extra.code);
  elements.push({ name, constructor: made.replace(/`/g, ""), doc: html(words), demo, more });
}

// The whole pages the site shows as they are: a page of the kit, its code from the first line.
const pages = {};
for (const name of ["kit/counter"]) {
  const file = join(demos, `${name}.lua`);
  if (!existsSync(file) || !shots[name]) {
    wrong.push(`${name}: no page under demos/ pictures it`);
    continue;
  }
  const code = readFileSync(file, "utf8")
    .split("\n")
    .filter((line) => !line.startsWith("--@"))
    .join("\n")
    .trim();
  pages[name] = { code, width: shots[name].width, height: shots[name].height };
}

// --- Out ------------------------------------------------------------------------------------

if (wrong.length > 0) {
  console.error(
    `catalog: the specification, the library and the pictures do not agree:\n  ${wrong.join("\n  ")}`,
  );
  process.exit(1);
}
mkdirSync(join(site, "src/data"), { recursive: true });
writeFileSync(
  join(site, "src/data/catalog.json"),
  JSON.stringify({ groups: GROUPS, components, elements, icon, pages }, null, 2) + "\n",
);
console.log(`catalog: ${components.length} components, ${elements.length} elements`);

// --- Helpers --------------------------------------------------------------------------------

// The page script `name` under demos/ and its picture: the code a reader is shown, and the size of
// the picture in points. Null where there is none.
function picture(name) {
  const file = join(demos, `${name}.lua`);
  if (!existsSync(file) || !shots[name]) return null;
  return {
    code: shown(readFileSync(file, "utf8")),
    width: shots[name].width,
    height: shots[name].height,
  };
}

// What a reader is shown of a page script: what it keeps (the lines before the page is made) and
// what it draws (what `finish` adds over the page where it adds something, the part otherwise),
// without the lines that say how it is pictured or that make it a page of the site.
function shown(source) {
  const lines = source.split("\n").filter((line) => !line.startsWith("--@"));
  const start = lines.findIndex((line) => line.startsWith("return page("));
  const keeps = lines
    .slice(0, start)
    .filter((line) => !/require\("\.\.\/_page"\)/.test(line))
    .join("\n")
    .trim();
  const body = (from, until) => {
    const open = lines.findIndex((line, index) => index >= start && from.test(line));
    if (open < 0) return "";
    const close = lines.findIndex((line, index) => index > open && until.test(line));
    return dedent(lines.slice(open + 1, close).join("\n"));
  };
  const finish = body(/^\tfinish = function/, /^\tend,/);
  const draw = body(/^return page\(function/, /^end/);
  // A part that is only words under what `finish` draws says nothing of it.
  const under = /^return pencere\.text\(/.test(draw);
  return [keeps, finish && !under ? draw : "", finish || draw].filter(Boolean).join("\n\n");
}

function dedent(text) {
  const lines = text.split("\n");
  const least = Math.min(
    ...lines.filter((line) => line.trim()).map((line) => line.match(/^\t*/)[0].length),
  );
  return lines
    .map((line) => line.slice(least))
    .join("\n")
    .trim();
}

// The first sentence of what the code says, without what it says in brackets: what the component
// is, as HTML.
function summary(words) {
  let sentence = words.split(/\.\s/)[0];
  for (let before = ""; before !== sentence;) {
    before = sentence;
    sentence = sentence.replace(/\s*\([^()]*\)/g, "");
  }
  return html(
    sentence
      .replace(/\s+([,:;])/g, "$1")
      .trim()
      .replace(/[.:]$/, "") + ".",
  );
}
