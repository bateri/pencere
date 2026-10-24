/**
 * Bytes and text without `Buffer`.
 *
 * `Buffer` is Node's; Deno has it only behind an import and a program compiled by `deno
 * compile` has nothing but the platform. What the protocol needs of it is small: text of
 * one byte a character (the frames are Latin-1, §4.2), the length of a path in UTF-8, the
 * order of two paths as bytes, and base64. A `Uint8Array` is what every runtime reads
 * and writes, so these are all that is built on it.
 */

/** How many bytes `String.fromCharCode` is given at once: a call takes its arguments on the stack. */
const SLICE = 0x2000;

/** The characters `start` to `end` of `bytes`, one character a byte (Latin-1, not `TextDecoder`'s `latin1`, which is windows-1252). */
export function latin1(bytes: Uint8Array, start = 0, end: number = bytes.length): string {
  let text = "";
  for (let at = start; at < end; at += SLICE) {
    const slice = bytes.subarray(at, Math.min(at + SLICE, end));
    text += String.fromCharCode.apply(null, slice as unknown as number[]);
  }
  return text;
}

/** The bytes of `text`, one a character; a character above U+00FF keeps its low byte. */
export function latin1Bytes(text: string): Uint8Array {
  const bytes = new Uint8Array(text.length);
  for (let at = 0; at < text.length; at += 1) bytes[at] = text.charCodeAt(at) & 0xff;
  return bytes;
}

const encoder = new TextEncoder();

/** The bytes of `text` in UTF-8. */
export function utf8Bytes(text: string): Uint8Array {
  return encoder.encode(text);
}

/** The length of `text` in UTF-8. */
export function utf8Length(text: string): number {
  return encoder.encode(text).length;
}

/** Negative when `a` comes before `b` as bytes, positive when after, 0 when they are the same. */
export function compareBytes(a: Uint8Array, b: Uint8Array): number {
  const common = Math.min(a.length, b.length);
  for (let at = 0; at < common; at += 1) {
    const difference = (a[at] ?? 0) - (b[at] ?? 0);
    if (difference !== 0) return difference;
  }
  return a.length - b.length;
}

/** Three bytes make four characters, so a slice of a multiple of three is base64 on its own. */
const BASE64_SLICE = 3 * 0x1000;

/** `data` as base64 with padding (RFC 4648 §4). */
export function toBase64(data: Uint8Array): string {
  const parts: string[] = [];
  for (let at = 0; at < data.length; at += BASE64_SLICE) {
    parts.push(btoa(latin1(data, at, Math.min(at + BASE64_SLICE, data.length))));
  }
  return parts.join("");
}
