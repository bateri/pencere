# editor-lua

An editor of SQL as an app of one Lua file (`src/program/app.lua`) and a page
(`src/view/page.lua`), run by `pencere`:

```sh
pencere dev          # run it in this pane, and again when a file changes
pencere build        # one executable: dist/editor-lua
dist/editor-lua      # the view, in a terminal that hosts pencere views and their editors
```

The editor is the terminal's (`pencere.editor`): the document, the carets, undoing, the input
method and the drawing are its own. The page makes a language of it with the hooks the kit gives:
it colours the lines in view with a small lexer, makes each statement of more than one line a
region that folds, opens a popup of completions by the caret, and sends the statement under the
caret to the program with ⌘↩. It loads a large sample in pieces when it opens.

The sandbox's tests run this page against a host's editors (`kit/sandbox/src/luau_tests.rs`).
