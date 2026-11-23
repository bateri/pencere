// The notes of a document (src/lib/notes.js): what the builder makes of a Markdown footnote is taken
// apart, each note is said after its number in the margin's place, and the section at the end has the
// notes of what the page shows.
import { describe, expect, it } from "vitest";
import { placeNotes, takeNotes } from "../src/lib/notes.js";

const ref = (name, number) =>
  `<sup><a href="#user-content-fn-${name}" id="user-content-fnref-${name}" data-footnote-ref aria-describedby="footnote-label">${number}</a></sup>`;
const section = (items) =>
  `<section data-footnotes class="footnotes"><h2 class="sr-only" id="footnote-label">Footnotes</h2>\n<ol>\n${items
    .map(
      ([name, words], index) =>
        `<li id="user-content-fn-${name}">\n<p>${words} <a href="#user-content-fnref-${name}" data-footnote-backref="" aria-label="Back to reference ${index + 1}" class="data-footnote-backref">↩</a></p>\n</li>`,
    )
    .join("\n")}\n</ol>\n</section>`;

describe("the notes of a document", () => {
  const html = `<p>One${ref("a", 1)} and two${ref("b", 2)}.</p>\n${section([
    ["a", "First."],
    ["b", "Second."],
  ])}`;

  it("are taken out with the heading the builder gives them", () => {
    const { html: bare, notes } = takeNotes(html);
    expect(bare).not.toContain("data-footnotes");
    expect(bare).not.toContain("Footnotes");
    expect([...notes.keys()]).toEqual(["a", "b"]);
  });

  it("are said after their number, in a line, with no link back", () => {
    const { html: bare, notes } = takeNotes(html);
    const { html: placed, end } = placeNotes(bare, notes);
    expect(placed).toContain(
      '<span class="note__number" aria-hidden="true" data-pagefind-ignore>1</span> First.</span>',
    );
    expect(placed).not.toContain("↩");
    expect(end).toContain("Back to reference 2");
    expect(end).not.toContain("Footnotes");
    // The number links to the end of the page, and is said again where it is no link; the copy that is
    // shown in the other layout is no second reading for a screen reader or the search.
    expect(placed).toContain(
      '<a class="note__end" href="#user-content-fn-a" id="user-content-fnref-a">1</a>',
    );
    expect(placed).toContain(
      '<span class="note__here" aria-hidden="true" data-pagefind-ignore>1</span>',
    );
    expect(end).toContain("data-pagefind-ignore");
  });

  it("are numbered as they come on the page, and a part of the page has the notes of that part", () => {
    const { notes } = takeNotes(html);
    const { html: placed, end } = placeNotes(`<p>Only two${ref("b", 2)}.</p>`, notes);
    expect(placed).toContain(
      '<span class="note__number" aria-hidden="true" data-pagefind-ignore>1</span> Second.',
    );
    expect(end).toContain('id="user-content-fn-b"');
    expect(end).not.toContain('id="user-content-fn-a"');
    expect(end).toContain("Back to reference 1");
  });

  it("are the sections of several documents together, and a page without a note has no section", () => {
    const both = `<p>x${ref("a", 1)}</p>${section([["a", "A."]])}<p>y${ref("b", 1)}</p>${section([["b", "B."]])}`;
    const { html: bare, notes } = takeNotes(both);
    expect([...notes.keys()]).toEqual(["a", "b"]);
    const { html: placed } = placeNotes(bare, notes);
    expect(placed).toContain(
      '<span class="note__number" aria-hidden="true" data-pagefind-ignore>2</span> B.',
    );
    expect(placeNotes("<p>Nothing.</p>", new Map())).toEqual({ html: "<p>Nothing.</p>", end: "" });
  });
});
