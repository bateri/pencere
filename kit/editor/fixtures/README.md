# The editor's examples

What the editor does, written as examples: a text, the steps a person, a page and an input method
take on it, and what the editor holds after them. They are the one statement of what an edit
means; `tests/fixtures.rs` runs every one of them against the editor (`cargo test -p
pencere-editor`). The page's side of the same (the queries, the events, the replies) is tested in
the sandbox's tests.

Places are characters (code points), from 0; a range is `[from, to]` (a selection's is
`[anchor, head]`), the end not in it.

## A file

```json
{ "examples": [{ "about": "…", "text": "…", "options": {}, "room": 5, "steps": [] }] }
```

- `about` — what the example shows, in words.
- `text` — the document at version 0, a caret at its start.
- `options` (optional) — `tab_width`, `hard_tabs`, `read_only`, `comment` (what starts a line
  comment, `"--"`, or `{ "line": "--", "block": ["/*", "*/"] }`, either part left out), `pairs`
  (each two characters, opener and closer: `["()", "\"\""]`), `indent`
  (`{ "increase": "…", "decrease": "…" }`, regular expressions) and `folds` (`"indent"` to fold by
  the indent as well).
- `room` (optional) — the most bytes the document may come to, in place of the 16 MiB of an editor.
- `steps` — done in turn. Each has `do`, and may have `at` (the time, seconds, from then on: what
  undoing groups by) and `repeat` (done that many times).

## Steps

| `do`          | What                                                                                                                                                                         |
| ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `select`      | The person puts the selections at `ranges`, the main one `main` (0).                                                                                                         |
| `type`        | The person types `text` at every caret.                                                                                                                                      |
| `paste`       | The person pastes `text`.                                                                                                                                                    |
| `tab`         | The tab key.                                                                                                                                                                 |
| `copy`, `cut` | What a copy (or a cut) takes is `gives`.                                                                                                                                     |
| `command`     | An editing command: `command` is its name in `snake_case` (`delete_backward`, `move_lines_up`, `find` with `pattern`, `regex`, `case`, `move` with `movement` and `extend`). |
| `change`      | The page's `changes` (`[from, to, insert]` each), against the version `base`; `history` `false` for one not to be undone, `tag` its name.                                    |
| `page_select` | The page's selections `ranges`, in the places of `version`.                                                                                                                  |
| `decorate`    | The page's decorations of `layer` for `from`–`to`, of `version`: `[from, to, kind]` each, a kind `style`, `line`, `gutter` or `fold`.                                        |
| `load`        | A piece of the document the page loads, `text`; `more` says another follows.                                                                                                 |
| `compose`     | An input method composes `text`, `selected` the part of it it selects, in place of `replace` where it names one.                                                             |
| `commit`      | The input method is done with what it composed.                                                                                                                              |
| `commit_text` | The input method puts `text` in, for good.                                                                                                                                   |
| `abort`       | The input method gives up.                                                                                                                                                   |
| `frame`       | A frame of the host's: the editor searches again, where the document changed since it searched.                                                                              |
| `expect`      | What the editor holds now (below).                                                                                                                                           |

A `find` has `word` as well, and `replace` and `replace_all` have the `text` put in.

## What is expected

Each key of an `expect` is checked where it is there:

- `text`, `version`, `ranges`, `main`, `composing`, `marked` (`[from, to]`, or `null`);
- `decorations` — for each layer named, its decorations as `[from, to, kind]`, in order;
- `folded` — the folded regions; `hidden` — the lines they hide, `[first, last]`; `folds` — the
  regions that fold, the one from each line that one folds from (the page's, or by the indent), and
  the lines the gutter has a button by are their first lines;
- `found` — what the search found;
- `utf16` — `[place, units]` pairs: the place is that many units of UTF-16 into the text, both ways;
- `bracket` — `[at, to]`: the bracket at (or before) `at` matches the one at `to` (`null`: none);
- `notes` — what the editor told the page since the last `expect`, in order: `{ "edited": { "version",
"origin", "tag", "changes" } }`, `{ "selection": { "ranges", "main", "composing" } }`,
  `{ "refused": "<reason>" }`, `{ "found": { "count", "current", "more" } }` (`current` from 1,
  left out for none). Every `expect` takes the notes, whether it checks them or not.

`language.json` holds what the page says of its language (its comments, the rules of its indent,
folding by the indent), and that an editor that says none of it is as one for prose was before.

`invalid.json` holds what the editor refuses: a range the wrong way round, a place past the end,
changes that overlap, a version too old or yet to come, a document too large. Each is refused, the
page is told why, and nothing of it is done.
