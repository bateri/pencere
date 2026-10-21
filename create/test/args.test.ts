import { describe, expect, it } from "vitest";

import { UsageError, parse } from "../src/args.js";

const fails = (argv: string[]) => {
  try {
    parse(argv);
  } catch (error) {
    expect(error).toBeInstanceOf(UsageError);
    return (error as Error).message;
  }
  throw new Error(`${JSON.stringify(argv)} parsed`);
};

describe("create-pencere's command line", () => {
  it("asks for nothing it is not given", () => {
    expect(parse([])).toEqual({ kind: "create" });
    expect(parse(["my-tool"])).toEqual({ kind: "create", directory: "my-tool" });
    expect(parse(["--template", "vue"])).toEqual({ kind: "create", template: "vue" });
  });

  it("reads the directory and the template in any order, in long and short form", () => {
    for (const argv of [
      ["my-tool", "--template", "vue"],
      ["--template=vue", "my-tool"],
      ["-t", "vue", "my-tool"],
    ])
      expect(parse(argv)).toEqual({ kind: "create", directory: "my-tool", template: "vue" });
  });

  it("knows help and the version, whatever else is on the line", () => {
    expect(parse(["--help"])).toEqual({ kind: "help" });
    expect(parse(["-h", "x"])).toEqual({ kind: "help" });
    expect(parse(["--version"])).toEqual({ kind: "version" });
    expect(parse(["-v"])).toEqual({ kind: "version" });
  });

  it("says which templates there are when it is asked for another", () => {
    expect(fails(["--template", "svelte"])).toContain("react and vue");
  });

  it("refuses a second directory, an empty one and an option it does not have", () => {
    expect(fails(["a", "b"])).toContain("one directory");
    expect(fails([""])).toContain("empty");
    expect(fails(["--tempate", "vue"])).toContain("tempate");
    expect(fails(["--template"])).toContain("template");
  });
});
