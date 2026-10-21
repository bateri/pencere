import { Bundle } from "@pencere/sdk";
import type { Connectable, ServeOptions, ServeResult } from "@pencere/sdk";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { EXIT, run, shown } from "../src/run.js";
import type { Environment, Finish } from "../src/run.js";
import type { CloseTiming } from "../src/server-process.js";
import { ECHO, FakeTransport, alive, node, tools, until } from "./support.js";

const QUICK: CloseTiming = { stdin: 100, term: 100, kill: 2000 };
const VIEW = fileURLToPath(new URL("view/", import.meta.url));

type Serve = (server: Connectable, options: ServeOptions) => Promise<ServeResult>;

/** `run` in an environment that is a terminal, with a `serve` of the test's. */
function terminal(serve: Serve = async () => ({ kind: "closed", by: "user" })) {
  const out: string[] = [];
  const err: string[] = [];
  const listeners = new Map<string, (() => void)[]>();
  const served: { server: Connectable; options: ServeOptions }[] = [];
  const env: Environment = {
    stdin: { isTTY: true },
    stdout: { isTTY: true, write: (text) => out.push(text) },
    stderr: { write: (text) => err.push(text) },
    process: {
      on: (signal, listener) => listeners.set(signal, [...(listeners.get(signal) ?? []), listener]),
      removeListener: (signal, listener) =>
        listeners.set(
          signal,
          (listeners.get(signal) ?? []).filter((other) => other !== listener),
        ),
    },
    serve: (server, options) => {
      served.push({ server, options });
      return serve(server, options);
    },
    version: "1.2.3",
    discoveryTimeout: 10_000,
    timing: QUICK,
  };
  return {
    env,
    served,
    out: () => out.join(""),
    err: () => err.join(""),
    emit: (signal: string) => (listeners.get(signal) ?? []).forEach((listener) => listener()),
    listening: (signal: string) => (listeners.get(signal) ?? []).length,
  };
}

const argv = (spec: { command: string; args: readonly string[] }): string[] => [
  spec.command,
  ...spec.args,
];

/** The command line of `pencere run` for a server, after the options of pencere. */
const command = (spec: { command: string; args: readonly string[] }, ...own: string[]) => [
  "run",
  ...own,
  "--",
  ...argv(spec),
];

const code = (finish: Finish): number => ("code" in finish ? finish.code : -1);

describe("pencere run", () => {
  it("says what it is and how to use it", async () => {
    const t = terminal();
    expect(await run(["--help"], t.env)).toEqual({ code: EXIT.ok });
    expect(t.out()).toContain("usage: pencere run");
    const v = terminal();
    expect(await run(["--version"], v.env)).toEqual({ code: EXIT.ok });
    expect(v.out()).toBe("pencere 1.2.3\n");
  });

  it("names a mistake in the command line and shows no view", async () => {
    const t = terminal();
    expect(await run(["run", "--bogus", "--", "s"], t.env)).toEqual({ code: EXIT.usage });
    expect(t.err()).toContain("--bogus");
    expect(t.err()).toContain("pencere --help");
    expect(t.served).toEqual([]);
  });

  it("needs a terminal, and says that the server is for an MCP client", async () => {
    for (const side of ["stdin", "stdout"] as const) {
      const t = terminal();
      const env: Environment = { ...t.env, [side]: { isTTY: false, write: () => {} } };
      expect(await run(["run", "--", "python", "my server.py"], env)).toEqual({
        code: EXIT.noView,
      });
      expect(t.err()).toContain("not a terminal");
      expect(t.err()).toContain("python 'my server.py' is an MCP server");
      expect(t.served).toEqual([]);
    }
  });

  it("shows the tool it was told to, with its arguments, and starts nothing to ask", async () => {
    const t = terminal();
    const finish = await run(
      ["run", "--tool", "list", "--args", '{"all":true}', "--", "/nonexistent/server"],
      t.env,
    );
    expect(finish).toEqual({ code: EXIT.ok });
    expect(t.served).toHaveLength(1);
    expect(t.served[0]?.options.tool).toEqual({ name: "list", arguments: { all: true } });
    expect(t.served[0]?.options.bundle).toBeUndefined();
    expect(t.served[0]?.options.fallback).toBeUndefined();
    expect(t.err()).toBe("");
    expect(t.listening("SIGINT") + t.listening("SIGTERM")).toBe(0);
  });

  it("asks the server which tool has a view when it was not told", async () => {
    const t = terminal();
    expect(await run(command(tools("one")), t.env)).toEqual({ code: EXIT.ok });
    expect(t.served[0]?.options.tool).toEqual({ name: "show" });
  });

  it("asks the person to choose when it cannot", async () => {
    const many = terminal();
    expect(await run(command(tools("many")), many.env)).toEqual({ code: EXIT.usage });
    expect(many.err()).toContain("first, second");
    expect(many.err()).toContain("--tool <name>");
    expect(many.served).toEqual([]);

    const none = terminal();
    expect(await run(command(tools("none")), none.env)).toEqual({ code: EXIT.failed });
    expect(none.err()).toContain("names a view");
    expect(none.err()).toContain("plain, other");

    const empty = terminal();
    expect(await run(command(tools("empty")), empty.env)).toEqual({ code: EXIT.failed });
    expect(empty.err()).toContain("no tools");
  });

  it("says what went wrong when the server cannot be asked", async () => {
    const missing = terminal();
    expect(await run(["run", "--", "/nonexistent/server"], missing.env)).toEqual({
      code: EXIT.failed,
    });
    expect(missing.err()).toContain('cannot start "/nonexistent/server"');

    const broken = terminal();
    const failed = await run(
      command(node(`console.error("ModuleNotFoundError: mcp"); process.exit(1);`)),
      broken.env,
    );
    expect(failed).toEqual({ code: EXIT.failed });
    expect(broken.err()).toContain("did not answer as an MCP server");
    expect(broken.err()).toContain("  ModuleNotFoundError: mcp");
    expect(broken.served).toEqual([]);
  });

  it("shows the view from a directory as a bundle", async () => {
    const t = terminal();
    expect(await run(command(tools("one"), "--view", VIEW), t.env)).toEqual({ code: EXIT.ok });
    const bundle = t.served[0]?.options.bundle;
    expect(bundle).toBeInstanceOf(Bundle);
    expect(bundle?.manifest.entry).toBe("index.html");
    expect(bundle?.manifest.files.map((file) => file.path)).toEqual(["index.html"]);
  });

  it("says what is wrong with a view directory, before it asks the terminal for anything", async () => {
    const missing = terminal();
    expect(
      await run(["run", "--tool", "t", "--view", "/nonexistent/view", "--", "s"], missing.env),
    ).toEqual({ code: EXIT.usage });
    expect(missing.err()).toContain("--view /nonexistent/view: no such directory");

    const empty = terminal();
    // A directory with no `index.html` is not a view.
    expect(
      await run(
        [
          "run",
          "--tool",
          "t",
          "--view",
          fileURLToPath(new URL("servers/", import.meta.url)),
          "--",
          "s",
        ],
        empty.env,
      ),
    ).toEqual({ code: EXIT.usage });
    expect(empty.err()).toContain('the entry "index.html" is not one of the files');
    expect(empty.served).toEqual([]);
  });

  describe("what the terminal said", () => {
    const outcome = async (result: ServeResult) => {
      const t = terminal(async () => result);
      const finish = await run(["run", "--tool", "t", "--", "python", "server.py"], t.env);
      return { code: code(finish), err: t.err() };
    };

    it("is a success when the view closed, whoever closed it", async () => {
      for (const by of ["user", "view", "program"] as const)
        expect(await outcome({ kind: "closed", by })).toEqual({ code: EXIT.ok, err: "" });
    });

    it("is a failure when the terminal ended the view with an error", async () => {
      const result = await outcome({ kind: "closed", by: "error" });
      expect(result.code).toBe(EXIT.failed);
      expect(result.err).toContain("ended the view with an error");
    });

    it("tells a terminal that does not show views what a server is for", async () => {
      const result = await outcome({ kind: "no-view", reason: "no-host" });
      expect(result.code).toBe(EXIT.noView);
      expect(result.err).toContain("does not show pencere views");
      expect(result.err).toContain("python server.py is an MCP server");
      expect(result.err).toContain("MCP client");
    });

    it("tells a terminal of another version of the protocol", async () => {
      const result = await outcome({ kind: "no-view", reason: "no-common-version" });
      expect(result.code).toBe(EXIT.noView);
      expect(result.err).toContain("no version of the pencere protocol");
    });

    it("says why a view was refused or could not start, in words", async () => {
      for (const [reason, why, expected] of [
        ["refused", "busy", "already shows a view"],
        ["refused", "policy", "policy"],
        ["refused", "limit", "--view"],
        ["refused", "other", "refused to show the view (other)"],
        ["refused", undefined, "refused to show the view."],
        ["failed", "bundle", "could not verify"],
        ["failed", "mcp", "talking to the server"],
        ["failed", undefined, "could not start the view."],
      ] as const) {
        const result = await outcome({
          kind: "no-view",
          reason,
          ...(why === undefined ? {} : { code: why }),
        });
        expect(result.code, `${reason} ${why}`).toBe(EXIT.failed);
        expect(result.err, `${reason} ${why}`).toContain(expected);
      }
    });
  });

  it("says that the server ended while its view was up, with what it said", async () => {
    const t = terminal(async (server, options) => {
      await server.connect(new FakeTransport());
      await new Promise<void>((resolve) =>
        options.signal?.addEventListener("abort", () => resolve(), { once: true }),
      );
      await server.close();
      return { kind: "closed", by: "aborted" };
    });
    const dying = node(`
      console.error("Traceback (most recent call last):\\n\\u001b[2JValueError: boom");
      setTimeout(() => process.exit(7), 50);
    `);
    const finish = await run(["run", "--tool", "t", "--", ...argv(dying)], t.env);
    expect(finish).toEqual({ code: EXIT.failed });
    expect(t.err()).toContain("ended (exit status 7) while its view was open");
    expect(t.err()).toContain("  Traceback (most recent call last):");
    // What it wrote is shown as text.
    expect(t.err()).toContain("  \\u001b[2JValueError: boom");
    expect(t.err()).not.toContain("\u001b");
  });

  it("says that the server ended before the view ran", async () => {
    const t = terminal(async (server, options) => {
      await server.connect(new FakeTransport());
      await new Promise<void>((resolve) =>
        options.signal?.addEventListener("abort", () => resolve(), { once: true }),
      );
      return { kind: "no-view", reason: "aborted" };
    });
    const finish = await run(["run", "--tool", "t", "--", ...argv(node(`process.exit(2)`))], t.env);
    expect(finish).toEqual({ code: EXIT.failed });
    expect(t.err()).toContain("(exit status 2) while its view was opening");
  });

  it("says when the server cannot be started once the terminal has said yes", async () => {
    const t = terminal(async (server) => {
      await server.connect(new FakeTransport());
      return { kind: "closed", by: "user" };
    });
    expect(await run(["run", "--tool", "t", "--", "/nonexistent/server"], t.env)).toEqual({
      code: EXIT.failed,
    });
    expect(t.err()).toContain('cannot start "/nonexistent/server"');
  });

  it("reports what `serve` threw", async () => {
    const t = terminal(async () => {
      throw new Error("the stream broke");
    });
    expect(await run(["run", "--tool", "t", "--", "s"], t.env)).toEqual({ code: EXIT.failed });
    expect(t.err()).toContain("pencere: the stream broke");
  });

  it("ends as the person ended it, after the server is gone", async () => {
    let pid: number | undefined;
    const t = terminal(async (server, options) => {
      const transport = new FakeTransport();
      await server.connect(transport);
      transport.onmessage?.({ jsonrpc: "2.0", id: 1, method: "initialize" });
      await until(() => transport.sent.length === 1, "an answer");
      pid = (transport.sent[0] as unknown as { result: { pid: number } }).result.pid;
      // What `serve` does on a signal while a view is up: it ends the view and returns,
      // and does not wait for the server (the connector's close is not awaited).
      await new Promise<void>((resolve) => {
        const heard = (): void => {
          t.env.process.removeListener("SIGINT", heard);
          resolve();
        };
        t.env.process.on("SIGINT", heard);
      });
      void server.close();
      expect(options.signal?.aborted).toBe(false);
      return { kind: "closed", by: "signal" };
    });
    const running = run(["run", "--tool", "t", "--", ...argv(ECHO)], t.env);
    await until(() => pid !== undefined, "the server to answer");
    t.emit("SIGINT");
    const finish = await running;
    expect(finish).toEqual({ signal: "SIGINT" });
    expect(alive(pid ?? 0)).toBe(false);
    expect(t.listening("SIGINT") + t.listening("SIGTERM")).toBe(0);
    expect(t.err()).toBe("");
  });

  it("ends as the person ended it while the server is being asked", async () => {
    const t = terminal();
    const silent = node(`console.error(process.pid); setInterval(() => {}, 1000);`);
    const running = run(command(silent), t.env);
    // The server is running once it has said which process it is: it is in the tail
    // only, so the test looks for the process itself.
    await new Promise((resolve) => setTimeout(resolve, 400));
    t.emit("SIGTERM");
    expect(await running).toEqual({ signal: "SIGTERM" });
    expect(t.served).toEqual([]);
    expect(t.err()).toBe("");
    expect(t.listening("SIGINT") + t.listening("SIGTERM")).toBe(0);
  });
});

describe("a command shown as it is typed", () => {
  it("quotes what a shell would take apart", () => {
    expect(shown({ command: "python", args: ["my server.py", "--flag=1", "it's"] })).toBe(
      "python 'my server.py' --flag=1 'it'\\''s'",
    );
  });
});
