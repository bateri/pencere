// WCAG contrast of every text/ground pair of the site's tokens, in both themes (styles/shell.css).
// Reads the pairs `--name: light-dark(light, dark)` from :root, so the numbers are the page's own.
// Body text must reach 4.5:1; labels, large type and a control's icon 3:1. Exits 1 on a failure.
import { readFileSync } from "node:fs";

const read = (name) => readFileSync(new URL(`../src/styles/${name}`, import.meta.url), "utf8");
const block = (css, open) => css.slice(css.indexOf(open), css.indexOf("}", css.indexOf(open)));
// The tokens: shell.css's :root, and home.css's body.home (the terminal panes of the home page, the
// same in both themes).
const root = [block(read("shell.css"), ":root {"), block(read("home.css"), "body.home {")].join(
  "\n",
);
const pairs = {};
for (const m of root.matchAll(
  /--([\w-]+):\s*light-dark\(\s*(#[0-9a-f]{3,8}|rgb\([^)]*\))\s*,\s*(#[0-9a-f]{3,8}|rgb\([^)]*\))\s*\)/g,
))
  pairs[m[1]] = { light: m[2], dark: m[3] };
const plain = {};
for (const m of root.matchAll(/--([\w-]+):\s*(#[0-9a-f]{3,8});/g)) plain[m[1]] = m[2];

const parse = (c) => {
  if (c.startsWith("#")) {
    let h = c.slice(1);
    if (h.length === 3) h = [...h].map((x) => x + x).join("");
    return { rgb: [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16)), a: 1 };
  }
  const n = c.match(/[\d.]+/g).map(Number);
  return { rgb: n.slice(0, 3), a: n[3] ?? 1 };
};
const over = (fg, bg) => fg.rgb.map((v, i) => v * fg.a + bg[i] * (1 - fg.a));
const lum = (rgb) => {
  const [r, g, b] = rgb.map((v) => {
    v /= 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
const ratio = (a, b) => {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
};
const pick = (name, theme) => parse(pairs[name]?.[theme] ?? plain[name]);

// The browser's bar (theme-color, components/ThemeHead.astro) is the page's ground.
const head = readFileSync(new URL("../src/components/ThemeHead.astro", import.meta.url), "utf8");
const metas = {
  light: head.match(/const LIGHT = "(#[0-9a-f]+)"/)?.[1],
  dark: head.match(/const DARK = "(#[0-9a-f]+)"/)?.[1],
};
let bad = 0;
for (const theme of ["light", "dark"]) {
  if (metas[theme] !== pairs.ground[theme]) {
    bad++;
    console.log(
      `FAIL theme-color of the ${theme} theme is ${metas[theme]}, the ground's is ${pairs.ground[theme]}`,
    );
  }
}

// [text token, ground token(s), minimum, what it is]
const checks = [
  ["ink", ["ground", "sheet", "wash"], 4.5, "headings, body"],
  ["soft", ["ground", "sheet", "wash"], 4.5, "lead text, secondary"],
  ["muted", ["ground", "sheet", "wash"], 4.5, "captions, links of the menu, kbd"],
  ["warm", ["ground", "sheet"], 4.5, "warm accent as text"],
  ["code-foreground", ["ground", "sheet", "wash"], 4.5, "code"],
  ["code-token-comment", ["ground", "sheet", "wash"], 4.5, "code comment"],
  ["code-token-string", ["ground", "sheet", "wash"], 4.5, "code string"],
  ["code-token-constant", ["ground", "sheet", "wash"], 4.5, "code constant"],
  ["code-token-keyword", ["ground", "sheet", "wash"], 4.5, "code keyword"],
  ["code-token-function", ["ground", "sheet", "wash"], 4.5, "code function"],
  [
    "code-token-punctuation",
    ["ground", "sheet", "wash"],
    3,
    "code punctuation (not read as words)",
  ],
  ["ground", ["button"], 4.5, "solid button words"],
  ["pane-ink", ["pane"], 4.5, "pane text"],
  ["pane-dim", ["pane"], 4.5, "pane bar"],
  ["pane-prompt", ["pane"], 4.5, "pane prompt"],
  ["ground", ["button-hover"], 4.5, "solid button words, hovered"],
  ["term-ink", ["term-ground"], 4.5, "home pane text"],
  ["term-title", ["term-ground"], 4.5, "home pane title"],
  ["term-dim", ["term-ground"], 4.5, "home pane tab"],
  ["term-faint", ["term-ground"], 4.5, "home pane host name"],
  ["term-accent", ["term-ground"], 4.5, "home pane prompt"],
];
for (const theme of ["light", "dark"]) {
  console.log(`\n${theme}`);
  for (const [fg, grounds, min, what] of checks) {
    for (const g of grounds) {
      const gp = pick(g === "button" ? "ink" : g, theme);
      // translucent grounds (wash) lie on the page's ground
      const base = over(gp, parse(pairs.ground[theme]).rgb);
      const text = parse(
        pairs[fg]?.[theme] ?? plain[fg] ?? pairs[fg === "ground" ? "ground" : fg][theme],
      );
      const r = ratio(over(text, base), base);
      const ok = r >= min;
      if (!ok) bad++;
      console.log(
        `${ok ? "ok  " : "FAIL"} ${r.toFixed(2).padStart(5)} (>= ${min})  ${fg} on ${g}  - ${what}`,
      );
    }
  }
}
console.log(bad ? `\n${bad} failure(s)` : "\nall pairs pass");
process.exit(bad ? 1 : 0);
