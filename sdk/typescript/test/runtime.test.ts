/**
 * What lets the same code run on Node, Deno and Bun: the runtime is told from its globals, and the
 * library keeps to what all three have.
 */

import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { detectRuntime, runtime } from "../src/terminal.js";

const SRC = fileURLToPath(new URL("../src/", import.meta.url));

/** The code of a source file, without its comments. */
function code(path: string): string {
  return readFileSync(path, "utf8")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|[^:\\])\/\/.*$/gm, "$1");
}

const sources = (dir = SRC): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((item) =>
    item.isDirectory()
      ? sources(`${dir}${item.name}/`)
      : item.name.endsWith(".ts")
        ? [`${dir}${item.name}`]
        : [],
  );

describe("detectRuntime", () => {
  it("tells Deno and Bun by the global each has, and Node by neither", () => {
    expect(detectRuntime({})).toBe("node");
    expect(detectRuntime({ Deno: {} })).toBe("deno");
    expect(detectRuntime({ Bun: {} })).toBe("bun");
    expect(detectRuntime({ process: {} })).toBe("node");
  });

  it("is node where these tests run", () => {
    expect(runtime).toBe("node");
    expect(detectRuntime()).toBe("node");
  });
});

describe("the library's source", () => {
  it("uses no `Buffer`: Deno has none without an import, and a compiled program has only the platform", () => {
    const offenders = sources().filter((path) => /\bBuffer\b/.test(code(path)));
    expect(offenders).toEqual([]);
  });

  it("keeps the module of a page free of imports: a browser loads it as a file", () => {
    const page = code(`${SRC}page/index.ts`);
    expect(page).not.toMatch(/^\s*import\b/m);
    expect(page).not.toMatch(/\bfrom\s+["']/);
    expect(page).not.toMatch(/\brequire\(/);
    expect(page).not.toMatch(/\bprocess\b/);
  });

  it("keeps the tty's file free of imports of the library: its tests run it from source on each runtime", () => {
    const imports = [...code(`${SRC}terminal.ts`).matchAll(/\bfrom\s+["']([^"']+)["']/g)].map(
      (match) => match[1],
    );
    expect(imports.sort()).toEqual(["node:child_process", "node:fs"]);
  });
});
