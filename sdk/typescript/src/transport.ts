/**
 * An MCP transport over frames (specification §9): the host is the MCP client,
 * the program's server speaks to it in `msg` messages, one JSON-RPC message in
 * each.
 */

import { deserializeMessage } from "@modelcontextprotocol/server";
import type { JSONRPCMessage, Transport } from "@modelcontextprotocol/server";

import type { Link } from "./link.js";
import { toWire } from "./json.js";

/**
 * The transport a server is connected to for one view. It is made for one `open`
 * and ends with it: a host that starts the view again gets a new one (§10.7).
 */
export class PencereTransport implements Transport {
  onclose?: (() => void) | undefined;
  onerror?: ((error: Error) => void) | undefined;
  onmessage?: ((message: JSONRPCMessage) => void) | undefined;

  readonly #link: Link;
  #closed = false;

  constructor(link: Link) {
    this.#link = link;
  }

  /** Nothing to start: the link is already reading, and what arrives is passed in with {@link deliver}. */
  async start(): Promise<void> {}

  async send(message: JSONRPCMessage): Promise<void> {
    if (this.#closed) throw new Error("the view has closed");
    await this.#link.send("msg", toWire(message)).written;
  }

  async close(): Promise<void> {
    if (this.#closed) return;
    this.#closed = true;
    this.onclose?.();
  }

  /**
   * A `msg` of the host. Its payload is one JSON-RPC message; anything else is
   * dropped (§4.5), and a drop is reported, not fatal.
   */
  deliver(payload: string): JSONRPCMessage | undefined {
    if (this.#closed) return undefined;
    let message: JSONRPCMessage;
    try {
      message = deserializeMessage(payload);
    } catch (error) {
      this.onerror?.(error instanceof Error ? error : new Error(String(error)));
      return undefined;
    }
    this.onmessage?.(message);
    return message;
  }
}
