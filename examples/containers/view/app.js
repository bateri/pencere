"use strict";

// The page of the containers program, written against the bridge of a page and nothing
// else: single-member JSON objects posted to the window that embeds it. `call`,
// `subscribe` and `cancel` go to the program; its `response`, `push` and `progress` come
// back; `context` is the host's own word about the terminal. It needs no build step and
// no library.

// The roles of the terminal's theme that `context` carries (nine of them).
const ROLES = [
  "background",
  "foreground",
  "dim",
  "accent",
  "border",
  "success",
  "error",
  "warning",
  "info",
];

const rows = document.getElementById("rows");
const status = document.getElementById("status");

let containers = [];
let stats = new Map();
let chosen = 0;
let busy = false;
let nextId = 1;

// What each request that is out waits for: a call for its answer, a subscription for its
// pushes. The program numbers nothing: every id here is this page's own.
const calls = new Map();
const subscriptions = new Map();

function post(message) {
  window.parent.postMessage(message, "*");
}

function call(method, params) {
  return new Promise((resolve, reject) => {
    const id = nextId++;
    calls.set(id, { resolve, reject });
    post({ call: params === undefined ? { id, method } : { id, method, params } });
  });
}

function subscribe(topic, onPush) {
  const id = nextId++;
  subscriptions.set(id, onPush);
  post({ subscribe: { id, topic } });
}

function say(text, failed = false) {
  status.textContent = text;
  status.className = failed ? "failed" : "";
}

function wordsOf(error) {
  return String((error && (error.message || error.code)) || "failed");
}

// The terminal's theme, font and size, as the variables the stylesheet reads.
function follow(context) {
  const root = document.documentElement;
  if (context.mode === "dark" || context.mode === "light") {
    root.setAttribute("data-theme", context.mode);
  }
  const colors = context.colors || {};
  for (const role of ROLES) {
    if (typeof colors[role] === "string") root.style.setProperty("--" + role, colors[role]);
  }
  if (typeof context.font === "string") root.style.setProperty("--font", context.font);
  const size = context.size || {};
  if (typeof size.width === "number") root.style.setProperty("--pane-width", size.width + "px");
  if (typeof size.height === "number") root.style.setProperty("--pane-height", size.height + "px");
}

function adopt(result) {
  if (!result || !Array.isArray(result.containers)) return false;
  containers = result.containers;
  chosen = Math.min(chosen, Math.max(containers.length - 1, 0));
  render();
  return true;
}

function hear(value) {
  if (!value || !Array.isArray(value.stats)) return;
  stats = new Map(value.stats.map((stat) => [stat.id, stat]));
  render();
}

function cell(content, className = "") {
  const td = document.createElement("td");
  td.className = className;
  if (typeof content === "string") td.textContent = content;
  else td.append(content);
  return td;
}

// The elements of each row, built once: the page is updated in place, so that the keyboard
// focus stays where it is.
const parts = [];

function build(_container, index) {
  const tr = document.createElement("tr");
  const dot = document.createElement("span");
  dot.className = "state";
  const button = document.createElement("button");
  button.addEventListener("click", () => {
    chosen = index;
    toggle(index);
  });
  const [name, image, ports] = [cell(""), cell(""), cell("")];
  const [cpu, memory] = [cell("", "number"), cell("", "number")];
  tr.append(cell(dot), name, image, ports, cpu, memory, cell(button));
  parts[index] = { tr, dot, button, name, image, ports, cpu, memory };
  return tr;
}

function update(container, index) {
  const { tr, dot, button, name, image, ports, cpu, memory } = parts[index];
  const stat = container.state === "running" ? stats.get(container.id) : undefined;
  tr.className = container.state + (index === chosen ? " chosen" : "");
  dot.setAttribute("title", container.state);
  button.textContent = container.state === "running" ? "Stop" : "Start";
  button.disabled = busy;
  name.textContent = container.name;
  image.textContent = container.image;
  ports.textContent = container.ports;
  cpu.textContent = stat ? stat.cpu.toFixed(1) + "%" : "—";
  memory.textContent = stat ? stat.memory + " MiB" : "—";
}

function render() {
  if (parts.length !== containers.length) {
    parts.length = 0;
    rows.replaceChildren(...containers.map(build));
  }
  containers.forEach(update);
}

async function toggle(index) {
  const container = containers[index];
  if (!container || busy) return;
  busy = true;
  render();
  try {
    const result = await call(container.state === "running" ? "stop" : "start", {
      id: container.id,
    });
    adopt(result);
    say((result && result.message) || "");
  } catch (error) {
    say(wordsOf(error), true);
  } finally {
    busy = false;
    render();
  }
}

function answered(response) {
  const waiting = calls.get(response.id);
  if (waiting) {
    calls.delete(response.id);
    if (response.error) waiting.reject(response.error);
    else waiting.resolve(response.result);
  } else if (subscriptions.delete(response.id) && response.error) {
    // The program ended the subscription: nothing more will be pushed.
    say("The readings stopped: " + wordsOf(response.error), true);
  }
}

window.addEventListener("message", (event) => {
  if (event.source !== window.parent) return;
  const message = event.data;
  if (typeof message !== "object" || message === null) return;
  const members = Object.keys(message);
  const body = message[members[0]];
  if (members.length !== 1 || typeof body !== "object" || body === null) return;
  if (members[0] === "context") follow(body);
  else if (members[0] === "response") answered(body);
  else if (members[0] === "push") {
    const onPush = subscriptions.get(body.id);
    if (onPush) onPush(body.value);
  }
});

document.addEventListener("keydown", (event) => {
  if (event.key === "ArrowDown" && chosen < containers.length - 1) chosen += 1;
  else if (event.key === "ArrowUp" && chosen > 0) chosen -= 1;
  else if (event.key === "Enter" || event.key === " ") toggle(chosen);
  else return;
  event.preventDefault();
  render();
});

say("Waiting for the program…");
// The host sends `context` once the page has loaded, but a page cannot know it was listening
// in time, so it asks.
post({ context: {} });
call("list").then(
  (result) => {
    if (adopt(result)) say(containers.length + " containers");
  },
  (error) => say(wordsOf(error), true),
);
subscribe("stats", hear);
