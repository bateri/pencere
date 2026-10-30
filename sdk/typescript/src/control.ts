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
 * const served = serve({ views: [{ kind: "web", bundle }], calls, control });
 * // ... later, when the files changed:
 * await control.reopen({ views: [{ kind: "web", bundle: await bundleFromDirectory(directory) }] });
 * ```
 *
 * Both calls need a host that lists `dismiss` in its `welcome`; with another the
 * view is not touched and the answer says so.
 */

import { FileError } from "./files.js";
import type { MAX_PIECE } from "./files.js";
import type { NoViewReason, ViewSpec } from "./serve.js";

/** What the view that replaces the closed one is made of. */
export interface ReopenOptions {
  /**
   * The kinds of view the program can show now, in its order, with their files, as
   * `ServeOptions.views` has them: the new `hello` lists them. Without it, the list `serve`
   * was given, or the last reopening gave, is used again: the host keeps what it verified, so
   * what did not change does not cross the stream a second time.
   */
  views?: readonly ViewSpec[];
}

/** What {@link ViewControl.reopen} came to. */
export type ReopenResult =
  /** The old view is closed and the host has accepted the open of the new one (it answered with a `need`). */
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
      readonly message?: string;
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
  attach(data: Uint8Array): Promise<string>;
  detach(name: string): void;
  read(handle: string, offset: number, length: number): Promise<Uint8Array>;
  write(handle: string, offset: number, data: Uint8Array, end: boolean): Promise<void>;
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
   * them from its store), or with `next.views` (only what changed is sent).
   * Waits as long as the host takes to tear the old view down, which the host
   * bounds. What the old view had asked of the program is over with it, as for any view.
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

  /**
   * Attaches `data` to the view that runs, for it to draw (§8.8), and resolves with the name the
   * view draws it by: `sha256:` and its digest. Resolves once the host holds the bytes; a file the
   * view has attached already, or that the host still holds, does not cross the stream again.
   * What is attached goes with the view. Rejects with a {@link FileError}: `unsupported` with no
   * view that has files (`lua` 2.1 or `wasm` 3.0 and later), the host's code when it refuses
   * (`limit`, …), `ended` when the view ended first.
   */
  attach(data: Uint8Array): Promise<string> {
    const target = targets.get(this);
    return target === undefined
      ? Promise.reject(new FileError("unsupported"))
      : target.attach(data);
  }

  /** Lets the view's attached file of `name` go: the host forgets it and the view no longer draws it. */
  detach(name: string): void {
    targets.get(this)?.detach(name);
  }

  /**
   * Reads up to `length` bytes (at most {@link MAX_PIECE}) from `offset` of the file the person
   * gave by `handle` (§8.9): fewer than were asked for is the end of the file. Rejects with the
   * host's code (`handle`, `range`, `busy`, `gone`, `io`) as for `attach`.
   */
  read(handle: string, offset: number, length: number): Promise<Uint8Array> {
    const target = targets.get(this);
    return target === undefined
      ? Promise.reject(new FileError("unsupported"))
      : target.read(handle, offset, length);
  }

  /**
   * Writes `data` (at most {@link MAX_PIECE}) at `offset` of the place the person chose to save in
   * by `handle` (§8.10); `end` when they are the file's last, which resolves once the host has
   * saved it. The writes of a handle go in order with no gap; one that is not the last resolves at
   * once, and a failure the host told of rejects the next. Rejects with the host's code
   * (`handle`, `order`, `size`, `space`, `io`) as for `attach`.
   */
  write(handle: string, offset: number, data: Uint8Array, end = false): Promise<void> {
    const target = targets.get(this);
    return target === undefined
      ? Promise.reject(new FileError("unsupported"))
      : target.write(handle, offset, data, end);
  }
}
