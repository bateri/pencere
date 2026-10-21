import { describe, expect, it } from "vitest";

import { Tail, plain } from "../src/tail.js";

describe("the tail of what a server wrote", () => {
  it("keeps what fits, as it was written", () => {
    const tail = new Tail(100);
    tail.push("one\ntwo\n");
    tail.push(Buffer.from("three\n"));
    expect(tail.text()).toBe("one\ntwo\nthree\n");
    expect(tail.empty).toBe(false);
    expect(new Tail().empty).toBe(true);
  });

  it("keeps the end of what overflows, in whole lines, and says that there was more", () => {
    const tail = new Tail(20);
    for (let line = 1; line <= 10; line += 1) tail.push(`line ${line}\n`);
    const text = tail.text();
    expect(text.startsWith("[earlier output left out]\n")).toBe(true);
    const lines = text.split("\n").slice(1, -1);
    expect(lines.at(-1)).toBe("line 10");
    // Whole lines only: nothing that began in what was dropped.
    for (const line of lines) expect(line).toMatch(/^line \d+$/);
    expect(text.length).toBeLessThan(20 + 30);
  });

  it("keeps the end of a single line longer than all of it", () => {
    const tail = new Tail(10);
    tail.push(`${"a".repeat(50)}END\n`);
    expect(tail.text()).toContain("END");
    expect(tail.text().length).toBeLessThan(50);
    tail.push("b".repeat(30));
    expect(tail.text().endsWith("b".repeat(10))).toBe(true);
  });

  it("does not split a character between two chunks", () => {
    const tail = new Tail(100);
    const bytes = Buffer.from("→ ok\n");
    tail.push(bytes.subarray(0, 2));
    tail.push(bytes.subarray(2));
    expect(tail.text()).toBe("→ ok\n");
  });

  it("shows text, not commands to the terminal", () => {
    expect(plain("a\tb\r\nc\rd")).toBe("a\tb\nc\nd");
    expect(plain("\u001b]52;c;eA==\u0007\u001b_pencere;open\u001b\\")).toBe(
      "\\u001b]52;c;eA==\\u0007\\u001b_pencere;open\\u001b\\",
    );
    expect(plain("\u009b31m\u007f")).toBe("\\u009b31m\\u007f");
    const tail = new Tail();
    tail.push("\u001b[2J boom\n");
    expect(tail.text()).toBe("\\u001b[2J boom\n");
  });
});
