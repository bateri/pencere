// The rules of scripts/regions.mjs: the parts an example marks are found, and a part that no heading
// of the topic names, a mark that is not closed and a name given twice are each found.
import { describe, expect, it } from "vitest";
import { regionProblems, regionsOf } from "./regions.mjs";

const CODE = [
  "local x = 1",
  "-- #region A spring",
  "local function springs()",
  "\treturn 1",
  "end",
  "-- #endregion",
  "",
  "-- #region What goes round and round",
  "local function going_round() end",
  "-- #endregion",
  "",
].join("\n");
const DOCUMENT = "# Motion\n\n## A spring\n\nText.\n\n### What goes round and round\n\nText.\n";

describe("the parts of a file", () => {
  it("are read by their names, with the code between the marks", () => {
    const { regions, problems } = regionsOf(CODE);
    expect(problems).toEqual([]);
    expect([...regions.keys()]).toEqual(["A spring", "What goes round and round"]);
    expect(regions.get("A spring")).toBe("local function springs()\n\treturn 1\nend");
  });

  it("have the indentation they all share taken off", () => {
    const { regions } = regionsOf("-- #region A\n\tone\n\t\ttwo\n-- #endregion\n");
    expect(regions.get("A")).toBe("one\n\ttwo");
  });

  it("are found when one starts inside another, is never ended or is named twice", () => {
    expect(regionsOf("-- #region A\n-- #region B\n-- #endregion\n").problems.join("\n")).toContain(
      "starts inside",
    );
    expect(regionsOf("-- #region A\ncode\n").problems.join("\n")).toContain("is never ended");
    expect(
      regionsOf("-- #region A\n-- #endregion\n-- #region A\n-- #endregion\n").problems.join("\n"),
    ).toContain("two parts named");
    expect(regionsOf("-- #endregion\n").problems.join("\n")).toContain("none has started");
  });

  it("are found when a mark has no name, on the line of the mark", () => {
    const { regions, problems } = regionsOf(
      "local x = 1\n-- #region\ncode\n-- #endregion\n",
      "f.lua",
    );
    expect(problems).toEqual(["f.lua:2: a part has no name"]);
    expect(regions.size).toBe(0);
    expect(regionsOf("-- #region   \ncode\n-- #endregion\n", "f.lua").problems).toEqual([
      "f.lua:1: a part has no name",
    ]);
    // A word that only starts with the mark's is no mark.
    expect(regionsOf("-- #regional\n").problems).toEqual([]);
  });
});

describe("the parts and the headings of the topic", () => {
  it("agree when each part has a heading of its name", () => {
    expect(regionProblems("page.lua", CODE, DOCUMENT, "kit/spec/motion.md")).toEqual([]);
  });

  it("do not when a heading is renamed, and the part is left for no page", () => {
    const renamed = DOCUMENT.replace("## A spring", "## Springs");
    const found = regionProblems("page.lua", CODE, renamed, "kit/spec/motion.md");
    expect(found).toHaveLength(1);
    expect(found[0]).toContain('the part "A spring" has no heading of its name');
  });

  it("do not when a heading of the first depth, or a line of prose, only says the name", () => {
    const found = regionProblems(
      "page.lua",
      CODE,
      "# A spring\n\nWhat goes round and round.\n",
      "kit/spec/motion.md",
    );
    expect(found).toHaveLength(2);
  });

  it("agree when the heading and the part spell a quote or an ampersand differently", () => {
    const code = "-- #region It's here & now\ncode\n-- #endregion\n";
    const found = regionProblems("page.lua", code, "# T\n\n## It\u2019s here & now\n", "k.md");
    expect(found).toEqual([]);
    expect(regionProblems("page.lua", code, "# T\n\n## It's here & now\n", "k.md")).toEqual([]);
  });

  it("do not when two parts are one heading's slug", () => {
    const code = "-- #region It's\na\n-- #endregion\n-- #region Its\nb\n-- #endregion\n";
    const found = regionProblems("page.lua", code, "# T\n\n## Its\n", "k.md");
    expect(found).toHaveLength(1);
    expect(found[0]).toContain("one slug");
  });

  it("do not when the file has no part at all", () => {
    expect(
      regionProblems("page.lua", "local x = 1\n", DOCUMENT, "kit/spec/motion.md")[0],
    ).toContain("has no part");
  });
});
