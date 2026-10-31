/**
 * The payloads of the messages (specification Appendix A) as a program writes
 * and reads them. Members are written in the order the specification gives;
 * what a host sends is read leniently, member by member, and a message that
 * cannot be understood is `undefined`, which the caller ignores (§4.5). What the
 * view sends is read strictly (§9.2): an object that is not one of the three
 * messages a view sends is ignored.
 */

import type { BundleManifest } from "./bundle.js";
import { parseObject, toWire } from "./json.js";

/** The protocol versions this library speaks: this version, and the one before it when there is one (§5.4). */
export const VERSIONS: readonly number[] = [1];

/** A feature a host lists in its `welcome`: it closes a view when the program sends `dismiss` (§10.8). */
export const FEATURE_DISMISS = "dismiss";

/** A feature a host lists in its `welcome`: it keeps a folder on the program's machine for a file's bytes to cross through (§8.11). */
export const FEATURE_STAGE = "stage";

/** The kinds of view of the table of §5.5. */
export type ViewKind = "lua" | "wasm" | "web";

/** The kinds of view of the table of §5.5. */
export const VIEW_KINDS: readonly ViewKind[] = ["lua", "wasm", "web"];

/** The media type the entry of a bundle has for each kind of view (§6.3). */
export const ENTRY_TYPES: Readonly<Record<ViewKind, string>> = {
  lua: "text/x-lua",
  wasm: "application/wasm",
  web: "text/html",
};

/** The version of the bridge of a page that this library's `web` views are written against (§9.4). */
export const WEB_VERSION = "1.0";

/** A version of a kind's interface: `major.minor` (§5.5). */
const VERSION = /^(0|[1-9][0-9]{0,8})\.(0|[1-9][0-9]{0,8})$/;

/** Whether `text` is a version of §5.5. */
export function isVersion(text: string): boolean {
  return VERSION.test(text);
}

/** A kind of view and the versions of its interface, in the order of whoever wrote the list (§5.1, §5.2). */
export interface ViewOffer {
  readonly kind: string;
  readonly versions: readonly string[];
}

/** An icon for the program (§7.3). */
export interface Icon {
  /** A `data:` URI with base64 data. */
  src: string;
  mimeType?: string;
  /** `WxH`, as HTML has them. */
  sizes?: string[];
  /** The background the icon was drawn for. */
  theme?: "light" | "dark";
}

/** The browser capabilities a page may ask for (§6.3). */
export type Permission = "camera" | "microphone" | "geolocation" | "clipboardWrite";

/** What a page asks to reach (§6.3, §12.2). Origins are `scheme://host[:port]`. */
export interface Access {
  /** The origins the page may send network requests to. */
  connect?: string[];
  /** The origins it may load scripts, styles, images, fonts and media from. */
  resources?: string[];
  /** The origins it may nest as frames. */
  frames?: string[];
  /** The origins a `<base>` element may name. */
  base?: string[];
  permissions?: Permission[];
}

/** Why a program ends (`Bye.reason`). */
export type ByeReason = "exit" | "signal" | "error";

/** The payload of `hello`: the versions the program speaks and the kinds of view it can show, in its order (§5.1). */
export function helloPayload(
  views: readonly ViewOffer[],
  versions: readonly number[] = VERSIONS,
): string {
  return toWire({
    versions: [...versions],
    views: views.map((view) => ({ kind: view.kind, versions: [...view.versions] })),
  });
}

/** What an `open` carries (§6.3). */
export interface OpenFields {
  token: string;
  view: string;
  version: string;
  program?: string | undefined;
  title?: string | undefined;
  icons?: readonly Icon[] | undefined;
  /** For a `web` view only. */
  access?: Access | undefined;
  bundle: BundleManifest;
}

function icon(item: Icon): Record<string, unknown> {
  const body: Record<string, unknown> = { src: item.src };
  if (item.mimeType !== undefined) body.mimeType = item.mimeType;
  if (item.sizes !== undefined) body.sizes = item.sizes;
  if (item.theme !== undefined) body.theme = item.theme;
  return body;
}

function access(item: Access): Record<string, unknown> {
  const body: Record<string, unknown> = {};
  if (item.connect !== undefined) body.connect = item.connect;
  if (item.resources !== undefined) body.resources = item.resources;
  if (item.frames !== undefined) body.frames = item.frames;
  if (item.base !== undefined) body.base = item.base;
  if (item.permissions !== undefined) body.permissions = item.permissions;
  return body;
}

/** The payload of `open`: members in the order of Appendix A, whatever order the caller built them in. */
export function openPayload(open: OpenFields): string {
  const body: Record<string, unknown> = {
    token: open.token,
    view: open.view,
    version: open.version,
  };
  if (open.program !== undefined) body.program = open.program;
  if (open.title !== undefined) body.title = open.title;
  if (open.icons !== undefined) body.icons = open.icons.map(icon);
  if (open.access !== undefined) body.access = access(open.access);
  body.bundle = {
    entry: open.bundle.entry,
    files: open.bundle.files.map((file) => ({
      path: file.path,
      sha256: file.sha256,
      size: file.size,
      type: file.type,
    })),
  };
  return toWire(body);
}

/** The payload of `bye`: the token the view was opened with, and why the program ends. */
export function byePayload(token: string, reason: ByeReason): string {
  return toWire({ token, reason });
}

/** The payload of `dismiss`: the token the view was opened with (§10.8). */
export function dismissPayload(token: string): string {
  return toWire({ token });
}

/** A file a program attaches to its view (§8.8): its digest and its length in bytes. */
export interface AttachFile {
  readonly sha256: string;
  readonly size: number;
}

/** The payload of `attach`: files the program adds to the view that runs, for it to draw (§8.8). */
export function attachPayload(files: readonly AttachFile[]): string {
  return toWire({ files: files.map(({ sha256, size }) => ({ sha256, size })) });
}

/** The payload of `detach`: digests of attached files the program lets go of (§8.8). */
export function detachPayload(digests: readonly string[]): string {
  return toWire({ files: digests });
}

/** The payload of `read`: bytes of a file the person gave the view, by its handle (§8.9). */
export function readPayload(handle: string, offset: number, length: number): string {
  return toWire({ handle, offset, length });
}

/** The payload of `stage`: the file of a handle the person gave, from `offset`, through the host's folder (§8.11). */
export function stagePayload(handle: string, offset: number): string {
  return toWire({ handle, offset });
}

/** The payload of `fetch`: the file of `size` bytes the program sends for the place of a handle, from `offset` (§8.11). */
export function fetchPayload(handle: string, size: number, offset: number): string {
  return toWire({ handle, size, offset });
}

/** The payload of `unstage`: the id of the `stage` or the `fetch` the program takes back (§8.11). */
export function unstagePayload(id: number): string {
  return toWire({ id });
}

// ---- what the host says ------------------------------------------------------

/** What a host answers to `hello` with a version in common (§5.2), or offers again unasked (§10.7). */
export interface WelcomeOffer {
  readonly kind: "offer";
  readonly version: number;
  readonly token: string;
  readonly host?: { readonly name: string; readonly version: string };
  readonly features: readonly string[];
  /**
   * What the host will open: for an answer to a `hello`, the program's list cut down to what both
   * speak, in the program's order; for an unsolicited `welcome`, every kind and every version the
   * host speaks, in the host's order (§10.7). Empty when nothing is shared (§5.2).
   */
  readonly views: readonly ViewOffer[];
  /**
   * The kinds of the program's list that the host opens, and at no version the program speaks,
   * each with the versions the host does open (§5.2). Empty when there are none.
   */
  readonly opens: readonly ViewOffer[];
  readonly restart: boolean;
}

/** What a host answers to `hello` (§5.2) or offers again unasked (§10.7). */
export type Welcome =
  | WelcomeOffer
  /** A host that shares no protocol version with this program. */
  | { readonly kind: "unsupported"; readonly supported: readonly number[] };

/** A token: 16 bytes in base64url without padding (§6.1). */
const TOKEN = /^[A-Za-z0-9_-]{22}$/;

const DIGEST = /^[0-9a-f]{64}$/;

function strings(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string")
    : [];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/**
 * The entries of a `views` list that are of the shape of §5.2: a string `kind` and a list of
 * versions, at least one of them a version of §5.5. A kind listed twice counts once, at its first
 * place (§5.1). Anything else in the list is left out.
 */
export function readViews(value: unknown): ViewOffer[] {
  if (!Array.isArray(value)) return [];
  const views: ViewOffer[] = [];
  for (const item of value) {
    if (!isRecord(item) || typeof item.kind !== "string") continue;
    if (views.some((view) => view.kind === item.kind)) continue;
    const versions = strings(item.versions).filter(isVersion);
    if (versions.length > 0) views.push({ kind: item.kind, versions });
  }
  return views;
}

/**
 * Reads a `welcome`. `offered` is what the program's `hello` listed: a version it did not offer is
 * not one the host can have chosen, so such a host is one that shares no version.
 */
export function parseWelcome(payload: string, offered: readonly number[]): Welcome | undefined {
  const body = parseObject(payload);
  if (body === undefined) return undefined;
  const { version, token } = body;
  if (version !== undefined) {
    if (typeof version !== "number" || !Number.isInteger(version) || version < 1) return undefined;
    if (typeof token !== "string" || !TOKEN.test(token)) return undefined;
    if (!offered.includes(version)) return { kind: "unsupported", supported: [] };
    const host = body.host;
    return {
      kind: "offer",
      version,
      token,
      ...(isRecord(host) && typeof host.name === "string" && typeof host.version === "string"
        ? { host: { name: host.name, version: host.version } }
        : {}),
      features: strings(body.features),
      views: readViews(body.views),
      opens: readViews(body.opens),
      restart: body.restart === true,
    };
  }
  if (Array.isArray(body.supported)) {
    const supported = body.supported.filter(
      (n): n is number => typeof n === "number" && Number.isInteger(n) && n >= 1,
    );
    return { kind: "unsupported", supported };
  }
  return undefined;
}

/** A kind of view and one version of it: what the program opens. */
export interface Choice {
  readonly kind: string;
  readonly version: string;
}

/**
 * What the program opens, given its own list and what a `welcome` lists (§5.3): the first kind of
 * the program's list that the host lists and, for it, the first of the program's versions of it
 * that the host lists. The program's order decides: the answer to a `hello` already keeps it
 * (§5.2), and an unsolicited `welcome` lists the host's own order (§10.7). `undefined` when
 * nothing is shared.
 */
export function chooseView(
  mine: readonly ViewOffer[],
  theirs: readonly ViewOffer[],
): Choice | undefined {
  for (const view of mine) {
    const host = theirs.find((offer) => offer.kind === view.kind);
    if (host === undefined) continue;
    const version = view.versions.find((v) => host.versions.includes(v));
    if (version !== undefined) return { kind: view.kind, version };
  }
  return undefined;
}

/**
 * The sentence for a program whose host opens kinds of its list, but not in the versions it
 * speaks (§5.2): the versions of those kinds that it is written for, and those the host opens.
 * `undefined` when the host named no such kind.
 */
export function noVersionSentence(
  mine: readonly ViewOffer[],
  opens: readonly ViewOffer[],
): string | undefined {
  const wanted: string[] = [];
  const held: string[] = [];
  for (const view of mine) {
    const host = opens.find((offer) => offer.kind === view.kind);
    if (host === undefined) continue;
    wanted.push(`${view.kind} ${view.versions.join(" or ")}`);
    held.push(`${view.kind} ${host.versions.join(" or ")}`);
  }
  if (wanted.length === 0) return undefined;
  return `this program is written for ${wanted.join(", ")}; this terminal opens ${held.join(", ")}`;
}

/** Reads a `need`: the digests the host lacks, in its order. */
export function parseNeed(payload: string): string[] | undefined {
  const body = parseObject(payload);
  if (body === undefined || !Array.isArray(body.missing)) return undefined;
  return strings(body.missing).filter((digest) => DIGEST.test(digest));
}

/** A digest of an `attach` that the host does not attach, and why: `limit`, `view`, `size`, `digest`. */
export interface Refusal {
  readonly sha256: string;
  readonly code: string;
}

/**
 * Reads what a `need` refuses: empty for the `need` of an `open`, and for one that answers an
 * `attach` whose files were all taken (§8.8).
 */
export function parseRefused(payload: string): Refusal[] | undefined {
  const body = parseObject(payload);
  if (body === undefined || !Array.isArray(body.missing)) return undefined;
  if (body.refused === undefined) return [];
  if (!Array.isArray(body.refused)) return undefined;
  const out: Refusal[] = [];
  for (const entry of body.refused as unknown[]) {
    if (typeof entry !== "object" || entry === null || Array.isArray(entry)) continue;
    const { sha256, code } = entry as Record<string, unknown>;
    if (typeof sha256 === "string" && DIGEST.test(sha256) && typeof code === "string")
      out.push({ sha256, code });
  }
  return out;
}

/** What a host says at the end of a file a program wrote, or at the first thing that went wrong (§8.10). */
export interface Wrote {
  /** `handle`, `order`, `size`, `space`, `io`; absent when the file was written. */
  readonly code?: string;
}

/** Reads a `wrote`. */
export function parseWrote(payload: string): Wrote | undefined {
  const body = parseObject(payload);
  if (body === undefined) return undefined;
  return typeof body.code === "string" ? { code: body.code } : {};
}

const PIPE = /^[0-9a-f]{32}$/;

/** A byte count of a `staged` or `fetched`. */
function count(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0 ? value : undefined;
}

/** What a host says of a `stage` (§8.11): the pipe to open, then how the file went. */
export type Staged =
  | { readonly kind: "pipe"; readonly pipe: string }
  /** The file went through the pipe and ended `sent` bytes into it. */
  | { readonly kind: "sent"; readonly sent: number }
  /** `handle`, `busy`, `gone`, `size`, `route`, `io` or `cancelled`. */
  | { readonly kind: "failed"; readonly code: string };

/** Reads a `staged`; `undefined` when it is none of the three. */
export function parseStaged(payload: string): Staged | undefined {
  const body = parseObject(payload);
  if (body === undefined) return undefined;
  if (typeof body.pipe === "string") {
    return PIPE.test(body.pipe) ? { kind: "pipe", pipe: body.pipe } : undefined;
  }
  const sent = count(body.sent);
  if (sent !== undefined) return { kind: "sent", sent };
  return typeof body.code === "string" ? { kind: "failed", code: body.code } : undefined;
}

/** What a host says of a `fetch` (§8.11): the pipe to open, then whether the file is saved. */
export type Fetched =
  | { readonly kind: "pipe"; readonly pipe: string }
  /** The file is in the place the person chose, `saved` bytes long. */
  | { readonly kind: "saved"; readonly saved: number }
  /** `handle`, `busy`, `size`, `space`, `route`, `io` or `cancelled`; `at` goes with `route`: the length the host holds. */
  | { readonly kind: "failed"; readonly code: string; readonly at?: number };

/** Reads a `fetched`; `undefined` when it is none of the three. */
export function parseFetched(payload: string): Fetched | undefined {
  const body = parseObject(payload);
  if (body === undefined) return undefined;
  if (typeof body.pipe === "string") {
    return PIPE.test(body.pipe) ? { kind: "pipe", pipe: body.pipe } : undefined;
  }
  const saved = count(body.saved);
  if (saved !== undefined) return { kind: "saved", saved };
  if (typeof body.code !== "string") return undefined;
  const at = count(body.at);
  return at === undefined
    ? { kind: "failed", code: body.code }
    : { kind: "failed", code: body.code, at };
}

/** Why a host ended or refused a view (`Closed.reason`). */
export type ClosedReason = "refused" | "error" | "user" | "view" | "program";

/** What a host says when a view ended or was refused (§6.4, §10). */
export interface Closed {
  readonly reason: ClosedReason;
  /** `token`, `busy`, `bundle`, `view`, ...; a code this library does not know is no more than the reason. */
  readonly code?: string;
  /** For people, never parsed: why a view the host draws itself failed, for a script the error and its line (§9.8). */
  readonly message?: string;
}

const REASONS: readonly string[] = ["refused", "error", "user", "view", "program"];

/**
 * Reads a `closed`. A reason this version does not know still ends the view,
 * and is read as `error`: a host that has learned a new reason has no other way
 * to say that the view is gone.
 */
export function parseClosed(payload: string): Closed | undefined {
  const body = parseObject(payload);
  if (body === undefined || typeof body.reason !== "string") return undefined;
  const reason = REASONS.includes(body.reason) ? (body.reason as ClosedReason) : "error";
  return {
    reason,
    ...(typeof body.code === "string" ? { code: body.code } : {}),
    ...(typeof body.message === "string" ? { message: body.message } : {}),
  };
}

// ---- the six messages (§9.2) ----------------------------------------------------

/**
 * The most a `msg` may carry in either direction (§9.3), in bytes of the JSON text the sender gave,
 * counted before the escapes of the wire (`unescapedLength`): the escapes can make it six times as
 * long on the wire.
 */
export const MAX_VIEW_MESSAGE = 1024 * 1024;

/** One of the six messages of §9.2: an object with one member, named for what it is. */
export type ViewMessage =
  | { call: { id: number; method: string; params?: unknown } }
  | {
      response:
        { id: number; result: unknown } | { id: number; error: { code: string; message?: string } };
    }
  | { subscribe: { id: number; topic: string; params?: unknown } }
  | { push: { id: number; value: unknown } }
  | { cancel: { id: number } }
  | { progress: { id: number; done: number; total?: number; note?: string } };

/** The payload of a `call` of the view. A program never writes it; a test of a program does. */
export function callPayload(id: number, method: string, params?: unknown): string {
  const call: Record<string, unknown> = { id, method };
  if (params !== undefined) call.params = params;
  return toWire({ call });
}

/** The payload of a `subscribe` of the view. A program never writes it; a test of a program does. */
export function subscribePayload(id: number, topic: string, params?: unknown): string {
  const subscribe: Record<string, unknown> = { id, topic };
  if (params !== undefined) subscribe.params = params;
  return toWire({ subscribe });
}

/** The payload of a `cancel` of the view. A program never writes it; a test of a program does. */
export function cancelPayload(id: number): string {
  return toWire({ cancel: { id } });
}

/** The payload of the `response` that ends a `call` with a result. A result is never `null`: nothing to say is `{}`. */
export function responsePayload(id: number, result: unknown): string {
  return toWire({ response: { id, result: result ?? {} } });
}

/** The payload of the `response` that ends a `call` with an error: a short `code` the view can act on, and words for the person. */
export function failurePayload(id: number, code: string, message?: string): string {
  const error: Record<string, unknown> = { code };
  if (message !== undefined) error.message = message;
  return toWire({ response: { id, error } });
}

/** The payload of a `push`: a value of the subscription `id`. */
export function pushPayload(id: number, value: unknown): string {
  return toWire({ push: { id, value: value ?? null } });
}

/** The payload of a `progress`: how far the call `id` has got. */
export function progressPayload(id: number, done: number, total?: number, note?: string): string {
  const progress: Record<string, unknown> = { id, done };
  if (total !== undefined) progress.total = total;
  if (note !== undefined) progress.note = note;
  return toWire({ progress });
}

/** What the view asks of the program (§9.2): the three messages it sends. */
export type ViewRequest =
  | {
      readonly type: "call";
      readonly id: number;
      readonly method: string;
      readonly params?: unknown;
    }
  | {
      readonly type: "subscribe";
      readonly id: number;
      readonly topic: string;
      readonly params?: unknown;
    }
  | { readonly type: "cancel"; readonly id: number };

function requestId(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 1 ? value : undefined;
}

/**
 * Reads what the view sent in a `msg`. The view is untrusted: only an object with exactly one
 * member, `call`, `subscribe` or `cancel`, whose value is an object with an `id` that is a
 * positive integer and the other members of the message (a string `method` or `topic`), is a
 * message; anything else — another member, a second member, a wrong kind, an `id` of 0 — is
 * `undefined` and is ignored. Unknown members of the inner object are ignored (§9.2).
 */
export function parseViewRequest(payload: string): ViewRequest | undefined {
  const body = parseObject(payload);
  if (body === undefined) return undefined;
  const names = Object.keys(body);
  const name = names[0];
  if (names.length !== 1 || name === undefined) return undefined;
  const inner = body[name];
  if (!isRecord(inner)) return undefined;
  const id = requestId(inner.id);
  if (id === undefined) return undefined;
  switch (name) {
    case "call":
      if (typeof inner.method !== "string") return undefined;
      return {
        type: "call",
        id,
        method: inner.method,
        ...("params" in inner ? { params: inner.params } : {}),
      };
    case "subscribe":
      if (typeof inner.topic !== "string") return undefined;
      return {
        type: "subscribe",
        id,
        topic: inner.topic,
        ...("params" in inner ? { params: inner.params } : {}),
      };
    case "cancel":
      return { type: "cancel", id };
    default:
      return undefined;
  }
}
