// toppen, the system monitor, drawn with real elements after toppen/docs/overview.png: the cards and
// their dot graphs move on every second and the table keeps its order by CPU. Taken from the bateri
// landing page (src/scripts/pens/toppen.js). The processes are sample data.
import { h, esc, ICON, kbd, acts } from "./kit.js";

const THREADS = 8;
const COLS = 44,
  ROWS = 8;

export const PROCS = [
  [
    "rustc",
    4312,
    "you",
    104.3,
    851,
    23,
    "running",
    54.87,
    "rustc --crate-name bt_gpu --edition=2021 crates/bt-gpu/src/lib.rs",
  ],
  [
    "node",
    2960,
    "you",
    51.2,
    403,
    24,
    "running",
    460.36,
    "node node_modules/.bin/vite --port 5173",
  ],
  [
    "postgres",
    429,
    "postgres",
    12.7,
    192,
    13,
    "running",
    169.15,
    "/opt/homebrew/opt/postgresql@17/bin/postgres -D /opt/homebrew/var/postgresql@17",
  ],
  ["cargo", 4298, "you", 8.8, 146, 22, "running", 17.55, "cargo build --release"],
  [
    "WindowServer",
    152,
    "_windowserver",
    7.6,
    826,
    23,
    "running",
    9760.75,
    "/System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer -daemon",
  ],
  [
    "bateri",
    1201,
    "you",
    4.2,
    306,
    19,
    "running",
    472.27,
    "/Applications/bateri.app/Contents/MacOS/bateri",
  ],
  [
    "redis-server",
    310,
    "you",
    3.4,
    39,
    6,
    "running",
    419.86,
    "/opt/homebrew/opt/redis/bin/redis-server 127.0.0.1:6379",
  ],
  ["claude", 3317, "you", 3.0, 570, 11, "running", 847.44, "claude"],
  ["java", 2610, "you", 2.6, 589, 44, "running", 945.01, "java -jar build/libs/api.jar"],
  ["python3", 3170, "you", 1.9, 107, 21, "running", 642.02, "python3 -m http.server 8000"],
  [
    "Safari",
    880,
    "you",
    1.5,
    458,
    15,
    "running",
    566.54,
    "/Applications/Safari.app/Contents/MacOS/Safari",
  ],
  [
    "mds_stores",
    412,
    "root",
    1.2,
    393,
    17,
    "running",
    494.12,
    "/System/Library/Frameworks/CoreServices.framework/Frameworks/Metadata.framework/Support/mds_stores",
  ],
  ["docker", 1840, "you", 0.9, 830, 6, "running", 183.24, "com.docker.backend services"],
  ["git", 4401, "you", 0.4, 6.6, 2, "sleeping", 0.5, "git fetch origin"],
  ["zsh", 2821, "you", 0.1, 7.1, 1, "sleeping", 2.58, "-zsh"],
  ["sshd", 99, "root", 0.0, 5.2, 1, "sleeping", 0.71, "/usr/sbin/sshd -D"],
  ["launchd", 1, "root", 0.0, 21, 4, "running", 112.92, "/sbin/launchd"],
].map(([name, pid, user, cpu, mem, thr, state, time, cmd]) => ({
  name,
  pid,
  user,
  cpu,
  base: cpu,
  mem,
  thr,
  state,
  time,
  cmd,
}));

const HEADS = [
  ["name", "Name"],
  ["pid", "PID", 1],
  ["user", "User"],
  ["cpu", "CPU", 1],
  ["mem", "Memory", 1],
  ["thr", "Threads", 1],
  ["state", "State"],
  ["time", "Time", 1],
  ["cmd", "Command"],
];

const mem = (m) =>
  m >= 1024 ? `${(m / 1024).toFixed(1)} GiB` : `${m < 10 ? m.toFixed(1) : Math.round(m)} MiB`;
const time = (s) => {
  const c = Math.round(s * 100);
  return `${Math.floor(c / 6000)}:${((c % 6000) / 100).toFixed(2).padStart(5, "0")}`;
};
const clamp = (v) => Math.min(1, Math.max(0.04, v));

export function toppen() {
  const procs = PROCS.map((p) => ({ ...p }));
  const cards = [
    {
      k: "cpu",
      tone: "accent",
      name: "CPU",
      of: "Apple M1 Pro",
      sub: `Now, on ${THREADS} threads`,
      unit: "%",
      stats: [
        ["Load", "2.43"],
        ["Top core", "97%"],
        ["Frequency", "2.3 GHz"],
      ],
      mid: 0.3,
      wob: 0.07,
    },
    {
      k: "mem",
      tone: "success",
      name: "Memory",
      of: "",
      sub: "In use of 16.0 GiB",
      big: "10.2",
      unit: "GiB",
      pill: "Normal",
      stats: [
        ["App", "8.2 GiB"],
        ["Wired", "1.3 GiB"],
        ["Compressed", "657 MiB"],
      ],
      mid: 0.64,
      wob: 0.012,
    },
    {
      k: "net",
      tone: "info",
      name: "Network",
      of: "en0",
      sub: "Downloading",
      unit: "MiB/s",
      stats: [
        ["Uploading", "87 KiB/s"],
        ["Received", "40 GiB"],
        ["Sent", "6.0 GiB"],
      ],
      mid: 0.13,
      wob: 0.05,
    },
    {
      k: "disk",
      tone: "warning",
      name: "Disk",
      of: "Data",
      sub: "Free of 494.0 GiB",
      big: "193",
      unit: "GiB",
      stats: [
        ["Reading", "354 KiB/s"],
        ["Writing", "673 KiB/s"],
        ["Used", "61%"],
      ],
      mid: 0.36,
      wob: 0.3,
    },
    {
      k: "gpu",
      tone: "ansi5",
      name: "GPU",
      of: "Apple M1 Pro GPU",
      sub: "Now",
      unit: "%",
      stats: [
        ["Memory", "753 MiB"],
        ["Average", "49%"],
        ["Peak", "82%"],
      ],
      mid: 0.44,
      wob: 0.12,
    },
  ];
  let seed = 7;
  const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  for (const c of cards) {
    let v = c.mid;
    c.vals = Array.from(
      { length: COLS },
      () =>
        (v = clamp(
          v +
            (c.mid - v) * 0.3 +
            (rnd() - 0.5) * c.wob * 2 +
            (c.k === "net" && rnd() < 0.09 ? 0.5 : 0),
        )),
    );
  }
  const big = (c) => {
    const v = c.vals.at(-1);
    if (c.k === "cpu") return Math.round(procs.reduce((a, p) => a + p.cpu, 0) / THREADS);
    if (c.k === "gpu") return Math.round(v * 100);
    if (c.k === "net") return (v * 10).toFixed(1);
    return c.big;
  };
  const lit = (v) => Math.max(1, Math.round(v * ROWS));

  const el = h(`
  <div class="pn pn--top">
    <div class="tp-cards">
      ${cards
        .map(
          (c) => `
      <div class="tp-card" style="--tone:var(--bt-${c.tone})" data-card="${c.k}">
        <div class="tp-card__h"><b>${c.name}</b><span class="pn-dim pn-cut">${c.of}</span></div>
        <div class="tp-card__s pn-dim">${c.sub}</div>
        <div class="tp-card__n"><strong data-big>${big(c)}</strong><span>${c.unit}</span>${c.pill ? `<i class="pn-pill pn-pill--ok">${c.pill}</i>` : ""}</div>
        <div class="tp-card__st">${c.stats.map(([l, v]) => `<span><u>${l}</u>${v}</span>`).join("")}</div>
        <div class="tp-graph">${c.vals.map((v) => `<i style="--v:${lit(v)}"></i>`).join("")}</div>
      </div>`,
        )
        .join("")}
    </div>
    <div class="tp-bars">
      ${[
        ["cpu", "CPU", "load 2.43"],
        ["mem", "Memory", "10 GiB"],
        ["net", "Network", ""],
        ["disk", "Disk", "Data"],
        ["gpu", "GPU", ""],
      ]
        .map(([k, name, tail]) =>
          k === "net"
            ? `<div><b>${name}</b><span class="pn-dim tp-bars__t">down <em data-bar-net>1.3</em> MiB/s · up 87 KiB/s</span></div>`
            : `<div><b>${name}</b><i class="tp-bar"><s data-bar="${k}"></s></i><em data-pct="${k}"></em><span class="pn-dim pn-cut">${tail}</span></div>`,
        )
        .join("")}
    </div>
    <div class="tp-table">
      <div class="tp-tools">
        <span class="pn-seg"><b class="is-on">Processes</b><b>Ports</b></span>
        <span class="pn-field">${ICON.search}</span>
        <span class="pn-dim tp-count">54 processes</span>
        <span class="pn-sp"></span>
        <span class="pn-seg tp-shape"><b class="is-on">Flat</b><b>Tree</b><b>Group</b></span>
      </div>
      <div class="tp-th tp-grid">
        ${HEADS.map(([k, label, right]) => `<button type="button" data-act="sort" data-k="${k}" class="${right ? "pn-r" : ""}"><span>${label}</span>${ICON.chevron}</button>`).join("")}
      </div>
      <div class="tp-body" data-body></div>
    </div>
    <div class="pn-keys">
      <span>mac</span><span class="tp-os">macOS 15.4</span><span>up 3d 0h</span><span class="tp-load">load 2.43 2.10 1.84</span>
      <span class="pn-warn" data-paused hidden>paused</span>
      <span class="pn-sp"></span>
      <span class="tp-every">every 1 s</span>
      <span class="tp-k">${kbd("/")}filter</span><span class="tp-k">${kbd("⌘", "K")}commands</span><span class="tp-k">${kbd("?")}keys</span>
      <button type="button" data-act="pause" aria-pressed="false">${kbd("Space")}<span data-pause-label>pause</span></button>
    </div>
  </div>`);

  const body = el.querySelector("[data-body]");
  let sort = "cpu",
    desc = true,
    sel = procs[0].pid,
    paused = false,
    held = false;
  const order = () => {
    const dir = desc ? -1 : 1;
    procs.sort(
      (a, b) =>
        (typeof a[sort] === "string" ? a[sort].localeCompare(b[sort]) : a[sort] - b[sort]) * dir ||
        a.pid - b.pid,
    );
  };
  const paintRows = () => {
    const focused = body.contains(document.activeElement)
      ? document.activeElement.dataset.pid
      : null;
    body.innerHTML = procs
      .map(
        (p) => `
      <button type="button" class="tp-tr tp-grid${p.pid === sel ? " is-sel" : ""}" data-act="pick" data-pid="${p.pid}" aria-pressed="${p.pid === sel}">
        <span class="tp-name pn-cut">${esc(p.name)}</span><span class="pn-r pn-dim">${p.pid}</span><span class="pn-cut">${p.user}</span>
        <span class="pn-r ${p.cpu >= 100 ? "pn-err" : p.cpu >= 50 ? "pn-warn" : ""}">${p.cpu.toFixed(1)}%</span>
        <span class="pn-r">${mem(p.mem)}</span><span class="pn-r">${p.thr}</span>
        <span class="${p.state === "running" ? "pn-ok" : "pn-dim"}">${p.state}</span><span class="pn-r">${time(p.time)}</span>
        <span class="pn-dim pn-cut">${esc(p.cmd)}</span>
      </button>`,
      )
      .join("");
    if (focused) body.querySelector(`[data-pid="${focused}"]`)?.focus({ preventScroll: true });
  };
  const paintHead = () => {
    for (const b of el.querySelectorAll(".tp-th button")) {
      const on = b.dataset.k === sort;
      b.classList.toggle("is-on", on);
      b.classList.toggle("is-up", on && !desc);
      b.setAttribute("aria-sort", on ? (desc ? "descending" : "ascending") : "none");
    }
  };
  const paintCards = () => {
    for (const c of cards) {
      const card = el.querySelector(`[data-card="${c.k}"]`);
      card.querySelector("[data-big]").textContent = big(c);
      const cols = card.querySelectorAll(".tp-graph i");
      c.vals.forEach((v, i) => cols[i].style.setProperty("--v", lit(v)));
    }
    const cpu = big(cards[0]);
    const pct = { cpu, mem: 64, disk: 61, gpu: big(cards[4]) };
    for (const k in pct) {
      el.querySelector(`[data-bar="${k}"]`).style.width = pct[k] + "%";
      el.querySelector(`[data-pct="${k}"]`).textContent = pct[k] + "%";
    }
    el.querySelector("[data-bar-net]").textContent = big(cards[2]);
  };
  order();
  paintRows();
  paintHead();
  paintCards();

  body.addEventListener("pointerenter", () => (held = true));
  body.addEventListener("pointerleave", () => (held = false));
  let down = false;
  body.addEventListener("pointerdown", () => (down = true));
  addEventListener("pointerup", () => (down = false));
  addEventListener("pointercancel", () => (down = false));

  function tick() {
    for (const c of cards) {
      const v = c.vals.at(-1);
      c.vals.push(
        clamp(
          v +
            (c.mid - v) * 0.3 +
            (Math.random() - 0.5) * c.wob * 2 +
            (c.k === "net" && Math.random() < 0.09 ? 0.5 : 0),
        ),
      );
      c.vals.shift();
    }
    for (const p of procs) {
      if (p.base >= 0.2) p.cpu = Math.max(0, p.base * (1 + (Math.random() - 0.5) * 0.14));
      if (p.state === "running") p.time += p.cpu / 100;
    }
    if (!held) order();
    if (!down) paintRows();
    paintCards();
  }

  acts(el, {
    sort: (b) => {
      const k = b.dataset.k;
      if (k === sort) desc = !desc;
      else {
        sort = k;
        desc = !["name", "user", "state", "cmd"].includes(k);
      }
      order();
      paintRows();
      paintHead();
    },
    pick: (b) => {
      sel = +b.dataset.pid;
      paintRows();
    },
    pause: (b) => {
      paused = !paused;
      b.setAttribute("aria-pressed", String(paused));
      el.querySelector("[data-pause-label]").textContent = paused ? "resume" : "pause";
      el.querySelector("[data-paused]").hidden = !paused;
    },
  });

  let timer = 0;
  el.start = () => {
    if (!timer) timer = setInterval(() => paused || tick(), 1000);
  };
  el.stop = () => {
    clearInterval(timer);
    timer = 0;
  };
  return el;
}
