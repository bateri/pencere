/** What the tests share: stand-in servers, a transport to connect them to, and a few waits. */

import type { JSONRPCMessage, Transport } from "@modelcontextprotocol/server";
import { fileURLToPath } from "node:url";

import type { ServerCommand } from "../src/server-process.js";

/** A server that is a Node program given as text. */
export function node(code: string, ...args: string[]): ServerCommand {
  return { command: process.execPath, args: ["-e", code, ...args] };
}

/** An MCP server on stdio with the tools of `mode` (see `servers/tools.mjs`). */
export function tools(mode: "one" | "many" | "legacy" | "none" | "empty"): ServerCommand {
  const script = fileURLToPath(new URL("servers/tools.mjs", import.meta.url));
  return { command: process.execPath, args: [script, mode] };
}

/** A server that answers each request with the pid it runs as, and says what it was asked. */
export const ECHO = node(`
  const readline = require("node:readline");
  readline.createInterface({ input: process.stdin }).on("line", (line) => {
    const message = JSON.parse(line);
    if (message.id === undefined) return;
    process.stdout.write(JSON.stringify({
      jsonrpc: "2.0", id: message.id, result: { method: message.method, pid: process.pid },
    }) + "\\n");
  });
`);

export function request(id: number, method: string): JSONRPCMessage {
  return { jsonrpc: "2.0", id, method } as JSONRPCMessage;
}

/** The terminal's stream as the program's MCP server sees it: records what it is sent. */
export class FakeTransport implements Transport {
  onclose?: (() => void) | undefined;
  onerror?: ((error: Error) => void) | undefined;
  onmessage?: ((message: JSONRPCMessage) => void) | undefined;
  readonly sent: JSONRPCMessage[] = [];
  closed = false;

  async start(): Promise<void> {}

  async send(message: JSONRPCMessage): Promise<void> {
    if (this.closed) throw new Error("the view has closed");
    this.sent.push(message);
  }

  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    this.onclose?.();
  }
}

/** Polls until `condition` holds. */
export async function until(condition: () => boolean, what: string, ms = 5000): Promise<void> {
  const deadline = Date.now() + ms;
  while (!condition()) {
    if (Date.now() > deadline) throw new Error(`gave up waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

/** Whether a process with this pid exists. */
export function alive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}
