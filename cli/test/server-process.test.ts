import type { JSONRPCMessage } from "@modelcontextprotocol/server";
import { execFileSync } from "node:child_process";
import { describe, expect, it } from "vitest";

import { ServerProcess, StartError } from "../src/server-process.js";
import type { CloseTiming, Exit } from "../src/server-process.js";
import { Tail } from "../src/tail.js";
import { alive, node, request, until } from "./support.js";

const QUICK: CloseTiming = { stdin: 100, term: 100, kill: 2000 };

/** A started process, and everything it said to the client. */
async function started(code: string, timing: CloseTiming = QUICK) {
  const tail = new Tail();
  const server = new ServerProcess(node(code), tail, timing);
  const messages: JSONRPCMessage[] = [];
  const errors: Error[] = [];
  const exits: Exit[] = [];
  let closed = 0;
  server.onmessage = (message) => messages.push(message);
  server.onerror = (error) => errors.push(error);
  server.onexit = (exit) => exits.push(exit);
  server.onclose = () => (closed += 1);
  await server.start();
  return { server, tail, messages, errors, exits, closed: () => closed };
}

describe("a server that is a process", () => {
  it("reads messages whole, however its output is cut into chunks", async () => {
    const run = await started(`
      const out = (text) => new Promise((resolve) => setTimeout(() => { process.stdout.write(text); resolve(); }, 20));
      (async () => {
        await out('{"jsonrpc":"2.0","method":"a"}\\n{"jsonrpc":"2.0",');
        await out('"method":"b"}\\n');
        await out('{"jsonrpc":"2.0","method":"c"}\\n{"jsonrpc":"2.0","method":"d"}\\n{"jsonrpc":"2.0","met');
        await out('hod":"e"}\\r\\n');
      })();
    `);
    await until(() => run.messages.length === 5, "five messages");
    expect(run.messages.map((m) => ("method" in m ? m.method : undefined))).toEqual([
      "a",
      "b",
      "c",
      "d",
      "e",
    ]);
    await run.server.close();
  });

  it("skips a line that is not JSON and reads the ones around it", async () => {
    const run = await started(`
      process.stdout.write('{"jsonrpc":"2.0","method":"before"}\\nhello, a stray log line\\n{"jsonrpc":"2.0","method":"after"}\\n');
    `);
    await until(() => run.messages.length === 2, "two messages");
    expect(run.messages.map((m) => ("method" in m ? m.method : undefined))).toEqual([
      "before",
      "after",
    ]);
    await run.server.close();
  });

  it("writes each message as one line, whole, whatever the message holds", async () => {
    // The server gives back what it is sent, line by line.
    const run = await started(`process.stdin.pipe(process.stdout);`);
    const first = {
      jsonrpc: "2.0",
      id: 1,
      method: "tools/call",
      params: { text: "two\nlines, a   and \u0085", big: "x".repeat(200_000) },
    } as JSONRPCMessage;
    await run.server.send(first);
    await run.server.send(request(2, "ping"));
    await until(() => run.messages.length === 2, "the echo of two messages");
    expect(run.messages[0]).toEqual(first);
    expect(run.messages[1]).toEqual(request(2, "ping"));
    await run.server.close();
  });

  it("keeps what the server writes to stderr, and reads none of it as messages", async () => {
    const run = await started(`
      console.error("warming up");
      console.error('{"jsonrpc":"2.0","method":"not-a-message"}');
    `);
    await run.server.ended;
    expect(run.tail.text()).toBe('warming up\n{"jsonrpc":"2.0","method":"not-a-message"}\n');
    expect(run.messages).toEqual([]);
  });

  it("starts the server with the whole environment of the program", async () => {
    process.env.PENCERE_TEST_ENVIRONMENT = "kept";
    try {
      const run = await started(
        `console.error(process.env.PENCERE_TEST_ENVIRONMENT + " " + process.env.HOME);`,
      );
      await run.server.ended;
      expect(run.tail.text()).toBe(`kept ${process.env.HOME}\n`);
    } finally {
      delete process.env.PENCERE_TEST_ENVIRONMENT;
    }
  });

  it("starts the server in a process group of its own", async () => {
    const run = await started(`console.error(process.pid); setInterval(() => {}, 1000);`);
    await until(() => !run.tail.empty, "the server's pid");
    const pid = Number(run.tail.text());
    expect(run.server.pid).toBe(pid);
    const group = (of: number) =>
      Number(execFileSync("ps", ["-o", "pgid=", "-p", String(of)], { encoding: "utf8" }));
    // Its own: ⌃C goes to the group that has the terminal, which is this one.
    expect(group(pid)).toBe(pid);
    expect(group(process.pid)).not.toBe(pid);
    await run.server.close();
  });

  it("says that a server ended that nobody asked to end, once, and then that it is closed", async () => {
    const run = await started(`console.error("it fell"); process.exit(3);`);
    const exit = await run.server.ended;
    expect(exit).toEqual({ code: 3, signal: null, expected: false });
    expect(run.exits).toEqual([exit]);
    expect(run.closed()).toBe(1);
    expect(run.tail.text()).toBe("it fell\n");
    // Nothing is sent to what is gone.
    await expect(run.server.send(request(1, "ping"))).rejects.toThrow("not running");
    await run.server.close();
    expect(run.exits).toHaveLength(1);
  });

  it("ends a server by closing its stdin, and that is not a failure", async () => {
    const run = await started(
      `process.stdin.resume(); process.stdin.on("end", () => process.exit(0));`,
    );
    const began = Date.now();
    await run.server.close();
    expect(Date.now() - began).toBeLessThan(QUICK.stdin + 1000);
    expect(run.exits).toEqual([{ code: 0, signal: null, expected: true }]);
    expect(run.closed()).toBe(1);
    // Closing again is nothing to do.
    await run.server.close();
    expect(run.exits).toHaveLength(1);
    await expect(run.server.send(request(1, "ping"))).rejects.toThrow("not running");
  });

  it("takes a server that ignores the end of its stdin with SIGTERM", async () => {
    const run = await started(`setInterval(() => {}, 1000);`);
    await run.server.close();
    expect(run.exits).toEqual([{ code: null, signal: "SIGTERM", expected: true }]);
  });

  it("takes a server that ignores that too with SIGKILL, and what it started with it", async () => {
    const run = await started(`
      const { spawn } = require("node:child_process");
      process.on("SIGTERM", () => {});
      // Stands in for what \`npx\`, \`uv run\` and \`sh -c\` start: a grandchild that may not leave either.
      const child = spawn(process.execPath, ["-e", 'process.on("SIGTERM", () => {}); setInterval(() => {}, 1000)'], { stdio: "ignore" });
      console.error(child.pid);
      setInterval(() => {}, 1000);
    `);
    await until(() => !run.tail.empty, "the grandchild's pid");
    const grandchild = Number(run.tail.text());
    expect(alive(grandchild)).toBe(true);
    await run.server.close();
    expect(run.exits).toEqual([{ code: null, signal: "SIGKILL", expected: true }]);
    await until(() => !alive(grandchild), "the grandchild to be gone");
  });

  it("says plainly when the command cannot be started", async () => {
    const server = new ServerProcess(
      { command: "/nonexistent/pencere-server", args: [] },
      new Tail(),
      QUICK,
    );
    let exits = 0;
    server.onexit = () => (exits += 1);
    const failure = await server.start().catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(StartError);
    expect((failure as StartError).message).toContain('cannot start "/nonexistent/pencere-server"');
    expect((failure as StartError).message).toContain("no such file or directory");
    // It never ran, so it did not end.
    await server.close();
    expect(exits).toBe(0);
  });

  it("starts once", async () => {
    const run = await started(`setInterval(() => {}, 1000);`);
    await expect(run.server.start()).rejects.toThrow("already");
    await run.server.close();
  });
});
