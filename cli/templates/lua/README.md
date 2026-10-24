# {{name}}

An app for [pencere](https://github.com/bateri/pencere): a program that shows a view in the pane of
the terminal it runs in.

- `src/program/` is the program (`app.lua` and the modules it requires), `src/view/` the page it
  shows (`page.lua`), `src/assets/` the pictures the page and the program name as `assets/…`.
- Nothing generated is under `src/`: `pencere build` makes one executable, `dist/{{name}}`, of the
  app. Give it to anyone with the same kind of machine; `pencere build --target
x86_64-unknown-linux-musl` makes it for another.
- `pencere dev` runs it in this pane and runs it again when a file under `src/` changes. `print`
  goes to `.pencere/log`, since the terminal is under the view.

The program runs with the rights of whoever starts it, as any program does. Nothing in an app is
sandboxed; the view is.
