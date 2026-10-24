/**
 * The program's end of the stream: the bytes the host writes into the tty's
 * input are read, frame by frame, from one reader; the bytes the program writes
 * are framed and queued in order, each message whole.
 */

import { Ids, StreamDecoder, encode } from "./frame.js";
import type { DecodeError, DecodeEvent, Message, Outgoing, Split } from "./frame.js";

/** What the program reads: its stdin. */
export interface Input {
  readonly isTTY?: boolean | undefined;
  on(event: "data", listener: (chunk: Uint8Array | string) => void): unknown;
  removeListener(event: "data", listener: (chunk: Uint8Array | string) => void): unknown;
  resume(): unknown;
  pause(): unknown;
}

/** What the program writes: its stdout. */
export interface Output {
  readonly isTTY?: boolean | undefined;
  write(chunk: Uint8Array | string, callback?: (error?: Error | null) => void): unknown;
  on?(event: "error", listener: (error: Error) => void): unknown;
  removeListener?(event: "error", listener: (error: Error) => void): unknown;
}

/**
 * Finds the terminal's answer to `CSI c` in what is not a frame: `ESC [ ? ... c`
 * with digits and semicolons between (specification §5.3).
 */
export class ReplyScanner {
  /** 0 nothing, 1 after ESC, 2 after ESC [, 3 inside the parameters. */
  #state = 0;

  /** Takes the next byte; `true` when it ends a reply. */
  feed(byte: number): boolean {
    switch (this.#state) {
      case 1:
        if (byte === 0x5b) {
          this.#state = 2;
          return false;
        }
        break;
      case 2:
        if (byte === 0x3f) {
          this.#state = 3;
          return false;
        }
        break;
      case 3:
        if ((byte >= 0x30 && byte <= 0x39) || byte === 0x3b) return false;
        if (byte === 0x63) {
          this.#state = 0;
          return true;
        }
        break;
    }
    this.#state = byte === 0x1b ? 1 : 0;
    return false;
  }
}

/** What the link reports. */
export interface LinkHandlers {
  /** A message of the host, whole. */
  message(message: Message): void;
  /** The terminal answered `CSI c`. */
  reply(): void;
  /** A frame or a message of the host was dropped (§4.7). */
  dropped?(error: DecodeError): void;
}

/** A message that was queued: its id, and the moment it has been written. */
export interface Sent {
  readonly id: number;
  readonly written: Promise<void>;
}

/** One reader and one ordered writer on the program's tty. */
export class Link {
  readonly #input: Input;
  readonly #output: Output;
  readonly #handlers: LinkHandlers;
  readonly #ids = new Ids();
  readonly #scanner = new ReplyScanner();
  readonly #decoder: StreamDecoder;
  #queue: Promise<void> = Promise.resolve();
  #running = false;
  #stopped = false;
  #muted = false;
  #failure: Error | undefined;

  readonly #onData = (chunk: Uint8Array | string): void => {
    if (!this.#running) return;
    const bytes = typeof chunk === "string" ? Buffer.from(chunk, "latin1") : chunk;
    this.#decoder.feed(bytes, (event) => this.#event(event));
  };

  readonly #onError = (error: Error): void => {
    this.#failure = error;
  };

  constructor(input: Input, output: Output, handlers: LinkHandlers) {
    this.#input = input;
    this.#output = output;
    this.#handlers = handlers;
    this.#decoder = new StreamDecoder("to-program", {
      noise: (byte) => {
        if (this.#running && this.#scanner.feed(byte)) this.#handlers.reply();
      },
    });
  }

  #event(event: DecodeEvent): void {
    if (!this.#running) return;
    if (event.type === "message") this.#handlers.message(event.message);
    else this.#handlers.dropped?.(event.error);
  }

  /** The error the output last reported, if any: a tty that is gone. */
  get failure(): Error | undefined {
    return this.#failure;
  }

  /** Starts reading the input and watching the output. */
  start(): void {
    if (this.#running) return;
    this.#running = true;
    this.#output.on?.("error", this.#onError);
    this.#input.on("data", this.#onData);
    this.#input.resume();
  }

  /**
   * Stops reading: nothing more is delivered, and the input is left paused. What
   * was queued and has not begun to be written is dropped: a program that stops
   * is giving the terminal back, and a frame written after that would land on the
   * screen it was just given.
   */
  stop(): void {
    this.#stopped = true;
    if (!this.#running) return;
    this.#running = false;
    this.#input.removeListener("data", this.#onData);
    this.#input.pause();
    this.#output.removeListener?.("error", this.#onError);
  }

  /**
   * Stops writing what is queued, and anything queued from now on, while reading goes on: the
   * program has ended its view and waits for the host's last word (§10.3). {@link Link.sendNow} and
   * {@link Link.writeNow} still write.
   */
  mute(): void {
    this.#muted = true;
  }

  /**
   * Queues a message toward the host. Its frames are written together, and after
   * everything queued before it.
   *
   * @throws {WriteError} for a message that does not follow the wire; nothing is queued.
   */
  send(
    kind: string,
    payload: string,
    params: { re?: number; digest?: string; split?: Split } = {},
  ): Sent {
    const id = this.#ids.next();
    const message: Outgoing = {
      kind,
      id,
      payload,
      ...(params.re === undefined ? {} : { re: params.re }),
      ...(params.digest === undefined ? {} : { digest: params.digest }),
    };
    const bytes = encode("to-host", message, params.split ?? "fill");
    return { id, written: this.#write(bytes) };
  }

  /** Queues bytes that are not a frame (an escape sequence), after everything queued before. */
  raw(text: string): Promise<void> {
    return this.#write(Buffer.from(text, "latin1"));
  }

  /**
   * Writes now, ahead of what is queued and without waiting for the write to end:
   * for a program that is leaving and will not run another turn. A tty's output
   * is synchronous on POSIX, so what is written here is written when this returns.
   */
  writeNow(text: string): void {
    this.#output.write(Buffer.from(text, "latin1"), (error) => {
      if (error) this.#failure = error;
    });
  }

  /**
   * Writes a message now, like {@link Link.writeNow}: the `bye` of a program that
   * is leaving. It has the next id, as any message has, and the id is returned.
   */
  sendNow(kind: string, payload: string): number {
    const id = this.#ids.next();
    const bytes = encode("to-host", { kind, id, payload });
    this.#output.write(bytes, (error) => {
      if (error) this.#failure = error;
    });
    return id;
  }

  /** Resolves when everything queued so far has been written. */
  flush(): Promise<void> {
    return this.#queue;
  }

  #write(bytes: Uint8Array): Promise<void> {
    const written = this.#queue.then(
      () =>
        new Promise<void>((resolve) => {
          if (this.#stopped || this.#muted) {
            resolve();
            return;
          }
          // A write to a stream that is gone may throw as well as report: neither
          // is thrown at the program, and the queue goes on to its end.
          try {
            this.#output.write(bytes, (error) => {
              if (error) this.#failure = error;
              resolve();
            });
          } catch (error) {
            this.#failure = error instanceof Error ? error : new Error(String(error));
            resolve();
          }
        }),
    );
    this.#queue = written;
    return written;
  }
}
