import type { JSONRPCMessage } from "@modelcontextprotocol/server";
import { describe, expect, it } from "vitest";

import { StreamDecoder } from "../src/frame.js";
import { Link } from "../src/link.js";
import { PencereTransport } from "../src/transport.js";
import { Rig } from "./support.js";

function setup() {
  const rig = new Rig();
  const link = new Link(rig.input, rig.output, { message() {}, reply() {} });
  const transport = new PencereTransport(link);
  return { rig, link, transport };
}

describe("the transport", () => {
  it("writes a JSON-RPC message as a msg, with non-ASCII text as escapes", async () => {
    const { rig, transport } = setup();
    await transport.send({
      jsonrpc: "2.0",
      id: 3,
      result: { content: [{ type: "text", text: "tamam é 😀\x7f" }] },
    });
    const [message] = rig.messages();
    expect(message?.kind).toBe("msg");
    expect(message?.payload).toBe(
      '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"tamam \\u00e9 \\ud83d\\ude00\\u007f"}]}}',
    );
  });

  it("writes a message longer than a frame in chunks of one message", async () => {
    const { rig, transport } = setup();
    const text = "x".repeat(10_000);
    await transport.send({ jsonrpc: "2.0", id: 1, result: { text } });
    await rig.settle();
    const events = new StreamDecoder("to-host").collect(Buffer.from(rig.text(), "latin1"));
    expect(events).toEqual([]);
    expect(rig.messages()).toHaveLength(1);
    expect(JSON.parse(rig.messages()[0]?.payload ?? "")).toEqual({
      jsonrpc: "2.0",
      id: 1,
      result: { text },
    });
  });

  it("hands what the host sent to the server, once it is a JSON-RPC message", () => {
    const { transport } = setup();
    const received: JSONRPCMessage[] = [];
    transport.onmessage = (m) => received.push(m);
    transport.deliver('{"jsonrpc":"2.0","id":1,"method":"tools/list"}');
    expect(received).toEqual([{ jsonrpc: "2.0", id: 1, method: "tools/list" }]);
  });

  it("drops what is not one, and says so without ending", () => {
    const { transport } = setup();
    const errors: Error[] = [];
    transport.onerror = (e) => errors.push(e);
    transport.onmessage = () => expect.unreachable();
    for (const payload of [
      "not json",
      "[]",
      "{}",
      '{"jsonrpc":"2.0"}',
      '{"jsonrpc":"1.0","id":1,"method":"x"}',
    ])
      expect(transport.deliver(payload), payload).toBeUndefined();
    expect(errors.length).toBe(5);
  });

  it("is closed once: the server is told, and nothing is written or delivered after", async () => {
    const { rig, transport } = setup();
    let closed = 0;
    transport.onclose = () => (closed += 1);
    transport.onmessage = () => expect.unreachable();
    await transport.close();
    await transport.close();
    expect(closed).toBe(1);
    await expect(transport.send({ jsonrpc: "2.0", id: 1, result: {} })).rejects.toThrow(/closed/);
    expect(transport.deliver('{"jsonrpc":"2.0","id":1,"method":"ping"}')).toBeUndefined();
    expect(rig.messages()).toEqual([]);
  });
});
