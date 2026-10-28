// {{name}}: a program that shows a page of the kit in the pane of the terminal it runs in, written
// with the SDK (@pencere/sdk). The page is Lua (src/view/page.lua), which the terminal draws in its
// own font and theme. `deno task dev` runs it and runs it again when a file changes;
// `deno task build` makes one executable of it; `pencere test` runs the page's tests.

import { bundleFromDirectory, serve } from "@pencere/sdk";

const view = await bundleFromDirectory(new URL("../view/", import.meta.url), { entry: "page.lua" });

await serve({
  views: [{ kind: "lua", bundle: view }],
  program: "{{name}}",
  title: "{{name}}",
  calls: {
    hello: (params) => {
      const name = (params as { name?: string } | undefined)?.name;
      return { message: `Hello, ${name ?? "world"}!` };
    },
  },
  topics: {
    clock: (_params, subscription) => {
      const send = () => subscription.push({ now: Date.now() / 1000 });
      send();
      const timer = setInterval(send, 1000);
      // Runs when the view stops listening.
      return () => clearInterval(timer);
    },
  },
  // Runs once the terminal is given back, when there is no view to show.
  fallback: () => console.log("This terminal shows no views, so here is the text: Hello, world!"),
});
