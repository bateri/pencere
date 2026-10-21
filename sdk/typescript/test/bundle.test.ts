import { mkdir, mkdtemp, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import {
  BUNDLE_LIMITS,
  BundleError,
  bundleFromDirectory,
  bundleFromFiles,
  isBundlePath,
  isMediaType,
  mediaTypeOf,
} from "../src/bundle.js";
import { EXAMPLE_DIGESTS, EXAMPLE_FILES } from "./support.js";

const files = Object.entries(EXAMPLE_FILES).map(([path, text]) => ({
  path,
  data: Buffer.from(text),
}));

describe("a bundle of files", () => {
  it("lists them as the examples of the specification do, in the order given", () => {
    const { manifest } = bundleFromFiles(files);
    expect(manifest).toEqual({
      entry: "index.html",
      files: [
        { path: "index.html", sha256: EXAMPLE_DIGESTS["index.html"], size: 132, type: "text/html" },
        { path: "app.js", sha256: EXAMPLE_DIGESTS["app.js"], size: 67, type: "text/javascript" },
        { path: "style.css", sha256: EXAMPLE_DIGESTS["style.css"], size: 61, type: "text/css" },
      ],
    });
    expect(bundleFromFiles([...files].reverse()).manifest.files.map((f) => f.path)).toEqual([
      "style.css",
      "app.js",
      "index.html",
    ]);
  });

  it("holds the bytes of each file under its digest", () => {
    const bundle = bundleFromFiles(files);
    expect(Buffer.from(bundle.read(EXAMPLE_DIGESTS["app.js"]) ?? []).toString()).toBe(
      EXAMPLE_FILES["app.js"],
    );
    expect(bundle.read("0".repeat(64))).toBeUndefined();
  });

  it("carries the meta of the view and the entry it was given", () => {
    const meta = { prefersBorder: true };
    const bundle = bundleFromFiles([{ path: "main.html", data: Buffer.from("<p>") }], {
      entry: "main.html",
      meta,
    });
    expect(bundle.manifest).toMatchObject({ entry: "main.html", meta });
  });

  it("keeps one entry for two files with the same bytes", () => {
    const same = Buffer.from("same");
    const bundle = bundleFromFiles([
      { path: "a.txt", data: same },
      { path: "index.html", data: same },
    ]);
    expect(bundle.manifest.files.map((f) => f.sha256)).toEqual([
      bundle.manifest.files[0]?.sha256,
      bundle.manifest.files[0]?.sha256,
    ]);
    expect(bundle.read(bundle.manifest.files[0]?.sha256 ?? "")).toBeDefined();
  });

  it("refuses what §8.2 does not allow, with a reason", () => {
    const html = { path: "index.html", data: Buffer.from("<p>") };
    expect(() => bundleFromFiles([{ path: "other.html", data: Buffer.from("") }])).toThrow(
      /entry "index.html"/,
    );
    expect(() => bundleFromFiles([html, { path: "a b.js", data: Buffer.from("") }])).toThrow(
      BundleError,
    );
    expect(() => bundleFromFiles([html, { ...html }])).toThrow(/twice/);
    expect(() =>
      bundleFromFiles([html, { path: "a.js", data: Buffer.from(""), type: "Text/JS" }]),
    ).toThrow(/media type/);
  });

  it("refuses what is over a limit", () => {
    const html = { path: "index.html", data: Buffer.from("<p>") };
    const big = (path: string, size: number) => ({ path, data: Buffer.alloc(size) });
    expect(bundleFromFiles([html, big("max.bin", BUNDLE_LIMITS.file)]).manifest.files).toHaveLength(
      2,
    );
    expect(() => bundleFromFiles([html, big("max.bin", BUNDLE_LIMITS.file + 1)])).toThrow(
      /at most/,
    );
    const seven = Array.from({ length: 7 }, (_, i) => big(`f${i}.bin`, BUNDLE_LIMITS.file));
    const rest = BUNDLE_LIMITS.total - 7 * BUNDLE_LIMITS.file - html.data.length;
    expect(() => bundleFromFiles([html, ...seven, big("rest.bin", rest)])).not.toThrow();
    expect(() => bundleFromFiles([html, ...seven, big("rest.bin", rest + 1)])).toThrow(/in all/);
    const many = Array.from({ length: BUNDLE_LIMITS.files }, (_, i) => ({
      path: `f${i}.txt`,
      data: Buffer.alloc(0),
    }));
    expect(() => bundleFromFiles([html, ...many])).toThrow(/at most 1024 files/);
    expect(bundleFromFiles([html, ...many.slice(1)]).manifest.files).toHaveLength(1024);
  });
});

describe("a path", () => {
  it("is as the grammar of §8.2 says", () => {
    for (const good of ["a", "index.html", "a/b/c.js", "A_b-c.d~e", "x".repeat(255)])
      expect(isBundlePath(good), good).toBe(true);
    for (const bad of [
      "",
      "/a",
      "a/",
      "a//b",
      ".",
      "..",
      "a/../b",
      "../etc/passwd",
      "a/./b",
      "a b",
      "a\\b",
      "café",
      "a?b",
      "a#b",
      "a:b",
      "a%2e",
      "x".repeat(256),
      "a\nb",
      "a\0b",
    ])
      expect(isBundlePath(bad), JSON.stringify(bad)).toBe(false);
  });
});

describe("a media type", () => {
  it("is type/subtype, lowercase, without parameters", () => {
    for (const good of ["text/html", "application/json", "image/svg+xml", "font/woff2"])
      expect(isMediaType(good), good).toBe(true);
    for (const bad of [
      "",
      "text",
      "text/",
      "/html",
      "Text/Html",
      "text/html; charset=utf-8",
      "a/b/c",
    ])
      expect(isMediaType(bad), bad).toBe(false);
  });

  it("is told by the extension, and octet-stream when it is not known", () => {
    expect(mediaTypeOf("a/b/App.JS")).toBe("text/javascript");
    expect(mediaTypeOf("logo.svg")).toBe("image/svg+xml");
    expect(mediaTypeOf("font.woff2")).toBe("font/woff2");
    expect(mediaTypeOf("README")).toBe("application/octet-stream");
    expect(mediaTypeOf("dir.d/README")).toBe("application/octet-stream");
    expect(mediaTypeOf("x.weird")).toBe("application/octet-stream");
    expect(mediaTypeOf("x.weird", { ".weird": "application/x-weird" })).toBe("application/x-weird");
  });

  it("is one that every extension the library knows gives", () => {
    for (const extension of [".html", ".js", ".css", ".json", ".svg", ".png", ".woff2", ".wasm"])
      expect(isMediaType(mediaTypeOf(`x${extension}`))).toBe(true);
  });
});

describe("a bundle of a directory", () => {
  let dir: string;
  beforeEach(async () => {
    dir = await mkdtemp(join(tmpdir(), "pencere-bundle-"));
  });
  afterEach(async () => {
    await rm(dir, { recursive: true, force: true });
  });

  it("walks it, in byte order of the paths, without hidden files and without following a link to a directory", async () => {
    await mkdir(join(dir, "assets", "deep"), { recursive: true });
    await mkdir(join(dir, ".git"));
    await writeFile(join(dir, "index.html"), EXAMPLE_FILES["index.html"]);
    await writeFile(join(dir, "app.js"), EXAMPLE_FILES["app.js"]);
    await writeFile(join(dir, "assets", "style.css"), EXAMPLE_FILES["style.css"]);
    await writeFile(join(dir, "assets", "deep", "logo.svg"), "<svg/>");
    await writeFile(join(dir, ".DS_Store"), "junk");
    await writeFile(join(dir, ".git", "config"), "junk");
    await symlink(join(dir, "app.js"), join(dir, "linked.js"));
    await symlink(dir, join(dir, "loop"));
    const bundle = await bundleFromDirectory(dir, { types: { ".svg": "image/x-svg" } });
    expect(bundle.manifest.files.map((f) => [f.path, f.type])).toEqual([
      ["app.js", "text/javascript"],
      ["assets/deep/logo.svg", "image/x-svg"],
      ["assets/style.css", "text/css"],
      ["index.html", "text/html"],
      ["linked.js", "text/javascript"],
    ]);
    expect(bundle.read(EXAMPLE_DIGESTS["style.css"])).toBeDefined();
  });

  it("takes a URL, and says what is wrong with a name it cannot list", async () => {
    await writeFile(join(dir, "index.html"), "<p>");
    await writeFile(join(dir, "bad name.js"), "1");
    await expect(bundleFromDirectory(new URL(`file://${dir}/`))).rejects.toThrow(/bad name\.js/);
  });
});
