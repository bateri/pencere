/**
 * The command line: `pencere run [options] -- <command> [args...]`.
 *
 * Everything after the first `--` is the server's command, taken as it is: a
 * server's own flags are not pencere's to read. What comes before it is read with
 * `node:util`'s `parseArgs`, strictly, so a mistyped option is an error and not a
 * flag handed to nobody.
 */

import { parseArgs } from "node:util";

export const USAGE = `\
usage: pencere run [options] -- <command> [args...]

Runs <command>, an MCP server that speaks stdio and is written in any language,
and shows its view in this terminal pane. The server is started as an MCP client
would start it, so it is the same server an MCP Apps host runs.

  pencere run -- python server.py
  pencere run --tool list_containers --view ./view -- node dist/server.js

options:
  --tool <name>   the tool whose view to show; without it, the one tool of the
                  server that names a view (_meta.ui.resourceUri)
  --args <json>   the arguments the tool is called with, a JSON object
  --view <dir>    show the view from the files in <dir> (sent to the terminal
                  once, then kept by content) instead of the server's own
                  ui:// resource
  -h, --help      this text
  -v, --version   the version of pencere

exit status: 0 the view ran and closed; 1 it could not run (the server failed to
start or ended, the terminal refused or ended the view); 2 a mistake in the
command line; 3 this terminal does not show pencere views; 130 and 143 after ^C
and SIGTERM.
`;

/** What the command line asks for. */
export type Command =
  | { readonly kind: "help" }
  | { readonly kind: "version" }
  | {
      readonly kind: "run";
      readonly command: string;
      readonly args: readonly string[];
      readonly tool?: string;
      readonly arguments?: Record<string, unknown>;
      readonly view?: string;
    };

/** A mistake in the command line: what it is, so that the person can fix it. */
export class UsageError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "UsageError";
  }
}

/** The arguments of a tool call: one JSON object. */
function toolArguments(text: string): Record<string, unknown> {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch (error) {
    throw new UsageError(`--args is not JSON: ${error instanceof Error ? error.message : error}`);
  }
  if (typeof value !== "object" || value === null || Array.isArray(value))
    throw new UsageError("--args must be a JSON object");
  return value as Record<string, unknown>;
}

/** @throws {UsageError} for anything that is not a command of pencere. */
export function parse(argv: readonly string[]): Command {
  const [name, ...rest] = argv;
  if (name === undefined)
    throw new UsageError("no command: use `pencere run [options] -- <command> [args...]`");
  if (name === "-h" || name === "--help" || name === "help") return { kind: "help" };
  if (name === "-v" || name === "--version") return { kind: "version" };
  if (name !== "run")
    throw new UsageError(
      `unknown command "${name}": use \`pencere run [options] -- <command> [args...]\``,
    );

  // The server's command is everything after the first `--`.
  const dash = rest.indexOf("--");
  const own = dash === -1 ? rest : rest.slice(0, dash);
  let parsed: ReturnType<typeof parseOwn>;
  try {
    parsed = parseOwn(own);
  } catch (error) {
    throw new UsageError(error instanceof Error ? error.message : String(error));
  }
  const { values, positionals } = parsed;
  if (values.help === true) return { kind: "help" };
  const [stray] = positionals;
  if (stray !== undefined)
    throw new UsageError(`unexpected "${stray}": the server's command goes after --`);
  if (dash === -1) throw new UsageError("no command: put the server's command after --");
  const [command, ...args] = rest.slice(dash + 1);
  if (command === undefined || command === "") throw new UsageError("no command after --");
  for (const option of ["tool", "view"] as const)
    if (values[option] === "") throw new UsageError(`--${option} needs a value`);
  return {
    kind: "run",
    command,
    args,
    ...(values.tool === undefined ? {} : { tool: values.tool }),
    ...(values.args === undefined ? {} : { arguments: toolArguments(values.args) }),
    ...(values.view === undefined ? {} : { view: values.view }),
  };
}

function parseOwn(args: readonly string[]) {
  return parseArgs({
    args: [...args],
    options: {
      tool: { type: "string" },
      args: { type: "string" },
      view: { type: "string" },
      help: { type: "boolean", short: "h" },
    },
    allowPositionals: true,
    strict: true,
  });
}
