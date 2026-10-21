/**
 * The program's MCP server, stood in for by a process.
 *
 * `serve` of the SDK takes anything that can `connect` a transport and `close`
 * the session, and does everything else of a pencere program: the question to the
 * terminal, the token, the bundle, ⌃C and `bye`, the host starting the view again
 * (§10.7) and the program reopening it (§10.8). It connects once for each view it
 * opens. This is that server for a program that has none of its own: it starts the
 * command for each connection and carries the JSON-RPC messages between the
 * terminal's stream and the command's stdio, line by line, untouched. A server
 * that has been connected to a view once has done its `initialize`; the next view
 * is a new session, and so a new process, which is also what a stdio server
 * expects: one `initialize` for the life of the process.
 */

import type { Transport } from "@modelcontextprotocol/server";
import type { Connectable } from "@pencere/sdk";

import { CLOSE_TIMING, ServerProcess } from "./server-process.js";
import type { CloseTiming, Exit, ServerCommand } from "./server-process.js";
import type { Tail } from "./tail.js";

export interface ConnectorOptions {
  /** What the server writes to stderr goes here, as do the connector's own notes on the stream. */
  readonly tail: Tail;
  /**
   * The server's process ended, and the connector had not asked it to. The view
   * has nothing to talk to any more: the caller ends it.
   */
  readonly unexpectedExit: (exit: Exit) => void;
  readonly timing?: CloseTiming;
}

export class ProcessConnector implements Connectable {
  readonly #spec: ServerCommand;
  readonly #options: ConnectorOptions;
  #current: { readonly server: ServerProcess; readonly transport: Transport } | undefined;
  readonly #ending = new Set<Promise<void>>();
  #starts = 0;

  constructor(spec: ServerCommand, options: ConnectorOptions) {
    this.#spec = spec;
    this.#options = options;
  }

  /** How many processes this connector has started: one for each view that was opened. */
  get starts(): number {
    return this.#starts;
  }

  /** @throws {StartError} when the command cannot be started. */
  async connect(transport: Transport): Promise<void> {
    // One process for one session: the last is gone before the next begins.
    await this.close();
    const { tail, unexpectedExit, timing } = this.#options;
    const server = new ServerProcess(this.#spec, tail, timing ?? CLOSE_TIMING);
    server.onmessage = (message) => {
      // A view that is gone has no use for the answer, and says so by failing here.
      transport.send(message).catch(() => {});
    };
    server.onerror = (error) => tail.push(`pencere: ${error.message}\n`);
    server.onexit = (exit) => {
      if (!exit.expected) unexpectedExit(exit);
    };
    transport.onmessage = (message) => {
      // A server that is gone cannot be written to, and says so by ending (`unexpectedExit`).
      server.send(message).catch(() => {});
    };
    transport.onclose = () => {
      void server.close();
    };
    try {
      await transport.start();
      await server.start();
      this.#starts += 1;
    } catch (error) {
      transport.onmessage = undefined;
      transport.onclose = undefined;
      throw error;
    }
    this.#current = { server, transport };
  }

  /** Ends the process of the session that is connected, if there is one, and resolves when it is gone. */
  async close(): Promise<void> {
    const current = this.#current;
    this.#current = undefined;
    if (current === undefined) return;
    const ending = current.server.close().then(() => current.transport.close());
    this.#ending.add(ending);
    try {
      await ending;
    } finally {
      this.#ending.delete(ending);
    }
  }

  /**
   * Resolves when every process this connector started has ended. `serve` lets go of
   * a session without waiting for it (a signal must not wait for a stubborn server),
   * and a program that is about to leave has to.
   */
  async drain(): Promise<void> {
    await this.close();
    while (this.#ending.size > 0) await Promise.all([...this.#ending]);
  }
}
