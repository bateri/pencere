/**
 * The command: what is asked, what is checked, what is written, what is told.
 *
 * Everything the process owns (the directory it runs in, the streams, the
 * person at the keyboard, where the templates are) comes in through an
 * `Environment`, so the tests run the command on a temporary directory with
 * answers given and no terminal.
 */

import { readFileSync } from "node:fs";
import { createInterface } from "node:readline";
import { basename, relative, resolve } from "node:path";

import { TEMPLATES, USAGE, UsageError, isTemplate, parse } from "./args.js";
import type { Template } from "./args.js";
import { FRAMEWORKS, ScaffoldError, check, packageName, plan, write } from "./scaffold.js";

/** How the command ended; the process exits with it. */
export const EXIT = { ok: 0, failed: 1, usage: 2, cancelled: 130 } as const;

/** The person closed the question (⌃C or ⌃D) before answering it. */
export class Cancelled extends Error {}

/** A question to the person: what to ask, and the answer that stands for an empty one. */
export type Ask = (question: string, fallback: string) => Promise<string>;

/** What the command needs of the process. */
export interface Environment {
  readonly cwd: string;
  readonly stdout: { write(text: string): unknown };
  readonly stderr: { write(text: string): unknown };
  readonly version: string;
  /** The directory with `common/` and the templates. */
  readonly templates: URL | string;
  /**
   * Asks the person a question and says the answer, or `fallback` for an empty
   * one. Not given when nobody can be asked: stdin or stdout is not a terminal.
   */
  readonly ask?: Ask | undefined;
}

/** The environment of this process. */
export function systemEnvironment(): Environment {
  const manifest = JSON.parse(
    readFileSync(new URL("../package.json", import.meta.url), "utf8"),
  ) as { version: string };
  return {
    cwd: process.cwd(),
    stdout: process.stdout,
    stderr: process.stderr,
    version: manifest.version,
    templates: new URL("../templates/", import.meta.url),
    ...(process.stdin.isTTY && process.stdout.isTTY ? { ask } : {}),
  };
}

/** One question to the person at the terminal. */
function ask(question: string, fallback: string): Promise<string> {
  const lines = createInterface({ input: process.stdin, output: process.stdout });
  return new Promise((resolve, reject) => {
    lines.once("close", () => reject(new Cancelled()));
    lines.question(`${question} (${fallback}) `, (answer) => {
      lines.removeAllListeners("close");
      lines.close();
      resolve(answer.trim() === "" ? fallback : answer.trim());
    });
  });
}

/** Asks which template until the person names one. */
async function askTemplate(ask: Ask): Promise<Template> {
  const [first] = TEMPLATES;
  for (;;) {
    const answer = (await ask(`Template, ${TEMPLATES.join(" or ")}?`, first)).toLowerCase();
    if (isTemplate(answer)) return answer;
  }
}

/** Runs the command line `argv` (without `node` and the script) and says how it ended. */
export async function create(argv: readonly string[], env: Environment): Promise<number> {
  try {
    return await createProject(argv, env);
  } catch (error) {
    if (error instanceof Cancelled) {
      env.stderr.write("\n");
      return EXIT.cancelled;
    }
    if (error instanceof UsageError) {
      env.stderr.write(`create-pencere: ${error.message}\nTry "create-pencere --help".\n`);
      return EXIT.usage;
    }
    if (error instanceof ScaffoldError) {
      env.stderr.write(`create-pencere: ${error.message}\n`);
      return EXIT.failed;
    }
    throw error;
  }
}

async function createProject(argv: readonly string[], env: Environment): Promise<number> {
  const command = parse(argv);
  if (command.kind === "help") {
    env.stdout.write(USAGE);
    return EXIT.ok;
  }
  if (command.kind === "version") {
    env.stdout.write(`create-pencere ${env.version}\n`);
    return EXIT.ok;
  }

  const { ask } = env;
  let directory = command.directory;
  if (directory === undefined) {
    if (ask === undefined)
      throw new UsageError("name the directory: npm create pencere@latest my-tool");
    directory = await ask("Project directory?", "pencere-app");
  }
  const template = command.template ?? (ask === undefined ? TEMPLATES[0] : await askTemplate(ask));

  const target = resolve(env.cwd, directory);
  const name = packageName(basename(target));
  if (name === undefined)
    throw new UsageError(`"${basename(target)}" holds nothing a package can be called`);
  await check(target);
  await write(target, await plan(env.templates, { name, template }));

  const there = relative(env.cwd, target);
  const lines = [
    `Created ${name} (${FRAMEWORKS[template]}) in ${target}.`,
    ...(name === basename(target) ? [] : [`The package is called "${name}".`]),
    "",
    "Next:",
    ...(there === "" ? [] : [`  cd ${there.includes(" ") ? JSON.stringify(there) : there}`]),
    "  npm install",
    "  npm run dev",
    "",
    "Run in a terminal that shows pencere views, `npm run dev` opens the view in the pane",
    "and shows it again each time you save. Anywhere else the same server is an ordinary",
    "MCP server: `npm run build`, then give an MCP Apps host `node dist/server/main.js`.",
    "",
  ];
  env.stdout.write(lines.join("\n"));
  return EXIT.ok;
}
