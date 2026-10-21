/**
 * An MCP server that is a process: JSON-RPC messages, one to a line, on its stdin
 * and stdout (MCP's stdio transport), seen from the client's side.
 *
 * This is the one place a server is started. Discovery asks a server what it has
 * with it, and the adapter that stands in for the program's MCP server on the
 * terminal's stream (`connector.ts`) forwards to it, so both see the same
 * environment, the same process group and the same ending.
 *
 * Why this and not `StdioClientTransport` of the MCP client package: that one
 * passes a server six environment variables and no more, which would make a
 * server start differently under pencere than under the chat client whose
 * configuration the person copied; and it cannot start the server in a process
 * group of its own. The server must have one. ⌃C is a signal of the terminal to
 * the foreground group, the pencere program's own, and a server that took it too
 * would die before the program had said `bye`, and a Python server would print a
 * traceback for what the person meant as "close this". The server is ended by the
 * program that started it: its stdin closes, which every stdio server takes as
 * the end, then SIGTERM and SIGKILL to its whole group, which catches what a
 * wrapper (`npx`, `uv run`, `sh -c`) started. A server that leaves on its own when its
 * stdin closes, or falls, leaves what it started behind in the group, and that gets the
 * same two endings: the group is the program's to end, not only the process that leads it.
 */

import { ReadBuffer, serializeMessage } from "@modelcontextprotocol/client";
import type { JSONRPCMessage, Transport } from "@modelcontextprotocol/server";
import { spawn } from "node:child_process";
import type { ChildProcess } from "node:child_process";

import type { Tail } from "./tail.js";

/** What to run: a program and its arguments, as the person gave them. */
export interface ServerCommand {
  readonly command: string;
  readonly args: readonly string[];
}

/** How a server's process ended. */
export interface Exit {
  readonly code: number | null;
  readonly signal: NodeJS.Signals | null;
  /** The program was ending the process (`close`) when it ended: not a failure. */
  readonly expected: boolean;
}

/** How long each step of ending a server is given, in milliseconds. */
export interface CloseTiming {
  /** After its stdin closed, for the server to leave on its own. */
  readonly stdin: number;
  /** After SIGTERM. */
  readonly term: number;
  /** After SIGKILL, which cannot be refused: only for the process table to catch up. */
  readonly kill: number;
}

export const CLOSE_TIMING: CloseTiming = { stdin: 1500, term: 1500, kill: 2000 };

/** After a process ended, how long to wait for the last of its output before letting go of its pipes. */
const PIPE_GRACE = 250;

/** How often the process table is asked whether a group is gone, in milliseconds. */
const GROUP_POLL = 20;

/** What is held of a server's output while a line is not whole. */
export interface ProcessLimits {
  /**
   * The longest line, in bytes, that is read. A line past it is dropped, with an error, and the
   * lines after it are read as they come. 10 MiB, which is the MCP client library's.
   */
  readonly line: number;
}

const LIMITS: ProcessLimits = { line: 10 * 1024 * 1024 };

/** The process could not be started. */
export class StartError extends Error {
  readonly command: string;

  constructor(command: string, cause: Error) {
    super(`cannot start "${command}": ${reason(cause)}`, { cause });
    this.name = "StartError";
    this.command = command;
  }
}

function reason(error: Error): string {
  const code = (error as NodeJS.ErrnoException).code;
  if (code === "ENOENT") return "no such file or directory (is it installed, and on PATH?)";
  if (code === "EACCES") return "permission denied";
  return error.message;
}

function toError(value: unknown): Error {
  return value instanceof Error ? value : new Error(String(value));
}

/** Resolves `true` when `promise` settles within `ms`, `false` when the time is up first. */
async function within(promise: Promise<unknown>, ms: number): Promise<boolean> {
  let timer: NodeJS.Timeout | undefined;
  const late = new Promise<false>((resolve) => {
    timer = setTimeout(() => resolve(false), ms);
  });
  try {
    return await Promise.race([promise.then(() => true as const), late]);
  } finally {
    clearTimeout(timer);
  }
}

/** Whether anything is left of the process group `pid` leads. */
function groupExists(pid: number): boolean {
  try {
    process.kill(-pid, 0);
    return true;
  } catch {
    // Nothing there (ESRCH), or nothing that can be signalled: either way nothing to wait for.
    return false;
  }
}

/** Resolves `true` when the group is gone within `ms`, `false` when something is left. */
async function groupGone(pid: number, ms: number): Promise<boolean> {
  const deadline = Date.now() + ms;
  while (groupExists(pid)) {
    if (Date.now() >= deadline) return false;
    await new Promise((resolve) => setTimeout(resolve, GROUP_POLL));
  }
  return true;
}

/** The process of a stdio MCP server, as a transport for an MCP client, and a tool to end it. */
export class ServerProcess implements Transport {
  onclose?: (() => void) | undefined;
  onerror?: ((error: Error) => void) | undefined;
  onmessage?: ((message: JSONRPCMessage) => void) | undefined;
  /** The process has ended and what it wrote has been read: once, before `onclose`. */
  onexit?: ((exit: Exit) => void) | undefined;

  readonly #spec: ServerCommand;
  readonly #tail: Tail;
  readonly #timing: CloseTiming;
  readonly #read: ReadBuffer;
  #child: ChildProcess | undefined;
  #closing = false;
  #shutdown: Promise<void> | undefined;
  /** The process has exited: what it did is known, and what is left is its pipes. */
  #exited = false;
  #ended = false;
  /** The ending of what is left in the process's group, begun when the process ended. */
  #sweeping: Promise<void> = Promise.resolve();
  readonly #done: Promise<Exit>;
  #finished!: (exit: Exit) => void;

  constructor(
    spec: ServerCommand,
    tail: Tail,
    timing: CloseTiming = CLOSE_TIMING,
    limits: ProcessLimits = LIMITS,
  ) {
    this.#spec = spec;
    this.#tail = tail;
    this.#timing = timing;
    this.#read = new ReadBuffer({ maxBufferSize: limits.line });
    this.#done = new Promise<Exit>((resolve) => {
      this.#finished = resolve;
    });
  }

  /** Resolves once the process has ended (never, if it was never started). */
  get ended(): Promise<Exit> {
    return this.#done;
  }

  get pid(): number | undefined {
    return this.#child?.pid;
  }

  /** Starts the process; it is running when this resolves. @throws {StartError} */
  start(): Promise<void> {
    if (this.#child !== undefined)
      return Promise.reject(new Error("the server process was started already"));
    return new Promise<void>((resolve, reject) => {
      const child = spawn(this.#spec.command, [...this.#spec.args], {
        stdio: ["pipe", "pipe", "pipe"],
        // A process group of its own: see the description of this file.
        detached: true,
        shell: false,
      });
      this.#child = child;
      let spawned = false;
      // Not `once`: after the process runs, an error is a signal that was not sent or a write that
      // did not go through, and there can be more than one; an `error` nobody listens for ends the program.
      child.on("error", (error) => {
        // Before `spawn` this is a program that could not be started; after, a signal
        // or a write that did not go through.
        if (spawned) this.onerror?.(error);
        else {
          this.#ended = true;
          reject(new StartError(this.#spec.command, error));
        }
      });
      child.once("spawn", () => {
        spawned = true;
        resolve();
      });
      // A write to a process that has gone fails with EPIPE, and `send` tells the caller
      // so; the process's end is told by `onexit`, once, and is not an error as well.
      child.stdin.on("error", () => {});
      child.stdout.on("error", (error) => this.onerror?.(error));
      child.stdout.on("data", (chunk: Buffer) => this.#data(chunk));
      child.stderr.on("error", () => {});
      child.stderr.on("data", (chunk: Buffer) => this.#tail.push(chunk));

      let status: Omit<Exit, "expected"> | undefined;
      let expected = false;
      let timer: NodeJS.Timeout | undefined;
      /** What the process did, as it is known the moment it is: asked to end or not is decided then. */
      const exited = (code: number | null, signal: NodeJS.Signals | null): void => {
        if (status !== undefined) return;
        status = { code, signal };
        expected = this.#closing;
        this.#exited = true;
      };
      const end = (): void => {
        if (!spawned || this.#ended) return;
        clearTimeout(timer);
        this.#ended = true;
        // The last of its output is read or is not coming: a grandchild may hold the pipes.
        for (const stream of [child.stdin, child.stdout, child.stderr]) stream?.destroy();
        this.#read.clear();
        const exit: Exit = { ...(status ?? { code: null, signal: null }), expected };
        this.#finished(exit);
        this.onexit?.(exit);
        this.onclose?.();
        // Whatever the leader's end was, what it started is not ended with it: right now, with
        // the group's number still its, what is left in the group is the program's to end.
        this.#sweeping = this.#sweep();
      };
      child.once("exit", (code, signal) => {
        exited(code, signal);
        timer = setTimeout(end, PIPE_GRACE);
      });
      child.once("close", (code, signal) => {
        exited(code, signal);
        end();
      });
    });
  }

  /**
   * What the server wrote to stdout: whole lines are messages, and the rest waits for its end. The
   * chunk is read a line at a time, so that a line past the limit costs the lines around it
   * nothing: the buffer that gives up on it gives up on what it holds, which is that line.
   */
  #data(chunk: Buffer): void {
    let from = 0;
    while (from < chunk.length) {
      const newline = chunk.indexOf(0x0a, from);
      const to = newline === -1 ? chunk.length : newline + 1;
      this.#line(chunk.subarray(from, to));
      from = to;
    }
  }

  #line(piece: Buffer): void {
    try {
      this.#read.append(piece);
    } catch (error) {
      // A line past the limit is dropped, and the next one is read as it comes.
      this.onerror?.(toError(error));
      return;
    }
    for (;;) {
      let message: JSONRPCMessage | null;
      try {
        message = this.#read.readMessage();
      } catch (error) {
        // A line that is JSON and is not a message: it is gone, and the lines after it are not.
        this.onerror?.(toError(error));
        continue;
      }
      if (message === null) return;
      try {
        this.onmessage?.(message);
      } catch (error) {
        this.onerror?.(toError(error));
      }
    }
  }

  send(message: JSONRPCMessage): Promise<void> {
    const stdin = this.#child?.stdin;
    if (this.#ended || this.#closing || !stdin?.writable)
      return Promise.reject(new Error("the server process is not running"));
    return new Promise<void>((resolve, reject) => {
      stdin.write(serializeMessage(message), (error) =>
        error === null || error === undefined ? resolve() : reject(error),
      );
    });
  }

  /**
   * Ends the process, as asked: its stdin closes, and if it has not left by itself
   * its group gets SIGTERM and then SIGKILL. Resolves when it is gone. A process that
   * is already gone, or was never started, is a close that has nothing to do.
   */
  close(): Promise<void> {
    this.#shutdown ??= this.#shutDown();
    return this.#shutdown;
  }

  async #shutDown(): Promise<void> {
    const child = this.#child;
    if (child === undefined) return;
    if (!this.#ended) {
      // Exited by itself already: closing it is waiting for its pipes, which is not what it
      // was that ended it.
      if (this.#exited) await this.#done;
      else await this.#end(child);
    }
    await this.#sweeping;
  }

  /** Asks the process to end, as its stdin closing does, and then makes it. */
  async #end(child: ChildProcess): Promise<void> {
    this.#closing = true;
    try {
      child.stdin?.end();
    } catch {
      // Closed already: the process is on its way out.
    }
    if (await within(this.#done, this.#timing.stdin)) return;
    this.#signal("SIGTERM");
    if (await within(this.#done, this.#timing.term)) return;
    this.#signal("SIGKILL");
    await within(this.#done, this.#timing.kill);
  }

  /**
   * The process has ended: what is left in its group (a wrapper's child, `sh -c 'server &'`) is
   * ended as a leader that would not leave is, SIGTERM and then SIGKILL. Called as the process
   * ends, so that the group's number is still its and has not been given to another.
   */
  async #sweep(): Promise<void> {
    const pid = this.#child?.pid;
    if (pid === undefined || !groupExists(pid)) return;
    this.#signal("SIGTERM");
    if (await groupGone(pid, this.#timing.term)) return;
    this.#signal("SIGKILL");
    await groupGone(pid, this.#timing.kill);
  }

  /** To the process's whole group; to the process alone where a group cannot be signalled. */
  #signal(signal: NodeJS.Signals): void {
    const child = this.#child;
    const pid = child?.pid;
    if (child === undefined || pid === undefined) return;
    try {
      process.kill(-pid, signal);
    } catch {
      try {
        child.kill(signal);
      } catch {
        // Gone between the question and the signal.
      }
    }
  }
}
