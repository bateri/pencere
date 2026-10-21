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
import type { DecodeEvent, Direction } from "../src/frame.js";
import { fixture, manifest } from "./support.js";
import type { DecodeEntry } from "./support.js";

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
        s.steps.flatMap((step) => ("in" in step ? [step.in] : step.out === null ? [] : [step.out])),
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
    expect(entries.length).toBeGreaterThanOrEqual(52);
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
    expect(canonical.length).toBeGreaterThanOrEqual(19);
    for (const entry of canonical)
      expect(written(entry, "fill"), entry.file).toBe(fixture(entry.file).toString("latin1"));
  });

  it("is the two chunked examples when the sender picks those sizes", () => {
    for (const [file, size] of [
      ["to-host/file-chunked.frame", 60],
      ["to-host/msg-chunked.frame", 100],
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
