// The shared parts of the pen mockups: a small element builder, line icons, key caps.
// Taken from the bateri landing page (src/scripts/pens/kit.js); their looks are in src/styles/lab-pens.css.
export const h = (html) => {
  const t = document.createElement("template");
  t.innerHTML = html.trim();
  return t.content.firstElementChild;
};

export const esc = (s) =>
  String(s).replace(
    /[&<>"]/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c],
  );

const ic = (d, extra = "") =>
  `<svg class="pn-ic" viewBox="0 0 16 16" aria-hidden="true"${extra}>${d}</svg>`;
export const ICON = {
  search: ic(`<circle cx="7" cy="7" r="4.2"/><path d="M10.2 10.2 13.5 13.5"/>`),
  chevron: ic(`<path d="M4 6l4 4 4-4"/>`),
  right: ic(`<path d="M6 4l4 4-4 4"/>`),
  close: ic(`<path d="M4 4l8 8M12 4l-8 8"/>`),
  check: ic(`<path d="M3.5 8.4l3 3 6-6.6"/>`),
  db: ic(
    `<ellipse cx="8" cy="3.8" rx="5" ry="2"/><path d="M3 3.8v8.4c0 1.1 2.2 2 5 2s5-.9 5-2V3.8M3 8c0 1.1 2.2 2 5 2s5-.9 5-2"/>`,
  ),
  table: ic(
    `<rect x="2.5" y="3" width="11" height="10" rx="1.6"/><path d="M2.5 6.6h11M2.5 9.8h11M6.4 3v10"/>`,
  ),
  eye: ic(
    `<path d="M1.6 8s2.4-4.2 6.4-4.2S14.4 8 14.4 8s-2.4 4.2-6.4 4.2S1.6 8 1.6 8Z"/><circle cx="8" cy="8" r="1.7"/>`,
  ),
  play: ic(`<path d="M5 3.4v9.2L12.4 8z"/>`),
  layers: ic(`<path d="M8 2.4 2.2 5.4 8 8.4l5.8-3zM2.2 8 8 11l5.8-3M2.2 10.6 8 13.6l5.8-3"/>`),
  copy: ic(
    `<rect x="5.6" y="5.6" width="7.6" height="7.6" rx="1.5"/><path d="M10.4 5.6V4.3a1.5 1.5 0 0 0-1.5-1.5H4.3a1.5 1.5 0 0 0-1.5 1.5v4.6a1.5 1.5 0 0 0 1.5 1.5h1.3"/>`,
  ),
  download: ic(`<path d="M8 2.6v7.2M5 7l3 3 3-3M3 12.8h10"/>`),
  folder: ic(
    `<path d="M2 4.6a1.4 1.4 0 0 1 1.4-1.4h2.7l1.5 1.6h5a1.4 1.4 0 0 1 1.4 1.4v5.4a1.4 1.4 0 0 1-1.4 1.4H3.4A1.4 1.4 0 0 1 2 11.6Z"/>`,
  ),
  file: ic(`<path d="M4 2.6h5.2L12.4 6v7.4H4Z"/><path d="M9 2.6V6h3.4"/>`),
  files: ic(`<path d="M5.4 4.6h4.2l2.6 2.6v6H5.4Z"/><path d="M5.4 11.4H3.6V2.8h4.2l1 1"/>`),
  branch: ic(
    `<circle cx="4.6" cy="3.6" r="1.4"/><circle cx="4.6" cy="12.4" r="1.4"/><circle cx="11.4" cy="5.6" r="1.4"/><path d="M4.6 5v6M11.4 7c0 2.6-6.8 1.6-6.8 4"/>`,
  ),
  list: ic(`<path d="M6.4 4h7M6.4 8h7M6.4 12h7"/><path d="M2.6 4h1.2M2.6 8h1.2M2.6 12h1.2"/>`),
  send: ic(`<path d="M13.6 2.4 2.4 7l4.4 1.8L8.6 13.4Z"/><path d="M13.6 2.4 6.8 8.8"/>`),
  back: ic(`<path d="M6 4.2 2.8 7.4 6 10.6"/><path d="M2.8 7.4h6.4a3.2 3.2 0 0 1 3.2 3.2v1"/>`),
  info: ic(`<circle cx="8" cy="8" r="5.6"/><path d="M8 7.4v3.6M8 5.2v.2"/>`),
  help: ic(
    `<circle cx="8" cy="8" r="5.6"/><path d="M6.4 6.4a1.7 1.7 0 1 1 2.4 1.6c-.5.3-.8.6-.8 1.2M8 11.2v.2"/>`,
  ),
};

export const kbd = (...keys) => keys.map((k) => `<kbd>${k}</kbd>`).join("");

export const spark = (pts, w = 64, hh = 18, cls = "") => {
  const d = pts
    .map(
      (v, i) =>
        `${i ? "L" : "M"}${((i / (pts.length - 1)) * w).toFixed(1)} ${(hh - 1 - v * (hh - 3)).toFixed(1)}`,
    )
    .join(" ");
  return `<svg class="pn-spark ${cls}" viewBox="0 0 ${w} ${hh}" preserveAspectRatio="none" aria-hidden="true"><path d="${d} L${w} ${hh} L0 ${hh}Z" class="pn-spark-f"/><path d="${d}" class="pn-spark-l"/></svg>`;
};

export const acts = (el, table) =>
  el.addEventListener("click", (ev) => {
    const b = ev.target.closest("[data-act]");
    if (b && el.contains(b) && table[b.dataset.act]) table[b.dataset.act](b, ev);
  });
