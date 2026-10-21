/**
 * `ViewControl`: how the program that runs `serve` closes the view it shows, or
 * shows another in its place, without ending (specification §10.8).
 *
 * `serve` resolves when the view has closed, which is what a program that is
 * only a view wants. A program that goes on after its view — a tool that shows
 * a text interface for a while, or one whose author reloads the view as the
 * files change — makes a control, hands it to `serve` and calls it while
 * `serve` runs:
 *
 * ```ts
 * const control = new ViewControl();
 * const served = serve(server, { tool: "list_containers", bundle, control });
 * // ... later, when the files changed:
 * await control.reopen({ bundle: await bundleFromDirectory(directory) });
 * ```
 *
 * Both calls need a host that lists `dismiss` in its `welcome`; with another the
 * view is not touched and the answer says so.
 */

import type { Bundle } from "./bundle.js";
import type { ToolCall } from "./messages.js";
import type { NoViewReason } from "./serve.js";

/** What the view that replaces the closed one is made of. */
export interface ReopenOptions {
  /**
   * The files of the new view. Without it the files `serve` was given, or the
   * last reopening gave, are used again: the host keeps what it verified, so
   * what did not change does not cross the stream a second time.
   */
  bundle?: Bundle;
  /** The tool whose view to show. Without it, the tool of the view that closes. */
  tool?: string | ToolCall;
}

/** What {@link ViewControl.reopen} came to. */
export type ReopenResult =
  /** The old view is closed and the new one runs: the host has called its tool. */
  | { readonly kind: "reopened" }
  /** The host does not list `dismiss`. Nothing was sent and the view is as it was. */
  | { readonly kind: "unsupported" }
  /**
   * There is no view to close: `serve` has not shown one yet, or has ended, or the
   * view is already being closed or replaced; or the view was closed by someone else
   * (the person, the view) while this was under way. Nothing was sent, or what was
   * sent no longer matters.
   */
  | { readonly kind: "unavailable" }
  /**
   * The old view closed and no new one could be opened: the host did not answer,
   * refused or ended the open before the view ran. `serve` ends the same way and
   * runs its `fallback`, as it does when the first open fails.
   */
  | {
      readonly kind: "no-view";
      readonly reason: NoViewReason;
      readonly code?: string;
    };

/** What {@link ViewControl.close} came to. */
export type CloseResult =
  /** The view is closed; `serve` resolves with `closed` by `program`. */
  | { readonly kind: "closed" }
  /** The host does not list `dismiss`. Nothing was sent and the view is as it was. */
  | { readonly kind: "unsupported" }
  /** As for {@link ReopenResult}: there is no view to close. */
  | { readonly kind: "unavailable" };

/** What a running `serve` answers to a control; `serve` is the only one that makes it. */
export interface ViewTarget {
  reopen(next: ReopenOptions): Promise<ReopenResult>;
  close(): Promise<CloseResult>;
}

const targets = new WeakMap<ViewControl, ViewTarget>();

/** `serve` ties a control to the session it runs. A control belongs to one `serve` at a time. */
export function bindControl(control: ViewControl, target: ViewTarget): void {
  targets.set(control, target);
}

/** `serve` lets go of a control when it ends; one another `serve` took meanwhile stays. */
export function unbindControl(control: ViewControl, target: ViewTarget): void {
  if (targets.get(control) === target) targets.delete(control);
}

/**
 * The program's hand on the view `serve` shows. Pass it as `ServeOptions.control`;
 * until `serve` has a view, and once it has ended, its calls answer `unavailable`.
 */
export class ViewControl {
  /**
   * Closes the view and opens one again: with the same files (the host serves
   * them from its store), or with `next.bundle` (only what changed is sent).
   * Waits as long as the host takes to tear the old view down, which the host
   * bounds. The program's MCP server is connected afresh, as for any view.
   */
  reopen(next: ReopenOptions = {}): Promise<ReopenResult> {
    const target = targets.get(this);
    return target === undefined ? Promise.resolve({ kind: "unavailable" }) : target.reopen(next);
  }

  /**
   * Closes the view and ends `serve` with `closed` by `program`, giving the terminal
   * back first, so that the program can show its own interface.
   */
  close(): Promise<CloseResult> {
    const target = targets.get(this);
    return target === undefined ? Promise.resolve({ kind: "unavailable" }) : target.close();
  }
}
