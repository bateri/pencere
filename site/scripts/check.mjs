// Whether the site and the repository agree, before the site is built (src/docs.js says the rule).
// Two things are looked at here; what a page reads from a file is checked where it is read
// (src/lib/repo.js), and the sections of a document shown in parts where its pages are made
// (src/lib/documents.js).
//
// 1. Every document of the repository is somewhere: it is a page, a page reads it, or the map
//    names it as kept apart, with the reason. A new README cannot be left out by forgetting it.
// 2. A name the site's own words set as code is a name the repository has: the pages lead a reader
//    with words of their own, and those words name nothing the repository does not.
// 3. The kit's page API, a file for each topic: every link into one goes to a file and a heading
//    that are there, and the index of them holds no rule (scripts/links.mjs).

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { PAGES, APART, HOME } from "../src/docs.js";
import { indexProblems, linkProblems } from "./links.mjs";

const site = join(dirname(fileURLToPath(import.meta.url)), "..");
const repo = join(site, "..");
const wrong = [];

// The files the repository keeps. Where the site is built from a copy without its history, there
// is no list of them to check against, and the build says so.
let tracked = null;
try {
  tracked = execFileSync("git", ["ls-files"], {
    cwd: repo,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  })
    .split("\n")
    .filter(Boolean);
} catch {
  console.warn(
    "check: not a checkout of the repository; the documents are not checked against the site's map.",
  );
}

// --- 1. The documents -----------------------------------------------------------------------

const matches = (pattern) => {
  const like = new RegExp(
    `^${pattern.replace(/[.+?^${}()|[\]\\]/g, "\\$&").replace(/\*/g, "[^/]+")}$`,
  );
  return (file) => like.test(file);
};
if (tracked) {
  const documents = tracked.filter((file) => file.endsWith(".md") && !file.startsWith("site/"));
  const shown = new Set(PAGES.filter((page) => page.source).map((page) => page.source));
  const read = [...HOME, ...PAGES.flatMap((page) => page.reads ?? [])].map(matches);
  const apart = APART.map(([pattern]) => ({ pattern, is: matches(pattern), used: false }));
  for (const file of documents) {
    const kept = apart.filter((entry) => entry.is(file));
    for (const entry of kept) entry.used = true;
    if (shown.has(file) || read.some((is) => is(file)) || kept.length > 0) continue;
    wrong.push(`${file}: no page shows or reads it, and src/docs.js does not name it in APART`);
  }
  for (const source of shown)
    if (!documents.includes(source))
      wrong.push(`${source}: a page of src/docs.js shows it, and the repository has no such file`);
  for (const entry of apart)
    if (!entry.used)
      wrong.push(`APART names ${entry.pattern}, and the repository has no such document`);
}

// --- 2. The names in the site's own words -----------------------------------------------------

const under = (dir) =>
  readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? under(path) : [path];
  });
// What the repository says: its documents, the command's templates and its words of use, and the
// kit's library.
const says = (tracked ?? [])
  .filter(
    (file) =>
      !file.startsWith("site/") &&
      (/\.(md|lua|luau|ts|js|html)$/.test(file) ||
        file === "cli/src/lib.rs" ||
        file === "cli/src/new.rs"),
  )
  .filter((file) => !/(^|\/)(node_modules|target|dist)\//.test(file))
  .map((file) => readFileSync(join(repo, file), "utf8"))
  .join("\n");
const plain = (text) =>
  text
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&amp;/g, "&")
    .replace(/\s+/g, " ")
    .trim();
if (tracked) {
  const own = under(join(site, "src")).filter((file) => file.endsWith(".astro"));
  const text = says.replace(/\s+/g, " ");
  for (const file of own) {
    for (const [, name] of readFileSync(file, "utf8").matchAll(/<code>([^<{}]+)<\/code>/g)) {
      if (!text.includes(plain(name)))
        wrong.push(
          `${relative(site, file)}: \`${plain(name)}\` is set as code, and no file of the repository has it`,
        );
    }
  }
}

// --- 3. The topics of the kit's page API -----------------------------------------------------

if (tracked) {
  const text = new Map(
    tracked
      .filter(
        (file) =>
          /\.(md|rs|lua|luau|ts|js|mjs|astro|toml|wit)$/.test(file) &&
          !/(^|\/)(node_modules|target|dist)\//.test(file) &&
          !file.includes(".test.") &&
          existsSync(join(repo, file)),
      )
      .map((file) => [file, readFileSync(join(repo, file), "utf8")]),
  );
  wrong.push(...linkProblems(text));
  wrong.push(
    ...indexProblems(
      text.get("kit/SPEC.md") ?? "",
      tracked.filter((file) => /^kit\/spec\/[a-z0-9-]+\.md$/.test(file)),
    ),
  );
}

if (wrong.length > 0) {
  console.error(
    `The site and the repository do not agree:\n${wrong.map((line) => `  ${line}`).join("\n")}`,
  );
  process.exit(1);
}
