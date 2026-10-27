# pencere-editor

The editor of text of [pencere](https://github.com/bateri/pencere)'s native views: a document, its
selections, its history and what an input method composes, which a terminal puts in the room a page
gives it (`pencere_view::Element::editor`). It is the host's, as a text field is, and written once
here, so that it is the same in every terminal.

A page is told what changed and where the carets are, changes the text against the version it saw
(carried over what the person typed since), colours it, marks it and says where it folds, by layers,
and reads it through a copy the host posts it. What the words mean — the language, completions,
errors, what a key does in an editor of modes — is the page's or its program's: the editor knows no
language. Places are characters (code points); the units of UTF-16 an input method of macOS counts
in are turned into places here.

It depends on the page engine (`pencere-view`), the rope it keeps the text in (`ropey`), the
boundaries of characters and words (`unicode-segmentation`) and regular expressions (`regex`), and
on no platform, no WebAssembly runtime and no font library (`make check`).

The examples in `fixtures/` are what an edit means; the tests run them.

## License

Apache-2.0 — see [LICENSE](LICENSE).
