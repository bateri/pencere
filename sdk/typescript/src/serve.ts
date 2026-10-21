/**
 * `serve`: what an MCP server does to show its view in the pane it runs in.
 *
 * Started by something that is not a terminal, the server is an ordinary MCP
 * server on stdio. Started in a terminal, it asks the terminal whether it hosts
 * views (specification §5), opens its view with the token the answer carries
 * (§6), sends the files of its bundle that the host lacks (§8), becomes the
 * MCP server of the view over the stream (§9) and, when the view closes, gives
 * the terminal back (§10). With no host it returns, and the tool's own text
 * interface takes over (§13). A program that wants to close its view and go on,
 * or show another, holds a `ViewControl` (§10.8).
 */

import type { Transport } from "@modelcontextprotocol/server";
import { StdioServerTransport } from "@modelcontextprotocol/server/stdio";
import type { Readable, Writable } from "node:stream";

import type { Bundle } from "./bundle.js";
import { bindControl, unbindControl } from "./control.js";
import type {
  CloseResult,
  ReopenOptions,
  ReopenResult,
  ViewControl,
  ViewTarget,
} from "./control.js";
import type { Message } from "./frame.js";
import { Link } from "./link.js";
import type { Input, Output } from "./link.js";
import {
  FEATURE_BUNDLE,
  FEATURE_DISMISS,
  VERSIONS,
  byePayload,
  dismissPayload,
  helloPayload,
  openPayload,
  parseClosed,
  parseNeed,
  parseWelcome,
} from "./messages.js";
import type { ByeReason, Closed, ToolCall, Welcome } from "./messages.js";
import {
  DEVICE_ATTRIBUTES,
  ENTER_ALTERNATE_SCREEN,
  LEAVE_ALTERNATE_SCREEN,
  SttyTerminal,
} from "./terminal.js";
import type { Terminal } from "./terminal.js";
import { PencereTransport } from "./transport.js";

/**
 * What `serve` needs of the server: `McpServer` and `Server` of `@modelcontextprotocol/server` have it.
 * On a terminal `serve` connects once for each view it opens, and closes the session
 * when the view is gone: the first view, one the host starts again (§10.7) and one the
 * program reopens (§10.8).
 */
export interface Connectable {
  connect(transport: Transport): Promise<void>;
  close(): Promise<void>;
}

/** What `serve` is asked. */
export interface ServeOptions {
  /**
   * The tool whose `_meta.ui.resourceUri` names the view (MCP Apps § Resource
   * Discovery), and the arguments the host calls it with once the view is up.
   */
  tool: string | ToolCall;
  /**
   * The view's files, for a host that takes bundles. Without it, or with a host
   * that does not, the host reads the view's document from the server's own
   * `resources/read`, so the server should serve one self-contained document
   * at the same `ui://` resource.
   */
  bundle?: Bundle;
  /**
   * What the program does when no view is shown: no host answered, the host
   * shares no version, or it refused or ended the open before the view ran. It
   * runs after the terminal has been given back, and so it does when a
   * `ViewControl.reopen` closed the view and could not open another.
   */
  fallback?: () => void | Promise<void>;
  /** Ends the view, or the wait for a host, from the program's own code. */
  signal?: AbortSignal;
  /**
   * Lets the program close its view and go on, or show another, while `serve`
   * runs (§10.8). `serve` itself works as it did without one.
   */
  control?: ViewControl;
  /** How long to wait for the host's answer, in milliseconds (§5.3). 2000 by default. */
  answerTimeout?: number;
  /**
   * How long a reopening ({@link ViewControl.reopen}) waits on the host, in
   * milliseconds: for the `closed` that answers its `dismiss`, and then for the
   * `welcome` that answers the `hello` after it, each. 10000 by default, longer than
   * {@link answerTimeout}, because the first question is asked of a stream that may
   * have no host at all, and these are asked of a host that has just answered,
   * while it takes a view down and, after the view has been shown, a new one up.
   * When the time is up `serve` ends as `no-view` with the reason `timeout`.
   */
  reopenTimeout?: number;
  /**
   * After an answer that did not come in time: how long to go on reading, so that
   * a reply to `CSI c` that is only late does not reach the shell as typed text
   * (§5.3), in milliseconds. 1000 by default.
   */
  lateReplyGrace?: number;
}

/** Why no view was shown. */
export type NoViewReason =
  /** Nothing answered `hello` in time, or the terminal answered `CSI c` first, or the modes could not be set. */
  | "no-host"
  /** A host answered and shares no version with this library. */
  | "no-common-version"
  /**
   * A host that had answered did not answer in time while the program replaced its
   * view ({@link ViewControl.reopen}): the `closed` of the `dismiss`, or the `welcome`
   * of the `hello` after it. Not `no-host`: there is one.
   */
  | "timeout"
  /** The host refused the `open`. `code` says why when it does. */
  | "refused"
  /** The host ended the open before the view ran (a bundle that did not verify, the MCP start-up). */
  | "failed"
  /** The program's own `signal`, or a SIGINT or SIGTERM, ended the wait. */
  | "aborted";

/** What ended a view that ran. */
export type EndedBy =
  | "user"
  | "view"
  /** The program closed the view itself, with `ViewControl.close`. */
  | "program"
  | "error"
  /** The program's own `signal` ended it. */
  | "aborted"
  /** A SIGINT or SIGTERM, with a listener of the program's own to decide what comes next. */
  | "signal";

/** How `serve` ended. */
export type ServeResult =
  /** Served MCP on stdio until the client left. */
  | { readonly kind: "stdio" }
  /** No view was shown; the `fallback` ran if there is one, unless the wait was aborted. */
  | { readonly kind: "no-view"; readonly reason: NoViewReason; readonly code?: string }
  /** A view ran and has closed. */
  | { readonly kind: "closed"; readonly by: EndedBy };

type Signal = "SIGINT" | "SIGTERM";

/** What `serve` takes from the process it runs in. */
export interface ProcessLike {
  readonly pid: number;
  on(event: "exit", listener: (code: number) => void): unknown;
  on(event: Signal, listener: () => void): unknown;
  removeListener(event: "exit", listener: (code: number) => void): unknown;
  removeListener(event: Signal, listener: () => void): unknown;
  listenerCount(event: Signal): number;
  kill(pid: number, signal: Signal): unknown;
}

/** The streams, the tty and the process `serve` works with: the system's, unless a test lends others. */
export interface Environment {
  input: Readable & Input;
  output: Writable & Output;
  terminal: Terminal;
  process: ProcessLike;
}

/** The environment of this process. */
export function systemEnvironment(): Environment {
  return {
    input: process.stdin,
    output: process.stdout,
    terminal: new SttyTerminal(),
    process,
  };
}

/**
 * Serves `server`: see the module's description. Resolves when the view has
 * closed, when there is no view to show (after the `fallback` ran), or, on
 * stdio, when the client has gone.
 *
 * A SIGINT or SIGTERM while the view is up (⌃C reaches the program as SIGINT,
 * §10.4) sends `bye`, leaves the alternate screen, gives the terminal back and
 * then, if the program has no listener of its own for the signal, lets the signal
 * end the process. The same happens when the process exits while the view is up.
 */
export async function serve(server: Connectable, options: ServeOptions): Promise<ServeResult> {
  return serveWith(server, options, systemEnvironment());
}

/** {@link serve} in an environment of the caller's choosing. */
export async function serveWith(
  server: Connectable,
  options: ServeOptions,
  environment: Environment,
): Promise<ServeResult> {
  const { input, output } = environment;
  // Started by something that is not a terminal: an ordinary MCP server (§13).
  if (!input.isTTY) return serveStdio(server, input, output);
  let result: ServeResult;
  if (options.signal?.aborted) result = { kind: "no-view", reason: "aborted" };
  // There is a tty to read and nowhere to write the question.
  else if (!output.isTTY) result = { kind: "no-view", reason: "no-host" };
  else {
    const session = new TerminalSession(server, options, environment);
    const control = options.control;
    if (control !== undefined) bindControl(control, session);
    try {
      result = await session.run();
    } finally {
      if (control !== undefined) unbindControl(control, session);
    }
  }
  if (result.kind === "no-view" && result.reason !== "aborted") await options.fallback?.();
  return result;
}

async function serveStdio(
  server: Connectable,
  input: Readable,
  output: Writable,
): Promise<ServeResult> {
  const transport = new StdioServerTransport(input, output);
  await server.connect(transport);
  // `connect` has taken the transport's callbacks; this one is called after them.
  await new Promise<void>((resolve) => {
    const taken = transport.onclose;
    transport.onclose = () => {
      taken?.();
      resolve();
    };
  });
  return { kind: "stdio" };
}

// ---- the program's conversation with the host --------------------------------

type Offer = Extract<Welcome, { kind: "offer" }>;

/** What the host answered to `hello`, or what stood in for an answer. */
type Answer =
  | { readonly kind: "offer"; readonly welcome: Offer }
  | { readonly kind: "unsupported" }
  | { readonly kind: "reply" }
  | { readonly kind: "timeout" }
  | { readonly kind: "aborted" };

/** Asking the host, or opening a view (and running it), or over. */
type Phase = "asking" | "opening" | "over";

/** What a `dismiss` that is out is for, and what comes of the answer to it. */
type Plan =
  | { readonly kind: "close"; readonly resolve: (result: CloseResult) => void }
  | {
      readonly kind: "reopen";
      readonly next: ReopenOptions;
      readonly resolve: (result: ReopenResult) => void;
    };

/** A view is being closed or replaced at the program's request (§10.8), and how far that has come. */
interface Change {
  readonly plan: Plan;
  /** The id of the `dismiss`, which the `closed` that answers it carries as `re`. */
  readonly id: number;
  /** `dismissing`: waiting for the `closed`; `greeting`: waiting for a new token; `opening`: the new view's open is out. */
  stage: "dismissing" | "greeting" | "opening";
  /** The end of the wait for the `closed`, while `dismissing`. */
  timer: ReturnType<typeof setTimeout> | undefined;
}

class TerminalSession implements ViewTarget {
  readonly #server: Connectable;
  readonly #options: ServeOptions;
  readonly #env: Environment;
  readonly #link: Link;
  /** The tool and the files of the view that is opening or open, which a reopening may replace. */
  #tool: ToolCall;
  #bundle: Bundle | undefined;
  /** The host's last `welcome` listed `dismiss`. */
  #dismiss = false;
  #change: Change | undefined;

  #phase: Phase = "asking";
  #asking: { helloId: number; settle: (answer: Answer) => void } | undefined;
  #draining: (() => void) | undefined;
  #transport: PencereTransport | undefined;
  /** The token the open under way, or the view that runs, was made with: the view's identity (§10.3). */
  #token: string | undefined;
  #openId = 0;
  /** The host called the tool of the open at least once: the view ran. */
  #viewRan = false;
  #alternate = false;
  #cleaned = false;
  #unwatch: (() => void) | undefined;
  #end: { resolve: (result: ServeResult) => void; reject: (error: unknown) => void } | undefined;

  constructor(server: Connectable, options: ServeOptions, environment: Environment) {
    this.#server = server;
    this.#options = options;
    this.#env = environment;
    this.#tool = toolCall(options.tool);
    this.#bundle = options.bundle;
    this.#link = new Link(environment.input, environment.output, {
      message: (message) => this.#onMessage(message),
      reply: () => this.#onReply(),
    });
  }

  async run(): Promise<ServeResult> {
    try {
      this.#env.terminal.enter();
    } catch {
      // A tty whose modes cannot be set is a tty nothing can be asked on.
      return { kind: "no-view", reason: "no-host" };
    }
    try {
      this.#watch();
      this.#link.start();
      const hello = this.#link.send("hello", helloPayload());
      void this.#link.raw(DEVICE_ATTRIBUTES);
      const answer = await this.#ask(hello.id);
      if (answer.kind !== "offer") {
        if (answer.kind === "timeout") await this.#drainLateReply();
        this.#phase = "over";
        this.#cleanup(undefined);
        return { kind: "no-view", reason: noViewReason(answer.kind) };
      }
      const ended = new Promise<ServeResult>((resolve, reject) => {
        this.#end = { resolve, reject };
      });
      await this.#open(answer.welcome);
      return await ended;
    } finally {
      this.#phase = "over";
      this.#cleanup(undefined);
    }
  }

  // ---- asking ---------------------------------------------------------------

  #ask(helloId: number, timeout = this.#options.answerTimeout ?? 2000): Promise<Answer> {
    return new Promise<Answer>((resolve) => {
      const settle = (answer: Answer): void => {
        clearTimeout(timer);
        this.#asking = undefined;
        resolve(answer);
      };
      const timer = setTimeout(() => settle({ kind: "timeout" }), timeout);
      this.#asking = { helloId, settle };
    });
  }

  /** Reads on for a while so that the reply to `CSI c` of a slow terminal does not reach the shell (§5.3). */
  #drainLateReply(): Promise<void> {
    const grace = this.#options.lateReplyGrace ?? 1000;
    return new Promise<void>((resolve) => {
      const finish = (): void => {
        clearTimeout(timer);
        this.#draining = undefined;
        resolve();
      };
      const timer = setTimeout(finish, grace);
      this.#draining = finish;
    });
  }

  #onReply(): void {
    // The terminal answered `CSI c` before any `welcome`: there is no host (§5.3).
    this.#asking?.settle({ kind: "reply" });
    this.#draining?.();
  }

  // ---- opening --------------------------------------------------------------

  /** Connects a new MCP session to the stream and asks for the view with the token of `welcome` (§6.3). */
  async #open(welcome: Offer): Promise<void> {
    this.#phase = "opening";
    this.#dismiss = welcome.features.includes(FEATURE_DISMISS);
    const transport = new PencereTransport(this.#link);
    this.#transport = transport;
    // The session is connected before the open is written, because the host
    // starts it the moment it has accepted the open.
    await this.#server.connect(transport);
    if (this.#phase !== "opening") return;
    if (!this.#alternate) {
      this.#alternate = true;
      void this.#link.raw(ENTER_ALTERNATE_SCREEN);
    }
    this.#token = welcome.token;
    const bundle =
      this.#bundle !== undefined && welcome.features.includes(FEATURE_BUNDLE)
        ? this.#bundle.manifest
        : undefined;
    this.#openId = this.#link.send(
      "open",
      openPayload({ token: welcome.token, tool: this.#tool, bundle }),
    ).id;
  }

  #onMessage(message: Message): void {
    switch (message.kind) {
      case "welcome":
        this.#onWelcome(message);
        return;
      case "need":
        this.#onNeed(message);
        return;
      case "closed":
        this.#onClosed(message);
        return;
      case "msg":
        this.#onMsg(message);
        return;
      default:
      // A kind this version does not know, or one only a program sends: ignored (§4.5).
    }
  }

  #onWelcome(message: Message): void {
    const re = message.control.re;
    // A `hello` is out: the first one, or the one of a reopening (§10.8).
    const asking = this.#asking;
    if (asking !== undefined) {
      if (re === String(asking.helloId)) {
        const welcome = parseWelcome(message.payload, VERSIONS);
        if (welcome === undefined) return;
        asking.settle(
          welcome.kind === "offer" ? { kind: "offer", welcome } : { kind: "unsupported" },
        );
      } else if (re === undefined && this.#change !== undefined) {
        // A host that came up after a break while the program waits to be offered a view
        // (§10.7): what it offers is what the `hello` was for, and the `hello` may be lost.
        const welcome = parseWelcome(message.payload, VERSIONS);
        if (welcome?.kind === "offer" && welcome.restart) asking.settle({ kind: "offer", welcome });
      }
      return;
    }
    if (this.#phase !== "opening" || re !== undefined) return;
    // Unsolicited: a host that came up after a break asks for the view again (§10.7).
    const welcome = parseWelcome(message.payload, VERSIONS);
    if (welcome?.kind !== "offer" || !welcome.restart) return;
    // A reopening under way is about a view the host no longer knows, or about an open it
    // has forgotten: it ends here, unavailable, and the restart is the view that comes.
    this.#endChange()?.plan.resolve({ kind: "unavailable" });
    void this.#restart(welcome);
  }

  async #restart(welcome: Offer): Promise<void> {
    // The new open is a new view: whether the host runs it is not yet known.
    this.#viewRan = false;
    // The token is the old view's, which the host has forgotten: until the new open is out
    // there is no view, and a `dismiss` or a `bye` with the dead token would be nobody's.
    this.#token = undefined;
    try {
      // A fresh MCP session; the bundle is served from the host's store, and its
      // files are sent again for whatever the store lacks (§10.7).
      await this.#closeSession();
      await this.#open(welcome);
    } catch (error) {
      this.#fail(error);
    }
  }

  #onNeed(message: Message): void {
    const bundle = this.#bundle;
    if (this.#phase !== "opening" || bundle === undefined) return;
    if (message.control.re !== String(this.#openId)) return;
    const missing = parseNeed(message.payload);
    if (missing === undefined) return;
    try {
      for (const digest of missing) {
        const data = bundle.read(digest);
        if (data === undefined) continue;
        void this.#link.send("file", Buffer.from(data).toString("base64"), { digest });
      }
    } catch (error) {
      this.#fail(error);
    }
  }

  #onMsg(message: Message): void {
    if (this.#phase !== "opening") return;
    const received = this.#transport?.deliver(message.payload);
    if (
      received !== undefined &&
      "method" in received &&
      received.method === "tools/call" &&
      (received.params as { name?: unknown } | undefined)?.name === this.#tool.name
    ) {
      this.#viewRan = true;
      // A reopened view that has called its tool is the reopening's end; a call that
      // comes while the old view is still being closed is not (§10.8).
      const change = this.#change;
      if (change?.plan.kind === "reopen" && change.stage === "opening") {
        this.#endChange();
        change.plan.resolve({ kind: "reopened" });
      }
    }
  }

  // ---- ending ---------------------------------------------------------------

  #onClosed(message: Message): void {
    if (this.#phase !== "opening") return;
    const re = message.control.re;
    const closed = parseClosed(message.payload);
    if (closed === undefined) return;
    const change = this.#change;
    if (change?.stage === "dismissing" && re === String(change.id)) {
      // The answer to the program's own `dismiss`: the view is gone and the program goes on.
      change.stage = "greeting";
      clearTimeout(change.timer);
      void this.#dismissed(change.plan).catch((error: unknown) => this.#fail(error));
      return;
    }
    // An answer to an open that is no longer the program's one is not about this view.
    if (re !== undefined && re !== String(this.#openId)) return;
    void this.#hostClosed(closed).catch(() => {
      // Whatever went wrong while the view was being let go, the host said how it ended.
      this.#cleanup(undefined);
      this.#settle(resultOf(closed, this.#viewRan));
    });
  }

  async #hostClosed(closed: Closed): Promise<void> {
    // The host has torn the view down and forgotten its token: there is no `bye` to send.
    this.#phase = "over";
    this.#token = undefined;
    await this.#closeSession();
    await this.#link.flush();
    this.#cleanup(undefined);
    this.#settle(resultOf(closed, this.#viewRan));
  }

  /** Ends the MCP session of the view, if one runs. */
  async #closeSession(): Promise<void> {
    const transport = this.#transport;
    this.#transport = undefined;
    if (transport === undefined) return;
    try {
      await this.#server.close();
    } catch {
      // The session is over either way.
    }
  }

  #settle(result: ServeResult): void {
    this.#end?.resolve(result);
    this.#end = undefined;
    // What `serve` ends with is also what a reopening that was under way comes to.
    const change = this.#endChange();
    if (change === undefined) return;
    if (change.plan.kind === "reopen" && result.kind === "no-view") change.plan.resolve(result);
    else change.plan.resolve({ kind: "unavailable" });
  }

  #fail(error: unknown): void {
    const end = this.#end;
    this.#end = undefined;
    const change = this.#endChange();
    this.#phase = "over";
    this.#cleanup("error");
    end?.reject(error);
    change?.plan.resolve({ kind: "unavailable" });
  }

  // ---- closing the view at the program's request (§10.8) ---------------------

  reopen(next: ReopenOptions): Promise<ReopenResult> {
    const blocked = this.#blocked();
    if (blocked !== undefined) return Promise.resolve({ kind: blocked });
    return new Promise((resolve) => this.#dismissView({ kind: "reopen", next, resolve }));
  }

  close(): Promise<CloseResult> {
    const blocked = this.#blocked();
    if (blocked !== undefined) return Promise.resolve({ kind: blocked });
    return new Promise((resolve) => this.#dismissView({ kind: "close", resolve }));
  }

  /** Why the view cannot be dismissed now, if it cannot: there is none to close, or the host cannot do it. */
  #blocked(): "unavailable" | "unsupported" | undefined {
    if (this.#phase !== "opening" || this.#token === undefined || this.#change !== undefined)
      return "unavailable";
    return this.#dismiss ? undefined : "unsupported";
  }

  #dismissView(plan: Plan): void {
    const token = this.#token;
    if (token === undefined) return plan.resolve({ kind: "unavailable" });
    const { id } = this.#link.send("dismiss", dismissPayload(token));
    const change: Change = { plan, id, stage: "dismissing", timer: undefined };
    // A host that never answers is not waited for for ever: what the person is left with is
    // a view the program has asked to close and a program that waits on it.
    change.timer = setTimeout(
      () => this.#dismissTimedOut(change),
      this.#options.reopenTimeout ?? 10_000,
    );
    change.timer.unref();
    this.#change = change;
  }

  /** The reopening or closing that is under way ends: its timer is over, and nothing is under way. */
  #endChange(): Change | undefined {
    const change = this.#change;
    this.#change = undefined;
    clearTimeout(change?.timer);
    return change;
  }

  /** The host has not answered the `dismiss` in time. */
  #dismissTimedOut(change: Change): void {
    if (this.#change !== change || change.stage !== "dismissing") return;
    this.#giveUp("error", { kind: "no-view", reason: "timeout" });
  }

  /**
   * The host stopped answering while the program replaced its view: the program ends its wait,
   * as it does for a signal, with `bye` for the view the host may still show, the terminal
   * given back, and `result` for `serve`.
   */
  #giveUp(reason: ByeReason, result: ServeResult): void {
    this.#phase = "over";
    this.#cleanup(reason);
    void this.#closeSession();
    this.#settle(result);
  }

  /** The `closed` that answers the `dismiss` has come. */
  async #dismissed(plan: Plan): Promise<void> {
    // The host has torn the view down and forgotten its token: there is no `bye` to send.
    this.#token = undefined;
    if (plan.kind === "close") {
      this.#phase = "over";
      await this.#closeSession();
      await this.#link.flush();
      this.#cleanup(undefined);
      this.#endChange();
      plan.resolve({ kind: "closed" });
      this.#settle({ kind: "closed", by: "program" });
      return;
    }
    // The old MCP session ends before the new one is connected; the alternate screen stays.
    await this.#closeSession();
    if (this.#phase !== "opening") return;
    if (plan.next.bundle !== undefined) this.#bundle = plan.next.bundle;
    if (plan.next.tool !== undefined) this.#tool = toolCall(plan.next.tool);
    // The pane is idle: a new token takes a `hello`. The host is known and the fence
    // is for telling "no host" from "no answer yet", so there is none.
    const hello = this.#link.send("hello", helloPayload());
    const answer = await this.#ask(hello.id, this.#options.reopenTimeout ?? 10_000);
    if (this.#phase !== "opening" || this.#change === undefined) return;
    if (answer.kind !== "offer") {
      this.#phase = "over";
      // The host answered before: one that is silent now is late, not absent.
      const reason = answer.kind === "timeout" ? "timeout" : noViewReason(answer.kind);
      this.#settle({ kind: "no-view", reason });
      return;
    }
    this.#change.stage = "opening";
    // The new open is a new view: whether the host runs it is not yet known.
    this.#viewRan = false;
    await this.#open(answer.welcome);
  }

  /**
   * Gives the terminal back, on every path out: `bye` when a view is open or
   * opening and it is the program that ends (§10.3), the primary screen, the
   * tty's modes, the stream. Synchronous and once, so that a signal handler and
   * an `exit` handler can run it.
   */
  #cleanup(reason: ByeReason | undefined): void {
    if (this.#cleaned) return;
    this.#cleaned = true;
    this.#unwatch?.();
    this.#unwatch = undefined;
    const link = this.#link;
    if (reason !== undefined && this.#token !== undefined) {
      link.sendNow("bye", byePayload(this.#token, reason));
    }
    if (this.#alternate) link.writeNow(LEAVE_ALTERNATE_SCREEN);
    this.#env.terminal.restore();
    link.stop();
  }

  // ---- signals and exit -----------------------------------------------------

  #watch(): void {
    const proc = this.#env.process;
    const signals: readonly Signal[] = ["SIGINT", "SIGTERM"];
    const handlers = signals.map((signal) => {
      const handler = (): void => this.#interrupted(signal);
      proc.on(signal, handler);
      return { signal, handler };
    });
    // The process is leaving while the view is up: say so, and give the terminal back.
    const onExit = (code?: number): void =>
      this.#cleanup(code === undefined || code === 0 ? "exit" : "error");
    proc.on("exit", onExit);
    const abort = (): void => this.#aborted();
    this.#options.signal?.addEventListener("abort", abort, { once: true });
    this.#unwatch = () => {
      for (const { signal, handler } of handlers) proc.removeListener(signal, handler);
      proc.removeListener("exit", onExit);
      this.#options.signal?.removeEventListener("abort", abort);
    };
  }

  #interrupted(signal: Signal): void {
    const phase = this.#phase;
    this.#phase = "over";
    this.#cleanup("signal");
    // Our listener has kept the signal's default, which ends the process, from
    // running: give it back unless the program has a listener of its own.
    const proc = this.#env.process;
    if (proc.listenerCount(signal) === 0) {
      proc.kill(proc.pid, signal);
      return;
    }
    this.#asking?.settle({ kind: "aborted" });
    this.#draining?.();
    if (phase === "opening") this.#leaveOpening("signal");
  }

  #aborted(): void {
    const phase = this.#phase;
    this.#phase = "over";
    this.#cleanup("exit");
    this.#asking?.settle({ kind: "aborted" });
    this.#draining?.();
    if (phase === "opening") this.#leaveOpening("aborted");
  }

  /**
   * The program ends its own wait while a view is opening or open. `closed` says
   * a view ran; before the host has called the tool there was none to close.
   */
  #leaveOpening(by: "signal" | "aborted"): void {
    void this.#closeSession();
    this.#settle(this.#viewRan ? { kind: "closed", by } : { kind: "no-view", reason: "aborted" });
  }
}

function toolCall(tool: string | ToolCall): ToolCall {
  return typeof tool === "string" ? { name: tool } : tool;
}

function noViewReason(answer: "unsupported" | "reply" | "timeout" | "aborted"): NoViewReason {
  switch (answer) {
    case "unsupported":
      return "no-common-version";
    case "aborted":
      return "aborted";
    case "reply":
    case "timeout":
      return "no-host";
  }
}

/** What `closed` means for the program that asked for the view (§13). */
function resultOf(closed: Closed, viewRan: boolean): ServeResult {
  switch (closed.reason) {
    case "user":
      return { kind: "closed", by: "user" };
    case "view":
      return { kind: "closed", by: "view" };
    case "program":
      return { kind: "closed", by: "program" };
    case "refused":
    case "error":
      // Before the view ran, a refusal or a failure is as good as no host.
      if (viewRan) return { kind: "closed", by: "error" };
      return {
        kind: "no-view",
        reason: closed.reason === "refused" ? "refused" : "failed",
        ...(closed.code === undefined ? {} : { code: closed.code }),
      };
  }
}
