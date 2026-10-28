# ui-lua

The components of the kit's library (`@pencere/ui`) on one page: an app of one Lua file
(`src/program/app.lua`) and a page (`src/view/page.lua`), run by `pencere`:

```sh
pencere dev          # run it in this pane, and again when a file changes
pencere build        # one executable: dist/ui-lua
dist/ui-lua          # the view, in a terminal that hosts pencere views
```

Each component is drawn as an app would draw it, and what the person does with it is written at the
foot of the page as the action the page was given. The page draws nothing of its own but the
headings of its sections: every colour is a role of the terminal's theme, so the gallery is the
same page in a light theme and a dark one.

The drive's tests run this page under the host with no screen and picture it in both themes
(`kit/drive/src/shot_tests.rs`); each component has tests of its own
(`kit/drive/src/library_ui_tests.rs`).
