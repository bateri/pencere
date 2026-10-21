// A tool that closes its view and shows another, then closes that one and goes
// on: what `ViewControl` does, for the end-to-end test against the stand-in
// host. The first view is replaced when its tool is called (the view runs), the
// second is closed the same way.
//
//   node reopen.mjs
import {
  RESOURCE_MIME_TYPE,
  registerAppResource,
  registerAppTool,
} from "@modelcontextprotocol/ext-apps/server";
import { McpServer } from "@modelcontextprotocol/server";
import { ViewControl, bundleFromFiles, serve } from "@pencere/sdk";

const page = (text) =>
  bundleFromFiles([{ path: "index.html", data: Buffer.from(`<!doctype html><p>${text}\n`) }]);

const control = new ViewControl();
const server = new McpServer({ name: "reopen", version: "0.0.0" });
const outcomes = {};
let calls = 0;

registerAppTool(
  server,
  "show",
  { description: "Show", _meta: { ui: { resourceUri: "ui://reopen/view.html" } } },
  async () => {
    calls += 1;
    if (calls === 1) outcomes.reopen = control.reopen({ bundle: page("two") });
    else if (calls === 2) outcomes.close = control.close();
    return { content: [{ type: "text", text: `call ${calls}` }] };
  },
);
registerAppResource(server, "View", "ui://reopen/view.html", {}, async (uri) => ({
  contents: [{ uri: uri.href, mimeType: RESOURCE_MIME_TYPE, text: "<!doctype html><p>view" }],
}));

const result = await serve(server, { tool: "show", bundle: page("one"), control });
console.log(`reopen: ${JSON.stringify(await outcomes.reopen)}`);
console.log(`close: ${JSON.stringify(await outcomes.close)}`);
console.log(`serve: ${JSON.stringify(result)}`);
