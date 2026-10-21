/**
 * What is written: a template turned into the files of a project, and the
 * rules for writing them.
 *
 * A project is the files of `templates/common` with those of `templates/<name>`
 * laid over them, so what both frameworks share (the server, the build of the
 * server, the stylesheet, the dev script) is written once. The files are text
 * with three placeholders, `{{name}}`, `{{title}}` and `{{framework}}`, in a
 * form that keeps every template file valid JSON, TypeScript or HTML as it
 * stands: they are formatted and read like any other source in this repository,
 * and only the generated project has them filled in.
 */

import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import type { Template } from "./args.js";

/** What the project is called, and the template it comes from. */
export interface Project {
  readonly name: string;
  readonly template: Template;
}

/** A file of the project, by its path relative to the project's directory. */
export interface PlannedFile {
  readonly path: string;
  readonly text: string;
}

/** The templates' names for the frameworks, as the project's README says them. */
export const FRAMEWORKS: Record<Template, string> = { react: "React", vue: "Vue" };

/**
 * A file the template keeps under another name: npm leaves `.gitignore` out of
 * every package it publishes, so the template ships `_gitignore`.
 */
const RENAMED: Readonly<Record<string, string>> = { _gitignore: ".gitignore" };

/** Why a project could not be written: what is in the way, so that the person can fix it. */
export class ScaffoldError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ScaffoldError";
  }
}

/**
 * The package name a directory's name stands for, or `undefined` when none
 * does. npm names are lowercase and take no leading dot or underscore, and the
 * name is also the host of the view's `ui://` address, so it keeps to
 * `a-z 0-9 . _ -`: any other run of characters becomes one hyphen.
 */
export function packageName(directoryName: string): string | undefined {
  const name = directoryName
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^[._-]+/, "")
    .replace(/[._-]+$/, "");
  return name === "" ? undefined : name;
}

/** The name as words, for what a person reads: `my-tool` is "My Tool". */
export function titleOf(name: string): string {
  return name
    .split(/[._-]+/)
    .filter((word) => word !== "")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

/** The paths of the files under `directory`, relative and `/`-separated, in byte order. */
async function walk(directory: string, relative = ""): Promise<string[]> {
  const found: string[] = [];
  for (const item of await readdir(join(directory, relative), { withFileTypes: true })) {
    const path = relative === "" ? item.name : `${relative}/${item.name}`;
    // A link or anything else that is neither is not part of a template.
    if (item.isDirectory()) found.push(...(await walk(directory, path)));
    else if (item.isFile()) found.push(path);
  }
  return found.sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));
}

function fill(text: string, values: Readonly<Record<string, string>>): string {
  return text.replace(/\{\{(name|title|framework)\}\}/g, (_, key: string) => values[key] ?? "");
}

function renamed(path: string): string {
  const slash = path.lastIndexOf("/");
  const [directory, file] =
    slash === -1 ? ["", path] : [path.slice(0, slash + 1), path.slice(slash + 1)];
  return `${directory}${RENAMED[file] ?? file}`;
}

/**
 * The files of a project made from the template in `templates`, which holds
 * `common/` and a directory for each template.
 *
 * @throws {ScaffoldError} when `templates` has no such template.
 */
export async function plan(templates: URL | string, project: Project): Promise<PlannedFile[]> {
  const root = templates instanceof URL ? fileURLToPath(templates) : templates;
  const values = {
    name: project.name,
    title: titleOf(project.name),
    framework: FRAMEWORKS[project.template],
  };
  const files = new Map<string, string>();
  for (const layer of ["common", project.template]) {
    const directory = join(root, layer);
    let paths: string[];
    try {
      paths = await walk(directory);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT")
        throw new ScaffoldError(`there is no template "${layer}"`);
      throw error;
    }
    for (const path of paths)
      files.set(renamed(path), fill(await readFile(join(directory, path), "utf8"), values));
  }
  return [...files].map(([path, text]) => ({ path, text }));
}

/**
 * Whether `directory` can be written into: it does not exist, or it is empty
 * (a `.git` the person has just made does not count). Anything else is the
 * person's work, and nothing is written over it.
 *
 * @throws {ScaffoldError} when it cannot, saying why.
 */
export async function check(directory: string): Promise<void> {
  let entries: string[];
  try {
    entries = await readdir(directory);
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code === "ENOENT") return;
    if (code === "ENOTDIR") throw new ScaffoldError(`${directory} is a file, not a directory`);
    throw error;
  }
  const held = entries.filter((entry) => entry !== ".git").sort();
  if (held.length === 0) return;
  const shown = held.slice(0, 3).join(", ") + (held.length > 3 ? ", …" : "");
  throw new ScaffoldError(
    `${directory} is not empty (${shown}); nothing was written. Name a directory that does not exist yet, or an empty one.`,
  );
}

/**
 * Writes `files` under `directory`, making it and what is between. A file that
 * exists already is never replaced: `check` is made before, and this is the
 * second refusal, for a file that came in between.
 *
 * @throws {ScaffoldError} when a file is already there.
 */
export async function write(directory: string, files: readonly PlannedFile[]): Promise<void> {
  for (const file of files) {
    const path = join(directory, file.path);
    await mkdir(dirname(path), { recursive: true });
    try {
      await writeFile(path, file.text, { flag: "wx" });
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "EEXIST")
        throw new ScaffoldError(`${path} already exists; it was not replaced`);
      throw error;
    }
  }
}
