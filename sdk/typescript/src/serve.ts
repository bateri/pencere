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
import { constants } from "node:os";
import { ReadStream, WriteStream } from "node:tty";

import { BundleError } from "./bundle.js";
import type { Bundle } from "./bundle.js";
import { toBase64 } from "./bytes.js";
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
  VERSIONS,
  VIEW_KINDS,
  WEB_VERSION,
  byePayload,
  chooseView,
  dismissPayload,
  helloPayload,
  isVersion,
  openPayload,
  parseClosed,
  parseNeed,
  parseWelcome,
} from "./messages.js";
import type {
  Access,
  ByeReason,
  Choice,
  Closed,
  Icon,
  ViewKind,
  ViewOffer,
  WelcomeOffer,
} from "./messages.js";
import { synchronousOutput } from "./output.js";
import { ViewError, createRouter } from "./router.js";
import type { CallContext, CallHandler, Router, Subscription, TopicHandler } from "./router.js";
import {
  DEVICE_ATTRIBUTES,
  ENTER_ALTERNATE_SCREEN,
  LEAVE_ALTERNATE_SCREEN,
  SttyTerminal,
  runStty,
  runtime,
  sttyOn,
} from "./terminal.js";
import type { Terminal } from "./terminal.js";

// What the handlers of a program are made of lives with the router; `serve` is where a program meets it.
export { ViewError } from "./router.js";
export type { CallContext, CallHandler, Subscription, TopicHandler } from "./router.js";

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

/** ⌃Z: the signal that stops a program (job control). */
type StopSignal = "SIGTSTP";

/** The numbers of the signals `serve` handles (POSIX). */
const SIGNAL_NUMBERS: Readonly<Record<Signal, number>> = { SIGINT: 2, SIGTERM: 15 };

/** What `serve` takes from the process it runs in. */
export interface ProcessLike {
  readonly pid: number;
  on(event: "exit", listener: (code: number) => void): unknown;
  on(event: Signal | StopSignal, listener: () => void): unknown;
  removeListener(event: "exit", listener: (code: number) => void): unknown;
  removeListener(event: Signal | StopSignal, listener: () => void): unknown;
  listenerCount(event: Signal): number;
  /**
   * Signals a process. The program stops itself with `SIGSTOP`, by its number (Bun does not know
   * the name), and the call returns when it is continued.
   */
  kill(pid: number, signal: Signal | number): unknown;
  /** Ends the process; used when a program may not signal itself (Deno without `--allow-run`). */
  exit?(code?: number): unknown;
}

/** The streams, the tty and the process `serve` works with: the system's, unless a test lends others. */
export interface Environment {
  input: Input;
  output: Output;
  terminal: Terminal;
  process: ProcessLike;
}

/** The environment of this process. */
export function systemEnvironment(): Environment {
  return {
    input: process.stdin,
    // Bun's stream for a tty does not report a write: see `synchronousOutput`.
    output: runtime === "bun" ? synchronousOutput(1, process.stdout.isTTY) : process.stdout,
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
    // Bun's stream for a tty does not report a write: see `synchronousOutput`.
    const stream = runtime === "bun" ? undefined : new WriteStream(write);
    const output = stream ?? synchronousOutput(write, true);
    return {
      environment: { input, output, terminal: new SttyTerminal(sttyOn(read), read), process },
      close: () => {
        input.destroy();
        if (stream === undefined) closeSync(write);
        else stream.destroy();
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
  const views = prepare(options.views);
  if (options.filter !== true) return serveChecked(options, views, systemEnvironment());
  const terminal = controllingTerminal();
  if (terminal === undefined) {
    const aborted = options.signal?.aborted === true;
    return ended(options, { kind: "no-view", reason: aborted ? "aborted" : "no-tty" });
  }
  try {
    return await serveChecked(options, views, terminal.environment);
  } finally {
    terminal.close();
  }
}

/** {@link serve} in an environment of the caller's choosing. */
export async function serveWith(
  options: ServeOptions,
  environment: Environment,
): Promise<ServeResult> {
  return serveChecked(options, prepare(options.views), environment);
}

/** {@link serve} for views that have been checked. */
async function serveChecked(
  options: ServeOptions,
  views: Prepared[],
  environment: Environment,
): Promise<ServeResult> {
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

/** A wait that ends when the answer comes or the time is up. */
interface Wait {
  /** The wait is over; runs what comes next, once. */
  readonly finish: () => void;
  /** The wait is given up without running what comes next: the program is leaving by another way. */
  readonly drop: () => void;
}

/** The program is waiting for the `closed` that answers its `bye` (§10.3). */
interface Farewell extends Wait {
  /** The id of the `bye` or `dismiss`. */
  readonly id: number;
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
  /** What the view asks of the program: found among the handlers, and answered. */
  readonly #router: Router;
  #alternate = false;
  #cleaned = false;
  /** The id of the `bye`, once it is out. */
  #byeId: number | undefined;
  /** The `hello` is out. */
  #helloSent = false;
  /** The terminal answered `CSI c`: what the host answered to the `hello` has come before it (§5.2). */
  #replied = false;
  /** A program that leaves before the host has answered waits for that, to throw away what comes (§5.3). */
  #replyWait: Wait | undefined;
  /** What is unread in the input is thrown away as the terminal is given back, though no `bye` was said. */
  #drain = false;
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
    this.#router = createRouter(
      {
        ...(options.calls === undefined ? {} : { calls: options.calls }),
        ...(options.topics === undefined ? {} : { topics: options.topics }),
      },
      (payload) => this.#tell(payload),
    );
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
      this.#helloSent = true;
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
    this.#replied = true;
    this.#replyWait?.finish();
    // A program that is leaving only waits for this: it concludes nothing from it.
    if (this.#phase === "leaving") return;
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
        void this.#link.send("file", toBase64(data), { digest });
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
    if (this.#router.handle(message.payload)) this.#heard = true;
  }

  /** Writes a message of the six toward the view; `false` when it was not (the view is gone). */
  #tell(payload: string): boolean {
    if (this.#phase !== "showing") return false;
    this.#link.send("msg", payload);
    return true;
  }

  /** What the view had asked is over with it: calls are cancelled, subscriptions stop. */
  #endView(): void {
    this.#router.end();
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
    await this.#greet(this.#change);
  }

  /**
   * The pane is idle and the program wants a view: a new token takes a `hello`, then `open`, as for
   * the first. The host is known and the fence is for telling "no host" from "no answer yet", so
   * there is none. `change` is the reopening this is the middle of, if it is one.
   */
  async #greet(change: Change | undefined): Promise<void> {
    const hello = this.#link.send("hello", helloPayload(this.#offers()));
    const answer = await this.#ask(hello.id, this.#options.reopenTimeout ?? 10_000);
    if (this.#phase !== "showing" || this.#change !== change) return;
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
    if (change !== undefined) change.stage = "opening";
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
   *
   * With no view to say it to, a program that leaves before the host has answered its `hello`
   * reads on until the terminal answers `CSI c` (the host's answer, if there is one, comes before
   * that, §5.2), and throws away what it read: a `welcome` and the reply are not typed text for the
   * shell (§5.3).
   */
  #goodbye(reason: ByeReason | undefined, then: () => void): void {
    const id = reason === undefined ? undefined : this.#sayBye(reason);
    if (id !== undefined) {
      this.#awaitAnswer(id, () => {
        this.#cleanup();
        then();
      });
      return;
    }
    if (reason !== undefined && this.#byeId === undefined && this.#helloSent && !this.#replied) {
      this.#awaitReply(() => {
        this.#drain = true;
        this.#cleanup();
        then();
      });
      return;
    }
    this.#cleanup();
    then();
  }

  /** Waits for the `closed` that answers message `id`, for the time a `bye` is given (§10.3); `then` runs when it comes or the time is up. */
  #awaitAnswer(id: number, then: () => void): void {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let done = false;
    const finish = (): void => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      this.#farewell = undefined;
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

  /** Waits for the terminal's answer to `CSI c`, for the time a `bye` is given; `then` runs when it comes or the time is up. */
  #awaitReply(then: () => void): void {
    let timer: ReturnType<typeof setTimeout> | undefined;
    let done = false;
    const finish = (): void => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      this.#replyWait = undefined;
      then();
    };
    timer = setTimeout(finish, this.#options.byeTimeout ?? 500);
    this.#replyWait = {
      finish,
      drop: () => {
        done = true;
        clearTimeout(timer);
        this.#replyWait = undefined;
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
   * can wait ({@link TerminalSession.#goodbye}). An `exit` handler cannot wait for the event loop,
   * so it passes a `reason` and the answer is read from the tty, synchronously, first
   * ({@link TerminalSession.#readLastWord}).
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
    this.#replyWait?.drop();
    if (reason !== undefined) this.#sayBye(reason);
    this.#endView();
    const link = this.#link;
    // The process is leaving and nothing runs after this: what the host answers is read here.
    if (reason !== undefined) this.#readLastWord();
    // What the host wrote before it learned of the `bye` and the program did not read must not be
    // left in the tty for whatever reads it next (§10.3); typed-ahead characters go with it.
    if (this.#byeId !== undefined || this.#drain) this.#env.terminal.discardInput();
    if (this.#alternate) link.writeNow(LEAVE_ALTERNATE_SCREEN);
    this.#env.terminal.restore();
    link.stop();
    this.#unwatch?.();
    this.#unwatch = undefined;
  }

  /**
   * The process is exiting and nothing runs after this: the host's last word is read from the tty
   * here, without the event loop, for as long as `bye` is given (§10.3), and thrown away. It is the
   * `closed` that answers the `bye`; for a program that has no view yet, the terminal's answer to
   * `CSI c`, which the host's own comes before (§5.2, §5.3). What is left of the input is thrown
   * away after it.
   */
  #readLastWord(): void {
    const id = this.#byeId;
    let done: () => boolean;
    if (id !== undefined) {
      let answered = false;
      this.#farewell = { id, finish: () => void (answered = true), drop: () => {} };
      done = () => answered;
    } else if (this.#helloSent && !this.#replied) {
      done = () => this.#replied;
    } else {
      return;
    }
    // What comes is thrown away, but for the answer.
    this.#phase = "leaving";
    this.#drain = true;
    this.#env.terminal.waitFor((bytes) => {
      this.#link.feed(bytes);
      return done();
    }, this.#options.byeTimeout ?? 500);
    this.#farewell = undefined;
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
    // ⌃Z: the tty is given back before the program stops. A runtime that cannot catch SIGTSTP
    // (there is none on Windows) leaves the program with the signal's default.
    const onStop = (): void => this.#suspend();
    let stoppable = true;
    try {
      proc.on("SIGTSTP", onStop);
    } catch {
      stoppable = false;
    }
    const abort = (): void => this.#aborted();
    this.#options.signal?.addEventListener("abort", abort, { once: true });
    this.#unwatch = () => {
      for (const { signal, handler } of handlers) proc.removeListener(signal, handler);
      if (stoppable) proc.removeListener("SIGTSTP", onStop);
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
        try {
          proc.kill(proc.pid, signal);
        } catch {
          // Deno refuses a program the signal to itself unless it may run programs; the status a
          // shell reports for a program that the signal ended (128 and the signal's number) is the
          // next best.
          proc.exit?.(128 + SIGNAL_NUMBERS[signal]);
        }
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

  // ---- ⌃Z ---------------------------------------------------------------------

  /**
   * ⌃Z: the program is to stop, and the shell that gets the terminal must find it as it left it. A
   * view that is up is closed (`dismiss`, or `bye` for a host that has none, and the `closed` that
   * answers it is waited for, as for ⌃C), the primary screen and the tty's modes are given back,
   * and the program stops. When it is continued it takes the modes again and asks for its view
   * anew, as for the first one (§10.8).
   *
   * Without a view (the program is still asking, or is replacing the view) only the modes are
   * given back and taken again. A program that is on its way out has no use for the signal.
   */
  #suspend(): void {
    if (this.#cleaned || this.#phase === "leaving" || this.#phase === "over") return;
    const token = this.#token;
    if (this.#phase === "showing" && token !== undefined && this.#change === undefined) {
      // What the view had asked is over with it; what the host says meanwhile is read and thrown
      // away, but for its answer.
      this.#phase = "leaving";
      this.#endView();
      const id = this.#dismiss
        ? this.#link.send("dismiss", dismissPayload(token)).id
        : this.#link.send("bye", byePayload(token, "signal")).id;
      this.#awaitAnswer(id, () => void this.#suspended());
      return;
    }
    this.#env.terminal.restore();
    this.#stop();
    this.#take();
  }

  /** The view is gone from the host: the terminal is given back, the program stops, and it asks again when it is continued. */
  async #suspended(): Promise<void> {
    this.#forgetView();
    // What the host wrote before it learned of the program's word is not left for the shell.
    this.#env.terminal.discardInput();
    if (this.#alternate) {
      this.#alternate = false;
      this.#link.writeNow(LEAVE_ALTERNATE_SCREEN);
    }
    this.#env.terminal.restore();
    this.#stop();
    // Continued.
    if (!this.#take()) {
      this.#phase = "over";
      this.#settle({ kind: "no-view", reason: "no-tty" });
      return;
    }
    this.#phase = "showing";
    try {
      await this.#greet(undefined);
    } catch (error) {
      this.#fail(error);
    }
  }

  /**
   * Stops the program, with the signal that cannot be caught, and returns when it is continued. The
   * tty is the shell's meanwhile. A program that may not signal itself (Deno without `--allow-run`)
   * does not stop; it goes straight on.
   */
  #stop(): void {
    const proc = this.#env.process;
    try {
      proc.kill(proc.pid, constants.signals.SIGSTOP);
    } catch {
      // Nothing stops it.
    }
  }

  /** Takes the tty's modes again, as when the program began; `false` when it cannot (it was continued in the background). */
  #take(): boolean {
    try {
      this.#env.terminal.enter();
      return true;
    } catch {
      return false;
    }
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
