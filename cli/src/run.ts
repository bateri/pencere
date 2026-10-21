/**
 * `pencere run` and `pencere dev`: an MCP server written in any language as a
 * pencere program.
 *
 * What a program does to show its view is `serve` of the SDK, and everything of
 * that is the SDK's: the question to the terminal, the token, the bundle, ⌃C and
 * `bye`, the host starting the view again, the fallback, and closing the view to
 * show it again. What is this file's is what a program of someone else's language
 * cannot do for itself: choosing the tool, making the server a process the SDK can
 * connect to (`connector.ts`), keeping what the server says on stderr for the
 * screen the person gets back, saying what happened, in an exit status as well as
 * in words, and, for `dev`, handing the SDK a control and the changes of the
 * files to reopen the view with (`dev.ts`).
 */

import { BundleError, ViewControl, bundleFromDirectory, serve } from "@pencere/sdk";
import type { Bundle, Connectable, ServeOptions, ServeResult, ToolCall } from "@pencere/sdk";
import { readFileSync } from "node:fs";
import { stat } from "node:fs/promises";
import { resolve } from "node:path";

import { USAGE, UsageError, parse } from "./args.js";
import type { Command } from "./args.js";
import { ProcessConnector } from "./connector.js";
import { Reloader } from "./dev.js";
import type { ReloadTiming } from "./dev.js";
import { DiscoveryError, Interrupted, discoverTool } from "./discover.js";
import { StartError } from "./server-process.js";
import type { CloseTiming, Exit, ServerCommand } from "./server-process.js";
import { Tail } from "./tail.js";
import { WatchError, fingerprintOf, watchRoots } from "./watch.js";
import type { Root, Watch, Watching } from "./watch.js";

/** The exit statuses of `pencere run` (the signals aside, which end the process as they would any other). */
export const EXIT = {
  /** The view ran and closed. */
  ok: 0,
  /** It could not run: the server did not start or ended, the terminal refused or ended the view. */
  failed: 1,
  /** A mistake in the command line. */
  usage: 2,
  /** This terminal does not show pencere views. */
  noView: 3,
} as const;

type Signal = "SIGINT" | "SIGTERM";

/** How `pencere` ended: with a status, or with the signal the person sent, which the process then raises itself. */
export type Finish = { readonly code: number } | { readonly signal: Signal };

/** What `run` takes from the process it is in, so that a test can lend others. */
export interface Environment {
  readonly stdin: { readonly isTTY?: boolean | undefined };
  readonly stdout: { readonly isTTY?: boolean | undefined; write(text: string): unknown };
  readonly stderr: { write(text: string): unknown };
  readonly process: {
    on(event: Signal, listener: () => void): unknown;
    removeListener(event: Signal, listener: () => void): unknown;
  };
  readonly serve: (server: Connectable, options: ServeOptions) => Promise<ServeResult>;
  readonly version: string;
  /** How long the server has to answer a question of the discovery, in milliseconds. */
  readonly discoveryTimeout: number;
  readonly timing?: CloseTiming | undefined;
  /** How `pencere dev` waits for the files to be quiet, and asks again for a view that is not up. */
  readonly reload: ReloadTiming;
  /** Watches files for `pencere dev`. */
  readonly watch: Watch;
}

/** The environment of this process. */
export function systemEnvironment(): Environment {
  const manifest = JSON.parse(
    readFileSync(new URL("../package.json", import.meta.url), "utf8"),
  ) as { version: string };
  return {
    stdin: process.stdin,
    stdout: process.stdout,
    stderr: process.stderr,
    process,
    serve,
    version: manifest.version,
    discoveryTimeout: 30_000,
    reload: { settle: 150, retry: 250, tries: 20 },
    watch: watchRoots,
  };
}

/** Runs the command line `argv` (without `node` and the script) and says how it ended. */
export async function run(argv: readonly string[], env: Environment): Promise<Finish> {
  let command: Command;
  try {
    command = parse(argv);
  } catch (error) {
    if (!(error instanceof UsageError)) throw error;
    env.stderr.write(`pencere: ${error.message}\nTry "pencere --help".\n`);
    return { code: EXIT.usage };
  }
  switch (command.kind) {
    case "help":
      env.stdout.write(USAGE);
      return { code: EXIT.ok };
    case "version":
      env.stdout.write(`pencere ${env.version}\n`);
      return { code: EXIT.ok };
    case "run":
    case "dev":
      return runServer(command, env);
  }
}

type RunCommand = Extract<Command, { kind: "run" | "dev" }>;

/** The command as the person would type it, to say which server a message is about. */
export function shown(spec: ServerCommand): string {
  const word = (text: string): string =>
    /^[\w@%+=:,./-]+$/.test(text) ? text : `'${text.replaceAll("'", "'\\''")}'`;
  return [spec.command, ...spec.args].map(word).join(" ");
}

function describeExit(exit: Exit): string {
  if (exit.signal !== null) return `killed by ${exit.signal}`;
  return `exit status ${exit.code ?? "unknown"}`;
}

async function runServer(command: RunCommand, env: Environment): Promise<Finish> {
  const spec: ServerCommand = { command: command.command, args: command.args };
  const say = (text: string): void => void env.stderr.write(`${text}\n`);
  if (env.stdin.isTTY !== true || env.stdout.isTTY !== true) {
    say(
      `pencere: ${command.kind} shows the view in the terminal it runs in, and its input or output is not a terminal here.`,
    );
    say(
      `${shown(spec)} is an MCP server: an MCP client (a chat app, an editor) takes its own command, not pencere's.`,
    );
    return { code: EXIT.noView };
  }
  if (command.kind === "dev") {
    for (const path of command.watch) {
      try {
        await stat(path);
      } catch (error) {
        say(`pencere: --watch ${path}: ${pathProblem(error)}`);
        return { code: EXIT.usage };
      }
    }
  }

  // ⌃C and SIGTERM end this process in the end, and the server first: the SDK
  // takes the signal while a view is up, and this is for the rest of the time.
  let interrupted: Signal | undefined;
  const interrupt = new AbortController();
  const handlers = (["SIGINT", "SIGTERM"] as const).map((signal) => {
    const handler = (): void => {
      interrupted ??= signal;
      interrupt.abort();
    };
    env.process.on(signal, handler);
    return { signal, handler };
  });
  const tail = new Tail();
  const failure: { exit?: Exit } = {};
  const abort = new AbortController();
  const connector = new ProcessConnector(spec, {
    tail,
    ...(env.timing === undefined ? {} : { timing: env.timing }),
    // The view has nothing to talk to any more: it is ended, and what happened is told after.
    unexpectedExit: (exit) => {
      failure.exit ??= exit;
      abort.abort();
    },
  });
  /** What the server said before it failed, on the screen the person has now. */
  const said = (from: Tail = tail): void => {
    if (from.empty) return;
    say("The end of what it wrote to stderr:");
    for (const line of from.text().replace(/\n$/, "").split("\n")) say(`  ${line}`);
  };
  const interruptedBy = (): Finish | undefined =>
    interrupted === undefined ? undefined : { signal: interrupted };
  let reloading: Reloading | undefined;

  try {
    const chosen = await choose(command, spec, env, interrupt.signal);
    if (!("tool" in chosen)) {
      if (chosen.message !== "") say(chosen.message);
      if (chosen.said !== undefined) said(chosen.said);
      return interruptedBy() ?? { code: chosen.code };
    }
    let bundle: Bundle | undefined;
    if (command.view !== undefined) {
      try {
        bundle = await bundleFromDirectory(command.view);
      } catch (error) {
        say(`pencere: --view ${command.view}: ${viewProblem(error)}`);
        return interruptedBy() ?? { code: EXIT.usage };
      }
    }
    const early = interruptedBy();
    if (early !== undefined) return early;

    // The files are watched before the view is asked for, so that a change that comes while it opens is not missed.
    const control = command.kind === "dev" ? new ViewControl() : undefined;
    if (command.kind === "dev" && control !== undefined) {
      try {
        reloading = await startReloading(command, bundle, control, env);
      } catch (error) {
        if (!(error instanceof WatchError)) throw error;
        say(`pencere: ${error.message}`);
        return interruptedBy() ?? { code: error.missing ? EXIT.usage : EXIT.failed };
      }
    }

    let result: ServeResult | undefined;
    let thrown: unknown;
    try {
      result = await env.serve(connector, {
        tool: chosen.tool,
        ...(bundle === undefined ? {} : { bundle }),
        ...(control === undefined ? {} : { control }),
        signal: abort.signal,
      });
    } catch (error) {
      thrown = error;
    }
    // `serve` lets go of a server without waiting for it; leaving is not for a server that lingers.
    await connector.drain();
    // The view is over: what the files do from here is nothing, and what was learned of them is told now,
    // on the screen the person has back.
    reloading?.close();
    for (const note of reloading?.reloader.notes ?? []) say(`pencere: ${note}`);

    const finish = interruptedBy();
    if (finish !== undefined) return finish;
    if (failure.exit !== undefined) {
      say(
        `pencere: the server ${shown(spec)} ended (${describeExit(failure.exit)}) while its view was ${
          result?.kind === "closed" ? "open" : "opening"
        }.`,
      );
      said();
      return { code: EXIT.failed };
    }
    if (thrown !== undefined) {
      if (thrown instanceof StartError) {
        say(`pencere: ${thrown.message}`);
        return { code: EXIT.failed };
      }
      say(`pencere: ${thrown instanceof Error ? thrown.message : String(thrown)}`);
      said();
      return { code: EXIT.failed };
    }
    if (result === undefined) return { code: EXIT.failed };
    return finishedWith(result, spec, said, say);
  } finally {
    reloading?.close();
    for (const { signal, handler } of handlers) env.process.removeListener(signal, handler);
  }
}

/** What `pencere dev` watches the files with, and what it does about a change. */
interface Reloading {
  readonly reloader: Reloader;
  /** Ends the watching and the acting on changes. */
  close(): void;
}

/** @throws {WatchError} when a path cannot be watched. */
async function startReloading(
  command: Extract<Command, { kind: "dev" }>,
  bundle: Bundle | undefined,
  control: ViewControl,
  env: Environment,
): Promise<Reloading> {
  const view = command.view === undefined ? undefined : resolve(command.view);
  const server = command.watch.map((path) => resolve(path));
  const reloader = new Reloader({
    control,
    ...(view === undefined ? {} : { view }),
    ...(bundle === undefined ? {} : { bundle }),
    ...(server.length === 0 ? {} : { fingerprint: () => fingerprintOf(server) }),
    timing: env.reload,
  });
  // What the server's files hold now is what the view that opens next is made with.
  await reloader.start();
  const roots: Root[] = [
    ...(view === undefined ? [] : [{ path: view, role: "view" as const }]),
    ...server.map((path) => ({ path, role: "server" as const })),
  ];
  const watching: Watching = env.watch(
    roots,
    (role) => (role === "view" ? reloader.viewChanged() : reloader.serverChanged()),
    (message) => reloader.warn(message),
  );
  return {
    reloader,
    close: () => {
      watching.close();
      reloader.stop();
    },
  };
}

/** The tool to show, or what to say about why there is none. */
async function choose(
  command: RunCommand,
  spec: ServerCommand,
  env: Environment,
  signal: AbortSignal,
): Promise<
  | { readonly tool: ToolCall }
  | {
      readonly message: string;
      readonly code: number;
      /** What the server wrote while it was asked, to show with the message. */
      readonly said?: Tail;
    }
> {
  const call = (name: string): { readonly tool: ToolCall } => ({
    tool: { name, ...(command.arguments === undefined ? {} : { arguments: command.arguments }) },
  });
  if (command.tool !== undefined) return call(command.tool);
  // The asked server's stderr is its own: the one of the server that runs the view is not mixed with it.
  const asked = new Tail();
  let found: Awaited<ReturnType<typeof discoverTool>>;
  try {
    found = await discoverTool(spec, asked, {
      version: env.version,
      timeout: env.discoveryTimeout,
      signal,
      ...(env.timing === undefined ? {} : { timing: env.timing }),
    });
  } catch (error) {
    // The caller sees the signal, and says nothing of the question it interrupted.
    if (error instanceof Interrupted) return { message: "", code: EXIT.failed };
    if (error instanceof StartError) {
      return { message: `pencere: ${error.message}`, code: EXIT.failed };
    }
    if (error instanceof DiscoveryError) {
      return { message: `pencere: ${error.message}`, code: EXIT.failed, said: asked };
    }
    throw error;
  }
  switch (found.kind) {
    case "found":
      return call(found.tool);
    case "none":
      return {
        message:
          `pencere: no tool of ${shown(spec)} names a view (a tool whose _meta.ui.resourceUri is a ui:// resource)` +
          (found.tools.length === 0
            ? ": the server has no tools."
            : `: its tools are ${found.tools.join(", ")}.`),
        code: EXIT.failed,
      };
    case "many":
      return {
        message:
          `pencere: ${shown(spec)} has several tools that name a view: ${found.tools.join(", ")}.\n` +
          "Choose one with --tool <name>.",
        code: EXIT.usage,
      };
  }
}

function viewProblem(error: unknown): string {
  if (error instanceof BundleError) return error.message;
  const code = (error as NodeJS.ErrnoException | undefined)?.code;
  if (code === "ENOENT") return "no such directory";
  if (code === "ENOTDIR") return "not a directory";
  if (code === "EACCES") return "permission denied";
  return error instanceof Error ? error.message : String(error);
}

function pathProblem(error: unknown): string {
  const code = (error as NodeJS.ErrnoException | undefined)?.code;
  if (code === "ENOENT") return "no such file or directory";
  if (code === "EACCES") return "permission denied";
  return error instanceof Error ? error.message : String(error);
}

/** What the terminal said when it refused a view, or ended it, in a few words. */
function terminalSaid(reason: "refused" | "failed", code: string | undefined): string {
  switch (code) {
    case "busy":
      return "this pane already shows a view";
    case "policy":
      return "the terminal's policy, or your answer to it, did not allow the view";
    case "limit":
    case "manifest":
      return "the view's files are not what the terminal takes (--view: check the directory)";
    case "bundle":
      return "the terminal could not verify the view's files";
    case "mcp":
      return "the terminal could not start talking to the server";
    case "timeout":
      return "the terminal gave up waiting for the view to start";
    default:
      return reason === "refused"
        ? `the terminal refused to show the view${code === undefined ? "" : ` (${code})`}`
        : `the terminal could not start the view${code === undefined ? "" : ` (${code})`}`;
  }
}

function finishedWith(
  result: ServeResult,
  spec: ServerCommand,
  said: () => void,
  say: (text: string) => void,
): Finish {
  switch (result.kind) {
    case "closed":
      if (result.by === "error") {
        say("pencere: the terminal ended the view with an error.");
        said();
        return { code: EXIT.failed };
      }
      return { code: EXIT.ok };
    case "stdio":
      // `run` asked for a terminal first; this is a terminal that went away in between.
      say("pencere: the terminal went away.");
      return { code: EXIT.noView };
    case "no-view": {
      const { reason, code } = result;
      switch (reason) {
        case "no-host":
          say("pencere: this terminal does not show pencere views.");
          say(
            `${shown(spec)} is an MCP server: add it to an MCP client as it is, or run it with pencere in a terminal that supports pencere.`,
          );
          return { code: EXIT.noView };
        case "no-common-version":
          say(
            "pencere: this terminal and this pencere share no version of the pencere protocol: update the older one.",
          );
          return { code: EXIT.noView };
        case "refused":
        case "failed":
          say(`pencere: ${terminalSaid(reason, code)}.`);
          said();
          return { code: EXIT.failed };
        case "aborted":
          return { code: EXIT.failed };
      }
    }
  }
}
