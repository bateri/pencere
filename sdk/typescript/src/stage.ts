/**
 * The folder a host keeps on the program's machine (specification §8.11), as the program proves it
 * and opens the pipes in it. A `welcome`'s `stage` is an offer, and a path alone proves nothing: a
 * remote shell inside a remote shell, or a container, may show another folder, of another account,
 * at the same path.
 */

import { closeSync, constants, fstatSync, lstatSync, openSync, readSync } from "node:fs";

import { isPipeName } from "./messages.js";
import type { StageOffer } from "./messages.js";

/** The file in the folder that holds the token. */
const MARKER = "marker";

/** Why a pipe could not be opened: it is not the host's, or it is not there (yet). */
export class PipeError extends Error {
  /** The system's code (`ENXIO` for a pipe nobody has opened for reading, `ENOENT` for one that is not there), if it was the system's. */
  readonly errno: string | undefined;

  constructor(message: string, errno?: string) {
    super(message);
    this.name = "PipeError";
    this.errno = errno;
  }
}

/** A folder that is the host's, proved. */
export class StageFolder {
  readonly #dir: string;
  readonly #euid: number;
  readonly #dev: number;
  readonly #ino: number;

  /** `dev` and `ino` are those of the directory the proof looked at. */
  constructor(dir: string, euid: number, dev: number, ino: number) {
    this.#dir = dir;
    this.#euid = euid;
    this.#dev = dev;
    this.#ino = ino;
  }

  /** The folder's path. */
  get dir(): string {
    return this.#dir;
  }

  /**
   * Opens the pipe the host named, for reading or for writing, and returns the descriptor, which
   * the caller closes. The name must be 32 lowercase hex digits; the pipe is opened without
   * following a link and without blocking, and is used only if it is a FIFO of the program's own
   * user. A pipe nobody has opened for reading cannot be opened for writing: a {@link PipeError}
   * with `errno` `ENXIO`, and the caller tries again for a bounded time (§8.11). A read of the
   * descriptor that has nothing to give, or no writer yet, ends with `EAGAIN` or reads `0`; the
   * file is complete when the host says so and the bytes are all there, not when a read says `0`.
   * Node has no way to open a file relative to a directory, so the folder is checked to be the
   * directory the proof saw just before the pipe is opened; the pipe is checked after.
   */
  openPipe(name: string, mode: "read" | "write"): number {
    if (!isPipeName(name)) throw new PipeError("not a name the host gives a pipe");
    if (!this.#unchanged()) throw new PipeError("the folder is no longer the one proved");
    const access = mode === "read" ? constants.O_RDONLY : constants.O_WRONLY;
    let fd: number;
    try {
      fd = openSync(`${this.#dir}/${name}`, access | constants.O_NONBLOCK | constants.O_NOFOLLOW);
    } catch (error) {
      const code = (error as { code?: unknown }).code;
      throw new PipeError("the pipe cannot be opened", typeof code === "string" ? code : undefined);
    }
    try {
      const stat = fstatSync(fd);
      if (!stat.isFIFO() || stat.uid !== this.#euid) {
        throw new PipeError("not a pipe of the program's user");
      }
    } catch (error) {
      closeSync(fd);
      throw error;
    }
    return fd;
  }

  /** Whether the path still names the directory the proof saw, and that directory is still closed to the group and others. */
  #unchanged(): boolean {
    try {
      const now = lstatSync(this.#dir);
      return (
        now.isDirectory() &&
        now.dev === this.#dev &&
        now.ino === this.#ino &&
        now.uid === this.#euid &&
        (now.mode & 0o077) === 0
      );
    } catch {
      return false;
    }
  }
}

/**
 * Proves that the folder a `welcome` offers is the host's: an absolute path; a directory that
 * belongs to the effective user of the program and that the group and others cannot use; a
 * `marker` in it that is a regular file, opened without following a link, whose content is exactly
 * the token. The folder itself is not followed if it is a link. `undefined` when anything fails: the
 * program then uses messages (§8.11).
 */
export function proveStage(offer: StageOffer): StageFolder | undefined {
  const euid = process.geteuid?.();
  if (euid === undefined || !offer.dir.startsWith("/")) return undefined;
  try {
    const dir = lstatSync(offer.dir);
    if (!dir.isDirectory() || dir.uid !== euid || (dir.mode & 0o077) !== 0) return undefined;
    const fd = openSync(
      `${offer.dir}/${MARKER}`,
      constants.O_RDONLY | constants.O_NONBLOCK | constants.O_NOFOLLOW,
    );
    try {
      if (!fstatSync(fd).isFile()) return undefined;
      // One byte more than the token, so that a marker that holds more is not mistaken for it.
      const held = new Uint8Array(offer.token.length + 1);
      let length = 0;
      for (;;) {
        const read = readSync(fd, held, length, held.length - length, null);
        if (read === 0) break;
        length += read;
        if (length === held.length) break;
      }
      if (length !== offer.token.length) return undefined;
      for (let i = 0; i < length; i += 1) {
        if (held[i] !== offer.token.charCodeAt(i)) return undefined;
      }
    } finally {
      closeSync(fd);
    }
    return new StageFolder(offer.dir, euid, dir.dev, dir.ino);
  } catch {
    return undefined;
  }
}
