// The rule of scripts/repeats.mjs, against pages made up here: a sentence said by the cards and again
// by the table is found, and so is a card that is gone.
import { describe, expect, it } from "vitest";
import { moduleProblems, problems, said } from "./repeats.mjs";

const ELEMENTS = [
  { name: "div", doc: "A box: a column of its children until told otherwise." },
  {
    name: "text",
    doc: "One line of text, with <code>wrap</code>, as many lines as its box needs.",
  },
];
const CARDS =
  '<a class="card"><b>pencere.div</b><p>A box: a column of its children until told otherwise.</p></a>' +
  '<a class="card"><b>pencere.text</b><p>One line of text, with <code>wrap</code>, as many lines as its box needs.</p></a>';
const TABLE =
  "<table><tr><td><code>pencere.div()</code></td><td>A box: a column of its children until told otherwise.</td></tr></table>";
const page = (inside) =>
  `<html><head><title>A box: a column of its children until told otherwise.</title></head><body><nav>menu</nav><main>${inside}</main></body></html>`;

describe("what the elements' pages say", () => {
  it("counts a sentence over the tags inside it, in the main part of the page alone", () => {
    expect(
      said(
        page(CARDS),
        ELEMENTS.map((element) => element.doc),
      ),
    ).toEqual([1, 1]);
  });

  it("finds the sentence of a constructor said by the cards and by the table", () => {
    expect(problems("kit/elements", page(CARDS), ELEMENTS)).toEqual([]);
    expect(problems("kit/elements", page(CARDS + TABLE), ELEMENTS)).toEqual([
      "kit/elements: what pencere.div is, is said 2 times, and once is how it is said there",
    ]);
  });

  it("takes the curly quotes of the document's table for the straight ones of the catalogue", () => {
    const element = { name: "image", doc: "A picture: a file of the view's own bundle." };
    const table =
      "<table><tr><td>A picture: a file of the view\u2019s own bundle.</td></tr></table>";
    const card = '<a class="card"><p>A picture: a file of the view\'s own bundle.</p></a>';
    expect(problems("kit/elements", page(card + table), [element])).toEqual([
      "kit/elements: what pencere.image is, is said 2 times, and once is how it is said there",
    ]);
  });

  it("finds a card that is gone", () => {
    expect(problems("kit/elements", page(CARDS.split("</a>")[0] + "</a>"), ELEMENTS)).toEqual([
      "kit/elements: what pencere.text is, is said 0 times, and once is how it is said there",
    ]);
  });
});

describe("what the library's pages say", () => {
  const MODULES = [
    { name: "@pencere/keys", says: "Keys as a page writes them, as a person reads them." },
    {
      name: "@pencere/fuzzy",
      says: "What a person types, found in texts the way a picker finds it.",
    },
  ];
  const list = MODULES.map(
    (module) => `<li><a><b>${module.name}</b><span>${module.says}</span></a></li>`,
  ).join("");

  it("finds the overview that says every module once", () => {
    expect(moduleProblems("kit/library", page(`<ul>${list}</ul>`), MODULES)).toEqual([]);
  });

  it("finds a module's sentence said again, on its own page and in a list", () => {
    expect(
      moduleProblems("kit/library/keys", page(`<p>${MODULES[0].says}</p>${list}`), [MODULES[0]]),
    ).toEqual([
      "kit/library/keys: what @pencere/keys opens with is said 2 times, and once is how it is said there",
    ]);
  });

  it("finds a module whose sentence is not there", () => {
    expect(
      moduleProblems("kit/library", page(`<ul>${list.split("</li>")[0]}</li></ul>`), MODULES),
    ).toEqual([
      "kit/library: what @pencere/fuzzy opens with is said 0 times, and once is how it is said there",
    ]);
  });
});
