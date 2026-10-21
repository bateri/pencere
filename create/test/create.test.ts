import { mkdir, mkdtemp, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";

import { Cancelled, EXIT, create } from "../src/create.js";
import type { Environment } from "../src/create.js";

const dirs: string[] = [];
afterEach(async () => {
  for (const dir of dirs.splice(0)) await rm(dir, { recursive: true, force: true });
});

async function directory(): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "create-pencere-"));
  dirs.push(dir);
  return dir;
}

/** The command in `cwd`, with its output kept and, when `answers` is given, a person who gives them. */
function harness(cwd: string, answers?: (string | Error)[]) {
  const out: string[] = [];
  const err: string[] = [];
  const asked: string[] = [];
  const env: Environment = {
    cwd,
    stdout: { write: (text) => out.push(text) },
    stderr: { write: (text) => err.push(text) },
    version: "1.2.3",
    templates: new URL("../templates/", import.meta.url),
    ask:
      answers === undefined
        ? undefined
        : async (question, fallback) => {
            asked.push(`${question} [${fallback}]`);
            const answer = answers.shift();
            if (answer instanceof Error) throw answer;
            return answer === undefined || answer === "" ? fallback : answer;
          },
  };
  return { env, out: () => out.join(""), err: () => err.join(""), asked };
}

const exists = (path: string) =>
  stat(path).then(
    () => true,
    () => false,
  );

describe("create-pencere", () => {
  it("writes the project into the directory it is given, and says what to do next", async () => {
    const cwd = await directory();
    const run = harness(cwd);
    expect(await create(["my-tool", "--template", "vue"], run.env)).toBe(EXIT.ok);
    const manifest = JSON.parse(await readFile(join(cwd, "my-tool/package.json"), "utf8"));
    expect(manifest.name).toBe("my-tool");
    expect(manifest.devDependencies.vue).toBeDefined();
    expect(await exists(join(cwd, "my-tool/.gitignore"))).toBe(true);
    expect(await exists(join(cwd, "my-tool/_gitignore"))).toBe(false);
    expect(run.out()).toContain("Created my-tool (Vue)");
    expect(run.out()).toMatch(/cd my-tool\n {2}npm install\n {2}npm run dev/);
    expect(run.err()).toBe("");
  });

  it("makes a React project when no template is named and nobody can be asked", async () => {
    const cwd = await directory();
    expect(await create(["app"], harness(cwd).env)).toBe(EXIT.ok);
    const manifest = JSON.parse(await readFile(join(cwd, "app/package.json"), "utf8"));
    expect(manifest.devDependencies["@vitejs/plugin-react"]).toBeDefined();
  });

  it("needs the directory when nobody can be asked, and writes nothing without it", async () => {
    const cwd = await directory();
    const run = harness(cwd);
    expect(await create([], run.env)).toBe(EXIT.usage);
    expect(run.err()).toContain("name the directory");
    expect(await readdir(cwd)).toEqual([]);
  });

  it("asks for what the command line left out, and again for a template that is not one", async () => {
    const cwd = await directory();
    const run = harness(cwd, ["asked-for", "angular", "VUE"]);
    expect(await create([], run.env)).toBe(EXIT.ok);
    expect(run.asked).toEqual([
      "Project directory? [pencere-app]",
      "Template, react or vue? [react]",
      "Template, react or vue? [react]",
    ]);
    expect(await exists(join(cwd, "asked-for/src/view/App.vue"))).toBe(true);
    // Asked again with nothing said would read as a question that was not heard.
    expect(run.err()).toBe('"angular" is not a template: react or vue.\n');
  });

  it("takes the default of a question the person answers with nothing", async () => {
    const cwd = await directory();
    expect(await create([], harness(cwd, ["", ""]).env)).toBe(EXIT.ok);
    expect(await exists(join(cwd, "pencere-app/src/view/App.tsx"))).toBe(true);
  });

  it("asks only for what it was not told", async () => {
    const cwd = await directory();
    const run = harness(cwd, ["vue"]);
    expect(await create(["named"], run.env)).toBe(EXIT.ok);
    expect(run.asked).toEqual(["Template, react or vue? [react]"]);
  });

  it("ends quietly when the person closes a question", async () => {
    const cwd = await directory();
    const run = harness(cwd, [new Cancelled()]);
    expect(await create([], run.env)).toBe(EXIT.cancelled);
    expect(await readdir(cwd)).toEqual([]);
  });

  it("writes nothing into a directory that holds something", async () => {
    const cwd = await directory();
    await mkdir(join(cwd, "mine"));
    await writeFile(join(cwd, "mine/notes.txt"), "keep");
    const run = harness(cwd);
    expect(await create(["mine"], run.env)).toBe(EXIT.failed);
    expect(run.err()).toContain("is not empty");
    expect(await readdir(join(cwd, "mine"))).toEqual(["notes.txt"]);
    expect(run.out()).toBe("");
  });

  it("writes into an empty directory, named by `.`, and does not say to change into it", async () => {
    const cwd = await directory();
    const here = join(cwd, "Here Now");
    await mkdir(here);
    const run = harness(here);
    expect(await create([".", "-t", "react"], run.env)).toBe(EXIT.ok);
    expect(JSON.parse(await readFile(join(here, "package.json"), "utf8")).name).toBe("here-now");
    expect(run.out()).toContain('The package is called "here-now".');
    expect(run.out()).not.toContain("cd ");
  });

  it("calls the package what a directory's odd name allows", async () => {
    const cwd = await directory();
    const run = harness(cwd);
    expect(await create(["My Tool!"], run.env)).toBe(EXIT.ok);
    const manifest = JSON.parse(await readFile(join(cwd, "My Tool!/package.json"), "utf8"));
    expect(manifest.name).toBe("my-tool");
    expect(run.out()).toContain('cd "My Tool!"');
  });

  it("refuses a directory whose name is no name at all", async () => {
    const cwd = await directory();
    const run = harness(cwd);
    expect(await create(["___"], run.env)).toBe(EXIT.usage);
    expect(run.err()).toContain("nothing a package can be called");
    expect(await readdir(cwd)).toEqual([]);
  });

  it("says what is wrong with the command line, and writes nothing", async () => {
    const cwd = await directory();
    const run = harness(cwd);
    expect(await create(["a", "--template", "svelte"], run.env)).toBe(EXIT.usage);
    expect(run.err()).toContain('there is no template "svelte"');
    expect(run.err()).toContain("--help");
    expect(await readdir(cwd)).toEqual([]);
  });

  it("answers --help and --version", async () => {
    const run = harness("/");
    expect(await create(["--help"], run.env)).toBe(EXIT.ok);
    expect(run.out()).toContain("usage: create-pencere");
    const version = harness("/");
    expect(await create(["--version"], version.env)).toBe(EXIT.ok);
    expect(version.out()).toBe("create-pencere 1.2.3\n");
  });
});
