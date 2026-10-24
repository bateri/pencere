# {{name}}

An app for [pencere](https://github.com/bateri/pencere): a program that shows a view in the pane of
the terminal it runs in.

- `app.lua` is the program. `page.lua` is the view; `assets/` holds its pictures.
- `pencere dev` runs it in this pane and runs it again when a file changes. `print` goes to
  `.pencere/log`, since the terminal is under the view.
- `pencere build` makes one executable, `{{name}}`, of the app. Give it to anyone with the same
  kind of machine; `pencere build --target x86_64-unknown-linux-musl` makes it for another.

The program runs with the rights of whoever starts it, as any program does. Nothing in an app is
sandboxed; the view is.
