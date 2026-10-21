/**
 * The command line: `create-pencere [directory] [--template react|vue]`.
 *
 * It is read with `node:util`'s `parseArgs`, strictly, so a mistyped option is an
 * error and not a flag handed to nobody. What is left out is asked for when a
 * person can be asked (create.ts), and a script that cannot be asked has to say it.
 */

import { parseArgs } from "node:util";

/** The templates, in the order they are offered. The first is the default. */
export const TEMPLATES = ["react", "vue"] as const;
export type Template = (typeof TEMPLATES)[number];

export const USAGE = `\
usage: create-pencere [directory] [options]

Starts a pencere tool in <directory>: an MCP server whose view opens in the
terminal pane it runs in, with a view built by Vite. It is the same server,
and the same view, in any MCP Apps host.

  npm create pencere@latest my-tool
  npm create pencere@latest my-tool -- --template vue

With no directory or no template it asks, when it can; otherwise the directory
is needed and the template is react.

options:
  -t, --template <name>  the view's framework: ${TEMPLATES.join(" or ")}
  -h, --help             this text
  -v, --version          the version of create-pencere

exit status: 0 the project was written; 1 it could not be (the directory is not
empty, or cannot be written); 2 a mistake in the command line.
`;

/** What the command line asks for. */
export type Command =
  | { readonly kind: "help" }
  | { readonly kind: "version" }
  | { readonly kind: "create"; readonly directory?: string; readonly template?: Template };

/** A mistake in the command line: what it is, so that the person can fix it. */
export class UsageError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "UsageError";
  }
}

export function isTemplate(name: string): name is Template {
  return (TEMPLATES as readonly string[]).includes(name);
}

/** @throws {UsageError} for anything that is not an option or a directory of `create-pencere`. */
export function parse(argv: readonly string[]): Command {
  let parsed: ReturnType<typeof parseOwn>;
  try {
    parsed = parseOwn(argv);
  } catch (error) {
    throw new UsageError(error instanceof Error ? error.message : String(error));
  }
  const { values, positionals } = parsed;
  if (values.help === true) return { kind: "help" };
  if (values.version === true) return { kind: "version" };
  if (positionals.length > 1)
    throw new UsageError(`one directory, not ${positionals.length}: ${positionals.join(" ")}`);
  const [directory] = positionals;
  if (directory === "") throw new UsageError("the directory is empty");
  let template: Template | undefined;
  if (values.template !== undefined) {
    if (!isTemplate(values.template))
      throw new UsageError(
        `there is no template "${values.template}": the templates are ${TEMPLATES.join(" and ")}`,
      );
    template = values.template;
  }
  return {
    kind: "create",
    ...(directory === undefined ? {} : { directory }),
    ...(template === undefined ? {} : { template }),
  };
}

function parseOwn(argv: readonly string[]) {
  return parseArgs({
    args: [...argv],
    allowPositionals: true,
    options: {
      template: { type: "string", short: "t" },
      help: { type: "boolean", short: "h" },
      version: { type: "boolean", short: "v" },
    },
  });
}
