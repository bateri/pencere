// sqlpen, the SQL client, drawn with real elements after sqlpen/docs/grid.png: the tables at the
// left, the editor above, the result below. Run runs the statement under the caret; one press on a
// table opens its columns and two show its rows; a column's name in the result sorts by it. The
// database is opened read-only, which the READ ONLY pill in the heading says. Taken from the bateri
// landing page (src/scripts/pens/sqlpen.js). The tables and rows are sample data.
import { h, ICON, kbd, acts } from "./kit.js";

const k = (s) => `<i class="sq-kw">${s}</i>`,
  f = (s) => `<i class="sq-fn">${s}</i>`,
  c = (s) => `<i class="sq-cm">${s}</i>`;
const CODE = [
  [c("-- sqlpen · demo · SQLite 3.50.2"), 1],
  [
    c(
      "-- ⌘Enter runs the statement under the caret (or what is selected) · ⌘E shows how it would run",
    ),
    1,
  ],
  [c("-- Tab completes a word: tables, columns, keywords"), 1],
  ["", 1],
  [
    `${k("select")} c.country, ${f("count")}(*) ${k("as")} orders, ${f("round")}(${f("sum")}(o.total), <i class="sq-num">2</i>) ${k("as")} revenue`,
  ],
  [`${k("from")} orders o`],
  [`${k("join")} customers c ${k("on")} c.id = o.customer_id`],
  [`${k("where")} o.status &lt;&gt; <i class="sq-str">'refunded'</i>`],
  [`${k("group by")} c.country`],
  [`${k("order by")} revenue ${k("desc")};`],
  [""],
  [`${k("select")} * ${k("from")} revenue_by_month;`],
  [""],
];
const TABLES = [
  ["customers", "2k"],
  ["events", "5k"],
  ["order_items", "50k"],
  ["orders", "20k"],
  ["products", "200"],
];

const KINDS = ["office", "audio", "travel", "kitchen", "home"],
  THINGS = ["Notebook", "Lamp", "Chair", "Kettle", "Backpack", "Headphones", "Desk", "Mug"],
  TAGS = ["sale", "eco", "gift", "new"];
const PRODUCTS = {
  said: "select * from products limit 1000",
  count: "200 rows",
  ms: "0 ms",
  cols: [
    ["id", "integer", 1, 62],
    ["sku", "text", 0, 104],
    ["title", "text", 0, 150],
    ["category", "text", 0, 104],
    ["price", "real", 1, 76],
    ["tags", "text", 0, 0],
  ],
  rows: Array.from({ length: 14 }, (_, i) => [
    i + 1,
    `SKU-${String(i + 1).padStart(5, "0")}`,
    `${THINGS[i % 8]} #${i + 1}`,
    KINDS[i % 5],
    +((84.19 + i * 79.19) % 495).toFixed(2),
    `["${TAGS[i % 4]}"]`,
  ]),
};
const BY_COUNTRY = {
  said: "select c.country, count(*) as orders, …",
  count: "6 rows",
  ms: "2 ms",
  cols: [
    ["country", "text", 0, 110],
    ["orders", "integer", 1, 96],
    ["revenue", "real", 1, 130],
    ["", "", 0, 0],
  ],
  rows: [
    ["DE", 4812, 386204.18],
    ["US", 4377, 351930.02],
    ["FR", 2946, 233118.4],
    ["TR", 2105, 171884.95],
    ["JP", 1988, 164202.11],
    ["BR", 1402, 109377.66],
  ].map((r) => [...r, ""]),
};
// Numbers as the database writes them: no thousands separator, and 301.9 stays 301.9.
const cell = (v) => v;

export function sqlpen() {
  let res = PRODUCTS,
    sort = null,
    desc = false,
    openCols = false,
    lastTap = 0;

  const el = h(`
  <div class="pn pn--sql">
    <div class="sq-bar">
      <span class="sq-db">${ICON.db}</span><strong>sqlpen</strong><strong>demo</strong><span class="pn-dim sq-opt">in memory</span>
      <span class="pn-pill pn-pill--info sq-opt2">SQLite 3.50.2</span><span class="pn-pill pn-pill--warn">READ ONLY</span>
      <span class="pn-sp"></span>
      <span class="pn-btn sq-opt">${ICON.layers}Explain ${kbd("⌘E")}</span>
      <button type="button" class="pn-btn pn-btn--go" data-act="run">${ICON.play}Run ${kbd("⌘ Enter")}</button>
    </div>
    <div class="sq-in">
      <div class="sq-tables">
        <div class="sq-th"><strong>Tables</strong><span class="pn-dim">5 · 1 views</span></div>
        ${TABLES.map(([n, count]) =>
          n === "products"
            ? `<button type="button" class="sq-t" data-act="table" aria-expanded="false"><span class="sq-tw">${ICON.right}</span>${ICON.table}<span>${n}</span><span class="pn-dim pn-r">${count}</span></button><div class="sq-cols" data-cols hidden>${PRODUCTS.cols.map(([name, kind], i) => `<span>${name}<i class="pn-dim">${kind}${i ? "" : " · key"}</i></span>`).join("")}</div>`
            : `<span class="sq-t"><span class="sq-tw">${ICON.right}</span>${ICON.table}<span>${n}</span><span class="pn-dim pn-r">${count}</span></span>`,
        ).join("")}
        <span class="sq-t sq-view"><span class="sq-tw">${ICON.right}</span>${ICON.eye}<span>revenue_by_month</span></span>
      </div>
      <div class="sq-work">
        <div class="sq-code">${CODE.map(([line, opt], i) => `<div class="${i === 4 ? "is-at" : ""}${opt ? " sq-opt" : ""}"><span class="sq-ln">${i + 1}</span><span class="sq-fold">${i === 0 || i === 4 ? "▾" : ""}</span><span>${i === 4 ? `<i class="sq-caret"></i>` : ""}${line}</span></div>`).join("")}</div>
        <div class="sq-rh">
          <strong>Results</strong><span class="pn-pill pn-pill--ok" data-count></span><span class="pn-dim" data-ms></span><span class="pn-dim pn-cut sq-opt" data-said></span>
          <span class="pn-sp"></span><span class="pn-btn pn-btn--s">${ICON.copy}Copy</span><span class="pn-btn pn-btn--s">${ICON.download}CSV</span>
        </div>
        <div class="sq-grid" data-grid></div>
      </div>
    </div>
    <div class="pn-keys">
      <span>${kbd("⌘Enter")}run</span><span>${kbd("Tab")}complete</span><span>${kbd("⌘E")}plan</span><span class="sq-opt">${kbd("⇧⌘E")}plan, run</span><span class="sq-opt">${kbd("⌘.")}stop</span>
      <span class="pn-sp"></span><span>Ln 5, Col 1</span><span>SQLite</span>
    </div>
  </div>`);

  const grid = el.querySelector("[data-grid]");
  function paint(arrive) {
    const rows = res.rows.slice();
    if (sort != null)
      rows.sort(
        (a, b) =>
          (typeof a[sort] === "number"
            ? a[sort] - b[sort]
            : String(a[sort]).localeCompare(String(b[sort]))) * (desc ? -1 : 1),
      );
    const tpl = `44px ${res.cols.map(([, , , w]) => (w ? w + "px" : "minmax(0, 1fr)")).join(" ")}`;
    grid.innerHTML = `
      <div class="sq-gh" style="grid-template-columns:${tpl}"><span>#</span>${res.cols
        .map(([name, kind, right], i) =>
          name
            ? `<button type="button" data-act="sort" data-i="${i}" class="${right ? "pn-r" : ""}${sort === i ? " is-on" : ""}${sort === i && desc ? " is-down" : ""}"><b>${name}${ICON.chevron}</b><small>${kind}</small></button>`
            : "<span></span>",
        )
        .join("")}</div>
      ${rows.map((r, n) => `<div class="sq-gr${arrive ? " is-new" : ""}" style="grid-template-columns:${tpl};${arrive ? `animation-delay:${n * 55}ms` : ""}"><span>${n + 1}</span>${r.map((v, i) => `<span class="${res.cols[i][2] ? "pn-r" : ""}${res.cols[i][0] === "tags" ? " sq-json" : ""} pn-cut">${cell(v)}</span>`).join("")}</div>`).join("")}`;
    el.querySelector("[data-count]").textContent = res.count;
    el.querySelector("[data-ms]").textContent = res.ms;
    el.querySelector("[data-said]").textContent = res.said;
  }
  const show = (next) => {
    res = next;
    sort = null;
    desc = false;
    paint(true);
  };
  paint(false);

  acts(el, {
    run: () => show(BY_COUNTRY),
    sort: (b) => {
      const i = +b.dataset.i;
      if (sort === i) desc = !desc;
      else {
        sort = i;
        desc = false;
      }
      paint(false);
    },
    // One press opens or closes the columns; a second right after it (a double press) shows the
    // table's rows.
    table: (b) => {
      const now = Date.now(),
        twice = now - lastTap < 420;
      lastTap = now;
      if (twice) {
        openCols = true;
        show(PRODUCTS);
      } else openCols = !openCols;
      b.setAttribute("aria-expanded", String(openCols));
      el.querySelector("[data-cols]").hidden = !openCols;
    },
  });
  return el;
}
