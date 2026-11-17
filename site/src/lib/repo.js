// Reading the repository: how a page of the site takes a value out of one of its files
// (src/docs.js says why; src/lib/project.js is what the pages take, scripts/catalog.mjs reads the
// elements and the components on its own, before the site is built). A page asks for a section, a block of code, a table or a sentence by what the file calls
// it, and a file that no longer has it stops the build: a page cannot go on showing what the
// repository stopped saying.
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

// The site is built from its own directory, which is in the repository's.
const ROOT = resolve(process.cwd(), "..");
const gone = (file, what) =>
  new Error(`${file} has no ${what}: a page of the site reads it (src/lib/repo.js).`);

/** A file of the repository, as text. */
export const read = (file) => readFileSync(resolve(ROOT, file), "utf8");

/** The directories of a directory of the repository that have the file `name` in them, sorted. */
export const having = (dir, name) =>
  readdirSync(resolve(ROOT, dir), { withFileTypes: true })
    .filter((entry) => entry.isDirectory() && existsSync(resolve(ROOT, dir, entry.name, name)))
    .map((entry) => entry.name)
    .sort();

// The lines of a text, each with whether it is inside a block of code.
const lines = (text) => {
  let fenced = false;
  return text.split("\n").map((line) => {
    const fence = /^\s*```/.test(line);
    const inside = fenced || fence;
    if (fence) fenced = !fenced;
    return { line, code: inside };
  });
};

/** What a file says under the heading `title`, up to the next heading that is not inside it. */
export function section(file, title) {
  const all = lines(read(file));
  const name = (text) => text.replace(/`/g, "").trim();
  const heading = ({ line, code }) => (code ? null : line.match(/^(#{1,6})\s+(.*)$/));
  const from = all.findIndex((entry) => heading(entry) && name(heading(entry)[2]) === name(title));
  if (from < 0) throw gone(file, `section "${title}"`);
  const depth = heading(all[from])[1].length;
  const to = all.findIndex(
    (entry, index) => index > from && heading(entry) && heading(entry)[1].length <= depth,
  );
  return all
    .slice(from + 1, to < 0 ? undefined : to)
    .map((entry) => entry.line)
    .join("\n");
}

/** The blocks of code in a text, in order: { lang, code, before }, `before` the paragraph over it. */
export function blocks(text) {
  const found = [];
  for (const match of text.matchAll(/```([^\n]*)\n([\s\S]*?)\n```/g)) {
    const over =
      text
        .slice(0, match.index)
        .trimEnd()
        .split(/\n\s*\n/)
        .at(-1) ?? "";
    found.push({ lang: match[1].trim(), code: match[2], before: over.replace(/\s+/g, " ").trim() });
  }
  return found;
}

/** The block of code in the language `lang` of the section `title` of a file (the `nth` of them). */
export function block(file, title, lang, nth = 0) {
  const found = blocks(section(file, title)).filter((entry) => entry.lang === lang)[nth];
  if (!found) throw gone(file, `block of ${lang} under "${title}"`);
  return found;
}

/** The lines of a block of shell commands: what is run, and what the block says of it after `#`. */
export const commands = (code) =>
  code
    .split("\n")
    .filter((line) => line.trim())
    .map((line) => {
      const [run, says = ""] = line.split(/\s+#\s+/);
      return { run: run.trim(), says: says.trim() };
    });

/** The rows of the first table of a text (Markdown's, with pipes), without its heading: cells of Markdown. */
export function table(text) {
  const rows = text.split("\n").filter((line) => /^\s*\|/.test(line));
  return rows.slice(2).map((row) =>
    row
      .trim()
      .replace(/^\||\|$/g, "")
      .split("|")
      .map((cell) => cell.trim()),
  );
}

/** The paragraphs of a text that are plain words: not code, a table, a list or a heading. */
export const paragraphs = (text) =>
  text
    .replace(/```[\s\S]*?```/g, "")
    .split(/\n\s*\n/)
    .map((part) => part.trim())
    .filter((part) => part && !/^(\||[-*] |\d+\. |#|<)/.test(part))
    .map((part) => part.replace(/\s+/g, " "));

/** The sentence a paragraph opens with: up to the first full stop that ends one, outside code. */
export function sentence(paragraph) {
  const text = paragraph.replace(/\s+/g, " ").trim();
  let code = false;
  for (let i = 0; i < text.length; i++) {
    if (text[i] === "`") code = !code;
    else if (!code && text[i] === "." && (i + 1 === text.length || text[i + 1] === " "))
      return text.slice(0, i + 1);
  }
  return text;
}

const escape = (text) =>
  text.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

// A link of Markdown, whose words may be code.
const LINK = /\[((?:[^\]`]|`[^`]*`)*)\]\(([^)\s]*)\)/g;
const words = (markdown) =>
  markdown
    .split(/(`[^`]*`)/)
    .map((part) =>
      part.startsWith("`") && part.endsWith("`") && part.length > 1
        ? `<code>${escape(part.slice(1, -1))}</code>`
        : escape(part).replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>"),
    )
    .join("");

/** A line of Markdown as HTML: code and strong words, and links as their words (a line may stand inside a link). */
export const inline = (markdown) => words(markdown.replace(LINK, "$1"));

/** The same, with a link to another site kept as a link (one into the repository stays words). */
export const rich = (markdown) => {
  const links = [];
  const marked = markdown.replace(LINK, (whole, text, href) =>
    /^https?:/.test(href) ? `\u0001${links.push([text, href]) - 1}\u0001` : text,
  );
  return words(marked).replace(
    /\u0001(\d+)\u0001/g,
    (_, at) => `<a href="${escape(links[at][1])}">${words(links[at][0])}</a>`,
  );
};

/** The items of the first list of a text (Markdown's, with `-`), each on one line. */
export const items = (text) =>
  text
    .replace(/```[\s\S]*?```/g, "")
    .split(/\n(?=- )/)
    .filter((part) => part.startsWith("- "))
    .map((part) =>
      part
        .slice(2)
        .split(/\n\s*\n/)[0]
        .replace(/\s+/g, " ")
        .trim(),
    );
