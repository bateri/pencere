/**
 * Starts the containers program. Run it in a terminal that hosts pencere views and the
 * view opens in the pane; run it anywhere else and it prints the containers and exits.
 */

import { bundleFromDirectory, serve } from "@pencere/sdk";

import { ICON, createProgram, sampleContainers, table } from "./containers.js";

const containers = sampleContainers();

const result = await serve({
  views: [{ kind: "web", bundle: await bundleFromDirectory(new URL("../view/", import.meta.url)) }],
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
