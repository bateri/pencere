/**
 * `pencere dev`: what a change to the files comes to.
 *
 * `pencere run` shows a view once. `pencere dev` watches the files of the view and
 * the files the server reads, and when they change it has the SDK close the view
 * and open it again (`ViewControl.reopen`, specification §10.8): the old view and
 * the server of its session are gone, the server is started again as for any view,
 * and the new view's files are checked against what the terminal already holds,
 * so only the files that changed cross the stream. Nothing of that is this file's:
 * what is, is deciding *when*.
 *
 * - **A burst is one reopening.** A build writes many files; the change is acted
 *   on once the files have been quiet for a moment (`settle`), and a change that
 *   comes while a reopening is under way is acted on after it, again after a
 *   moment of quiet.
 * - **Only what changed.** A change to the view's directory is a reopening when
 *   the files it holds are different from what the view shows (a file saved as it
 *   was, or one that is not part of a view, changes nothing). A change to
 *   `--watch`'s paths is a reopening when what the paths hold has moved (their
 *   size or modification time): the events of one write that arrive late are not
 *   a second write.
 * - **A half-built view waits.** A build that has emptied its output directory
 *   leaves something that is not a view; that is not shown, and the next change,
 *   which is the build writing more, is looked at in its turn.
 * - **A view that is not there yet waits too.** A change that comes before the
 *   first view is up is asked again a few times, a moment apart.
 *
 * What the person gets to know is said after the view has closed (`notes`): the
 * screen is the view's while it is up.
 */

import { BundleError, bundleFromDirectory } from "@pencere/sdk";
import type { Bundle, ReopenOptions, ReopenResult } from "@pencere/sdk";

/** How long `Reloader` waits, and how often it asks again. */
export interface ReloadTiming {
  /** The quiet, in milliseconds, after which the changes so far are acted on. */
  readonly settle: number;
  /** How long to wait, in milliseconds, before asking again for a view that is not up yet. */
  readonly retry: number;
  /** How many times to ask again before leaving the change for the next one to carry. */
  readonly tries: number;
}

/** What `Reloader` needs of the program's hand on its view: `ViewControl`'s `reopen`. */
export interface Reopener {
  reopen(next: ReopenOptions): Promise<ReopenResult>;
}

export interface ReloaderOptions {
  readonly control: Reopener;
  /** The directory the view's files are in (`--view`), absent when the terminal reads the server's own resource. */
  readonly view?: string;
  /** What the view was first opened with. */
  readonly bundle?: Bundle;
  /** What the server's own files (`--watch`) hold, in a form that changes when they do. Without it every change to them counts. */
  readonly fingerprint?: () => Promise<string>;
  readonly timing: ReloadTiming;
  /** Reads the view's files. `bundleFromDirectory`, unless a test lends another. */
  readonly load?: (directory: string) => Promise<Bundle>;
}

/** What has changed since the last reopening. */
interface Change {
  /** Something under the view's directory. */
  readonly view: boolean;
  /** Something of the server's own files. */
  readonly server: boolean;
}

function merged(a: Change | undefined, b: Change): Change {
  return a === undefined ? b : { view: a.view || b.view, server: a.server || b.server };
}

/** Whether `bundle` holds the files `shown` does, byte for byte, under the same entry. */
function sameFiles(bundle: Bundle, shown: Bundle | undefined): boolean {
  return shown !== undefined && JSON.stringify(bundle.manifest) === JSON.stringify(shown.manifest);
}

function why(error: unknown): string {
  if (error instanceof BundleError) return error.message;
  const code = (error as NodeJS.ErrnoException | undefined)?.code;
  if (code === "ENOENT") return "a file went away while it was read";
  return error instanceof Error ? error.message : String(error);
}

/** Closes the view and opens it again when what it is made of, or the server, changed. */
export class Reloader {
  readonly #options: ReloaderOptions;
  readonly #notes: string[] = [];
  readonly #waiting: (() => void)[] = [];
  /** The files the view shows now. */
  #shown: Bundle | undefined;
  /** What the server's files held when the view was last opened. */
  #serverPrint: string | undefined;
  /** Changes to act on once the files are quiet. */
  #pending: Change | undefined;
  /** Changes that could not be acted on, which the next change carries along. */
  #held: Change | undefined;
  #timer: ReturnType<typeof setTimeout> | undefined;
  #running: Promise<void> | undefined;
  #stopped = false;
  #tries = 0;
  /** Why the view's files could not be read at the last change, while that is so. */
  #problem: string | undefined;
  #reopened = 0;

  constructor(options: ReloaderOptions) {
    this.#options = options;
    this.#shown = options.bundle;
  }

  /** Notes what the server's files hold now, as the view that is about to open sees them. Call before watching. */
  async start(): Promise<void> {
    this.#serverPrint = await this.#options.fingerprint?.();
  }

  /** How many times the view has been reopened. */
  get reopened(): number {
    return this.#reopened;
  }

  /** What to tell the person once the view has closed. */
  get notes(): readonly string[] {
    const notes = [...this.#notes];
    if (this.#problem !== undefined)
      notes.push(
        `the last change to the view's files could not be shown (${this.#problem}); the view is as the change before it left it`,
      );
    else if (this.#held !== undefined)
      notes.push("the last change was not shown, because there was no view to show it in");
    return notes;
  }

  /** Something under the view's directory changed. */
  viewChanged(): void {
    this.#note({ view: true, server: false });
  }

  /** Something of the server's own files changed. */
  serverChanged(): void {
    this.#note({ view: false, server: true });
  }

  /** Something to tell the person afterwards that is not a change: a watcher that failed. */
  warn(text: string): void {
    this.#notes.push(text);
  }

  /** No more reopenings: the view has ended, or cannot be reopened. */
  stop(): void {
    this.#stopped = true;
    this.#disarm();
    this.#pending = undefined;
    this.#release();
  }

  /** Resolves when nothing is waiting to be acted on and nothing is being acted on. */
  settled(): Promise<void> {
    if (this.#idle()) return Promise.resolve();
    return new Promise((resolve) => this.#waiting.push(resolve));
  }

  #idle(): boolean {
    return this.#timer === undefined && this.#running === undefined && this.#pending === undefined;
  }

  #release(): void {
    if (!this.#idle()) return;
    for (const resolve of this.#waiting.splice(0)) resolve();
  }

  #note(change: Change): void {
    if (this.#stopped) return;
    // What could not be acted on before is carried along with this one.
    const carried = merged(this.#pending, change);
    this.#pending = this.#held === undefined ? carried : merged(carried, this.#held);
    this.#held = undefined;
    this.#tries = 0;
    this.#arm(this.#options.timing.settle);
  }

  #arm(delay: number): void {
    this.#disarm();
    this.#timer = setTimeout(() => {
      this.#timer = undefined;
      this.#kick();
    }, delay);
  }

  #disarm(): void {
    if (this.#timer !== undefined) clearTimeout(this.#timer);
    this.#timer = undefined;
  }

  #kick(): void {
    if (this.#running !== undefined || this.#stopped) return;
    this.#running = this.#pass().finally(() => {
      this.#running = undefined;
      // What came while this ran has had its moment of quiet only if nothing came since.
      if (!this.#stopped && this.#pending !== undefined && this.#timer === undefined)
        this.#arm(this.#options.timing.settle);
      this.#release();
    });
  }

  async #pass(): Promise<void> {
    const change = this.#pending;
    this.#pending = undefined;
    if (change === undefined) return;
    try {
      await this.#act(change);
    } catch (error) {
      // Looking at the files must not end the view the person is looking at.
      this.#notes.push(`could not look at the files: ${why(error)}`);
    }
  }

  async #act(change: Change): Promise<void> {
    const { view, fingerprint } = this.#options;

    let serverPrint: string | undefined;
    let serverMoved = false;
    if (change.server) {
      serverPrint = await fingerprint?.();
      serverMoved = serverPrint === undefined || serverPrint !== this.#serverPrint;
    }

    let bundle: Bundle | undefined;
    let viewMoved = false;
    if (change.view && view !== undefined) {
      try {
        bundle = await (this.#options.load ?? bundleFromDirectory)(view);
      } catch (error) {
        // A build that is half done: the next change is the rest of it.
        this.#problem = why(error);
        this.#held = merged(this.#held, change);
        return;
      }
      this.#problem = undefined;
      viewMoved = !sameFiles(bundle, this.#shown);
    }
    if (!viewMoved && !serverMoved) return;

    const result = await this.#options.control.reopen(
      viewMoved && bundle !== undefined ? { bundle } : {},
    );
    switch (result.kind) {
      case "reopened":
        this.#reopened += 1;
        if (viewMoved) this.#shown = bundle;
        if (serverMoved) this.#serverPrint = serverPrint;
        return;
      case "unsupported":
        this.#notes.push(
          "this terminal does not reopen a view when asked, so `pencere dev` ran the view as `pencere run` does and the changes were not shown: close it and run it again to see them",
        );
        this.stop();
        return;
      case "no-view":
        // The new view was refused or could not start: `serve` says why as it ends.
        this.stop();
        return;
      case "unavailable": {
        const { retry, tries } = this.#options.timing;
        if (this.#tries < tries) {
          this.#tries += 1;
          this.#pending = merged(this.#pending, change);
          this.#arm(retry);
        } else {
          this.#held = merged(this.#held, change);
        }
        return;
      }
    }
  }
}
