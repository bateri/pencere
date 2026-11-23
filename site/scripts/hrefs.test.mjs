// The rule of scripts/hrefs.mjs, against pages made up here: a link left as a path of the repository
// is found, a link to a page, to a heading or to the web is not.
import { describe, expect, it } from "vitest";
import { strays } from "./hrefs.mjs";

describe("the links of a page", () => {
  it("are found when one goes to a Markdown file or climbs out with ../, as the notes at a page's end did", () => {
    const page =
      '<p><a href="../SPEC.md">the kit</a> <a href="spec/page.md#a-page">a topic</a> <a href="../kit/">up</a></p>' +
      '<section data-footnotes><ol><li><a href="../SPEC.md">the kit</a></li></ol></section>';
    expect(strays(page)).toEqual(["../SPEC.md", "spec/page.md#a-page", "../kit/", "../SPEC.md"]);
  });

  it("are quiet where each goes to a page, a heading or the web, even a file of Markdown there", () => {
    const page =
      '<a href="/kit/spec/page#a-page">a</a><a href="#top">b</a><a href="https://github.com/o/r/blob/main/kit/SPEC.md">c</a>' +
      '<a href="//example.com/x.md">d</a><a href="mailto:me@example.com">e</a><a class="x" href=\'/kit\'>f</a>';
    expect(strays(page)).toEqual([]);
  });

  it("do not take a script, a comment or a tag that is no link for one", () => {
    const page =
      '<!-- <a href="../x.md">x</a> --><script>const s = \'<a href="../y.md">y</a>\';</script><link href="../z.md"><b data-href="../w.md">w</b>';
    expect(strays(page)).toEqual([]);
  });
});
