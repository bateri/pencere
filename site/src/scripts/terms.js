// What a terminal shows for the same two jobs, set as text: the screen of a process viewer in htop's
// layout, and what git prints for a changed file. They are drawn here, not captured: a capture would
// show the machine it was taken on. The processes are the toppen mockup's and the file, with its last
// commit, is the textpen mockup's, so both halves of the scene show one machine and one change.
import { PROCS } from "./pens/toppen.js";
import { FILES, sides } from "./pens/textpen.js";

const esc = (s) =>
  String(s).replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]);
const el = (html) => {
  const t = document.createElement("template");
  t.innerHTML = html;
  return t.content.firstElementChild;
};

// Quiet processes under the busy ones, so the list is as long as the screen.
const QUIET = [
  ["logd", 88, "root", 0.0, 34, 5, "sleeping", 61.2, "/usr/libexec/logd"],
  ["configd", 91, "root", 0.0, 12, 7, "sleeping", 20.44, "/usr/libexec/configd"],
  ["opendirectoryd", 104, "root", 0.0, 18, 6, "sleeping", 33.9, "/usr/libexec/opendirectoryd"],
  ["syslogd", 86, "root", 0.0, 4.1, 3, "sleeping", 8.02, "/usr/sbin/syslogd"],
  ["cfprefsd", 388, "you", 0.0, 9.4, 4, "sleeping", 14.71, "/usr/sbin/cfprefsd agent"],
  ["distnoted", 391, "you", 0.0, 6.0, 2, "sleeping", 5.18, "/usr/sbin/distnoted agent"],
  ["trustd", 402, "you", 0.0, 16, 3, "sleeping", 26.33, "/usr/libexec/trustd --agent"],
  ["tmux", 2790, "you", 0.0, 8.3, 1, "sleeping", 3.94, "tmux new -s work"],
  ["ssh-agent", 655, "you", 0.0, 3.2, 1, "sleeping", 0.22, "/usr/bin/ssh-agent -l"],
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

const CORES = 8;
const BAR = 40;
const clock = (s) => {
  const c = Math.round(s * 100);
  return `${Math.floor(c / 6000)}:${String(Math.floor((c % 6000) / 100)).padStart(2, "0")}.${String(c % 100).padStart(2, "0")}`;
};
const res = (m) => (m >= 1024 ? `${(m / 1024).toFixed(1)}G` : `${Math.round(m)}M`);
const meter = (label, share, text, tone = "g") => {
  const on = Math.max(1, Math.round(share * (BAR - text.length)));
  const off = BAR - text.length - on;
  return `<span class="tm-c">${label}</span><b>[</b><span class="tm-${tone}">${"|".repeat(on)}</span>${" ".repeat(Math.max(0, off))}<span class="tm-q">${text}</span><b>]</b>`;
};

export function htop() {
  const procs = [...PROCS, ...QUIET].map((p) => ({ ...p }));
  const cores = Array.from({ length: CORES }, (_, i) => 0.08 + ((i * 37) % 11) / 30);
  const root = el(`<pre class="tm tm--htop"></pre>`);
  const paintAll = () => {
    procs.sort((a, b) => b.cpu - a.cpu || a.pid - b.pid);
    const half = CORES / 2;
    const top = [];
    for (let i = 0; i < half; i++) {
      top.push(
        `  ${meter(String(i).padStart(3), cores[i], `${(cores[i] * 100).toFixed(1)}%`)}  ${meter(String(i + half).padStart(3), cores[i + half], `${(cores[i + half] * 100).toFixed(1)}%`)}`,
      );
    }
    const running = procs.filter((p) => p.state === "running" && p.cpu >= 1).length;
    top.push(
      `  ${meter("Mem", 0.64, "10.2G/16.0G")}  <span class="tm-c">Tasks:</span> <b>54</b>, <b>412</b> thr; <b>${running}</b> running`,
    );
    top.push(
      `  ${meter("Swp", 0.2, "820M/4.00G", "r")}  <span class="tm-c">Load average:</span> <b>2.43</b> 2.10 <span class="tm-q">1.84</span>`,
    );
    top.push(`  ${" ".repeat(BAR + 5)}  <span class="tm-c">Uptime:</span> <b>3 days, 00:14:07</b>`);
    const head = `<span class="tm-head">${"    PID USER           PRI  NI  VIRT   RES S  CPU%  MEM%     TIME+  Command".padEnd(400)}</span>`;
    const rows = procs.map((p, i) => {
      const line =
        `${String(p.pid).padStart(7)} ${p.user.padEnd(14).slice(0, 14)} ${"20".padStart(3)} ${"0".padStart(3)} ${`${408 + (p.pid % 5)}G`.padStart(5)} ${res(p.mem).padStart(5)} ` +
        `${p.state === "running" ? "R" : "S"} ${p.cpu.toFixed(1).padStart(5)} ${((p.mem / 16384) * 100).toFixed(1).padStart(5)} ${clock(p.time).padStart(9)}  `;
      return i === 0
        ? `<span class="tm-sel">${esc(line + p.cmd).padEnd(400)}</span>`
        : `${esc(line)}<span class="${p.state === "running" ? "" : "tm-q"}">${esc(p.cmd)}</span>`;
    });
    const keys = [
      "Help",
      "Setup",
      "Search",
      "Filter",
      "Tree",
      "SortBy",
      "Nice -",
      "Nice +",
      "Kill",
      "Quit",
    ]
      .map((k, i) => `<b>F${i + 1}</b><span class="tm-key">${k.padEnd(6)}</span>`)
      .join("");
    root.innerHTML = `${top.join("\n")}\n${head}\n${rows.join("\n")}<span class="tm-foot">${keys}<span class="tm-key">${" ".repeat(300)}</span></span>`;
  };
  paintAll();
  const tick = () => {
    for (let i = 0; i < CORES; i++)
      cores[i] = Math.min(
        0.96,
        Math.max(0.02, cores[i] + (0.28 - cores[i]) * 0.25 + (Math.random() - 0.5) * 0.22),
      );
    for (const p of procs) {
      if (p.base >= 0.2) p.cpu = Math.max(0, p.base * (1 + (Math.random() - 0.5) * 0.14));
      if (p.state === "running") p.time += p.cpu / 100;
    }
    paintAll();
  };
  let timer = 0;
  root.start = () => {
    if (!timer) timer = setInterval(tick, 1000);
  };
  root.stop = () => {
    clearInterval(timer);
    timer = 0;
  };
  return root;
}

// `git status --short` and `git diff` for the file the textpen mockup compares, from the same data:
// the lines git would print, coloured as git colours them, between a shell's prompts. (The scene
// that shows them takes the colour out again: there they are in the shade.)
export function gitdiff() {
  const path = "src/server.ts";
  const file = FILES[path];
  const lines = (text) => text.replace(/\n$/, "").split("\n");
  const was = lines(file.was),
    now = lines(file.text);
  const rows = sides(file).map(([l, r]) => ({
    l,
    r,
    a: was[l - 1],
    b: now[r - 1],
    same: !!l && !!r && was[l - 1] === now[r - 1],
  }));
  // One hunk: from three lines before the first change to three after the last, as git joins them.
  const first = rows.findIndex((x) => !x.same),
    last = rows.findLastIndex((x) => !x.same);
  const hunk = rows.slice(Math.max(0, first - 3), last + 4);
  const body = [];
  let gone = [],
    come = [];
  const put = () => {
    body.push(
      ...gone.map((t) => `<span class="tm-r">-${esc(t)}</span>`),
      ...come.map((t) => `<span class="tm-g">+${esc(t)}</span>`),
    );
    gone = [];
    come = [];
  };
  for (const x of hunk) {
    if (x.same) {
      put();
      body.push(` ${esc(x.a)}`);
    } else {
      if (x.l) gone.push(x.a);
      if (x.r) come.push(x.b);
    }
  }
  put();
  const olds = hunk.filter((x) => x.l),
    news = hunk.filter((x) => x.r);
  const changed = Object.entries(FILES)
    .filter(([, f]) => f.git)
    .sort(([a, f], [b, g]) => (f.git === "U") - (g.git === "U") || a.localeCompare(b));
  const prompt = (cmd) =>
    `<span class="tm-c">~/shop</span> <span class="tm-q">main</span> $ ${cmd}`;
  return el(
    `<pre class="tm tm--git">${[
      prompt("git status --short"),
      ...changed.map(
        ([name, f]) => `<span class="tm-r">${f.git === "U" ? "??" : " M"}</span> ${esc(name)}`,
      ),
      prompt(`git diff ${path}`),
      `<b>diff --git a/${path} b/${path}</b>`,
      `<b>index 8c1f2a9..b47d0e3 100644</b>`,
      `<b>--- a/${path}</b>`,
      `<b>+++ b/${path}</b>`,
      `<span class="tm-c">@@ -${olds[0].l},${olds.length} +${news[0].r},${news.length} @@</span>`,
      ...body,
      prompt(`<span class="tm-cur"> </span>`),
    ].join("\n")}</pre>`,
  );
}
