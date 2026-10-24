import { mkdtempSync, openSync, closeSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";

import { synchronousOutput } from "../src/output.js";

const dir = mkdtempSync(join(tmpdir(), "pencere-output-"));
afterAll(() => rmSync(dir, { recursive: true, force: true }));

function written(
  action: (
    output: ReturnType<typeof synchronousOutput>,
    calls: (Error | null | undefined)[],
  ) => void,
): { text: string; calls: (Error | null | undefined)[] } {
  const path = join(dir, `file-${Math.random().toString(36).slice(2)}`);
  const fd = openSync(path, "w");
  const calls: (Error | null | undefined)[] = [];
  try {
    action(synchronousOutput(fd, true), calls);
  } finally {
    closeSync(fd);
  }
  return { text: readFileSync(path, "latin1"), calls };
}

describe("synchronousOutput", () => {
  it("writes bytes and text, and reports each write when it returns", () => {
    const { text, calls } = written((output, calls) => {
      expect(output.write(Uint8Array.from([0x1b, 0x5b, 0x63]), (error) => calls.push(error))).toBe(
        true,
      );
      output.write("ok", (error) => calls.push(error));
      output.write("no callback needed");
    });
    expect(text).toBe("\x1b[cokno callback needed");
    expect(calls).toEqual([null, null]);
  });

  it("says what the stream is, as the stream does", () => {
    expect(synchronousOutput(1, true).isTTY).toBe(true);
    expect(synchronousOutput(1, undefined).isTTY).toBeUndefined();
  });

  it("reports a write that fails, to the callback and not to the caller", () => {
    const closed = openSync(join(dir, "closed"), "w");
    closeSync(closed);
    const calls: (Error | null | undefined)[] = [];
    const output = synchronousOutput(closed, true);
    expect(output.write("x", (error) => calls.push(error))).toBe(false);
    expect(calls).toHaveLength(1);
    expect(calls[0]).toBeInstanceOf(Error);
  });

  it("takes up where a partial write stopped, and asks again after EAGAIN", () => {
    const taken: string[] = [];
    let refusals = 2;
    const output = synchronousOutput(9, true, (fd, bytes, offset, length) => {
      expect(fd).toBe(9);
      if (refusals > 0) {
        refusals -= 1;
        throw Object.assign(new Error("try again"), { code: "EAGAIN" });
      }
      const count = Math.min(length, 3);
      taken.push(Buffer.from(bytes.subarray(offset, offset + count)).toString("latin1"));
      return count;
    });
    const calls: (Error | null | undefined)[] = [];
    output.write("abcdefgh", (error) => calls.push(error));
    expect(taken).toEqual(["abc", "def", "gh"]);
    expect(refusals).toBe(0);
    expect(calls).toEqual([null]);
  });

  it("gives up on any other error", () => {
    const output = synchronousOutput(9, true, () => {
      throw Object.assign(new Error("gone"), { code: "EIO" });
    });
    const calls: (Error | null | undefined)[] = [];
    output.write("x", (error) => calls.push(error));
    expect(calls).toHaveLength(1);
    expect(calls[0]?.message).toBe("gone");
  });

  it("writes all of a large chunk", () => {
    const big = "x".repeat(3 * 1024 * 1024);
    expect(written((output) => output.write(big)).text).toBe(big);
  });
});
