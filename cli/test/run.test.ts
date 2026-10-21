import { Bundle, ViewControl } from "@pencere/sdk";
import type {
  Connectable,
  ReopenOptions,
  ReopenResult,
  ServeOptions,
  ServeResult,
} from "@pencere/sdk";
import { createHash } from "node:crypto";
import { writeFileSync } from "node:fs";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vitest";

import { EXIT, run, shown } from "../src/run.js";
import type { Environment, Finish } from "../src/run.js";
import type { CloseTiming } from "../src/server-process.js";
import { WatchError } from "../src/watch.js";
import type { Role, Root } from "../src/watch.js";
import { ECHO, FakeTransport, alive, node, raw, tools, until } from "./support.js";

const QUICK: CloseTiming = { stdin: 100, term: 100, kill: 2000 };
const VIEW = fileURLToPath(new URL("view/", import.meta.url));

type Serve = (server: Connectable, options: ServeOptions) => Promise<ServeResult>;

/** `run` in an environment that is a terminal, with a `serve` of the test's. */
function terminal(serve: Serve = async () => ({ kind: "closed", by: "user" })) {
  const out: string[] = [];
  const err: string[] = [];
  const listeners = new Map<string, (() => void)[]>();
  const served: { server: Connectable; options: ServeOptions }[] = [];
  const watches: {
    roots: readonly Root[];
    onChange: (role: Role) => void;
    onError: (message: string) => void;
    closed: boolean;
  }[] = [];
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
    reload: { settle: 0, retry: 0, tries: 3 },
    watch: (roots, onChange, onError) => {
      const watch = { roots, onChange, onError, closed: false };
      watches.push(watch);
      return {
        close: () => {
          watch.closed = true;
        },
      };
    },
  };
  return {
    env,
    served,
    watches,
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

  it("shows the tool names and the failures a server wrote as text, not as commands to the terminal", async () => {
    const hostile = terminal();
    expect(await run(command(raw("hostile")), hostile.env)).toEqual({ code: EXIT.usage });
    expect(hostile.err()).toContain("first\\u001b[2J, second\\u001b]0;pwned\\u0007");
    expect(hostile.err()).not.toMatch(/[\u001b\u0007]/);

    const failing = terminal();
    expect(await run(command(raw("failing")), failing.env)).toEqual({ code: EXIT.failed });
    expect(failing.err()).toContain("no tools \\u001b[2Jfor you");
    expect(failing.err()).not.toContain("\u001b");

    // The tools of a server with none that names a view are listed too.
    const none = terminal();
    expect(await run(command(raw("foreign")), none.env)).toEqual({ code: EXIT.failed });
    expect(none.err()).toContain("its tools are show.");
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

    it("says that a terminal that stopped answering while the view was shown again is not one that shows no views", async () => {
      const result = await outcome({ kind: "no-view", reason: "timeout" });
      // 1, not 3: there is a terminal that shows views, and it went quiet.
      expect(result.code).toBe(EXIT.failed);
      expect(result.err).toContain("did not answer in time");
      expect(result.err).not.toContain("does not show pencere views");
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

describe("a hangup", () => {
  // ssh going away ends the terminal's session with SIGHUP: a server that ignores the end of its
  // stdin would live on if the view, and so the server, were not ended for it.
  it("ends the view and the server, and then the process, by the signal", async () => {
    let pid: number | undefined;
    const t = terminal(async (server, options) => {
      const transport = new FakeTransport();
      await server.connect(transport);
      transport.onmessage?.({ jsonrpc: "2.0", id: 1, method: "initialize" });
      await until(() => transport.sent.length === 1, "an answer");
      pid = (transport.sent[0] as unknown as { result: { pid: number } }).result.pid;
      // `serve` does not listen for SIGHUP: it is ended by the signal the caller aborts it with.
      await new Promise<void>((resolve) =>
        options.signal?.addEventListener("abort", () => resolve(), { once: true }),
      );
      await server.close();
      return { kind: "closed", by: "aborted" };
    });
    const running = run(["run", "--tool", "t", "--", ...argv(ECHO)], t.env);
    await until(() => pid !== undefined, "the server to answer");
    t.emit("SIGHUP");
    expect(await running).toEqual({ signal: "SIGHUP" });
    expect(alive(pid ?? 0)).toBe(false);
    expect(t.listening("SIGHUP") + t.listening("SIGINT") + t.listening("SIGTERM")).toBe(0);
    expect(t.err()).toBe("");
  });

  it("ends the question to the server too", async () => {
    const t = terminal();
    const silent = node(`console.error(process.pid); setInterval(() => {}, 1000);`);
    const running = run(command(silent), t.env);
    await new Promise((resolve) => setTimeout(resolve, 400));
    t.emit("SIGHUP");
    expect(await running).toEqual({ signal: "SIGHUP" });
    expect(t.served).toEqual([]);
  });
});

describe("a command shown as it is typed", () => {
  it("quotes what a shell would take apart", () => {
    expect(shown({ command: "python", args: ["my server.py", "--flag=1", "it's"] })).toBe(
      "python 'my server.py' --flag=1 'it'\\''s'",
    );
  });
});

describe("pencere dev", () => {
  const dirs: string[] = [];
  afterEach(async () => {
    for (const dir of dirs.splice(0)) await rm(dir, { recursive: true, force: true });
  });

  /** A view directory of the test's own, with an `index.html` that says `text`. */
  async function view(text = "one"): Promise<string> {
    const dir = await mkdtemp(join(tmpdir(), "pencere-cli-dev-"));
    dirs.push(dir);
    await writeFile(join(dir, "index.html"), `<!doctype html><p>${text}`);
    return dir;
  }

  const devArgv = (...own: string[]) => ["dev", "--tool", "t", ...own, "--", "s"];

  it("watches the view's directory and the server's paths, and ends the watching with the view", async () => {
    const dir = await view();
    const server = fileURLToPath(new URL("support.ts", import.meta.url));
    const t = terminal();
    expect(await run(devArgv("--view", `${dir}/`, "--watch", server), t.env)).toEqual({
      code: EXIT.ok,
    });
    expect(t.served[0]?.options.control).toBeInstanceOf(ViewControl);
    expect(t.served[0]?.options.bundle).toBeInstanceOf(Bundle);
    expect(t.watches).toHaveLength(1);
    expect(t.watches[0]?.roots).toEqual([
      { path: resolve(dir), role: "view" },
      { path: server, role: "server" },
    ]);
    expect(t.watches[0]?.closed).toBe(true);
    expect(t.err()).toBe("");
  });

  it("is not what run does: run is given no control and watches nothing", async () => {
    const t = terminal();
    expect(await run(["run", "--tool", "t", "--", "s"], t.env)).toEqual({ code: EXIT.ok });
    expect(t.served[0]?.options.control).toBeUndefined();
    expect(t.watches).toEqual([]);
  });

  it("reopens the view with the files as they are now when they change", async () => {
    const dir = await view("one");
    const asked: ReopenOptions[] = [];
    const t = terminal(async (_server, options) => {
      const control = options.control;
      if (control === undefined) throw new Error("no control");
      control.reopen = async (next) => {
        asked.push(next ?? {});
        return { kind: "reopened" };
      };
      await writeFile(join(dir, "index.html"), "<!doctype html><p>two");
      t.watches[0]?.onChange("view");
      await until(() => asked.length === 1, "the view to be reopened");
      return { kind: "closed", by: "user" };
    });
    expect(await run(devArgv("--view", dir), t.env)).toEqual({ code: EXIT.ok });
    const bundle = asked[0]?.bundle;
    expect(bundle).toBeInstanceOf(Bundle);
    const shown = t.served[0]?.options.bundle;
    expect(bundle?.manifest.files[0]?.sha256).not.toBe(shown?.manifest.files[0]?.sha256);
    expect(t.err()).toBe("");
  });

  it("restarts the view, with no new files, when the server's own files change", async () => {
    const dir = await view();
    const server = join(dir, "index.html");
    const asked: ReopenOptions[] = [];
    const t = terminal(async (_server, options) => {
      const control = options.control;
      if (control === undefined) throw new Error("no control");
      control.reopen = async (next) => {
        asked.push(next ?? {});
        return { kind: "reopened" };
      };
      // What the server's file holds has changed (its size does), not only been written to.
      await writeFile(server, "a script that is longer than it was");
      t.watches[0]?.onChange("server");
      await until(() => asked.length === 1, "the view to be reopened");
      return { kind: "closed", by: "user" };
    });
    expect(await run(devArgv("--watch", server), t.env)).toEqual({ code: EXIT.ok });
    expect(asked).toEqual([{}]);
  });

  it("says, once the view has closed, that the terminal cannot reopen a view", async () => {
    const dir = await view();
    let asked = 0;
    const t = terminal(async (_server, options) => {
      const control = options.control;
      if (control === undefined) throw new Error("no control");
      control.reopen = async () => {
        asked += 1;
        return { kind: "unsupported" } satisfies ReopenResult;
      };
      await writeFile(join(dir, "index.html"), "<!doctype html><p>two");
      t.watches[0]?.onChange("view");
      await until(() => asked === 1, "the view to be asked to reopen");
      // Said while the view is up, it would be under the view's screen.
      expect(t.err()).toBe("");
      return { kind: "closed", by: "user" };
    });
    expect(await run(devArgv("--view", dir), t.env)).toEqual({ code: EXIT.ok });
    expect(t.err()).toContain("does not reopen a view when asked");
    expect(t.err()).toContain("changes were not shown");
  });

  it("names a path to watch that is not there, before it starts anything", async () => {
    const t = terminal();
    expect(await run(devArgv("--watch", "/nonexistent/server.py"), t.env)).toEqual({
      code: EXIT.usage,
    });
    expect(t.err()).toContain("--watch /nonexistent/server.py: no such file or directory");
    expect(t.served).toEqual([]);
    expect(t.watches).toEqual([]);
  });

  it("says what could not be watched, and shows no view", async () => {
    const dir = await view();
    const t = terminal();
    for (const [missing, code] of [
      [true, EXIT.usage],
      [false, EXIT.failed],
    ] as const) {
      const env: Environment = {
        ...t.env,
        watch: () => {
          throw new WatchError("cannot watch it: no", missing);
        },
      };
      expect(await run(devArgv("--view", dir), env)).toEqual({ code });
    }
    expect(t.err()).toContain("pencere: cannot watch it: no");
    expect(t.served).toEqual([]);
  });

  it("ends the watching when the terminal had no view to show", async () => {
    const dir = await view();
    const t = terminal(async () => ({ kind: "no-view", reason: "no-host" }));
    expect(await run(devArgv("--view", dir), t.env)).toEqual({ code: EXIT.noView });
    expect(t.watches[0]?.closed).toBe(true);
  });

  it("shows the view's files as they are once the watching has begun, not as they were before", async () => {
    const dir = await view("one");
    const t = terminal();
    const env: Environment = {
      ...t.env,
      watch: (roots, onChange, onError) => {
        // A save in the moment between the files being read and the watching beginning.
        writeFileSync(join(dir, "index.html"), "<!doctype html><p>two");
        return t.env.watch(roots, onChange, onError);
      },
    };
    expect(await run(devArgv("--view", dir), env)).toEqual({ code: EXIT.ok });
    const digest = createHash("sha256").update("<!doctype html><p>two").digest("hex");
    expect(t.served[0]?.options.bundle?.manifest.files[0]?.sha256).toBe(digest);
  });

  describe("a server that ends", () => {
    /** A server that falls, with a message, the first time it is started and runs after that. */
    const flaky = (marker: string) =>
      node(
        `const fs = require("node:fs");
         if (!fs.existsSync(process.argv[1])) {
           fs.writeFileSync(process.argv[1], "started");
           console.error("SyntaxError: unexpected token\\n\\u001b[2Jat server.py:3");
           setTimeout(() => process.exit(7), 30);
         } else {
           process.stdin.resume();
           process.stdin.on("end", () => process.exit(0));
         }`,
        marker,
      );

    /** A `serve` that holds the view until the server falls (the first time) and ends it as the SDK would. */
    const attempts =
      (served: { count: number }): Serve =>
      async (server, options) => {
        served.count += 1;
        await server.connect(new FakeTransport());
        if (served.count > 1) return { kind: "closed", by: "user" };
        await new Promise<void>((resolve) =>
          options.signal?.addEventListener("abort", () => resolve(), { once: true }),
        );
        await server.close();
        return { kind: "closed", by: "aborted" };
      };

    async function setup() {
      const dir = await view();
      const file = join(dir, "server.py");
      await writeFile(file, "print(1)");
      return { dir, file, marker: join(dir, "marker") };
    }

    it("is not the end of pencere dev: it says what the server wrote, waits for a change, and shows the view again", async () => {
      const { file, marker } = await setup();
      const served = { count: 0 };
      const t = terminal(attempts(served));
      const running = run(
        ["dev", "--tool", "t", "--watch", file, "--", ...argv(flaky(marker))],
        t.env,
      );
      await until(() => t.err().includes("Waiting for a change"), "the wait for a change");
      expect(served.count).toBe(1);
      expect(t.err()).toContain("ended (exit status 7) while its view was");
      expect(t.err()).toContain("  SyntaxError: unexpected token");
      expect(t.err()).toContain("  \\u001b[2Jat server.py:3");
      expect(t.err()).not.toContain("\u001b");

      // The server's file is saved again: the view is shown again, with a server that runs.
      await writeFile(file, "print(2)  # fixed");
      t.watches[0]?.onChange("server");
      expect(await running).toEqual({ code: EXIT.ok });
      expect(served.count).toBe(2);
      // Said once, on the screen the person had back; not again for the view that came.
      expect(t.err().match(/^ {2}SyntaxError: unexpected token$/gm)).toHaveLength(1);
      expect(t.err().match(/Waiting for a change/g)).toHaveLength(1);
      expect(t.watches[0]?.closed).toBe(true);
    });

    it("is waited out with the same files: a save that leaves them as they were is not another try", async () => {
      const { file, marker } = await setup();
      const served = { count: 0 };
      const t = terminal(attempts(served));
      const running = run(
        ["dev", "--tool", "t", "--watch", file, "--", ...argv(flaky(marker))],
        t.env,
      );
      await until(() => t.err().includes("Waiting for a change"), "the wait for a change");
      // The events of the write that broke it, late: what the file holds is what it held.
      t.watches[0]?.onChange("server");
      t.watches[0]?.onChange("server");
      await new Promise((resolve) => setTimeout(resolve, 100));
      expect(served.count).toBe(1);
      await writeFile(file, "print(2)  # fixed");
      t.watches[0]?.onChange("server");
      expect(await running).toEqual({ code: EXIT.ok });
      expect(served.count).toBe(2);
    });

    it("ends for good when the person interrupts while it waits", async () => {
      const { file, marker } = await setup();
      const served = { count: 0 };
      const t = terminal(attempts(served));
      const running = run(
        ["dev", "--tool", "t", "--watch", file, "--", ...argv(flaky(marker))],
        t.env,
      );
      await until(() => t.err().includes("Waiting for a change"), "the wait for a change");
      t.emit("SIGINT");
      expect(await running).toEqual({ signal: "SIGINT" });
      expect(served.count).toBe(1);
      expect(t.watches[0]?.closed).toBe(true);
      expect(t.listening("SIGINT") + t.listening("SIGTERM") + t.listening("SIGHUP")).toBe(0);
    });

    it("is the end of pencere run, which watches nothing to try again for", async () => {
      const { marker } = await setup();
      const t = terminal(attempts({ count: 0 }));
      const finish = await run(["run", "--tool", "t", "--", ...argv(flaky(marker))], t.env);
      expect(finish).toEqual({ code: EXIT.failed });
      expect(t.err()).not.toContain("Waiting for a change");
    });
  });
});
