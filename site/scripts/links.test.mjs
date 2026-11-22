// The rules of scripts/links.mjs, against small repositories made up here: a link into the kit's
// topics (kit/spec) that goes nowhere is found, and so is a rule written in the index.
import { describe, expect, it } from "vitest";
import { indexProblems, linkProblems, slug } from "./links.mjs";

const files = (entries) => new Map(Object.entries(entries));

const INDEX = [
  "# The kit",
  "",
  "One file for each topic.",
  "",
  "## The page",
  "",
  "- [Elements](spec/elements.md): What a page is made of.",
  "- [Events](spec/events.md): What the host tells a page.",
  "",
].join("\n");
const ELEMENTS = "# Elements\n\nAn element.\n\n## The builders\n\nBuilt by `box()`.\n";
const EVENTS = "# Events\n\n## What a page is told\n\nSee [Elements](elements.md#the-builders).\n";

describe("the slug of a heading", () => {
  it("is the one the site's builder gives it", () => {
    expect(slug("The builders")).toBe("the-builders");
    expect(slug("`@pencere/syntax`")).toBe("penceresyntax");
    expect(slug("What is not seen, the document and its places")).toBe(
      "what-is-not-seen-the-document-and-its-places",
    );
    expect(slug("The derived tones (RECOMMENDED)")).toBe("the-derived-tones-recommended");
  });
});

describe("the links into the topics", () => {
  const base = {
    "kit/SPEC.md": INDEX,
    "kit/spec/elements.md": ELEMENTS,
    "kit/spec/events.md": EVENTS,
  };
  it("are none when each goes to a file and a heading that are there", () => {
    expect(linkProblems(files(base))).toEqual([]);
  });
  it("find a link to a file that is not there", () => {
    const found = linkProblems(
      files({ ...base, "spec/draft/pencere.md": "See [it](../../kit/spec/nothing.md).\n" }),
    );
    expect(found).toHaveLength(1);
    expect(found[0]).toContain("spec/draft/pencere.md");
    expect(found[0]).toContain("kit/spec/nothing.md");
  });
  it("find a link to a heading that is not there", () => {
    const found = linkProblems(
      files({ ...base, "kit/spec/events.md": EVENTS.replace("the-builders", "the-builder") }),
    );
    expect(found).toHaveLength(1);
    expect(found[0]).toContain("#the-builder");
  });
  it("find a mention in a comment of code, by its path", () => {
    const found = linkProblems(
      files({ ...base, "kit/drive/src/lib.rs": "//! what `kit/spec/gone.md` asks\n" }),
    );
    expect(found).toHaveLength(1);
    expect(found[0]).toContain("kit/drive/src/lib.rs");
  });
  it("find a link by an address of the repository on the web", () => {
    const found = linkProblems(
      files({
        ...base,
        "README.md": "[x](https://github.com/bateri/pencere/blob/main/kit/spec/elements.md#nope)\n",
      }),
    );
    expect(found).toHaveLength(1);
  });
});

describe("the index of the topics", () => {
  const topics = ["kit/spec/elements.md", "kit/spec/events.md"];
  it("is only headings and a line for each topic", () => {
    expect(indexProblems(INDEX, topics)).toEqual([]);
  });
  it("holds no rule", () => {
    const found = indexProblems(
      INDEX.replace("One file for each topic.", "A host MUST draw every element."),
      topics,
    );
    expect(found.join("\n")).toMatch(/MUST/);
  });
  it("holds no sentence that is no description of a topic", () => {
    const found = indexProblems(`${INDEX}\nA page is a table that a script returns.\n`, topics);
    expect(found.join("\n")).toContain("A page is a table");
  });
  it("lists each topic once", () => {
    expect(indexProblems(INDEX.replace(/- \[Events\].*\n/, ""), topics).join("\n")).toContain(
      "kit/spec/events.md",
    );
    expect(
      indexProblems(`${INDEX}- [Events](spec/events.md): Again.\n`, topics).join("\n"),
    ).toContain("more than once");
  });
});
