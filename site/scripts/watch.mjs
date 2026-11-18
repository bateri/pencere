// While the site is watched (astro dev), a change to a file of the repository that a page shows is
// shown at once, as a change to one of the site's own files is. The builder watches the site's
// directory; the documents, the templates and the library the pages read are outside it, and
// without this a page went on showing what a file said when the watching began.
//
// What is watched is what the site's map says the pages read (src/docs.js): nothing is listed here.

import { execFileSync } from "node:child_process";
import { dirname, join, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { PAGES, HOME } from "../src/docs.js";

const site = join(dirname(fileURLToPath(import.meta.url)), "..");
const repo = join(site, "..");

// A file or a directory of the repository for each thing a page reads: a pattern is watched from
// the directory its first `*` is in.
const read = [
  ...new Set(
    [...HOME, ...PAGES.flatMap((page) => [page.source, ...(page.reads ?? [])])]
      .filter(Boolean)
      .map((path) =>
        path.includes("*") ? path.slice(0, path.indexOf("*")).replace(/\/[^/]*$/, "") : path,
      ),
  ),
].map((path) => join(repo, path));

// What the catalogue is made from (scripts/catalog.mjs): it is made again when one of them changes.
const catalogued = [
  join(repo, "kit/SPEC.md"),
  join(repo, "kit/sandbox/library/ui"),
  join(site, "demos"),
  join(site, "public/shots/shots.json"),
];

const within = (file, paths) => paths.some((path) => file === path || file.startsWith(path + sep));

/** A plugin of the site's builder, for the time the site is watched. */
export function repository() {
  return {
    name: "pencere:repository",
    apply: "serve",
    configureServer(server) {
      server.watcher.add(read);
      let timer;
      let again = false;
      const changed = (file) => {
        const catalogue = within(file, catalogued);
        if (!catalogue && !within(file, read)) return;
        again ||= catalogue;
        clearTimeout(timer);
        timer = setTimeout(() => {
          if (again) {
            try {
              execFileSync(process.execPath, [join(site, "scripts/catalog.mjs")], {
                stdio: "inherit",
              });
            } catch {
              // What does not agree was said by the catalogue; the pages stay as they were.
            }
            again = false;
          }
          server.moduleGraph.invalidateAll();
          server.ws.send({ type: "full-reload" });
        }, 60);
      };
      for (const event of ["change", "add", "unlink"]) server.watcher.on(event, changed);
    },
  };
}
