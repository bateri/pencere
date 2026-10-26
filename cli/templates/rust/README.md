# {{name}}

A program that shows a view in the pane of the terminal it runs in, written with the
[`pencere`](https://crates.io/crates/pencere) library. In a terminal that hosts pencere it shows
the view; in any other it prints the text.

- `pencere dev` runs it in this pane, builds and runs it again when the program changes, and shows
  the view again, without ending the program, when the page or a picture changes.
- `pencere build` makes the program for this machine in `dist/` (`--target` for another).
- `src/program/main.rs` is the program: its calls and topics.
- `src/view/page.lua` is the view it shows, with the scripts it `require`s, and `src/assets/` the
  pictures of the view; `build.rs` carries both into the program (`pencere::app!()`).
- Nothing generated is under `src/`: cargo builds into `target/`.
