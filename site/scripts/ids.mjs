// After the site is built: no page has two elements with one `id`. A heading's `id` is the slug of
// its words, so two topics that have a heading of the same words give one id on the page that shows
// them together (`/kit/spec/all`), and a link to it goes to the first. Run on the built pages by the
// build (package.json), and by the tests.
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

/** The ids of a page that more than one element has, each with how many have it. */
export function duplicates(html) {
  const markup = html
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<(script|style)\b[\s\S]*?<\/\1>/g, "");
  const seen = new Map();
  for (const tag of markup.matchAll(/<[a-zA-Z][^>]*>/g)) {
    const id = tag[0].match(/\sid=(?:"([^"]*)"|'([^']*)'|([^\s>"']+))/);
    const value = id && (id[1] ?? id[2] ?? id[3]);
    if (value) seen.set(value, (seen.get(value) ?? 0) + 1);
  }
  return [...seen].filter(([, count]) => count > 1);
}

const pages = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory()
      ? entry.name === "pagefind"
        ? []
        : pages(join(dir, entry.name))
      : entry.name.endsWith(".html")
        ? [join(dir, entry.name)]
        : [],
  );

// The built site, from the command line.
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const dist = join(dirname(fileURLToPath(import.meta.url)), "..", "dist");
  const wrong = pages(dist).flatMap((file) =>
    duplicates(readFileSync(file, "utf8")).map(
      ([id, count]) => `${relative(dist, file)}: ${count} elements have the id "${id}"`,
    ),
  );
  if (wrong.length > 0) {
    console.error(
      `A built page has an id twice, and a link to it goes to the first:\n${wrong.map((line) => `  ${line}`).join("\n")}`,
    );
    process.exit(1);
  }
  console.log("ids: no page has an id twice");
}
