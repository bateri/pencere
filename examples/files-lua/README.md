# files-lua

An app that moves files with its view: its page shows a picture the program attaches, receives a
file the person drops on it, pastes or chooses in the Open panel, and saves a copy of it where the
person chooses in the Save panel. Run by `pencere`:

```sh
pencere dev                          # run it in this pane, and again when a file changes
pencere build                        # one executable: dist/files-lua
dist/files-lua --into ~/Downloads    # received files go there (by default, where it is run)
pencere test                         # the page's tests, under the host with no screen
```

It needs a terminal that opens `lua` 2.1, the version of the page API that has files beside the
bundle (the protocol, Sections 8.8 to 8.10). The program's side is `pencere.files`
(`src/program/app.lua`); the page's is the `files` event and a reply's `pick` and `save`
(`src/view/page.lua`). The view never sees a file's bytes: it is handed a handle, passes it to its
program, and draws the file by it.
