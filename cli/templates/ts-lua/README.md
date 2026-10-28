# {{name}}

A program that shows a page of the kit in the pane of the terminal it runs in, written with
[`@pencere/sdk`](https://www.npmjs.com/package/@pencere/sdk) for Deno (Node and Bun run it too).

- `src/program/` is the program (`main.ts`), `src/view/` the page it shows (`page.lua`, and the
  pictures it names as `assets/…` in `src/view/assets/`): the terminal draws the page itself, in its
  own font and theme.
- `deno task dev` runs it, and runs it again when a file under `src/` changes.
- `deno task build` makes one executable, `dist/{{name}}`, with the Deno runtime, the program and
  the view in it. Nothing generated is under `src/`.
- `pencere test` runs the tests in `test/`: Lua files (`*_test.lua`) that open the page under a host
  with no screen and do to it what a person does. The program is a stand-in made of the calls the
  page makes (`calls = { hello = function(params) … end }`); to test the page with the real program,
  name it: `test.open({ command = { "deno", "run", "-A", "src/program/main.ts" } })`.
