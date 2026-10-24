# containers

A program with a view, to try pencere with. It lists some made-up containers, shows what each
running one uses, and lets you stop and start them; nothing here needs Docker.

- `src/containers.ts` is the program: the calls the view may make (`list`, `stop`, `start`) and the
  topic it may subscribe to (`stats`, pushed at once and then every second). Nothing in it knows
  about a terminal.
- `src/main.ts` is the only place that asks a terminal to show the view, with `serve` from
  `@pencere/sdk`.
- `view/` is the view, a web page: `index.html`, `style.css` and `app.js`, written by hand against
  the bridge of a page (specification §9.4) with no build step and no library. It asks the host
  for its `context` and follows it, so the colours, the font and the light or dark mode are the
  terminal's; it calls `list`, `stop` and `start`, and draws what `stats` pushes. The terminal
  gets the three files once and keeps them by content.

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
