// The page of the containers program, written against the bridge of a page: `call` and
// `subscribe` go to the program, `onContext` is the host's own word about the terminal. The bridge
// is `@pencere/sdk/page`, a module with no dependency that goes in the bundle beside this page (the
// import map in index.html says where); there is no build step and no other library.
import { call, onContext, subscribe } from "@pencere/sdk/page";

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

document.addEventListener("keydown", (event) => {
  if (event.key === "ArrowDown" && chosen < containers.length - 1) chosen += 1;
  else if (event.key === "ArrowUp" && chosen > 0) chosen -= 1;
  else if (event.key === "Enter" || event.key === " ") toggle(chosen);
  else return;
  event.preventDefault();
  render();
});

say("Waiting for the program…");
// The first listener asks the host for the context, so a page that started to listen after the
// host sent it still has it.
onContext(follow);
call("list").then(
  (result) => {
    if (adopt(result)) say(containers.length + " containers");
  },
  (error) => say(wordsOf(error), true),
);
// The program ends a subscription by answering it with an error: nothing more will be pushed.
subscribe("stats", undefined, {
  onPush: hear,
  onError: (error) => say("The readings stopped: " + wordsOf(error), true),
});
