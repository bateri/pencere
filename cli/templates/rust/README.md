# {{name}}

A program that shows a view in the pane of the terminal it runs in, written with the
[`pencere`](https://crates.io/crates/pencere) library. In a terminal that hosts pencere it shows
the view; in any other it prints the text.

- `pencere dev` runs it in this pane, and again when the program, the page or a picture changes.
- `pencere build` makes the program for this machine in `dist/`.
- `src/program/main.rs` is the program: its calls and topics.
- `src/view/page.lua` is the view it shows, with the scripts it `require`s, and `src/assets/` the
  pictures of the view; `build.rs` carries both into the program (`pencere::app!()`).
- `pencere test` runs the tests in `test/` against the page, under a host with no screen.
- Nothing generated is under `src/`: cargo builds into `target/`.

What each does, in full: [the command's README](https://pencere.dev/kit/command).
