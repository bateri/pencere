// The code of an example that a topic of the kit shows beside its rule. A file of the example marks
// the parts that are shown, each between two comments, and the name of a part is the heading it stands
// under on the page of the topic:
//
//   -- #region A spring
//   local function springs() ... end
//   -- #endregion
//
// The site reads the parts (src/layouts/Doc.astro, src/components/Numbered.astro) and puts each one
// after what the section of its heading says. A part whose heading is gone would be code that no page
// shows, and a page that asks for a file that has no such part would show less than it says, so both
// stop the build (scripts/check.mjs); the functions are given the text and say what is wrong, so that a
// test can give them a file made up.
import { headings, slug } from "./links.mjs";

// A mark with no name is still a mark that opens a part: it is read as one, and said to have no name.
const OPEN = /^\s*--\s*#region(?:\s+(.*?))?\s*$/;
const CLOSE = /^\s*--\s*#endregion\s*$/;

/** The parts of a file: `regions` is a map of name to code, and `problems` says what is wrong with the marks, one line each. */
export function regionsOf(text, file = "the file") {
  const regions = new Map();
  const problems = [];
  let name = null;
  let lines = [];
  text.split("\n").forEach((line, index) => {
    const open = line.match(OPEN);
    if (open) {
      const opened = open[1] ?? "";
      if (name !== null)
        problems.push(
          `${file}:${index + 1}: the part ${opened === "" ? "with no name" : `"${opened}"`} starts inside the part "${name}"`,
        );
      else if (opened === "") problems.push(`${file}:${index + 1}: a part has no name`);
      else if (regions.has(opened))
        problems.push(`${file}:${index + 1}: there are two parts named "${opened}"`);
      name = opened;
      lines = [];
    } else if (CLOSE.test(line)) {
      if (name === null) problems.push(`${file}:${index + 1}: a part ends and none has started`);
      // A part with no name has been said already, and it has no heading to be shown under.
      else if (name !== "" && !regions.has(name)) regions.set(name, dedent(lines));
      name = null;
    } else if (name !== null) {
      lines.push(line);
    }
  });
  if (name !== null)
    problems.push(`${file}: the part ${name === "" ? "with no name" : `"${name}"`} is never ended`);
  return { regions, problems };
}

// The code with the indentation every line has taken off, and no empty line at either end.
function dedent(lines) {
  const kept = lines.slice();
  while (kept.length && kept[0].trim() === "") kept.shift();
  while (kept.length && kept.at(-1).trim() === "") kept.pop();
  const indents = kept
    .filter((line) => line.trim() !== "")
    .map((line) => line.match(/^[ \t]*/)[0].length);
  const cut = indents.length ? Math.min(...indents) : 0;
  return kept.map((line) => line.slice(cut)).join("\n");
}

/** What is wrong with a file's parts and the document that is to show them: each part has a heading of its name (a heading of the second to the fourth depth), and there is a part. */
export function regionProblems(file, code, document, source) {
  const { regions, problems } = regionsOf(code, file);
  // A part and a heading are matched by their slugs: the page's heading is set with curly quotes and
  // entities, which the words of the Markdown and the name of the part do not have
  // (src/components/Numbered.astro matches them the same way).
  const titles = new Set(
    headings(document)
      .filter((entry) => entry.depth >= 2 && entry.depth <= 4)
      .map((entry) => slug(entry.text)),
  );
  const named = new Map();
  for (const name of regions.keys()) {
    if (!titles.has(slug(name)))
      problems.push(
        `${file}: the part "${name}" has no heading of its name in ${source}, so no page shows it`,
      );
    if (named.has(slug(name)))
      problems.push(
        `${file}: the parts "${named.get(slug(name))}" and "${name}" have one slug, and one heading shows only one of them`,
      );
    else named.set(slug(name), name);
  }
  if (regions.size === 0 && problems.length === 0)
    problems.push(
      `${file}: ${source} is shown beside this code, and it has no part (-- #region Name ... -- #endregion)`,
    );
  return problems;
}
