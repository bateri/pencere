# {{name}}

An app for [pencere](https://github.com/bateri/pencere): a program that shows a view in the pane of
the terminal it runs in. The program is Lua (`app.lua`); the view is a web page (`web/`).

- `pencere dev` runs it in this pane and runs it again when a file changes; a change to the page
  shows the page again without ending the program. `print` goes to `.pencere/log`, since the
  terminal is under the view.
- `pencere build` makes one executable, `{{name}}`, of the app: the program, the page and the
  interpreter in one file. `pencere build --target x86_64-unknown-linux-musl` makes it for another
  kind of machine.

`web/pencere-page.js` is the bridge between the page and the program (`@pencere/sdk/page` of the
TypeScript SDK, one file with no dependencies); the page imports it by the name its import map gives.
It is a copy: `pencere new` writes the one of the version of `pencere` that made this project.
The program runs with the rights of whoever starts it, as any program does; the page is sandboxed
and reaches nothing but its program unless `serve` is given an `access` for it.
