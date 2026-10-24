/**
 * The router: what a program does with what its view says (specification §9.2). The view
 * calls, subscribes and cancels; the router finds the handler by name, runs it, and writes
 * the program's `response`, `push` and `progress` back through the function it was given.
 * `serve` writes them to the host; a test, or a harness that drives the handlers of an app
 * without a terminal, writes them where it likes. There is one dispatch: `serve` uses this one.
 *
 * ```ts
 * const sent: unknown[] = [];
 * const router = createRouter({ calls: { list: () => ({ items: [] }) } }, (payload) => {
 *   sent.push(JSON.parse(payload));
 *   return true;
 * });
 * router.handle({ call: { id: 1, method: "list" } });
 * // later: sent is [{ response: { id: 1, result: { items: [] } } }]
 * ```
 */

import { toWire, unescapedLength } from "./json.js";
import {
  MAX_VIEW_MESSAGE,
  failurePayload,
  parseViewRequest,
  progressPayload,
  pushPayload,
  responsePayload,
} from "./messages.js";
import type { ViewRequest } from "./messages.js";

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

/** What a router is given to answer with. */
export interface RouterHandlers {
  /** The calls the view may make, by method. A method that is not here is answered `not-found`. */
  calls?: Readonly<Record<string, CallHandler>>;
  /** The subscriptions the view may make, by topic. A topic that is not here is answered `not-found`. */
  topics?: Readonly<Record<string, TopicHandler>>;
}

/**
 * Writes a message of the program toward the view: the payload of a `msg`, as JSON text (§4.5).
 * `false` when it was not written (the view is gone); a message over 1 MiB is not offered.
 */
export type Send = (payload: string) => boolean;

/** The program's side of one view's requests. */
export interface Router {
  /**
   * Takes a message of the view: a `call`, a `subscribe` or a `cancel`, as the object the view
   * posted or as JSON text. The view is untrusted: anything else, or anything malformed, is ignored
   * (§9.2). `true` when it was one of the three.
   */
  handle(message: string | object): boolean;
  /** The view is over: every call is cancelled, and every subscription is stopped and its cleanup run. */
  end(): void;
}

/** A subscription of the view that runs. */
interface Subscribed {
  readonly controller: AbortController;
  /** What the topic's handler returned to stop whatever feeds it. */
  cleanup: (() => void) | undefined;
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

/** Stops whatever feeds a subscription: what a handler returned. Its failure is not the view's. */
function stop(cleanup: () => void): void {
  try {
    cleanup();
  } catch {
    // The subscription is over either way.
  }
}
/** The router of a program whose handlers are `handlers`, answering through `send`. */
export function createRouter(handlers: RouterHandlers, send: Send): Router {
  const calls = new Map<number, AbortController>();
  const subs = new Map<number, Subscribed>();

  /** Writes a message toward the view; `false` when it was not (the view is gone, or it is over a message's limit). */
  const tell = (payload: string): boolean => {
    if (unescapedLength(payload) > MAX_VIEW_MESSAGE) return false;
    try {
      return send(payload);
    } catch {
      return false;
    }
  };

  /** The response that ends a call with `result`. */
  const answer = (id: number, result: unknown): void => {
    let payload: string;
    try {
      payload = responsePayload(id, result);
    } catch {
      payload = failurePayload(id, "internal", "the result cannot be written as JSON");
    }
    if (unescapedLength(payload) > MAX_VIEW_MESSAGE)
      payload = failurePayload(id, "message-too-long", "the result is over the limit of a message");
    tell(payload);
  };

  const stopSubscribed = (subscribed: Subscribed): void => {
    subscribed.controller.abort();
    const { cleanup } = subscribed;
    subscribed.cleanup = undefined;
    if (cleanup !== undefined) stop(cleanup);
  };

  const call = (request: Extract<ViewRequest, { type: "call" }>): void => {
    const { id } = request;
    // An id that is out is not used again; a view that does is not following §9.2.
    if (calls.has(id) || subs.has(id)) return;
    const handler = handlerOf(handlers.calls, request.method);
    if (handler === undefined) {
      tell(failurePayload(id, "not-found", "there is nothing to call by that name"));
      return;
    }
    const controller = new AbortController();
    calls.set(id, controller);
    const context: CallContext = {
      method: request.method,
      signal: controller.signal,
      progress: (done, total, note) => {
        if (controller.signal.aborted) return;
        if (!Number.isFinite(done) || (total !== undefined && !Number.isFinite(total))) return;
        tell(progressPayload(id, done, total, note));
      },
    };
    void (async () => {
      try {
        const result = await handler(request.params, context);
        // Cancelled, or the view is over: the answer would be ignored.
        if (controller.signal.aborted) return;
        calls.delete(id);
        if (result instanceof ViewError) tell(failureOf(id, result));
        else answer(id, result);
      } catch (error) {
        if (controller.signal.aborted) return;
        calls.delete(id);
        tell(failureOf(id, error));
      }
    })();
  };

  const subscribe = (request: Extract<ViewRequest, { type: "subscribe" }>): void => {
    const { id, topic } = request;
    if (calls.has(id) || subs.has(id)) return;
    const handler = handlerOf(handlers.topics, topic);
    if (handler === undefined) {
      tell(failurePayload(id, "not-found", "there is nothing to subscribe to by that name"));
      return;
    }
    const controller = new AbortController();
    const subscribed: Subscribed = { controller, cleanup: undefined };
    subs.set(id, subscribed);
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
        return tell(payload);
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
        subs.delete(id);
        controller.abort();
        tell(failureOf(id, error));
      }
    })();
  };

  /** The view gives up a call that is out, or ends a subscription; an id that is not out changes nothing (§9.2). */
  const cancel = (id: number): void => {
    const pending = calls.get(id);
    if (pending !== undefined) {
      calls.delete(id);
      pending.abort();
      return;
    }
    const subscribed = subs.get(id);
    if (subscribed === undefined) return;
    subs.delete(id);
    stopSubscribed(subscribed);
  };

  return {
    handle(message) {
      const request = parseViewRequest(typeof message === "string" ? message : toWire(message));
      if (request === undefined) return false;
      switch (request.type) {
        case "call":
          call(request);
          break;
        case "subscribe":
          subscribe(request);
          break;
        case "cancel":
          cancel(request.id);
          break;
      }
      return true;
    },

    end() {
      const pending = [...calls.values()];
      const subscribed = [...subs.values()];
      calls.clear();
      subs.clear();
      for (const call of pending) call.abort();
      for (const one of subscribed) stopSubscribed(one);
    },
  };
}
