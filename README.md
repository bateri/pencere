# pencere

A program running in a terminal shows its own view in its pane. On your machine, or on a server
over plain ssh with no GPU, the program takes its pane the way a full-screen program like vim
takes the screen, and gives it back when it ends — but what it puts there is a real interface, not
a grid of characters.

The program does not draw the view. It hands the terminal the files of the view, the terminal runs
them, and while the view is up the two talk to each other in six small messages: the view calls the
program and subscribes to its topics, and the program answers and pushes. That is the whole
protocol, and it is the same for every kind of view.

There are two families of views:

- **views the terminal draws itself**, with the pencere kit, in its own font and theme: a
  WebAssembly component (`wasm`) or a Lua script (`lua`);
- **a web page** (`web`): HTML and JavaScript, built with whatever builds to the web, in the
  terminal's web view. It takes the terminal's colours and font from a message the terminal sends
  it.

The messages ride in the terminal's own stream: no port, no extra connection, nothing to set up on
the server. The view opens in the program's own pane and closes when the program ends. It leaves
the terminal's own shortcuts alone, loads its files once and keeps them by content, and a program
in a terminal that does not support pencere is told so and can fall back to text.

The view runs on the machine of the person looking at it, so it is kept in a sandbox. A component
or a script gets nothing it was not handed; a web page reaches nothing but its own program unless
the program names a domain and the person allows it.

```ts
import { bundleFromDirectory, serve } from "@pencere/sdk";

const result = await serve({
  views: [{ kind: "web", bundle: await bundleFromDirectory(new URL("./view/", import.meta.url)) }],
  program: "containers",
  title: "Containers",
  calls: { list: () => ({ containers: listContainers() }) },
  topics: {
    stats: (_params, subscription) => {
      const timer = setInterval(() => subscription.push(readStats()), 1000);
      return () => clearInterval(timer);
    },
  },
  fallback: () => console.log(table()),
});
```

## Status

Early: nothing here is stable yet, and the shared release comes later. The repository holds

- the specification and its fixtures, the one truth both libraries are tested from
  ([`spec/draft`](spec/draft/pencere.md));
- a TypeScript SDK for programs that show a view ([`@pencere/sdk`](sdk/typescript)), and the same
  for Rust (crate `pencere`, [`program`](program));
- a Rust library for terminals that host views, doing no I/O (crate `pencere-host`,
  [`host/rust`](host/rust)), and the part of the wire it shares with programs (crate
  `pencere-wire`, [`wire`](wire));
- the kit a terminal draws native views with: the page engine, the sandbox that runs a component
  or a Lua script, the interpreter and the SDK a page is written with ([`kit`](kit));
- a stand-in terminal that plays the host, for trying a program and for tests
  ([`host/fake`](host/fake));
- an example program with a web view to try it all with ([`examples/containers`](examples/containers)),
  and the same list as a native view of the terminal, as a WebAssembly component or a Lua script
  ([`examples/containers-rust`](examples/containers-rust), written with the program library).

[bateri](https://github.com/bateri/bateri) will be the first host. The `@pencere/sdk` on npm is an
earlier draft of this protocol; the one in this repository replaces it and is not released yet,
and neither is the crate. How a package is released is in [RELEASING.md](RELEASING.md).

## License

Apache-2.0 — see [LICENSE](LICENSE). The specification, the libraries and the tools are
all under it, so any terminal or program, open or closed, can implement the protocol or embed
this code. Copyright belongs to the pencere Authors ([NOTICE](NOTICE), [AUTHORS](AUTHORS)):
everyone who contributes keeps the copyright of their contribution.
