/**
 * Which tool's view to show, when the person did not say.
 *
 * The server is started once, for as long as it takes to ask it: `initialize` and
 * `tools/list`, as any MCP client does. The tool that names a view is the one
 * whose `_meta.ui.resourceUri` does (MCP Apps, Resource Discovery; the flat
 * `_meta["ui/resourceUri"]` of the first drafts, which the extension's own
 * libraries still read, counts too). One such tool is the answer. None, or several,
 * is for the person to settle, and the answer says which tools there were. The list is read page
 * by page, as far as the terminal reads it ({@link MAX_TOOL_PAGES}).
 */

import { Client } from "@modelcontextprotocol/client";

import { ServerProcess, StartError } from "./server-process.js";
import type { CloseTiming, ServerCommand } from "./server-process.js";
import { plain } from "./tail.js";
import type { Tail } from "./tail.js";

/**
 * The most pages of tools that are read: what the terminal reads of the same server, so that a
 * tool the terminal would not find is not the tool this finds, and a server that never stops
 * listing does not keep this asking.
 */
export const MAX_TOOL_PAGES = 16;

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

/**
 * The `ui://` resource a tool names, in either of the two places MCP Apps has had it. An address
 * of another scheme is no view: the terminal reads `ui://` resources only, and a tool it would
 * refuse is not a tool to show.
 */
export function viewOf(tool: { readonly _meta?: unknown }): string | undefined {
  const meta = tool._meta;
  if (typeof meta !== "object" || meta === null) return undefined;
  const record = meta as Record<string, unknown>;
  const ui = record.ui;
  if (typeof ui === "object" && ui !== null) {
    const uri = (ui as Record<string, unknown>).resourceUri;
    if (typeof uri === "string" && uri.startsWith("ui://")) return uri;
  }
  const flat = record["ui/resourceUri"];
  return typeof flat === "string" && flat.startsWith("ui://") ? flat : undefined;
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
    const tools: { readonly name: string; readonly _meta?: unknown }[] = [];
    let cursor: string | undefined;
    // A server that says it has no tools is asked for none, as `listTools` would not ask it.
    const asking = client.getServerCapabilities()?.tools !== undefined;
    for (let pages = 1; asking; pages += 1) {
      // A page at a time, as the terminal reads them: `listTools` without a cursor reads
      // them all, to a limit of its own, which is not the terminal's.
      const page = await client.request(
        { method: "tools/list", params: cursor === undefined ? {} : { cursor } },
        { timeout },
      );
      tools.push(...page.tools);
      cursor = page.nextCursor;
      if (cursor === undefined) break;
      if (pages >= MAX_TOOL_PAGES)
        throw new DiscoveryError(`the server has more than ${MAX_TOOL_PAGES} pages of tools`);
    }
    const withView = tools.filter((tool) => viewOf(tool) !== undefined).map((tool) => tool.name);
    const [first, ...others] = withView;
    if (first === undefined) return { kind: "none", tools: tools.map((tool) => tool.name) };
    return others.length === 0 ? { kind: "found", tool: first } : { kind: "many", tools: withView };
  } catch (error) {
    if (signal?.aborted) throw new Interrupted();
    // A process that could not be started says so itself, wherever the client put it.
    if (error instanceof StartError) throw error;
    if (error instanceof Error && error.cause instanceof StartError) throw error.cause;
    if (error instanceof DiscoveryError) throw error;
    // What the server said is shown as text: an error message is its to write.
    throw new DiscoveryError(
      `"${spec.command}" did not answer as an MCP server that speaks stdio does: ${plain(
        error instanceof Error ? error.message : String(error),
      )}`,
      { cause: error },
    );
  } finally {
    signal?.removeEventListener("abort", abort);
    await client.close().catch(() => {});
    await server.close();
  }
}
