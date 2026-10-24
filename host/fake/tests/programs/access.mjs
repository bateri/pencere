// A program whose page asks to reach an origin and a browser capability, so the person is
// asked before the page starts, for the end-to-end tests of what the question says and of
// what the page is told it may use. The program and the view have names of their own, so
// the question can be seen to carry them.
//
//   node access.mjs
import { bundleFromFiles, serve } from "@pencere/sdk";

const result = await serve({
  views: [
    {
      kind: "web",
      bundle: bundleFromFiles([
        { path: "index.html", data: Buffer.from("<!doctype html><p>view\n") },
      ]),
      access: { connect: ["https://api.example.com"], permissions: ["clipboardWrite"] },
    },
  ],
  program: "Domain Desk",
  title: "Show it",
  calls: { show: () => ({ shown: true }) },
});
console.log(`serve: ${JSON.stringify(result)}`);
