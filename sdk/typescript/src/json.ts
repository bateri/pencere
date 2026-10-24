/**
 * JSON text for the wire (specification §4.5): one line of compact JSON in which
 * every character a frame may not carry is escaped, so that what is written is
 * printable ASCII and nothing a tty's line discipline treats specially.
 */

/** The characters `JSON.stringify` leaves raw that a frame may not carry: DEL and everything from U+0080 up. */
const RAW = /[\u007f-￿]/g;

/**
 * Escapes the characters of JSON text that a frame may not carry: DEL and every
 * code unit from U+0080 up become `\uXXXX` with lowercase hex digits, and a code
 * point from U+10000 up is written as the surrogate pair its code units already
 * are. `JSON.stringify` has escaped the rest (`\"`, `\\`, `\b`, `\t`, `\n`,
 * `\f`, `\r`, and `\u00XX` for the other controls, lowercase) and writes DEL and
 * non-ASCII text raw, which is what this repairs.
 */
export function escape(json: string): string {
  return json.replace(RAW, (unit) => `\\u${unit.charCodeAt(0).toString(16).padStart(4, "0")}`);
}

/**
 * The payload text of a JSON value: compact, members in the order the value
 * has them (the order of Appendix A for the types there), and printable ASCII.
 */
export function toWire(value: unknown): string {
  const text = JSON.stringify(value);
  if (text === undefined) throw new TypeError("the value has no JSON text");
  return escape(text);
}

const HEX4 = /^[0-9a-fA-F]{4}$/;

/** The unit a `\uXXXX` at `at` stands for, if `text` has four hex digits there. */
function unitAt(text: string, at: number): number | undefined {
  const digits = text.slice(at, at + 4);
  return HEX4.test(digits) ? Number.parseInt(digits, 16) : undefined;
}

/**
 * How many bytes JSON text has once its escapes are undone: the length of the text a message is
 * before the escapes of the wire (§4.5) made it longer, which is what the 1 MiB limit of a view's
 * message counts (§9.3). Each escape counts as the UTF-8 of the character it stands for (a
 * surrogate pair as one character of four bytes), and every other character as its own UTF-8. It
 * reads text that may not be JSON: a backslash that does not begin an escape counts as itself, and
 * nothing is checked. The host counts the same way.
 */
export function unescapedLength(text: string): number {
  let length = 0;
  let at = 0;
  while (at < text.length) {
    const code = text.charCodeAt(at);
    if (code !== 0x5c) {
      // A character as itself: one byte up to U+007F, two up to U+07FF, three to U+FFFF (a lone
      // surrogate is written as U+FFFD, three), four for a pair.
      if (code < 0x80) length += 1;
      else if (code < 0x800) length += 2;
      else if (code >= 0xd800 && code <= 0xdbff && isLow(text.charCodeAt(at + 1))) {
        length += 4;
        at += 1;
      } else length += 3;
      at += 1;
      continue;
    }
    const next = text[at + 1];
    if (next === undefined) {
      length += 1;
      at += 1;
    } else if (next !== "u") {
      length += 1;
      at += 2;
    } else {
      const unit = unitAt(text, at + 2);
      if (unit === undefined) {
        length += 1;
        at += 1;
      } else if (unit >= 0xd800 && unit <= 0xdbff && text.startsWith("\\u", at + 6)) {
        const low = unitAt(text, at + 8);
        if (low !== undefined && low >= 0xdc00 && low <= 0xdfff) {
          length += 4;
          at += 12;
        } else {
          length += 3;
          at += 6;
        }
      } else {
        length += unit < 0x80 ? 1 : unit < 0x800 ? 2 : 3;
        at += 6;
      }
    }
  }
  return length;
}

function isLow(code: number): boolean {
  return code >= 0xdc00 && code <= 0xdfff;
}

/** Parses `text` as JSON and returns it if it is an object (not an array, not `null`). */
export function parseObject(text: string): Record<string, unknown> | undefined {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return undefined;
  }
  if (typeof value !== "object" || value === null || Array.isArray(value)) return undefined;
  return value as Record<string, unknown>;
}
