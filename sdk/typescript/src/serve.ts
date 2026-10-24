/**
 * `serve`: what a program does to show a view in the pane it runs in.
 *
 * Started in a terminal, the program asks the terminal whether it hosts views and which
 * kinds, in which versions of their interface (specification §5), opens the first it can
 * show with the token the answer carries (§6), sends the files of its bundle that the
 * host lacks (§8), answers the calls of the view and feeds its subscriptions (§9) and,
 * when the view closes, gives the terminal back (§10). With no host, no tty or no kind
 * in common it returns, and the program's own text interface takes over (§13). A program
 * that wants to close its view and go on, or show another, holds a `ViewControl`
 * (§10.8). A filter in a pipeline, whose stdin and stdout are its data, asks with
 * `filter` and talks to the terminal it opens itself (§13).
 */

import { closeSync, openSync } from "node:fs";
import type { Readable, Writable } from "node:stream";
import { ReadStream, WriteStream } from "node:tty";

import { BundleError } from "./bundle.js";
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
  ENTRY_TYPES,
  FEATURE_BUNDLE,
  FEATURE_DISMISS,
  MAX_VIEW_MESSAGE,
  VERSIONS,
  VIEW_KINDS,
  WEB_VERSION,
  byePayload,
  chooseView,
  dismissPayload,
  failurePayload,
  helloPayload,
  isVersion,
  openPayload,
  parseClosed,
  parseNeed,
  parseViewRequest,
  parseWelcome,
  progressPayload,
  pushPayload,
  responsePayload,
} from "./messages.js";
import type {
  Access,
  ByeReason,
  Choice,
  Closed,
  Icon,
  ViewKind,
  ViewOffer,
  ViewRequest,
  WelcomeOffer,
} from "./messages.js";
import {
  DEVICE_ATTRIBUTES,
  ENTER_ALTERNATE_SCREEN,
  LEAVE_ALTERNATE_SCREEN,
  SttyTerminal,
  runStty,
  sttyOn,
} from "./terminal.js";
import type { Terminal } from "./terminal.js";

// ---- what the program offers ---------------------------------------------------

/** The files of a view: a bundle, or a function of the version that makes the bundle of that version. */
export type BundleSource = Bundle | ((version: string) => Bundle | Promise<Bundle>);

/** A kind of view the program can show (specification §5.5). */
export interface ViewSpec {
  /** `web`, `wasm` or `lua`. */
  kind: ViewKind;
  /**
   * The versions of the kind's interface the view is written against, in the order the program
   * prefers them (§5.5). `["1.0"]` for a `web` view when left out; a `wasm` or `lua` view
   * has to say.
   */
  versions?: readonly string[];
  /**
   * The files of the view. The entry's type has to be that of the kind: `text/html` for `web`,
   * `application/wasm` for `wasm`, `text/x-lua` for `lua` (§6.3). A function is asked for the
   * bundle of the version the host and the program agreed on, which suits a view that has a
   * build for each version of the kit.
   */
  bundle: BundleSource;
  /** What a `web` view asks to reach, which the person is asked about (§12.2). Not for a kit view. */
  access?: Access;
}

/** An error a handler ends a call or a subscription with: the view gets `code`, and `message` if there is one (§9.2). */
export class ViewError extends Error {
  /** A short name the view can act on. */
  readonly code: string;
  /** The words for the person, if the error was made with some. */
  readonly detail: string | undefined;

  constructor(code: string, message?: string) {
    super(message ?? code);
    this.name = "ViewError";
    this.code = code;
    this.detail = message;
  }
}

/** What a handler of a call is given besides the call's parameters. */
export interface CallContext {
  /** The method the view called. */
  readonly method: string;
  /**
   * Aborted when the view gives the call up with a `cancel` (§9.2) or when the view is over.
   * A handler that stops its work then need not answer: the answer would be ignored.
   */
  readonly signal: AbortSignal;
  /** Tells the view how far the call has got (§9.2). Ignored once the call is cancelled or answered. */
  progress(done: number, total?: number, note?: string): void;
}

/**
 * Answers a call of the view: returns the result (`undefined` and `null` answer `{}`, since a
 * result is never `null`) or throws a {@link ViewError}, or any error, which is
 * `{ code: "internal" }`. A returned `ViewError` is an error response too.
 */
export type CallHandler = (params: unknown, call: CallContext) => unknown;

/** What a handler of a topic is given to feed a subscription with. */
export interface Subscription {
  /** The topic the view subscribed to. */
  readonly topic: string;
  /** Aborted when the view cancels the subscription or is over. */
  readonly signal: AbortSignal;
  /**
   * Pushes a value to the view. `false` when it was not sent: the subscription is over, or the
   * message would be over 1 MiB (§9.3). `undefined` is pushed as `null`.
   */
  push(value: unknown): boolean;
}

/**
 * Feeds a subscription of the view. May return a function (or a promise of one) that runs when
 * the subscription ends — the view cancels it, or the view is over — to stop whatever feeds it.
 * Throwing ends the subscription with an error response.
 */
export type TopicHandler = (
  params: unknown,
  subscription: Subscription,
) => void | (() => void) | Promise<void | (() => void)>;

/** What `serve` is asked. */
export interface ServeOptions {
  /**
   * The kinds of view the program can show, in the order it prefers them; at least one. The
   * hello lists them with their versions, and the first kind the host will open, at the first
   * of its versions the host speaks, is the view (§5.3).
   */
  views: readonly ViewSpec[];
  /** What the program calls itself, and what it calls the view: the host names its pane with them (§7.3). */
  program?: string;
  title?: string;
  /** Icons for the program (§7.3). */
  icons?: readonly Icon[];
  /** The calls the view may make, by method. A method that is not here is answered `not-found`. */
  calls?: Readonly<Record<string, CallHandler>>;
  /** The subscriptions the view may make, by topic. A topic that is not here is answered `not-found`. */
  topics?: Readonly<Record<string, TopicHandler>>;
  /**
   * What the program does when no view is shown: no tty, no host answered, the host shares no
   * version or no kind, or it refused or ended the open before the view ran. It runs after the
   * terminal has been given back, and so it does when a `ViewControl.reopen` closed the view
   * and could not open another. It is told why.
   */
  fallback?: (why: NoView) => void | Promise<void>;
  /** Ends the view, or the wait for a host, from the program's own code. */
  signal?: AbortSignal;
  /**
   * Lets the program close its view and go on, or show another, while `serve`
   * runs (§10.8). `serve` itself works as it did without one.
   */
  control?: ViewControl;
  /**
   * The program is a filter in a pipeline (`… | tool | …`): its stdin and stdout
   * carry the pipeline's data, and `serve` leaves them alone. It asks for the view
   * on the program's controlling terminal (`/dev/tty`) whatever stdin and stdout
   * are; a program with no controlling terminal has no view (`no-tty`) and its
   * `fallback` runs. A tool chooses this itself, with a flag of its own such as
   * `--select`: from inside, a pipeline and a program that another program started
   * look the same, and a guess that took one for the other would write frames into a
   * terminal that other program is using (§13).
   */
  filter?: boolean;
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
  /**
   * After a `bye`: how long to go on reading, and throwing away what is read, for the `closed`
   * that answers it, before the unread rest of the input is thrown away too and the terminal is
   * given back (§10.3), in milliseconds. 500 by default.
   */
  byeTimeout?: number;
}

// ---- how it ended -----------------------------------------------------------------

/** Why no view was shown. */
export type NoViewReason =
  /** Stdin or stdout is not a tty, a filter has no controlling terminal, or the tty's modes cannot be set: there is no pane to ask in (§13). */
  | "no-tty"
  /** Nothing answered `hello` in time, or the terminal answered `CSI c` first (§5.3). */
  | "no-host"
  /** A host answered and shares no protocol version with this library. */
  | "no-common-version"
  /** A host answered and will open none of the kinds of view the program offered, at a version both speak (§5.2). */
  | "no-common-view"
  /**
   * A host that had answered did not answer in time while the program replaced its
   * view ({@link ViewControl.reopen}): the `closed` of the `dismiss`, or the `welcome`
   * of the `hello` after it. Not `no-host`: there is one.
   */
  | "timeout"
  /** The host refused the `open`. `code` says why when it does. */
  | "refused"
  /** The host ended the open before the view ran (a bundle that did not verify, files that did not come in time). */
  | "failed"
  /** The program's own `signal`, or a SIGINT or SIGTERM, ended the wait. */
  | "aborted";

/** No view was shown, and why. */
export interface NoView {
  readonly kind: "no-view";
  readonly reason: NoViewReason;
  /** The `code` of the `closed` of a refusal or a failure (§6.4). */
  readonly code?: string;
  /** The words of the `closed`, for people (§8.8). */
  readonly message?: string;
}

/** What ended a view that ran. */
export type EndedBy =
  /** The person closed it (§10.5). */
  | "user"
  /** The page asked to be closed (§10.6). */
  | "view"
  /** The program closed the view itself, with `ViewControl.close`. */
  | "program"
  /** The view failed after it ran, or the host ended it with an error. `code` and `message` say how. */
  | "error"
  /** The program's own `signal` ended it. */
  | "aborted"
  /** A SIGINT or SIGTERM, with a listener of the program's own to decide what comes next. */
  | "signal";

/** The kind of view that was shown, and the version of its interface. */
export interface ShownView {
  readonly kind: string;
  readonly version: string;
}

/** How `serve` ended. */
export type ServeResult =
  /** No view was shown; the `fallback` ran if there is one, unless the wait was aborted. */
  | NoView
  /** A view ran and has closed. */
  | {
      readonly kind: "closed";
      readonly by: EndedBy;
      /** The view that ran. */
      readonly view: ShownView;
      /** With `by` `error`: the `code` of the `closed`. */
      readonly code?: string;
      /** With `by` `error`: the words the host gave, which for a view it draws itself are its error and the line (§8.8). */
      readonly message?: string;
    };

// ---- the environment -----------------------------------------------------------------

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
    terminal: new SttyTerminal(runStty, 0),
    process,
  };
}

/**
 * The controlling terminal of this process, opened for a filter: one descriptor to
 * read and one to write, and `stty` on the first. `undefined` when the process has
 * none. `close` lets the terminal go, without which the open handle would keep the
 * process alive once its pipeline is done.
 */
export function controllingTerminal(): { environment: Environment; close(): void } | undefined {
  let read: number | undefined;
  try {
    read = openSync("/dev/tty", "r");
    const write = openSync("/dev/tty", "w");
    const input = new ReadStream(read);
    const output = new WriteStream(write);
    return {
      environment: { input, output, terminal: new SttyTerminal(sttyOn(read), read), process },
      close: () => {
        input.destroy();
        output.destroy();
      },
    };
  } catch {
    if (read !== undefined) closeSync(read);
    return undefined;
  }
}

// ---- checking what the program offers ----------------------------------------------------

/** A kind of view the program offers, checked. */
interface Prepared {
  readonly kind: ViewKind;
  readonly versions: readonly string[];
  readonly bundle: BundleSource;
  readonly access: Access | undefined;
}

/** Whether the bundle's entry has the type the kind of view asks for (§6.3). */
function checkEntry(kind: ViewKind, bundle: Bundle): Bundle {
  const entry = bundle.manifest.files.find((file) => file.path === bundle.manifest.entry);
  const wanted = ENTRY_TYPES[kind];
  if (entry?.type !== wanted)
    throw new BundleError(
      `the entry "${bundle.manifest.entry}" of a ${kind} view has the type ${entry?.type ?? "none"}; it has to be ${wanted}`,
    );
  return bundle;
}

/** @throws {TypeError | BundleError} for a list that is not one a program may offer. */
function prepare(views: readonly ViewSpec[]): Prepared[] {
  if (!Array.isArray(views) || views.length === 0)
    throw new TypeError("a program offers at least one kind of view");
  const prepared: Prepared[] = [];
  views.forEach((spec, at) => {
    const name = `views[${at}]`;
    if (!VIEW_KINDS.includes(spec.kind))
      throw new TypeError(`${name}: "${String(spec.kind)}" is not one of ${VIEW_KINDS.join(", ")}`);
    if (prepared.some((view) => view.kind === spec.kind))
      throw new TypeError(`${name}: the kind "${spec.kind}" is listed twice`);
    const versions = spec.versions ?? (spec.kind === "web" ? [WEB_VERSION] : undefined);
    if (versions === undefined || versions.length === 0)
      throw new TypeError(`${name}: a ${spec.kind} view names the versions of its interface`);
    for (const version of versions)
      if (typeof version !== "string" || !isVersion(version))
        throw new TypeError(`${name}: "${String(version)}" is not a version (major.minor)`);
    if (new Set(versions).size !== versions.length)
      throw new TypeError(`${name}: a version is listed twice`);
    if (spec.access !== undefined && spec.kind !== "web")
      throw new TypeError(`${name}: only a web view has an access to ask for`);
    if (typeof spec.bundle !== "function") checkEntry(spec.kind, spec.bundle);
    prepared.push({
      kind: spec.kind,
      versions: [...versions],
      bundle: spec.bundle,
      access: spec.access,
    });
  });
  return prepared;
}

// ---- serve ------------------------------------------------------------------------------

/**
 * Shows the view of the program: see the module's description. Resolves when the view has
 * closed or when there is no view to show (after the `fallback` ran).
 *
 * A SIGINT or SIGTERM while the view is up (⌃C reaches the program as SIGINT,
 * §10.4) sends `bye`, reads and throws away the input until the host's answer (§10.3),
 * leaves the alternate screen, gives the terminal back and then, if the program has
 * no listener of its own for the signal, lets the signal end the process. The same
 * happens when the process exits while the view is up, except that it cannot wait.
 *
 * @throws {TypeError | BundleError} for `views` that are not a list a program may offer.
 */
export async function serve(options: ServeOptions): Promise<ServeResult> {
  prepare(options.views);
  if (options.filter !== true) return serveWith(options, systemEnvironment());
  const terminal = controllingTerminal();
  if (terminal === undefined) {
    const aborted = options.signal?.aborted === true;
    return ended(options, { kind: "no-view", reason: aborted ? "aborted" : "no-tty" });
  }
  try {
    return await serveWith(options, terminal.environment);
  } finally {
    terminal.close();
  }
}

/** {@link serve} in an environment of the caller's choosing. */
export async function serveWith(
  options: ServeOptions,
  environment: Environment,
): Promise<ServeResult> {
  const views = prepare(options.views);
  const { input, output } = environment;
  let result: ServeResult;
  if (options.signal?.aborted) result = { kind: "no-view", reason: "aborted" };
  // There is no tty to read, or nowhere to write the question (§13).
  else if (!input.isTTY || !output.isTTY) result = { kind: "no-view", reason: "no-tty" };
  else {
    const session = new TerminalSession(views, options, environment);
    const control = options.control;
    if (control !== undefined) bindControl(control, session);
    try {
      result = await session.run();
    } finally {
      if (control !== undefined) unbindControl(control, session);
    }
  }
  return ended(options, result);
}

/** How `serve` ends: with the `fallback` first when no view was shown, unless the wait was aborted. */
async function ended(options: ServeOptions, result: ServeResult): Promise<ServeResult> {
  if (result.kind === "no-view" && result.reason !== "aborted") await options.fallback?.(result);
  return result;
}

// ---- the program's conversation with the host --------------------------------

/** What the host answered to `hello`, or what stood in for an answer. */
type Answer =
  | { readonly kind: "offer"; readonly welcome: WelcomeOffer }
  | { readonly kind: "unsupported" }
  | { readonly kind: "reply" }
  | { readonly kind: "timeout" }
  | { readonly kind: "aborted" };

/**
 * Asking the host; a view being opened or open; the program saying goodbye (`bye` is out, and its
 * answer is waited for); or over.
 */
type Phase = "asking" | "showing" | "leaving" | "over";

/** What a `dismiss` that is out is for, and what comes of the answer to it. */
type Plan =
  | { readonly kind: "close"; readonly resolve: (result: CloseResult) => void }
  | {
      readonly kind: "reopen";
      /** What the program offers from now on, if the reopening brings another list. */
      readonly views: Prepared[] | undefined;
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

/** A subscription of the view that runs. */
interface Subscribed {
  readonly controller: AbortController;
  /** What the topic's handler returned to stop whatever feeds it. */
  cleanup: (() => void) | undefined;
}

/** The program is waiting for the `closed` that answers its `bye` (§10.3). */
interface Farewell {
  /** The id of the `bye`. */
  readonly id: number;
  /** The wait is over: the answer came, or the time is up. Runs what comes next, once. */
  readonly finish: () => void;
  /** The wait is given up without running what comes next: the program is leaving by another way. */
  readonly drop: () => void;
}

/** A handler by name, among the handlers the program gave; a name like `constructor` is no handler. */
function handlerOf<T>(
  handlers: Readonly<Record<string, T>> | undefined,
  name: string,
): T | undefined {
  return handlers !== undefined && Object.hasOwn(handlers, name) ? handlers[name] : undefined;
}

/** What an error that ended a call or a subscription tells the view. */
function failureOf(id: number, error: unknown): string {
  if (error instanceof ViewError) return failurePayload(id, error.code, error.detail);
  return failurePayload(id, "internal", error instanceof Error ? error.message : undefined);
}

class TerminalSession implements ViewTarget {
  readonly #options: ServeOptions;
  readonly #env: Environment;
  readonly #link: Link;
  /** What the program offers: replaced by a reopening that brings others. */
  #views: Prepared[];
  /** The host's last `welcome` listed `dismiss`. */
  #dismiss = false;
  #change: Change | undefined;

  #phase: Phase = "asking";
  #asking: { helloId: number; settle: (answer: Answer) => void } | undefined;
  #draining: (() => void) | undefined;
  /** The files of the view that is opening or open, which `need` is answered from. */
  #bundle: Bundle | undefined;
  /** The token the open under way, or the view that runs, was made with: the view's identity (§10.3). */
  #token: string | undefined;
  #openId = 0;
  #shown: ShownView | undefined;
  /** The view is up on the host's side: what the view says is served. */
  #live = false;
  /** The host answered the open with a `need`: it took the open (§6.4). */
  #accepted = false;
  /** The view has asked something of the program. */
  #heard = false;
  readonly #calls = new Map<number, AbortController>();
  readonly #subs = new Map<number, Subscribed>();
  #alternate = false;
  #cleaned = false;
  /** The id of the `bye`, once it is out. */
  #byeId: number | undefined;
  #farewell: Farewell | undefined;
  #unwatch: (() => void) | undefined;
  #end: { resolve: (result: ServeResult) => void; reject: (error: unknown) => void } | undefined;

  constructor(views: Prepared[], options: ServeOptions, environment: Environment) {
    this.#views = views;
    this.#options = options;
    this.#env = environment;
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
      return { kind: "no-view", reason: "no-tty" };
    }
    try {
      this.#watch();
      this.#link.start();
      const hello = this.#link.send("hello", helloPayload(this.#offers()));
      void this.#link.raw(DEVICE_ATTRIBUTES);
      const answer = await this.#ask(hello.id);
      if (answer.kind !== "offer") {
        if (answer.kind === "timeout") await this.#drainLateReply();
        this.#phase = "over";
        this.#cleanup();
        return { kind: "no-view", reason: noViewReason(answer.kind) };
      }
      const choice = this.#choose(answer.welcome);
      if (choice === undefined) {
        // A host with a token and nothing to open: the program knows at once, and does not wait (§5.3).
        this.#phase = "over";
        this.#cleanup();
        return { kind: "no-view", reason: "no-common-view" };
      }
      const ended = new Promise<ServeResult>((resolve, reject) => {
        this.#end = { resolve, reject };
      });
      try {
        await this.#open(answer.welcome, choice);
      } catch (error) {
        this.#fail(error);
      }
      return await ended;
    } finally {
      this.#phase = "over";
      this.#cleanup();
    }
  }

  /** What the `hello` lists: the kinds the program offers, with their versions. */
  #offers(): ViewOffer[] {
    return this.#views.map((view) => ({ kind: view.kind, versions: view.versions }));
  }

  /** The view to open, if the host will open one of the program's kinds (§5.3). */
  #choose(welcome: WelcomeOffer): Choice | undefined {
    // Every kind of view arrives as a bundle: a host without the feature has nothing to open (§8.1).
    if (!welcome.features.includes(FEATURE_BUNDLE)) return undefined;
    return chooseView(this.#offers(), welcome.views);
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

  /** Asks for the view with the token of `welcome` (§6.3): the kind and the version `choice` names. */
  async #open(welcome: WelcomeOffer, choice: Choice): Promise<void> {
    this.#phase = "showing";
    this.#dismiss = welcome.features.includes(FEATURE_DISMISS);
    const spec = this.#views.find((view) => view.kind === choice.kind);
    if (spec === undefined)
      throw new Error(`the host chose a kind of view the program did not offer`);
    const bundle =
      typeof spec.bundle === "function"
        ? checkEntry(spec.kind, await spec.bundle(choice.version))
        : spec.bundle;
    // Something ended the wait while the bundle was being made: the view is not opened.
    if (this.#phase !== "showing") return;
    if (!this.#alternate) {
      this.#alternate = true;
      void this.#link.raw(ENTER_ALTERNATE_SCREEN);
    }
    this.#bundle = bundle;
    this.#token = welcome.token;
    this.#shown = { kind: choice.kind, version: choice.version };
    this.#accepted = false;
    this.#heard = false;
    this.#live = true;
    this.#openId = this.#link.send(
      "open",
      openPayload({
        token: welcome.token,
        view: choice.kind,
        version: choice.version,
        program: this.#options.program,
        title: this.#options.title,
        icons: this.#options.icons,
        access: spec.access,
        bundle: bundle.manifest,
      }),
    ).id;
  }

  #onMessage(message: Message): void {
    if (this.#phase === "leaving") {
      // The program has said `bye`: what comes is read and thrown away, but for the answer to it (§10.3).
      if (message.kind === "closed") this.#onFarewell(message);
      return;
    }
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
    if (this.#phase !== "showing" || re !== undefined) return;
    // Unsolicited: a host that came up after a break asks for the view again (§10.7).
    const welcome = parseWelcome(message.payload, VERSIONS);
    if (welcome?.kind !== "offer" || !welcome.restart) return;
    const choice = this.#choose(welcome);
    if (choice === undefined) return;
    // A reopening under way is about a view the host no longer knows, or about an open it
    // has forgotten: it ends here, unavailable, and the restart is the view that comes.
    this.#endChange()?.plan.resolve({ kind: "unavailable" });
    void this.#restart(welcome, choice);
  }

  async #restart(welcome: WelcomeOffer, choice: Choice): Promise<void> {
    // What the old view had asked is over with it, and its token is the host's to have forgotten:
    // until the new open is out there is no view, and a `dismiss` or a `bye` with the dead token
    // would be nobody's.
    this.#forgetView();
    try {
      await this.#open(welcome, choice);
    } catch (error) {
      this.#fail(error);
    }
  }

  #onNeed(message: Message): void {
    const bundle = this.#bundle;
    if (this.#phase !== "showing" || bundle === undefined || this.#accepted) return;
    if (message.control.re !== String(this.#openId)) return;
    const missing = parseNeed(message.payload);
    if (missing === undefined) return;
    this.#accepted = true;
    try {
      for (const digest of new Set(missing)) {
        const data = bundle.read(digest);
        if (data === undefined) continue;
        void this.#link.send("file", Buffer.from(data).toString("base64"), { digest });
      }
    } catch (error) {
      this.#fail(error);
      return;
    }
    // A reopened view whose open the host took is the reopening's end (§10.8).
    const change = this.#change;
    if (change?.plan.kind === "reopen" && change.stage === "opening") {
      this.#endChange();
      change.plan.resolve({ kind: "reopened" });
    }
  }

  // ---- what the view says (§9) ----------------------------------------------

  #onMsg(message: Message): void {
    if (this.#phase !== "showing" || !this.#live) return;
    const request = parseViewRequest(message.payload);
    if (request === undefined) return;
    this.#heard = true;
    switch (request.type) {
      case "call":
        this.#call(request);
        return;
      case "subscribe":
        this.#subscribe(request);
        return;
      case "cancel":
        this.#cancel(request.id);
        return;
    }
  }

  /** Writes a message of the six toward the view; `false` when it was not (the view is gone, or it is over a message's limit). */
  #tell(payload: string): boolean {
    if (this.#phase !== "showing") return false;
    if (payload.length > MAX_VIEW_MESSAGE) return false;
    try {
      this.#link.send("msg", payload);
      return true;
    } catch {
      return false;
    }
  }

  /** The response that ends a call with `result`. */
  #answer(id: number, result: unknown): void {
    let payload: string;
    try {
      payload = responsePayload(id, result);
    } catch {
      payload = failurePayload(id, "internal", "the result cannot be written as JSON");
    }
    if (payload.length > MAX_VIEW_MESSAGE)
      payload = failurePayload(id, "message-too-long", "the result is over the limit of a message");
    this.#tell(payload);
  }

  #call(request: Extract<ViewRequest, { type: "call" }>): void {
    const { id } = request;
    // An id that is out is not used again; a view that does is not following §9.2.
    if (this.#calls.has(id) || this.#subs.has(id)) return;
    const handler = handlerOf(this.#options.calls, request.method);
    if (handler === undefined) {
      this.#tell(failurePayload(id, "not-found", "there is nothing to call by that name"));
      return;
    }
    const controller = new AbortController();
    this.#calls.set(id, controller);
    const context: CallContext = {
      method: request.method,
      signal: controller.signal,
      progress: (done, total, note) => {
        if (controller.signal.aborted) return;
        if (!Number.isFinite(done) || (total !== undefined && !Number.isFinite(total))) return;
        this.#tell(progressPayload(id, done, total, note));
      },
    };
    void (async () => {
      try {
        const result = await handler(request.params, context);
        // Cancelled, or the view is over: the answer would be ignored.
        if (controller.signal.aborted) return;
        this.#calls.delete(id);
        if (result instanceof ViewError) this.#tell(failureOf(id, result));
        else this.#answer(id, result);
      } catch (error) {
        if (controller.signal.aborted) return;
        this.#calls.delete(id);
        this.#tell(failureOf(id, error));
      }
    })();
  }

  #subscribe(request: Extract<ViewRequest, { type: "subscribe" }>): void {
    const { id, topic } = request;
    if (this.#calls.has(id) || this.#subs.has(id)) return;
    const handler = handlerOf(this.#options.topics, topic);
    if (handler === undefined) {
      this.#tell(failurePayload(id, "not-found", "there is nothing to subscribe to by that name"));
      return;
    }
    const controller = new AbortController();
    const subscribed: Subscribed = { controller, cleanup: undefined };
    this.#subs.set(id, subscribed);
    const subscription: Subscription = {
      topic,
      signal: controller.signal,
      push: (value) => {
        if (controller.signal.aborted) return false;
        let payload: string;
        try {
          payload = pushPayload(id, value);
        } catch {
          return false;
        }
        return this.#tell(payload);
      },
    };
    void (async () => {
      try {
        const cleanup = await handler(request.params, subscription);
        if (typeof cleanup !== "function") return;
        // Cancelled while the handler started: what feeds the subscription stops at once.
        if (controller.signal.aborted) stop(cleanup);
        else subscribed.cleanup = cleanup;
      } catch (error) {
        if (controller.signal.aborted) return;
        this.#subs.delete(id);
        controller.abort();
        this.#tell(failureOf(id, error));
      }
    })();
  }

  /** The view gives up a call that is out, or ends a subscription; an id that is not out changes nothing (§9.2). */
  #cancel(id: number): void {
    const call = this.#calls.get(id);
    if (call !== undefined) {
      this.#calls.delete(id);
      call.abort();
      return;
    }
    const subscribed = this.#subs.get(id);
    if (subscribed === undefined) return;
    this.#subs.delete(id);
    this.#stop(subscribed);
  }

  #stop(subscribed: Subscribed): void {
    subscribed.controller.abort();
    const { cleanup } = subscribed;
    subscribed.cleanup = undefined;
    if (cleanup !== undefined) stop(cleanup);
  }

  /** What the view had asked is over with it: calls are cancelled, subscriptions stop. */
  #endView(): void {
    const calls = [...this.#calls.values()];
    const subs = [...this.#subs.values()];
    this.#calls.clear();
    this.#subs.clear();
    for (const call of calls) call.abort();
    for (const subscribed of subs) this.#stop(subscribed);
  }

  /** The host's view is gone, or the program no longer believes in it: nothing of it is served. */
  #forgetView(): void {
    this.#live = false;
    this.#token = undefined;
    this.#bundle = undefined;
    this.#accepted = false;
    this.#heard = false;
    this.#endView();
  }

  // ---- ending ---------------------------------------------------------------

  #onClosed(message: Message): void {
    const shown = this.#shown;
    if (this.#phase !== "showing" || this.#token === undefined || shown === undefined) return;
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
    void this.#hostClosed(closed, shown);
  }

  async #hostClosed(closed: Closed, shown: ShownView): Promise<void> {
    const result = this.#resultOf(closed, shown);
    // The host has torn the view down and forgotten its token: there is no `bye` to send.
    this.#phase = "over";
    this.#forgetView();
    await this.#link.flush();
    this.#cleanup();
    this.#settle(result);
  }

  /** What `closed` means for the program that asked for the view (§13). */
  #resultOf(closed: Closed, shown: ShownView): ServeResult {
    const detail = {
      ...(closed.code === undefined ? {} : { code: closed.code }),
      ...(closed.message === undefined ? {} : { message: closed.message }),
    };
    switch (closed.reason) {
      case "user":
      case "view":
      case "program":
        return { kind: "closed", by: closed.reason, view: shown };
      case "refused":
        return { kind: "no-view", reason: "refused", ...detail };
      case "error":
        // A view the host draws itself that failed ran; before the view ran, an error is as good as no host.
        if (closed.code === "view" || this.#heard)
          return { kind: "closed", by: "error", view: shown, ...detail };
        return { kind: "no-view", reason: "failed", ...detail };
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
    this.#endView();
    const phase = this.#phase;
    this.#phase = "leaving";
    // A program that fails says so and waits for the answer like any program that ends.
    this.#goodbye(phase === "over" ? undefined : "error", () => {
      this.#phase = "over";
      end?.reject(error);
      change?.plan.resolve({ kind: "unavailable" });
    });
  }

  // ---- closing the view at the program's request (§10.8) ---------------------

  reopen(next: ReopenOptions): Promise<ReopenResult> {
    const blocked = this.#blocked();
    if (blocked !== undefined) return Promise.resolve({ kind: blocked });
    let views: Prepared[] | undefined;
    try {
      views = next.views === undefined ? undefined : prepare(next.views);
    } catch (error) {
      // A list the program may not offer is the caller's mistake, and the view is as it was.
      return Promise.reject(error);
    }
    return new Promise((resolve) => this.#dismissView({ kind: "reopen", views, resolve }));
  }

  close(): Promise<CloseResult> {
    const blocked = this.#blocked();
    if (blocked !== undefined) return Promise.resolve({ kind: blocked });
    return new Promise((resolve) => this.#dismissView({ kind: "close", resolve }));
  }

  /** Why the view cannot be dismissed now, if it cannot: there is none to close, or the host cannot do it. */
  #blocked(): "unavailable" | "unsupported" | undefined {
    if (this.#phase !== "showing" || this.#token === undefined || this.#change !== undefined)
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
    this.#phase = "leaving";
    this.#endView();
    this.#goodbye(reason, () => {
      this.#phase = "over";
      this.#settle(result);
    });
  }

  /** The `closed` that answers the `dismiss` has come. */
  async #dismissed(plan: Plan): Promise<void> {
    // The host has torn the view down and forgotten its token: there is no `bye` to send.
    this.#forgetView();
    if (plan.kind === "close") {
      this.#phase = "over";
      await this.#link.flush();
      this.#cleanup();
      this.#endChange();
      plan.resolve({ kind: "closed" });
      const view = this.#shown;
      if (view !== undefined) this.#settle({ kind: "closed", by: "program", view });
      return;
    }
    if (plan.views !== undefined) this.#views = plan.views;
    // The pane is idle: a new token takes a `hello`. The host is known and the fence
    // is for telling "no host" from "no answer yet", so there is none.
    const hello = this.#link.send("hello", helloPayload(this.#offers()));
    const answer = await this.#ask(hello.id, this.#options.reopenTimeout ?? 10_000);
    if (this.#phase !== "showing" || this.#change === undefined) return;
    const choice = answer.kind === "offer" ? this.#choose(answer.welcome) : undefined;
    if (answer.kind !== "offer" || choice === undefined) {
      this.#phase = "over";
      // The host answered before: one that is silent now is late, not absent.
      const reason =
        answer.kind === "timeout"
          ? "timeout"
          : answer.kind === "offer"
            ? "no-common-view"
            : noViewReason(answer.kind);
      this.#settle({ kind: "no-view", reason });
      return;
    }
    this.#change.stage = "opening";
    await this.#open(answer.welcome, choice);
  }

  // ---- giving the terminal back ---------------------------------------------

  /**
   * Says `bye` (§10.3) with the token of the view that is opening or open, once; the id of the
   * `bye`, or `undefined` when there is no view or it was said already.
   */
  #sayBye(reason: ByeReason): number | undefined {
    if (this.#byeId !== undefined || this.#token === undefined) return undefined;
    // What is queued and not yet written would reach the host after the `bye`, and open or feed a
    // view the program has just ended.
    this.#link.mute();
    this.#byeId = this.#link.sendNow("bye", byePayload(this.#token, reason));
    return this.#byeId;
  }

  /**
   * The program ends, or ends its use of the terminal, by its own choice: `bye` when a view is
   * opening or open, then the input is read, and thrown away, until the answer to it (§10.3), then
   * the terminal is given back, and then `then` runs. Asynchronous only for the wait.
   */
  #goodbye(reason: ByeReason | undefined, then: () => void): void {
    const id = reason === undefined ? undefined : this.#sayBye(reason);
    if (id === undefined) {
      this.#cleanup();
      then();
      return;
    }
    let timer: ReturnType<typeof setTimeout> | undefined;
    let done = false;
    const finish = (): void => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      this.#farewell = undefined;
      this.#cleanup();
      then();
    };
    timer = setTimeout(finish, this.#options.byeTimeout ?? 500);
    this.#farewell = {
      id,
      finish,
      drop: () => {
        done = true;
        clearTimeout(timer);
        this.#farewell = undefined;
      },
    };
  }

  /** The program is saying goodbye and a `closed` came: the answer to its `bye` ends the wait (§10.3). */
  #onFarewell(message: Message): void {
    const farewell = this.#farewell;
    if (farewell === undefined) return;
    const re = message.control.re;
    // The answer to the `bye`. A `closed` with no `re` is the host's own word that the view is gone
    // (the person closed it as the `bye` went out): the `bye` will not be answered, and nothing
    // more is coming.
    if (re !== undefined && re !== String(farewell.id)) return;
    if (parseClosed(message.payload) === undefined) return;
    farewell.finish();
  }

  /**
   * Gives the terminal back, on every path out: `bye` when a view is open or
   * opening and it is the program that ends and it was not said yet (§10.3), what the
   * terminal wrote and the program did not read, the primary screen, the tty's modes, the
   * stream. Synchronous and once, so that a signal handler and an `exit` handler can run
   * it. The wait for the answer to the `bye` is not here: it comes before, where the program
   * can wait ({@link TerminalSession.#goodbye}); an `exit` handler cannot.
   *
   * The listeners go last. A parent that forwards signals (npm running a script)
   * sends the ⌃C the tty already sent a moment later; while a listener is there
   * the second one only waits for the event loop, and without one its default
   * action ends the process between two of the writes above — the terminal is
   * left on the alternate screen, with the shell's prompt drawn on it.
   */
  #cleanup(reason?: ByeReason): void {
    if (this.#cleaned) return;
    this.#cleaned = true;
    this.#farewell?.drop();
    if (reason !== undefined) this.#sayBye(reason);
    this.#endView();
    const link = this.#link;
    // What the host wrote before it learned of the `bye` and the program did not read must not be
    // left in the tty for whatever reads it next (§10.3); typed-ahead characters go with it.
    if (this.#byeId !== undefined) this.#env.terminal.discardInput();
    if (this.#alternate) link.writeNow(LEAVE_ALTERNATE_SCREEN);
    this.#env.terminal.restore();
    link.stop();
    this.#unwatch?.();
    this.#unwatch = undefined;
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
    // The process is leaving while the view is up: say so, and give the terminal back. It cannot
    // wait for the answer to the `bye`: nothing runs after this.
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
    // The program is saying goodbye already: this is the second signal of the same ⌃C, which a
    // parent forwards, and the first one is being served.
    if (phase === "leaving") return;
    this.#phase = "leaving";
    this.#goodbye("signal", () => {
      // Our listener has kept the signal's default, which ends the process, from
      // running: give it back unless the program has a listener of its own.
      const proc = this.#env.process;
      if (proc.listenerCount(signal) === 0) {
        proc.kill(proc.pid, signal);
        return;
      }
      this.#leave(phase, "signal");
    });
  }

  #aborted(): void {
    const phase = this.#phase;
    if (phase === "leaving") return;
    this.#phase = "leaving";
    this.#goodbye("exit", () => this.#leave(phase, "aborted"));
  }

  /**
   * The program has ended its own wait, or its view, and gone on: whoever waits for the host or
   * for the view is told. `closed` says a view ran; before the host has taken the open there was
   * none to close.
   */
  #leave(phase: Phase, by: "signal" | "aborted"): void {
    this.#asking?.settle({ kind: "aborted" });
    this.#draining?.();
    if (phase !== "showing") return;
    this.#phase = "over";
    const view = this.#shown;
    this.#settle(
      this.#accepted && view !== undefined
        ? { kind: "closed", by, view }
        : { kind: "no-view", reason: "aborted" },
    );
  }
}

/** Stops whatever feeds a subscription: what a handler returned. Its failure is not the view's. */
function stop(cleanup: () => void): void {
  try {
    cleanup();
  } catch {
    // The subscription is over either way.
  }
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
