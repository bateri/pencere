// Three rules about the kit's page API, which is a file for each topic (kit/spec) and an index of
// them (kit/SPEC.md):
//
// 1. Every link into a topic goes to a file that is there and to a heading that is in it: a Markdown
//    link, an address of the repository on the web, or the path of a topic written in a comment of
//    code (`kit/spec/events.md`).
// 2. The index holds no rule: only its headings, one line that says what it is, and a line for each
//    topic, a link and a description. The rules are in the topics, where a change to one is a change
//    to the one place that says it.
// 3. Nothing names the kit's document by a number of its old sections: the document had numbered
//    sections once, and a reference that kept the number goes to no place now. A comment of code
//    or a sentence of a document names the topic's file (or links it).
//
// The functions are given the files (a map of path to text) and say what is wrong, so that a test
// can give them a repository made up; scripts/check.mjs reads the real one.
import { posix } from "node:path";

const plain = (text) =>
  text
    .replace(/`/g, "")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/(\*\*|__)(.+?)\1/g, "$2");

/** The id the site's builder gives a heading with this text (GitHub's rule, as Astro keeps it). */
export const slug = (text) =>
  plain(text)
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\p{M}\s_-]/gu, "")
    .replace(/ /g, "-");

/** The headings of a text of Markdown, outside blocks of code: { depth, text, id } (a repeated one is numbered, as the builder numbers it). */
export function headings(markdown) {
  const seen = new Map();
  let fenced = false;
  const found = [];
  for (const line of markdown.split("\n")) {
    if (/^\s*```/.test(line)) fenced = !fenced;
    const match = !fenced && line.match(/^(#{1,6})\s+(.*?)\s*#*\s*$/);
    if (!match) continue;
    const base = slug(match[2]);
    const times = seen.get(base) ?? 0;
    seen.set(base, times + 1);
    found.push({
      depth: match[1].length,
      text: plain(match[2]),
      id: times ? `${base}-${times}` : base,
    });
  }
  return found;
}

const TOPIC = /^kit\/spec\/[a-z0-9-]+\.md$/;
const WEB = /^https:\/\/github\.com\/[^/]+\/pencere\/blob\/main\//;

/** What is wrong with the links into the topics, one line each; `files` is a map of path to text. */
export function linkProblems(files) {
  const ids = new Map();
  const idsOf = (path) => {
    if (!ids.has(path))
      ids.set(path, new Set(headings(files.get(path)).map((heading) => heading.id)));
    return ids.get(path);
  };
  const found = new Set();
  const check = (from, line, target, fragment) => {
    const where = `${from}:${line}`;
    if (!files.has(target)) {
      found.add(`${where}: links to ${target}, and the repository has no such file`);
    } else if (fragment && !idsOf(target).has(decodeURIComponent(fragment))) {
      found.add(`${where}: links to ${target}#${fragment}, and the file has no such heading`);
    }
  };
  for (const [path, text] of files) {
    text.split("\n").forEach((line, index) => {
      const at = index + 1;
      // A Markdown link, to a topic or to the index, from a document or a comment.
      for (const [, href] of line.matchAll(/\]\(([^)\s]+)\)/g)) {
        const [name, fragment = ""] = href.split("#");
        let target;
        if (name === "") target = path;
        else if (WEB.test(name)) target = name.replace(WEB, "");
        else if (/^[a-z][a-z0-9+.-]*:/i.test(name) || name.startsWith("//")) continue;
        else target = posix.normalize(posix.join(posix.dirname(path), name));
        if (TOPIC.test(target) || target === "kit/SPEC.md") check(path, at, target, fragment);
      }
      // A topic named by its path, in a comment or in the words of a document.
      for (const [, name, fragment = ""] of line.matchAll(
        /kit\/spec\/([a-z0-9-]+)\.md(?:#([\w-]+))?/g,
      )) {
        check(path, at, `kit/spec/${name}.md`, fragment);
      }
    });
  }
  return [...found];
}

// The words of a rule: the protocol's key words, and the lower-case ones that say the same.
const ITEM = /^- \[[^\]]+\]\(spec\/([a-z0-9-]+)\.md\): (.+)$/;
const RULE = /\b(MUST|SHALL|SHOULD|REQUIRED|RECOMMENDED|OPTIONAL|must|shall|should)\b/;
const words = (text) => text.split(/\s+/).filter(Boolean).length;

/** What is wrong with the index of the topics: a rule, a line that is no description, a topic missing or twice. */
export function indexProblems(text, topics) {
  const found = [];
  const listed = new Map();
  let opened = false;
  let said = false;
  text.split("\n").forEach((line, index) => {
    const at = `kit/SPEC.md:${index + 1}`;
    if (!line.trim()) return;
    if (RULE.test(line))
      found.push(`${at}: the index holds no rule, and this line has the word of one: ${line}`);
    if (/^# /.test(line)) {
      if (opened) found.push(`${at}: a second title: ${line}`);
      opened = true;
      return;
    }
    if (/^## /.test(line)) return;
    const item = line.match(ITEM);
    if (item) {
      const file = `kit/spec/${item[1]}.md`;
      listed.set(file, (listed.get(file) ?? 0) + 1);
      if (words(item[2]) > 40)
        found.push(`${at}: the description of ${file} is longer than 40 words`);
      return;
    }
    // One line, right after the title, says what the index is.
    if (opened && !said && words(line) <= 50 && !/^[-#|>]/.test(line)) {
      said = true;
      return;
    }
    found.push(
      `${at}: a line of the index that is no heading and no topic's line (the rules are in the topics): ${line}`,
    );
  });
  for (const topic of topics) {
    if (!listed.has(topic)) found.push(`kit/SPEC.md: ${topic} is in no line of the index`);
  }
  for (const [file, times] of listed) {
    if (!topics.includes(file))
      found.push(`kit/SPEC.md: lists ${file}, which is no file of the topics`);
    else if (times > 1) found.push(`kit/SPEC.md: ${file} is listed more than once`);
  }
  return found;
}

/** The files the index lists for the topics (its lines of a topic, not every link it has), in its order. */
export function topicsOf(text) {
  const found = [];
  for (const line of text.split("\n")) {
    const item = line.match(ITEM);
    if (item) found.push(`kit/spec/${item[1]}.md`);
  }
  return found;
}

// The kit's document by its path or its names, and a number of its sections within a few words of it, the
// number first or the document first. A sentence ends at a full stop or a semicolon.
const DOCUMENT = String.raw`(?:kit/SPEC(?:\.md)?\x60?|SPEC\.md\x60?|\bkit(?:'s)? (?:[Ss]pecification|[Ss]pec|SPEC|[Dd]ocument|page API))`;
const NUMBER = String.raw`(?:Sections?|§)\s*\d`;
const OLD_NUMBER = new RegExp(
  `${DOCUMENT}[^.;]{0,40}?${NUMBER}|${NUMBER}[\\d.]*[^.;]{0,25}?\\b(?:of|in) (?:the )?${DOCUMENT}`,
  "g",
);
// What a comment or a list adds to the front of a line, which the sentence does not have.
const LEADER = /^[ \t]*(?:\/\/[/!]?|--|#|\*|>)?[ \t]*/;

/** The places that name the kit's document by a number of its old sections; `files` is a map of path to text. */
export function numberProblems(files) {
  const found = [];
  for (const [path, text] of files) {
    let flat = "";
    const starts = [];
    for (const line of text.split("\n")) {
      starts.push(flat.length);
      flat += `${line.replace(LEADER, "")} `;
    }
    for (const match of flat.matchAll(OLD_NUMBER)) {
      const line = starts.findLastIndex((start) => start <= match.index) + 1;
      found.push(
        `${path}:${line}: names the kit's document by a number of its old sections ("${match[0].replace(/\s+/g, " ")}"): name the topic's file, kit/spec/..., or link it`,
      );
    }
  }
  return found;
}
