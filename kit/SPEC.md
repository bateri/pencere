# The pencere kit: the page API, colours and sandbox floors

Version `2.0` of the kinds `lua` and `wasm` ([the protocol](../spec/draft/pencere.md), Sections 5.5
and 9.8). This document is normative for what the protocol leaves to the kit: with the interface
[`wit/view.wit`](wit/view.wit) for `wasm` views, and with the prelude [`luau/prelude`](luau/prelude)
for `lua` views. The key words are those of Section 2 of the protocol. A statement here that the
interface or the prelude contradicts is a mistake in this document.

The kit is what a view of the kinds `wasm` and `lua` is written against. It does not draw: the
view says what the page is, and the host lays it out, resolves its colours in the theme of the pane
and draws it. A host that keeps the rules of this document and the sandbox floors shows the same
page as any other.

## 1. Versions

| Kind   | The version is                                                                                    | Source of truth               |
| ------ | ------------------------------------------------------------------------------------------------- | ----------------------------- |
| `wasm` | `3.0`: the component interface `pencere:view@3.0.0`                                               | `wit/view.wit`                |
| `lua`  | `2.1` (and `2.0`): the page API whose `pencere.version` is `2`, in Luau `0.740` (`kit/luau/pins`) | `luau/prelude`, this document |

The two kinds are numbered apart (protocol, Section 5.4). `lua` `2.x` is bound to the release of
Luau that the kit pins: a script that needs syntax or a library function a later Luau added is not a
`lua` `2.x` script, and such a Luau arrives with a new minor version of `lua`. A host SHOULD run the
kit's own interpreter, the component `kit/sandbox/luau/luau.wasm`, so that a script means one thing
on every host.

`2.0` is `1.0` and these, each a change to what a view or a host already used (so a major version):
overlays and anchors, texts of many looks, canvases, commands and their keys, dragging and dropping,
what an element is to a screen reader and its state, fills from one colour to another, a middle
button, keys that go up, the modifiers alone and files dropped on the page (Sections 2.1 to 2.3 and
4); editors of text (Sections 2.1 and 2.3, and the host's part in Section 4), with their search and
what it finds (`found`), what a page says of its language (its comments, the rules of its indent,
folding by the indent), the spaces, tabs and guides of the indent they show; a reply's `link` and
`wake`, with the kit's timers, `pencere.time` (Sections 2.2 and 2.3); the kit's library, with a
language that colours an editor, `@pencere/syntax` (Section 2.6); what a reply selects in a field and
the keys a field has no use for, commands done by more than one set of keys and menus' entries with
keys (Sections 2.1 and 2.3); and an editor's changes kept out of its history and named, the place it
reveals put where the page says, the other places of its selection marked, its `editor_ready` and
the commands of whole lines (Sections 2.1 and 2.3); the time of day and the host's keyboard, with
`mod` (Sections 2.1 and 2.2, and the interface's `system`), what spins, and the `null` members of
what a page decodes (Sections 2.1 and 2.4); and the pointer's rest over a word of an editor, the names
of ranges of its text that an overlay stands by, and the names of its styles; widgets in lines; the presses a page maps and the keys it changes, with chords; and looks
drawn by the modifiers held (Sections 2.1, 2.3 and 2.6); and how an editor's selection looks, with the corners of its lines rounded as a terminal's are, and its caret (its corners, light, blinking, motion and look away from the keyboard) (Section 2.1); and what the host says of how its terminal draws its cursor and its selection and of what it draws, which a page reads and decides on (Sections 2.2, 2.3, 2.6 and 4). The kit's own sandbox opens `2.0` and no longer `1.0`.

`2.1` is `2.0` and the files beside the bundle (protocol, Sections 8.8 to 8.10), each an addition a
`2.0` script does not notice (so a minor version of `lua`, and a major one of `wasm`, whose records
change): the `files` a page is told carry how the person gave them (`source`) and each file's handle,
name, size, type and, on the person's machine, its path; `files_over` and `files_left` while files
are dragged over the page; `saving` when the person chose a place in the Save panel; a reply's `pick`
and `save`, the Open and the Save panel, shown only for a reply to something the person did; and an
image named `sha256:` and a digest (a file the program attached) or `file:` and a handle (one the
person gave) (Sections 2.1, 2.3 and 5). A host that opens `lua` `2.1` opens `2.0` too; the kit's own
sandbox opens `wasm` `3.0` and no longer `2.0`.

The regular expressions of a view (an editor's search, the rules of its indent) are in the syntax of
Rust's `regex` crate, `1.x` at the release the kit pins (its `Cargo.lock`), and one may take at most
4 MiB compiled (`pencere_view::edit::limits::PATTERN_SIZE`): a pattern that would take more is none.
A release of `regex` whose syntax a view could notice (a pattern it reads otherwise, or one it takes
that an earlier refused) arrives with a new minor version of both kinds, as a later Luau does for
`lua`.

## 2. The Lua page API

A script is Luau. What it can use besides the language is the base functions, `string`, `table`,
`math`, `utf8`, `require` (the other scripts of its bundle and the kit's library: Sections 2.5 and
2.6), and `pencere`, the table the prelude fills. It has no files of the system's, no network, no
clock but the time `render` and a wake hand it (Section 2.3) and the time of day its host says
(Section 2.2), and no randomness.

A script **returns the page**, a table of the functions the host calls:

| Function                                                              | Called when                                                                                                                                                                                                                                                                                      |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `render(size, now, reduce_motion) -> element[, animating[, wake_at]]` | The host draws. `size` is `{ w =, h = }` in points, `now` seconds on the host's clock, `reduce_motion` the person's setting. `animating` asks for the next frame at once; a page at rest costs no frames. `wake_at` (a time on `now`'s clock) asks a page at rest to be rendered once more then. |
| `settle() -> boolean` (optional)                                      | The page was laid out: bring back what depends on the layout. `true` when something changed. (A timer set here is asked for: the kit has the page rendered again.)                                                                                                                               |
| `handle(event) -> reply` (optional)                                   | The person did something, or the time the page asked to be woken at came. See Section 2.3.                                                                                                                                                                                                       |
| `message(message) -> reply` (optional)                                | The program said something that no `pencere.call` or `pencere.subscribe` of the page was waiting for. `message` is the decoded JSON.                                                                                                                                                             |
| `room() -> reply` (optional)                                          | The program has read enough that a message `pencere.send` answered `false` to may be sent again.                                                                                                                                                                                                 |

### 2.1 Elements

An element is a plain table that a chain of builder methods fills. Lengths are points.

| Constructor                      | Element                                                                                                            |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `pencere.div()`                  | A box: a column of its children until told otherwise.                                                              |
| `pencere.text(string \| number)` | One line of text, in the font, size and colour of the boxes it is in; with `wrap`, as many lines as its box needs. |
| `pencere.spark(values, color)`   | A sparkline of numbers, newest last; `:peak(value)` is its largest.                                                |
| `pencere.image(path)`            | A picture: a file of the view's own bundle (protocol, Section 9.8).                                                |
| `pencere.rich(parts)`            | A text of many looks: its parts (`pencere.span(words)`, or a string) one after the other, as one paragraph.        |
| `pencere.canvas(paths)`          | A box with paths drawn in it (`pencere.path()`), in its own points. It has no size of its own.                     |
| `pencere.editor(id, options)`    | The room of an editor of text the host keeps, lays out and draws (below); `div():editor(id, options)` the same.    |
| `pencere.spacer()`               | A box that takes the room left over.                                                                               |

The builders return the element. Layout: `row`, `col`, `wrap`, `grid(columns)`, `grid_rows(rows)`,
`span`, `hidden`, `items_start|center|end|stretch`, `justify_start|center|end|between`, `gap`,
`gap_x`, `gap_y`, `grow`, `no_shrink`, `w`, `h`, `size`, `w_pct`, `w_full`, `h_full`, `size_full`,
`min_w`, `min_h`, `max_w`, `max_h`, `p`, `px`, `py`, `pt|pr|pb|pl`, `mt|mr|mb|ml`, `absolute`, `inset`,
`overlay`, `anchor(id, side, { gap =, align = })`, `clip`, `scroll()` (a box whose content scrolls
behind it, down and across: the host scrolls it, by its id, Section 2.2).
Looks: `bg`, `bg_gradient(angle, from, to)`, `border(width, color)`, `rounded`, `rounded_corners`,
`shadow(dx, dy, blur, color)`, `opacity(0..1)` (the element with all inside it: each thing it draws
is that much fainter, its words too, past a host's floor of contrast). Text, which the elements
inside inherit: `text_size`, `weight`, `color`,
`family("system" | "terminal")`, `align("start" | "center" | "end")`, `italic`, `underline`, `strike`,
`truncate`. Images: `fit("contain" | "cover")`, `tint(color)`. Names and answers: `id`, `press(id)`,
`focusable`, `focused`, `text_field(id, "line" | "lines" | "secret")`, `label(words)`, `tip(words,
keys)`, `command(id, title, keys, enabled)`, `draggable(id)`, `drop_target(id)`. To a screen reader:
`role(name, level)`, `checked(on)`, `selected(chosen)`, `expanded(open)`, `disabled(off)`,
`value(words)`. Motion: `motion({...})`. Children: `child`, `children`. A grid track is a number
(points), `pencere.fr(n)`, `pencere.range(least, n)` or `"auto"`.

A text is one line: what does not fit runs past its box, or, with `truncate`, ends in "…". `wrap` on
a text puts its words in new lines where its box is too narrow for them (and at its line breaks),
and its box is as tall as the lines; in a row it gives its room away down to its longest word. A
text inside a `text_field` element is what the host's field says while it is empty
(`div():text_field("filter"):child(text("Filter images"))`): the host draws it, the page does not.

A **text field** is a field of the host's own, put over the room of the element that says it is one,
and named by that element's id (`text_field(id)`, or the element's own id): a page has as many as it
names, and an element that says it is a field and has no name is not one. `"line"` (the default) is
one line; `"lines"` breaks the line at enter and scrolls; `"secret"` is one line whose characters are
not shown and are never copied. The page is told what a field holds when the person changes it
(`text`), sends the keyboard to one (`keyboard = { field = name }`), and puts a text in one, selects
in one, or both (`fill`, Section 2.3): a dialog that renames a file comes up with the name in its
field and the stem selected, a palette opened again with its query selected, so that what is typed
takes its place.

While a field has the keyboard, the keys it has no use for are the page's, as `key` events (a
command's keys first), and the field keeps the keyboard unless the reply sends it elsewhere. A field
has a use for a key that changes its text or what is selected in it, and for no other:

- escape, tab and the function keys are the page's in every field; in one line (`"line"`,
  `"secret"`) so are enter, up, down, page up and page down, whatever is held with them (in one line
  they would only take the caret to an end); in lines, enter is the field's without command or
  control, and up and page up, down and page down, are as left and right are below;
- a character is the field's, and with command or control only where it is one of the field's own
  chords: select all, paste, undo and redo, and copy and cut where something is selected in a field
  whose text may be copied. Every other chord is the page's;
- backspace, left and home are the page's where nothing is selected and the caret is at the start
  (an empty field gives backspace to the page), and delete, right and end where it is at the end,
  whatever is held with them; elsewhere they are the field's.

So a list with a filter above it takes its commands while the person types: the arrows move in the
list, enter chooses, ⌘K opens a palette, and ⌫ in the empty filter of a path goes up a folder. A
host asks the rule with its field's state (`pencere_view::FieldState::takes`); one whose system
gives its fields chords besides these (a Mac's ⌃A and ⌃E) may keep those too.

A **label** is what an element is in words, for a person who does not see it: a host that offers its
system's screen reader says the label of an element that has one, and the words inside one that has
none. A button that is an icon alone needs one.

A **tip** is what the host says of an element while the pointer rests on it, with the keys that do
the same where there are some (`tip("Containers", "1")`): a tooltip of the host's own.

An **overlay** (`overlay()`) is taken out of the flow and out of every box that holds it: it is placed
by its `inset` against the page (`inset(0, 0, 0, 0)` is the whole page, the ground of a dialog), drawn
after everything else (so over it), cut by no box that holds it, and found by the pointer before what
is under it. What it is written inside still holds it for the rest: it is as faint as that is, hidden
with it, and the path a key takes goes out through it (`layout.path_of`). `anchor(id, side, options)`
makes an element an overlay **beside the element named `id`**, on `side` of it (`"below"`, the
default, `"above"`, `"right"` or `"left"`), `options.gap` points away, its start, middle or end lined
up with the element's (`options.align`: `"start"`, the default, `"center"` or `"end"`). Where the page
has not the room for it on that side and has it on the other, it goes on the other; along the side it
is moved as far as it takes to stay on the page. Where no element of that name is on the page, it
stands where its `inset` puts it. A menu that drops from a button is
`div():anchor("sort", "below", { gap = 4 }):child(...)`.

A **text of many looks** (`pencere.rich`) is one paragraph made of parts. A part
(`pencere.span(words)`) says only what it changes of the element's look: `color`, `weight` (`bold`),
`italic`, `underline`, `strike`, `family`, a ground behind its words alone (`bg`), and `link(id)`,
which makes it answer the pointer by that name (`layout.hit` says it, a screen reader says a link)
and underlines it. The size is the element's for every part. Where the element `wrap`s, the parts run
on from line to line as the words of one text do, and a word made of two parts with nothing between
them (`**bold**,`) is not broken between them; where it does not, it is one line for each line break,
cut with "…" where it may be (`truncate`).

A **canvas** (`pencere.canvas(paths)`) is a box with paths drawn in it, in its own points from its
top-left corner. A path (`pencere.path()`) is steps — `move_to(x, y)`, `line_to(x, y)`,
`quad_to(cx, cy, x, y)`, `cubic_to(ax, ay, bx, by, x, y)`, `close()`, and `rect(x, y, w, h)`,
`arc(cx, cy, r, from, to)` (degrees clockwise from the top: a ring that fills), `circle(cx, cy, r)`
made of them — with a colour inside it (`fill`: where a line out of a point crosses the path an odd
number of times) and a line along it (`stroke(width, color)`, round where it turns and ends). The
canvas has no size of its own; its style gives it one.

A **command** (`command(id, title, keys, enabled)`) is something the page can be told to do: the host
lists it by its `title` where a person looks for what can be done (its command palette, its menus),
and tells the page `{ type = "command", id = id }` when it is asked for there or by its `keys`, in
place of the key. The keys are written as a person writes them: modifiers (`cmd`, `ctrl`, `alt`,
`shift`) and a key (one character, or the name of a key the page reads) joined by `+`
(`"cmd+shift+k"`, `"ctrl+enter"`, `"f5"`, `"cmd++"` for the key `+`); `pencere.keys(words)` reads
them. `mod` is the main modifier of the host's system (CodeMirror's `Mod`, Electron's `CmdOrCtrl`):
`"mod+s"` is ⌘S on a Mac and Ctrl+S elsewhere. `pencere.keyboard()` says how the host's system makes
and writes its shortcuts: `"mac"` (⌘ the main modifier, the keys written as signs, ⌘⇧K),
`"windows"` (Ctrl the main one, the keys written as words, Ctrl+Shift+K) or `"linux"` (as Windows,
the logo key Super); what a page shows of a shortcut follows it. A command that more than one set of keys does has a list of them
(`{ "ctrl+page_down", "ctrl+alt+right" }`), as an editor's keybindings do: each of them does it, and
a list of the commands shows the first. A command can be done
while the keyboard is on its element or inside it: the page says which element has the keyboard with
`focused()` (the element its `Ui` says has the focus). The commands of the page's outermost box can be
done whatever has the keyboard. Where two say the same keys, the one nearer the keyboard is done; one
that is not `enabled` is listed and its keys do nothing (they are a key for the page).

An element that can be **dragged** (`draggable(id)`) answers the pointer as a pressed one does; a
**drop target** (`drop_target(id)`) is where what is dragged can be put down. The page's `Ui` keeps the
drag (Section 2.2): what the page draws of it while it goes (an overlay at the pointer, the target
marked) is the page's.

An element says what it is to a **screen reader** where its look says it and its answers do not:
`role("checkbox" | "switch" | "radio" | "tab" | "menu_item" | "link" | "heading" | "image" |
"progress" | "slider" | "button" | "text", level)` (a heading's level is 1 to 6), and where it stands:
`checked(on)` (a checkbox, a switch, a radio), `selected(chosen)` (the tab in front, the row picked),
`expanded(open)`, `disabled(off)` (it does not answer the pointer, nor take the keyboard) and
`value(words)` (a progress's `"40%"`). A text field is one by `text_field`, and an editor by `editor`,
not by a role.

An **editor** (`pencere.editor(id, options)`) is the host's, as a text field is: its document, its
carets and selections (as many as the person makes, a column with ⌥), undoing, what an input method
composes, finding, folding and the drawing of its lines are the host's editor's, the same in every
host, and the room scrolls as a box that scrolls does (`pencere.layout.scroll_to(id, …)` scrolls
it). The box's fill and border are the page's. `options` are `wrap` (long lines go on in the next
row), `tab_width` (4), `hard_tabs`, `gutter` (the numbers of the lines, the page's marks and the
buttons that fold; on by default), `read_only` (the person cannot change the text; the page can),
`keys` (`"editor"`, or `"page"`: every key the page's, and the editor types nothing and does none of
its own commands for a key, for an editor of modes the page drives), `hint` (what it says while it is
empty; or the first text inside it) and `pairs` (the brackets and quotes that close themselves,
`"()[]''"`).

What the page says of its **language**, as a language configuration of VS Code does, the editor
does; an editor that says none of it is one for prose, as it was: a line break keeps the line's
indent, what is typed moves no line, nothing is commented and nothing folds but what the page folds.

- `comment` is how the language writes its comments: a line comment (`"--"`), or
  `{ line = "--", block = { "--[[", "]]" } }` with either part left out (the block a pair, or
  `{ open =, close = }`). `"toggle_comment"` comments the lines of each range with the line
  comment, or uncomments them where every line with text on it is commented; a language with block
  comments alone puts the lines of each range in one, or takes them out. `"toggle_block_comment"`
  puts each range in the block comment (a caret's range is its line's text, its indent left out),
  or takes it out where it is in one: the tokens just outside the range or at its ends, a space or a
  tab between them and the text taken with them. Where some ranges are in one and some are not,
  those that are not are put in one. No key does it: a page asks for it (`⌘/` toggles comments).
- `indent = { increase =, decrease = }` are regular expressions (Rust's `regex`, as Section 1 pins
  it: no look-around, no back-references) matched against a line's text, either left out. A line break
  after a line whose text before the caret matches `increase` gives the new line a level of indent
  more, as one after an opener of `pairs` does, and between such a line and what matches `decrease`
  that goes on a line of its own. A line that what is typed makes match `decrease` (it did not
  before) goes a level out from the indent the line above gives (the nearest line with text: its
  own indent, a level more where it matches `increase` or ends with an opener), where it is not
  already further out; an undo takes it back with what was typed. Lua's are
  `increase = "\\b(then|do|else)\\s*$"` and `decrease = "^\\s*(end|else|elseif|until)\\b"`. A
  pattern that is not one fails the page, the words naming the option, and so do more than 64
  different patterns in the editors of one frame. A line longer than 10 000 characters is not read
  by them.
- `folds` is `"page"` (the page's fold decorations alone, Section 2.3; the default) or `"indent"`:
  the editor folds by the indent as well, a line with text folding the lines after it that are
  further in, and the lines with no text among them but not those after the last of them nor a run
  of more than 1 000 (a tab is as far in as to its next stop). The page's folds come with them, and its own is the one on a line
  both fold from. They are worked out from the text as it is, for the lines in view, and so follow
  every change.

How an editor looks is the page's to say, and what it leaves out is the editor's default, in the
theme's colours:

| option                                               | what it is                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | default                                    |
| ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ |
| `line_height`                                        | how tall a row is, times the font's size (1 to 3); the text is in its middle, with a point at least above and below                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | 1.6                                        |
| `padding`                                            | the room kept around the text, points (0 to 64)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | 4                                          |
| `scroll_margin`                                      | the rows kept between the caret and the top or the bottom of the room when the editor scrolls to the caret (at most half the rows in view)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | 0                                          |
| `numbers`                                            | what the numbers of the lines count: `"absolute"`, `"relative"` (the caret's line its own number, every other line how many lines it is from it, a folded region one line) or `"hidden"` (the gutter keeps its marks and fold buttons)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | `"absolute"`                               |
| `gutters`                                            | the gutter's columns, from the left, by name: the editor's `"marks"` (the dots), `"numbers"` and `"folds"` (the buttons that fold) are there where the page says none, in that order, and a page lists the ones it wants, with its own among them (`{ name =, width =, events =, hover =, pad = }`, below); one it leaves out is not there                                                                                                                                                                                                                                                                                                                                                                                                           |
| `marks`                                              | `{ bar =, hover =, ruler = }`: how wide a bar of the gutter is (3 points), how wide one the pointer is over is (6) and how wide the overview ruler is (4); 1 to 16                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `events`                                             | what an editor tells the page of its own, as a list of words: `"hover"` is the pointer's rest over a word and its going from it (`hover` event, below), `"target"` the pointer over a word with a press of the page's held and that press (`target` event, below); none unless asked, for the rare things are told and what comes at the rate of the pointer's moving is not                                                                                                                                                                                                                                                                                                                                                                         | none                                       |
| `hover`                                              | `{ delay = }`: how long the pointer rests over a word before the page is told of it, seconds (0 to 10; VS Code's `editor.hover.delay`, Zed's `hover_popover_delay`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | `{ delay = 0.3 }`                          |
| `gestures`                                           | how a press is taken, by its button, modifiers and count: `{ ["press"] = "place", ["shift+press"] = "extend", ["mod+press"] = "target", ["alt+press"] = "column", ["press×2"] = "word", ["press×3"] = "line" }` are what an editor does where the page says nothing; `"add_caret"` and `false` (nothing) are the others, `"context"` a press that opens the page's menu (below), `"target"` the page's alone (told as a `target` event, no caret moves, the editor has the keyboard; it needs `events = { "target" }`); `"middle+press"` and the other buttons may be `"target"`, `"context"` or `false`                                                                                                                                             | as written                                 |
| `keymap`                                             | the editor's keys the page changes: `{ ["mod+d"] = "select_next", ["mod+/"] = false, ["mod+k mod+c"] = "toggle_comment" }`, the keys of a chord apart by a space (at most four); the name of a command that needs none (`"undo"`, …), `"copy"`, `"cut"`, `"paste"`, `"tab"`, `"escape"`, `"to_line_start"`, `"to_line_end"`, the table of a command (`{ command = "move", movement = "down" }`) or `false` (nothing of the editor's: the key is the page's). A chord waits `chord_timeout` seconds for its next key and the page is told (`keymap_pending`); a key that is none of its next ends it and does nothing; Escape ends it. `pencere.layout.keymap(editor)` says what the keys do now                                                      | as written                                 |
| `chord_timeout`                                      | seconds a chord waits for its next key (0.1 to 10)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | `1`                                        |
| `widgets`                                            | how the groups of widgets (text the page puts in lines, below) look and when they show, by name: `{ hints = { show = "unless_held", hold = "control+alt", size = 0.9, color =, background =, radius = 3, pad = 3, gap = 16, margin = 3 } }`; `show` is `"on"`, `"off"`, `"unless_held"` (VS Code's `onUnlessPressed`: hidden while `hold` is held) or `"when_held"` (`offUnlessPressed`); `size` times the text (0.5 to 2), `radius`, `pad`, `gap` and `margin` points (0 to 64): the corners, the room inside the box, the gap before a widget at a line's end, and the room kept outside the box of an inline one, between it and the characters beside it                                                                                         | as written                                 |
| `links`                                              | `{ delay = }`: the least time between two words the pointer comes to being told of (`target`), seconds (0 to 1)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | `{ delay = 0.05 }`                         |
| `current_line`                                       | where the ground of the main caret's line is drawn: `"text"`, `"gutter"` or `"all"` (both); its colour is `colors.current_line`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | `"text"`                                   |
| `whitespace`                                         | which spaces and tabs are shown, as VS Code's `renderWhitespace`: `"none"`, `"boundary"` (all but a space alone between two words), `"selection"` (those in what is selected), `"trailing"` (those after the last word of a line, every one of a line with no words) or `"all"`                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | `"none"`                                   |
| `indent_guides`                                      | a thin line down each level of indent, the block the main caret is in having its own colour while the editor has the keyboard                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | false                                      |
| `matches`                                            | the other places of the main selection's text, marked where they are in view (CodeMirror's `highlightSelectionMatches`): `"selection"` (or `true`), those of a main selection on one line, at most 200 characters and not all spaces, in the case written, and where it is a whole word only where they are whole words (no letter, digit or `_` just before or after); `"word"`, those and the places of the word a caret is in; at most 100 in view, the selection itself not among them. Only drawn: the selection is as it was                                                                                                                                                                                                                   | `"none"`                                   |
| `caret.shape`                                        | `"bar"` (before the character), `"block"` (over the character, which is drawn again on it; a space wide at the end of a line) or `"underline"`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | `"bar"`                                    |
| `caret.height`                                       | `"row"` (as tall as the row, as the current line's ground is) or `"text"` (as tall as the text)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | `"row"`                                    |
| `caret.width`                                        | how wide the bar is, or how thick the line under the character, points (0.5 to 8)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | 2                                          |
| `caret.color`, `caret.text`                          | the caret's colour, and the colour of the character under a block                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | cursor, background                         |
| `caret.radius`                                       | how round the caret's corners are, a fraction (0 to 0.5) of a row's height, held to half of the caret's width and height; 0, the default, is square                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `caret.glow`                                         | the soft light round the caret, in its colour: one number (0 to 3) for how far it reaches and how dark it is together, 1 a terminal's cursor's; 0, the default, none. Drawn as the caret's `shadow` (no offset), so a host that draws no soft shadow draws none                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `caret.blink`                                        | whether the caret blinks (`true`, the default); one that does not is lit, and no frame is asked for it                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `caret.blink_interval`                               | how long the caret is lit, and then dark, seconds (0.05 to 5, 0.5 by default; VS Code's `ViewCursors.BLINK_INTERVAL`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `caret.blink_for`                                    | how long after the caret last moved it goes on blinking, seconds (0 to 3600, 15 by default), lit and still after it so that a page at rest draws no frames; 0: it never stops                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `caret.unfocused`                                    | what the caret is while the window or the pane does not have the keyboard, where this editor has it in the page: `"hidden"` (the default), `"hollow"` (a block is an outline as thick as a bar, with its character as it is; a bar and a line are as they are with the keyboard) or `"solid"` (as with the keyboard); it does not blink. An editor the keyboard is not in draws no caret, whatever this says                                                                                                                                                                                                                                                                                                                                         |
| `caret.motion`                                       | how the caret goes from a place to another, on the host's clock: `"snap"` (at once, the default), `"ease"` (0.18 seconds, a terminal's, whatever the way), `"spring"` (a spring with no overshoot, stiffness 30, a terminal's, that keeps its speed where the caret is sent on, so that a key held down is one motion), `{ ease = seconds }` (0.05 to 1) or `{ spring = stiffness }` (5 to 100); with Reduce Motion on, `"snap"`. A caret glides where it went to another place of the document, the editor has as many carets as it had, and both places are in the room; it is at its place at once anywhere else (a room that scrolls to it, a layout that moved it, the pointer selecting, an input method composing). It is lit while it glides |
| `selection.radius`                                   | how round the corners of a selection are, a fraction (0 to 0.5) of a row's height; 0, the default, is square. Where the lines of a selection meet, the corners that turn inward are rounded too, as a terminal's selection is (`pencere_view::bands` is the rule, shared with a terminal's own); a fraction and not points, so that the corner grows with the text                                                                                                                                                                                                                                                                                                                                                                                   |
| `colors.mark_hover`                                  | what is laid over a gutter marker while the pointer is over it or it is pressed, where the marker says no colour of its own (the foreground at 30%)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `colors.current_line`                                | the ground of the main caret's line, while the editor has the keyboard                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | foreground at 5%                           |
| `colors.selection`, `colors.selection_blurred`       | the ground of what is selected, while the editor has the keyboard and while it has not                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | selection, selection at half               |
| `colors.found`                                       | the ground of what a search found                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | warning at 30%                             |
| `colors.matches`                                     | the ground of the other places of the main selection's text                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | selection at 40%                           |
| `colors.brackets`                                    | the ring around the bracket by the caret and the one that matches it                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | dim at 60%                                 |
| `colors.numbers`, `colors.current_number`            | the numbers of the lines, and the caret's line's                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | quiet, dim                                 |
| `colors.gutter`                                      | the ground of the gutter                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | what the editor is on, else the background |
| `colors.folds`, `colors.folded`                      | the buttons that fold and the `⋯` after a folded line, and the ground of that `⋯`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | dim, foreground at 8%                      |
| `colors.whitespace`                                  | the dots, arrows and boxes of the spaces and tabs shown                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              | border                                     |
| `colors.indent_guides`, `colors.active_indent_guide` | the guides of the indent, and the guide of the main caret's block (`false`: drawn as the others are); with the guides' colour `false` and the block's not, the block's guide alone is drawn                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | border, quiet                              |
| `colors.hint`                                        | what the editor says while it is empty                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | quiet                                      |

A colour is any colour of the page (a role's name, a value, `{ colour, alpha }`); `false` draws none
(`colors = { current_line = false }`, or `caret = { color = false }` for an editor with no caret). A
number out of its range is taken to its nearest end. An option an editor has no such name for is a
mistake, and so is a word it does not take: the page fails where it says it. The text is the box's
colour and font (`:color`, `:text_size`, `:family`), and what the page decorates (Section 2.3) is
drawn over all of it. The host blinks the caret, as the options of the editor that has the keyboard say (`caret.blink`, `caret.blink_interval`, `caret.blink_for`), and not with Reduce Motion on.

What is **not seen** is drawn where the page asks for it, and an editor that says nothing of it, one
for prose, shows none of it. A space shown is a dot in the middle of its place, a tab an arrow as
wide as it is (to its stop), and any other space (a no-break space, the spaces of typesetting, an
ideographic space) a box as wide as it is; a `\r` before a line break and the other controls show
nothing, and count as spaces for where a line's text ends. A line has a guide for each level of
`tab_width` columns its indent reaches into (a tab as far as its next stop, a level part filled
counted), and a line with no text those of the block it is in, as VS Code has them: the block the
line with text above it opens, or the one the line with text below it ends; none at the top or the
bottom of the document, nor in a run of more than 1 000 lines with no text. The main caret's block
is the one its line opens or ends, else the one its line's last guide is of, and goes on over the
lines that show that guide (VS Code's `highlightActiveIndentation`). A line that wraps has its
guides on its first row (the rows after it start at the left), and the lines a fold hides show
nothing. Both are drawn over the grounds (the caret's line, what is found and selected, the lines
and looks the page decorates) and under the text and the carets.

Its document lives as long
as an editor of its name is on the page, hidden or not: a page of tabs hides the editors of the
files not in front and keeps their documents. The language is the page's or its program's: the
editor colours, marks and folds what the page says (Section 2.3), comments and indents as its
options say, and knows no language itself.

A place in a document is a **character** (a code point) counted from 0, and a line is counted from
0; a range is from one place to another, the end not in it. A version is a number that grows by one
with every change of the document, whoever made it. The caret of an editor has a name of its own,
`pencere.caret_of(id)`: an overlay anchored to it stands by the caret (`:anchor(pencere.caret_of("sql"),
"below")`, a popup of completions, which goes above it where there is no room below), and
`pencere.layout.bounds` says where it is. A range of the text has a name too, where the page gives
it one (`{ from =, to =, anchor = "name" }`, Section 2.3): `pencere.range_of(id, name)` is what an overlay
stands beside as it stands beside the caret, and `pencere.layout.bounds` its box (from where the range
starts across, as wide as its first row's part of it is, from the top of that row to the bottom of its
last); the overlay is not shown while no part of the range is in the room, and is placed again when the
text, the range or the scroll moves it. A name belongs to its editor, an editor holds at most 64, and
of a name said twice the later is the one; a name the page took away is no box. A popup's keys (↑ ↓ ↩ ⇥ Esc) are commands of the page
written only while the popup is open (a command that is there and not `enabled` is a key for the
page, and so not the editor's).

A **fill from one colour to another** (`bg_gradient(angle, from, to)`) runs along a line at `angle`
degrees as a style sheet's `linear-gradient` does (`180` from the top down, `90` from the left to the
right), over the box's `bg`; a line of text on it is read against its colour half way along.

A **motion** says how a change of an element moves instead of jumping, as a style sheet's
`transition` and a motion library's layout animation do: `seconds` (0.2), `ease` (`"out"`, the
default, `"linear"`, `"in-out"`, `"spring"`), and which of `color`, `opacity`, `size` (a width or
height in points) and `place` (moved among the others, it slides there) follow a change; `enter` and
`exit` (`{ opacity, dx, dy, seconds }`) say how it comes in when it appears and how it goes when it is
gone. `spin` (seconds, more than 0) turns it round, clockwise, a turn in that many seconds, for as
long as it is on the page and with no id too: its picture or its canvas turns about its middle, its
box and its place stay, and the host draws a frame for each step (a spinner while something loads,
`image("loader.svg"):motion({ spin = 1 })`). The host does the moving, frame by frame, and does not ask the page for the frames in between;
the element is followed by its id, so one with no id does not move. Nothing moves under Reduce Motion,
on the frame the page changes size, or for what scrolling brings into view or takes out of it. A row
of a list is `motion({ place = true, enter = { dy = 6 }, exit = { dy = -3, seconds = 0.14 } })`; a
hover is `bg(hovered and "raised2" or nil):motion({ seconds = 0.12, color = true })`.

### 2.2 What a page remembers, and asks of the layout

`pencere.Ui`, `pencere.Fades`, `pencere.Rows` and `pencere.Built` keep what a page needs between frames
(pointer and keyboard state, eased levels, the heights of a long list and which of its rows are
built). A fade takes a highlight's 0.12 s, or its own time (`fades:to(id, level, seconds)`), and is read
along a curve where one is given (`fades:level(id, "out")`; `pencere.ease(t, ease)` is the curve). `pencere.layout` answers what is where on the page as it was last laid out: `hit`, `ids_at`,
`bounds` (a field's too, by its name), `scroll_extent` (`content` and `view` down, `content_w` and
`view_w` across, with `max_offset` and `max_offset_x`, and how far it is scrolled, `offset` and
`offset_x`), `focus_order`, `path_of`, `drag_source` and `drop_target` (the topmost element at a point
that can be dragged, and that what is dragged can be put down on). They are defined by the prelude.

The text of an editor is read at once, from what the host posted of it before it told the page of
the version it is: `pencere.layout.editor_version(id)` (the version read, the one the last `edit`
said), `editor_length(id)` and `editor_lines(id)`, `editor_text(id, from, to)` (at most 1 MiB a read;
a large document is read in pieces), `line_start(id, line)`, `line_of(id, at)` and `word_at(id, at)` (the word at a place, or the run of
marks or of spaces, as a double click takes it: its `from` and `to`; nil past the end of the text, so that a
page that asks for what is at its caret asks of the word the pointer rests on by the same rule). A page reads in
the `edit` event the text of the version the event is of.

**How the host's terminal draws** is told by the host and applied by the page. `pencere.terminal_look()`
says how the host's terminal draws its cursor and its selection, as the person set them, in the words
and units of an editor's options: `{ caret = { shape =, width = (points), radius =, glow =, blink =,
blink_interval = (seconds), unfocused =, motion = }, selection = { radius = } }`, each field only where
the host has such a setting (`nil` is the kit's own default for an editor, and a host that has no such
settings says none: `{}`). The colours are not in it (the theme's roles are the host's to resolve) nor
Reduce Motion (`render` has it). It is read as it is asked, so it is what the person has set now; the
page is told `{ type = "terminal_look" }` (Section 2.3) when it changes. **The host applies none of it to the
page's editors**: the page decides what they take, and an editor is as its options say, whatever the
host's terminal looks like. `pencere.draws()` says what the host draws that not every host does, as a
table of names that are true: `{ shadow = true }` where it draws a box's soft shadow (a caret's `glow`
is one), `{}` where it draws only what every host does; the same every time it is asked. A page that
wants its editors to look as the person's terminal does gives them all of it where it says nothing,
with `@pencere/terminal` (Section 2.6).

**Timers** run when the host wakes the page (a reply's `wake`, Section 2.3), and are named as the
program runner's. `pencere.time.after(seconds, fn)` runs `fn` once, `seconds` (0 or more) from now;
`pencere.time.every(seconds, fn)` every `seconds` (more than 0) from then on, once and not once for
each for runs it fell behind on. Both return a timer: `:cancel()` stops it (`true` where it was still
waiting) and `:active()` says whether it waits. `pencere.time.now()` is the time the host last gave
the page, `render`'s `now` or a wake's (0 before any); it does not go on between them.
`pencere.time.wall()` is the time of day, as the host's system says it: the seconds of Unix time
(since 1970-01-01 00:00:00 UTC, leap seconds not counted) and how far the person's local time is
ahead of UTC, seconds. It is read as it is asked and goes on within a call; it is what "4 minutes
ago" and "14:32" are told from, and no clock to time a page's work by (the person may set it). The kit asks
for the wakes its timers want with the page's replies and frames, and when the page is woken runs
those that are due, in the order of their times (and of their making), before `handle` is told of the
wake, which it is only where the page asked a `wake` itself. What `fn` returns is a reply, put
together with the other timers' and the page's: every ask of an editor, in order; a frame where any
wants one; of what says one thing (the keyboard, the pasteboard, a link) the last; the earliest wake.
A timer set while the page is woken counts from the wake's time, and one it sets runs at a later
wake, not at that one. A timer set anywhere else (an event, a message, `render`, the script's start)
counts from the next wake, which the kit asks for at once: the time the page last had may be that of
a frame long gone, so a timer is late by at most a frame and never early (one set in `settle`, which
has no reply, has the page rendered again, and its frame asks). An error in `fn` is the page's, with
its line. A page that sets none and asks no `wake` is asked and told nothing new.

**What the kit puts together, it holds to what a reply may hand the editors** (Section 4: 1 MiB of
text, 20 000 decorations): what the page's parts return (`syntax:reply`, the timers, the page's own
reply) is one reply, and the decorations that would take it past that wait for the page's next
reply, which the kit asks a wake for at once. They wait in their order, and with each of them every
decoration after it for the same editor, so that what a layer is given last is what it has; they
are made against a version, which the editor carries over what is done in between. Nothing else
waits: a load, a change, a selection or a command goes when the page asks it, and an ask too large
for a reply by itself goes as it is, for the host to refuse (Section 4: the page fails, as a
module's would). How replies are put together is `pencere.merge`'s (Section 2.3).

A long list builds only the rows near what is in view, and keeps the rest of its length with a spacer
above and one below. `box:list(rows, make)` does all of it: in a box with a name that scrolls, a
column with no padding above and no gap between its children, it puts a spacer, the rows near what
is in view (`make(i)` makes row i) and a spacer, and returns the box. The host moves what is built at
once and the page builds what comes into view a frame or two later, so two views are made ahead of
what is in view, the way the list goes, and half a view behind (as much both ways before it has
moved, all of it the other way at an end of the list, which can only go back), and the same rows
are made again while they hold what is in view with a view to spare the way the list goes. The kit
asks for the frame a list needs: a `scrolled` event that takes a list near the end of the rows made
draws the page again whatever `handle` answers, and so does a layout that gives a list a taller
window than its rows fill. A list that goes less than a view a frame finds its rows built, and the
page is built about once a view, not at every turn of the wheel. A list the page asked past its end
(a log that is followed) is built at the end of the rows it is given, though it grew since it was
laid out. Before the box is laid out its window is taken as the page's height; a list the page stops building is forgotten, and is built
afresh when it comes back. `pencere.list_window(id)` is the first and the last row the list made and
how tall its window is (what a page asks its program for, a window at a time, and how far a page of
rows goes). A box that cannot hold a long list (no name, no scroll, a row or a grid, padding above,
a gap) is refused by name when the page is made.

`pencere.Rows` is where a list's rows stand: `Rows.new(heights)`, `Rows.from_tops(tops)` (where each
row starts, from 0, the last entry where the last row ends: a table the page keeps as the list grows,
taken as it is) or `Rows.even(count, height)` (rows of one height, as long as any list),
`:padded(lead, trail)` for room around them; `:window(offset, view, overscan)` the rows that touch a
window, `:at(y)`, `:top(index)`, `:total()` and `:reveal(index, offset, view)` the offset that shows
a row. `pencere.Built` (`Built.new()`, `built:span(rows, offset, view)`, `built:stale(rows, offset,
view)`) is what `:list` decides with, for a page that builds a list its own way.

How far a box is scrolled is the **host's**, not the page's. A box that says `scroll()` and has an id
is moved by the wheel, the trackpad and its scroll bars as the host's system moves them (its speed,
direction and momentum; where the system does, a box pulled past its end springs back, and the bars
show as the system's do), at once and on the page as it was last laid out: the page is not asked.
The page is told where its boxes went (`scrolled`, Section 2.3) and builds what came into view.
`pencere.layout.scroll_offset(id)` is how far a box is scrolled (down, then across), as the page was
last told or asked; `pencere.layout.scroll_to(id, { y =, x =, animate = })` asks the host to scroll
it (a way not given stays where it is, past the end is the end, `animate` glides there unless the
person asked for less motion), done before the next frame is laid out. A box with no id does not
scroll.

A page reads what its `Ui` keeps as fields: `ui.hover`, the id of the pressable element the pointer
is over; `ui.pressed`, the one a button went down on and is held on (nil when none is); and
`ui.focus`, the element the keyboard is on (nil before any). It changes them through the methods:
`point(layout, at)` (the pointer moved, or left with `at` nil), `press` and `release` (a button went
down and came up: `release` gives the element clicked), `focus_on(id)`, `focus_step(layout, back)`
(to the next element that takes the keyboard, or the one before), and `key_path(layout)` is where a
key goes, the focused element and the named ones around it. Each that takes the pointer says
whether anything it keeps changed, so a page draws a frame only for what it sees.

`Ui` keeps a drag: `press` on an element that can be dragged begins one, `point` makes it a drag once
the pointer has moved `pencere.DRAG_START` (4) points and follows it, `dragging()` is
`{ source =, at =, target = }` while it goes (`target`, the drop target under the pointer, is never the
element dragged), `release` puts it down (it is no click) and `take_drop()` then gives
`{ source =, target =, at = }` once, and `cancel_drag()` gives it up (the escape key).

### 2.3 Events and replies

`handle` is given `{ type = ..., ... }` in the page's points:

| `type`             | Members                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `move`, `leave`    | `x`, `y` (`leave` has none)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `press`, `release` | `x`, `y`, `button` (`"left"`, `"right"` or `"middle"`), and for `press` `mods` and `count` (2 for the second of a double click, as the system counts them)                                                                                                                                                                                                                                                                                                                                                                             |
| `scroll`           | `x`, `y`, `dx`, `dy`: a wheel no box the host scrolls took, the points the content would move left and up by (for a page that does something of its own with the wheel)                                                                                                                                                                                                                                                                                                                                                                |
| `scrolled`         | `boxes`: the boxes the host scrolled, each `{ id =, x =, y = }`, how far it is scrolled now                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `key`              | `key` (`"up"`, `"down"`, `"left"`, `"right"`, `"page_up"`, `"page_down"`, `"home"`, `"end"`, `"enter"`, `"escape"`, `"tab"`, `"backspace"`, `"delete"`, `"f1"` to `"f20"`, or the character), `mods`                                                                                                                                                                                                                                                                                                                                   |
| `text`             | `field` (its name) and `text`: what a field holds now, after the person changed it (not after the page's own `fill`)                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `focus`            | `field`: the field the person clicked into, or nil when the keyboard went back to the page of itself (not said when it went where the page sent it)                                                                                                                                                                                                                                                                                                                                                                                    |
| `menu`             | `index` (from 1) of the entry picked from the menu the page asked for                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `key_up`           | `key` and `mods`, as `key`: a key the page was told of went up                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `mods`             | `mods`: the modifiers held changed, with no other key                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `command`          | `id`: the command asked for, from the host's list or by its keys (the key is not told as well)                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `files`            | `x`, `y`, `source` (`"drop"` on the page at `x`, `y`; `"paste"` at the caret; `"pick"` in the Open panel the page asked for, with none when the person turned it away), `files` (each `{ handle =, name =, size =, type =, path =, kind = }`, below) and `paths` (those the files have)                                                                                                                                                                                                                                                |
| `files_over`       | `x`, `y`, `count`: files dragged from outside the page are over it, for showing where they would go                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `files_left`       | (no field) they went from it without being dropped                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `saving`           | `handle` and `name`: the place the person chose in the Save panel the page asked for (neither: turned away)                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `edit`             | `editor`, `version` (what the document came to), `origin` (`"person"`, `"compose"`, `"page"`, `"undo"`, `"redo"`, `"load"`), `tag` (the page's name for a change it asked for with one), `changes` (`{ { from =, to =, insert = }, ... }`, in the places of the version before, in order)                                                                                                                                                                                                                                              |
| `selection`        | `editor`, `version`, `ranges` (`{ { anchor =, head = }, ... }`), `main` (from 1), `composing` (an input method is composing at the main caret; the host does none of the page's commands then)                                                                                                                                                                                                                                                                                                                                         |
| `viewport`         | `editor`, `first`, `last`: the lines in view, for a page that colours only what is seen                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `editor_refused`   | `editor`, `reason` (`"reversed"`, `"out-of-document"`, `"overlapping"`, `"stale-version"`, `"future-version"`, `"too-large"`, `"too-many-decorations"`, `"outside-range"`, `"bad-pattern"`, `"no-editor"`): what the page asked was not done, none of it                                                                                                                                                                                                                                                                               |
| `found`            | `editor`, `version`, `count` (at most 10 000), `current` (the match the main selection is exactly on, from 1; nil for none), `more` (there may be more than `count`): what its search finds (below)                                                                                                                                                                                                                                                                                                                                    |
| `wake`             | `now`: the time the page asked to be woken at (a reply's `wake`, below) has come, seconds on `render`'s clock; told only to a page that asked, after the timers that were due ran (Section 2.2)                                                                                                                                                                                                                                                                                                                                        |
| `terminal_look`    | (no field) what `pencere.terminal_look()` says is not what it was: the person changed a setting of the host's terminal that is its cursor's or its selection's. Told once for each change, and not for what was the same. The page reads it again and answers as it answers any event (a page that takes it for its editors answers `true`: the editor's options are asked again)                                                                                                                                                      |
| `gutter`           | `editor`, `column`, `line` (from 0), `kind` (`"press"`, `"enter"`, `"leave"`), `button`, `count`, `mods` (of a press), `marker` (`{ id =, layer =, from =, to = }`, nil where the cell is empty) and `cell` (`{ x =, y =, w =, h = }`, the page's points): the pointer in a gutter column that hears it (below)                                                                                                                                                                                                                        |
| `hover`            | `editor`, `kind` (`"dwell"`, `"leave"`), `at` (the place under the pointer), `from`, `to` (the word it is on, or the run of marks: a double click's), `line`, `hits` (`{ { layer =, id =, from =, to = }, … }`: the styles with an `id` that hold `at`), `word` (its box, the page's points) , `mods` and `version`: in an editor with `events = { "hover" }`, the pointer rested `hover.delay` over a word, or went from one it was told of                                                                                           |
| `target`           | `editor`, `kind` (`"enter"`, `"leave"`, `"press"`), `source` (`"word"`, `"link"`, `"widget"`), `at`, `from`, `to`, `id` (of a link), `word` (the box), `mods`, `button`, `count` (of a press) and `version`: in an editor with `events = { "target" }`, the pointer came to a word (or went from it) while exactly the modifiers of the press that is the page's (`gestures`: `"target"`) are held, at most once in `links.delay`, or that press: `source = "link"` where it is on a look that is `held`, has an `id` and is drawn now |
| `keymap_pending`   | `editor`, `keys` (the chord's keys so far, as written; none when it is not waited on any more)                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `editor_ready`     | `editor`, `version`: the host made the editor, as the page was laid out with it, shown or hidden; its text is posted before this is told, and what the page asks of it is done from now on. A page loads a document into an editor it has just made (a tab's that is not in front too) when it hears this                                                                                                                                                                                                                              |

`move`, `press` and `release` over an editor have `place`, `{ editor =, at =, inside =, version = }`
(`inside`: the place was in a selection or at a caret before a press moved the caret): the place of its
document under the pointer (a word to say what it is of while the pointer rests on it, a name to go to
at a ⌘-click). A press in an editor is the editor's as well: it puts the caret.

`mods` is `{ shift =, control =, alt =, command = }`. A reply is `nil` (nothing changed), `true` or
`false` (a frame is worth drawing or not), or a table: `redraw`, `close`, `copy` (text for the
pasteboard), `cursor` (`"arrow"`, `"pointer"`, `"text"`, `"resize_horizontal"`, `"resize_vertical"`,
`"not_allowed"`, `"grab"`, `"grabbing"` or `"crosshair"`), `keyboard` (`"page"` or `{ field = name }`),
`fill` (`{ [name] = text }`, or `{ [name] = { text =, select = } }`: what fields are to hold from now
on, and what is selected in them; `select` is `{ from =, to = }`, characters from 0 in either order,
past the end the end, or `true` for all of it; a field given no `text` keeps what it holds, and one
given no `select` has its caret where its system puts it after a text, at the end; a name that is
not a field of the page is passed over, below), `menu = { at = { x =, y = }, entries = { { title =, keys =,
enabled = }, "-", { title =, keys =, enabled =, paste = editor } } }` (an entry's `keys` are written as a
command's, `"ctrl+w"` or `"f2"`, and shown at the end of its line; `key`, the name it had, is read the
same; an entry with `paste` is one the host performs itself when it is picked, below),
`link` (an address to open in the person's browser, below), `wake` (in how many seconds to wake the
page, below), and `editor = { [name] = ask, or { ask, ... } }`, what the page asks of its editors,
each ask one of:

| Ask                                                                                  | What                                                                                                                                                                                                                                                                                                                                                                  |
| ------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `{ load = text, more = true }`                                                       | A piece of the document: the first in place of the whole text, each after it (`more` said before) on its end. Nothing of it is undone, and what could be undone before cannot be.                                                                                                                                                                                     |
| `{ change = { { from =, to =, insert = }, ... }, base = version, history =, tag = }` | Changes against the version the page saw: carried over what was done since (a range's start after what came in at it, its end before), done, undone as one; where `history` is `false`, never undone, and what can be undone or done again is carried over them as over any change after it (CodeMirror's `addToHistory: false`). `tag` is told back in their `edit`. |
| `{ select = { { anchor =, head = }, ... }, version =, main = }`                      | The selections, in the places of `version`.                                                                                                                                                                                                                                                                                                                           |
| `{ reveal = place, version =, align = }`                                             | The place scrolled into view: as far as it takes, and not where it shows (`"nearest"`, the default), or its row at the top of the room (`"start"`), in its middle (`"center"`) or at its bottom (`"end"`).                                                                                                                                                            |
| `{ command = name, ... }`                                                            | What the person could do with a key: `"move"` (`movement`, `extend`), `"insert"` (`text`), `"undo"`, `"find"` and the commands of a search, those of whole lines (below), … (`pencere_view::edit::EditCommand`).                                                                                                                                                      |
| `{ decorate = layer, version =, from =, to =, decorations = { ... } }`               | The layer's decorations for `from`–`to`, in place of those it had there; the other layers stay.                                                                                                                                                                                                                                                                       |

A layer is named by the page (`"syntax"`, `"diagnostics"`, `"folds"`): the colours of what is in view
are sent without taking the errors away. A decoration is `{ from =, to =, color =, background =,
weight =, italic =, strike =, underline = colour or { wavy = colour } }` (a look), `{ from =, to =,
line = colour }` (a ground for each line it touches; a look may say an `id`, which the `hits` of a rest
over it tell), `{ from =, to =, gutter = colour, … }` (a
marker in a column of the gutter, below), `{ from =, to =, ruler = colour, lane = }` (a mark on the
overview ruler, below), `{ from =, to =, zone = name, height = points, side = "below" | "above" }` (a gap for the page, below),
`{ from =, to =, anchor = name }` (a name for the range: `pencere.range_of`, Section 2.1),
`{ from =, to =, widget = words, side = … }` (text that is not the document's, below) or
`{ from =, to =, fold = true }` (a region that folds: its first line stays; whether it is folded is the
person's). Decorations of an older version are carried over the changes made
since. What the editor cannot do is refused and the page told why (`editor_refused`): a range the wrong
way round, a place past the end, changes that overlap, a version older than the last 1 024 or yet to
come, a document larger than an editor may hold.

**The gutter** is a row of named columns at the left of an editor's text. The page lists them in the
order it wants (`gutters = { "numbers", { name = "breakpoints", width = 14 }, "marks", "folds" }`);
the three the editor has (`"marks"`, the column of dots, `"numbers"` and `"folds"`) are there in that
order where it says none, and one the page leaves out is not there (no `"folds"`, no buttons that
fold). The numbers keep to their left the editor's padding where they are the first column, so that
they do not stand against the edge of the room (`pad = points` says another; none after another
column). A column of its own has a `width` (points, as wide as the dots' where none is said) and says
what of the pointer the page hears of it: `events = { "press", "menu", "hover" }` (a column of the
page's hears the first two by default, `false` or `{}` nothing). A press of the main button in a
column that hears it is the page's alone, `gutter` with `kind = "press"`: the line is not selected,
nothing folds, the caret does not move (the editor has the keyboard); a press of another button, or
of the main one with ⌃, is `menu`. That is so where the cell is empty as well (a page puts a
breakpoint there). The numbers are as they were: a press selects the line, and the buttons that fold
fold. `hover = { shape =, color = }` shows a faint marker in the cell the pointer is over where
nothing stands yet (VS Code's faded red dot for a breakpoint to be put); `events` with `"hover"` tells
the page of the pointer's coming to a cell and going from it (`kind = "enter"`, `"leave"`) and no
other hover is told: how a marker looks while the pointer is over it and the preview are the editor's
own, so that the page hears the rare thing and not each move.

A **marker** is a decoration `{ from =, to =, gutter = colour, column = "marks", shape =, id =, label
=, cursor =, hover =, pressed = }`: the range says where (a `from` that is a `to` is a place) and
the layer, the 20 000 and the carrying over of the page's decorations are as any decoration's.
`shape` is `"dot"` (centred on the range's first line: what a gutter mark was drawn as before it had
a shape), `"bar"` (a thin bar, `marks.bar` wide, along every line the range touches, the rows a long
line wraps into too; a range that ends where a line starts stops at the line before; `marks.hover`
wide while the pointer is over it, in a column that hears the pointer), `"above"` and `"below"` (a
small triangle, three bars tall, on the edge above or below the range's first line: the place lines
were taken away from, a `from == to` at the start of a line) and `"image"` (`image = "assets/x.svg"`,
a picture of the view's bundle, tinted with the colour, as tall as the row); a marker whose column the
editor has not is not drawn. A marker in lines that are folded away is drawn on the line the fold is on, and
presses there are its. `id` names it in the page's events, `label` is what a screen reader says of it
and what the host says while the pointer rests on it (the keys that do the same in brackets: the
page writes them), `cursor` is the pointer's shape over it, and `hover` and `pressed` its colour then
(`colors.mark_hover` laid over it where it says none). The pointer is the I-beam over the text, the
arrow over the gutter and the hand over what answers a press there (a button that folds, a column
that hears presses, a marker with a `cursor` of its own), in every host; a page that asks for another
of the shapes that mean something it does itself (a divider's, a drag's) has it.

The **overview ruler** is a strip down the right edge of the room, as tall as the room, whatever
the style of the scroll bars (they come and go, the ruler stays); `{ from =, to =, ruler = colour,
lane = }` puts a mark there where the range's lines are in the document, at least 2 points tall, in
`"full"` of its width (`marks.ruler`, 4 points) or a third of it, `"left"`, `"center"` or `"right"`
(VS Code's lanes). The scroll bar's knob is drawn over it.

A **zone** is a gap an editor keeps open after a line for the page to put something in (VS Code's zone
widget, CodeMirror's block widget): `{ from =, to =, zone = name, height = points }` after the line the
range starts on, `height` points tall exactly (at most 200 rows' worth). The text after it goes down by
it and the content grows; the numbers, the scrolling and the ruler count it (a mark of the ruler is the
rows of its own lines, not the gap after). What stands in it is
an element of the page, a child of the editor's element that says it is the zone's,
`element:zone(name)`: it is laid out as a box as wide as the room and as tall as the page says (the
same as the zone's `height`), put where the gap is, and goes up and down with the text as it
scrolls (not across, with a long line: it stays where the room starts): it is cut by the room, answers the pointer and the keyboard as any element, and is
in the outline. It is on the page only while the editor has the gap (a zone is carried over the
changes as its decoration is, and goes with it).

**Links.** A look may say `held = "command"` (modifiers joined by `+`: `command`, `control`, `alt`,
`shift`, and `mod`, the host's main modifier: ⌘ on a Mac, Ctrl elsewhere), `over = true`
and `cursor`: it is drawn only while exactly those modifiers (and no other) are held, as a press of
`gestures` is taken by exactly its modifiers, and, with `over`, the pointer is over its range, and gives the pointer's shape then (the
hand, where it is `held` and says none). The editor does that itself, so that holding ⌘ and moving over
words draw and take back links with no event of the pointer's moving: the page writes a link when it is
told the pointer came to a word (`target`, `kind = "enter"`: with the `version` the event has, as any
late answer is written) and takes it back at `"leave"`; its `id` names the link in the `target` of a press
on it. A word is told of once however the pointer moves in it, and at most once in `links.delay` of
all of them. `gestures` makes a press the page's (`"target"`); the same table puts a caret where ⌥ is
held, drops a press, or takes ⌘ for the link where the editor takes it for a caret (VS Code's
`multiCursorModifier`: both are rows of one table, so that the same keys cannot be both). A page that
holds the modifier and moves the pointer is the host's to tell of the modifiers: `Input::Mods` before the
move, as the host knows them (the drive takes the modifiers of a key and a press as well, and a window
that does not have the keyboard resets them).

**Widgets** are text the page puts in lines that is not the document's (an inlay hint, a line's end
error, a blame): `{ from =, to =, widget = "words" | { "words", { "more", color =, id = } }, side = "before" |
"after" | "end", group =, id =, label =, cursor =, anchor =, color =, background =, pad_left =, pad_right =, margin_left =, margin_right = }`.
`"before"` stands at `from` before the character there (`name: |arg`), `"after"` at `to` after the one before
it (`x|: int`), the caret between the two; either takes room in the line (the rows, the widths, wrapping, the
places the pointer is over and the selection's ends go through it), and the text, the caret's movements,
copying, searching and the history do not know it: a click on it puts the caret at the nearest place, and
a selection that ends at it does not take it. `"end"` stands after the line's last row, `gap` points
beyond the text, takes no room and is cut at the room's edge with "…". It is drawn small (`size`) on a ground
of its group's, in `color` (the part's own where it says one), with the room inside its box (`pad_left`, `pad_right`) and outside it (`margin_left`, `margin_right`: the group's `margin` where it says none; an inline widget only) kept before and after. A widget in a group that is not shown is
not there. A widget's `label` is what a screen reader and the host's tips say of it, as a marker's. A widget (or
a part) with an `id` is the hand's, is what a rest is told of (`hover`, `over` =
`"widget"`, `id`) and what a press is, whatever modifiers are held, in an editor that has `events = {
"target" }` (`target`, `source = "widget"`, `id`, no caret moves); `anchor` names its box, as a range's.
At most 8 parts, 200 characters and 32 widgets to a line; a widget over them is a reply the host
refuses. Its measure is kept with the line's, and is made again only when what it says or how wide it
is changes.

**The editor's own keys** (what `keymap` changes; `pencere.layout.keymap(editor)` lists them with the
page's changes, each `{ keys =, does = }` with `does` as `keymap` takes it, nil where the page took the key
away): ←, →, ↑, ↓ move (with ⇧ they extend, with ⌥ ← → by words, with ⌘ ← → to the line's ends and ⌘↑ ⌘↓ to the
document's, `page_up`, `page_down`, `home`, `end` as written, `ctrl+a` and `ctrl+e` the line's ends);
`backspace` and `delete` (`alt` by words, `cmd+backspace` to the line's start, `ctrl+k` to its end);
`enter` is `newline`, `tab` is `tab` and `shift+tab` `outdent`, `escape` is `escape` (one selection);
`cmd+z`, `cmd+shift+z` are `undo`, `redo`; `cmd+a` `select_all`; `cmd+d` `select_next`; `cmd+/`
`toggle_comment`; `cmd+]`, `cmd+[` `indent`, `outdent`; `cmd+alt+]`, `cmd+alt+[` `unfold`, `fold`;
`cmd+g`, `cmd+shift+g` `find_next`, `find_previous`; `cmd+c`, `cmd+x`, `cmd+v` `copy`, `cut`, `paste`;
`alt+up`, `alt+down` `move_lines_up`, `move_lines_down`; `alt+shift+up`, `alt+shift+down`
`duplicate_lines`; `cmd+alt+up`, `cmd+alt+down` `add_caret_above`, `add_caret_below`. A key that is a
command of the page (enabled) and the first key of a chord of the editor's `keymap` is a page that
fails where it wrote them (a key of the keymap that is one alone is the page's command's, as the
editor's own keys are).

The pointer's **rest** is told to a page that asks (`events = { "hover" }`): when it stays `hover.delay`
(0.3 s) over a word (the run a double click takes; not over spaces, the gutter, a gap, or past the end of
a line) the page is told `hover` with `kind = "dwell"` and the word's range and box, and the `hits`: the
styles with an `id` that hold the place under it (a diagnostic, a link), so that the page does not search
its own lists. It is told once however the pointer moves within the word; when the pointer goes from a
word it was told of the page is told `kind = "leave"` at once (what a card does then, how long it waits
and whether the pointer came onto it is the page's: `@pencere/ui`'s `popover` does it, below). A scroll,
a key, a press, an edit, a window that no longer has the keyboard and a button held (a drag) end a rest, tell
the `leave` where there was a `dwell`, and tell nothing more until the pointer moves again. An answer
that comes late is written with the `version` the event carries: the editor carries it over what was
typed meanwhile as it does any decoration. The page reads the word at its own caret by the same rule
(`pencere.layout.word_at`), so that a command that does what the pointer's rest does (Show Hover) answers
as the rest does.

A zone with `side = "above"` is the gap before its line (a code lens: the row of what a function
has, over it), between the line before it that shows and the line, or before the first row where the
line is the first; the rows of the line before it do not take it, and it is the page's element as any
zone's is, made for the lines in view (`viewport`). A line folded away has none to show, and the first
line of a fold keeps its own. An editor does not hold an editor in a zone: peeking at another file's
definition is a `popover` that holds a read-only editor element.

A change kept out of the history is how a page puts in what came from elsewhere (a file changed on
disk, a reformat) and keeps what the person can undo. It sends what changed and not the whole text:
what is undone after it is carried over it, and a group whose text the change replaced whole would
be carried to an end of it. A `load` is for a document the page begins: nothing before it can be
undone. An editor tells `editor_ready` once for each document it holds: a page whose tree names an
editor for the first time asks of it when it hears it, as an ask made before is refused
(`no-editor`).

The commands of whole lines and of what is by a caret are those of CodeMirror's and VS Code's
standard keys, each a `{ command = name }` and none with a key of the editor's own (a page gives
them its keys, as its commands): `"fold_all"` (every region that folds and is in none that does),
`"delete_lines"` (the lines of each range, the caret on the line that comes in their place as far
along it as it was), `"select_line"` (the lines of each range, whole; a range that is whole lines
takes the next too), `"move_lines_up"`, `"move_lines_down"`, `"duplicate_lines"` (below them, the
ranges on the copy) and `"duplicate_lines_up"` (above them, the ranges on the copy), `"join_lines"`
(each line of a range with the next, a caret's with the one after it: the later line's indent taken
away and a space between them where neither side has one or is empty, the caret where they met),
`"insert_blank_line"` (a line after a caret's, indented as a line break at its end would indent it,
the caret on it), `"split_line"` (a line break at each caret, the caret before it),
`"transpose_chars"` (the characters either side of a caret swapped, the two before it at the end of
a line, the caret after them), `"delete_to_line_start"` and `"delete_to_line_end"` (to an end of the
caret's line, or the break there where it is at it).

An editor **finds** what the page asks it to: `{ command = "find", pattern =, regex =, case =, word
= }` looks for `pattern` through the whole document (a regular expression, Section 1, where `regex`, in either
case where `case` is `false`, and where `word` only a whole word: no letter or digit, of any script,
and no `_` just before the match or just after it), marks what it finds, at most 10 000 matches, and
selects the first at the main selection or after it. The search goes on as the document changes: the
editor looks again, once a frame, before it draws. The page is told what it finds (`found`) after
every `find`, and again whenever the count, or the match the main selection is on, changes;
`"stop_finding"` ends the search and tells a `count` of 0. An editor that is never asked to find
tells none. `"find_next"` and `"find_previous"` select the next match from the main caret, or the
one before. `{ command = "replace", text = }` puts `text` in place of the main selection where it is
a match and selects the next match, and only selects the next match where it is not; `{ command =
"replace_all", text = }` puts it in place of every match in the document, not only the 10 000 a
search marks, as one change undone as one (a search for whole words that runs out of what it may
read again, a pattern that matches inside words again and again, stops short, and says `more`). A
regular expression's `text` takes what the match's groups hold: `$1` or `${1}` by number, `$name` or
`${name}` by name, `$$` a `$` (a name is the longest run of letters, digits and `_`: `${1}x`, not
`$1x`); a plain search's goes in as it is. `"select_found"` selects every match, the main one the
match the main selection was on (else the one `find` would select); `"select_occurrences"` selects
every place the main selection's text is, in the case written, or, where the main selection is a
caret, every place the word it is in is (the word `"select_next"` takes: a word by Unicode's rules,
cut where a mark stands inside it), as a whole word; the search stays as it was. A document the
person cannot change (`read_only`) is not replaced in. `"copy"` and `"cut"` put what is selected on the
host's pasteboard as the editor's copy and cut keys do (a caret copies and cuts its line; a `read_only` editor
copies and does not cut). **There is no command that pastes**: a page can write the pasteboard (`copy`) but
never read it, so text reaches an editor from the pasteboard only on the person's key or the person's pick of a
menu entry the host performs itself (`paste = editor` in a menu's entry): the host reads its pasteboard and the
text goes in as the paste key's does, in a `read_only` editor not at all and at several carets a line each
where the text has as many lines; the page is told the pick (`menu`, the `index`) as any and never the text
(an `edit` that tells what went in is told, as for the person's own key). The line's words are the kit's
("Paste"), whatever the page wrote, and it pastes only into the editor that had the keyboard when the page
asked for the menu, and only a line that is on: a page cannot send what is on the pasteboard to an editor
the person is not in, nor give the pick another name.
A page asks for a paste by a command, `{ command = "paste" }`, and is failed where it writes it, as for any
word that is no command.

**A press that opens a menu.** `gestures` with `"context"` (`["right+press"] = "context"`) puts the caret
where a text view puts it for a press that opens a menu (VS Code's rule) and has the editor take the
keyboard: outside every selection it puts one caret at the place; inside one (at its ends too, and at a
caret) the selections stay. The press is then told the page as any (`press` with its `button`), with
the `place` it has: `{ editor, at, inside, version }`, where `inside` says the place was in a selection or at
a caret before the press moved the caret, and `version` is the editor's after; the page opens its menu from
that `press` (`menu` in its reply, at the press's `x` and `y`), and asks `copy`, `cut` and its other
commands of the editor when the entry is picked (those carry no version; an ask that does takes the
editor's current one, `pencere.layout.editor_version`, not the press's). A second press of the quick run
opens the menu as the first does. No `events` word is needed: the gesture is
the page's asking. A host whose system makes ⌃ with the main button a right press (a Mac's) may send it
as the main button with ⌃: the drive takes it for the right press where the page's `gestures` say a
right press does what opens a menu and say nothing of ⌃ with the main button, and tells the page the
press as the right button's, without the ⌃ (so does its release).

A reply's **`fill`** and its **`keyboard = { field = name }`** may name a field that the very page the
reply comes with brings: a reply that opens a field (a find bar, a palette, a dialog) fills it and
sends the keyboard to it in the same reply, as an element is focused when it comes. A field is the
host's only once a frame has laid it out (Section 4), so what a reply asks of a field the host does
not have yet waits for the next frame that lays the page out, and is done once the host has that
frame's fields: in the order it was asked (a reply's texts, then its keyboard), and what is asked
after it waits behind it, though it names a field the host has. A name that frame still does not
have is passed over, as any name that is no field of the page is: its text is dropped and the
keyboard goes to the page. A frame that is not drawn (the view is hidden) leaves it waiting.

A reply's **`link`** asks the host to open an address in the person's browser, by the rule of a web
page's `link` (protocol, Sections 9.7 and 9.8): only an `http` or `https` address, every byte of it
printable ASCII without the space, at most 8192 bytes. The host SHOULD ask the person first (as
Section 9.7 of the protocol says), opens it on
the person's machine, and never tells the page what came of it. Any other address is **dropped**, as a
page's is: the page is not closed for it and not told, and the rest of the reply stands. A `link` that
is not a string is an error of the script. The link is the host's, not a message to the program. It
is not a span's `link(id)` either, which names a part of a text for the pointer: a page opens what
such a part stands for by answering its press with `{ link = url }`. (`wasm`: the `link` of `reply`
in the interface, `Reply::with_link` in the Rust SDK.)

The **files the person gives** a page (`lua` `2.1`, `wasm` `3.0`; protocol, Section 8.9) come as
`files`, each with the `handle` the host made of it (32 lowercase hex digits; none for a `kind` of
`"folder"`, which is not read), its `name`, its `size` in bytes, its media `type` (the host's guess)
and, only where the host knows the program reads the same files as the person, its `path`. The name
is the person's and the type a guess: words, never a path to join to another. A page draws such a
file as `image("file:" .. handle)` and passes the handle on to its program, which reads the file by
it; the handle is nothing once the view has ended.

A reply's **`pick`** (`true`, or `{ multiple = true }`) asks the host for the Open panel, and its
**`save`** (the name to propose, or `{ name =, type = }`) for the Save panel: what the person chooses
comes as `files` with `source` `"pick"`, or as `saving`. A reply asks for one panel; where replies are
merged (`pencere.merge`, the timers) the last one's stands. **A panel is shown only for a reply to
something the person did**: a press or its release (a button acts on the release), a key, a pick
in a menu, a command. Asked for in a reply to
anything else (the pointer moving, a wake, a message of the program, the room to send), it is passed
over, so that a page cannot open panel after panel by itself; and the host shows one panel at a time,
dropping one asked for while another is up. (`wasm`: the `panel` of `reply` and the events `files`,
`files-over`, `files-left` and `saving` in the interface; `Reply::panel`, `Panel` and `FileEntry` in
the Rust SDK.)

A reply's **`wake`** asks the host to wake the page that many seconds from now (0 or more; 0 for the
next frame). Once the time has come the host tells `handle` `{ type = "wake", now = }`, `now` the
time of the frame it is woken at on `render`'s clock, and the page answers as it answers any event:
the work a page does over time (a long document read a piece at a time, a save once the person stops
typing, a search that waits for the typing to stop) goes on with no event of the person's, and what
it finds reaches the editors, the keyboard or the pasteboard with the wake's reply. Of the times a
page asks, in its replies and (for a timer set as it is drawn) its frames, the earliest stands, so
that what one part of a page asks is not put off by another; a wake that came is gone, and a page
that wants another asks again. The host wakes a page at most once a frame: a page that asks
`wake = 0` each time it is woken is woken once a frame, as one that is `animating` is drawn. A
`wake` that is not a number of seconds, 0 or more, is an error of the script. It is not `render`'s
`wake_at`, which draws a page at rest again at a time: a wake calls `handle`, and its reply can ask
what a frame cannot. In Lua the kit's timers ask it for the page (Section 2.2). (`wasm`: the `wake`
of `reply` and of `frame` and the event `wake` in the interface, a value that is not a number of
seconds a reply or a frame that is not one; `Reply::with_wake`, `Frame::wake` and `Event::Wake` in
the Rust SDK.)

**Replies put together.** A page made of parts that answer apart (its timers, a module of the
library, its own `handle`) answers with one reply: `pencere.merge(first, second, …)` is that reply,
the replies (each nil, `true`, `false` or a table) put together in the order given and none of them
changed. It asks everything they ask: the asks of an editor, of each editor in the order of the
replies; the texts of `fill`, by name, the later's where two name one field; a frame where any
wants one (`true`, `redraw`) and the page closed where any asks; of what says one thing (`keyboard`,
`copy`, `cursor`, `menu`, `link`) the last that says it; and the earliest `wake`. Of replies none of
which is a table, it is `true` where any is `true`, nil where every one is nil, and `false`
otherwise. This is how the kit puts together what its timers and the page answer (Section 2.2).

### 2.4 The program

What the page says to its program is the six messages of the protocol (Section 9.2):
`pencere.call(method, params, done, progress)` asks and is answered once, `pencere.subscribe(topic,
params, on_value)` asks to hear a topic, `pencere.cancel(id)` gives either up, `pencere.json` encodes
and decodes JSON (arrays and objects nested up to 127 levels, the outermost counting as the first: protocol, Section 4.5).
A member that is `null` holds nothing in what is decoded, as one that is not there does, so that
`if changes.git then` reads both alike; where they mean different things, `pencere.json.has(object,
key)` says whether the object had the key (`null` too) and `pencere.json.is_null(object, key)`
whether it had it as `null`, for every object decoded, those of the program's messages included,
while the page has set nothing at that key. What is decoded is encoded again as it was: its `null`
members, and an empty list as a list. A message the program cannot take yet waits its turn (`pencere.send`, the raw word,
says whether it took the message), and the host holds the page to the limits of Section 5.

### 2.5 The other scripts of the bundle

The entry of the bundle is the script, and it may split itself over the other files of its bundle
whose media type is `text/x-lua` with `require`, by Luau's own rules for requiring by string, so
that the editors and the language server of Luau read the paths as they are meant:

- A path starts with `./` or `../` and is relative to the requiring script's own path in the bundle
  (`require("./screens/logs")` in `app/main.lua` is `app/screens/logs`; the script's path is the
  `path` its manifest names). It has no extension: `name` is the script `name.luau` or `name.lua`,
  or the script `name/init.luau` or `name/init.lua`, and it is one of them, or an error. An `init`
  script is the module of its directory; within it `./` is relative to the directory's parent, and
  `@self/` is the directory itself (`require("@self/child")`).
- The bundle is the whole world, with the kit's library beside it (Section 2.6). A path that goes
  out of the root of the bundle with `../`, an absolute path, a path with no `./` or `../`, an alias
  other than `@self` and `@pencere` (there is no configuration file to define one), a file that is
  in the bundle but is not typed `text/x-lua`, and a name the bundle has nothing at are all errors,
  raised at the line of the `require`, that say the path. `require` reads no file of the system.
- A script runs once per page, in a thread with globals of its own like the entry's, when it is
  first required, and what it returns (exactly one value) is what every `require` of it returns.
  A script that requires itself through others is an error that names the chain
  (`require cycle: a.lua -> b.lua -> a.lua`). The entry is not a module: it cannot be required.
- An error in a required script, a syntax error included, says the script's own path and line
  (`screens/logs.lua:12: ...`); the entry's say its path as well.
- The text of the scripts a page loads, the entry's too, is held to the size of a script together
  (the floor of Section 4: 4 MiB): a `require` past it is an error that says so.

### 2.6 The kit's library

`require("@pencere/<name>")` is a module of the **kit's library**: Luau that the kit writes on this
page API alone (Sections 2.1 to 2.5, and nothing of the interpreter's besides), at the kit's
version, for what every page would otherwise write again. A host that runs the kit's interpreter
MUST give a script the library of its own kit's version; the kit's sandbox does
(`pencere_sandbox::library`). A page needs nothing else to use it, and MAY write its own instead.

- The alias is `@pencere`, in any letter case, as Luau's aliases are. A module of the library
  requires another with `@pencere/`, `./` or `@self/`, as the scripts of a bundle require one
  another, and reaches no script of the bundle: a path past the library's root is an error. A
  script of the bundle reaches the library through the alias alone, as no path of a bundle has an
  `@` in it.
- A module made of several files has the others as its parts (`@pencere/languages/sql`): they can
  be required, but they are not the library's interface, and what they are may change with any
  version of the kit.
- A name the library has nothing at is an error, raised at the line of the `require`, that says so
  (`the kit's library has no module "x"`). A module runs once in a page, as a script of the bundle
  does. An error at a line of it says the module and the line (`@pencere/syntax:12: …`); an error it
  raises for what its caller gave it says the caller's line.
- The modules do not count toward the size of a script (Section 4: 4 MiB). The kit holds them to
  1 MiB together.
- The kit's parts are in four layers: the host, the prelude, the library and the app
  ([`README.md`](README.md), "Where a part of the kit goes"). What a layer uses is below it: a
  module of the library is what an app could have written on the page API, and requires no script
  of an app; the prelude requires no module of the library.

#### `@pencere/syntax`

A language that colours an editor is the page's, and `@pencere/syntax` makes one the way
CodeMirror's `StreamLanguage` does. `Syntax.define { name =, start =, token =, copy =, equal =,
blank =, styles = }` (with `local Syntax = require("@pencere/syntax")`) reads a line a token at a
time: `token(stream, state)` takes at least a character from the stream and says the name of its
style, or nil, and the state carries what a line leaves open to the next (`start()` is the state
before the first line; `copy` and `equal` copy and compare two, by default tables all the way down;
`blank(state)` is told of a line with nothing on it). `styles` gives a name a look, a decoration's
(`color`, `background`, `weight`, `italic`, `underline`, `strike`): a name may be several apart
(`"keyword strong"`, the later's look over the earlier's), a dotted one falls back to what is before
its last dot (`"string.special"` to `"string"`), and one no style says is not coloured. The stream is
the line, its `\r` left out (`string`), and where it is (`pos`, and `start` where the token began,
bytes from 1): `sol()`, `eol()`, `peek()` and `next()` (a character is a whole UTF-8 sequence),
`eat(match)` and `eat_while(match)` (a character is itself, a longer string a Lua pattern the
character matches, a function says yes or no), `eat_space()`, `skip_to_end()`, `skip_to(text)`,
`backup(count)` (characters), `column()` and `indentation()` (a tab to its next stop),
`match(pattern, consume, case_insensitive)` (a Lua pattern read where the stream is: the text matched
or its captures, taken unless `consume` is false) and `current()`. A `token` that takes nothing fails
the page, as any error of the language's does, in words that say the language's name and the line
(`sql: line 12: …`). `language:tokens(text, state, tab_width)` is a line's tokens in characters, for
a text that is not in an editor (`state` the state before it, `start()`'s where nil; `tab_width` what
a tab is to the stream's columns, 4), and `language:look(name)` the look its styles give a token's
name (a copy; nil for none), found as the syntax finds it.

`Syntax.new(language, id, { layer =, step =, margin =, tab_width = })` colours the editor `id` with
it. It keeps the state before every 32nd line of the document (a checkpoint) and before each line of
the window it colours, and no other, so that what it holds grows with a document 32 lines at a time;
a line whose state it does not keep is read from the checkpoint before it. It is given every event
of the page (`syntax:handle(event)`): an `edit` makes it read again from the first line the change
touched (from the checkpoint before it, where that line's state is not kept) until a line whose
state is kept ends in the state it ended in before, a `viewport` says what to colour, and for those
two it does a call's work at once; any other event (the pointer moving, a key) costs it nothing, what is
left being its timer's. It colours the lines in view and `margin` lines on each side (as many as
are in view) in the layer `layer` (`"syntax"`), in characters, a line again only where it or the
state before it changed, and takes away what it coloured out of them. One call reads at most `step`
bytes (20 000), the lines to colour first, and a line to its first 10 000 characters (the state
after it is the state there), and it holds at most 10 000 decorations for the page, half of what a
reply may hand the editors (a line that would take it past that waits for the next call). Where the
lines in view are further on than a call reads to, they are coloured from the state the language
starts in, and the calls after read on to them and colour again what that got wrong. What it asks
of the editor goes in the page's reply (`syntax:reply(reply)`, its asks before the page's own; or
`syntax:asks()`). While there is more to do (`syntax:busy()`) it goes on by itself, a call a frame:
a timer of its own (`pencere.time.after(0, …)`) wakes the page, does the next call and hands what it
found to the editor with the wake's reply, so that a large document is coloured to its end with no
event of the person's (`syntax:work()` does a call at once, for a page that wants one).

#### `@pencere/languages`

The languages an editor reads are the page's, and `@pencere/languages` (with
`local languages = require("@pencere/languages")`) knows the kit's as CodeMirror's
`@codemirror/language-data` does: a **description** of each says what it is called and how a file in
it is told, and costs nothing to keep; the **language** is built from one only when a page loads it.
A description is a frozen table: `name`; `aliases`, its other names; `extensions`, what a file's name
ends in after its last dot; `filenames`, whole names of files in it (`"Makefile"`, `".bashrc"`);
`patterns`, Lua patterns a file's whole name in lower case is in it by (`"^dockerfile[%.%-]"`);
`interpreters`, the programs a `#!` first line runs a script in it with, without their version
(`"python"`); `first_line`, a Lua pattern the first line of a file in it matches (`"^%s*<%?xml"`), or
nil; and `family`, the module of readers it is built with (`"clike"`, `"scripting"`, `"markup"`,
`"data"`, `"sheets"`, `"tools"` or `"sql"`). Names, aliases, extensions and file names are matched in
any letter case.

The kit's languages are C, C#, C++, CSS, Diff, Dockerfile, Go, HTML, INI, Java, JavaScript, JSON, JSX,
Kotlin, Less, Lua, Makefile, Markdown, PHP, PostgreSQL, Python, Ruby, Rust, SCSS, Shell, SQL, SQLite,
Swift, TOML, TSX, TypeScript, XML and YAML. None is plain text: that is the page's word for a file in
none of them.

- `languages.list()` is every description, sorted by name in any case, a new list each call.
- `languages.find(name)` is the description whose name or one of whose aliases is `name`, in any
  case, or nil.
- `languages.detect(path, first_line)` is the description of the file at `path` (its whole name,
  after the last `/` or `\`; nil for a document with no name) whose first line is `first_line` (or
  nil), by its whole name (`filenames`), then `patterns`, then its extension, then its first line:
  the program of a `#!` line (the one `env` is given, past its options and settings; `python3.12` is
  `python`), then `first_line`. It is nil where none of them says a language.
- `languages.load(which)` is the language `which` says (a name or an alias in any case, or a
  description), built the first time it is asked for and the same table after; nil for a name the
  module does not know. `languages.loaded(which)` is the language where it has been built, nil
  otherwise: for a page that would not build one in a call that is short of time.
- An argument that is neither a name nor one of the module's descriptions is an error at the
  caller's line.

A language is a frozen `{ name =, description =, syntax =, config = }`. `syntax` is a language of
`@pencere/syntax`: `Syntax.new(language.syntax, id)` colours an editor with it, and
`language.syntax:tokens(line, state)` a text that is not in one. `config` is what an editor is told
of the language, in the editor's own options (Section 2.1): `comment`, `indent` (in Rust's syntax),
`folds` (`"indent"`), `pairs`, and `hard_tabs` and `tab_width` only where the language or its one
formatter fixes how it indents (Go and the Makefile, tabs; Rust and Python, four spaces; YAML, two
spaces and no tabs). A page gives `config` to an editor as it is (`pencere.editor(id,
language.config)`), its own options in a copy (`table.clone(language.config)`); a page that gives the
editor another `tab_width` gives `Syntax.new` the same.

Requiring the module builds no language: it reads its descriptions alone, and a family's module is
required when a language of it is first loaded. A language that holds another loads the other by
name, through the module, when it first meets it: HTML its scripts and styles by the tag's `lang` or
`type` (`lang="ts"`, `type="module"`, `type="text/css"`), a script read to the tag that closes it, on
its own line too; Markdown the code in a fence, by the first word of the fence's info. A held language
the module does not know is read as text. A reader takes at least a character a token and does work
in proportion to the line, and where nothing is open at a line's end it leaves the empty state `{}`,
which the lines after it share.

SQL is three descriptions that share one reader. `PostgreSQL` and `SQLite` each read the characters
the two databases read apart as its own does: `[` (a subscript or `ARRAY[…]` in PostgreSQL, a quoted
name in SQLite), `?` (an operator of `jsonb` in PostgreSQL, a parameter in SQLite) and `@name`,
`$name` (parameters of SQLite). `SQL`, for a file that does not say its database, reads the forms of
both: a `[` right after a name or a bracket is a subscript and any other opens a name; `?|` and `?&`
are operators and any other `?` is a parameter. Each of the three reads `--` and `/* */` comments
(nested in PostgreSQL), strings in `'` over lines (`''` a quote in one), `E'…'` and `X'…'`,
`$tag$…$tag$` (PostgreSQL's and SQL's), quoted names in `"` and in `` ` `` (SQLite's and SQL's),
`$1` and `:name` parameters and `::` casts. `#` is no comment: PostgreSQL's `#>` is an operator.

`languages.styles` is the one table of looks every language names its tokens from (a look is what
`Syntax.define`'s styles say one is), frozen, in the theme's roles:

| Tokens                                                       | Look                             |
| ------------------------------------------------------------ | -------------------------------- |
| `keyword`, `heading`                                         | `accent` (`heading` bold)        |
| `keyword.modifier`, `variable.special`                       | `accent`, italic                 |
| `string`, `code`, `inserted`                                 | `success`                        |
| `number`, `atom`, `constant`, `attribute`, `list`, `changed` | `warning`                        |
| `type`                                                       | `info`                           |
| `link`, `url`                                                | `info` (`link` underlined)       |
| `tag`, `deleted`                                             | `error`                          |
| `invalid`                                                    | `error`, a wavy underline        |
| `comment`, `quote`                                           | `dim` toward `quiet`, italic     |
| `punctuation`, `operator`, `meta`                            | `foreground` toward `background` |
| `function`, `property`, `string.special`, `escape`, `regexp` | mixed from two roles             |
| `emphasis`, `strong`, `strike`                               | the face's alone                 |

`languages.look(name)` is the look a token named `name` has by `styles`, as `language:look` finds it
(Section 2.6, `@pencere/syntax`), or nil: for a page that colours code that is not in an editor.

#### `@pencere/fuzzy`

What a person types, found in texts the way a picker, a palette or a filter finds it
(`local fuzzy = require("@pencere/fuzzy")`). A pattern is in a text where each of its characters is
there, in order, with anything between them; a space in the pattern is not looked for. Of the ways
it is there, the one that scores best is taken, as fzf's second algorithm scores it: a character
scores more at the start of the text and of a word (after a space, after one of `/ \ : , ; |`, after
any other character that is not a letter or a digit, a capital after a small letter, a digit after
what is not one), a run more than the same characters apart, a gap costs, and a first character late
in the text costs a point for each character before it, at most five.

- `fuzzy.match(pattern, text, options)` is the score and the places of the pattern's characters
  (characters of UTF-8 from 1, in order; a text that is not UTF-8 a byte a character; a text matched
  in its first 64 KiB), or nil where it is not there; an empty pattern is in every text, with a score
  of 0 and no places. `options.case` is `"smart"` (the default: a capital is told from its small
  letter only where the pattern has a capital), `"ignore"` or `"respect"`; a letter is lowered by
  Unicode's simple mapping for ASCII, Latin-1, Latin Extended-A, the letters in pairs of Latin
  Extended-B and Latin Extended Additional, Greek, Cyrillic, Armenian and the full-width Latin
  letters, and any other is compared as it is. With `options.path` the text is a path: a `/` and a
  `\` are one character, a pattern with a `/` in it is matched against the whole path, and any other,
  where it is in the path's last part (the name; a separator at the end left out), scores above any
  match that is not. `options.highlight` says which places: `"best"` (the default), those that gave
  the score, or `"first"`, where the pattern is first all there, from as late a first character as
  that allows; the score is the same. A match whose table of scores would pass 32 768 places is
  scored along the places `"first"` gives.
- `fuzzy.filter(pattern, items, options)` is the items the pattern is in,
  `{ { item =, index =, score =, positions = }, ... }`, a higher score first and of two the same the
  earlier item; with an empty pattern every item in order. Its options are `match`'s and `text` (an
  item's text where the items are not strings: a function of the item, or the name of its field; a
  number is its digits) and `limit` (how many to keep at most: the best, kept as they are found). It
  does all at once.
- `fuzzy.search(pattern, items, options)` (the same options) goes through the items a list holds
  when it is made a step at a time. `search:step(budget)` goes through at least one item, until what
  it compared (bytes of text and places of the tables of scores) comes to `budget` (60 000 unless
  said, some milliseconds), and says whether it is over; `search:results()` is the best so far, in
  order (a list without a `limit` is sorted again as it grows: a long one is searched with a limit);
  `search:progress()` how many items it went through and of how many; `search:done()` whether it goes
  no further (it went through every item or was cancelled); `search:cancel()` stops it, keeping what
  it found. `search:run(on_progress)` does a step at once and returns what `on_progress(search)`
  returns for it (a reply, for the page to answer with), then goes on by itself, a step a frame: a
  timer of its own (`pencere.time.after(0, …)`) wakes the page, does a step and tells `on_progress`,
  whose reply is the wake's, until it is done or cancelled. `search:narrow(pattern)` is a search for
  `pattern` in its place, with its items and options, and cancels it: where every text the new
  pattern is in, the old one's is in as well (its characters in the new pattern's in order, told
  apart in case no less), it goes only through what the old one found and the items it had not gone
  through. An error a list's item gives (no text) is the caller's, at its line; in a step `run` goes
  on with, the page's.
- `fuzzy.spans(text, positions, look, from, to)` is the text as parts of a text of many looks
  (`pencere.rich`): the characters at the places in `look`, what a `pencere.span` says (`color`,
  `weight`, `italic`, `underline`, `strike`, `family`, `bg`, `link`;
  `{ color = "accent", weight = 700 }` where none is given), the rest in the element's own look.
  `from` and `to` (characters) take a part of the text, the places counted in the whole; places
  outside it are passed over. It makes at most 1 024 parts, the rest plain in the last.

#### `@pencere/keys`

Keys as a page writes them (`"cmd+shift+k"`, Section 2.1), as a person reads them
(`local keys = require("@pencere/keys")`).

- `keys.parse(words)` is `pencere.keys(words)` with the key checked and named as a key event names
  it (`"page_up"`, `"delete"`, `"enter"`, `"escape"`, `" "`, `"f1"` to `"f20"`, or one character), or
  nil and why for words that are not keys.
- `keys.caps(words, options)` is a cap for each key (`{ "⇧", "⌘", "K" }`), and
  `keys.label(words, options)` the label: on a Mac the caps with nothing between (`"⇧⌘K"`), elsewhere
  joined by `+` (`"Ctrl+Shift+K"`). `words` are words or what `keys.parse` made; words that are not
  keys are an error at the caller's line. A Mac writes the modifiers in the order ⌃ ⌥ ⇧ ⌘ and the
  arrows `↑ ↓ ← →`; the other keys that are not characters it writes as words, as any other platform
  does (`Enter`, `Esc`, `Tab`, `Backspace`, `Delete`, `Space`, `Home`, `End`, `PageUp`, `PageDown`),
  unless the page asks for their glyphs (`options.symbols`, below): `⇞ ⇟` (page up, down), `↖ ↘`
  (home, end), `↩` (enter), `⎋` (escape), `⇥` (tab), `⌫` (backspace), `⌦` (delete), `␣` (space). Any
  other platform writes `Ctrl`, `Alt`, `Shift`, `Super` (`Win` on Windows) in that order and `Up`,
  `Down`, `Left`, `Right`, `PageUp`, `PageDown`, `Home`, `End`, `Enter`, `Esc`, `Tab`, `Backspace`,
  `Delete`, `Space`. Both write a function key `F5` and a letter as a capital (ASCII,
  Latin-1, Latin Extended-A, Greek and Cyrillic; another character as it is). `cmd` is the command
  key, matched as it is and never as control: elsewhere than on a Mac it is the system's logo key,
  `Super` (`Win` on Windows). `mod+…` is the host's main modifier (Section 2.1): `⌘` on a Mac, `Ctrl`
  elsewhere.
- `options.symbols` is whether a Mac writes the glyphs of the keys above (false where it is not given,
  unless `keys.set_symbols(true)` said so for the page; `keys.symbols()` says which).
- `options.platform` is `"mac"`, `"windows"`, `"linux"` or `"other"` (a platform a page does not name:
  written as Linux is); where it is not given it is `keys.platform()`, which `keys.set_platform(name)`
  sets for the page and which is the host's keyboard convention, `pencere.keyboard()` (Section 2.2),
  or `"mac"` where the page API says none. Windows writes the command key `Win`, Linux and `"other"`
  `Super`.
- `keys.matches(event, words)` is whether a key event (`key` or `key_up`, or a table with no `type`)
  is those keys: the same modifiers, no more and no fewer, and the same key, a character in either
  case, as the host matches a command's keys; an event of another type is not.

#### `@pencere/markdown`

`@pencere/markdown` (with `local markdown = require("@pencere/markdown")`) reads Markdown and draws
it. The dialect is CommonMark (0.31.2) with GitHub's extensions (GFM): tables, their columns aligned
as the delimiter row says (`\|` a pipe in a cell); task lists; strikethrough (`~` or `~~`); and the
links that need no brackets (`www.…`, `http://…`, `https://…`, `ftp://…`, an address of mail). HTML in
the text, a block of it or a tag in a line, is kept as the text it is and drawn so, never run. A
reference to a character is the character HTML names (every name of WHATWG's list, and numbers).

`markdown.parse(text)` is the tree of the text, read at once:
`{ type = "document", children = { block, ... } }`. Every block says the lines of the text it came
from: `line` (from 1) and `lines`. A block is a `heading` (`level`, `children`), a `paragraph`
(`children`), a `quote` (`children`), a `list` (`ordered`, `start`, `tight`, `children`: its items),
an `item` (`checked`: nil, or true or false for a task; `children`), `code` (`language`: the first word
of its fence's info, or nil; `info`; `text`, its last line break left out), `html` (`text`), a `rule`
or a `table` (`align`: a list of `"left"`, `"center"`, `"right"` or false; `head`: a list of cells;
`rows`: a list of lists of cells; a cell is a list of inlines). An inline is `text` (`text`),
`emphasis`, `strong` or `strike` (`children`), `code` (`text`), a `link` (`href`, `title`,
`children`), an `image` (`src`, `title`, `alt`: its description as plain text), `html` (`text`), a
`softbreak` or a `hardbreak`. An address is written as commonmark.js writes it: a byte an address may
not hold, or one that is not printable ASCII, is `%XX`.

`markdown.render(document, options)` is the element of a document, a column of its blocks: a
paragraph a text of many looks that wraps; a heading such a text, bold, in the size `headings` gives
its level (`{ 22, 18, 16, 14.5, 13.5, 13 }`), with the role of a heading; a list a column of rows, a
marker and the item's blocks (a task's marker a checkbox); a quote its blocks beside a bar; a table a
grid with a column for each of its columns (at most 64); a block of code a box in the terminal's
font; HTML a box of its text; a rule a line. Its `options` are each optional: `id`, the start of the
names of its elements (`"markdown"`); `colors`, roles of the theme: `text` (its words; the box's
where nil), `quiet` (`"dim"`), `link` (`"accent"`), `code` (`"raised2"`), `head` (`"raised1"`),
`quote` and `rule` (`"border"`); `gap`, the points between blocks (12); `size`, the size of the text
the page sets, for what it works out before the layout (13); `link`, `code`, `image` and
`languages`, below; and `limit`, the most elements it makes (10 000), what is past it left out and
an "…" in its place. An option it has no such name for, or one of the wrong kind, fails the page at
the line of the page's call. Two draws on one page have different ids.

A link is a part of a text named `<id>/link/<n>`. Given the page's events, `markdown.handle(event)`
answers a click on a link (the left button pressed and let go on it) with what `link(href)` returns:
by default `{ link = href }` for an `http` or `https` address, nil for any other; it answers nil to
every other event. `markdown.href(id)` is the address of the link of that name, for a page that finds
what was clicked its own way. An image is its description, as words of the text, since a page reaches
only the pictures of its bundle; where `image(src, alt, title)` is given, an image is the element it
returns, and the text around it stands in a row that wraps. A block of code is the element
`code(language, text)` returns; where that is not given, the block is coloured line by line by the
language `languages` finds (`find(word)`, then `load(description)`, which returns `{ syntax = }`, as
`@pencere/languages` does; by default that module, looked for when code is first drawn), a token in
the look `syntax:look(name)` gives. A draw colours at most 20 000 bytes of code and draws the rest
plain.

`markdown.document()` is a document kept for a text that changes.

- `doc:set(text)` reads again what changed. It starts at the block before the first line that
  changed, where that line may go on with that block (the line right after it, or, after blank lines,
  a list or code indented; a table whose header was the last line of a paragraph is read with that
  paragraph), and otherwise at the line after that block. It reads until, in the lines it did not
  change, a block begins at a line where one began before; the blocks after that are the ones it had,
  moved by the lines the change added or took. A change inside a fence, a list or a quote so reads
  that block again, and no more; where a definition of a link changes, the blocks that looked up its
  label have their inlines read again.
- One call reads at most 20 000 bytes (a line and a block cost besides their bytes). The rest is read
  by a timer of its own (`pencere.time.after(0, …)`), a call a frame, each asking for a frame, while
  `doc:busy()`; `doc:work()` does a call's work at once. Until it is done, `doc:tree()` is the
  document as far as it is read; `doc:reread()` is how many lines and blocks the last change was read
  in, so far.
- `doc:render(options)` is `markdown.render`'s element of it: a block that is as it was, drawn with
  the same options, is the element it was. Code left plain for want of what a draw colours is
  coloured in the draws after, and a frame is asked for each.
- `doc:list(id, options)` is a box named `id` that scrolls and holds the blocks as a long list
  (`box:list`, Section 2.2), which the page sizes with the builders after it
  (`doc:list("preview"):size_full()`). A block is a row named `<id>/block/<n>`, as tall as it is
  likely to be until the layout says; `doc:settle()`, which the page's `settle` returns, reads how tall
  the rows made came out, and is true where one changed. `doc:reveal(line)` scrolls the list to the
  block that holds line `line` of the text, and again as the layout says where that block stands. A
  document's id is `markdown<n>` unless the options say otherwise.

#### `@pencere/terminal`

Editors that look as the person's terminal draws its cursor and its selection
(`local terminal = require("@pencere/terminal")`). The host says how it draws them
(`pencere.terminal_look()`, Section 2.2) and applies none of it; this is for a page that wants its
editors to take it where the page says nothing.

- `terminal.editor(options, look)` is a copy of `options` (an editor's options; none: `{}`) with each
  field of `look` that the page did not write put in it, a field at a time: `caret` and `selection`
  are merged, not replaced, so that a page that writes `caret = { shape = "bar" }` keeps its bar and
  takes the rest of the terminal's. What the page wrote stays, `false` and `0` included. `look` is
  `pencere.terminal_look()` where none is given (a page that shows the look itself, a settings view,
  gives its own); a host that says nothing gives `options` back as they were. A table that is not
  options or a look, or an `options.caret` that is not a table, is an error at the caller's line.
- A page calls it from `render`, so that the options follow the setting: it asks again when the page
  is told `terminal_look`. A page in Rust (`pencere_page`) asks `terminal_look()` and puts it in the
  options with `TerminalLook::fill`: a field of the options at the editor's default is taken for one the page
  said nothing of (a Rust page cannot tell a default it wrote from one it left, as a Lua page's table can).

#### `@pencere/ui`

The components of a page (`local ui = require("@pencere/ui")`), written on the page API alone: a
component is a method of a context that the page makes once (`local u = ui.new()`) and calls in
`render` with a name and its props, and what the person does with it comes back from `handle` as
**actions**. The context sees to what the hand did (`pencere.Ui`, the keyboard's place, the layers in
front); the page keeps what it would miss if it were lost (`props.state`, a table the component changes in
place, or, where none is given, one the context keeps by the name for as long as the component is drawn).

- `u:begin(size, now, reduce_motion)` and `u:finish(root)` (`root`, `animating`, `wake_at`: what `render`
  returns) bracket a render; `u:settle()` is `settle`'s; `u:handle(event)` is the reply, the actions
  (`{ from = name, kind = …, … }`) and whether a component took the event, and the page does the
  rest of the event where none did.
- Components are drawn in the theme's roles and a few sizes (`ui.new({ theme =, density = "regular" |
"compact" })`), never in values; the host keeps their words readable. A component inside another
  says its action to the one around it first, which may take it for its own; one it does not take is
  the page's.
- A `popover` stands by an element (`anchor = "name"`), by the caret of an editor
  (`anchor = pencere.caret_of(editor)`) or by a range of its text (`anchor = pencere.range_of(editor,
name)`), and says `{ kind = "dismiss" }` at what `dismiss` lists: `"escape"`, `"away"` (a press outside it
  that is not on its anchor) and, by an editor, `"edit"`, `"caret"` (the selection moved), `"scroll"` (the
  lines in view changed) and `"leave"`; by an editor all of them, by an element the first two. `"leave"`
  waits `hide_delay` (0.3 seconds): the card stays if the pointer came onto it or rested on another word
  in that time (the page forwards the editor's `hover` events and its `wake` to `u:handle`, as it
  forwards every event). `focus = true` writes the commands `up`, `down`, `page_up` and `page_down`
  (while it is open) which scroll the card's body, and the keyboard stays in the editor. A card of a
  range that is not in the room is not shown. `step` (40 points) and `page` (0.9 of the card) say how far
  the keys of a card that has the focus scroll it, `max_height` (320 points) how tall it grows. A card
  does not close of itself when the pointer was on it as `hide_delay` ended and goes off it later: it goes at
  what else `dismiss` lists.
- `u:editor_menu(id, { editor =, at =, read_only =, before =, after = })` is the menu of an editor, the same
  in every editor of every page: the reply that asks the host for Cut, Copy, Paste, Select All, Toggle Line
  Comment and Select All Occurrences, each with the keys the editor has for it now (`layout.keymap`, as the
  page changed them), the page's own lines before and after (`before`, `after`, `"-"` for a rule). The page
  opens it from the `press` of a right button in an editor written with `gestures = { ["right+press"] =
"context" }` (`at` is the press's `x` and `y`), or, without `at`, under the caret for the keyboard
  (`ui.EDITOR_MENU_COMMAND`, Shift F10, is the command of that). Copy, Cut and the rest are not greyed for an
  empty selection, as the keys are not (Copy and Cut take the caret's line); `read_only` greys Cut, Paste and
  Toggle Line Comment. Paste is the host's line (`paste = editor`): the page neither reads the pasteboard nor
  asks the editor to paste. `u:editor_menu_pick(act)` is the reply that asks the editor for the line picked
  (nil for the page's lines and Paste).
- `button`, `icon_button`, `toggle`, `badge`, `kbd`, `rule`, `banner`; `input`, `segments`, `tabs`;
  `modal`, `dialog`, `toasts`, `menu` (the host's), `editor_menu`, `popover`; `list`, `palette`,
  `completions`, `tree`, `grid`; `split`, `status`; `find_bar`. `ui.commands(list)` writes the page's commands once for the host, the
  palette and the help, and holds them while a modal is in front. `ui.component(name, spec)` makes
  a component of the page's own the same way.
- An icon-only button and a field must say what they are in words (`label`), or the page fails at the
  line that drew them. A layer takes the keyboard when it opens and gives it back when it closes.

## 3. Colours

A colour is said in one of these ways, wherever one is taken:

| Form                                                | Meaning                                                                                           |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `"accent"`, `"red"`, `"raised2"`, …                 | A **role** of the theme. The way to prefer: it follows the theme the pane wears.                  |
| `"#rrggbb"`, `"#rrggbbaa"`                          | A **value**, sRGB, in either letter case; the last pair is the alpha.                             |
| `{ light = "#…", dark = "#…" }`                     | A value for a light theme and, if the page wants one, another for a dark theme (`dark` optional). |
| `pencere.mix(colour, toward, amount)`               | Any of the first three taken `amount` (`0` to `1`) of the way toward another of them.             |
| `{ colour, alpha }`, `pencere.alpha(colour, alpha)` | Any of the above with an opacity of `0` to `1` on top.                                            |

The opacity is **multiplied** with the alpha of a value: `pencere.alpha("#80808080", 0.5)` is drawn at a
quarter of full opacity. A mix is made in sRGB, channel by channel, a value's alpha with them, and then
the opacity is put on it; a host MUST resolve it so (`Color::resolve` in the engine does). It is how a
highlight comes in on its ground and how a word brightens: `pencere.mix("dim", "foreground", level)`.

The roles are those of the theme and three tones derived from them:

- the theme's own: `background`, `foreground`, `dim`, `accent`, `cursor`, `success`, `error`, `info`,
  `warning` — and `selection`, the ground of what the person selected, and `border`, the colour of
  rules and borders. These have the **same names as in a page's `context`** (protocol, Sections 9.5
  and 11.2);
- the sixteen colours of the ANSI palette by xterm's names: `black`, `red`, `green`, `yellow`,
  `blue`, `magenta`, `cyan`, `white`, and the same eight prefixed `bright-`;
- the kit's three derived tones, below: `raised<n>` (`pencere.raised(n)`, in the interface
  `raised(n)`), `quiet` and `shadow`.

A name that is none of these is an error at the line of the script that wrote it.

### 3.1 The derived tones (RECOMMENDED)

The host resolves a role in its theme, and may choose the derived ones as it likes. A host SHOULD
use these formulas, so that the same page looks alike on every host. `mix(a, b, t)` is the colour `t`
of the way from `a` to `b`, each channel rounded to the nearest value (halves away from zero), the
channels in sRGB.

| Tone        | Meaning                                                                   | Formula                                                |
| ----------- | ------------------------------------------------------------------------- | ------------------------------------------------------ |
| `raised(n)` | A surface `n` steps above the background; a card is 1, a field on it is 2 | `mix(background, foreground, 0.04 × min(n, 8))`        |
| `quiet`     | Text a step quieter than `dim`: readable and not standing out             | `mix(dim, background, 1/3)`                            |
| `shadow`    | The colour a shadow is cast in                                            | black on a dark theme; the `foreground` on a light one |

`raised(n)` has an **upper bound**: a host MUST accept any `n` from 0 to 255 (the interface carries
a byte, and so does the Lua name `raised<n>`) but treats an `n` above 8 as 8, and a page uses `1` to
`4` in practice. Because a surface is lighter than the ground on a dark theme and darker on a light
one, the formula uses the foreground as the end it moves toward.

`border` is not a derived tone but a role of the theme, with the name and the meaning it has in a
page's `context`. Where the theme gives none, the host makes it up as the protocol says (Section 11.2):
`mix(background, foreground, 1/5)`. The three tones above are for what no theme gives.

## 4. What a host gives a view at least

The sandbox floors are those of the protocol, Section 9.8 (memory 64 MiB, a call 250 ms, the start 2 s,
20 000 elements to a frame, a tree 128 deep, a line of text 64 KiB, a component 4 MiB (or the entry of
a script and the scripts it requires, together), 32 messages to the program to a call, a message
1 MiB, 4096 values to a sparkline or steps to a path, 1024 parts to a text, paths to a canvas or
commands to an element, and the limits on pictures). A view that stays within them
opens on every host. The values of this repository's implementation are `ModuleLimits::floor()` in
`kit/sandbox/src/sandbox.rs`: what an author's page is written and tested against (`pencere test`
holds a page to them). A terminal gives more time: `ModuleLimits::default()` is the floors with a call
5 s and the start 10 s. Time is the wall clock's and the wall clock of a busy machine is no measure of a
page, which runs on a thread of its own that the drawing never waits for; the limit is there to stop a
page that does not come back, not one that is slow.

Besides drawing what the engine lays out, a host does what follows. This repository's drive
(`pencere-drive`, `kit/drive`) does it for a host, all but what only the host can do: measure and
draw text in its fonts, draw in its renderer and theme, turn its keys into the kit's, and be its
system's input method, pasteboard, pointer, fields, menus, browser, screen reader, clock and program.
A host:

- puts one field of its own over each text field of the page (`Laid::fields`), of its kind, saying
  its hint while it is empty, and tells the page what the person types into it, where the person
  clicks into one, and takes the texts a reply fills them with and what it selects in them; and,
  while one has the keyboard, gives the page the keys it has no use for there, as the kit's rule
  says with the field's text and selection (`FieldState::takes`, Section 2.1); it takes what a
  reply asks of a field (its text, its selection, the keyboard) only of a field it has put over the
  page, and holds what a reply asks of one it has not until the next frame that lays the page out
  has been laid and its fields put over the page, then takes it in the order it was asked
  (Section 2.3: a reply that opens a field and fills it and sends the keyboard to it in one is
  done as it says); a name that frame does not have either it passes over;
- draws a run's `underline` and `strike` where its font puts those lines, in the run's colour, and
  sets an `italic` run in its face's italic;
- takes the pointer's shape a reply asks for, and tells a press's `count` and a scroll across;
- opens the link a reply asks to open as it opens a web page's `link` (protocol, Sections 9.7 and
  9.8): only an `http` or `https` address of printable ASCII without the space, at most 8192 bytes, in
  the person's browser on the person's machine, the person asked first where it asks for a page's,
  with as many questions open at once as for a page's; it drops any other and tells the page nothing
  either way (`pencere_host::session::Session::module_link`, which raises `Action::OpenLink` as a
  page's `link` does);
- wakes a page at the time it asked to be woken at: the earliest of what its replies and frames
  asked, which the sandbox keeps (`pencere_sandbox::Update::wake`, on render's clock). It arms one
  wakeup for that time and, at the first frame at or after it, wakes the page with the frame's time
  (`Module::wake`) before it asks for the frame's page. It wakes from the frame, never more than once
  a frame however often a page asks (a page that asks again at once costs a frame's work, as an
  `animating` one does), and never before the time or for nothing (the sandbox passes over such a
  wake). While the view is not seen (its pane hidden, its window minimized or on another space), a
  host MAY hold a page's wakes to one a second; it does not drop them: a wake held is given at the
  next one it gives, or when the view is seen again;
- scrolls the boxes that scroll (`Scrolls`): the wheel and the trackpad move the box under the
  pointer, as its system does (a gesture stays with the box it started on, and where the system does
  so a box pulled past its end springs back), its scroll bars are drawn as the system's are and can
  be dragged, the page's `scroll_to` is done before the page is next laid out, and the page is told
  where its boxes went; a wheel no box took is the page's (`scroll`);
- may draw a point of the page larger than a point of its screen, the page given fewer points for
  the same room: a terminal draws a page as large as its own text is to the kit's (12.5 points), so a
  page's words are the size of the terminal's and grow and shrink with them (`Laid::accessible` and the
  fields are in the page's points; the host places them);
- moves the elements that say a `motion` between the frames of the page, lays the page out again
  at each frame while one moves (`Motions`), and says an element's `tip` while the pointer rests
  on it (`Laid::tips`);
- tells its system's screen reader what the page is: each element as what it says it is (its `role`),
  the other pressable elements as buttons, the fields as fields, the links of a text as links, and the
  words of the rest, each by its `label` where it has one, with its state (checked, selected,
  expanded, of no use, its value) (`Laid::accessible`);
- draws a box's fill from one colour to another as a style sheet's `linear-gradient` of that angle
  draws it, over its plain fill and under its border, and a canvas's paths as `pencere_sandbox`'s
  `draw_figure` draws them (the one rasteriser, so that a figure is the same on every host), in the
  colours of its theme;
- lists the commands that can be done now (`Laid::commands`: those of the element the page says has
  the keyboard and of the boxes around it) where a person looks for what can be done, and tells the
  page the one asked for there; and, before it tells the page a key, looks for the command those keys
  do (`Laid::command_for`) and tells the page that command in place of the key;
- keeps telling the page where the pointer is, and the release, wherever the pointer goes while a
  button pressed on the page is held (it does not tell a `leave` then), so that a drag that leaves the
  page and comes back, or ends outside it, is followed to its end;
- tells the page the middle button, the keys that go up after it was told they went down, the
  modifiers when they alone change, and the files dropped on the page from outside it;
- says the time of day as its system does, with the person's offset from UTC (the interface's
  `system.wall-clock`, `Program::wall_clock`: the sandbox's own default reads the system's clock and
  knows no time zone, so a host that knows the person's says it), and how its system makes and
  writes shortcuts (`system.keyboard`, `Program::keyboard`: `mac`, `windows` or `linux`);
- draws a picture turned where its draw list says (`Picture::turn`, degrees clockwise about the
  middle of its rectangle, cut at its rectangle turned with it): what spins (a canvas that spins is
  turned before it reaches the host, and draws as any other);
- puts an editor of its own (`pencere-editor`, the same in every host) in each editor's room
  (`Laid::editors`), tells the page each one it makes (`editor_ready`, its text posted first), keeps
  its document as long as an editor of that name is on the page, hidden or
  not, and draws it in its place of the draw list (`Paint::Editor`), over the room's fill and under
  what covers it; draws its carets (their corners, light and outline as the options say, `caret.*`) blinks and moves them (`caret.motion`) on
  its own clock, as the options of the editor that has the keyboard say (the page is asked for no frames
  for it, whether the caret blinks or glides); gives it its system's input method (what is composed, and the questions of where the
  caret is and what is around it, answered at once, in the units its system counts in) and its
  pasteboard (text reaches an editor from it only on the person's key or the person's pick of a menu entry
  the host performs itself, `Host::paste`: a page cannot ask for it; what the page has an editor copy or
  cut is handed over to put on it, `Drained::copied`); sends a key to the page's command for it first, the editor's own next, the text last
  (none of the page's while an input method composes, and every key to the page where the editor
  gives it its keys), and tells its screen reader the editor as one;
- scrolls an editor's room as it scrolls a box, over the content the editor laid out, keeps the
  caret in view where the editor asks, and tells the page the lines in view; where a caret an
  overlay is anchored to moves (or to a range of its text the page named, which also moves with the
  text), or an editor with one scrolls, it lays the page's last tree out again without asking the page,
  and does not show an overlay whose range is out of the room;
- says how its terminal draws its cursor and its selection (`system.terminal-look`,
  `Program::terminal_look`, `pencere.terminal_look()`), as the person set them, in the words and units
  of an editor's options: a field only where it has such a setting, none where it has none (the default,
  which is a host with no settings of the kind). It MUST tell the page `terminal-look` when what it
  says changes, and only then, and MUST NOT apply it to a page's editors itself: the page decides. The
  page is told from the thread of the pane (the host's settings are reloaded on another, which
  wakes the pane's as it does for anything else; `Page::terminal_look_changed` is called there);
- says what it draws that not every host does (`system.draws`, `Program::draws`, `pencere.draws()`):
  `shadow` where it draws a box's soft shadow (`Quad::shadow`), only what it draws. A host MAY leave the
  soft shadow out; a caret's `glow` is then not seen, and the page can tell (`pencere.draws()`);
- tells the drive the pointer's shape when only the modifiers changed what is under it (a link
  that ⌘ made: `Drained::cursor`, the shape the host takes where the pointer is over the page);
- times the pointer's rest over a word of an editor that hears of it on its own clock, as it does the
  caret's blink (`Page::wake_at` has the time), and tells the page `hover` once, and its leaving;
- posts the page the text of each editor before it tells the page of the version it is (the copy
  costs nothing that grows with the document), and holds a reply to what one may hand the editors
  (1 MiB of text, 20 000 decorations) and the editors to the floors of the protocol (Section 9.8).

## 5. Pictures

[`README.md`](README.md) describes how a page names a picture and how the host draws it; the limits
and the formats are those of the protocol, Section 9.8.

Besides a file of the bundle, a page of `lua` `2.1` or `wasm` `3.0` names a file beside it: `sha256:`
and the digest of a file its program attached (protocol, Section 8.8) and `file:` and the handle of
one the person gave it (Section 8.9). A manifest path has no `:`, so the names never meet; the host
answers them from its own tables (the program's attached files, its handles), and a name that is in
neither is a picture that is not there. A file the program lets go of (`detach`) is forgotten, and
not drawn any more.
