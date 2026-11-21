/**
 * The proof of the terminal's own route (specification §4.10): a listener on the loopback of the
 * program's machine that gives one token to one connection. The terminal asks with a `probe`; the
 * program opens the listener at a port the system chooses and says it in the `probed`; the
 * connection that comes finds the listener closed before it is written the token, so that a second
 * one is refused; and a listener nobody connects to is closed when `ttl` seconds have passed, and
 * when the view ends.
 */

import { createServer } from "node:net";
import type { Server, Socket } from "node:net";

import type { NotListening, ProbeAsked } from "./messages.js";

/** How long the connection that came may take to be written the token. */
const WRITE_WITHIN_MS = 5000;

/** A listener that was asked for, and is not closed. */
interface Up {
  readonly server: Server;
  timer: ReturnType<typeof setTimeout> | undefined;
  closed: boolean;
}

/** The listener of the program, if it has one up: at most one (§4.10). */
export class Probe {
  #up: Up | undefined;

  /**
   * Opens a listener on `127.0.0.1` that gives `asked.token` to one connection. Resolves with its port,
   * or why there is none. Neither the listener nor the timer keeps the program alive.
   */
  open(asked: ProbeAsked): Promise<number | NotListening> {
    if (this.#up !== undefined) return Promise.resolve("busy");
    return new Promise((resolve) => {
      const server = createServer();
      const up: Up = { server, timer: undefined, closed: false };
      this.#up = up;
      server.unref();
      server.on("error", () => {
        this.#close(up);
        resolve("io");
      });
      server.on("connection", (socket: Socket) => {
        // One connection: the listener is closed before the token is written, so that another
        // that comes finds none.
        if (up.closed) {
          socket.destroy();
          return;
        }
        this.#close(up);
        socket.unref();
        socket.setTimeout(WRITE_WITHIN_MS, () => socket.destroy());
        socket.on("error", () => socket.destroy());
        socket.end(asked.token, "latin1");
      });
      server.listen({ host: "127.0.0.1", port: 0 }, () => {
        const address = server.address();
        if (up.closed) {
          // Closed (the view ended) before it came up: it is not left listening.
          server.close();
          resolve("io");
        } else if (address === null || typeof address === "string") {
          this.#close(up);
          resolve("io");
        } else {
          up.timer = setTimeout(() => this.#close(up), asked.ttl * 1000);
          up.timer.unref();
          resolve(address.port);
        }
      });
    });
  }

  /** The view is over, or the program leaves: the listener, if it is up, is closed. */
  end(): void {
    if (this.#up !== undefined) this.#close(this.#up);
  }

  #close(up: Up): void {
    if (up.closed) return;
    up.closed = true;
    clearTimeout(up.timer);
    if (this.#up === up) this.#up = undefined;
    try {
      up.server.close();
    } catch {
      // A server that is not listening yet has nothing to close: it is closed when it comes up.
    }
  }
}
