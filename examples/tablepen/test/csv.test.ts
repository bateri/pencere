/** CSV as RFC 4180 writes it, read and written back. */

import { describe, expect, it } from "vitest";

import { format, parse } from "../src/csv.js";

describe("parse", () => {
  it("reads records of fields, with LF or CRLF, and no record for the last line break", () => {
    expect(parse("name,size\nweb,3\r\ndb,40\n")).toEqual([
      ["name", "size"],
      ["web", "3"],
      ["db", "40"],
    ]);
    expect(parse("a,b")).toEqual([["a", "b"]]);
    expect(parse("")).toEqual([]);
  });

  it("reads quoted fields with commas, quotes and line breaks in them, and empty fields", () => {
    expect(parse('"a, b","say ""hi""","two\nlines",\n,x,\n')).toEqual([
      ["a, b", 'say "hi"', "two\nlines", ""],
      ["", "x", ""],
    ]);
  });

  it("keeps a quote in the middle of an unquoted field as it is", () => {
    expect(parse('5" disk,ok\n')).toEqual([['5" disk', "ok"]]);
  });
});

describe("format", () => {
  it("quotes only what has to be, and is read back as it was", () => {
    const records = [
      ["name", "note"],
      ["web", "plain"],
      ["db", 'has, a comma and "quotes"'],
      ["cache", "two\nlines"],
      ["edge", " padded "],
    ];
    const text = format(records);
    expect(text.split("\n")[1]).toBe("web,plain");
    expect(text).toContain('"has, a comma and ""quotes"""');
    expect(parse(text)).toEqual(records);
  });
});
