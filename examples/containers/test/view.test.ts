/**
 * The page's script, run against a document that is only as much DOM as it uses: it must
 * say what the bridge of a page says, follow what the host tells it and draw what the
 * program answers and pushes. (How it looks is for a person to see.)
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

const CONTEXT = {
  mode: "light",
  colors: {
    background: "#ffffff",
    foreground: "#1f2328",
    dim: "#6a6c70",
    accent: "#58a6ff",
    border: "#d2d3d4",
    success: "#3fb950",
    error: "#f85149",
    warning: "#d29922",
    info: "#58a6ff",
  },
  font: "Menlo, monospace",
  size: { width: 800, height: 600 },
  reduceMotion: false,
};

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

  /** What the host posts to the page, from the window that embeds it unless said otherwise. */
  async host(data: unknown, source: unknown = this.parent) {
    this.message?.({ source, data });
    await new Promise((resolve) => setImmediate(resolve));
  }

  async key(key: string) {
    this.keys.fire("keydown", { key, preventDefault() {} });
    await new Promise((resolve) => setImmediate(resolve));
  }

  /** The request the page made, found by the member that names it. */
  sent(member: string): Record<string, unknown> {
    const found = this.posted.find((message) => member in message);
    return (found?.[member] ?? {}) as Record<string, unknown>;
  }

  /** The program's answer to the call the page made last. */
  answer(result: object) {
    const call = this.posted.filter((message) => "call" in message).at(-1)?.["call"] as
      { id: number } | undefined;
    return this.host({ response: { id: call?.id, result } });
  }

  /** Up to the point where the program has answered `list` and the host has sent the context. */
  async shown() {
    await this.host({ context: CONTEXT });
    await this.host({
      response: { id: this.sent("call")["id"], result: { containers: CONTAINERS } },
    });
  }

  button(row: number): Element {
    return this.rows.children[row]?.children[6]?.children[0] as Element;
  }

  cell(row: number, column: number): string {
    return this.rows.children[row]?.children[column]?.textContent ?? "";
  }
}

describe("the page", () => {
  let page: Page;
  beforeEach(() => {
    page = new Page();
  });

  it("asks the host for the context, then the program for the list and the stats", () => {
    expect(page.posted).toEqual([
      { context: {} },
      { call: { id: 1, method: "list" } },
      { subscribe: { id: 2, topic: "stats" } },
    ]);
  });

  it("follows the context: the mode, the nine roles, the font and the size", async () => {
    await page.host({ context: CONTEXT });
    expect(page.root.attributes.get("data-theme")).toBe("light");
    expect(page.root.properties.get("--background")).toBe("#ffffff");
    expect(page.root.properties.get("--foreground")).toBe("#1f2328");
    expect(page.root.properties.get("--info")).toBe("#58a6ff");
    expect(page.root.properties.get("--font")).toBe("Menlo, monospace");
    expect(page.root.properties.get("--pane-width")).toBe("800px");
    expect(page.root.properties.get("--pane-height")).toBe("600px");

    // A context follows every change of the terminal; the members it lacks change nothing.
    await page.host({ context: { mode: "dark", colors: { background: "#0d1117" } } });
    expect(page.root.attributes.get("data-theme")).toBe("dark");
    expect(page.root.properties.get("--background")).toBe("#0d1117");
    expect(page.root.properties.get("--font")).toBe("Menlo, monospace");
  });

  it("takes only the roles of the theme, as colours of its own", async () => {
    await page.host({ context: { colors: { background: "#000000", "x; --evil": "red" } } });
    expect([...page.root.properties.keys()]).toEqual(["--background"]);
  });

  it("draws the containers the program answers with", async () => {
    await page.shown();
    expect(page.rows.children).toHaveLength(2);
    expect(page.rows.children[0]?.className).toBe("running chosen");
    expect(page.rows.children[1]?.className).toBe("stopped");
    expect(page.button(0).textContent).toBe("Stop");
    expect(page.button(1).textContent).toBe("Start");
    expect(page.status.textContent).toBe("2 containers");
    expect(page.cell(0, 1)).toBe("web");
    expect(page.cell(0, 2)).toBe("nginx:1.27");
    expect(page.cell(0, 3)).toBe("8080→80");
    // Nothing has been pushed yet.
    expect(page.cell(0, 4)).toBe("—");
  });

  it("draws what the program pushes to the subscription, for the running containers", async () => {
    await page.shown();
    const subscription = page.sent("subscribe")["id"];
    await page.host({
      push: {
        id: subscription,
        value: { stats: [{ id: "web", cpu: 3.25, memory: 120 }] },
      },
    });
    expect(page.cell(0, 4)).toBe("3.3%");
    expect(page.cell(0, 5)).toBe("120 MiB");
    expect(page.cell(1, 4)).toBe("—");

    // A push of some other subscription, or without the stats, draws nothing.
    await page.host({ push: { id: 99, value: { stats: [{ id: "db", cpu: 1, memory: 1 }] } } });
    await page.host({ push: { id: subscription, value: "nonsense" } });
    expect(page.cell(1, 4)).toBe("—");
    expect(page.cell(0, 4)).toBe("3.3%");
  });

  it("calls the program to change a container, and draws the list it answers with", async () => {
    await page.shown();
    // The page is updated in place: the button that has the keyboard's focus is
    // still the button afterwards.
    const [row, button] = [page.rows.children[0], page.button(0)];
    page.button(0).fire("click");
    expect(page.posted.at(-1)).toEqual({ call: { id: 3, method: "stop", params: { id: "web" } } });
    expect(page.button(0).disabled).toBe(true);

    const stopped = CONTAINERS.map((c) => (c.id === "web" ? { ...c, state: "stopped" } : c));
    await page.answer({ containers: stopped, message: "Stopped web (nginx:1.27)" });
    expect(page.rows.children[0]?.className).toBe("stopped chosen");
    expect(page.rows.children[0]).toBe(row);
    expect(page.button(0)).toBe(button);
    expect(page.button(0).textContent).toBe("Start");
    expect(page.button(0).disabled).toBe(false);
    expect(page.status.textContent).toBe("Stopped web (nginx:1.27)");
  });

  it("is driven from the keyboard", async () => {
    await page.shown();
    await page.key("ArrowDown");
    expect(page.rows.children[1]?.className).toBe("stopped chosen");
    await page.key("Enter");
    expect(page.posted.at(-1)).toEqual({ call: { id: 3, method: "start", params: { id: "db" } } });
  });

  it("says what went wrong, whoever said it", async () => {
    await page.shown();
    page.button(0).fire("click");
    await page.host({
      response: {
        id: 3,
        error: { code: "no-such-container", message: "There is no container." },
      },
    });
    expect(page.status.textContent).toBe("There is no container.");
    expect(page.status.className).toBe("failed");
    expect(page.button(0).disabled).toBe(false);

    // An error with no words has its code to say.
    page.button(1).fire("click");
    await page.host({ response: { id: 4, error: { code: "internal" } } });
    expect(page.status.textContent).toBe("internal");
  });

  it("says so when the program ends the subscription", async () => {
    await page.shown();
    await page.host({ response: { id: 2, error: { code: "not-found" } } });
    expect(page.status.textContent).toBe("The readings stopped: not-found");
    expect(page.status.className).toBe("failed");
  });

  it("takes nothing from another window, and nothing that is not a message of the bridge", async () => {
    await page.host({ context: CONTEXT }, { not: "the host" });
    expect(page.root.attributes.size).toBe(0);

    await page.host("context");
    await page.host(null);
    await page.host({ context: CONTEXT, close: {} });
    await page.host({ context: "dark" });
    await page.host({ unknown: {} });
    expect(page.root.attributes.size).toBe(0);
    expect(page.rows.children).toHaveLength(0);
  });
});
