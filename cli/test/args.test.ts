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

describe("pencere run", () => {
  it("takes everything after -- as the server's command", () => {
    expect(parse(["run", "--", "python", "server.py"])).toEqual({
      kind: "run",
      command: "python",
      args: ["server.py"],
    });
  });

  it("reads its own options before --, and none of the server's flags after it", () => {
    expect(
      parse([
        "run",
        "--tool",
        "list",
        "--args",
        '{"all":true}',
        "--view",
        "./view",
        "--",
        "node",
        "server.js",
        "--tool",
        "x",
        "--",
        "-h",
      ]),
    ).toEqual({
      kind: "run",
      command: "node",
      args: ["server.js", "--tool", "x", "--", "-h"],
      tool: "list",
      arguments: { all: true },
      view: "./view",
    });
  });

  it("reads --option=value too", () => {
    expect(parse(["run", "--tool=list", "--view=v", "--", "s"])).toMatchObject({
      tool: "list",
      view: "v",
    });
  });

  it("names the mistakes in the command line", () => {
    expect(fails(["run", "node", "server.js"])).toContain("goes after --");
    expect(fails(["run"])).toContain("after --");
    expect(fails(["run", "--tool", "a"])).toContain("after --");
    expect(fails(["run", "--"])).toContain("no command");
    expect(fails(["run", "--bogus", "--", "s"])).toContain("--bogus");
    expect(fails(["run", "--tool", "--", "s"])).toContain("--tool");
    expect(fails(["run", "--tool", "", "--", "s"])).toContain("needs a value");
    expect(fails(["run", "--view", "", "--", "s"])).toContain("needs a value");
    expect(fails(["run", "--args", "{", "--", "s"])).toContain("not JSON");
    expect(fails(["run", "--args", "[1]", "--", "s"])).toContain("JSON object");
    expect(fails(["run", "--args", "null", "--", "s"])).toContain("JSON object");
    expect(fails(["run", "--args", '"x"', "--", "s"])).toContain("JSON object");
  });

  it("is one of two commands, and one is needed", () => {
    expect(fails([])).toContain("no command");
    expect(fails(["watch", "--", "s"])).toContain('unknown command "watch"');
  });

  it("does not watch: that is for dev", () => {
    expect(fails(["run", "--watch", "server.py", "--", "s"])).toContain("`pencere dev`");
  });

  it("answers help and version without a server's command", () => {
    for (const argv of [["--help"], ["-h"], ["help"], ["run", "--help"], ["run", "-h"]])
      expect(parse(argv)).toEqual({ kind: "help" });
    expect(parse(["run", "--tool", "a", "--help"])).toEqual({ kind: "help" });
    expect(parse(["--version"])).toEqual({ kind: "version" });
    expect(parse(["-v"])).toEqual({ kind: "version" });
    expect(parse(["dev", "--help"])).toEqual({ kind: "help" });
    expect(parse(["dev", "--watch", "a", "-h"])).toEqual({ kind: "help" });
  });
});

describe("pencere dev", () => {
  it("takes a view directory, the server's own paths and the server's command", () => {
    expect(
      parse([
        "dev",
        "--view",
        "dist/view",
        "--watch",
        "server.py",
        "--watch=lib",
        "--tool",
        "list",
        "--",
        "python",
        "server.py",
        "--watch",
        "x",
      ]),
    ).toEqual({
      kind: "dev",
      command: "python",
      args: ["server.py", "--watch", "x"],
      tool: "list",
      view: "dist/view",
      watch: ["server.py", "lib"],
    });
  });

  it("is enough with either the view or the server's paths", () => {
    expect(parse(["dev", "--view", "v", "--", "s"])).toEqual({
      kind: "dev",
      command: "s",
      args: [],
      view: "v",
      watch: [],
    });
    expect(parse(["dev", "--watch", "s.py", "--", "python", "s.py"])).toEqual({
      kind: "dev",
      command: "python",
      args: ["s.py"],
      watch: ["s.py"],
    });
  });

  it("names what is wrong", () => {
    expect(fails(["dev", "--", "s"])).toContain("nothing to watch");
    expect(fails(["dev", "--view", "v"])).toContain("after --");
    expect(fails(["dev", "--watch", "", "--", "s"])).toContain("--watch needs a value");
    expect(fails(["dev", "--watch", "--", "s"])).toContain("--watch");
    expect(fails(["dev", "--view", "v", "--args", "[]", "--", "s"])).toContain("JSON object");
  });
});
