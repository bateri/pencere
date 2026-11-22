# {{name}}

An app for [pencere](https://github.com/bateri/pencere): a program that shows a view in the pane of
the terminal it runs in. The program is Lua; the view is a web page.

- `src/program/` is the program (`app.lua` and the modules it requires), `src/view/` the web page it
  shows (`index.html` is its entry), `src/assets/` the pictures the program names as `assets/…`.
- `pencere dev` runs it in this pane, and again when a file under `src/` changes.
- `pencere build` makes one executable of it, `dist/{{name}}`.

What each does, in full: [the command's README](https://pencere.dev/kit/command).

`src/view/pencere-page.js` is the bridge between the page and the program (`@pencere/sdk/page` of the
TypeScript SDK, one file with no dependencies); the page imports it by the name its import map gives.
It is a copy: `pencere new` writes the one of the version of `pencere` that made this project.
The program runs with the rights of whoever starts it, as any program does; the page is sandboxed
and reaches nothing but its program unless `serve` is given an `access` for it.

There is no `test/` here: `pencere test` opens pages of the kit (Lua) under a host with no screen,
and a web page needs a browser. `pencere new --page lua` makes a project whose page can be tested.
