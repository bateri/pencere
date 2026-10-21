/**
 * The payloads of the messages (specification Appendix A) as a program writes
 * and reads them. Members are written in the order the specification gives;
 * what a host sends is read leniently, member by member, and a message that
 * cannot be understood is `undefined`, which the caller ignores (§4.5).
 */

import type { BundleManifest } from "./bundle.js";
import { parseObject, toWire } from "./json.js";

/** The protocol versions this library speaks: this version, and the one before it when there is one (§5.4). */
export const VERSIONS: readonly number[] = [1];

/** A feature a host lists in its `welcome` (§8.1). */
export const FEATURE_BUNDLE = "bundle";

/** The tool a view shows the result of, and what it is called with. */
export interface ToolCall {
  name: string;
  arguments?: Record<string, unknown>;
}

/** Why a program ends (`Bye.reason`). */
export type ByeReason = "exit" | "signal" | "error";

/** The payload of `hello`. */
export function helloPayload(versions: readonly number[] = VERSIONS): string {
  return toWire({ versions });
}

/** The payload of `open`: the token, the tool and, for a host that takes bundles, the manifest. */
export function openPayload(open: {
  token: string;
  tool: ToolCall;
  bundle?: BundleManifest | undefined;
}): string {
  const body: Record<string, unknown> = {
    token: open.token,
    tool: { name: open.tool.name, arguments: open.tool.arguments ?? {} },
  };
  if (open.bundle !== undefined) body.bundle = open.bundle;
  return toWire(body);
}

/** The payload of `bye`: the token the view was opened with, and why the program ends. */
export function byePayload(token: string, reason: ByeReason): string {
  return toWire({ token, reason });
}

/** What a host answers to `hello` (§5.2) or offers again unasked (§10.7). */
export type Welcome =
  | {
      readonly kind: "offer";
      readonly version: number;
      readonly token: string;
      readonly features: readonly string[];
      readonly restart: boolean;
    }
  | { readonly kind: "unsupported"; readonly supported: readonly number[] };

/** A token: 16 bytes in base64url without padding (§6.1). */
const TOKEN = /^[A-Za-z0-9_-]{22}$/;

const DIGEST = /^[0-9a-f]{64}$/;

function strings(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === "string")
    : [];
}

/**
 * Reads a `welcome`. `offered` is what the program's `hello` listed: a version it
 * did not offer is not one the host can have chosen, so such a host is one that
 * shares no version.
 */
export function parseWelcome(payload: string, offered: readonly number[]): Welcome | undefined {
  const body = parseObject(payload);
  if (body === undefined) return undefined;
  const { version, token } = body;
  if (version !== undefined) {
    if (typeof version !== "number" || !Number.isInteger(version) || version < 1) return undefined;
    if (typeof token !== "string" || !TOKEN.test(token)) return undefined;
    if (!offered.includes(version)) return { kind: "unsupported", supported: [] };
    return {
      kind: "offer",
      version,
      token,
      features: strings(body.features),
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

/** Reads a `need`: the digests the host lacks, in its order. */
export function parseNeed(payload: string): string[] | undefined {
  const body = parseObject(payload);
  if (body === undefined || !Array.isArray(body.missing)) return undefined;
  return strings(body.missing).filter((digest) => DIGEST.test(digest));
}

/** Why a host ended or refused a view (`Closed.reason`). */
export type ClosedReason = "refused" | "error" | "user" | "view";

/** What a host says when a view ended or was refused (§6.4, §10). */
export interface Closed {
  readonly reason: ClosedReason;
  /** `token`, `busy`, `bundle`, `mcp`, ...; a code this library does not know is no more than the reason. */
  readonly code?: string;
}

const REASONS: readonly string[] = ["refused", "error", "user", "view"];

/**
 * Reads a `closed`. A reason this version does not know still ends the view,
 * and is read as `error`: a host that has learned a new reason has no other way
 * to say that the view is gone.
 */
export function parseClosed(payload: string): Closed | undefined {
  const body = parseObject(payload);
  if (body === undefined || typeof body.reason !== "string") return undefined;
  const reason = REASONS.includes(body.reason) ? (body.reason as ClosedReason) : "error";
  return typeof body.code === "string" ? { reason, code: body.code } : { reason };
}
