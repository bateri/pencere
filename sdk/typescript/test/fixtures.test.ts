/**
 * Every `decode` entry of the fixtures' manifest, fed whole, cut at every byte
 * and one byte at a time; every `canonical` entry written back from its
 * messages. The Rust crate runs the same entries (`host/rust/tests/fixtures.rs`):
 * two implementations that pass agree on the wire.
 */

import { readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { LIMITS, StreamDecoder, encode } from "../src/frame.js";
import type { DecodeEvent, Direction, Message } from "../src/frame.js";
import {
  VERSIONS,
  byePayload,
  callPayload,
  cancelPayload,
  chooseView,
  dismissPayload,
  failurePayload,
  helloPayload,
  noVersionSentence,
  openPayload,
  parseClosed,
  parseNeed,
  parseViewRequest,
  parseWelcome,
  progressPayload,
  pushPayload,
  readViews,
  responsePayload,
  subscribePayload,
} from "../src/messages.js";
import type { OpenFields, ViewOffer } from "../src/messages.js";
import { fixture, manifest, payloadOf } from "./support.js";
import type { DecodeEntry, SessionEntry } from "./support.js";

function summary(events: readonly DecodeEvent[]) {
  const messages = [];
  const errors = [];
  for (const event of events) {
    if (event.type === "message") messages.push(event.message);
    else errors.push(event.error);
  }
  return { messages, errors };
}

function decode(direction: Direction, pieces: readonly Uint8Array[]): DecodeEvent[] {
  const decoder = new StreamDecoder(direction);
  return pieces.flatMap((piece) => decoder.collect(piece));
}

const entries = manifest().decode;

describe("the manifest", () => {
  it("lists what the directory holds", () => {
    const root = fileURLToPath(new URL("../../../spec/draft/fixtures/", import.meta.url));
    const found: string[] = [];
    const walk = (dir: string): void => {
      for (const name of readdirSync(dir)) {
        const path = join(dir, name);
        if (statSync(path).isDirectory()) walk(path);
        else if (name.endsWith(".frame")) found.push(relative(root, path));
      }
    };
    walk(root);
    const listed = [
      ...entries.map((e) => e.file),
      ...manifest().session.flatMap((s) =>
        s.steps.flatMap((step) =>
          "in" in step ? [step.in] : "out" in step && step.out !== null ? [step.out] : [],
        ),
      ),
    ];
    expect(listed.sort()).toEqual(found.sort());
  });

  it("names only errors the decoder has", () => {
    const known = ["malformed", "too-long", "chunk-order", "abandoned", "message-too-long"];
    for (const entry of entries) for (const name of entry.errors) expect(known).toContain(name);
    expect(manifest().protocol).toBe(1);
  });
});

describe("every entry decodes as the manifest says, however the stream is cut", () => {
  for (const entry of entries) {
    it(entry.file, () => {
      const bytes = fixture(entry.file);
      const expected = { messages: entry.messages, errors: entry.errors };
      const whole = decode(entry.direction, [bytes]);
      expect(summary(whole)).toEqual(expected);
      for (let at = 0; at <= bytes.length; at++) {
        const cut = decode(entry.direction, [bytes.subarray(0, at), bytes.subarray(at)]);
        expect(cut, `cut at ${at}`).toEqual(whole);
      }
      const singles = Array.from(bytes, (byte) => Uint8Array.of(byte));
      expect(decode(entry.direction, singles)).toEqual(whole);
    });
  }

  it("covers the entries the Rust crate covers", () => {
    expect(entries.length).toBeGreaterThanOrEqual(72);
  });
});

function written(entry: DecodeEntry, split: "fill" | { fixed: number }): string {
  return entry.messages
    .map((m) => {
      const re = m.control.re;
      const d = m.control.d;
      const bytes = encode(
        entry.direction,
        {
          kind: m.kind,
          id: m.id,
          payload: m.payload,
          ...(re === undefined ? {} : { re: Number(re) }),
          ...(d === undefined ? {} : { digest: d }),
        },
        split,
      );
      return Buffer.from(bytes).toString("latin1");
    })
    .join("");
}

describe("what the writer writes", () => {
  const canonical = entries.filter((e) => e.canonical);

  it("is every canonical entry, byte for byte", () => {
    expect(canonical.length).toBeGreaterThanOrEqual(32);
    for (const entry of canonical)
      expect(written(entry, "fill"), entry.file).toBe(fixture(entry.file).toString("latin1"));
  });

  it("is the two chunked examples when the sender picks those sizes", () => {
    for (const [file, size] of [
      ["to-host/file-chunked.frame", 60],
      ["to-host/msg-chunked.frame", 59],
    ] as const) {
      const entry = entries.find((e) => e.file === file);
      expect(entry, file).toBeDefined();
      expect(written(entry as DecodeEntry, { fixed: size }), file).toBe(
        fixture(file).toString("latin1"),
      );
    }
  });
});

describe("limits that do not fit in a file", () => {
  const chunks = (direction: Direction, total: number, per: number): Buffer[] => {
    const frames: Buffer[] = [];
    let sent = 0;
    for (let seq = 0; sent < total; seq++) {
      const size = Math.min(per, total - sent);
      sent += size;
      const control = `id=9${seq > 0 ? `,seq=${seq}` : ""}${sent < total ? ",more=1" : ""}`;
      frames.push(Buffer.from(`\x1b_pencere;msg;${control};${"a".repeat(size)}\x1b\\`, "latin1"));
    }
    expect(direction).toBeDefined();
    return frames;
  };

  for (const direction of ["to-host", "to-program"] as const) {
    const { message, frame } = LIMITS[direction];
    it(`${direction}: a message of ${message} bytes is delivered and one byte more is dropped`, () => {
      const per = frame - 40;
      const exact = decode(direction, chunks(direction, message, per));
      expect(summary(exact).errors).toEqual([]);
      expect(summary(exact).messages[0]?.payload.length).toBe(message);
      const over = decode(direction, chunks(direction, message + 1, per));
      expect(summary(over)).toEqual({ messages: [], errors: ["message-too-long"] });
      // The rest of a dropped message is chunks of a message nobody has open.
      const more = decode(direction, [...chunks(direction, message + 2 * per, per)]);
      expect(summary(more).errors[0]).toBe("message-too-long");
      expect(
        summary(more)
          .errors.slice(1)
          .every((e) => e === "chunk-order"),
      ).toBe(true);
    });

    it(`${direction}: the writer refuses a message over the limit and a frame is never over its own`, () => {
      expect(() =>
        encode(direction, { kind: "msg", id: 1, payload: "a".repeat(message + 1) }),
      ).toThrow(/message limit/);
      const bytes = encode(direction, { kind: "msg", id: 1, payload: "a".repeat(message) });
      const events = decode(direction, [bytes]);
      expect(summary(events)).toEqual({
        messages: [expect.objectContaining({ kind: "msg", id: 1 })],
        errors: [],
      });
    });
  }
});

// ---- the sessions: what a host and a program say to each other, read with this library ----------

function messagesOf(direction: Direction, file: string): Message[] {
  const events = new StreamDecoder(direction).collect(fixture(file));
  expect(
    events.filter((e) => e.type === "error"),
    file,
  ).toEqual([]);
  return events.flatMap((e) => (e.type === "message" ? [e.message] : []));
}

function json(payload: string): Record<string, unknown> {
  return JSON.parse(payload) as Record<string, unknown>;
}

/** The payload this library writes for what `payload` says, if it writes that kind of message. */
function rewritten(message: Message): string | undefined {
  const body = json(message.payload);
  switch (message.kind) {
    case "hello":
      return helloPayload(body.views as ViewOffer[], body.versions as number[]);
    case "open":
      return openPayload(body as unknown as OpenFields);
    case "bye":
      return byePayload(body.token as string, body.reason as "exit" | "signal" | "error");
    case "dismiss":
      return dismissPayload(body.token as string);
    case "msg": {
      const [name] = Object.keys(body);
      const inner = body[name ?? ""] as Record<string, unknown>;
      const id = inner.id as number;
      switch (name) {
        case "call":
          return callPayload(id, inner.method as string, inner.params);
        case "subscribe":
          return subscribePayload(id, inner.topic as string, inner.params);
        case "cancel":
          return cancelPayload(id);
        case "push":
          return pushPayload(id, inner.value);
        case "progress":
          return progressPayload(
            id,
            inner.done as number,
            inner.total as number | undefined,
            inner.note as string | undefined,
          );
        case "response": {
          const error = inner.error as { code: string; message?: string } | undefined;
          return error === undefined
            ? responsePayload(id, inner.result)
            : failurePayload(id, error.code, error.message);
        }
      }
    }
  }
  return undefined;
}

/** The view each hello and its welcome come to in the sessions that are about choosing (§5.3). */
const CHOSEN: Readonly<Record<string, { kind: string; version: string } | null>> = {
  "no-common-version#1": { kind: "web", version: "1.0" },
  "no-common-version#2": null,
  "several-versions#1": { kind: "wasm", version: "0.5" },
  "several-versions#2": { kind: "wasm", version: "0.4" },
  "several-versions#3": { kind: "wasm", version: "0.4" },
  "handshake#1": { kind: "web", version: "1.0" },
};

/**
 * The sessions in which the program is not this library but a hostile one, or one that tests a
 * limit: what it writes is not what this library writes.
 */
const HOSTILE = new Set([
  "hello-edges",
  "kit-open-ignores-access",
  "bundle-limits",
  "bundle-too-many-files",
  "bundle-corrupt",
]);

describe("what is read of the messages a view says", () => {
  const entries = manifest().message;

  it("is every entry of the manifest, as it says", () => {
    expect(entries.length).toBeGreaterThanOrEqual(9);
    for (const entry of entries) {
      const request = parseViewRequest(fixture(entry.file).toString("utf8"));
      expect(
        request === undefined ? null : { kind: request.type, id: request.id },
        entry.file,
      ).toEqual(entry.reads);
    }
  });
});

describe("every session of the manifest", () => {
  const sessions: SessionEntry[] = manifest().session;

  it("is read by this library, and the program's side is written as the fixtures have it", () => {
    expect(sessions.length).toBeGreaterThanOrEqual(34);
    for (const session of sessions) {
      let hello: Message | undefined;
      let hellos = 0;
      for (const step of session.steps) {
        if ("in" in step) {
          for (const message of messagesOf("to-host", step.in)) {
            const label = `${session.name} ${step.in} ${message.kind}#${message.id}`;
            // `file` is base64, and the frames the program writes besides are its own words.
            if (
              message.kind !== "file" &&
              !HOSTILE.has(session.name) &&
              rewritten(message) !== undefined
            )
              expect(rewritten(message), label).toBe(message.payload);
            if (message.kind === "hello") {
              hello = message;
              hellos++;
            }
            if (message.kind === "msg") {
              const request = parseViewRequest(message.payload);
              // What the program says is not what a view says: the three are the view's.
              expect(
                request === undefined || ["call", "subscribe", "cancel"].includes(request.type),
              ).toBe(true);
            }
          }
        } else if ("out" in step && step.out !== null) {
          for (const message of messagesOf("to-program", step.out)) {
            const label = `${session.name} ${step.out} ${message.kind}#${message.id}`;
            const payload = message.payload;
            switch (message.kind) {
              case "welcome": {
                const welcome = parseWelcome(payload, VERSIONS);
                expect(welcome, label).toBeDefined();
                if (welcome?.kind === "offer") {
                  expect(session.config.tokens, label).toContain(welcome.token);
                  expect(welcome.views, label).toEqual(readViews(json(payload).views));
                  // The answer to a hello keeps the program's order and lists no version the program did not.
                  const asked = readViews(json(hello?.payload ?? "{}").views);
                  for (const offered of welcome.views) {
                    const mine = asked.find((view) => view.kind === offered.kind);
                    expect(mine, label).toBeDefined();
                    for (const version of offered.versions)
                      expect(mine?.versions, label).toContain(version);
                    expect(offered.versions, label).toEqual(
                      (mine?.versions ?? []).filter((v) => offered.versions.includes(v)),
                    );
                  }
                  const key = `${session.name}#${hellos}`;
                  if (key in CHOSEN) {
                    const choice = chooseView(asked, welcome.views) ?? null;
                    expect(choice, key).toEqual(CHOSEN[key]);
                  }
                }
                break;
              }
              case "need":
                expect(parseNeed(payload), label).toBeDefined();
                break;
              case "closed":
                expect(parseClosed(payload), label).toBeDefined();
                break;
              case "msg":
                // The words of a page, which the host passes on as they came.
                expect(parseViewRequest(payload), label).toBeDefined();
                break;
              default:
                throw new Error(`${label}: a host writes no such message to a program`);
            }
          }
        }
      }
    }
  });

  it("reads the closed of each session as its note says: a re when it answers something, none when the view ran", () => {
    const closedOf = (name: string) =>
      (sessions.find((s) => s.name === name)?.steps ?? [])
        .flatMap((step) =>
          "out" in step && step.out !== null ? messagesOf("to-program", step.out) : [],
        )
        .filter((m) => m.kind === "closed")
        .map((m) => ({ re: m.control.re, ...parseClosed(m.payload) }));
    expect(closedOf("view-ends")).toEqual([
      { re: undefined, reason: "user" },
      { re: undefined, reason: "view" },
    ]);
    expect(closedOf("page-message-too-long")).toEqual([
      {
        re: undefined,
        reason: "error",
        code: "view",
        message: "a message of the view was over 1 MiB",
      },
    ]);
    expect(closedOf("files-timeout")).toEqual([{ re: "2", reason: "error", code: "timeout" }]);
    expect(closedOf("consent-timeout")).toEqual([{ re: "2", reason: "error", code: "timeout" }]);
    expect(closedOf("bundle-corrupt")).toEqual([{ re: "2", reason: "error", code: "bundle" }]);
    expect(closedOf("screen-refused")).toEqual([{ re: "2", reason: "refused", code: "screen" }]);
    expect(closedOf("policy-refused")).toEqual([{ re: "2", reason: "refused", code: "policy" }]);
    expect(closedOf("bundle-limits").map((c) => c.code)).toEqual(["limit", "limit"]);
    expect(closedOf("bundle-too-many-files").map((c) => c.code)).toEqual(["limit"]);
    // Answering a dismiss, not an open.
    expect(closedOf("messages-outside-a-view")).toEqual([{ re: "7", reason: "program" }]);
  });

  it("reads the restart of a view as an offer with a new token and no hello it answers", () => {
    const session = sessions.find((s) => s.name === "restart");
    const welcomes = (session?.steps ?? [])
      .flatMap((step) =>
        "out" in step && step.out !== null ? messagesOf("to-program", step.out) : [],
      )
      .filter((m) => m.kind === "welcome");
    expect(welcomes).toHaveLength(2);
    const [first, restart] = welcomes.map((m) => parseWelcome(m.payload, VERSIONS));
    expect(first).toMatchObject({ kind: "offer", restart: false });
    expect(restart).toMatchObject({ kind: "offer", restart: true });
    expect(welcomes[1]?.control.re).toBeUndefined();
    expect(
      restart?.kind === "offer" && first?.kind === "offer" && restart.token !== first.token,
    ).toBe(true);
  });

  it("says in words which versions would have served, from the welcome that names them (§5.2)", () => {
    const welcome = parseWelcome(payloadOf("to-program/welcome-opens.frame"), VERSIONS);
    expect(welcome).toMatchObject({
      kind: "offer",
      views: [{ kind: "web", versions: ["1.0"] }],
      opens: [{ kind: "wasm", versions: ["1.0"] }],
    });
    if (welcome?.kind !== "offer") throw new Error("an offer");
    const mine: ViewOffer[] = [
      { kind: "wasm", versions: ["2.0"] },
      { kind: "web", versions: ["1.0"] },
    ];
    expect(chooseView(mine, welcome.views)).toEqual({ kind: "web", version: "1.0" });
    expect(noVersionSentence(mine, welcome.opens)).toBe(
      "this program is written for wasm 2.0; this terminal opens wasm 1.0",
    );
  });

  it("answers a need with the files it names: the digests are the manifest's, in its order", () => {
    const session = sessions.find((s) => s.name === "bundle-partial");
    expect(session).toBeDefined();
    const files = (session?.steps ?? []).flatMap((step) =>
      "in" in step ? messagesOf("to-host", step.in) : [],
    );
    const open = files.find((m) => m.kind === "open");
    const manifestFiles = (json(open?.payload ?? "{}").bundle as { files: { sha256: string }[] })
      .files;
    const need = (session?.steps ?? [])
      .flatMap((step) =>
        "out" in step && step.out !== null ? messagesOf("to-program", step.out) : [],
      )
      .find((m) => m.kind === "need");
    const missing = parseNeed(need?.payload ?? "{}") ?? [];
    expect(missing.length).toBeGreaterThan(0);
    const wanted = manifestFiles
      .map((f) => f.sha256)
      .filter((digest) => !session?.config.store.includes(digest));
    expect(missing).toEqual(wanted);
  });
});
