"use strict";

// The view of the containers tool, written against the MCP Apps messages and
// nothing else: JSON-RPC over postMessage to the host, which is whatever shows
// this page (a chat client's frame, a terminal's pane). It needs no build step
// and no library.

const PROTOCOL_VERSION = "2026-01-26";

const rows = document.getElementById("rows");
const status = document.getElementById("status");

let containers = [];
let chosen = 0;
let busy = false;
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

function say(text, failed = false) {
  status.textContent = text;
  status.className = failed ? "failed" : "";
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

function adopt(result) {
  const listed = result.structuredContent && result.structuredContent.containers;
  if (!Array.isArray(listed)) return false;
  containers = listed;
  chosen = Math.min(chosen, Math.max(containers.length - 1, 0));
  render();
  return true;
}

function cell(content) {
  const td = document.createElement("td");
  if (typeof content === "string") td.textContent = content;
  else td.append(content);
  return td;
}

/** The elements of each row, built once: the page is updated in place, so that the keyboard focus stays where it is. */
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
  tr.append(cell(dot), name, image, ports, cell(button));
  parts[index] = { tr, dot, button, name, image, ports };
  return tr;
}

function update(container, index) {
  const { tr, dot, button, name, image, ports } = parts[index];
  tr.className = container.state + (index === chosen ? " chosen" : "");
  dot.setAttribute("title", container.state);
  button.textContent = container.state === "running" ? "Stop" : "Start";
  button.disabled = busy;
  name.textContent = container.name;
  image.textContent = container.image;
  ports.textContent = container.ports;
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
  const name = container.state === "running" ? "stop_container" : "start_container";
  busy = true;
  render();
  try {
    const result = await request("tools/call", { name, arguments: { id: container.id } });
    if (result.isError) say(textOf(result), true);
    else {
      adopt(result);
      say(textOf(result));
    }
  } catch (error) {
    say(error.message, true);
  } finally {
    busy = false;
    render();
  }
}

function handleNotification(method, params) {
  switch (method) {
    case "ui/notifications/tool-result":
      if (params.isError) say(textOf(params), true);
      else if (adopt(params)) say(containers.length + " containers");
      break;
    case "ui/notifications/tool-cancelled":
      say("The request was cancelled.", true);
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

document.addEventListener("keydown", (event) => {
  if (event.key === "ArrowDown" && chosen < containers.length - 1) chosen += 1;
  else if (event.key === "ArrowUp" && chosen > 0) chosen -= 1;
  else if (event.key === "Enter" || event.key === " ") toggle(chosen);
  else return;
  event.preventDefault();
  render();
});

// `pencere/pane` says this view was written for the terminal: it may take its
// container to be the whole pane and every key to be its own. A host that does
// not know the name ignores it and gives the view a fixed-size container.
say("Waiting for the host…");
request("ui/initialize", {
  appInfo: { name: "containers", version: "1.0.0" },
  appCapabilities: { experimental: { "pencere/pane": {} } },
  protocolVersion: PROTOCOL_VERSION,
}).then(
  (result) => {
    applyHostContext(result.hostContext || {});
    send({ method: "ui/notifications/initialized" });
  },
  (error) => say(error.message, true),
);
