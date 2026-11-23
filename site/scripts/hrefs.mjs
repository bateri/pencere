// After the site is built: no link of a page goes to a file by the path it has in the repository. A
// document links to its neighbours by relative paths (`../SPEC.md`, `spec/page.md`), and the site
// rewrites each one to the page that shows the file or to the file on the web (src/components/
// Numbered.astro); a link that is left as the document wrote it is a 404 on the site. Run on the built
// pages by the build (package.json), and by the tests.
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

/** The addresses of a page's links that are a path of the repository: a relative one that ends in `.md` or climbs out with `../`. */
export function strays(html) {
  const markup = html
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<(script|style)\b[\s\S]*?<\/\1>/g, "");
  const found = [];
  for (const tag of markup.matchAll(/<a\b[^>]*>/g)) {
    const link = tag[0].match(/\shref=(?:"([^"]*)"|'([^']*)'|([^\s>"']+))/);
    const href = link && (link[1] ?? link[2] ?? link[3]);
    if (href === undefined || /^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith("//")) continue;
    const path = href.split(/[#?]/)[0];
    if (path === ".." || path.startsWith("../") || /\.md$/i.test(path)) found.push(href);
  }
  return found;
}

const pages = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory()
      ? entry.name === "pagefind"
        ? []
        : pages(join(dir, entry.name))
      : entry.name.endsWith(".html")
        ? [join(dir, entry.name)]
        : [],
  );

// The built site, from the command line.
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const dist = join(dirname(fileURLToPath(import.meta.url)), "..", "dist");
  const wrong = pages(dist).flatMap((file) =>
    strays(readFileSync(file, "utf8")).map(
      (href) => `${relative(dist, file)}: a link goes to "${href}"`,
    ),
  );
  if (wrong.length > 0) {
    console.error(
      `A built page links to a path of the repository, and it is a 404 on the site:\n${wrong.map((line) => `  ${line}`).join("\n")}`,
    );
    process.exit(1);
  }
  console.log("hrefs: no page links to a path of the repository");
}
