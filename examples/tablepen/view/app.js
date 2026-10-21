"use strict";

// The view of tablepen, written against the MCP Apps messages and nothing else:
// JSON-RPC over postMessage to the host, which is whatever shows this page. It
// needs no build step and no library.
//
// The table comes in the result of `show_table`. When the program was asked to
// let the person choose, rows are marked and handed back with `choose_rows`; the
// program closes the view itself once it has them.

const PROTOCOL_VERSION = "2026-01-26";

const filterBox = document.getElementById("filter");
const status = document.getElementById("status");
const head = document.getElementById("head");
const body = document.getElementById("rows");
const keys = document.getElementById("keys");
const actions = document.getElementById("actions");
const chooseButton = document.getElementById("choose");
const cancelButton = document.getElementById("cancel");

let columns = [];
let rows = [];
let total = 0;
let select = false;
/** The places of the rows shown, filtered and sorted. */
let shown = [];
/** The place in `shown` of the row the keyboard is on. */
let cursor = 0;
const chosen = new Set();
let sort = { column: -1, descending: false };
let busy = false;
/** Each shown row's element and its box, by the row's place in the table. */
const elements = new Map();

let nextId = 1;
const pending = new Map();

function send(message) {
  window.parent.postMessage({ jsonrpc: "2.0", ...message }, "*");
}

function request(method, params) {
  return new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    send({ id, method, params });
  });
}

function textOf(result) {
  const first = (result.content || []).find((item) => item.type === "text");
  return first ? first.text : "";
}

function applyHostContext(context) {
  const root = document.documentElement;
  if (context.theme) root.setAttribute("data-theme", context.theme);
  const variables = (context.styles && context.styles.variables) || {};
  for (const [name, value] of Object.entries(variables)) {
    if (typeof value === "string") root.style.setProperty(name, value);
  }
}

const count = (n, word) => `${n.toLocaleString()} ${word}${n === 1 ? "" : "s"}`;

function say(text, failed = false) {
  status.textContent = text;
  status.className = failed ? "failed" : "";
}

function sayCounts() {
  const parts = [count(rows.length, "row")];
  if (total > rows.length)
    parts[0] = `first ${count(rows.length, "row")} of ${total.toLocaleString()}`;
  if (shown.length !== rows.length) parts.push(`${shown.length.toLocaleString()} shown`);
  if (select) parts.push(`${chosen.size.toLocaleString()} chosen`);
  say(parts.join(" · "));
  chooseButton.disabled = busy || chosen.size === 0;
  chooseButton.textContent = chosen.size === 0 ? "Choose" : `Choose ${count(chosen.size, "row")}`;
}

// ---- the table -------------------------------------------------------------

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

function arrange() {
  // The keyboard stays on its row when the rows are filtered or sorted again.
  const here = shown[cursor];
  const needle = filterBox.value.trim().toLowerCase();
  shown = [];
  for (let at = 0; at < rows.length; at++) {
    if (needle === "" || rows[at].some((cell) => cell.toLowerCase().includes(needle)))
      shown.push(at);
  }
  if (sort.column >= 0) {
    const by = sort.column;
    const sign = sort.descending ? -1 : 1;
    shown.sort((a, b) => sign * collator.compare(rows[a][by] ?? "", rows[b][by] ?? "") || a - b);
  }
  const place = shown.indexOf(here);
  cursor = place >= 0 ? place : Math.min(cursor, Math.max(shown.length - 1, 0));
  drawRows();
}

function drawHead() {
  const cells = [];
  if (select) {
    const th = document.createElement("th");
    th.className = "box";
    th.setAttribute("aria-label", "Chosen");
    cells.push(th);
  }
  columns.forEach((name, at) => {
    const th = document.createElement("th");
    const mark = sort.column === at ? (sort.descending ? " ▼" : " ▲") : "";
    th.textContent = name + mark;
    th.className = sort.column === at ? "sorted" : "";
    th.title = "Sort by " + name;
    th.addEventListener("click", () => sortBy(at));
    cells.push(th);
  });
  head.replaceChildren(...cells);
}

function drawRows() {
  elements.clear();
  const trs = shown.map((at, place) => {
    const tr = document.createElement("tr");
    let box;
    if (select) {
      box = document.createElement("input");
      box.type = "checkbox";
      box.tabIndex = -1;
      box.setAttribute("aria-label", "Choose this row");
      const td = document.createElement("td");
      td.append(box);
      tr.append(td);
    }
    const row = rows[at];
    for (let column = 0; column < columns.length; column++) {
      const td = document.createElement("td");
      td.textContent = row[column] ?? "";
      tr.append(td);
    }
    tr.addEventListener("click", () => {
      cursor = place;
      if (select) toggle(at);
      else update();
    });
    elements.set(at, { tr, box });
    return tr;
  });
  body.replaceChildren(...trs);
  update();
}

/** Marks the cursor and the chosen rows, without building the rows again. */
function update() {
  for (const [at, { tr, box }] of elements) {
    const chosenRow = chosen.has(at);
    tr.className = (chosenRow ? "chosen " : "") + (shown[cursor] === at ? "cursor" : "");
    if (box) box.checked = chosenRow;
  }
  sayCounts();
}

function moveTo(place) {
  if (shown.length === 0) return;
  cursor = Math.max(0, Math.min(shown.length - 1, place));
  update();
  const here = elements.get(shown[cursor]);
  if (here) here.tr.scrollIntoView({ block: "nearest" });
}

function sortBy(column) {
  if (sort.column !== column) sort = { column, descending: false };
  else if (!sort.descending) sort = { column, descending: true };
  else sort = { column: -1, descending: false };
  drawHead();
  arrange();
}

function toggle(at) {
  if (!select || at === undefined || busy) return;
  if (chosen.has(at)) chosen.delete(at);
  else chosen.add(at);
  update();
}

/** Chooses every row shown, or, when all of them are chosen already, none of them. */
function toggleShown() {
  if (!select || busy) return;
  const all = shown.length > 0 && shown.every((at) => chosen.has(at));
  for (const at of shown) {
    if (all) chosen.delete(at);
    else chosen.add(at);
  }
  update();
}

// ---- handing back and leaving ------------------------------------------------

async function choose() {
  if (!select || busy || chosen.size === 0) return;
  busy = true;
  sayCounts();
  try {
    const result = await request("tools/call", {
      name: "choose_rows",
      arguments: { rows: [...chosen].sort((a, b) => a - b) },
    });
    if (result.isError) {
      busy = false;
      sayCounts();
      say(textOf(result), true);
    } else say(textOf(result));
    // The program closes the view once it has the rows.
  } catch (error) {
    busy = false;
    sayCounts();
    say(error.message, true);
  }
}

/** Asks the host to close the view: nothing is handed back. */
function leave() {
  send({ method: "ui/notifications/request-teardown" });
}

chooseButton.addEventListener("click", choose);
cancelButton.addEventListener("click", leave);
filterBox.addEventListener("input", arrange);

filterBox.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    if (filterBox.value !== "") {
      filterBox.value = "";
      arrange();
    } else filterBox.blur();
  } else if (event.key === "Enter" || event.key === "ArrowDown") filterBox.blur();
  else return;
  event.preventDefault();
});

document.addEventListener("keydown", (event) => {
  if (event.target === filterBox || event.metaKey || event.ctrlKey || event.altKey) return;
  const page = Math.max(1, Math.floor(document.getElementById("scroll").clientHeight / 28) - 1);
  switch (event.key) {
    case "ArrowDown":
    case "j":
      moveTo(cursor + 1);
      break;
    case "ArrowUp":
    case "k":
      moveTo(cursor - 1);
      break;
    case "PageDown":
      moveTo(cursor + page);
      break;
    case "PageUp":
      moveTo(cursor - page);
      break;
    case "Home":
      moveTo(0);
      break;
    case "End":
      moveTo(shown.length - 1);
      break;
    case " ":
      toggle(shown[cursor]);
      break;
    case "a":
      toggleShown();
      break;
    case "/":
      filterBox.focus();
      break;
    case "Enter":
      choose();
      break;
    case "Escape":
      leave();
      break;
    default:
      return;
  }
  event.preventDefault();
});

function adopt(result) {
  const table = result.structuredContent;
  if (!table || !Array.isArray(table.columns) || !Array.isArray(table.rows)) return false;
  columns = table.columns.map(String);
  rows = table.rows.map((row) => (Array.isArray(row) ? row.map(String) : []));
  total = typeof table.total === "number" ? table.total : rows.length;
  select = table.select === true;
  actions.hidden = !select;
  keys.innerHTML = select
    ? "<kbd>space</kbd> choose · <kbd>a</kbd> all shown · <kbd>/</kbd> filter · <kbd>⏎</kbd> hand back · <kbd>esc</kbd> cancel"
    : "<kbd>/</kbd> filter · <kbd>↑</kbd> <kbd>↓</kbd> move · click a heading to sort · <kbd>esc</kbd> close";
  drawHead();
  arrange();
  return true;
}

// ---- the host ----------------------------------------------------------------

function handleNotification(method, params) {
  switch (method) {
    case "ui/notifications/tool-result":
      if (params.isError) say(textOf(params), true);
      else if (!adopt(params)) say("The program sent no table.", true);
      break;
    case "ui/notifications/tool-cancelled":
      say("The table did not come: " + (params.reason || "the call was cancelled"), true);
      break;
    case "ui/notifications/host-context-changed":
      applyHostContext(params);
      break;
  }
}

function handleRequest(id, method) {
  // The host asks the view to wind up before it takes it down, and may ping it.
  if (method === "ui/resource-teardown" || method === "ping") send({ id, result: {} });
  else send({ id, error: { code: -32601, message: "method not found" } });
}

window.addEventListener("message", (event) => {
  if (event.source !== window.parent) return;
  const message = event.data;
  if (!message || message.jsonrpc !== "2.0") return;
  if (typeof message.method === "string") {
    if (message.id === undefined) handleNotification(message.method, message.params || {});
    else handleRequest(message.id, message.method);
    return;
  }
  const waiting = pending.get(message.id);
  if (!waiting) return;
  pending.delete(message.id);
  if (message.error) waiting.reject(new Error(message.error.message));
  else waiting.resolve(message.result);
});

// `pencere/pane` says this view was written for the terminal: it may take its
// container to be the whole pane and every key to be its own.
say("Waiting for the table…");
request("ui/initialize", {
  appInfo: { name: "tablepen", version: "1.0.0" },
  appCapabilities: { experimental: { "pencere/pane": {} } },
  protocolVersion: PROTOCOL_VERSION,
}).then(
  (result) => {
    applyHostContext(result.hostContext || {});
    send({ method: "ui/notifications/initialized" });
  },
  (error) => say(error.message, true),
);
