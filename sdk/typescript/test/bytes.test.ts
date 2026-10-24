import { describe, expect, it } from "vitest";

import {
  compareBytes,
  latin1,
  latin1Bytes,
  toBase64,
  utf8Bytes,
  utf8Length,
} from "../src/bytes.js";

const everyByte = Uint8Array.from({ length: 256 }, (_, at) => at);

describe("latin1", () => {
  it("is one character a byte, for every byte (not windows-1252, which `TextDecoder` calls latin1)", () => {
    const text = latin1(everyByte);
    expect(text).toHaveLength(256);
    expect(text).toBe(Buffer.from(everyByte).toString("latin1"));
    expect(text.charCodeAt(0x80)).toBe(0x80);
    expect(text.charCodeAt(0x9c)).toBe(0x9c);
  });

  it("takes a range, and a range longer than a call can pass at once", () => {
    expect(latin1(Uint8Array.from([65, 66, 67, 68]), 1, 3)).toBe("BC");
    const long = new Uint8Array(100_000).fill(0x61);
    expect(latin1(long)).toBe("a".repeat(100_000));
    expect(latin1(long, 10, 20_010)).toBe("a".repeat(20_000));
  });

  it("is the inverse of latin1Bytes, and keeps the low byte of what is above U+00FF", () => {
    expect([...latin1Bytes(latin1(everyByte))]).toEqual([...everyByte]);
    expect([...latin1Bytes("AŁ€")]).toEqual([0x41, 0x41, 0xac]);
    expect([...latin1Bytes("")]).toEqual([]);
  });
});

describe("utf8", () => {
  it("counts bytes, not characters", () => {
    for (const text of ["", "abc", "ç", "日本語", "😀", "a\ud800b"]) {
      expect(utf8Length(text), text).toBe(Buffer.byteLength(text));
      expect([...utf8Bytes(text)], text).toEqual([...Buffer.from(text)]);
    }
  });
});

describe("compareBytes", () => {
  it("orders as bytes do, a shorter prefix first", () => {
    const order = (a: string, b: string) => Math.sign(compareBytes(utf8Bytes(a), utf8Bytes(b)));
    expect(order("a", "b")).toBe(-1);
    expect(order("b", "a")).toBe(1);
    expect(order("a", "a")).toBe(0);
    expect(order("a", "ab")).toBe(-1);
    expect(order("ab", "a")).toBe(1);
    // As UTF-8 bytes, not as UTF-16 code units: U+FF5E is EF BD 9E, U+1F600 is F0 9F 98 80.
    expect(order("～", "\u{1f600}")).toBe(-1);
    for (const [a, b] of [
      ["index.html", "app.js"],
      ["app.js", "style.css"],
      ["z", "é"],
    ] as const)
      expect(order(a, b)).toBe(Math.sign(Buffer.compare(Buffer.from(a), Buffer.from(b))));
  });
});

describe("toBase64", () => {
  it("is base64 with padding, as Node writes it, for every length around a slice", () => {
    for (const length of [0, 1, 2, 3, 4, 5, 255, 256, 12_287, 12_288, 12_289, 12_290, 100_001]) {
      const data = Uint8Array.from({ length }, (_, at) => (at * 31 + 7) % 256);
      expect(toBase64(data), `${length} bytes`).toBe(Buffer.from(data).toString("base64"));
    }
  });

  it("writes every byte value", () => {
    expect(toBase64(everyByte)).toBe(Buffer.from(everyByte).toString("base64"));
  });
});
