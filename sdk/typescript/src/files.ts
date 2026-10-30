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
  parseNeed,
  parseRefused,
  parseWrote,
  readPayload,
} from "./messages.js";

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
 * `io`; for a write `handle`, `order`, `size`, `space`, `io`.
 */
export class FileError extends Error {
  readonly code: string;

  constructor(code: string, message?: string) {
    super(message ?? `the file: ${code}`);
    this.name = "FileError";
    this.code = code;
  }
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

  /** The view is over: what it attached is no longer, and every wait is told it ended. */
  forget(): void {
    const ended = new FileError("ended", "the view ended before the file was through");
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
