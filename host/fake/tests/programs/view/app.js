"use strict";

// A page written against the interface of the `web` kind, and nothing else: the host's `context`
// comes in as a message, and the page calls its program with a `call` and reads the `response`.
// The stand-in host does not run it; it plays the same messages from the command line.

let nextId = 1;
const pending = new Map();

function send(message) {
  window.parent.postMessage(message, "*");
}

function call(method, params) {
  return new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    send({ call: { id, method, params } });
  });
}

function paint(colors) {
  const root = document.documentElement.style;
  for (const [role, color] of Object.entries(colors)) root.setProperty(`--${role}`, color);
}

window.addEventListener("message", (event) => {
  const message = event.data;
  if (message === null || typeof message !== "object") return;
  if (message.context) paint(message.context.colors);
  const response = message.response;
  if (response === undefined || !pending.has(response.id)) return;
  const { resolve, reject } = pending.get(response.id);
  pending.delete(response.id);
  if (response.error) reject(new Error(response.error.message ?? response.error.code));
  else resolve(response.result);
});

// A page that starts listening after the host sent its context asks for it again.
send({ context: {} });

call("list").then(
  ({ containers }) => {
    const rows = document.getElementById("rows");
    for (const container of containers) {
      const row = document.createElement("li");
      row.textContent = `${container.name}  ${container.image}  ${container.state}`;
      if (container.state === "stopped") row.className = "stopped";
      rows.append(row);
    }
  },
  (error) => {
    document.getElementById("status").textContent = error.message;
  },
);
