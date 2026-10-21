import { EventEmitter } from "node:events";
import { describe, expect, it } from "vitest";

import { StreamDecoder, encode } from "../src/frame.js";
import type { DecodeEvent, Message } from "../src/frame.js";
import { Link, ReplyScanner } from "../src/link.js";
import type { Input, Output } from "../src/link.js";

class FakeInput extends EventEmitter implements Input {
  isTTY = true;
  paused = 0;
  resumed = 0;
  resume(): this {
    this.resumed += 1;
    return this;
  }
  pause(): this {
    this.paused += 1;
    return this;
  }
  push(bytes: Uint8Array | string): void {
    this.emit("data", typeof bytes === "string" ? Buffer.from(bytes, "latin1") : bytes);
  }
}

class FakeOutput extends EventEmitter implements Output {
  isTTY = true;
  /** One entry for each call of `write`. */
  readonly writes: Buffer[] = [];
  /** How long a write takes to end, in turns of the event loop; 0 is at once. */
  delay = 0;
  fail: Error | undefined;
  write(chunk: Uint8Array | string, callback?: (error?: Error | null) => void): boolean {
    this.writes.push(Buffer.from(chunk as Uint8Array));
    const done = (): void => callback?.(this.fail);
    if (this.delay === 0) done();
    else setTimeout(done, this.delay);
    return true;
  }
}

const frame = (kind: string, id: number, payload = "{}", re?: number): Buffer =>
  Buffer.from(encode("to-program", { kind, id, payload, ...(re === undefined ? {} : { re }) }));
const REPLY = "\x1b[?62;4c";

describe("the reply to CSI c", () => {
  const scan = (text: string): number => {
    const scanner = new ReplyScanner();
    let replies = 0;
    for (const char of text) if (scanner.feed(char.charCodeAt(0))) replies += 1;
    return replies;
  };

  it("is ESC [ ? with digits and semicolons, and c", () => {
    for (const reply of ["\x1b[?62;4c", "\x1b[?1;2c", "\x1b[?c", "\x1b[?64;1;2;6;9;15;18;21;22c"])
      expect(scan(reply), reply).toBe(1);
  });

  it("is found among typed characters, and again each time", () => {
    expect(scan(`ls\r${REPLY}x${REPLY}`)).toBe(2);
    expect(scan(`\x1b\x1b[?62c`)).toBe(1);
    expect(scan(`\x1b[\x1b[?62c`)).toBe(1);
  });

  it("is not another sequence", () => {
    for (const other of [
      "\x1b[A",
      "\x1b[?62;4",
      "\x1b[>0;95;0c",
      "\x1b[62;4c",
      "\x1b[?6x2c",
      "[?62c",
    ])
      expect(scan(other), other).toBe(0);
  });
});

describe("what the link reports", () => {
  function listen() {
    const input = new FakeInput();
    const output = new FakeOutput();
    const log: string[] = [];
    const link = new Link(input, output, {
      message: (m: Message) => log.push(`${m.kind}#${m.id}`),
      reply: () => log.push("reply"),
      dropped: (e) => log.push(`dropped:${e}`),
    });
    link.start();
    return { input, output, link, log };
  }

  it("is the reply and the messages in the order the bytes came", () => {
    const first = listen();
    first.input.push(Buffer.concat([Buffer.from(REPLY), frame("welcome", 1)]));
    expect(first.log).toEqual(["reply", "welcome#1"]);
    const second = listen();
    second.input.push(Buffer.concat([frame("welcome", 1), Buffer.from(`${REPLY}ls\r`)]));
    expect(second.log).toEqual(["welcome#1", "reply"]);
  });

  it("is the same when the bytes come one at a time, or as text", () => {
    const bytes = Buffer.concat([
      Buffer.from("ab"),
      Buffer.from(REPLY),
      frame("closed", 2, '{"reason":"user"}'),
    ]);
    const single = listen();
    for (const byte of bytes) single.input.push(Uint8Array.of(byte));
    const text = listen();
    text.input.push(bytes.toString("latin1"));
    expect(single.log).toEqual(["reply", "closed#2"]);
    expect(text.log).toEqual(single.log);
  });

  it("tells what was dropped", () => {
    const { input, log } = listen();
    input.push("\x1b_pencere;msg;id=1;a\nb\x1b\\");
    expect(log).toEqual(["dropped:malformed"]);
  });

  it("stops: nothing more is delivered, the input is paused and what is read is not kept", () => {
    const { input, link, log } = listen();
    expect(input.resumed).toBe(1);
    link.stop();
    expect(input.paused).toBe(1);
    expect(input.listenerCount("data")).toBe(0);
    input.push(frame("welcome", 1));
    expect(log).toEqual([]);
    link.start();
    input.push(frame("welcome", 2));
    expect(log).toEqual(["welcome#2"]);
  });

  it("stops in the middle of what one read held", () => {
    const input = new FakeInput();
    const log: string[] = [];
    const link: Link = new Link(input, new FakeOutput(), {
      message: (m) => {
        log.push(`${m.kind}#${m.id}`);
        link.stop();
      },
      reply: () => log.push("reply"),
    });
    link.start();
    input.push(Buffer.concat([frame("welcome", 1), frame("welcome", 2), Buffer.from(REPLY)]));
    expect(log).toEqual(["welcome#1"]);
  });
});

describe("what the link writes", () => {
  const decode = (writes: readonly Buffer[]): DecodeEvent[][] =>
    writes.map((bytes) => new StreamDecoder("to-host").collect(bytes));

  it("counts the ids from 1 across the kinds", () => {
    const output = new FakeOutput();
    const link = new Link(new FakeInput(), output, { message() {}, reply() {} });
    expect([
      link.send("hello", "{}").id,
      link.send("open", "{}").id,
      link.send("file", "AA==", { digest: "a".repeat(64) }).id,
    ]).toEqual([1, 2, 3]);
    link.sendNow("bye", "{}");
    expect(decode(output.writes).at(-1)).toEqual([
      { type: "message", message: { kind: "bye", id: 4, control: {}, payload: "{}" } },
    ]);
  });

  it("writes each message whole and in the order it was queued, whatever the output takes to write", async () => {
    const output = new FakeOutput();
    output.delay = 2;
    const link = new Link(new FakeInput(), output, { message() {}, reply() {} });
    const big = Buffer.alloc(20_000, 7).toString("base64");
    link.send("file", big, { digest: "b".repeat(64) });
    link.send("msg", '{"a":1}');
    const last = link.send("msg", '{"b":2}');
    void link.raw("\x1b[c");
    await last.written;
    await link.flush();
    const writes = decode(output.writes);
    expect(
      writes.map((events) => events.map((e) => (e.type === "message" ? e.message.kind : e.error))),
    ).toEqual([["file"], ["msg"], ["msg"], []]);
    expect(output.writes[3]?.toString("latin1")).toBe("\x1b[c");
    const first = writes[0]?.[0];
    expect(first?.type === "message" && first.message.payload).toBe(big);
  });

  it("writes a bye now, ahead of what is queued", async () => {
    const output = new FakeOutput();
    output.delay = 5;
    const link = new Link(new FakeInput(), output, { message() {}, reply() {} });
    link.send("msg", "{}");
    link.writeNow("\x1b[?1049l");
    link.sendNow("bye", '{"token":"x"}');
    const start = (w: Buffer): string => w.toString("latin1").slice(0, 12);
    // The two that left now did not wait for the queue, which has not yet written its message.
    expect(output.writes.map(start)).toEqual(["\x1b[?1049l", "\x1b_pencere;by"]);
    await link.flush();
    expect(output.writes.map(start)).toEqual(["\x1b[?1049l", "\x1b_pencere;by", "\x1b_pencere;ms"]);
  });

  it("refuses a message a frame cannot carry and queues nothing", () => {
    const output = new FakeOutput();
    const link = new Link(new FakeInput(), output, { message() {}, reply() {} });
    expect(() => link.send("msg", "a\nb")).toThrow(/frame may not carry/);
    expect(output.writes).toEqual([]);
    expect(link.send("msg", "{}").id).toBe(2);
  });

  it("keeps the error of an output that failed, instead of throwing it at the program", async () => {
    const output = new FakeOutput();
    const link = new Link(new FakeInput(), output, { message() {}, reply() {} });
    link.start();
    output.fail = new Error("EIO");
    await link.send("msg", "{}").written;
    expect(link.failure?.message).toBe("EIO");
    const later = new Error("EPIPE");
    output.emit("error", later);
    expect(link.failure).toBe(later);
  });
});
