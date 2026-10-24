// {{name}}: a program that shows a view in the pane of the terminal it runs in, written with the
// SDK (@pencere/sdk). `deno task dev` runs it and runs it again when a file changes;
// `deno task build` makes one executable of it.

import { bundleFromDirectory, serve } from "@pencere/sdk";

const view = await bundleFromDirectory(new URL("../view/", import.meta.url));

await serve({
  views: [{ kind: "web", bundle: view }],
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
