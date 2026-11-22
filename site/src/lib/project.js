// What the pages of the site show of the project, each value with the file it is read from when
// the site is built (src/lib/repo.js). Nothing of the project is said here: a function finds what
// a file says, and the page shows that.
import {
  having,
  read,
  section,
  block,
  blocks,
  commands,
  table,
  paragraphs,
  sentence,
  inline,
  rich,
  items,
} from "./repo.js";

/** Installing: the command's one line, the libraries' lines, and the README's words for them. */
export function install() {
  const said = section("README.md", "Install");
  const [command, libraries] = blocks(said).filter((entry) => entry.lang === "sh");
  if (!command || !libraries)
    throw new Error(
      "README.md, Install: the command's line and the libraries' lines are not there.",
    );
  const words = paragraphs(said);
  return {
    command: command.code.trim(),
    // The README's sentence without its pointer to the command's own README, which is a page here.
    says: rich(command.before.replace(/\s*\(\[[^\]]*\]\([^)]*\)\)/, "").replace(/:$/, ".")),
    libraries: libraries.code.trim().split("\n"),
    librariesSay: rich(libraries.before.replace(/:$/, "")),
    host: rich(words.at(-1)),
  };
}

/** The programs made with pencere: the README's table of them in its order, and what it says of them all. */
export function pens() {
  const said = section("README.md", "Pens");
  const cells = [...said.matchAll(/<b><a href="([^"]+)">(\w+)<\/a><\/b><br>([\s\S]*?)<\/td>/g)];
  const own = said.match(/<b>Your own<\/b><br>([\s\S]*?)(?::\s*<a |<\/td>)/);
  if (cells.length === 0 || !own)
    throw new Error("README.md, Pens: the table of the programs is not there.");
  const words = paragraphs(said)[0];
  const first = sentence(words);
  return {
    // What the README says of every one of them, after it has said what they are.
    says: rich(words.slice(first.length).trim()),
    pens: cells.map(([, repo, name, words]) => ({ name, repo, words: words.trim() })),
    own: `${own[1].trim()}.`,
  };
}

/** The first app, in each language the README writes it in: the language, what is said of it, the code. */
export function firstApps() {
  const said = section("README.md", "A first app, in three languages");
  const apps = blocks(said)
    .map((entry) => ({ ...entry, name: entry.before.match(/\*\*([^*]+)\*\*,\s*([\s\S]*)$/) }))
    .filter((entry) => entry.name)
    .map(({ lang, code, name: [, name, note] }) => {
      const first = sentence(note.replace(/:$/, "."));
      return { lang, name, note: rich(first[0].toUpperCase() + first.slice(1)), code };
    });
  if (apps.length === 0)
    throw new Error("README.md, A first app: no block of code under a language's name.");
  return { says: rich(paragraphs(said)[0].split(/\s*\*\*/)[0]), apps };
}

/** The layers of the kit: the table of kit/README.md, the layer and what goes there. */
export function layers() {
  const said = section("kit/README.md", "Where a part of the kit goes");
  const rows = table(said);
  if (rows.length === 0)
    throw new Error(
      "kit/README.md, Where a part of the kit goes: the table of the layers is not there.",
    );
  return {
    says: inline(paragraphs(said)[0]),
    // A layer's cell is its number, its name and, in brackets, how a page reaches it.
    layers: rows.map(([layer, , goes]) => {
      const [, name, how = ""] = layer.match(/^\d+\.\s*([^(]*?)\s*(?:\((.*)\))?$/) ?? [];
      if (!name)
        throw new Error(
          `kit/README.md, the table of the layers: "${layer}" is not a layer's number and name.`,
        );
      return { name: inline(name), how: inline(how), goes: inline(goes) };
    }),
  };
}

/** The kit's library: what it is, and each module's name and the sentence its section opens with (kit/spec/library.md). */
export function library() {
  const said = section("kit/spec/library.md", "The kit's library");
  const [head, ...parts] = said.split(/\n## /);
  const modules = parts.map((part) => {
    const [title, ...rest] = part.split("\n");
    return {
      name: title.replace(/`/g, "").trim(),
      says: inline(sentence(paragraphs(rest.join("\n"))[0])),
    };
  });
  if (modules.length === 0)
    throw new Error("kit/spec/library.md names no module of the kit's library.");
  return { says: inline(sentence(paragraphs(head)[0])), modules };
}

/** What the page API says an element is (kit/spec/elements.md). */
export const elements = () => ({
  says: inline(paragraphs(section("kit/spec/elements.md", "Elements"))[0]),
});

/** The command's own lines of use, each with what its README says of it after `#` (cli/README.md, Use). */
export const use = () =>
  commands(block("cli/README.md", "Use", "sh").code).map((line) => ({
    ...line,
    says: inline(line.says),
  }));

/** The other kinds of project the command makes: the lines of its README's second block of use. */
export const kinds = () =>
  commands(block("cli/README.md", "Use", "sh", 1).code).map((line) => ({
    ...line,
    says: inline(line.says),
  }));

/** Installing the command: its README's line, and the sentence that says where it goes. */
export const command = () => ({
  says: inline(sentence(paragraphs(section("cli/README.md", "Install"))[0])),
});

// The names `pencere new` has for its projects, in cli/src/new.rs.
const TEMPLATES = { lua: "LUA", "lua-web": "LUA_WEB", ts: "TS", "ts-lua": "TS_LUA", rust: "RUST" };

/** The project `pencere new` makes: its files as the command writes them, and what its README says of them. */
export function project(template, name) {
  const source = read("cli/src/new.rs");
  const list = source.match(
    new RegExp(`const ${TEMPLATES[template]}: Template = &\\[([\\s\\S]*?)\\n\\];`),
  );
  if (!list) throw new Error(`cli/src/new.rs has no list of the files of the ${template} project.`);
  const files = [...list[1].matchAll(/\(\s*"([^"]+)",/g)].map((match) => match[1]);
  const file = (path) =>
    read(`cli/templates/${template}/${path}`).replaceAll("{{name}}", name).trimEnd();
  return { files, tree: tree(name, files), file, says: items(file("README.md")).map(inline) };
}

// A list of paths as a tree: directories first, as the command's README draws one.
function tree(root, files) {
  const lines = [`${root}/`];
  const walk = (paths, depth) => {
    const dirs = [
      ...new Set(paths.filter((path) => path.includes("/")).map((path) => path.split("/")[0])),
    ];
    for (const dir of dirs) {
      lines.push(`${"  ".repeat(depth)}${dir}/`);
      walk(
        paths
          .filter((path) => path.startsWith(`${dir}/`))
          .map((path) => path.slice(dir.length + 1)),
        depth + 1,
      );
    }
    for (const path of paths.filter((path) => !path.includes("/")))
      lines.push(`${"  ".repeat(depth)}${path}`);
  };
  walk(files, 1);
  return lines.join("\n");
}

/** The examples of the repository: each directory of examples/ with a README, and how that README opens. */
export const examples = () =>
  having("examples", "README.md").map((name) => {
    const words = paragraphs(read(`examples/${name}/README.md`))[0];
    if (!words) throw new Error(`examples/${name}/README.md says nothing of the example.`);
    return { name, says: inline(sentence(words).replace(/:$/, ".")) };
  });
