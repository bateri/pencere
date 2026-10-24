/**
 * Starts the containers program. Run it in a terminal that hosts pencere views and the
 * view opens in the pane; run it anywhere else and it prints the containers and exits.
 */

import { readFileSync } from "node:fs";

import { bundleFromDirectory, serve } from "@pencere/sdk";

import { ICON, createProgram, sampleContainers, table } from "./containers.js";

const containers = sampleContainers();

// The page imports its bridge from `@pencere/sdk/page` (see view/index.html). A page has no
// node_modules, so the module goes in the bundle beside the page, as pencere-page.js.
// (`readFileSync`, not the promise form: in a program compiled by `deno compile` only the former
// reads the files compiled into the binary.)
// (`import.meta.resolve` answers with a URL; Bun 1.0 answers with a promise of a path.)
const resolved: string = await import.meta.resolve("@pencere/sdk/page");
const bridge = readFileSync(resolved.startsWith("file:") ? new URL(resolved) : resolved);
const view = await bundleFromDirectory(new URL("../view/", import.meta.url), {
  files: [{ path: "pencere-page.js", data: bridge }],
});

const result = await serve({
  views: [{ kind: "web", bundle: view }],
  // What the terminal writes above the view, and the icon it draws beside it.
  program: "containers",
  title: "Containers",
  icons: [ICON],
  ...createProgram(containers),
  // Runs once the terminal has been given back, when there is no view to show.
  fallback: () => {
    console.log(table(containers));
  },
});

if (result.kind === "closed") {
  console.log(`containers: the view closed (${result.by})`);
}
