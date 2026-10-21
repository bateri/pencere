import { describe, expect, it } from "vitest";

import { isFrameText } from "../src/frame.js";
import { escape, parseObject, toWire } from "../src/json.js";
import { manifest } from "./support.js";

describe("toWire", () => {
  it("writes DEL and everything from U+0080 up as lowercase \\u escapes", () => {
    expect(toWire("a\x7fb")).toBe('"a\\u007fb"');
    expect(toWire("é")).toBe('"\\u00e9"');
    expect(toWire("✀")).toBe('"\\u2700"');
    expect(toWire("\u{1f600}")).toBe('"\\ud83d\\ude00"');
    expect(toWire("\u0080\u009c")).toBe('"\\u0080\\u009c"');
  });

  it("leaves the escapes JSON itself writes, and the plain characters", () => {
    expect(toWire('a"b\\c\b\t\n\f\r\u0001')).toBe('"a\\"b\\\\c\\b\\t\\n\\f\\r\\u0001"');
    expect(toWire("plain ~ text")).toBe('"plain ~ text"');
  });

  it("writes a lone surrogate as an escape, as well-formed JSON does", () => {
    expect(isFrameText(toWire("\ud800"))).toBe(true);
    expect(JSON.parse(toWire("\ud800"))).toBe("\ud800");
  });

  it("is compact, and keeps the order of the members", () => {
    expect(toWire({ b: 1, a: [1, { c: null }], d: "x" })).toBe(
      '{"b":1,"a":[1,{"c":null}],"d":"x"}',
    );
  });

  it("writes only what a frame may carry, whatever the text", () => {
    const text = Array.from({ length: 0x3000 }, (_, i) => String.fromCharCode(i)).join("");
    expect(isFrameText(toWire({ text, [text]: text }))).toBe(true);
    expect(JSON.parse(toWire(text))).toBe(text);
  });

  it("has no text for what JSON cannot say", () => {
    expect(() => toWire(undefined)).toThrow(TypeError);
  });

  it("writes the JSON payloads of the fixtures as the fixtures have them", () => {
    let checked = 0;
    for (const entry of manifest().decode) {
      if (!entry.canonical) continue;
      for (const message of entry.messages) {
        if (message.kind === "file") continue;
        expect(toWire(JSON.parse(message.payload)), entry.file).toBe(message.payload);
        checked += 1;
      }
    }
    expect(checked).toBeGreaterThanOrEqual(14);
    // The one with non-ASCII text in it among them.
    const chunked = manifest().decode.find((e) => e.file === "to-program/msg-chunked.frame");
    expect(chunked?.messages[0]?.payload).toContain("\\u");
  });
});

describe("escape", () => {
  it("repairs text that `JSON.stringify` wrote", () => {
    expect(escape(JSON.stringify({ text: "é\x7f" }))).toBe('{"text":"\\u00e9\\u007f"}');
  });
});

describe("parseObject", () => {
  it("takes an object and nothing else", () => {
    expect(parseObject('{"a":1}')).toEqual({ a: 1 });
    for (const text of ["[]", "null", "1", '"x"', "true", "not json", "", "{"])
      expect(parseObject(text), text).toBeUndefined();
  });
});
