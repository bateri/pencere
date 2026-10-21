/**
 * The end of what a server wrote to its stderr.
 *
 * While a view is up the terminal's screen is the view's, so what the server says
 * on stderr would be lost under the alternate screen. It is kept here instead,
 * a bounded amount, the end of it (the cause of a failure is the last thing a
 * server says), and shown on the screen the person gets back, if the server failed.
 */

import { StringDecoder } from "node:string_decoder";

/** How much is kept: enough for a stack trace, not for a server that never stops talking. */
export const TAIL_LIMIT = 16 * 1024;

export class Tail {
  readonly #limit: number;
  readonly #decoder = new StringDecoder("utf8");
  #text = "";
  #dropped = false;

  constructor(limit: number = TAIL_LIMIT) {
    this.#limit = limit;
  }

  /** Adds what the server wrote; a character cut between two chunks is kept whole. */
  push(chunk: Uint8Array | string): void {
    this.#text += typeof chunk === "string" ? chunk : this.#decoder.write(Buffer.from(chunk));
    if (this.#text.length <= this.#limit) return;
    this.#dropped = true;
    const cut = this.#text.length - this.#limit;
    // Whole lines: a line that begins in what was dropped is not worth showing half of.
    const newline = this.#text.indexOf("\n", cut);
    const whole = newline === -1 ? "" : this.#text.slice(newline + 1);
    // A single line longer than the limit has no whole line to keep: its end is.
    this.#text = whole === "" ? this.#text.slice(cut) : whole;
  }

  /** What is kept, as the person is shown it: text, not terminal commands (see {@link plain}). */
  text(): string {
    return (this.#dropped ? "[earlier output left out]\n" : "") + plain(this.#text);
  }

  get empty(): boolean {
    return this.#text === "";
  }

  /** Forgets what is kept: it has been shown, and what comes next is about the next run of the server. */
  clear(): void {
    this.#text = "";
    this.#dropped = false;
  }
}

/**
 * Text a server wrote, made safe to put on the person's terminal: what is not a
 * character to read (an escape, which could be a command to the terminal, among
 * them) is written out as `\u001b`, and a carriage return is a line end. Line ends and
 * tabs stay.
 */
export function plain(text: string): string {
  return text.replace(/\r\n?/g, "\n").replace(
    // C0 other than tab and line feed, DEL and C1.
    /[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/g,
    (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`,
  );
}
