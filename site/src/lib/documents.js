// The documents of the repository that are pages (src/docs.js), as the site's builder reads them:
// each one's content and headings, and where a link to a file of the repository goes on the site.
import { PAGES, LEFT } from "../docs.js";
import { read as readFile } from "./repo.js";
import { slug, topicsOf } from "../../scripts/links.mjs";

// The files themselves. The builder has to be told them here, as written paths: a page of the map
// whose file is not among them stops the build and says so.
const FILES = import.meta.glob([
  "../../../spec/draft/{pencere,apps}.md",
  "../../../kit/{SPEC,README}.md",
  "../../../kit/spec/*.md",
  "../../../{cli,program,sdk/typescript,host/rust}/README.md",
]);
const file = (source) => {
  const load = FILES[`../../../${source}`];
  if (!load)
    throw new Error(
      `${source} is a page in src/docs.js and is not among the files of src/lib/documents.js.`,
    );
  return load;
};

// A document: what it shows and its headings ({ depth, slug, text }).
export async function document(source) {
  const module = await file(source)();
  return { Content: module.Content, headings: module.getHeadings() };
}

// The topics of the kit's page API, in the order its index lists them: the files of the lines of
// `kit/SPEC.md` for a topic (its other links, to a topic it names, are not a place of its own).
export const topics = () => topicsOf(readFile("kit/SPEC.md"));

// For each document, the section (a heading of depth 2) every heading of it is in.
let sections;
async function read() {
  if (sections) return sections;
  sections = new Map();
  for (const source of new Set(PAGES.filter((page) => page.source).map((page) => page.source))) {
    const inside = new Map();
    let section = "";
    for (const heading of (await document(source)).headings) {
      // The checks of the site name a heading by the id scripts/links.mjs gives it: it is the builder's.
      if (slug(heading.text) !== heading.slug && !heading.slug.startsWith(`${slug(heading.text)}-`))
        throw new Error(
          `${source}: the heading "${heading.text}" has the id ${heading.slug}, and scripts/links.mjs says ${slug(heading.text)}.`,
        );
      if (heading.depth === 2) section = heading.slug;
      if (heading.depth >= 2) inside.set(heading.slug, section);
    }
    sections.set(source, inside);
  }
  check(sections);
  return sections;
}

// A document shown in parts is shown whole: each of its sections is on one page, or is named in
// LEFT; and a page asks for no section its document does not have.
function check(sections) {
  for (const [source, inside] of sections) {
    const parts = PAGES.filter((page) => page.source === source && page.sections);
    if (parts.length === 0) continue;
    const has = new Set([...inside.values()]);
    const left = LEFT[source] ?? [];
    for (const id of [...parts.flatMap((page) => page.sections), ...left])
      if (!has.has(id)) throw new Error(`${source} has no section ${id} (src/docs.js).`);
    for (const id of has) {
      const on = parts.filter((page) => page.sections.includes(id)).map((page) => `/${page.path}`);
      if (on.length + (left.includes(id) ? 1 : 0) !== 1)
        throw new Error(
          on.length === 0
            ? `The section ${id} of ${source} is on no page: give it to a page in src/docs.js, or name it in LEFT.`
            : `The section ${id} of ${source} is in more than one place: ${[...on, ...(left.includes(id) ? ["LEFT"] : [])].join(", ")}.`,
        );
    }
  }
}

// Where links go: `where(source, id)` is the address on the site of a file of the repository, or
// of the heading `id` in it, and nothing when the file is no page (or the heading is in a section
// no page shows).
export async function router() {
  const sections = await read();
  return (source, id = "") => {
    const pages = PAGES.filter((page) => page.source === source);
    if (pages.length === 0) return undefined;
    // With no heading named: the page that shows how the document opens.
    if (!id) return `/${(pages.find((page) => page.opening !== false) ?? pages[0]).path}`;
    const section = sections.get(source).get(id);
    const page =
      section === undefined
        ? pages.find((entry) => entry.opening !== false)
        : pages.find((entry) => !entry.sections || entry.sections.includes(section));
    return page ? `/${page.path}#${id}` : undefined;
  };
}
