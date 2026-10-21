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
