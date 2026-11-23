// WCAG contrast of every text/ground pair of the site's tokens, in both themes (styles/shell.css).
// Reads the pairs `--name: light-dark(light, dark)`, so the numbers are the page's own, and holds the
// plain colours that a browser without light-dark() shows to the pairs' dark halves.
// Body text must reach 4.5:1; labels, large type and a control's icon 3:1. Exits 1 on a failure.
import { readFileSync } from "node:fs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const shell = read("../src/styles/shell.css");
const home = read("../src/styles/home.css");
// The text of a rule's braces, from where `open` starts.
const block = (css, open) => {
  const from = css.indexOf(open);
  if (from < 0) throw new Error(`contrast: no "${open}" in the styles`);
  return css.slice(from, css.indexOf("\n}", from));
};

// A colour: #rgb, #rgba, #rrggbb, #rrggbbaa (either case), rgb() and rgba() with commas or spaces
// and an alpha as a number or a percentage. Anything else is an error, not a guess.
const parse = (text) => {
  const c = text.trim();
  const hex = c.match(/^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i)?.[1];
  if (hex) {
    const h = hex.length <= 4 ? [...hex].map((x) => x + x).join("") : hex;
    const n = [0, 2, 4, 6].map((i) => (h.length > i ? parseInt(h.slice(i, i + 2), 16) : 255));
    return { rgb: n.slice(0, 3), a: n[3] / 255 };
  }
  const fn = c.match(/^rgba?\(\s*([^)]*?)\s*\)$/i)?.[1];
  if (fn) {
    const parts = fn.split(/\s*[,/]\s*|\s+/).filter(Boolean);
    if (parts.length === 3 || parts.length === 4) {
      const rgb = parts.slice(0, 3).map(Number);
      const alpha =
        parts[3] === undefined
          ? 1
          : parts[3].endsWith("%")
            ? parseFloat(parts[3]) / 100
            : Number(parts[3]);
      if (rgb.every((v) => Number.isFinite(v)) && Number.isFinite(alpha)) return { rgb, a: alpha };
    }
  }
  throw new Error(`contrast: cannot read the colour "${text}"`);
};

// The arguments of light-dark(a, b): split at the comma that is not inside a bracket.
const split = (args) => {
  let depth = 0;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === "(") depth++;
    else if (args[i] === ")") depth--;
    else if (args[i] === "," && depth === 0) return [args.slice(0, i), args.slice(i + 1)];
  }
  throw new Error(`contrast: light-dark(${args}) has no two halves`);
};

const pairs = {};
for (const m of shell.matchAll(/--([\w-]+):\s*light-dark\(((?:[^()]|\([^()]*\))*)\);/g)) {
  const [light, dark] = split(m[2]);
  pairs[m[1]] = { light: light.trim(), dark: dark.trim() };
}
// The plain colours: shell.css's first :root (a browser that has no light-dark() reads these), and
// home.css's body.home (the terminal panes of the home page, the same in both themes).
const plain = {};
for (const text of [block(shell, ":root {"), block(home, "body.home {")])
  for (const m of text.matchAll(/--([\w-]+):\s*((?:#[0-9a-fA-F]+|rgba?\([^)]*\)));/g))
    plain[m[1]] = m[2];

const token = (name, theme) => {
  const value = pairs[name]?.[theme] ?? plain[name];
  if (!value)
    throw new Error(
      `contrast: --${name} is no colour token of styles/shell.css or of home.css's body.home`,
    );
  return parse(value);
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
const same = (a, b) =>
  a.rgb.every((v, i) => Math.abs(v - b.rgb[i]) < 0.5) && Math.abs(a.a - b.a) < 0.005;

let bad = 0;
const fail = (line) => {
  bad++;
  console.log(`FAIL ${line}`);
};

try {
  // The browser's bar (theme-color, components/ThemeHead.astro) is the page's ground.
  const head = read("../src/components/ThemeHead.astro");
  for (const [theme, name] of [
    ["light", "LIGHT"],
    ["dark", "DARK"],
  ]) {
    const value = head.match(new RegExp(`const ${name} = "([^"]+)"`))?.[1];
    if (!value) fail(`ThemeHead.astro has no const ${name}`);
    else if (!same(parse(value), token("ground", theme)))
      fail(`theme-color of the ${theme} theme is ${value}, the ground's is ${pairs.ground[theme]}`);
  }
  // What a browser without light-dark() shows is the dark theme.
  const first = block(shell, ":root {");
  for (const [name, pair] of Object.entries(pairs)) {
    const value = first.match(new RegExp(`--${name}:\\s*([^;]+);`))?.[1];
    if (!value) fail(`--${name} has a pair and no plain dark colour before it`);
    else if (!same(parse(value), parse(pair.dark)))
      fail(`--${name} is ${value} where light-dark() is not read, and ${pair.dark} where it is`);
  }

  // [text token, ground tokens, minimum, what it is]; a translucent ground lies on the page's.
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
    [
      "ground",
      ["ink", "button-hover"],
      4.5,
      "solid button words (the ink is its ground), and hovered",
    ],
    ["pane-ink", ["pane"], 4.5, "pane text"],
    ["pane-dim", ["pane"], 4.5, "pane bar"],
    ["pane-prompt", ["pane"], 4.5, "pane prompt"],
    ["term-ink", ["term-ground"], 4.5, "home pane text"],
    ["term-title", ["term-ground"], 4.5, "home pane title"],
    ["term-dim", ["term-ground"], 4.5, "home pane tab"],
    ["term-faint", ["term-ground"], 4.5, "home pane host name"],
    ["term-accent", ["term-ground"], 4.5, "home pane prompt"],
  ];
  for (const theme of ["light", "dark"]) {
    console.log(`\n${theme}`);
    const page = token("ground", theme).rgb;
    for (const [fg, grounds, min, what] of checks) {
      for (const g of grounds) {
        const base = over(token(g, theme), page);
        const r = ratio(over(token(fg, theme), base), base);
        const ok = r >= min;
        if (!ok) bad++;
        console.log(
          `${ok ? "ok  " : "FAIL"} ${r.toFixed(2).padStart(5)} (>= ${min})  ${fg} on ${g}  - ${what}`,
        );
      }
    }
  }
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
console.log(bad ? `\n${bad} failure(s)` : "\nall pairs pass");
process.exit(bad ? 1 : 0);
