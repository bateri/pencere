# browser-lua

An address line and a browser as an app of one Lua file (`src/program/app.lua`) and a page
(`src/view/page.lua`), run by `pencere`:

```sh
pencere dev          # run it in this pane, and again when a file changes
pencere build        # one executable: dist/browser-lua
dist/browser-lua     # the view, in a terminal that hosts pencere views and their browsers
```

The browser is the terminal's (`pencere.browser`): it loads the pages and draws them in its room,
and the page gets nothing of what is loaded: where the browser is and what it is doing, in `browser`
events, and the asks the page makes of it in a reply (`go`, `back`, `forward`, `reload`, `stop`). The
page asks for its first address when the browser says it is ready, follows the browser with its
address line, says what it was told (the title, a load that failed, a status that is not ready), and
keeps ⌘L for itself (`keys`) while the browser has the keyboard. A terminal that shows no browser says
so, and the page says it too.

The tests (`test/browser_test.lua`) run the page under `pencere test` and tell it what the browser's
engine would; the drive's tests run it against the host with no screen
(`kit/drive/src/luau_browser_tests.rs`).
