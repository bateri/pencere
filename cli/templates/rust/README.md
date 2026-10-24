# {{name}}

A program that shows a view in the pane of the terminal it runs in, written with the
[`pencere`](https://crates.io/crates/pencere) library. `cargo run` in a terminal that hosts pencere
shows the view; in any other it prints the text.

- `src/program/main.rs` is the program: its calls and topics.
- `src/view/page.lua` is the view it shows and `src/assets/` the pictures of the view; both are
  taken into the program when it is built.
- Nothing generated is under `src/`: cargo builds into `target/`.
