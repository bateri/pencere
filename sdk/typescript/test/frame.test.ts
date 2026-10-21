/** The decoder where the fixtures do not reach: what is noise, what ends a string, and the order of it all. */

import { describe, expect, it } from "vitest";

import { StreamDecoder, WriteError, encode } from "../src/frame.js";
import type { DecodeEvent } from "../src/frame.js";

const bytes = (text: string): Uint8Array => Buffer.from(text, "latin1");
const HELLO = "\x1b_pencere;hello;id=1;{}\x1b\\";

function run(text: string, direction: "to-host" | "to-program" = "to-host") {
  const noise: number[] = [];
  const decoder = new StreamDecoder(direction, { noise: (b) => noise.push(b) });
  const events: DecodeEvent[] = [];
  decoder.feed(bytes(text), (e) => events.push(e));
  return {
    events,
    noise: Buffer.from(noise).toString("latin1"),
    kinds: events.map((e) =>
      e.type === "message" ? `${e.message.kind}#${e.message.id}` : e.error,
    ),
  };
}

describe("a string that ends before pencere; is complete is not ours, however it ends", () => {
  for (const [name, text] of [
    ["ESC _ then ST", "\x1b_\x1b\\"],
    ["a partial prefix then ST", "\x1b_penc\x1b\\"],
    ["a partial prefix then another ESC _", "\x1b_penc\x1b_"],
    ["a partial prefix then ESC ESC", "\x1b_penc\x1b\x1b"],
    ["a partial prefix then ESC and a letter", "\x1b_penc\x1bx"],
    ["a partial prefix then a C0 control", "\x1b_penc\x01rest"],
    ["a partial prefix then a byte above 0x7e", "\x1b_penc\x9crest"],
    ["another protocol's string", "\x1b_Gi=1,a=q;AAAA\x1b\\"],
  ] as const) {
    it(`${name}: no message, no error`, () => {
      expect(run(text).kinds).toEqual([]);
      expect(run(text + HELLO).kinds).toEqual(["hello#1"]);
    });
  }

  it("a string cut by ESC ESC lets the second ESC begin the next one", () => {
    expect(run(`\x1b_penc\x1b${HELLO}`).kinds).toEqual(["hello#1"]);
  });
});

describe("a string whose prefix is complete is ours, and ends as the rules say", () => {
  it("a C0 control ends it as malformed, and scanning resumes after it", () => {
    expect(run(`\x1b_pencere;hello;id=1;{\x01}\x1b\\${HELLO}`).kinds).toEqual([
      "malformed",
      "hello#1",
    ]);
  });

  it("ESC followed by anything but a backslash ends it as malformed", () => {
    expect(run(`\x1b_pencere;hello;id=1;{}\x1b[c${HELLO}`).kinds).toEqual(["malformed", "hello#1"]);
  });

  it("ESC _ ends it as malformed and starts a new frame", () => {
    expect(run(`\x1b_pencere;hello;id=1;{}${HELLO}`).kinds).toEqual(["malformed", "hello#1"]);
  });

  it("ESC ESC ends it as malformed and the second begins what follows", () => {
    expect(run(`\x1b_pencere;hello;id=1;{}\x1b${HELLO}`).kinds).toEqual(["malformed", "hello#1"]);
  });

  it("a byte over the limit ends it as too-long, and the rest of it is noise", () => {
    const text = `\x1b_pencere;msg;id=1;${"a".repeat(600)}\x1b\\${HELLO}`;
    const result = run(text, "to-program");
    expect(result.kinds).toEqual(["too-long", "hello#1"]);
    expect(result.noise).toMatch(/^a+\x1b\\$/);
  });

  it("the grammar is held to: a payload may hold semicolons, a control may not hold spaces", () => {
    expect(run("\x1b_pencere;msg;id=1;a;b;c\x1b\\").events).toEqual([
      {
        type: "message",
        message: { kind: "msg", id: 1, control: {}, payload: "a;b;c" },
      },
    ]);
    expect(run("\x1b_pencere;msg;id=1, seq=1;x\x1b\\").kinds).toEqual(["malformed"]);
    expect(run("\x1b_pencere;msg;id=01;x\x1b\\").kinds).toEqual(["malformed"]);
    expect(run("\x1b_pencere;msg;id=4294967296;x\x1b\\").kinds).toEqual(["malformed"]);
    expect(run("\x1b_pencere;msg;id=1,re=0;x\x1b\\").kinds).toEqual(["malformed"]);
    expect(run("\x1b_pencere;msg;id=1,d=ABC;x\x1b\\").kinds).toEqual(["malformed"]);
    expect(run("\x1b_pencere;msg;;x\x1b\\").kinds).toEqual(["malformed"]);
    expect(run("\x1b_pencere;msg;id=1\x1b\\").kinds).toEqual(["malformed"]);
  });

  it("a known key out of range is malformed on every chunk, not only the first", () => {
    const text =
      "\x1b_pencere;msg;id=1,more=1;a\x1b\\" + "\x1b_pencere;msg;id=1,seq=1,re=0;b\x1b\\";
    // The second chunk is malformed, so the message it would have continued is never ended.
    expect(run(text).kinds).toEqual(["malformed"]);
  });
});

describe("what is noise", () => {
  it("is every byte outside a frame, in stream order with the frames", () => {
    const result = run(`ls\r\x1b[?62;4c${HELLO}\x1b[Aok`);
    expect(result.noise).toBe("ls\r\x1b[?62;4c\x1b[Aok");
    expect(result.kinds).toEqual(["hello#1"]);
  });

  it("is told to the hook between the events, not after them", () => {
    const order: string[] = [];
    const decoder = new StreamDecoder("to-program", {
      noise: (b) => order.push(String.fromCharCode(b)),
    });
    decoder.feed(bytes(`a\x1b_pencere;msg;id=1;x\x1b\\b`), (e) => order.push(e.type));
    expect(order).toEqual(["a", "message", "b"]);
  });

  it("is the same wherever the stream is cut", () => {
    const text = `ls\x1b\x1b[?62;4c${HELLO}z`;
    const whole = run(text);
    const noise: number[] = [];
    const decoder = new StreamDecoder("to-host", { noise: (b) => noise.push(b) });
    const events: DecodeEvent[] = [];
    for (const byte of bytes(text)) decoder.feed(Uint8Array.of(byte), (e) => events.push(e));
    expect(events).toEqual(whole.events);
    expect(Buffer.from(noise).toString("latin1")).toBe(whole.noise);
  });
});

describe("messages out of chunks", () => {
  it("a chunk that starts a message while another is open drops the old one", () => {
    const text = "\x1b_pencere;msg;id=1,more=1;a\x1b\\" + "\x1b_pencere;msg;id=2;b\x1b\\";
    expect(run(text).kinds).toEqual(["abandoned", "msg#2"]);
  });

  it("keeps the first chunk's parameters and ignores the later chunks' own", () => {
    const text =
      "\x1b_pencere;file;id=3,more=1,re=5;ab\x1b\\" + "\x1b_pencere;file;id=3,seq=1,x=1;cd\x1b\\";
    expect(run(text).events).toEqual([
      {
        type: "message",
        message: { kind: "file", id: 3, control: { re: "5" }, payload: "abcd" },
      },
    ]);
  });
});

describe("the writer", () => {
  it("refuses what a frame may not carry, and writes nothing", () => {
    const refuse = (message: Parameters<typeof encode>[1], code: string): void => {
      try {
        encode("to-host", message);
        expect.unreachable();
      } catch (error) {
        expect(error).toBeInstanceOf(WriteError);
        expect((error as WriteError).code).toBe(code);
      }
    };
    refuse({ kind: "Msg", id: 1, payload: "" }, "kind");
    refuse({ kind: "msg", id: 0, payload: "" }, "id");
    refuse({ kind: "msg", id: 1, re: 0, payload: "" }, "id");
    refuse({ kind: "msg", id: 1, digest: "xyz", payload: "" }, "digest");
    for (const bad of ["\n", "\r", "\x03", "\x1b", "\x7f", "\u0080", "é"])
      refuse({ kind: "msg", id: 1, payload: `a${bad}b` }, "payload");
  });

  it("puts the keys in the order id, seq, more, re, d and leaves out what is 0", () => {
    const text = Buffer.from(
      encode(
        "to-host",
        { kind: "file", id: 7, re: 3, digest: "a".repeat(64), payload: "A".repeat(10) },
        { fixed: 4 },
      ),
    ).toString("latin1");
    expect(text.split("\x1b\\").slice(0, -1)).toEqual([
      `\x1b_pencere;file;id=7,more=1,re=3,d=${"a".repeat(64)};AAAA`,
      "\x1b_pencere;file;id=7,seq=1,more=1;AAAA",
      "\x1b_pencere;file;id=7,seq=2;AA",
    ]);
  });

  it("cuts a file on four-character groups, whatever the frame has room for", () => {
    const payload = Buffer.alloc(6000, 1).toString("base64");
    const frames = Buffer.from(encode("to-host", { kind: "file", id: 1, payload })).toString(
      "latin1",
    );
    const parts = frames.split("\x1b\\").slice(0, -1);
    expect(parts.length).toBeGreaterThan(1);
    for (const part of parts.slice(0, -1)) {
      expect(part.length + 2).toBeLessThanOrEqual(4096);
      expect(part.slice(part.lastIndexOf(";") + 1).length % 4).toBe(0);
    }
  });
});
