/**
 * Which tool's view to show, when the person did not say.
 *
 * The server is started once, for as long as it takes to ask it: `initialize` and
 * `tools/list`, as any MCP client does. The tool that names a view is the one
 * whose `_meta.ui.resourceUri` does (MCP Apps, Resource Discovery; the flat
 * `_meta["ui/resourceUri"]` of the first drafts, which the extension's own
 * libraries still read, counts too). One such tool is the answer. None, or several,
 * is for the person to settle, and the answer says which tools there were.
 */

import { Client } from "@modelcontextprotocol/client";

import { ServerProcess, StartError } from "./server-process.js";
import type { CloseTiming, ServerCommand } from "./server-process.js";
import type { Tail } from "./tail.js";

/** What the server has with it. */
export type Discovery =
  | { readonly kind: "found"; readonly tool: string }
  /** No tool names a view. `tools` is every tool the server has. */
  | { readonly kind: "none"; readonly tools: readonly string[] }
  /** Several do: their names. */
  | { readonly kind: "many"; readonly tools: readonly string[] };

export interface DiscoverOptions {
  /** The version `pencere` calls itself, in the handshake. */
  readonly version: string;
  /** How long the server has to answer each question, in milliseconds. */
  readonly timeout: number;
  /** The person interrupted: the server is let go and the question ends. */
  readonly signal?: AbortSignal;
  readonly timing?: CloseTiming;
}

/** The server was started and did not answer as an MCP server does. */
export class DiscoveryError extends Error {
  constructor(message: string, options?: ErrorOptions) {
    super(message, options);
    this.name = "DiscoveryError";
  }
}

/** The person interrupted while the server was being asked. */
export class Interrupted extends Error {
  constructor() {
    super("interrupted");
    this.name = "Interrupted";
  }
}

/** The `ui://` resource a tool names, in either of the two places MCP Apps has had it. */
export function viewOf(tool: { readonly _meta?: unknown }): string | undefined {
  const meta = tool._meta;
  if (typeof meta !== "object" || meta === null) return undefined;
  const record = meta as Record<string, unknown>;
  const ui = record.ui;
  if (typeof ui === "object" && ui !== null) {
    const uri = (ui as Record<string, unknown>).resourceUri;
    if (typeof uri === "string" && uri !== "") return uri;
  }
  const flat = record["ui/resourceUri"];
  return typeof flat === "string" && flat !== "" ? flat : undefined;
}

/**
 * @throws {StartError} when the command cannot be started
 * @throws {DiscoveryError} when it started and did not answer as an MCP server
 * @throws {Interrupted} when `signal` aborted
 */
export async function discoverTool(
  spec: ServerCommand,
  tail: Tail,
  options: DiscoverOptions,
): Promise<Discovery> {
  const { signal, timeout } = options;
  if (signal?.aborted) throw new Interrupted();
  const server = new ServerProcess(spec, tail, options.timing);
  const client = new Client({ name: "pencere", version: options.version });
  // Closing the process fails whatever the client is waiting for.
  const abort = (): void => void server.close();
  signal?.addEventListener("abort", abort, { once: true });
  try {
    await client.connect(server, { timeout });
    const { tools } = await client.listTools(undefined, { timeout });
    const withView = tools.filter((tool) => viewOf(tool) !== undefined).map((tool) => tool.name);
    const [first, ...others] = withView;
    if (first === undefined) return { kind: "none", tools: tools.map((tool) => tool.name) };
    return others.length === 0 ? { kind: "found", tool: first } : { kind: "many", tools: withView };
  } catch (error) {
    if (signal?.aborted) throw new Interrupted();
    // A process that could not be started says so itself, wherever the client put it.
    if (error instanceof StartError) throw error;
    if (error instanceof Error && error.cause instanceof StartError) throw error.cause;
    throw new DiscoveryError(
      `"${spec.command}" did not answer as an MCP server that speaks stdio does: ${
        error instanceof Error ? error.message : String(error)
      }`,
      { cause: error },
    );
  } finally {
    signal?.removeEventListener("abort", abort);
    await client.close().catch(() => {});
    await server.close();
  }
}
