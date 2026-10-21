/**
 * The view's script, run against a page that is only as much DOM as it uses: it
 * must say what MCP Apps says, in the order MCP Apps says it, and draw what the
 * host's messages tell it to. (How it looks is for a person to see.)
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import vm from "node:vm";
import { beforeEach, describe, expect, it } from "vitest";

const SCRIPT = readFileSync(fileURLToPath(new URL("../view/app.js", import.meta.url)), "utf8");

class Element {
  children: Element[] = [];
  textContent = "";
  className = "";
  disabled = false;
  readonly attributes = new Map<string, string>();
  readonly properties = new Map<string, string>();
  readonly style = {
    setProperty: (name: string, value: string) => this.properties.set(name, value),
  };
  private readonly listeners = new Map<string, (event: unknown) => void>();

  constructor(readonly tag: string) {}
  append(...items: Element[]) {
    this.children.push(...items);
  }
  replaceChildren(...items: Element[]) {
    this.children = items;
  }
  setAttribute(name: string, value: string) {
    this.attributes.set(name, value);
  }
  addEventListener(type: string, listener: (event: unknown) => void) {
    this.listeners.set(type, listener);
  }
  fire(type: string, event: unknown = {}) {
    this.listeners.get(type)?.(event);
  }
}

const CONTAINERS = [
  { id: "web", name: "web", image: "nginx:1.27", state: "running", ports: "8080→80" },
  { id: "db", name: "db", image: "postgres:17", state: "stopped", ports: "" },
];

class Page {
  readonly posted: Record<string, unknown>[] = [];
  readonly parent = {
    postMessage: (message: object) => this.posted.push(JSON.parse(JSON.stringify(message))),
  };
  readonly rows = new Element("tbody");
  readonly status = new Element("p");
  readonly root = new Element("html");
  private readonly keys = new Element("document");
  private message: ((event: unknown) => void) | undefined;

  constructor() {
    const document = {
      getElementById: (id: string) => (id === "rows" ? this.rows : this.status),
      createElement: (tag: string) => new Element(tag),
      documentElement: this.root,
      addEventListener: (type: string, listener: (event: unknown) => void) =>
        this.keys.addEventListener(type, listener),
    };
    const window = {
      parent: this.parent,
      addEventListener: (_type: string, listener: (event: unknown) => void) => {
        this.message = listener;
      },
    };
    vm.runInNewContext(SCRIPT, { window, document });
  }

  /** What the host posts to the view. */
  async host(data: object, source: unknown = this.parent) {
    this.message?.({ source, data: { jsonrpc: "2.0", ...data } });
    await new Promise((resolve) => setImmediate(resolve));
  }

  async key(key: string) {
    this.keys.fire("keydown", { key, preventDefault() {} });
    await new Promise((resolve) => setImmediate(resolve));
  }

  /** The answer to what the view asked last. */
  answer(result: object) {
    return this.host({ id: this.posted.at(-1)?.["id"], result });
  }

  /** Up to the point where the host has sent the first tool result. */
  async shown() {
    await this.answer({ hostContext: { theme: "dark", styles: { variables: {} } } });
    await this.host({
      method: "ui/notifications/tool-result",
      params: { content: [], structuredContent: { containers: CONTAINERS } },
    });
  }

  button(row: number): Element {
    return this.rows.children[row]?.children[4]?.children[0] as Element;
  }
}

describe("the view", () => {
  let page: Page;
  beforeEach(() => {
    page = new Page();
  });

  it("opens with ui/initialize, saying it was written for the pane, then says it is initialized", async () => {
    expect(page.posted).toHaveLength(1);
    expect(page.posted[0]).toMatchObject({
      jsonrpc: "2.0",
      method: "ui/initialize",
      params: {
        protocolVersion: "2026-01-26",
        appCapabilities: { experimental: { "pencere/pane": {} } },
      },
    });

    await page.answer({
      hostContext: {
        theme: "dark",
        styles: { variables: { "--color-text-primary": "#fafafa", "--font-mono": "Menlo" } },
      },
    });
    expect(page.posted[1]).toEqual({ jsonrpc: "2.0", method: "ui/notifications/initialized" });
    expect(page.root.attributes.get("data-theme")).toBe("dark");
    expect(page.root.properties.get("--color-text-primary")).toBe("#fafafa");
    expect(page.root.properties.get("--font-mono")).toBe("Menlo");
  });

  it("draws the containers of the tool result", async () => {
    await page.shown();
    expect(page.rows.children).toHaveLength(2);
    expect(page.rows.children[0]?.className).toBe("running chosen");
    expect(page.rows.children[1]?.className).toBe("stopped");
    expect(page.button(0).textContent).toBe("Stop");
    expect(page.button(1).textContent).toBe("Start");
    expect(page.status.textContent).toBe("2 containers");
  });

  it("calls the tool that changes a container, and draws the list it answers with", async () => {
    await page.shown();
    // The page is updated in place: the button that has the keyboard's focus is
    // still the button afterwards.
    const [row, button] = [page.rows.children[0], page.button(0)];
    page.button(0).fire("click");
    expect(page.posted.at(-1)).toMatchObject({
      method: "tools/call",
      params: { name: "stop_container", arguments: { id: "web" } },
    });
    expect(page.button(0).disabled).toBe(true);

    const stopped = CONTAINERS.map((c) => (c.id === "web" ? { ...c, state: "stopped" } : c));
    await page.answer({
      content: [{ type: "text", text: "Stopped web (nginx:1.27)" }],
      structuredContent: { containers: stopped },
    });
    expect(page.rows.children[0]?.className).toBe("stopped chosen");
    expect(page.rows.children[0]).toBe(row);
    expect(page.button(0)).toBe(button);
    expect(page.button(0).disabled).toBe(false);
    expect(page.status.textContent).toBe("Stopped web (nginx:1.27)");
  });

  it("is driven from the keyboard", async () => {
    await page.shown();
    await page.key("ArrowDown");
    expect(page.rows.children[1]?.className).toBe("stopped chosen");
    await page.key("Enter");
    expect(page.posted.at(-1)).toMatchObject({
      method: "tools/call",
      params: { name: "start_container", arguments: { id: "db" } },
    });
  });

  it("says what went wrong, whether the tool or the host said it", async () => {
    await page.shown();
    page.button(0).fire("click");
    await page.answer({ isError: true, content: [{ type: "text", text: "No such container." }] });
    expect(page.status.textContent).toBe("No such container.");
    expect(page.status.className).toBe("failed");

    page.button(1).fire("click");
    await page.host({ id: page.posted.at(-1)?.["id"], error: { code: -32000, message: "denied" } });
    expect(page.status.textContent).toBe("denied");
  });

  it("winds up when the host tears it down, and answers nothing from elsewhere", async () => {
    await page.shown();
    await page.host({ id: 7, method: "ui/resource-teardown", params: { reason: "user" } });
    expect(page.posted.at(-1)).toEqual({ jsonrpc: "2.0", id: 7, result: {} });

    const count = page.posted.length;
    await page.host({ id: 8, method: "ping" }, { not: "the host" });
    expect(page.posted).toHaveLength(count);
  });
});
