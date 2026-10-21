/** What the tests share: the fixtures of the specification, and a host to talk to. */

import { McpServer } from "@modelcontextprotocol/server";
import {
  RESOURCE_MIME_TYPE,
  registerAppResource,
  registerAppTool,
} from "@modelcontextprotocol/ext-apps/server";
import { readFileSync } from "node:fs";
import { PassThrough } from "node:stream";
import { fileURLToPath } from "node:url";

import { StreamDecoder, encode } from "../src/frame.js";
import type { Direction, Message } from "../src/frame.js";
import { toWire } from "../src/json.js";
import type { Environment } from "../src/serve.js";
import type { Terminal } from "../src/terminal.js";

const FIXTURES = new URL("../../../spec/draft/fixtures/", import.meta.url);

/** The bytes of a fixture, by its path in `manifest.json`. */
export function fixture(path: string): Buffer {
  return readFileSync(fileURLToPath(new URL(path, FIXTURES)));
}

export interface ManifestMessage {
  kind: string;
  id: number;
  control: Record<string, string>;
  payload: string;
}

export interface DecodeEntry {
  file: string;
  direction: Direction;
  note: string;
  scope: "body" | "stream";
  canonical: boolean;
  messages: ManifestMessage[];
  errors: string[];
}

export interface SessionEntry {
  name: string;
  config: { host: { name: string; version: string }; tokens: string[]; store: string[] };
  steps: ({ in: string } | { out: string | null })[];
}

export interface Manifest {
  protocol: number;
  tokens: { issued: string[] };
  decode: DecodeEntry[];
  session: SessionEntry[];
}

export function manifest(): Manifest {
  return JSON.parse(fixture("manifest.json").toString("utf8")) as Manifest;
}

/** The payload of the fixture `file`, which holds one message. */
export function payloadOf(file: string): string {
  const entry = manifest().decode.find((e) => e.file === file);
  const message = entry?.messages[0];
  if (message === undefined) throw new Error(`${file} holds no message`);
  return message.payload;
}

/** The three files of the bundle in the examples of the specification. */
export const EXAMPLE_FILES = {
  "index.html":
    '<!doctype html>\n<meta charset="utf-8">\n<link rel="stylesheet" href="style.css">\n<div id="app"></div>\n<script src="app.js"></script>\n',
  "app.js": 'document.getElementById("app").textContent = "hello from pencere";\n',
  "style.css": "body { margin: 0; font: 14px var(--font-sans, sans-serif); }\n",
} as const;

/** The digests of {@link EXAMPLE_FILES} as the fixtures write them. */
export const EXAMPLE_DIGESTS = {
  "index.html": "aeba6c19a632d2153a9e336a88b385d438c474a996d5be1408f6cd2ca6898e54",
  "app.js": "bcc81d22a717681517bc99bbad72438377e02c19583fae67522475fd8b3ca62c",
  "style.css": "72735d03f2b7e5a9ddb8ebe1c5fde2da88aa74292ba7ac608490a94f0ebbac0e",
} as const;

export const TOKEN = "q7Vh2sB0xJm9kLw4TzN1cA";
export const TOKEN_2 = "Zk3mP8rD5uYb1Xe6HqS0vg";

/** What a test sees of the tty: the calls made on it, in order. */
export class FakeTerminal implements Terminal {
  readonly calls: string[] = [];
  failEnter = false;

  enter(): void {
    this.calls.push("enter");
    if (this.failEnter) throw new Error("no tty");
  }

  restore(): void {
    this.calls.push("restore");
  }
}

/** The process the program runs in, as far as `serve` looks at it. */
export class FakeProcess {
  readonly pid = 4242;
  readonly kills: [number, string][] = [];
  readonly #listeners = new Map<string, Set<(...args: never[]) => void>>();

  on(event: string, listener: (...args: never[]) => void): this {
    let set = this.#listeners.get(event);
    if (set === undefined) this.#listeners.set(event, (set = new Set()));
    set.add(listener);
    return this;
  }

  removeListener(event: string, listener: (...args: never[]) => void): this {
    this.#listeners.get(event)?.delete(listener);
    return this;
  }

  listenerCount(event: string): number {
    return this.#listeners.get(event)?.size ?? 0;
  }

  kill(pid: number, signal: string): boolean {
    this.kills.push([pid, signal]);
    return true;
  }

  /** Delivers `event` to what listens, as the runtime would. */
  emit(event: string, ...args: unknown[]): void {
    for (const listener of [...(this.#listeners.get(event) ?? [])])
      (listener as (...a: unknown[]) => void)(...args);
  }
}

/** One thing the program wrote: a message, or text that is not a frame (an escape sequence). */
export type Written =
  | { readonly type: "message"; readonly message: Message }
  | { readonly type: "text"; readonly text: string };

/** A terminal that hosts pencere, played by the test: it reads what the program writes and writes what a host writes. */
export class Rig {
  readonly input = Object.assign(new PassThrough(), { isTTY: true });
  readonly output = Object.assign(new PassThrough(), { isTTY: true });
  readonly terminal = new FakeTerminal();
  readonly process = new FakeProcess();
  /** Everything the program wrote, in order. */
  readonly written: Written[] = [];
  /** The frames of the program the host could not read: none, in a program that follows the wire. */
  readonly errors: string[] = [];
  readonly #decoder: StreamDecoder;
  readonly #waiters = new Set<() => void>();
  #pending = "";
  #hostIds = 0;
  #read = 0;

  constructor() {
    this.#decoder = new StreamDecoder("to-host", {
      noise: (byte) => {
        this.#pending += String.fromCharCode(byte);
      },
    });
    this.output.on("data", (chunk: Buffer) => {
      this.#decoder.feed(chunk, (event) => {
        if (event.type === "error")
          throw new Error(`the program wrote a bad frame: ${event.error}`);
        this.#flushText();
        this.written.push({ type: "message", message: event.message });
      });
      this.#flushText();
      for (const wake of [...this.#waiters]) wake();
    });
  }

  environment(): Environment {
    return {
      input: this.input,
      output: this.output,
      terminal: this.terminal,
      process: this.process,
    };
  }

  #flushText(): void {
    if (this.#pending === "") return;
    this.written.push({ type: "text", text: this.#pending });
    this.#pending = "";
  }

  /** The messages the program wrote, in order. */
  messages(): Message[] {
    return this.written.flatMap((w) => (w.type === "message" ? [w.message] : []));
  }

  /** Everything the program wrote that is not a frame, joined. */
  text(): string {
    return this.written.flatMap((w) => (w.type === "text" ? [w.text] : [])).join("");
  }

  /** Waits until the program has written one more message that `match` accepts (and returns it). */
  async next(match: string | ((message: Message) => boolean) = () => true): Promise<Message> {
    const accepts = typeof match === "string" ? (m: Message) => m.kind === match : match;
    const deadline = Date.now() + 3000;
    for (;;) {
      while (this.#read < this.written.length) {
        const entry = this.written[this.#read++];
        if (entry?.type === "message" && accepts(entry.message)) return entry.message;
      }
      if (Date.now() > deadline) throw new Error("the program wrote nothing that matches");
      await new Promise<void>((resolve) => {
        const timer = setTimeout(done, 25);
        const waiters = this.#waiters;
        function done(): void {
          clearTimeout(timer);
          waiters.delete(done);
          resolve();
        }
        waiters.add(done);
      });
    }
  }

  /** Waits for a `msg` and returns its JSON. */
  async nextJson(): Promise<Record<string, unknown>> {
    const message = await this.next("msg");
    return JSON.parse(message.payload) as Record<string, unknown>;
  }

  /** The host writes a message into the program's input. */
  say(kind: string, payload: string, re?: number): void {
    this.sayBytes(
      encode("to-program", {
        kind,
        id: ++this.#hostIds,
        payload,
        ...(re === undefined ? {} : { re }),
      }),
    );
  }

  /** The host writes a JSON-RPC message. */
  sayJson(value: unknown): void {
    this.say("msg", toWire(value));
  }

  /** The host writes raw bytes: a reply to `CSI c`, typed characters. */
  sayBytes(bytes: Uint8Array | string): void {
    this.input.write(typeof bytes === "string" ? Buffer.from(bytes, "latin1") : bytes);
  }

  /** Lets what is in flight arrive. */
  async settle(): Promise<void> {
    await new Promise((resolve) => setTimeout(resolve, 30));
  }
}

export function welcomePayload(token: string, features: string[] = ["bundle"], extra = {}): string {
  return toWire({
    version: 1,
    token,
    host: { name: "pencere-test", version: "0.0.0" },
    features,
    ...extra,
  });
}

/** What the tool of {@link makeServer} answers to a call, as the structured content of its result. */
export const CONTAINERS = [
  { id: "c1", name: "web", state: "running" },
  { id: "c2", name: "db", state: "running" },
];

/** An MCP server of the kind a tool author writes: one tool with a view, and the view's document. */
export function makeServer(): McpServer {
  const server = new McpServer({ name: "containers", version: "0.0.0" });
  registerAppTool(
    server,
    "list_containers",
    {
      description: "List the containers",
      _meta: { ui: { resourceUri: "ui://containers/view.html" } },
    },
    async () => ({
      content: [{ type: "text", text: `${CONTAINERS.length} containers` }],
      structuredContent: { containers: CONTAINERS },
    }),
  );
  registerAppResource(server, "Containers", "ui://containers/view.html", {}, async (uri) => ({
    contents: [{ uri: uri.href, mimeType: RESOURCE_MIME_TYPE, text: "<!doctype html><p>view" }],
  }));
  return server;
}
