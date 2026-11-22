# {{name}}

An app for [pencere](https://github.com/bateri/pencere): a program that shows a view in the pane of
the terminal it runs in.

- `src/program/` is the program (`app.lua` and the modules it requires), `src/view/` the page it
  shows (`page.lua`), `src/assets/` the pictures the page and the program name as `assets/…`.
- `pencere dev` runs it in this pane, and again when a file under `src/` changes.
- `pencere build` makes one executable of it, `dist/{{name}}`.
- `pencere test` runs the tests in `test/` against the page, under a host with no screen.

What each does, in full: [the command's README](https://pencere.dev/kit/command).

The program runs with the rights of whoever starts it, as any program does. Nothing in an app is
sandboxed; the view is.
