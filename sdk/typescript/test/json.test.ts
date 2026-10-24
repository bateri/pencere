import { describe, expect, it } from "vitest";

import { isFrameText } from "../src/frame.js";
import { escape, parseObject, toWire, unescapedLength } from "../src/json.js";
import { manifest } from "./support.js";

describe("unescapedLength", () => {
  it("is the text's own length in bytes when it has no escapes", () => {
    expect(unescapedLength("")).toBe(0);
    expect(unescapedLength("plain ~ text")).toBe(12);
    // Characters that are not escaped count as their UTF-8.
    expect(unescapedLength("é")).toBe(2);
    expect(unescapedLength("✀")).toBe(3);
    expect(unescapedLength("😀")).toBe(4);
    expect(unescapedLength("a\ud800b")).toBe(1 + 3 + 1);
  });

  it("counts a \\uXXXX escape as the UTF-8 of the character it stands for, and a surrogate pair as one of four bytes", () => {
    expect(unescapedLength(String.raw`\ud83d\ude00`)).toBe(4);
    expect(unescapedLength(String.raw`\u00e9\u2700\u007f`)).toBe(2 + 3 + 1);
    expect(unescapedLength(String.raw`\u0041`)).toBe(1);
  });

  it("counts any other escape as the one character it stands for", () => {
    expect(unescapedLength(String.raw`\"\\\/\n`)).toBe(4);
  });

  it("reads text that is not JSON: a backslash that begins no escape counts as itself, and nothing is checked", () => {
    expect(unescapedLength(String.raw`\u12`)).toBe(4);
    expect(unescapedLength(String.raw`\ud83d`)).toBe(3);
    expect(unescapedLength(String.raw`\ud83dx`)).toBe(4);
    expect(unescapedLength(String.raw`\ud83d\u0041`)).toBe(3 + 1);
    expect(unescapedLength("\\")).toBe(1);
  });

  it("is the length of the text before toWire escaped it, in bytes, for any text", () => {
    for (const text of ["a", "é", "✀ and 😀", "Türkçe ğüşiöç İĞÜŞÖÇ", "\u007f", "tab\tline\n"]) {
      const json = JSON.stringify({ text });
      expect(unescapedLength(toWire({ text })), text).toBe(Buffer.byteLength(json) - extra(json));
    }
    // JSON.stringify writes control characters as escapes; the host counts one character for each.
    function extra(json: string): number {
      return (json.match(/\\["\\bfnrt]|\\u00[01][0-9a-f]/g) ?? []).reduce(
        (sum, escape) => sum + escape.length - 1,
        0,
      );
    }
  });
});

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
