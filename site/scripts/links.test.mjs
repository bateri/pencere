// The rules of scripts/links.mjs, against small repositories made up here: a link into the kit's
// topics (kit/spec) that goes nowhere is found, and so is a rule written in the index.
import { describe, expect, it } from "vitest";
import {
  indexProblems,
  linkProblems,
  noteProblems,
  tagProblems,
  numberProblems,
  slug,
  topicsOf,
} from "./links.mjs";

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

describe("the topics the index lists", () => {
  it("are the files of its lines for a topic, once each and in its order", () => {
    expect(topicsOf(INDEX)).toEqual(["kit/spec/elements.md", "kit/spec/events.md"]);
  });
  it("are not the files its other lines link (the line that says what the index is)", () => {
    const text = INDEX.replace(
      "One file for each topic.",
      "One file for each topic; [Elements](spec/elements.md) says how to read them.",
    );
    expect(topicsOf(text)).toEqual(["kit/spec/elements.md", "kit/spec/events.md"]);
  });
  it("have names with digits", () => {
    expect(topicsOf("- [Two](spec/page2.md): The second.\n")).toEqual(["kit/spec/page2.md"]);
  });
});

describe("the old numbers of the kit's document", () => {
  const found = (text, path = "kit/luau/prelude/00-core.lua") =>
    numberProblems(new Map([[path, text]]));
  it("are found in a comment of code, by the path of the document", () => {
    const problems = found("-- The version of the page API (`kit/SPEC.md`, Section 1).\n");
    expect(problems).toHaveLength(1);
    expect(problems[0]).toContain("kit/luau/prelude/00-core.lua:1");
  });
  it("are found where the comment goes on at the next line, and at the line it begins", () => {
    const problems = found(
      "local a = 1\n-- What a drag may take (the rule of the kit's\n-- specification, Section 2.1): one line.\n",
    );
    expect(problems).toHaveLength(1);
    expect(problems[0]).toContain(":2");
  });
  it("are found in a document, by the words that name it", () => {
    expect(found("As the kit's SPEC, Section 4 says.\n", "cli/README.md")).toHaveLength(1);
    expect(found("See Section 2.3 of the kit's specification.\n", "cli/README.md")).toHaveLength(1);
    expect(found("The kit's document says it (§4).\n", "cli/README.md")).toHaveLength(1);
  });
  it("are not the protocol's own sections, nor a topic named by its file", () => {
    expect(found("(specification §9.2), and protocol, Section 4.5.\n")).toEqual([]);
    expect(
      found(
        "What the kit's document says (`kit/spec/events.md`): see Section 2 of the protocol.\n",
      ),
    ).toEqual([]);
    expect(
      found("The kit's library is in `kit/spec/library.md`.\nSection 5 is the protocol's.\n"),
    ).toEqual([]);
  });
});

describe("the notes of the topics", () => {
  const motion = "# Motion\n\nA motion.[^style]\n\n[^style]: A style sheet's `transition`.\n";
  it("are fine when each is said once and has its words", () => {
    expect(noteProblems(files({ "kit/spec/motion.md": motion }))).toEqual([]);
  });
  it("are found when one is said and has no words, or has words and is not said", () => {
    const lost = noteProblems(files({ "kit/spec/motion.md": "# Motion\n\nA motion.[^style]\n" }));
    expect(lost).toHaveLength(1);
    expect(lost[0]).toContain("has no words");
    const unsaid = noteProblems(
      files({ "kit/spec/motion.md": "# Motion\n\nA motion.\n\n[^style]: Words.\n" }),
    );
    expect(unsaid).toHaveLength(1);
    expect(unsaid[0]).toContain("nothing says it");
  });
  it("are found when one is said twice, or a name is in two topics", () => {
    expect(
      noteProblems(
        files({ "kit/spec/motion.md": `${motion.replace("A motion.", "A motion.[^style]")}` }),
      )[0],
    ).toContain("2 times");
    const two = noteProblems(
      files({
        "kit/spec/motion.md": motion,
        "kit/spec/layout.md": motion.replace("Motion", "Layout"),
      }),
    );
    expect(two).toHaveLength(1);
    expect(two[0]).toContain("also in kit/spec/motion.md");
  });
  it("are not read in a block of code or in a span of it", () => {
    const code = "# Motion\n\n```md\nA note[^x] is written so.\n```\n\nOr `[^y]`.\n";
    expect(noteProblems(files({ "kit/spec/motion.md": code }))).toEqual([]);
  });
  it("are not read in a block of code fenced with tildes, either", () => {
    const code = "# Motion\n\n~~~md\nA note[^x] is written so.\n```\n[^z]: Words.\n~~~\n\nAfter.\n";
    expect(noteProblems(files({ "kit/spec/motion.md": code }))).toEqual([]);
    // A fence is closed by its own mark: a tilde line does not end a block of backticks.
    const mixed = "# Motion\n\n````md\n~~~\nA[^x]\n````\n\nAfter.\n";
    expect(noteProblems(files({ "kit/spec/motion.md": mixed }))).toEqual([]);
  });
  it("are the same note in any case, as GitHub reads the labels", () => {
    const mixed = "# Motion\n\nA motion.[^Style]\n\n[^style]: Words.\n";
    expect(noteProblems(files({ "kit/spec/motion.md": mixed }))).toEqual([]);
    const twice = "# Motion\n\nA.[^Style] B.[^style]\n\n[^STYLE]: Words.\n";
    const found = noteProblems(files({ "kit/spec/motion.md": twice }));
    expect(found).toHaveLength(1);
    expect(found[0]).toContain("2 times");
    const two = noteProblems(
      files({
        "kit/spec/motion.md": motion,
        "kit/spec/layout.md": motion.replace("Motion", "Layout").replaceAll("style", "STYLE"),
      }),
    );
    expect(two).toHaveLength(1);
    expect(two[0]).toContain("also in kit/spec/motion.md");
  });
  it("are found when one holds a list, a block of code or more than one paragraph", () => {
    const note = (words) => `# Motion\n\nA motion.[^style]\n\n${words}\n`;
    const list = noteProblems(
      files({ "kit/spec/motion.md": note("[^style]: Kinds:\n    - one\n    - two") }),
    );
    expect(list).toHaveLength(1);
    expect(list[0]).toContain("holds a list");
    expect(
      noteProblems(files({ "kit/spec/motion.md": note("[^style]: - one\n    - two") })),
    ).toHaveLength(1);
    expect(
      noteProblems(
        files({ "kit/spec/motion.md": note("[^style]: Words.\n\n    A second paragraph.") }),
      ),
    ).toHaveLength(1);
    expect(
      noteProblems(
        files({ "kit/spec/motion.md": note("[^style]: Words.\n\n    ```lua\n    x()\n    ```") }),
      ),
    ).toHaveLength(1);
  });
  it("are fine when a sentence wraps over lines, or the next paragraph is no part of the note", () => {
    const wrapped =
      "# Motion\n\nA motion.[^style]\n\n[^style]: Words that\n    wrap, and wrap\nagain.\n\nAfter, and a list:\n\n- one\n- two\n";
    expect(noteProblems(files({ "kit/spec/motion.md": wrapped }))).toEqual([]);
  });
});

describe("tagProblems", () => {
  it("finds a tag written in a topic's words", () => {
    const text = 'A slider said "<title> column width".\n';
    expect(tagProblems(files({ "kit/spec/library.md": text }))).toEqual([
      "kit/spec/library.md:1: <title> is written as HTML, and a page would take it as a tag: set it as code",
    ]);
  });
  it("lets a tag be in code, a block of code or an address", () => {
    const text =
      'Said "`<title>` column width", ``a `<b>` c``, and <https://pencere.dev>.\n\n```html\n<title>x</title>\n```\n';
    expect(tagProblems(files({ "kit/spec/library.md": text }))).toEqual([]);
  });
});
