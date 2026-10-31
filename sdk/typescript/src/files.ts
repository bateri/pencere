/**
 * The files beside a view's bundle (specification §8.8 to §8.10), as `serve` keeps them for the
 * view that runs: what the program has attached for the view to draw, and the reads and writes of
 * files the person gave and places the person chose that wait for the host's answer. Everything is
 * the view's and goes with it ({@link Files.forget}).
 */

import { sha256 } from "./bundle.js";
import { fromBase64, toBase64 } from "./bytes.js";
import type { Message } from "./frame.js";
import {
  attachPayload,
  detachPayload,
  fetchPayload,
  parseFetched,
  parseNeed,
  parseRefused,
  parseStaged,
  parseWrote,
  readPayload,
  stagePayload,
  unstagePayload,
} from "./messages.js";
import type { StageOffer } from "./messages.js";

/** The most bytes a `read` asks for and a `write` carries (§8.9, §8.10). */
export const MAX_PIECE = 1 << 20;

/** How many reads the host answers at once (§8.9). */
const MAX_READS = 2;

/** The scheme of the name a view draws an attached file by (§9.8). */
const ATTACHED = "sha256:";

/**
 * Why a file could not be attached, read or saved. `code` is `unsupported` when no view that has
 * files runs (none is up, or it was opened at a version without them: `lua` before 2.1, `wasm`
 * before 3.0, a page), `ended` when the view ended before the host answered, or the host's own:
 * for an attach `limit`, `view`, `size`, `digest`; for a read `handle`, `range`, `busy`, `gone`,
 * `io`; for a write `handle`, `order`, `size`, `space`, `io`; for a `stage` or a `fetch` `handle`,
 * `busy`, `gone`, `size`, `space`, `route`, `io`, `cancelled` (§8.11).
 */
export class FileError extends Error {
  readonly code: string;
  /** With `route` of a `fetch`: how long a file the host holds (§8.11). */
  readonly at?: number;

  constructor(code: string, message?: string, at?: number) {
    super(message ?? `the file: ${code}`);
    this.name = "FileError";
    this.code = code;
    if (at !== undefined) this.at = at;
  }
}

/**
 * A `stage` or a `fetch` that is out (§8.11): the file of a handle, or the program's file for the
 * place of a handle, crossing through a pipe in the host's folder.
 */
export interface Transfer {
  /** The id of the message, which an `unstage` names. */
  readonly id: number;
  /**
   * The name of the pipe in the folder, once the host has made it; rejects with the host's code
   * when it could not begin.
   */
  readonly pipe: Promise<string>;
  /**
   * How the transfer ended: the offset of the end of the file a `stage` carried (`sent`), the
   * length of the file a `fetch` saved (`saved`). Rejects with a {@link FileError} of the host's
   * code (`at` goes with a `route` of a `fetch`), or `ended` when the view ended first.
   */
  readonly done: Promise<number>;
  /**
   * Takes the transfer back: the host closes and removes the pipe, and throws away what a `fetch`
   * had written of its place, which is then as it was before the `fetch`. `done` rejects with
   * `cancelled` at once.
   */
  unstage(): void;
}

/** Whether a view of `kind` opened at `version` has files beside its bundle (§5.5). */
export function hasFiles(kind: string, version: string): boolean {
  const [major = Number.NaN, minor = 0] = version.split(".").map(Number);
  if (!Number.isInteger(major)) return false;
  if (kind === "lua") return major > 2 || (major === 2 && minor >= 1);
  if (kind === "wasm") return major >= 3;
  return false;
}

interface Waiting<T> {
  resolve(value: T): void;
  reject(error: FileError): void;
}

interface Queued extends Waiting<Uint8Array> {
  readonly handle: string;
  readonly offset: number;
  readonly length: number;
}

interface Attaching extends Waiting<void> {
  readonly files: { readonly digest: string; readonly data: Uint8Array }[];
  /** The missing files are sent: the next `need` is the last answer. */
  sent: boolean;
}

interface Saving {
  failed?: string;
  last?: Waiting<void> | undefined;
}

/** A `stage` or a `fetch` that waits for the host's answers. */
interface Moving {
  readonly fetch: boolean;
  readonly pipe: Waiting<string>;
  readonly done: Waiting<number>;
  /** The host has named the pipe. */
  named: boolean;
}

/** How a message goes to the host: its id. */
export type SendFile = (
  kind: string,
  payload: string,
  params?: { digest?: string; handle?: string; offset?: number; end?: boolean },
) => number;

export class Files {
  readonly #attached = new Set<string>();
  readonly #attaches = new Map<number, Attaching>();
  readonly #reads = new Map<number, Waiting<Uint8Array>>();
  /** Reads that wait for one of those out to be answered. */
  #queued: Queued[] = [];
  /** The handle each write that is out was for. */
  readonly #writes = new Map<number, string>();
  readonly #saves = new Map<string, Saving>();
  /** The `stage`s and `fetch`es that are out, by the id they went out with. */
  readonly #transfers = new Map<number, Moving>();
  /** The folder the host offers (§8.11). It is the host's, not the view's: it outlives the view. */
  #stage: StageOffer | undefined;

  /** The host's `welcome` says where its folder is, or that it has none. */
  offer(stage: StageOffer | undefined): void {
    this.#stage = stage;
  }

  /** Where the host says its folder is. */
  get stageOffer(): StageOffer | undefined {
    return this.#stage;
  }

  /** The view is over: what it attached is no longer, and every wait is told it ended. */
  forget(): void {
    const ended = new FileError("ended", "the view ended before the file was through");
    for (const moving of this.#transfers.values()) {
      moving.pipe.reject(ended);
      moving.done.reject(ended);
    }
    this.#transfers.clear();
    for (const waiting of this.#attaches.values()) waiting.reject(ended);
    for (const waiting of this.#reads.values()) waiting.reject(ended);
    for (const waiting of this.#queued) waiting.reject(ended);
    this.#queued = [];
    for (const saving of this.#saves.values()) saving.last?.reject(ended);
    this.#attached.clear();
    this.#attaches.clear();
    this.#reads.clear();
    this.#writes.clear();
    this.#saves.clear();
  }

  /** Whether the `need` with this `re` answers an attach. */
  answersAttach(re: string | undefined): boolean {
    return re !== undefined && this.#attaches.has(Number(re));
  }

  attach(data: Uint8Array, send: SendFile): Promise<string> {
    const digest = sha256(data);
    const name = ATTACHED + digest;
    if (this.#attached.has(digest)) return Promise.resolve(name);
    return new Promise<void>((resolve, reject) => {
      const id = send("attach", attachPayload([{ sha256: digest, size: data.length }]));
      this.#attaches.set(id, { files: [{ digest, data }], sent: false, resolve, reject });
    }).then(() => name);
  }

  detach(name: string, send: SendFile): void {
    if (!name.startsWith(ATTACHED)) return;
    const digest = name.slice(ATTACHED.length);
    if (this.#attached.delete(digest)) send("detach", detachPayload([digest]));
  }

  read(handle: string, offset: number, length: number, send: SendFile): Promise<Uint8Array> {
    return new Promise((resolve, reject) => {
      const read = { handle, offset, length: Math.min(length, MAX_PIECE), resolve, reject };
      // The host answers a third read out `busy` (§8.9): it waits for one of the two to be answered.
      if (this.#reads.size < MAX_READS) this.#sendRead(read, send);
      else this.#queued.push(read);
    });
  }

  #sendRead(read: Queued, send: SendFile): void {
    const id = send("read", readPayload(read.handle, read.offset, read.length));
    this.#reads.set(id, { resolve: read.resolve, reject: read.reject });
  }

  write(
    handle: string,
    offset: number,
    data: Uint8Array,
    end: boolean,
    send: SendFile,
  ): Promise<void> {
    let saving = this.#saves.get(handle);
    if (saving === undefined) {
      saving = {};
      this.#saves.set(handle, saving);
    }
    if (saving.failed !== undefined) return Promise.reject(new FileError(saving.failed));
    const id = send("write", toBase64(data), { handle, offset, end });
    this.#writes.set(id, handle);
    if (!end) return Promise.resolve();
    const waiting = saving;
    return new Promise((resolve, reject) => {
      waiting.last = { resolve, reject };
    });
  }

  /** Sends a `stage` for the file of `handle` from `offset` (§8.11). */
  stage(handle: string, offset: number, send: SendFile): Transfer {
    return this.#begin(false, "stage", stagePayload(handle, offset), send);
  }

  /** Sends a `fetch` of the program's file, `size` bytes long, for the place of `handle`, its bytes starting at `offset` (§8.11). */
  fetch(handle: string, size: number, offset: number, send: SendFile): Transfer {
    return this.#begin(true, "fetch", fetchPayload(handle, size, offset), send);
  }

  #begin(fetch: boolean, kind: string, payload: string, send: SendFile): Transfer {
    let pipe!: Waiting<string>;
    let done!: Waiting<number>;
    const pipePromise = new Promise<string>((resolve, reject) => (pipe = { resolve, reject }));
    const donePromise = new Promise<number>((resolve, reject) => (done = { resolve, reject }));
    // A transfer whose result nobody awaits is not an unhandled rejection: whoever does await gets it.
    pipePromise.catch(() => undefined);
    donePromise.catch(() => undefined);
    const id = send(kind, payload);
    this.#transfers.set(id, { fetch, pipe, done, named: false });
    return {
      id,
      pipe: pipePromise,
      done: donePromise,
      unstage: () => this.unstage(id, send),
    };
  }

  /** Takes a `stage` or a `fetch` back (§8.11). One that is not out is nothing to take back. */
  unstage(id: number, send: SendFile): void {
    const moving = this.#transfers.get(id);
    if (moving === undefined) return;
    this.#transfers.delete(id);
    send("unstage", unstagePayload(id));
    const cancelled = new FileError("cancelled");
    moving.pipe.reject(cancelled);
    moving.done.reject(cancelled);
  }

  /** A `staged` or a `fetched` (§8.11): the transfer is told, and the last answer ends it. */
  onTransfer(message: Message, fetch: boolean): void {
    const re = Number(message.control.re);
    const moving = this.#transfers.get(re);
    if (moving === undefined || moving.fetch !== fetch) return;
    const told = fetch ? parseFetched(message.payload) : parseStaged(message.payload);
    if (told === undefined) return;
    switch (told.kind) {
      case "pipe":
        if (moving.named) return;
        moving.named = true;
        moving.pipe.resolve(told.pipe);
        return;
      case "sent":
        this.#transfers.delete(re);
        moving.done.resolve(told.sent);
        return;
      case "saved":
        this.#transfers.delete(re);
        moving.done.resolve(told.saved);
        return;
      case "failed": {
        this.#transfers.delete(re);
        // Only the `fetched` of a `route` has `at`.
        const failure = new FileError(told.code, undefined, (told as { at?: number }).at);
        if (!moving.named) moving.pipe.reject(failure);
        moving.done.reject(failure);
      }
    }
  }

  /** A `need` that answers an attach (§8.8). */
  onNeed(message: Message, send: SendFile): void {
    const re = Number(message.control.re);
    const missing = parseNeed(message.payload);
    const refused = parseRefused(message.payload);
    const attaching = this.#attaches.get(re);
    if (missing === undefined || refused === undefined || attaching === undefined) return;
    this.#attaches.delete(re);
    const refusal = refused[0];
    if (refusal !== undefined) return attaching.reject(new FileError(refusal.code));
    if (attaching.sent || missing.length === 0) {
      for (const file of attaching.files) this.#attached.add(file.digest);
      return attaching.resolve();
    }
    for (const digest of missing) {
      const file = attaching.files.find((candidate) => candidate.digest === digest);
      if (file !== undefined) send("file", toBase64(file.data), { digest });
    }
    attaching.sent = true;
    this.#attaches.set(re, attaching);
  }

  /** A `data` that answers a read (§8.9). */
  onData(message: Message, send: SendFile): void {
    const re = Number(message.control.re);
    const waiting = this.#reads.get(re);
    if (waiting === undefined) return;
    this.#reads.delete(re);
    const next = this.#queued.shift();
    if (next !== undefined) this.#sendRead(next, send);
    const code = message.control.e;
    if (code !== undefined) return waiting.reject(new FileError(code));
    const data = fromBase64(message.payload);
    if (data === undefined) waiting.reject(new FileError("io"));
    else waiting.resolve(data);
  }

  /** A `wrote` (§8.10): the file is saved, or failed at the write it answers. */
  onWrote(message: Message): void {
    const handle = this.#writes.get(Number(message.control.re));
    const wrote = parseWrote(message.payload);
    if (handle === undefined || wrote === undefined) return;
    for (const [id, of] of this.#writes) if (of === handle) this.#writes.delete(id);
    const saving = this.#saves.get(handle);
    if (saving === undefined) return;
    const last = saving.last;
    saving.last = undefined;
    if (wrote.code === undefined) {
      this.#saves.delete(handle);
      last?.resolve();
    } else {
      saving.failed = wrote.code;
      last?.reject(new FileError(wrote.code));
    }
  }
}
