// After the site is built: a page that shows the elements says what each constructor is once. The
// elements' overview has cards in the place of the table of the builders, and the document keeps the
// table (it is shown as a table where the topics are on one page); a page that showed the two would
// say each sentence twice. Run on the built pages by the build (package.json), and by the tests.
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { topicElements } from "../src/docs.js";

// The words a page shows, with its tags taken out as the catalogue takes them out of a sentence, and
// spaces made one. The Markdown of the document is set with curly quotes and the catalogue's sentence
// is not: the two are the same words.
const words = (html) =>
  html
    .replace(/<(script|style)[\s\S]*?<\/\1>/g, "")
    .replace(/<[^>]*>/g, "")
    .replace(/&#(?:39|x27);|[\u2018\u2019]/g, "'")
    .replace(/&quot;|&#34;|[\u201C\u201D]/g, '"')
    .replace(/\s+/g, " ");

/** How many times each sentence (HTML, as the catalogue has it) is said in the main part of `page`. */
export function said(page, sentences) {
  const main = page.match(/<main[\s\S]*<\/main>/)?.[0] ?? page;
  const text = words(main);
  return sentences.map((sentence) => {
    const plain = words(sentence).trim();
    return plain === "" ? 0 : text.split(plain).length - 1;
  });
}

/** What is wrong: a sentence said more than once, or not at all, where it is to be said once. */
export function problems(path, page, elements) {
  const counts = said(
    page,
    elements.map((element) => element.doc),
  );
  return elements.flatMap((element, at) =>
    counts[at] === 1
      ? []
      : [
          `${path}: what pencere.${element.name} is, is said ${counts[at]} times, and once is how it is said there`,
        ],
  );
}

// The built site, from the command line.
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const site = join(dirname(fileURLToPath(import.meta.url)), "..");
  const catalog = JSON.parse(readFileSync(join(site, "src/data/catalog.json"), "utf8"));
  const wrong = [];
  const read = (path) => {
    const file = join(site, "dist", `${path}.html`);
    if (existsSync(file)) return readFileSync(file, "utf8");
    wrong.push(`${path}: the build made no such page`);
    return undefined;
  };
  const overview = read("kit/elements");
  if (overview) wrong.push(...problems("kit/elements", overview, catalog.elements));
  // The page of an element made of the catalogue says its own sentence, and only that one (an
  // element of a topic is read of on the topic's page).
  const topics = new Set(topicElements());
  for (const element of catalog.elements.filter((entry) => !topics.has(entry.name))) {
    const page = read(`kit/elements/${element.name}`);
    if (page === undefined) continue;
    wrong.push(...problems(`kit/elements/${element.name}`, page, [element]));
  }
  if (wrong.length > 0) {
    console.error(
      `The built pages repeat themselves:\n${wrong.map((line) => `  ${line}`).join("\n")}`,
    );
    process.exit(1);
  }
  console.log(`repeats: the elements' pages say each constructor once`);
}
