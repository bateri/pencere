// The words of the catalogue as HTML (scripts/words.mjs): code is set as code, a link is its words,
// and a link whose words are code is no raw Markdown in the card.
import { describe, expect, it } from "vitest";
import { html } from "./words.mjs";

describe("the words of a sentence as HTML", () => {
  it("set code as code and escape the rest", () => {
    expect(html("A `<b>` & more")).toBe("A <code>&lt;b&gt;</code> &amp; more");
  });

  it("take a link for its words", () => {
    expect(html("See [the rules](kit/spec/page.md#a) now")).toBe("See the rules now");
  });

  it("take a link whose words are code for the code, not the Markdown of the link", () => {
    expect(html("Text from [`pencere.span`](#rich-text), as `rich`.")).toBe(
      "Text from <code>pencere.span</code>, as <code>rich</code>.",
    );
  });

  it("leave the shape of a link inside a span of code as it is written", () => {
    expect(html("Write `[words](address)` for one")).toBe(
      "Write <code>[words](address)</code> for one",
    );
  });
});
