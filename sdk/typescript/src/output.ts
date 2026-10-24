/**
 * An {@link Output} that writes with the file system's `write` and reports each write itself.
 *
 * Bun, for the tty's stream, never calls the callback of `write`, and the library's queue
 * goes on to the next message only when the callback of the last one has been called: the
 * `hello` goes out and the `CSI c` after it never does. A tty's output is synchronous on
 * POSIX (as {@link Link.writeNow} counts on), so the descriptor can be written to directly and
 * the write reported when it returns. The descriptor is blocking on a tty; a descriptor
 * that is not (`EAGAIN`) is waited for a moment at a time.
 */

import { writeSync } from "node:fs";

import { utf8Bytes } from "./bytes.js";
import type { Output } from "./link.js";

/** How long a write waits for a descriptor that cannot take more, before it asks again. */
const RETRY_MS = 2;

function pause(ms: number): void {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

/** The file system's `write` of `length` bytes of `bytes` from `offset`: how many it took. */
export type WriteFd = (fd: number, bytes: Uint8Array, offset: number, length: number) => number;

function writeAll(write: WriteFd, fd: number, bytes: Uint8Array): void {
  for (let done = 0; done < bytes.length;) {
    try {
      done += write(fd, bytes, done, bytes.length - done);
    } catch (error) {
      if ((error as { code?: unknown }).code !== "EAGAIN") throw error;
      pause(RETRY_MS);
    }
  }
}

/** The descriptor `fd` as an {@link Output}; `isTTY` is the stream's, which says what `fd` is. */
export function synchronousOutput(
  fd: number,
  isTTY: boolean | undefined,
  write: WriteFd = writeSync,
): Output {
  return {
    isTTY,
    write(chunk, callback) {
      let failure: Error | null = null;
      try {
        writeAll(write, fd, typeof chunk === "string" ? utf8Bytes(chunk) : chunk);
      } catch (error) {
        failure = error instanceof Error ? error : new Error(String(error));
      }
      callback?.(failure);
      return failure === null;
    },
  };
}
