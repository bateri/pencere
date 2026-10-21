import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vitest";

import { TEMPLATES } from "../src/args.js";
import type { Template } from "../src/args.js";
import { ScaffoldError, check, packageName, plan, titleOf, write } from "../src/scaffold.js";

const REAL = new URL("../templates/", import.meta.url);

const dirs: string[] = [];
afterEach(async () => {
  for (const dir of dirs.splice(0)) await rm(dir, { recursive: true, force: true });
});

async function directory(): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "create-pencere-"));
  dirs.push(dir);
  return dir;
}

const planned = async (template: Template, name = "my-tool") => plan(REAL, { name, template });

const textOf = (files: readonly { path: string; text: string }[], path: string): string => {
  const file = files.find((candidate) => candidate.path === path);
  if (file === undefined) throw new Error(`${path} is not planned`);
  return file.text;
};

describe("the name of a package", () => {
  it("is the directory's name, in the form npm and a ui:// address take", () => {
    expect(packageName("my-tool")).toBe("my-tool");
    expect(packageName("My Tool")).toBe("my-tool");
    expect(packageName("  Tool_1.2  ")).toBe("tool_1.2");
    expect(packageName("café app")).toBe("caf-app");
    expect(packageName(".hidden")).toBe("hidden");
    expect(packageName("a--b")).toBe("a--b");
  });

  it("is nothing when the directory's name holds nothing of one", () => {
    expect(packageName("")).toBeUndefined();
    expect(packageName("___")).toBeUndefined();
    expect(packageName("日本語")).toBeUndefined();
  });

  it("reads as words in a title", () => {
    expect(titleOf("my-tool")).toBe("My Tool");
    expect(titleOf("tool_1.2")).toBe("Tool 1 2");
    expect(titleOf("x")).toBe("X");
  });
});

describe("a template made into a project", () => {
  it("lays the framework's files over the common ones, and the framework wins", async () => {
    const root = await directory();
    for (const [path, text] of [
      ["common/a.txt", "common {{name}}"],
      ["common/b.txt", "only common"],
      ["react/a.txt", "react {{framework}}"],
      ["common/deep/c.txt", "{{title}}"],
    ] as const) {
      await mkdir(join(root, path, ".."), { recursive: true });
      await writeFile(join(root, path), text);
    }
    expect(await plan(root, { name: "my-tool", template: "react" })).toEqual([
      { path: "a.txt", text: "react React" },
      { path: "b.txt", text: "only common" },
      { path: "deep/c.txt", text: "My Tool" },
    ]);
  });

  it("keeps .gitignore under the name npm does not strip from a package", async () => {
    const root = await directory();
    await mkdir(join(root, "common/sub"), { recursive: true });
    await mkdir(join(root, "vue"), { recursive: true });
    await writeFile(join(root, "common/_gitignore"), "dist/\n");
    await writeFile(join(root, "common/sub/_gitignore"), "x\n");
    expect((await plan(root, { name: "t", template: "vue" })).map((file) => file.path)).toEqual([
      ".gitignore",
      "sub/.gitignore",
    ]);
  });

  it("says when a template is not there", async () => {
    const root = await directory();
    await mkdir(join(root, "common"));
    await expect(plan(root, { name: "t", template: "vue" })).rejects.toThrow(
      new ScaffoldError('there is no template "vue"'),
    );
  });

  for (const template of TEMPLATES) {
    describe(`the ${template} template`, () => {
      it("has what a pencere tool is made of", async () => {
        const paths = (await planned(template)).map((file) => file.path);
        expect(paths).toEqual(
          expect.arrayContaining([
            ".gitignore",
            "README.md",
            "index.html",
            "package.json",
            "scripts/dev.mjs",
            "src/server/main.ts",
            "src/server/tool.ts",
            "src/view/styles.css",
            "tsconfig.json",
            "tsconfig.server.json",
            "vite.config.ts",
          ]),
        );
        expect(paths).not.toContain("_gitignore");
        expect(paths.filter((path) => path.startsWith("src/view/main."))).toHaveLength(1);
      });

      it("has the name, the title and the framework in place, and no placeholder left", async () => {
        const files = await planned(template, "my-tool");
        for (const file of files) expect(file.text, file.path).not.toMatch(/\{\{[a-z]+\}\}/);
        expect(JSON.parse(textOf(files, "package.json")).name).toBe("my-tool");
        expect(textOf(files, "src/server/tool.ts")).toContain('"ui://my-tool/view"');
        expect(textOf(files, "src/server/tool.ts")).toContain('title: "My Tool"');
        expect(textOf(files, "index.html")).toContain("<title>My Tool</title>");
        expect(textOf(files, "README.md")).toMatch(/^# My Tool$/m);
        expect(textOf(files, "README.md")).toContain(template === "react" ? "React" : "Vue");
      });

      it("names no placeholder that is not filled in", async () => {
        // A template file is read as it stands, which is how a misspelt placeholder shows.
        const root = fileURLToPath(REAL);
        for (const layer of ["common", template])
          for (const entry of await readdir(join(root, layer), {
            recursive: true,
            withFileTypes: true,
          })) {
            if (!entry.isFile()) continue;
            const text = await readFile(join(entry.parentPath, entry.name), "utf8");
            for (const [placeholder] of text.matchAll(/\{\{[a-z]+\}\}/g))
              expect(["{{name}}", "{{title}}", "{{framework}}"], entry.name).toContain(placeholder);
          }
      });

      it("asks for @pencere/sdk and @pencere/cli by name, and for no version that floats", async () => {
        const manifest = JSON.parse(textOf(await planned(template), "package.json")) as {
          dependencies: Record<string, string>;
          devDependencies: Record<string, string>;
          scripts: Record<string, string>;
          type: string;
        };
        expect(manifest.dependencies["@pencere/sdk"]).toBeDefined();
        expect(manifest.devDependencies["@pencere/cli"]).toBeDefined();
        for (const [name, range] of Object.entries({
          ...manifest.dependencies,
          ...manifest.devDependencies,
        }))
          expect(range, name).toMatch(/^\^\d+\.\d+\.\d+$/);
        expect(manifest.type).toBe("module");
        expect(Object.keys(manifest.scripts)).toEqual(
          expect.arrayContaining(["build", "dev", "start", "typecheck"]),
        );
      });

      it("has every file its scripts run", async () => {
        const files = await planned(template);
        const manifest = JSON.parse(textOf(files, "package.json")) as {
          scripts: Record<string, string>;
        };
        const paths = new Set(files.map((file) => file.path));
        for (const [script, command] of Object.entries(manifest.scripts))
          for (const [, file] of command.matchAll(/(?:node|-p) ([\w./-]+\.(?:mjs|json))/g))
            expect(paths, `${script}: ${file}`).toContain(file);
      });

      it("builds the view where the server and the dev script look for it", async () => {
        const files = await planned(template);
        const config = textOf(files, "vite.config.ts");
        // Relative, so the view's files are found under whatever origin the document is served from.
        expect(config).toContain('base: "./"');
        expect(config).toContain('"dist/view"');
        expect(config).toContain('"dist/single"');
        const tool = textOf(files, "src/server/tool.ts");
        // The server is compiled to dist/server, next to dist/view and dist/single.
        expect(tool).toContain('new URL("../view/", import.meta.url)');
        expect(tool).toContain('new URL("../single/index.html", import.meta.url)');
        expect(JSON.parse(textOf(files, "tsconfig.server.json")).compilerOptions.outDir).toBe(
          "dist/server",
        );
        expect(textOf(files, "scripts/dev.mjs")).toContain('"dist/view"');
        expect(textOf(files, "scripts/dev.mjs")).toContain('"dist/server/main.js"');
      });

      it("gives the view the capability that says it was written for the pane", async () => {
        const files = await planned(template);
        const view = files.find((file) => /^src\/view\/App\.(tsx|vue)$/.test(file.path));
        expect(view?.text).toContain('"pencere/pane"');
      });
    });
  }
});

describe("where a project may be written", () => {
  it("is a directory that is not there, an empty one, or one that holds only a .git", async () => {
    const root = await directory();
    await check(join(root, "not-yet"));
    await check(root);
    await mkdir(join(root, ".git"));
    await check(root);
  });

  it("is not a directory that holds anything else, which is left as it is", async () => {
    const root = await directory();
    await writeFile(join(root, "notes.txt"), "mine");
    await mkdir(join(root, "src"));
    await expect(check(root)).rejects.toThrow(
      /is not empty \(notes\.txt, src\); nothing was written/,
    );
    expect(await readFile(join(root, "notes.txt"), "utf8")).toBe("mine");
  });

  it("is not a file", async () => {
    const root = await directory();
    await writeFile(join(root, "file"), "");
    await expect(check(join(root, "file"))).rejects.toThrow(/is a file/);
  });

  it("is written file by file, with the directories between", async () => {
    const root = await directory();
    const target = join(root, "a/b");
    await write(target, [
      { path: "x.txt", text: "x" },
      { path: "deep/er/y.txt", text: "y" },
    ]);
    expect(await readFile(join(target, "deep/er/y.txt"), "utf8")).toBe("y");
    expect(await readFile(join(target, "x.txt"), "utf8")).toBe("x");
  });

  it("never replaces a file", async () => {
    const root = await directory();
    await writeFile(join(root, "x.txt"), "mine");
    await expect(write(root, [{ path: "x.txt", text: "theirs" }])).rejects.toThrow(
      /already exists; it was not replaced/,
    );
    expect(await readFile(join(root, "x.txt"), "utf8")).toBe("mine");
  });
});
