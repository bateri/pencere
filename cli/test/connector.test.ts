import { describe, expect, it } from "vitest";

import { ProcessConnector } from "../src/connector.js";
import type { ConnectorOptions } from "../src/connector.js";
import { StartError } from "../src/server-process.js";
import type { CloseTiming, Exit } from "../src/server-process.js";
import { Tail } from "../src/tail.js";
import { ECHO, FakeTransport, alive, node, request, until } from "./support.js";

const QUICK: CloseTiming = { stdin: 100, term: 100, kill: 2000 };

function connector(command = ECHO, options: Partial<ConnectorOptions> = {}) {
  const tail = new Tail();
  const unexpected: Exit[] = [];
  return {
    tail,
    unexpected,
    connector: new ProcessConnector(command, {
      tail,
      unexpectedExit: (exit) => unexpected.push(exit),
      timing: QUICK,
      ...options,
    }),
  };
}

/** The pid a stand-in server answered with, for the response to request `id`. */
function pidIn(transport: FakeTransport, id: number): number {
  const answer = transport.sent.find((message) => "id" in message && message.id === id);
  return (answer as unknown as { result: { pid: number } }).result.pid;
}

describe("the server of a view, as a process", () => {
  it("carries messages between the stream and the process, in order and untouched", async () => {
    const { connector: c } = connector();
    const transport = new FakeTransport();
    await c.connect(transport);
    for (let id = 1; id <= 5; id += 1) transport.onmessage?.(request(id, `method-${id}`));
    await until(() => transport.sent.length === 5, "five answers");
    expect(
      transport.sent.map((m) => ("result" in m ? (m.result as { method: string }).method : "")),
    ).toEqual(["method-1", "method-2", "method-3", "method-4", "method-5"]);
    expect(transport.sent.map((m) => ("id" in m ? m.id : undefined))).toEqual([1, 2, 3, 4, 5]);
    await c.close();
    await c.drain();
  });

  it("starts a process for each connection, and ends the last one first", async () => {
    const { connector: c, unexpected } = connector();
    const first = new FakeTransport();
    await c.connect(first);
    first.onmessage?.(request(1, "initialize"));
    await until(() => first.sent.length === 1, "an answer");
    const before = pidIn(first, 1);

    // The host starts the view again (§10.7) or the program reopens it (§10.8):
    // a new session, and so a new process, which has its own `initialize`.
    await c.close();
    expect(alive(before)).toBe(false);
    const second = new FakeTransport();
    await c.connect(second);
    second.onmessage?.(request(1, "initialize"));
    await until(() => second.sent.length === 1, "an answer");
    const after = pidIn(second, 1);
    expect(after).not.toBe(before);
    expect(c.starts).toBe(2);
    expect(first.closed).toBe(true);

    // A connection that is made before the last was closed ends the last.
    const third = new FakeTransport();
    await c.connect(third);
    expect(alive(after)).toBe(false);
    expect(c.starts).toBe(3);
    await c.drain();
    // Everything above was asked of the processes, so none of it is a failure.
    expect(unexpected).toEqual([]);
  });

  it("ends the process when the stream is closed from the other side", async () => {
    const { connector: c, unexpected } = connector();
    const transport = new FakeTransport();
    await c.connect(transport);
    transport.onmessage?.(request(1, "initialize"));
    await until(() => transport.sent.length === 1, "an answer");
    const pid = pidIn(transport, 1);
    await transport.close();
    await c.drain();
    await until(() => !alive(pid), "the process to end");
    expect(unexpected).toEqual([]);
  });

  it("says when the process ends without being asked to", async () => {
    const {
      connector: c,
      unexpected,
      tail,
    } = connector(node(`console.error("it fell"); setTimeout(() => process.exit(9), 50);`));
    await c.connect(new FakeTransport());
    await until(() => unexpected.length === 1, "the process to end");
    expect(unexpected).toEqual([{ code: 9, signal: null, expected: false }]);
    expect(tail.text()).toBe("it fell\n");
    // Letting go of what has ended is not a second ending.
    await c.close();
    await c.drain();
    expect(unexpected).toHaveLength(1);
  });

  it("does not take it for a failure that a process ends while the connector ends it", async () => {
    const { connector: c, unexpected } = connector(node(`setInterval(() => {}, 1000);`));
    await c.connect(new FakeTransport());
    await c.close();
    await c.drain();
    expect(unexpected).toEqual([]);
  });

  it("keeps a message the stream cannot take from stopping the server", async () => {
    const { connector: c } = connector();
    const transport = new FakeTransport();
    await c.connect(transport);
    // The view is gone: what the server says to it has nowhere to go, and says so by failing.
    transport.closed = true;
    transport.onmessage?.(request(1, "initialize"));
    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(transport.sent).toEqual([]);
    transport.closed = false;
    transport.onmessage?.(request(2, "ping"));
    await until(() => transport.sent.length === 1, "an answer");
    await c.drain();
  });

  it("fails to connect, and leaves the stream alone, when the command cannot be started", async () => {
    const { connector: c } = connector({ command: "/nonexistent/pencere-server", args: [] });
    const transport = new FakeTransport();
    await expect(c.connect(transport)).rejects.toBeInstanceOf(StartError);
    expect(transport.onmessage).toBeUndefined();
    expect(c.starts).toBe(0);
    await c.drain();
  });
});
