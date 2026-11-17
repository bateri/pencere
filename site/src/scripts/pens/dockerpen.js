// dockerpen, Docker in the pane, drawn with real elements after dockerpen/docs/containers.png. The
// containers are grouped by compose project; a row opens its detail beside the list (above it in a
// narrow pane), All / Running / Stopped / Issues filter the list, and a group's heading folds it.
// "Issues" are the unhealthy and those that exited with an error: a container that was merely
// stopped is not one. Taken from the bateri landing page (src/scripts/pens/dockerpen.js). The
// containers are sample data.
import { h, ICON, kbd, spark, acts } from "./kit.js";

const C = [
  {
    name: "shop-api-1",
    group: "shop",
    service: "api",
    image: "node:22-alpine",
    state: "healthy",
    tone: "ok",
    up: "up 3m",
    cpu: 12.5,
    mem: ["48.2 MiB", "9.4% of 512 MiB", 0.09],
    net: ["1.46 KiB/s", "2.05 KiB/s"],
    disk: ["0 B/s", "4.00 KiB/s · 11 procs"],
    health: ["healthy", "ok", "CMD wget -q -O /dev/null http://localhost:3000/healthz", "ok"],
    command: "docker-entrypoint.sh node server.js",
    restarts: "2 · policy on-failure:3",
  },
  {
    name: "shop-db-1",
    group: "shop",
    service: "db",
    image: "postgres:17-alpine",
    state: "starting",
    tone: "warn",
    up: "up 3m",
    cpu: 200,
    mem: ["212 MiB", "20.7% of 1.00 GiB", 0.21],
    net: ["3.10 KiB/s", "9.42 KiB/s"],
    disk: ["1.20 MiB/s", "6.80 MiB/s · 9 procs"],
    health: ["starting", "warn", "CMD pg_isready -U shop", "no answer yet"],
    command: "docker-entrypoint.sh postgres",
    restarts: "0 · policy unless-stopped",
  },
  {
    name: "shop-mailer-1",
    group: "shop",
    service: "mailer",
    image: "python:3.13-slim",
    state: "exited 1",
    tone: "err",
    up: "2m ago",
    off: true,
    issue: true,
    why: "Exited with code 1, 2m ago.",
    command: "python mailer.py",
    restarts: "3 · policy on-failure:3",
  },
  {
    name: "shop-search-1",
    group: "shop",
    service: "search",
    image: "meilisearch:v1.11",
    state: "unhealthy",
    tone: "err",
    up: "up 3m",
    issue: true,
    cpu: 3.2,
    mem: ["301 MiB", "58.8% of 512 MiB", 0.59],
    net: ["0 B/s", "0 B/s"],
    disk: ["0 B/s", "0 B/s · 4 procs"],
    health: ["unhealthy", "err", "CMD curl -f http://localhost:7700/health", "connection refused"],
    command: "meilisearch --env production",
    restarts: "0 · policy unless-stopped",
  },
  {
    name: "shop-web-1",
    group: "shop",
    service: "web",
    image: "nginx:alpine",
    state: "running",
    tone: "ok",
    up: "up 3m",
    cpu: 0.1,
    mem: ["6.4 MiB", "2.5% of 256 MiB", 0.03],
    net: ["212 B/s", "1.10 KiB/s"],
    disk: ["0 B/s", "0 B/s · 5 procs"],
    command: "nginx -g 'daemon off;'",
    restarts: "0 · policy unless-stopped",
  },
  {
    name: "toolbox",
    group: "standalone",
    service: "",
    image: "redis:alpine",
    state: "running",
    tone: "ok",
    up: "up 1h",
    cpu: 0.3,
    mem: ["3.1 MiB", "of 11.7 GiB", 0.01],
    net: ["0 B/s", "0 B/s"],
    disk: ["0 B/s", "0 B/s · 4 procs"],
    command: "docker-entrypoint.sh redis-server",
    restarts: "0 · policy no",
  },
  {
    name: "blog-web-1",
    group: "blog",
    service: "web",
    image: "nginx:alpine",
    state: "exited 0",
    tone: "off",
    up: "3d ago",
    off: true,
    why: "Stopped, 3d ago.",
    command: "nginx -g 'daemon off;'",
    restarts: "0 · policy no",
  },
];
const GROUPS = [
  ["shop", "shop"],
  ["standalone", "Standalone"],
  ["blog", "blog"],
];
const FILTERS = [
  ["all", "All", () => true],
  ["running", "Running", (c) => !c.off],
  ["stopped", "Stopped", (c) => c.off],
  ["issues", "Issues", (c) => c.issue],
];
const count = (k) => C.filter(FILTERS.find((f) => f[0] === k)[2]).length;
const wave = (base, n = 24, seed = 3) =>
  Array.from({ length: n }, (_, i) =>
    Math.min(
      1,
      Math.max(0.05, base + Math.sin(i * 0.9 + seed) * 0.08 + Math.sin(i * 2.3 + seed) * 0.05),
    ),
  );

export function dockerpen() {
  let filter = "all",
    sel = "shop-api-1",
    open = "auto";
  const folded = new Set(["blog"]);
  const live = Object.fromEntries(C.filter((c) => c.cpu != null).map((c) => [c.name, c.cpu]));

  const el = h(`
  <div class="pn pn--dock">
    <div class="dk-in">
      <div class="dk-nav">
        ${[
          ["Containers", "", 1],
          ["Images", 2],
          ["Volumes", 3],
          ["Networks", 4],
          ["System", 5],
          ["Activity", 6],
        ]
          .map(
            ([n, k, on]) =>
              `<span class="${on ? "is-on" : ""}">${n}${on ? `<b class="pn-pill pn-pill--err">${count("issues")}</b>` : `<i>${k}</i>`}</span>`,
          )
          .join("")}
        <div class="dk-host"><span><i class="dk-dot dk-dot--ok"></i>Ubuntu 24.04 LTS</span><span>docker 29.0.1</span><span>6 cpus · 11.7 GiB</span></div>
      </div>
      <div class="dk-list">
        <div class="dk-head">
          <strong>Containers</strong><span class="pn-dim pn-cut">${count("running")} running · ${C.length} total</span>
          <span class="pn-sp"></span>
          <span class="pn-btn">Pull… ${kbd("u")}</span><span class="pn-btn">Clean up… ${kbd("c")}</span>
        </div>
        <div class="dk-filter">
          <span class="pn-field"><span class="pn-sp"></span>${kbd("/")}</span>
          <span class="pn-seg" role="group" aria-label="Show">${FILTERS.map(([k, n]) => `<button type="button" data-act="filter" data-k="${k}" aria-pressed="${k === "all"}">${n}<i class="${k === "issues" ? "pn-err" : ""}">${count(k)}</i></button>`).join("")}</span>
        </div>
        <div class="dk-cols dk-row"><span></span><span>NAME</span><span>IMAGE</span><span>STATE</span><span class="dk-cpu">CPU</span><span class="pn-r">UP</span></div>
        <div class="dk-rows" data-rows></div>
      </div>
      <div class="dk-detail" data-detail data-open="auto"></div>
    </div>
    <div class="pn-keys">
      <span>${kbd("↑↓")}choose</span><span>${kbd("s")}stop</span><span>${kbd("r")}restart</span><span>${kbd("e")}shell</span><span class="dk-k">${kbd("[ ]")}tabs</span><span class="dk-k">${kbd("Tab")}list</span><span class="dk-k">${kbd("Esc")}close</span>
      <span class="pn-sp"></span><span>${kbd("?")}help</span><span>${kbd("^C")}quit</span>
    </div>
  </div>`);

  const rows = el.querySelector("[data-rows]"),
    detail = el.querySelector("[data-detail]");
  const cpuText = (c) =>
    c.cpu == null
      ? "–"
      : `${live[c.name] >= 100 ? Math.round(live[c.name]) : live[c.name].toFixed(1)}%`;

  function paintRows() {
    const keep = FILTERS.find((f) => f[0] === filter)[2];
    rows.innerHTML = GROUPS.map(([g, title]) => {
      const all = C.filter((c) => c.group === g),
        shown = all.filter(keep);
      if (!shown.length) return "";
      const up = all.filter((c) => !c.off).length,
        issues = all.filter((c) => c.issue).length;
      const closed = filter === "all" && folded.has(g);
      return `
      <button type="button" class="dk-group" data-act="fold" data-g="${g}" aria-expanded="${!closed}">
        <span class="dk-tw${closed ? "" : " is-open"}">${ICON.right}</span><b>${title}</b><span class="pn-dim">${up}/${all.length} up</span>
        ${issues ? `<span class="pn-pill pn-pill--err">${issues} with issues</span>` : ""}
        <span class="pn-sp"></span><span class="dk-bars">${all.map((c) => `<i class="dk-${c.tone}"></i>`).join("")}</span>
      </button>
      ${
        closed
          ? ""
          : shown
              .map(
                (c) => `
      <button type="button" class="dk-row dk-c${c.off ? " is-off" : ""}${c.name === sel && open !== false ? " is-sel" : ""}" data-act="pick" data-name="${c.name}">
        <span><i class="dk-dot dk-dot--${c.tone}"></i></span><b class="pn-cut">${c.name}</b><span class="pn-dim pn-cut">${c.image}</span>
        <span class="dk-${c.tone}">${c.state}</span>
        <span class="dk-cpu">${c.cpu == null ? `<u class="pn-dim">–</u>` : `${spark(wave(Math.min(0.85, 0.12 + c.cpu / 260), 12, c.name.length), 44, 14)}<em data-cpu="${c.name}">${cpuText(c)}</em>`}</span>
        <span class="pn-dim pn-r">${c.up}</span>
      </button>`,
              )
              .join("")
      }`;
    }).join("");
    for (const b of el.querySelectorAll("[data-act=filter]"))
      b.setAttribute("aria-pressed", String(b.dataset.k === filter));
  }

  function paintDetail() {
    const c = C.find((x) => x.name === sel);
    detail.dataset.open = String(open);
    const card = (label, big, sub, graph) =>
      `<div class="dk-card"><u>${label}</u><strong>${big}</strong><span class="pn-dim">${sub}</span>${graph || ""}</div>`;
    detail.innerHTML = `
      <div class="dk-dh">
        <i class="dk-dot dk-dot--${c.tone}"></i><strong class="pn-cut">${c.name}</strong>
        <span class="pn-sp"></span>
        <span class="pn-pill pn-pill--${c.tone === "off" ? "off" : c.tone}">${c.state}</span>
        <button type="button" class="pn-btn pn-btn--s" data-act="close">Close ${kbd("Esc")}</button>
      </div>
      <div class="dk-sub pn-dim pn-cut">${c.image}${c.service ? ` · ${c.group} / ${c.service}` : ""}</div>
      <div class="dk-do">
        ${(c.off ? ["Start"] : ["Stop", "Restart", "Pause", "Shell"]).map((n) => `<span class="pn-btn pn-btn--s">${n}</span>`).join("")}
        <span class="pn-sp"></span><span class="pn-btn pn-btn--s dk-rm">Remove…</span>
      </div>
      <div class="dk-tabs"><span class="is-on">Overview</span><span>Logs</span><span>Environment</span><span>Processes</span><span>Inspect</span></div>
      <div class="dk-scroll">
        ${
          c.off
            ? `<div class="dk-card dk-card--wide"><u>STOPPED</u><span>${c.why}</span></div>`
            : `
        <div class="dk-cards">
          ${card("CPU", `<em data-cpu-big>${cpuText(c)}</em>`, "of one processor", spark(wave(Math.min(0.8, 0.12 + c.cpu / 260), 28, 2), 120, 30, "dk-g"))}
          ${card("MEMORY", c.mem[0], c.mem[1], `<i class="dk-fill" style="--v:${c.mem[2]}"></i>`)}
          ${card("NETWORK", `↓ ${c.net[0]}`, `↑ ${c.net[1]}`)}
          ${card("DISK · PROCESSES", `r ${c.disk[0]}`, `w ${c.disk[1]}`)}
        </div>`
        }
        ${c.health ? `<div class="dk-card dk-card--wide"><u>HEALTH <b class="pn-pill pn-pill--${c.health[1]}">${c.health[0]}</b></u><span class="pn-cut">${c.health[2]}</span><span class="dk-${c.health[1]}">${c.health[1] === "ok" ? "✓" : c.health[1] === "err" ? "✗" : "…"} ${c.health[3]}</span></div>` : ""}
        <div class="dk-kv">
          <u>CONTAINER</u>
          <span>image</span><b>${c.image}</b><span>command</span><b class="pn-cut">${c.command}</b>
          <span>restarts</span><b>${c.restarts}</b><span>platform</span><b>linux</b>
        </div>
      </div>`;
  }
  paintRows();
  paintDetail();

  acts(el, {
    filter: (b) => {
      filter = b.dataset.k;
      paintRows();
    },
    fold: (b) => {
      const g = b.dataset.g;
      folded.has(g) ? folded.delete(g) : folded.add(g);
      paintRows();
    },
    pick: (b) => {
      sel = b.dataset.name;
      open = true;
      paintRows();
      paintDetail();
    },
    close: () => {
      open = false;
      paintRows();
      paintDetail();
    },
  });

  // Live: the processor share of the running containers moves.
  let timer = 0;
  el.start = () => {
    if (timer) return;
    timer = setInterval(() => {
      for (const c of C)
        if (c.cpu != null) live[c.name] = c.cpu * (1 + (Math.random() - 0.5) * 0.16);
      for (const e of el.querySelectorAll("[data-cpu]"))
        e.textContent = cpuText(C.find((c) => c.name === e.dataset.cpu));
      const bigCpu = el.querySelector("[data-cpu-big]"),
        c = C.find((x) => x.name === sel);
      if (bigCpu && c.cpu != null) bigCpu.textContent = cpuText(c);
    }, 1000);
  };
  el.stop = () => {
    clearInterval(timer);
    timer = 0;
  };
  return el;
}
