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
  WebAssembly component (`wasm`) or a Lua script (`lua`), which may `require` the other scripts of
  its bundle;
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

## A first app, in three languages

The same program: a view, one call it answers, and one topic it feeds. Whichever you write it in,
it is one file to hand to someone, and `pencere new <name> --lang lua|ts|rust` starts it for you.
The `pencere` command installs with one line, on macOS and Linux ([more in its
README](cli/README.md#install)):

```sh
curl -fsSL https://raw.githubusercontent.com/bateri/pencere/main/cli/install.sh | sh
```

**Lua**, run by the `pencere` command (`pencere build` makes one executable of it). The program is
`src/program/app.lua`, the view `src/view/page.lua` and the pictures `src/assets/`:

```lua
local pencere = require("pencere")

pencere.serve { -- shows src/view/: page.lua, a script of the kit's page API, drawn by the terminal
  calls = { hello = function(params) return { message = "Hello, " .. params.name .. "!" } end },
  topics = {
    clock = function(_, sub)
      local timer = pencere.time.every(1, function() sub:push({ now = pencere.time.now() }) end)
      sub:on_end(function() timer:cancel() end)
    end,
  },
  fallback = function() print("This terminal shows no views.") end,
}
```

The view of a Lua app can be a web page instead (`src/view/index.html`, started by
`pencere new app --page web`: HTML and JavaScript, no npm and no build step; a page a tool builds is
packed with `pencere build --web dist/view`), or both, the terminal taking the first it can show
([`cli`](cli/README.md)).

**TypeScript**, on Node, Deno or Bun (`deno compile` makes one executable of it):

```ts
import { bundleFromDirectory, serve } from "@pencere/sdk";

await serve({
  views: [{ kind: "web", bundle: await bundleFromDirectory(new URL("./view/", import.meta.url)) }],
  calls: { hello: (params) => ({ message: `Hello, ${(params as { name: string }).name}!` }) },
  topics: {
    clock: (_params, subscription) => {
      const timer = setInterval(() => subscription.push({ now: Date.now() / 1000 }), 1000);
      return () => clearInterval(timer);
    },
  },
  fallback: () => console.log("This terminal shows no views."),
});
```

**Rust**, with the `pencere` crate:

```rust
let outcome = pencere::serve(
    Options::new()
        .view(View::web(Bundle::from_dir("view", "index.html")?))
        .call("hello", |params, _call| Ok(json!({ "message": format!("Hello, {}!", params["name"]) })))
        .topic("clock", |_params, subscription| {
            let feed = subscription.clone();
            std::thread::spawn(move || {
                while feed.push(json!({ "now": now() })) && !feed.sleep(Duration::from_secs(1)) {}
            });
            Ok(())
        })
        .fallback(|_| println!("This terminal shows no views.")),
)?;
```

## Status

Early: everything is `0.x`, and a minor release may still break. The repository holds

- the specification and its fixtures, the one truth both libraries are tested from
  ([`spec/draft`](spec/draft/pencere.md));
- a TypeScript SDK for programs that show a view ([`@pencere/sdk`](sdk/typescript)), and the same
  for Rust (crate `pencere`, [`program`](program));
- the `pencere` command ([`cli`](cli)): `new`, `dev` and `build` for apps written in Lua, which it
  runs and packs into one executable per target, and the runner a built app is;
- a Rust library for terminals that host views, doing no I/O (crate `pencere-host`,
  [`host/rust`](host/rust)), and the part of the wire it shares with programs (crate
  `pencere-wire`, [`wire`](wire));
- the kit a terminal draws native views with: the page engine, the sandbox that runs a component
  or a Lua script, the drive that does to a page what every host does, the interpreter and the SDK a
  page is written with ([`kit`](kit));
- a stand-in terminal that plays the host, for trying a program and for tests
  ([`host/fake`](host/fake));
- an example program with a web view to try it all with ([`examples/containers`](examples/containers)),
  and the same list as a native view of the terminal, as a WebAssembly component or a Lua script
  ([`examples/containers-rust`](examples/containers-rust), written with the program library), and
  the same as a Lua app of one script and a page ([`examples/containers-lua`](examples/containers-lua)).

[bateri](https://github.com/bateri/bateri) is the first host. The SDK is on npm, the crates on
crates.io and the command on the GitHub releases; how they are released is in
[RELEASING.md](RELEASING.md).

## License

Apache-2.0 — see [LICENSE](LICENSE). The specification, the libraries and the tools are
all under it, so any terminal or program, open or closed, can implement the protocol or embed
this code. Copyright belongs to the pencere Authors ([NOTICE](NOTICE), [AUTHORS](AUTHORS)):
everyone who contributes keeps the copyright of their contribution.
