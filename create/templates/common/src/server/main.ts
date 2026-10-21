/**
 * Starts the server. In a terminal that shows pencere views the view opens in the
 * pane; in one that does not, the fallback runs; started by an MCP client over
 * stdio (Claude Desktop, VS Code, `pencere run`), it is an ordinary MCP server.
 */

import { bundleFromDirectory, serve } from "@pencere/sdk";
import type { Bundle } from "@pencere/sdk";

import { VIEW_DIRECTORY, createServer } from "./tool.js";

/** The view's files, which only a terminal can use: an MCP client that starts the server needs none of it. */
async function viewBundle(): Promise<Bundle> {
  try {
    return await bundleFromDirectory(VIEW_DIRECTORY);
  } catch (error) {
    console.error(
      `{{name}}: the view is not built (${error instanceof Error ? error.message : error}); run \`npm run build\` first.`,
    );
    return process.exit(1);
  }
}

const result = await serve(createServer(), {
  tool: "show_counter",
  ...(process.stdin.isTTY ? { bundle: await viewBundle() } : {}),
  // Runs once the terminal has been given back, when there is no view to show.
  fallback: () => {
    console.log("{{title}} is a view, and this terminal does not show pencere views.");
    console.log(
      "Run it in a terminal that does, or give an MCP Apps host `node dist/server/main.js`.",
    );
  },
});

if (result.kind === "closed") {
  console.log(`{{name}}: the view closed (${result.by})`);
}
