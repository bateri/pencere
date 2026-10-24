# containers

A program with a view, to try pencere with. It lists some made-up containers, shows what each
running one uses, and lets you stop and start them; nothing here needs Docker.

- `src/containers.ts` is the program: the calls the view may make (`list`, `stop`, `start`) and the
  topic it may subscribe to (`stats`, pushed at once and then every second). Nothing in it knows
  about a terminal.
- `src/main.ts` is the only place that asks a terminal to show the view, with `serve` from
  `@pencere/sdk`.
- `view/` is the view, a web page: `index.html`, `style.css` and `app.js`, with no build step and no
  library but the bridge of a page, `@pencere/sdk/page`. A page has no `node_modules`: `main.ts`
  puts that module in the bundle beside the page (as `pencere-page.js`, with the `files` option of
  `bundleFromDirectory`) and an import map in `index.html` tells the browser where the name is.
  The page asks the host for its `context` and follows it, so the colours, the font and the light
  or dark mode are the terminal's; it calls `list`, `stop` and `start`, and draws what `stats`
  pushes. The terminal gets the four files once and keeps them by content.

Started in a terminal that hosts pencere, the view opens in the pane and ⌃C ends the program and
gives the terminal back. Started in one that does not, the program prints the containers and
exits.

## Try it without a terminal that hosts pencere

From the root of the repository, after `make setup` once:

```sh
npx tsc -b
cargo run -p pencere-fake-host -- --subscribe 'stats={"every":200}' --call 'stop={"id":"web"}' \
  --then interrupt -- node examples/containers/dist/main.js
```

The [fake host](../../host/fake/README.md) plays the terminal and a scripted page and writes what
happens, in order. Leave out `--then interrupt` to end the program with ⌃C yourself, and add
`--store some-directory` to see the second run find the files already kept.

## Deno and Bun

The program runs unchanged on Deno and Bun (the SDK needs nothing to be told which it is on):

```sh
npx tsc -b
deno run --allow-read --allow-run examples/containers/dist/main.js
bun examples/containers/dist/main.js
```

## One executable

`deno compile` makes a single file of the program, the Deno runtime, the SDK and the view, which
runs in a terminal that hosts pencere with nothing installed:

```sh
npx tsc -b
cd examples/containers
deno task compile        # writes bin/containers
./bin/containers
```

The task is in [`deno.jsonc`](deno.jsonc). `--include` puts in the files the program reads at run
time, which `deno compile` cannot see (the view, and the page's bridge module); `--no-config` and
the [import map](import-map.json) keep the executable to what the program needs, since in this
repository, an npm workspace, Deno would also put all of `node_modules` in it. In a project of your
own the import map names the package: `"@pencere/sdk": "npm:@pencere/sdk@^0.2.0"`. The first
compile for a version of Deno downloads that version's runtime.
