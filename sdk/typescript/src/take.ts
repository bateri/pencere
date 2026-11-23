/**
 * What the terminal hands a program that is running for it to take (§10.9): an address, a file,
 * a folder. The program says in its `open` which kinds it takes (`takes`); each thing the person
 * gives it is a `take`, and the library pushes it to the view's subscriptions of
 * {@link TAKE_TOPIC} at once.
 */

import { parseObject } from "./json.js";

/** A kind of thing a program takes: a member of `takes` of its `open`. */
export type TakeKind = "url" | "file" | "folder";

/** The topic a view subscribes to, to be told of what the terminal hands its program (§10.9). */
export const TAKE_TOPIC = "pencere.take";

/** The most bytes the path of a file or a folder may have in a `take`. */
export const MAX_PATH = 4096;

/** The most bytes an address may have: a link a view may ask to open is held to the same (§9.7). */
export const MAX_URL = 8192;

/** One thing the terminal hands the program: exactly one of the three shapes. */
export type Take =
  | { readonly url: string }
  | { readonly file: string; readonly line?: number; readonly column?: number }
  | { readonly folder: string };

/** The kind of thing a `take` is. */
export function takeKind(take: Take): TakeKind {
  if ("url" in take) return "url";
  if ("file" in take) return "file";
  return "folder";
}

/** A path as a `take` carries it: something, not too long, and no control character in it. */
function pathOk(path: unknown): path is string {
  return (
    typeof path === "string" &&
    // Absolute: a program does not read it against a folder of its own.
    path.startsWith("/") &&
    new TextEncoder().encode(path).length <= MAX_PATH &&
    // C0, DEL and C1.
    !/\p{Cc}/u.test(path)
  );
}

/** An address a view may ask to open: `http` or `https`, printable ASCII without the space (§9.7). */
function urlOk(url: unknown): url is string {
  return (
    typeof url === "string" &&
    url.length <= MAX_URL &&
    /^https?:\/\//i.test(url) &&
    // eslint-disable-next-line no-control-regex
    /^[\x21-\x7e]+$/.test(url)
  );
}

/** A position in a file: a whole number from 1 that fits 32 bits. */
function positionOk(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= 1 && value <= 0xffffffff;
}

/**
 * What a `take` carries, read from its payload: exactly one of `url`, `file` and `folder`, valid as
 * the specification says; `line` and `column` only with `file` (a `column` only with a `line`). A
 * member of another name is ignored; anything else makes it `undefined`, and the program ignores
 * the message.
 */
export function parseTake(payload: string): Take | undefined {
  const body = parseObject(payload);
  if (body === undefined) return undefined;
  const has = (name: string) => Object.hasOwn(body, name);
  const named = ["url", "file", "folder"].filter(has);
  if (named.length !== 1) return undefined;
  const places = has("line") || has("column");
  if (has("url")) return !places && urlOk(body.url) ? { url: body.url } : undefined;
  if (has("folder")) return !places && pathOk(body.folder) ? { folder: body.folder } : undefined;
  if (!pathOk(body.file)) return undefined;
  if (has("line") && !positionOk(body.line)) return undefined;
  if (has("column") && (!positionOk(body.column) || !has("line"))) return undefined;
  return {
    file: body.file,
    ...(has("line") ? { line: body.line as number } : {}),
    ...(has("column") ? { column: body.column as number } : {}),
  };
}
