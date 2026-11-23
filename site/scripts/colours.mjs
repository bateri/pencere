// Where the site's colours are written. A colour is a token of styles/shell.css (a pair for the
// light theme and the dark one, read by contrast.mjs), and every other place names it with var().
// Outside that file a literal colour (#hex, rgb(), hsl(), oklch(), ...) is a colour that does not
// change with the theme, and it is in one of three places only:
//   - the home page's first scene and what it draws, which are dark in both themes: components/Scene.astro
//     and styles/pens.css (the programs' own interfaces, drawn with real elements);
//   - the --term-* names of styles/home.css: the dark terminal that the home page's panes show;
//   - the lines named below, each with the reason it is not a colour of the page.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const site = join(dirname(fileURLToPath(import.meta.url)), "..");
const src = join(site, "src");

const WHOLE = new Set(["styles/shell.css", "styles/pens.css", "components/Scene.astro"]);
const ALLOWED = [
  [
    "styles/home.css",
    "0 24px 48px -16px #000;",
    "the shadow of a pane on the dark tile it lies on",
  ],
  ["components/Mark.astro", 'fill="#8eaee3"', "the sky in the mark: the same in both themes"],
  [
    "components/ThemeHead.astro",
    "const LIGHT =",
    "the browser's bar; contrast.mjs holds it to the ground",
  ],
  [
    "components/ThemeHead.astro",
    "const DARK =",
    "the browser's bar; contrast.mjs holds it to the ground",
  ],
];
const COLOUR =
  /(?<![\w&/-])#(?:[0-9a-fA-F]{8}|[0-9a-fA-F]{6}|[0-9a-fA-F]{3,4})(?![\w-])|\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\(/i;

const files = [];
const walk = (dir) => {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path);
    else if (/\.(css|astro)$/.test(name)) files.push(path);
  }
};
walk(src);

const wrong = [];
for (const path of files) {
  const file = relative(src, path).split("\\").join("/");
  if (WHOLE.has(file)) continue;
  // What is not a colour that is used: comments (/* */ and <!-- --> over lines, // to a line's end),
  // and a link's or an id's `#name`. Lines are kept, so that a finding says where it is.
  const blank = (c) => c.replace(/[^\n]/g, " ");
  const text = readFileSync(path, "utf8")
    .replace(/\/\*[\s\S]*?\*\/|<!--[\s\S]*?-->/g, blank)
    .replace(/(^|\s)\/\/.*$/gm, blank)
    .replace(/\b(?:href|id|for|aria-[a-z]+)="[^"]*"/g, blank);
  text.split("\n").forEach((line, index) => {
    if (!COLOUR.test(line)) return;
    if (file === "styles/home.css" && /^\s*--term-[\w-]+:/.test(line)) return;
    if (ALLOWED.some(([name, part]) => name === file && line.includes(part))) return;
    wrong.push(`${file}:${index + 1}: ${line.trim()}`);
  });
}

if (wrong.length > 0) {
  console.error(
    `A colour is written outside styles/shell.css (name it there, as a pair, and use var(); scripts/colours.mjs says where a literal may stay):\n${wrong.map((line) => `  ${line}`).join("\n")}`,
  );
  process.exit(1);
}
console.log(
  `colours: no literal colour outside the token file and the first scene (${files.length} files)`,
);
