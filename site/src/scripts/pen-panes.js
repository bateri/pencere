// The panes on the home page that show a program made with pencere. What is in one is the program's
// own interface drawn with real elements (src/scripts/pens), not a picture of it. A pane is
// still and takes no presses: nothing in it runs, and the card or the figure around it is what is
// read. An element asks for one with data-pen (the program's name) and data-show:
//
//   corner  on a program's card. The pane is 800 wide and near its real size, with a heading (the
//           program's mark, its name, what it calls its view). Its top left corner is seen; it
//           runs off the card's right and bottom. The way the bateri landing page lists its pens.
//   whole   in the figure of a pane's three moments. The pane has no heading, since the figure
//           draws the terminal's tab over it, and is brought whole to the size of its place.
import { h } from "./pens/kit.js";
import { toppen } from "./pens/toppen.js";
import { dockerpen } from "./pens/dockerpen.js";
import { sqlpen } from "./pens/sqlpen.js";
import { textpen } from "./pens/textpen.js";
import { pickpen } from "./pens/pickpen.js";
import { linearpen } from "./pens/linearpen.js";

const MAKE = { toppen, dockerpen, sqlpen, textpen, pickpen, linearpen };

// Each program's mark in a 12-unit box, in the colour of the picture it sends for a dark ground
// (each pen's tools/icon.mjs). sqlpen sends none: it has the mark of a database.
const line = (d, colour, width = 1.35, opacity = 1) =>
  `<path d="${d}" fill="none" stroke="${colour}" stroke-width="${width}" stroke-opacity="${opacity}" stroke-linecap="round" stroke-linejoin="round"/>`;
const MARK = {
  toppen: [
    [0.54, 6.78, 1.92, 4.2, 0.4],
    [3.54, 3.48, 1.92, 7.5, 0.7],
    [6.54, 7.98, 1.92, 3, 0.4],
    [9.54, 1.38, 1.92, 9.6, 1],
  ]
    .map(
      ([x, y, w, t, a]) =>
        `<rect x="${x}" y="${y}" width="${w}" height="${t}" rx="0.96" fill="#6ed0a8" fill-opacity="${a}"/>`,
    )
    .join(""),
  pickpen: [
    "M3.5 1.75h5",
    "M2.25 4h7.5a1.25 1.25 0 0 1 1.25 1.25v1.5a1.25 1.25 0 0 1-1.25 1.25h-7.5a1.25 1.25 0 0 1-1.25-1.25v-1.5a1.25 1.25 0 0 1 1.25-1.25z",
    "M3.5 6h5",
    "M3.5 10.25h5",
  ]
    .map((d) => line(d, "#5cc8a4", 1))
    .join(""),
  dockerpen:
    line(
      "M4.2 1.6C7.4 1.9 9.8 3.5 9.8 5.2C9.8 6.2 9.27 6.75 8.6 7.18A4.8 4.8 0 0 1 4.2 7.6Z",
      "#6ca8f5",
    ) + line("M1.6 9.1A7.4 7.4 0 0 0 10.4 9.1", "#6ca8f5", 1.35, 0.65),
  textpen:
    line("M3.7 3.5C2.7 3.9 1.8 4.7 1.1 6c.7 1.3 1.6 2.1 2.6 2.5", "#f0b25a", 1.35, 0.65) +
    line("M8.3 3.5C9.3 3.9 10.2 4.7 10.9 6c-.7 1.3-1.6 2.1-2.6 2.5", "#f0b25a", 1.35, 0.65) +
    line("M4.9 1.9h2.2M6 1.9v8.2M4.9 10.1h2.2", "#f0b25a"),
  linearpen:
    line("M6 1.7A4.3 4.3 0 1 0 6 10.3A4.3 4.3 0 1 0 6 1.7Z", "#c586c0", 1.35, 0.65) +
    line("M4 6.2l1.4 1.4 2.7-3", "#c586c0"),
  sqlpen: line(
    "M2.5 3c0-1.3 7-1.3 7 0v6c0 1.3-7 1.3-7 0ZM2.5 3c0 1.3 7 1.3 7 0M2.5 6c0 1.3 7 1.3 7 0",
    "#7a9cc6",
  ),
};

// A pane is laid out 800 wide, a pen's middle layout. On a card 470 of that is seen, after a margin
// at the left and the top that goes with the card's width.
const PANE = 800,
  SEEN = 470,
  LEFT = 0.075,
  TOP = 0.085,
  HEAD = 34;

const fit = new ResizeObserver((entries) => {
  for (const { target: place } of entries) {
    const pane = place.querySelector(".pen-pane");
    const w = place.clientWidth;
    if (!w) continue;
    if (place.dataset.show === "whole") {
      // In a narrow place the pane is laid out narrower, so that a pen takes its narrow layout
      // and its text is not brought down past reading.
      const wide = Math.min(PANE, Math.max(440, Math.round(w * 1.25)));
      const s = w / wide;
      Object.assign(pane.style, {
        left: "0",
        top: "0",
        width: `${wide}px`,
        height: `${Math.ceil(place.clientHeight / s)}px`,
        transform: `scale(${s.toFixed(4)})`,
      });
    } else {
      const left = Math.round(w * LEFT),
        top = Math.round(w * TOP);
      const s = (w - left) / SEEN;
      Object.assign(pane.style, {
        left: `${left}px`,
        top: `${top}px`,
        width: `${PANE}px`,
        height: `${Math.ceil((place.clientHeight - top) / s) + 40}px`,
        transform: `scale(${s.toFixed(4)})`,
      });
    }
  }
});

for (const place of document.querySelectorAll("[data-pen]")) {
  const name = place.dataset.pen;
  if (!MAKE[name]) continue;
  const whole = place.dataset.show === "whole";
  const pane = h(`<div class="pen-pane"></div>`);
  pane.inert = true;
  if (!whole) {
    pane.append(
      h(
        `<div class="pen-pane__head"><svg width="14" height="14" viewBox="0 0 12 12" aria-hidden="true">${MARK[name]}</svg>${place.dataset.place ? `<em>${place.dataset.place}</em>` : ""}<b>${name}</b><span>${place.dataset.title || ""}</span></div>`,
      ),
    );
  }
  const pen = MAKE[name]({ close() {} });
  const under = whole ? 0 : HEAD;
  pen.style.cssText = `left:0;top:${under}px;width:100%;height:calc(100% - ${under}px)`;
  pane.append(pen);
  place.append(pane);
  fit.observe(place);
}
