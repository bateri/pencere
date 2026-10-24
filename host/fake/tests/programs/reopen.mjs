// A program that closes its view and shows another, then shows it again as it was, then
// closes that one and goes on: what `ViewControl` does, for the end-to-end tests against
// the stand-in host. The page calls `show` once in every view it is in, and the program
// answers by changing the view that runs: the first call replaces it with a page whose
// document changed, the second with the same files, and the third closes it.
//
//   node reopen.mjs
import { ViewControl, bundleFromFiles, serve } from "@pencere/sdk";

const page = (text) =>
  bundleFromFiles([
    {
      path: "index.html",
      data: Buffer.from(`<!doctype html><link rel=stylesheet href=style.css><p>${text}\n`),
    },
    { path: "style.css", data: Buffer.from("p { margin: 0 }\n") },
  ]);

const control = new ViewControl();
const outcomes = [];
let shown = 0;

const result = await serve({
  views: [{ kind: "web", bundle: page("one") }],
  program: "reopen",
  calls: {
    show: () => {
      shown += 1;
      if (shown === 1)
        outcomes.push(control.reopen({ views: [{ kind: "web", bundle: page("two") }] }));
      else if (shown === 2) outcomes.push(control.reopen());
      else outcomes.push(control.close());
      return { shown };
    },
  },
  control,
});
for (const outcome of await Promise.all(outcomes)) {
  console.log(`control: ${JSON.stringify(outcome)}`);
}
console.log(`serve: ${JSON.stringify(result)}`);
