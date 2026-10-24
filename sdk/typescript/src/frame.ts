/**
 * Frames: the APC strings of the wire (specification §4), read and written.
 *
 * A frame is `ESC _ pencere; kind ; control ; payload ESC \`, and everything
 * between the two escapes is printable ASCII. A message is one or more frames
 * (chunks) with the same kind and id; a `message` event is what reading a
 * message's last chunk delivers.
 *
 * Both directions are implemented, with the limits of each, because the
 * fixtures exercise both: a program reads `to-program` and writes `to-host`.
 * Everything here works on bytes. A stream is never decoded as text: a byte
 * that is not printable ASCII ends a frame, and 0x9C inside a UTF-8 sequence
 * (the fixture `c1-st`) is one of them.
 */

/** What starts the body of every frame of this protocol. */
const PREFIX = "pencere;";

const ESC = 0x1b;
const BACKSLASH = 0x5c;
const UNDERSCORE = 0x5f;

/** `ESC _` before the body and `ESC \` after it. */
const FRAME_CLOSURE = 4;
/** What a frame has besides kind, control text and payload: `ESC _`, `pencere;`, two `;`, `ESC \`. */
const FRAME_FIXED = 2 + PREFIX.length + 2 + 2;

/** The two directions of the wire (specification §2). */
export type Direction = "to-host" | "to-program";

/** The limits of one direction (specification §4.6). */
export interface Limits {
  /** The most bytes a frame may have, `ESC _` and `ESC \` included. */
  readonly frame: number;
  /** The most bytes a message's payload may have. */
  readonly message: number;
}

/**
 * The limits of each direction. A message of a view is at most 1 MiB as the view said it, and the
 * escapes of the wire (§4.5) make a character up to six times as long, so six times that is what
 * always fits toward the program.
 */
export const LIMITS: Readonly<Record<Direction, Limits>> = {
  "to-host": { frame: 4096, message: 16 * 1024 * 1024 },
  "to-program": { frame: 511, message: 6 * 1024 * 1024 },
};

/**
 * Why a frame or a message was dropped (specification §4.7). Reporting these is
 * for diagnostics only: a drop has no effect beyond ending the message it hit.
 */
export type DecodeError =
  "malformed" | "too-long" | "chunk-order" | "abandoned" | "message-too-long";

/**
 * A complete message: its kind, its id, the parameters of its first chunk
 * besides `id`, `seq` and `more` (in the order they were written), and its
 * payload.
 */
export interface Message {
  /** `hello`, `open`, `msg`, ... Decoders deliver every well-formed message whatever its kind. */
  readonly kind: string;
  readonly id: number;
  /** `re`, `d` and any key this version does not know. */
  readonly control: Readonly<Record<string, string>>;
  /** The payloads of the chunks, concatenated. Printable ASCII. */
  readonly payload: string;
}

/** What a decoder delivers. */
export type DecodeEvent =
  | { readonly type: "message"; readonly message: Message }
  | { readonly type: "error"; readonly error: DecodeError };

// ---- one frame -------------------------------------------------------------

interface Chunk {
  kind: string;
  id: number;
  seq: number;
  more: boolean;
  /** Every parameter besides `id`, `seq` and `more`, in the order written. */
  control: Record<string, string>;
  payload: string;
}

const KIND = /^[a-z-]{1,16}$/;
const KEY = /^[a-z]{1,8}$/;
const VALUE = /^[A-Za-z0-9._-]{1,64}$/;
const DIGEST = /^[0-9a-f]{64}$/;

/** A decimal number as the wire writes it: digits, no leading zero (but `0`), at most 4294967295. */
function decimal(text: string): number | undefined {
  if (!/^(0|[1-9][0-9]{0,9})$/.test(text)) return undefined;
  const value = Number(text);
  return value <= 0xffff_ffff ? value : undefined;
}

/**
 * Parses the body of a frame (everything between `ESC _` and `ESC \`, `pencere;`
 * included) with the grammar of §4.1 and the parameters of §4.3. Every chunk
 * is held to the table of §4.3, whether or not its parameters are read.
 */
function parseChunk(body: string): Chunk | undefined {
  if (!body.startsWith(PREFIX)) return undefined;
  const afterPrefix = body.slice(PREFIX.length);
  const firstSemicolon = afterPrefix.indexOf(";");
  if (firstSemicolon < 0) return undefined;
  const kind = afterPrefix.slice(0, firstSemicolon);
  const afterKind = afterPrefix.slice(firstSemicolon + 1);
  const secondSemicolon = afterKind.indexOf(";");
  if (secondSemicolon < 0) return undefined;
  const controlText = afterKind.slice(0, secondSemicolon);
  const payload = afterKind.slice(secondSemicolon + 1);
  if (!KIND.test(kind)) return undefined;

  let id: number | undefined;
  let seq: number | undefined;
  let more: boolean | undefined;
  const control: Record<string, string> = {};
  const seen = new Set<string>();
  if (controlText !== "") {
    for (const pair of controlText.split(",")) {
      const equals = pair.indexOf("=");
      if (equals < 0) return undefined;
      const key = pair.slice(0, equals);
      const value = pair.slice(equals + 1);
      if (!KEY.test(key) || !VALUE.test(value) || seen.has(key)) return undefined;
      seen.add(key);
      switch (key) {
        case "id": {
          const n = decimal(value);
          if (n === undefined || n < 1) return undefined;
          id = n;
          break;
        }
        case "seq": {
          const n = decimal(value);
          if (n === undefined) return undefined;
          seq = n;
          break;
        }
        case "more":
          if (value !== "0" && value !== "1") return undefined;
          more = value === "1";
          break;
        case "re": {
          const n = decimal(value);
          if (n === undefined || n < 1) return undefined;
          control[key] = value;
          break;
        }
        case "d":
          if (!DIGEST.test(value)) return undefined;
          control[key] = value;
          break;
        default:
          control[key] = value;
      }
    }
  }
  if (id === undefined) return undefined;
  return { kind, id, seq: seq ?? 0, more: more ?? false, control, payload };
}

// ---- messages out of chunks ------------------------------------------------

interface OpenMessage {
  kind: string;
  id: number;
  /** The `seq` the next chunk must have. */
  next: number;
  control: Record<string, string>;
  payload: string;
}

/** Puts chunks together (specification §4.4). */
class Assembler {
  #open: OpenMessage | undefined;
  readonly #messageLimit: number;

  constructor(messageLimit: number) {
    this.#messageLimit = messageLimit;
  }

  chunk(chunk: Chunk, emit: (event: DecodeEvent) => void): void {
    if (chunk.seq === 0) {
      if (this.#open !== undefined) emit({ type: "error", error: "abandoned" });
      this.#open = {
        kind: chunk.kind,
        id: chunk.id,
        next: 0,
        control: chunk.control,
        payload: "",
      };
    } else {
      const open = this.#open;
      if (
        open === undefined ||
        open.kind !== chunk.kind ||
        open.id !== chunk.id ||
        open.next !== chunk.seq
      ) {
        this.#open = undefined;
        emit({ type: "error", error: "chunk-order" });
        return;
      }
    }
    const open = this.#open;
    if (open === undefined) return;
    open.next += 1;
    open.payload += chunk.payload;
    if (open.payload.length > this.#messageLimit) {
      this.#open = undefined;
      emit({ type: "error", error: "message-too-long" });
      return;
    }
    if (!chunk.more) {
      this.#open = undefined;
      emit({
        type: "message",
        message: { kind: open.kind, id: open.id, control: open.control, payload: open.payload },
      });
    }
  }
}

// ---- the stream decoder ----------------------------------------------------

/** Where the decoder is between bytes. */
const Scan = { Ground: 0, Esc: 1, Body: 2, BodyEsc: 3 } as const;
type Scan = (typeof Scan)[keyof typeof Scan];

/** What a decoder is told besides frames. */
export interface DecoderHooks {
  /**
   * A byte that is not part of a frame of this protocol, in stream order with
   * the events: terminal output, typed characters, the terminal's answer to
   * `CSI c`, other protocols' APC strings. A program reads the reply to its
   * `CSI c` fence here.
   */
  noise?: (byte: number) => void;
}

/**
 * Reads frames from raw bytes (specification §4.7): it finds `ESC _` in the
 * noise, collects what follows and delivers messages and errors as they end. It
 * may be fed in pieces of any size, down to one byte; the result does not depend
 * on where they are cut. A stream that ends inside a frame produces nothing.
 */
export class StreamDecoder {
  readonly #limits: Limits;
  readonly #assembler: Assembler;
  readonly #body: Uint8Array;
  readonly #noise: ((byte: number) => void) | undefined;
  #length = 0;
  #scan: Scan = Scan.Ground;

  constructor(direction: Direction, hooks: DecoderHooks = {}) {
    this.#limits = LIMITS[direction];
    this.#assembler = new Assembler(this.#limits.message);
    this.#body = new Uint8Array(this.#limits.frame);
    this.#noise = hooks.noise;
  }

  /** Feeds the next bytes of the stream; every event is passed to `emit` as it happens. */
  feed(data: Uint8Array, emit: (event: DecodeEvent) => void): void {
    for (const byte of data) this.#step(byte, emit);
  }

  /** Feeds the next bytes of the stream and returns the events they ended. */
  collect(data: Uint8Array): DecodeEvent[] {
    const events: DecodeEvent[] = [];
    this.feed(data, (event) => events.push(event));
    return events;
  }

  #noiseByte(byte: number): void {
    this.#noise?.(byte);
  }

  #step(byte: number, emit: (event: DecodeEvent) => void): void {
    switch (this.#scan) {
      case Scan.Ground:
        if (byte === ESC) this.#scan = Scan.Esc;
        else this.#noiseByte(byte);
        return;
      case Scan.Esc:
        if (byte === UNDERSCORE) {
          this.#length = 0;
          this.#scan = Scan.Body;
        } else if (byte === ESC) {
          // ESC ESC: the first is noise, the second may begin a string.
          this.#noiseByte(ESC);
        } else {
          this.#noiseByte(ESC);
          this.#noiseByte(byte);
          this.#scan = Scan.Ground;
        }
        return;
      case Scan.Body:
        this.#bodyByte(byte, emit);
        return;
      case Scan.BodyEsc:
        if (byte === BACKSLASH) {
          this.#end(false, emit);
          this.#scan = Scan.Ground;
        } else if (byte === UNDERSCORE) {
          this.#end(true, emit);
          this.#length = 0;
          this.#scan = Scan.Body;
        } else if (byte === ESC) {
          this.#end(true, emit);
          this.#scan = Scan.Esc;
        } else {
          this.#end(true, emit);
          this.#scan = Scan.Ground;
        }
        return;
    }
  }

  #bodyByte(byte: number, emit: (event: DecodeEvent) => void): void {
    if (byte === ESC) {
      this.#scan = Scan.BodyEsc;
      return;
    }
    if (this.#length < PREFIX.length) {
      // Until `pencere;` is complete the string may be someone else's: it is
      // dropped at its first differing byte, without a word.
      if (byte !== PREFIX.charCodeAt(this.#length)) {
        this.#scan = Scan.Ground;
        return;
      }
      this.#body[this.#length++] = byte;
      return;
    }
    if (byte < 0x20 || byte > 0x7e) {
      emit({ type: "error", error: "malformed" });
      this.#scan = Scan.Ground;
      return;
    }
    if (this.#length >= this.#limits.frame - FRAME_CLOSURE) {
      emit({ type: "error", error: "too-long" });
      this.#scan = Scan.Ground;
      return;
    }
    this.#body[this.#length++] = byte;
  }

  /** The string ended; `cancelled` is whether something other than `ESC \` ended it. */
  #end(cancelled: boolean, emit: (event: DecodeEvent) => void): void {
    const length = this.#length;
    this.#length = 0;
    // A string that ended before `pencere;` is complete is not ours, however it ended.
    if (length < PREFIX.length) return;
    if (cancelled) {
      emit({ type: "error", error: "malformed" });
      return;
    }
    const chunk = parseChunk(Buffer.from(this.#body.buffer, 0, length).toString("latin1"));
    if (chunk === undefined) emit({ type: "error", error: "malformed" });
    else this.#assembler.chunk(chunk, emit);
  }
}

// ---- writing ---------------------------------------------------------------

/** Counts a sender's message ids: from 1, upwards, each used once (§4.4). */
export class Ids {
  #next = 1;

  next(): number {
    const id = this.#next;
    this.#next = id >= 0xffff_ffff ? 1 : id + 1;
    return id;
  }
}

/** A message to write. */
export interface Outgoing {
  /** The kind, 1 to 16 of `a-z` and `-`. */
  readonly kind: string;
  /** The id, at least 1. */
  readonly id: number;
  /** The `re` parameter, an id. */
  readonly re?: number;
  /** The `d` parameter, 64 lowercase hex digits. */
  readonly digest?: string;
  /**
   * The payload: printable ASCII, 0x20 to 0x7E. JSON must have been through
   * {@link toWire} or {@link escape}; base64 satisfies it by construction.
   */
  readonly payload: string;
}

/** How a message is cut into chunks. */
export type Split =
  /** Every chunk but the last holds as much payload as its direction's frame limit lets it (§4.4). */
  | "fill"
  /** Every chunk but the last holds this much payload (rounded down to a multiple of four for `file`). */
  | { readonly fixed: number };

/** Why a message could not be written. Nothing is written in that case. */
export type WriteErrorCode = "kind" | "id" | "digest" | "payload" | "message-too-long" | "no-room";

const WRITE_ERRORS: Readonly<Record<WriteErrorCode, string>> = {
  kind: "the kind is not 1 to 16 of a-z and -",
  id: "the id is 0",
  digest: "the digest is not 64 lowercase hex digits",
  payload: "the payload has a byte a frame may not carry",
  "message-too-long": "the payload is over the message limit",
  "no-room": "no frame can carry this",
};

export class WriteError extends Error {
  readonly code: WriteErrorCode;

  constructor(code: WriteErrorCode) {
    super(WRITE_ERRORS[code]);
    this.name = "WriteError";
    this.code = code;
  }
}

/** Whether every character of `text` is one a frame may carry: printable ASCII. */
export function isFrameText(text: string): boolean {
  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i);
    if (code < 0x20 || code > 0x7e) return false;
  }
  return true;
}

function controlText(message: Outgoing, seq: number, more: boolean): string {
  let text = `id=${message.id}`;
  if (seq > 0) text += `,seq=${seq}`;
  if (more) text += ",more=1";
  if (seq === 0) {
    if (message.re !== undefined) text += `,re=${message.re}`;
    if (message.digest !== undefined) text += `,d=${message.digest}`;
  }
  return text;
}

function frameText(kind: string, control: string, payload: string): string {
  return `\x1b_${PREFIX}${kind};${control};${payload}\x1b\\`;
}

/**
 * Writes `message` as one or more frames for `direction`: keys in the order
 * `id`, `seq`, `more`, `re`, `d`, with `seq` and `more` left out when they are
 * 0, and `re` and `d` on the first chunk only (§4.3, §4.4).
 *
 * The same message and the same split give the same bytes on every
 * implementation, which is what the fixtures rely on.
 *
 * @throws {WriteError} for a message that does not follow the wire.
 */
export function encode(direction: Direction, message: Outgoing, split: Split = "fill"): Uint8Array {
  const limits = LIMITS[direction];
  if (!KIND.test(message.kind)) throw new WriteError("kind");
  if (!Number.isInteger(message.id) || message.id < 1 || message.id > 0xffff_ffff)
    throw new WriteError("id");
  if (
    message.re !== undefined &&
    (!Number.isInteger(message.re) || message.re < 1 || message.re > 0xffff_ffff)
  )
    throw new WriteError("id");
  if (message.digest !== undefined && !DIGEST.test(message.digest)) throw new WriteError("digest");
  if (!isFrameText(message.payload)) throw new WriteError("payload");
  if (message.payload.length > limits.message) throw new WriteError("message-too-long");

  const align = message.kind === "file";
  const room = (control: string): number =>
    Math.max(0, limits.frame - (FRAME_FIXED + message.kind.length + control.length));
  const frames: string[] = [];
  let offset = 0;
  let seq = 0;
  for (;;) {
    const rest = message.payload.length - offset;
    const last = controlText(message, seq, false);
    const lastCapacity = split === "fill" ? room(last) : split.fixed;
    if (rest <= lastCapacity) {
      if (FRAME_FIXED + message.kind.length + last.length + rest > limits.frame)
        throw new WriteError("no-room");
      frames.push(frameText(message.kind, last, message.payload.slice(offset)));
      break;
    }
    const more = controlText(message, seq, true);
    let capacity = split === "fill" ? room(more) : split.fixed;
    if (align) capacity -= capacity % 4;
    if (capacity <= 0) throw new WriteError("no-room");
    if (FRAME_FIXED + message.kind.length + more.length + capacity > limits.frame)
      throw new WriteError("no-room");
    frames.push(frameText(message.kind, more, message.payload.slice(offset, offset + capacity)));
    offset += capacity;
    seq += 1;
    if (seq > 0xffff_ffff) throw new WriteError("no-room");
  }
  return Buffer.from(frames.join(""), "latin1");
}
