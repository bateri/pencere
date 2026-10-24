# {{name}}

A program that shows a view in the pane of the terminal it runs in, written with
[`@pencere/sdk`](https://www.npmjs.com/package/@pencere/sdk) for Deno (Node and Bun run it too).

- `deno task dev` runs it, and runs it again when a file changes.
- `deno task build` makes one executable, `{{name}}`, with the Deno runtime, the program and the
  view in it.
