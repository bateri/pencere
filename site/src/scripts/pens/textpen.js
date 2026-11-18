// textpen, the code editor, drawn with real elements after textpen/docs/editor.png: a tree, tabs, and
// the open file with what git says of it. Taken from the bateri landing page
// (src/scripts/pens/textpen.js). The project and its files are sample data.
// Added here, not on the landing page: textpen({ compare: true }) shows the open file beside its
// last commit, drawn after textpen/docs/compare.png. The last commit's text is sample data too.
import { h, ICON, kbd, acts } from "./kit.js";

export const FILES = {
  "src/server.ts": {
    lang: "ts",
    git: "M",
    at: [19, 5],
    marks: { 3: "m", 6: "a", 7: "a" },
    text: `import { createServer } from "node:http";

const PORT = Number(process.env.PORT ?? 3000);

/** Answers with JSON. */
// Every price is in cents.
// Nothing here rounds.
function send(res, status: number, body: unknown) {
  res.end(JSON.stringify(body));
}

const server = createServer(async (req, res) => {
  if (req.method === "GET") {
    return send(res, 200, []);
  }
  send(res, 404, { error: \`no route for \${req.method}\` });
});

server.listen(PORT);
`,
    // The same file in the last commit, and which of its lines stands beside which line of the file
    // now (0: no line). Past the pairs given here the lines are the same on both sides.
    commit: "3ad91b7",
    beside: [
      [1, 1],
      [2, 2],
      [3, 0],
      [4, 0],
      [5, 3],
      [6, 4],
      [7, 5],
      [0, 6],
      [0, 7],
    ],
    was: `import { createServer } from "node:http";

import { formatPrice } from "./lib/format.ts";

const PORT = Number(process.env.PORT ?? 8080);

/** Answers with JSON. */
function send(res, status: number, body: unknown) {
  res.end(JSON.stringify(body));
}

const server = createServer(async (req, res) => {
  if (req.method === "GET") {
    return send(res, 200, []);
  }
  send(res, 404, { error: \`no route for \${req.method}\` });
});

server.listen(PORT);
`,
  },
  "src/lib/format.ts": {
    lang: "ts",
    git: "M",
    at: [1, 1],
    marks: { 9: "a" },
    text: `const currency = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" });

/** A price in cents, as people read it: 1999 → "$19.99". */
export function formatPrice(cents: number): string {
  return currency.format(cents / 100);
}

export const TAX_RATE = 0.0825;
export const SHIPPING = 499;
`,
  },
  "src/lib/orders.ts": {
    lang: "ts",
    at: [6, 1],
    text: `import { formatPrice } from "./format";

export type Order = { id: string; total: number; status: "paid" | "refunded" };

/** The order as one line of a receipt. */
export function describe(order: Order): string {
  return \`\${order.id}  \${formatPrice(order.total)}  \${order.status}\`;
}
`,
  },
  "src/routes/products.ts": {
    lang: "ts",
    git: "U",
    at: [8, 1],
    text: `import { send } from "../server";

const products = [
  { sku: "SKU-00001", title: "Notebook", price: 8419 },
  { sku: "SKU-00002", title: "Lamp", price: 16338 },
];

export function list(res) {
  send(res, 200, products);
}
`,
  },
  "src/routes/refunds.ts": {
    lang: "ts",
    at: [4, 1],
    text: `import type { Order } from "../lib/orders";

/** A refund never exceeds what was paid. */
export function refund(order: Order, cents: number): Order {
  if (cents > order.total) throw new Error("more than was paid");
  return { ...order, status: "refunded" };
}
`,
  },
  "config.yaml": {
    lang: "yaml",
    at: [2, 1],
    text: `# What the server reads when it starts.
port: 3000
currency: USD
mail:
  from: orders@lantern.example
  retries: 3
`,
  },
  Dockerfile: {
    lang: "docker",
    at: [1, 1],
    text: `FROM node:22-alpine
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --omit=dev
COPY . .
EXPOSE 3000
CMD ["node", "src/server.ts"]
`,
  },
  "README.md": {
    lang: "md",
    git: "M",
    at: [3, 1],
    marks: { 10: "m" },
    text: `# Lantern

A small shop: an API, a worker that sends the mail, and nothing else.

## Run it

    npm install
    npm start

Every price is kept in cents. \`formatPrice\` turns one into text.
`,
  },
  "worker/mailer.py": {
    lang: "py",
    at: [6, 1],
    text: `#!/usr/bin/env python3
"""Sends the mail the shop queues."""
import smtplib
from email.message import EmailMessage

def send(to: str, subject: str, body: str) -> None:
    message = EmailMessage()
    message["To"], message["Subject"] = to, subject
    message.set_content(body)
    with smtplib.SMTP("localhost") as smtp:
        smtp.send_message(message)
`,
  },
};
const LANG = {
  ts: ["TS", "TypeScript"],
  yaml: ["YM", "YAML"],
  docker: ["DK", "Dockerfile"],
  md: ["MD", "Markdown"],
  py: ["PY", "Python"],
};
const TREE = [
  [0, "notes", null, false],
  [0, "src", null, true, "m"],
  [1, "lib", null, true, "m"],
  [2, "format.ts", "src/lib/format.ts"],
  [2, "orders.ts", "src/lib/orders.ts"],
  [1, "routes", null, true, "u"],
  [2, "products.ts", "src/routes/products.ts"],
  [2, "refunds.ts", "src/routes/refunds.ts"],
  [1, "server.ts", "src/server.ts"],
  [0, "worker", null, false],
  [0, "config.yaml", "config.yaml"],
  [0, "Dockerfile", "Dockerfile"],
  [0, "README.md", "README.md"],
];

const escCode = (s) => s.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]);
const tok = (cls, s) => `<i class="tx-${cls}">${escCode(s)}</i>`;
const WORDS = {
  ts: "import|from|export|const|let|function|return|if|else|async|await|new|type|throw|number|string|unknown",
  py: "import|from|def|with|as|return|None|class|if|else",
  docker: "FROM|WORKDIR|COPY|RUN|EXPOSE|CMD|ENV",
};
export function paint(line, lang) {
  if (lang === "md")
    return /^#/.test(line)
      ? tok("kw", line)
      : /^ {4}/.test(line)
        ? tok("str", line)
        : escCode(line).replace(/`[^`]+`/g, (m) => `<i class="tx-fn">${m}</i>`);
  if (lang === "yaml") {
    if (/^\s*#/.test(line)) return tok("cm", line);
    const m = /^(\s*[\w-]+:)(\s*)(.*)$/.exec(line);
    return m
      ? tok("kw", m[1]) + m[2] + (m[3] ? tok(/^\d+$/.test(m[3]) ? "num" : "str", m[3]) : "")
      : escCode(line);
  }
  const cm = lang === "ts" ? "\\/\\/.*$|\\/\\*\\*.*?\\*\\/" : "#.*$";
  const rx = new RegExp(
    `(${cm})|("(?:[^"\\\\]|\\\\.)*"|'(?:[^'\\\\]|\\\\.)*'|\`(?:[^\`\\\\]|\\\\.)*\`)|\\b(\\d+(?:\\.\\d+)?)\\b|\\b(${WORDS[lang]})\\b|\\b([A-Z][A-Z0-9_]{2,})\\b|\\b([A-Za-z_]\\w*)(?=\\()`,
    "g",
  );
  let out = "",
    at = 0,
    m;
  while ((m = rx.exec(line))) {
    out +=
      escCode(line.slice(at, m.index)) +
      tok(m[1] ? "cm" : m[2] ? "str" : m[3] ? "num" : m[4] ? "kw" : m[5] ? "const" : "fn", m[0]);
    at = m.index + m[0].length;
  }
  return out + escCode(line.slice(at));
}
const base = (path) => path.split("/").pop();
const badge = (path) =>
  `<i class="tx-b tx-b--${FILES[path].lang}">${LANG[FILES[path].lang][0]}</i>`;
const gitCls = (path) =>
  FILES[path].git === "M" ? " tx-m" : FILES[path].git === "U" ? " tx-u" : "";

const svg = (d) => `<svg class="pn-ic" viewBox="0 0 16 16" aria-hidden="true">${d}</svg>`;
const CMP = {
  beside: svg(
    `<path d="M4 2.6h5.2L12.4 6v7.4H4Z"/><path d="M9 2.6V6h3.4"/><path d="M6.7 8.6h3M8.2 7.1v3M6.7 11.5h3"/>`,
  ),
  up: svg(`<path d="M8 12.6V3.6M4.4 7.2 8 3.6l3.6 3.6"/>`),
  down: svg(`<path d="M8 3.4v9M4.4 8.8 8 12.4l3.6-3.6"/>`),
  take: svg(`<path d="M3.6 4.4 7.2 8l-3.6 3.6M8.4 4.4 12 8l-3.6 3.6"/>`),
};
const split = (text) => text.replace(/\n$/, "").split("\n");
// The words that differ between two lines: what is left of each when the part they share at both
// ends is taken away, widened to whole words.
function differs(a, b) {
  let head = 0,
    tail = 0;
  while (head < Math.min(a.length, b.length) && a[head] === b[head]) head++;
  while (
    tail < Math.min(a.length, b.length) - head &&
    a[a.length - 1 - tail] === b[b.length - 1 - tail]
  )
    tail++;
  while (head > 0 && /\w/.test(a[head - 1])) head--;
  while (tail > 0 && /\w/.test(a[a.length - tail])) tail--;
  return [a.slice(head, a.length - tail), b.slice(head, b.length - tail)];
}
const marked = (html, word, cls) =>
  word ? html.replace(`>${escCode(word)}</i>`, ` data-w="${cls}">${escCode(word)}</i>`) : html;
// Which line of the last commit stands beside which line of the file now, for the whole file: the pairs
// the file gives, and after them line beside line to the end.
export function sides(file) {
  const pairs = [...file.beside],
    end = split(file.was).length;
  for (
    let [l, r] = [Math.max(...pairs.map((p) => p[0])), Math.max(...pairs.map((p) => p[1]))];
    l < end;
  )
    pairs.push([++l, ++r]);
  return pairs;
}
// The file beside its last commit: one row for each pair of lines, so both sides keep one height.
function beside(file) {
  const was = split(file.was),
    now = split(file.text);
  const pairs = sides(file);
  let open = false,
    before = "",
    changes = 0;
  const html = pairs
    .map(([l, r]) => {
      const a = was[l - 1],
        b = now[r - 1];
      const kind = !r ? "del" : !l ? "add" : a !== b ? "mod" : "";
      const [wa, wb] = kind === "mod" ? differs(a, b) : [];
      // A change is taken from its first line that the file has.
      const take = kind && r && !open;
      open = kind ? open || !!r : false;
      const first = kind !== before ? " is-first" : "";
      if (kind && !before) changes++;
      before = kind;
      const at = r === file.at[0];
      const left = l
        ? `<span class="tx-h${kind ? ` is-${kind}` : ""}"><span class="tx-ln">${l}</span><span class="tx-t">${marked(paint(a, file.lang), wa, "del")}</span></span>`
        : `<span class="tx-h is-gap is-gap--add${first}"></span>`;
      const right = r
        ? `<span class="tx-h tx-h--now${kind ? ` is-${kind}` : ""}${at ? " is-at" : ""}"><span class="tx-go">${take ? CMP.take : ""}</span><span class="tx-ln">${r}</span><span class="tx-t">${marked(paint(b, file.lang), wb, "add")}${at ? `<i class="tx-caret" style="--col:${file.at[1] - 1}"></i>` : ""}</span></span>`
        : `<span class="tx-h tx-h--now is-gap is-gap--del${first}"></span>`;
      return `<div class="tx-r">${left}${right}</div>`;
    })
    .join("");
  return { html, changes };
}

export function textpen({ compare = false } = {}) {
  let tabs = ["src/server.ts", "src/lib/format.ts", "README.md", "worker/mailer.py"],
    at = "src/server.ts";

  const el = h(`
  <div class="pn pn--text">
    <div class="tx-in">
      <div class="tx-side">
        <div class="tx-tools"><span class="is-on">${ICON.files}</span><span>${ICON.search}</span><span class="pn-sp"></span><span>${ICON.file}</span><span>${ICON.folder}</span><span>${ICON.list}</span></div>
        <div class="tx-proj"><b>LANTERN</b><span class="pn-dim">main · 4 changed</span></div>
        <div class="tx-tree" data-tree></div>
      </div>
      <div class="tx-main">
        <div class="tx-tabs" data-tabs></div>
        <div class="tx-cmp" data-cmp hidden></div>
        <div class="tx-code" data-code></div>
      </div>
    </div>
    <div class="pn-keys">
      <span>${ICON.branch}main</span><span class="pn-sp"></span>
      <span class="tx-k">${kbd("⌘", "K")}files</span><span class="tx-k">${kbd("F1")}commands</span><span class="tx-k">${kbd("⌘", "S")}save</span><span class="tx-k">${kbd("⌘", "F")}find</span><span class="tx-k">${kbd("⌃", "W")}close</span>
      <span class="pn-sp"></span><span data-pos></span><span class="tx-k2">Spaces: 2</span><span class="tx-k2">UTF-8</span><span class="tx-k2">LF</span><span data-lang></span>
    </div>
  </div>`);

  const tree = el.querySelector("[data-tree]"),
    bar = el.querySelector("[data-tabs]"),
    code = el.querySelector("[data-code]"),
    head = el.querySelector("[data-cmp]");
  function draw() {
    tree.innerHTML = TREE.map(([depth, name, path, isOpen, dot]) =>
      path
        ? `<button type="button" class="tx-f${path === at ? " is-on" : ""}${gitCls(path)}" data-act="open" data-path="${path}" style="--d:${depth}">${badge(path)}<span class="pn-cut">${name}</span><em>${FILES[path].git || ""}</em></button>`
        : `<span class="tx-f tx-dir${dot ? ` tx-${dot}` : ""}" style="--d:${depth}">${ICON.folder}<span class="pn-cut">${name}</span>${dot ? `<em class="tx-dot"></em>` : ""}</span>`,
    ).join("");
    bar.innerHTML =
      `<span class="tx-menu">${ICON.list}</span>` +
      tabs
        .map(
          (p) =>
            `<button type="button" class="tx-tab${p === at ? " is-on" : ""}${gitCls(p)}" data-act="open" data-path="${p}">${badge(p)}<span>${base(p)}</span>${p === at ? `<u>${ICON.close}</u>` : ""}</button>`,
        )
        .join("") +
      `<span class="pn-sp"></span>${compare && FILES[at].was ? `<span class="tx-cmpb is-on">${CMP.beside}</span>` : ""}<span class="tx-more">${ICON.chevron}</span>`;
    const file = FILES[at],
      lines = file.text.split("\n");
    const two = compare && !!file.was;
    head.hidden = !two;
    code.classList.toggle("is-cmp", two);
    const sides = two ? beside(file) : null;
    if (two)
      head.innerHTML = `<div><span>Last Commit</span><span class="pn-dim">${file.commit}</span></div><div><span class="pn-sp"></span><span>1 of ${sides.changes}</span>${CMP.up}${CMP.down}${ICON.close}</div>`;
    code.innerHTML = two
      ? sides.html
      : lines
          .map((line, i) => {
            const n = i + 1,
              mark = file.marks?.[n];
            return `<div class="tx-l${n === file.at[0] ? " is-at" : ""}"><span class="tx-ln">${n}</span><span class="tx-mk${mark ? ` tx-mk--${mark}` : ""}"></span><span class="tx-fold">${/[{:]$/.test(line) && !/^\s/.test(line) && file.lang !== "yaml" ? "<i></i>" : ""}</span><span class="tx-t">${paint(line, file.lang)}${n === file.at[0] ? `<i class="tx-caret" style="--col:${file.at[1] - 1}"></i>` : ""}</span></div>`;
          })
          .join("");
    el.querySelector("[data-pos]").textContent = `Ln ${file.at[0]}, Col ${file.at[1]}`;
    el.querySelector("[data-lang]").textContent = LANG[file.lang][1];
  }
  draw();

  acts(el, {
    open: (b) => {
      at = b.dataset.path;
      // A file that is not open gets a tab; past four, the oldest tab closes.
      if (!tabs.includes(at)) {
        tabs.push(at);
        if (tabs.length > 4) tabs.shift();
      }
      draw();
    },
  });
  return el;
}
