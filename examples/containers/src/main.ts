/**
 * Starts the containers tool. Run it from a terminal that hosts pencere views
 * and the view opens in the pane; from a terminal that does not, it prints the
 * containers; started by an MCP client, it is an ordinary stdio server.
 */

import { bundleFromDirectory, serve } from "@pencere/sdk";

import { VIEW_DIRECTORY, createServer, sampleContainers, table } from "./tool.js";

const containers = sampleContainers();

const result = await serve(createServer(containers), {
  tool: "list_containers",
  // The bundle reads and hashes every file of the view. Only a terminal can use
  // it: started by an MCP client, the server needs none of that.
  ...(process.stdin.isTTY ? { bundle: await bundleFromDirectory(VIEW_DIRECTORY) } : {}),
  // Runs once the terminal has been given back, when there is no view to show.
  fallback: () => {
    console.log(table(containers));
  },
});

if (result.kind === "closed") {
  console.log(`containers: the view closed (${result.by})`);
}
