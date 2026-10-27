/**
 * The bridge of a page (specification §9.4), for the page of a `web` view.
 *
 * A page reaches the host through the web platform's message channel and nothing else: it posts
 * single-member JSON objects to the window that embeds it and hears the host's on its own window.
 * This module is that channel as functions. It has no dependency, runs in a browser as an ES
 * module and does nothing until it is used.
 *
 * ```js
 * import { call, close, onContext, openLink, subscribe } from "@pencere/sdk/page";
 *
 * onContext((context) => document.documentElement.setAttribute("data-theme", context.mode));
 * const { containers } = await call("list");
 * const stop = subscribe("stats", undefined, {
 *   onPush: (value) => draw(value),
 *   onError: (error) => console.warn(`the readings stopped: ${error.message}`),
 * });
 * ```
 *
 * A page has no `node_modules`: the file goes in the bundle beside the page (the program's
 * `bundleFromDirectory` takes it with its `files` option) and an import map in the page's HTML
 * tells the browser where the name is:
 * `<script type="importmap">{"imports":{"@pencere/sdk/page":"./pencere-page.js"}}</script>`.
 *
 * A page that is not embedded by anything (opened on its own, in a browser) has no host: its calls
 * are rejected with the code `no-host`, and nothing is posted.
 */

/** The roles of the terminal's theme, each a colour as `#rrggbb` (§11.2). */
export interface Colors {
  readonly background: string;
  readonly foreground: string;
  readonly dim: string;
  readonly accent: string;
  /** The cursor's colour; the accent where the terminal's theme has none. */
  readonly cursor: string;
  readonly border: string;
  readonly success: string;
  readonly error: string;
  readonly warning: string;
  readonly info: string;
  /** The background of what the person selected. */
  readonly selection: string;
  /**
   * The theme's sixteen palette colours: black, red, green, yellow, blue, magenta, cyan, white,
   * then the same eight bright.
   */
  readonly ansi: readonly string[];
}

/**
 * What the host tells the page about the terminal (§9.5). A host that does not know an optional
 * member leaves it out; a page reads what it needs and keeps what it has for the rest.
 */
export interface Context {
  /** `dark` when the pane's background is darker than its foreground, `light` otherwise. */
  readonly mode: "dark" | "light";
  readonly colors: Colors;
  /** The terminal's monospace font, as a CSS `font-family` list. */
  readonly font?: string;
  /** The content area of the pane, in CSS pixels. */
  readonly size: { readonly width: number; readonly height: number };
  /** The person's language and region (BCP 47). */
  readonly locale?: string;
  /** The person's time zone (IANA). */
  readonly timeZone?: string;
  /** Whether the person asked the system for less motion. */
  readonly reduceMotion: boolean;
  /** What the person allowed of what the page asked for (§12.2). */
  readonly access?: { readonly connect?: readonly string[] };
}

/** How far a call that takes a while has got (§9.2). */
export interface Progress {
  readonly done: number;
  readonly total?: number;
  readonly note?: string;
}

/**
 * An error the program answered a call with, or ended a subscription with (§9.2): a short `code` to
 * act on, and words for the person (the code when the program gave none). A call of a page that has
 * no host is rejected with the code `no-host`.
 */
export class CallError extends Error {
  /** A short name a page can act on. */
  readonly code: string;

  constructor(code: string, message?: string) {
    super(message ?? code);
    this.name = "CallError";
    this.code = code;
  }
}

export interface CallOptions {
  /** Called with each `progress` the program reports for this call, until it is answered. */
  readonly onProgress?: (progress: Progress) => void;
  /**
   * Gives the call up: the `cancel` goes to the program, and the promise rejects with the
   * signal's reason. The program may stop the work and need not answer (§9.2); an answer after the
   * `cancel` is ignored.
   */
  readonly signal?: AbortSignal;
}

/** What hears a subscription. */
export interface SubscribeHandlers<Value = unknown> {
  /** A value of the subscription, in order. */
  readonly onPush: (value: Value) => void;
  /**
   * The program refused the subscription, or ended it, with an error (§9.2): called once, and no
   * push follows. A program ends a subscription in no other way; the page ends it with `cancel`.
   */
  readonly onError?: (error: CallError) => void;
  /**
   * The subscription is over, whoever ended it: the program (after `onError`) or the page (it
   * cancelled, or the bridge was disposed). Called once.
   */
  readonly onEnd?: () => void;
}

/** A subscription of the page: a function that ends it, which also has the name `cancel`. */
export type Subscribed = (() => void) & { cancel(): void };

/** The two things a page has of the host (§9.4): a way to post a message to it and to hear it. */
export interface Channel {
  /** Posts one message to the host. */
  post(message: object): void;
  /** Hears what the host posts; returns the way to stop. */
  listen(listener: (data: unknown) => void): () => void;
  /** `false` for a page nothing embeds: it has no host to ask. */
  readonly connected?: boolean;
}

/** The bridge of one page. */
export interface Bridge {
  /**
   * Asks the program to do something and resolves with its answer (a `result` is never `null`);
   * rejects with a {@link CallError} when the program answers with an error.
   */
  call<Result = unknown>(method: string, params?: unknown, options?: CallOptions): Promise<Result>;
  /**
   * Asks to hear `topic` as it changes, with `params` (or `undefined`). `handlers` is what hears
   * it, or a function that takes the values. Returns the way to end the subscription, a function
   * that has the name `cancel` too: it sends the `cancel`, and nothing is pushed after it.
   */
  subscribe<Value = unknown>(
    topic: string,
    params: unknown,
    handlers: ((value: Value) => void) | SubscribeHandlers<Value>,
  ): Subscribed;
  /**
   * Calls `listener` with the terminal's context now, if the page has it, and whenever it changes
   * (§9.5). Returns the way to stop.
   */
  onContext(listener: (context: Context) => void): () => void;
  /** The terminal's context as the host last told it, or `undefined` before it did. */
  context(): Context | undefined;
  /** Asks the host to close the view (§9.6). The program is told and decides what it does. */
  close(): void;
  /**
   * Asks the host to open `url` in the person's browser (§9.7). Only `http:` and `https:` links
   * are taken, and the host should ask the person first; it does not say whether it did.
   */
  openLink(url: string): void;
}

interface Pending {
  readonly resolve: (result: never) => void;
  readonly reject: (error: unknown) => void;
  readonly onProgress: ((progress: Progress) => void) | undefined;
}

interface Hearing {
  readonly handlers: SubscribeHandlers<never>;
  /** Marks the subscription over, and tells `onEnd`, once. */
  end(): void;
}

type Fields = Record<string, unknown>;

const isObject = (value: unknown): value is Fields =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** The id of a message that carries one: a positive integer (§9.2). */
const idOf = (body: Fields): number | undefined =>
  typeof body["id"] === "number" && Number.isInteger(body["id"]) && body["id"] > 0
    ? body["id"]
    : undefined;

function errorOf(value: unknown): CallError | undefined {
  if (!isObject(value) || typeof value["code"] !== "string") return undefined;
  return new CallError(
    value["code"],
    typeof value["message"] === "string" ? value["message"] : undefined,
  );
}

const text = (value: unknown): string | undefined =>
  typeof value === "string" ? value : undefined;

/**
 * The context of a message of the host: its required members (the mode, the roles of the theme, the
 * size) make a context, the rest is read as far as it is of the right kind; `undefined` for an
 * object that is no context.
 */
function understood(body: Fields): Context | undefined {
  const { mode, colors, size, font, locale, timeZone, reduceMotion, access } = body;
  if ((mode !== "dark" && mode !== "light") || !isObject(colors) || !isObject(size)) {
    return undefined;
  }
  const connect =
    isObject(access) && Array.isArray(access["connect"])
      ? access["connect"].filter((origin): origin is string => typeof origin === "string")
      : undefined;
  return {
    mode,
    colors: colors as unknown as Colors,
    ...(text(font) === undefined ? {} : { font: text(font) as string }),
    size: {
      width: typeof size["width"] === "number" ? size["width"] : 0,
      height: typeof size["height"] === "number" ? size["height"] : 0,
    },
    ...(text(locale) === undefined ? {} : { locale: text(locale) as string }),
    ...(text(timeZone) === undefined ? {} : { timeZone: text(timeZone) as string }),
    reduceMotion: reduceMotion === true,
    ...(connect === undefined ? {} : { access: { connect } }),
  };
}

/** The bridge of a page whose channel to the host is `channel`. */
export function createBridge(channel: Channel): Bridge {
  const connected = channel.connected !== false;
  let next = 1;
  const calls = new Map<number, Pending>();
  const subscriptions = new Map<number, Hearing>();
  const listeners = new Set<(context: Context) => void>();
  let current: Context | undefined;
  let stopListening: (() => void) | undefined;

  const respond = (id: number, body: Fields): void => {
    // Both a `result` and an `error`, or neither, is no answer (§9.2).
    const failed = "error" in body;
    if (failed === "result" in body) return;
    const waiting = calls.get(id);
    if (waiting !== undefined) {
      calls.delete(id);
      if (failed) waiting.reject(errorOf(body["error"]) ?? new CallError("invalid"));
      else waiting.resolve(body["result"] as never);
      return;
    }
    // An answer with an error to a subscription is the program ending it.
    const hearing = subscriptions.get(id);
    if (hearing === undefined || !failed) return;
    subscriptions.delete(id);
    hearing.handlers.onError?.(errorOf(body["error"]) ?? new CallError("invalid"));
    hearing.end();
  };

  const hear = (data: unknown): void => {
    if (!isObject(data)) return;
    const members = Object.keys(data);
    const name = members[0];
    const body = name === undefined ? undefined : data[name];
    if (members.length !== 1 || name === undefined || !isObject(body)) return;
    if (name === "context") {
      const context = understood(body);
      if (context === undefined) return;
      current = context;
      for (const listener of [...listeners]) listener(context);
      return;
    }
    const id = idOf(body);
    if (id === undefined) return;
    if (name === "response") respond(id, body);
    else if (name === "push") subscriptions.get(id)?.handlers.onPush(body["value"] as never);
    else if (name === "progress") {
      const done = body["done"];
      if (typeof done !== "number") return;
      const note = body["note"];
      const total = body["total"];
      calls.get(id)?.onProgress?.({
        done,
        ...(typeof total === "number" ? { total } : {}),
        ...(typeof note === "string" ? { note } : {}),
      });
    }
  };

  /**
   * Starts to hear the host, once, at the page's first use of the bridge. The host sends `context`
   * once the page has loaded, but a page cannot know it was listening in time, so it asks.
   */
  const listening = (): void => {
    if (stopListening !== undefined || !connected) return;
    stopListening = channel.listen(hear);
    if (current === undefined) channel.post({ context: {} });
  };

  const noHost = (): CallError => new CallError("no-host", "This page is not shown by a terminal");

  const bridge: Bridge = {
    call<Result>(method: string, params?: unknown, options: CallOptions = {}): Promise<Result> {
      const { signal, onProgress } = options;
      if (signal?.aborted) return Promise.reject(signal.reason);
      if (!connected) return Promise.reject(noHost());
      listening();
      const id = next++;
      return new Promise<Result>((resolve, reject) => {
        const abort = (): void => {
          if (!calls.delete(id)) return;
          channel.post({ cancel: { id } });
          reject(signal?.reason);
        };
        calls.set(id, {
          resolve: (result: never) => {
            signal?.removeEventListener("abort", abort);
            resolve(result);
          },
          reject: (error: unknown) => {
            signal?.removeEventListener("abort", abort);
            reject(error);
          },
          onProgress,
        });
        signal?.addEventListener("abort", abort, { once: true });
        channel.post({ call: params === undefined ? { id, method } : { id, method, params } });
      });
    },

    subscribe<Value>(
      topic: string,
      params: unknown,
      handlers: ((value: Value) => void) | SubscribeHandlers<Value>,
    ): Subscribed {
      const heard = (
        typeof handlers === "function" ? { onPush: handlers } : handlers
      ) as SubscribeHandlers<never>;
      let id = 0;
      let over = false;
      const finish = (): void => {
        if (over) return;
        over = true;
        heard.onEnd?.();
      };
      const cancel = (): void => {
        // The program has ended it, or it was ended already: there is nothing to cancel.
        if (subscriptions.delete(id)) channel.post({ cancel: { id } });
        finish();
      };
      const subscribed: Subscribed = Object.assign(cancel, { cancel });
      if (!connected) {
        // A page with no host has nothing to hear: the subscription is refused, after the caller
        // has the way to cancel it.
        queueMicrotask(() => {
          if (over) return;
          heard.onError?.(noHost());
          finish();
        });
        return subscribed;
      }
      listening();
      id = next++;
      subscriptions.set(id, { handlers: heard, end: finish });
      channel.post({
        subscribe: params === undefined ? { id, topic } : { id, topic, params },
      });
      return subscribed;
    },

    onContext(listener: (context: Context) => void): () => void {
      listening();
      listeners.add(listener);
      if (current !== undefined) listener(current);
      return () => {
        listeners.delete(listener);
      };
    },

    context(): Context | undefined {
      listening();
      return current;
    },

    close(): void {
      if (connected) channel.post({ close: {} });
    },

    openLink(url: string): void {
      if (connected) channel.post({ link: { url: String(url) } });
    },
  };
  return bridge;
}

/** What of a window the bridge uses: the window that embeds the page, and its `message` events. */
export interface WindowLike {
  readonly parent: { postMessage(message: unknown, targetOrigin: string): void };
  addEventListener(type: "message", listener: (event: MessageEventLike) => void): void;
  removeEventListener(type: "message", listener: (event: MessageEventLike) => void): void;
}

/** The parts of a `message` event the bridge reads. */
export interface MessageEventLike {
  readonly source: unknown;
  readonly data: unknown;
}

/**
 * The channel of a page in `win`: posts to the window that embeds it and hears only that window
 * (the host takes a message only from its page, and a page takes only the host's). A page that is
 * its own parent is embedded by nothing: its channel is not connected.
 */
export function windowChannel(win: WindowLike): Channel {
  const connected = (win.parent as unknown) !== win;
  return {
    connected,
    post: (message) => win.parent.postMessage(message, "*"),
    listen: (listener) => {
      const hear = (event: MessageEventLike): void => {
        if (event.source === win.parent) listener(event.data);
      };
      win.addEventListener("message", hear);
      return () => win.removeEventListener("message", hear);
    },
  };
}

let shared: Bridge | undefined;

/** The bridge of this page, made on first use. */
function page(): Bridge {
  return (shared ??= createBridge(windowChannel(globalThis as unknown as WindowLike)));
}

/** {@link Bridge.call}, on this page's bridge. */
export function call<Result = unknown>(
  method: string,
  params?: unknown,
  options?: CallOptions,
): Promise<Result> {
  return page().call<Result>(method, params, options);
}

/** {@link Bridge.subscribe}, on this page's bridge. */
export function subscribe<Value = unknown>(
  topic: string,
  params: unknown,
  handlers: ((value: Value) => void) | SubscribeHandlers<Value>,
): Subscribed {
  return page().subscribe<Value>(topic, params, handlers);
}

/** {@link Bridge.onContext}, on this page's bridge. */
export function onContext(listener: (context: Context) => void): () => void {
  return page().onContext(listener);
}

/** {@link Bridge.context}, on this page's bridge. */
export function context(): Context | undefined {
  return page().context();
}

/** {@link Bridge.close}, on this page's bridge. */
export function close(): void {
  page().close();
}

/** {@link Bridge.openLink}, on this page's bridge. */
export function openLink(url: string): void {
  page().openLink(url);
}
