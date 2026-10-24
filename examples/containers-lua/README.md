# containers-lua

A made-up Docker host as an app of one Lua file (`src/program/app.lua`) and a page
(`src/view/page.lua`) with its pictures (`src/assets/`), run by `pencere`:

```sh
pencere dev                  # run it in this pane, and again when a file changes
pencere build                # one executable: dist/containers-lua
dist/containers-lua          # the view, in a terminal that hosts pencere views
dist/containers-lua --docker # the containers of this machine's Docker
dist/containers-lua --text   # the list as text
dist/containers-lua --log /tmp/containers.log   # print and errors to a file, not the terminal
```

`pencere build --target x86_64-unknown-linux-musl` makes the same app for a Linux machine, with
no toolchain for it: the runner of that target is fetched from the release.
