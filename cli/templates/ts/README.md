# {{name}}

A program that shows a view in the pane of the terminal it runs in, written with
[`@pencere/sdk`](https://www.npmjs.com/package/@pencere/sdk) for Deno (Node and Bun run it too).

- `src/program/` is the program (`main.ts`), `src/view/` the web page it shows (`index.html` is its
  entry), `src/assets/` the pictures an app ships, if it has any.
- `deno task dev` runs it, and runs it again when a file under `src/` changes.
- `deno task build` makes one executable, `dist/{{name}}`, with the Deno runtime, the program and
  the view in it. Nothing generated is under `src/`.

There is no `test/` here: `pencere test` opens pages of the kit (Lua) under a host with no screen,
and a web page needs a browser. `pencere new --lang ts --page lua` makes a project whose page is a
page of the kit, with its first test.
