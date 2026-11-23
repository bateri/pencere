// The rule of scripts/ids.mjs, against pages made up here: a heading's id given twice is found, an
// id that is only in a script or a comment is not.
import { describe, expect, it } from "vitest";
import { duplicates } from "./ids.mjs";

describe("the ids of a page", () => {
  it("finds an id that two elements have, as two topics with one heading give", () => {
    const page =
      '<main><h3 id="the-roles">The roles</h3><p>.</p><h4 id="the-roles">The roles</h4><h4 id="other">x</h4></main>';
    expect(duplicates(page)).toEqual([["the-roles", 2]]);
  });

  it("finds an id of any element, in any quoting", () => {
    expect(duplicates("<div id=a></div><span id='a'></span><b id=\"a\"></b>")).toEqual([["a", 3]]);
  });

  it("does not take a longer name for the id, nor a script or a comment for an element", () => {
    const page =
      '<a data-id="x" href="#x">go</a><i id="x"></i><!-- <u id="x"></u> --><script>const s = \'<p id="x"></p>\';</script>';
    expect(duplicates(page)).toEqual([]);
  });

  it("is quiet where every id is its own", () => {
    expect(duplicates('<h2 id="a">A</h2><h2 id="b">B</h2>')).toEqual([]);
  });
});
