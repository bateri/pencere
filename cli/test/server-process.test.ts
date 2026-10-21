import type { JSONRPCMessage } from "@modelcontextprotocol/server";
import { execFileSync } from "node:child_process";
import { describe, expect, it } from "vitest";

import { ServerProcess, StartError } from "../src/server-process.js";
import type { CloseTiming, Exit, ProcessLimits } from "../src/server-process.js";
import { Tail } from "../src/tail.js";
import { alive, node, request, until } from "./support.js";

const QUICK: CloseTiming = { stdin: 100, term: 100, kill: 2000 };

/** A started process, and everything it said to the client. */
async function started(code: string, timing: CloseTiming = QUICK, limits?: ProcessLimits) {
  const tail = new Tail();
  const server = new ServerProcess(node(code), tail, timing, limits);
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

  it("says that a server fell that fell just before it was asked to end, not that it ended as asked", async () => {
    // A grandchild holds the pipes, so that the end of the process is read only after a grace;
    // the view closing in that time is not what ended the server.
    const run = await started(`
      const { spawn } = require("node:child_process");
      const child = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], { stdio: ["ignore", "inherit", "inherit"] });
      console.error(child.pid);
      setTimeout(() => process.exit(3), 30);
    `);
    await until(() => !run.tail.empty, "the grandchild's pid");
    const grandchild = Number(run.tail.text());
    const leader = run.server.pid ?? 0;
    try {
      await until(() => !alive(leader), "the server to be gone");
      await run.server.close();
      expect(run.exits).toEqual([{ code: 3, signal: null, expected: false }]);
    } finally {
      try {
        process.kill(grandchild);
      } catch {
        // Gone already: the test is about what is left of it, and this is what it left.
      }
    }
  });

  it("ends what a server that left on its own at the end of its stdin started in its group", async () => {
    const run = await started(`
      const { spawn } = require("node:child_process");
      const spawnChild = (code) =>
        spawn(process.execPath, ["-e", code], { stdio: "ignore" }).pid;
      // What \`sh -c 'server &'\` leaves: one that takes SIGTERM, and one that will not.
      const pids = [
        spawnChild("setInterval(() => {}, 1000)"),
        spawnChild('process.on("SIGTERM", () => {}); setInterval(() => {}, 1000)'),
      ];
      console.error(pids.join(" "));
      process.stdin.resume();
      process.stdin.on("end", () => process.exit(0));
    `);
    await until(() => !run.tail.empty, "the pids");
    const pids = run.tail.text().trim().split(" ").map(Number);
    expect(pids.every(alive)).toBe(true);
    await run.server.close();
    expect(run.exits).toEqual([{ code: 0, signal: null, expected: true }]);
    await until(() => pids.every((pid) => !alive(pid)), "the server's children to be gone");
  });

  it("ends what a server that fell on its own started in its group, as it ends what it ended itself", async () => {
    const run = await started(`
      const { spawn } = require("node:child_process");
      const child = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], { stdio: "ignore" });
      console.error(child.pid);
      setTimeout(() => process.exit(0), 50);
    `);
    await until(() => !run.tail.empty, "the child's pid");
    const child = Number(run.tail.text());
    expect(alive(child)).toBe(true);
    // Nobody asked it to end: it is a server that fell, and the program has no use for what it left.
    expect(await run.server.ended).toEqual({ code: 0, signal: null, expected: false });
    await run.server.close();
    await until(() => !alive(child), "the child of the server to be gone");
  });

  it("drops the line that is too long and reads the lines after it, in the same chunk or the next", async () => {
    const limits: ProcessLimits = { line: 1000 };
    // The line ends in the chunk that is over the limit, and whole lines follow it there.
    const same = await started(
      `process.stdout.write("x".repeat(1010) + '\\n{"jsonrpc":"2.0","method":"after"}\\n');`,
      QUICK,
      limits,
    );
    await until(() => same.messages.length === 1, "the line after the long one");
    expect(same.messages.map((m) => ("method" in m ? m.method : undefined))).toEqual(["after"]);
    expect(same.errors.map((error) => error.message)).toEqual([expect.stringContaining("maximum")]);
    await same.server.close();

    // The line is cut in two by the time between chunks, and the second chunk takes it over.
    const next = await started(
      `process.stdout.write("x".repeat(900));
       setTimeout(() => process.stdout.write("x".repeat(200) + '\\n{"jsonrpc":"2.0","method":"after"}\\n'), 50);`,
      QUICK,
      limits,
    );
    await until(() => next.messages.length === 1, "the line after the long one");
    expect(next.messages.map((m) => ("method" in m ? m.method : undefined))).toEqual(["after"]);
    await next.server.close();

    // A line that is within the limit is read whole, however the chunks fall.
    const fits = await started(
      `process.stdout.write('{"jsonrpc":"2.0","method":"fits","params":{"p":"' + "x".repeat(800));
       setTimeout(() => process.stdout.write('"}}\\n{"jsonrpc":"2.0","method":"after"}\\n'), 50);`,
      QUICK,
      limits,
    );
    await until(() => fits.messages.length === 2, "both lines");
    expect(fits.errors).toEqual([]);
    await fits.server.close();
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
