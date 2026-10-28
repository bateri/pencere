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

| Kind   | The version `2.0` is                                                           | Source of truth               |
| ------ | ------------------------------------------------------------------------------ | ----------------------------- |
| `wasm` | the component interface `pencere:view@2.0.0`                                   | `wit/view.wit`                |
| `lua`  | the page API whose `pencere.version` is `2`, in Luau `0.740` (`kit/luau/pins`) | `luau/prelude`, this document |

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
`wake`, with the kit's timers, `pencere.time` (Sections 2.2 and 2.3); and the kit's library, with a
language that colours an editor, `@pencere/syntax` (Section 2.6). The kit's own sandbox opens `2.0`
and no longer `1.0`.

The regular expressions of a view (an editor's search, the rules of its indent) are in the syntax of
Rust's `regex` crate, `1.x` at the release the kit pins (its `Cargo.lock`), and one may take at most
4 MiB compiled (`pencere_view::edit::limits::PATTERN_SIZE`): a pattern that would take more is none.
A release of `regex` whose syntax a view could notice (a pattern it reads otherwise, or one it takes
that an earlier refused) arrives with a new minor version of both kinds, as a later Luau does for
`lua`.

## 2. The Lua page API

A script is Luau. What it can use besides the language is the base functions, `string`, `table`,
`math`, `utf8`, `require` (the other scripts of its bundle and the kit's library: Sections 2.5 and
2.6), and `pencere`, the table the prelude fills. It has no files
of the system's, no network, no clock but the time `render` and a wake hand it (Section 2.3), and no
randomness.

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
(`text`), sends the keyboard to one (`keyboard = { field = name }`), and puts a text in one (`fill`).

While a field has the keyboard, the keys that do not edit its text are the page's, as `key` events,
and the field keeps the keyboard unless the reply sends it elsewhere: escape and tab in every field,
and in a line (`"line"`, `"secret"`) besides: enter, up and down with whatever modifier (in one line
they would only take the caret to an end), page up and page down, the function keys, and a chord
of command or control with a key the field has no use for. The characters, left and right, delete,
home and end, and the field's own editing chords (select all, copy, cut, paste, undo; copy with
nothing selected is not one) stay the field's. So a list with a filter above it takes its
commands while the person types: the arrows move in the list, enter chooses, ⌘K opens a palette.

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
(`"cmd+shift+k"`, `"ctrl+enter"`, `"f5"`); `pencere.keys(words)` reads them. A command can be done
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

| option                                               | what it is                                                                                                                                                                                                                                                                      | default                                    |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ |
| `line_height`                                        | how tall a row is, times the font's size (1 to 3); the text is in its middle, with a point at least above and below                                                                                                                                                             | 1.6                                        |
| `padding`                                            | the room kept around the text, points (0 to 64)                                                                                                                                                                                                                                 | 4                                          |
| `scroll_margin`                                      | the rows kept between the caret and the top or the bottom of the room when the editor scrolls to the caret (at most half the rows in view)                                                                                                                                      | 0                                          |
| `numbers`                                            | what the numbers of the lines count: `"absolute"`, `"relative"` (the caret's line its own number, every other line how many lines it is from it, a folded region one line) or `"hidden"` (the gutter keeps its marks and fold buttons)                                          | `"absolute"`                               |
| `current_line`                                       | where the ground of the main caret's line is drawn: `"text"`, `"gutter"` or `"all"` (both); its colour is `colors.current_line`                                                                                                                                                 | `"text"`                                   |
| `whitespace`                                         | which spaces and tabs are shown, as VS Code's `renderWhitespace`: `"none"`, `"boundary"` (all but a space alone between two words), `"selection"` (those in what is selected), `"trailing"` (those after the last word of a line, every one of a line with no words) or `"all"` | `"none"`                                   |
| `indent_guides`                                      | a thin line down each level of indent, the block the main caret is in having its own colour while the editor has the keyboard                                                                                                                                                   | false                                      |
| `caret.shape`                                        | `"bar"` (before the character), `"block"` (over the character, which is drawn again on it; a space wide at the end of a line) or `"underline"`                                                                                                                                  | `"bar"`                                    |
| `caret.height`                                       | `"row"` (as tall as the row, as the current line's ground is) or `"text"` (as tall as the text)                                                                                                                                                                                 | `"row"`                                    |
| `caret.width`                                        | how wide the bar is, or how thick the line under the character, points (0.5 to 8)                                                                                                                                                                                               | 2                                          |
| `caret.color`, `caret.text`                          | the caret's colour, and the colour of the character under a block                                                                                                                                                                                                               | cursor, background                         |
| `colors.current_line`                                | the ground of the main caret's line, while the editor has the keyboard                                                                                                                                                                                                          | foreground at 5%                           |
| `colors.selection`, `colors.selection_blurred`       | the ground of what is selected, while the editor has the keyboard and while it has not                                                                                                                                                                                          | selection, selection at half               |
| `colors.found`                                       | the ground of what a search found                                                                                                                                                                                                                                               | warning at 30%                             |
| `colors.brackets`                                    | the ring around the bracket by the caret and the one that matches it                                                                                                                                                                                                            | dim at 60%                                 |
| `colors.numbers`, `colors.current_number`            | the numbers of the lines, and the caret's line's                                                                                                                                                                                                                                | quiet, dim                                 |
| `colors.gutter`                                      | the ground of the gutter                                                                                                                                                                                                                                                        | what the editor is on, else the background |
| `colors.folds`, `colors.folded`                      | the buttons that fold and the `⋯` after a folded line, and the ground of that `⋯`                                                                                                                                                                                               | dim, foreground at 8%                      |
| `colors.whitespace`                                  | the dots, arrows and boxes of the spaces and tabs shown                                                                                                                                                                                                                         | border                                     |
| `colors.indent_guides`, `colors.active_indent_guide` | the guides of the indent, and the guide of the main caret's block (`false`: drawn as the others are); with the guides' colour `false` and the block's not, the block's guide alone is drawn                                                                                     | border, quiet                              |
| `colors.hint`                                        | what the editor says while it is empty                                                                                                                                                                                                                                          | quiet                                      |

A colour is any colour of the page (a role's name, a value, `{ colour, alpha }`); `false` draws none
(`colors = { current_line = false }`, or `caret = { color = false }` for an editor with no caret). A
number out of its range is taken to its nearest end. An option an editor has no such name for is a
mistake, and so is a word it does not take: the page fails where it says it. The text is the box's
colour and font (`:color`, `:text_size`, `:family`), and what the page decorates (Section 2.3) is
drawn over all of it. The host blinks the caret.

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
`pencere.layout.bounds` says where it is. A popup's keys (↑ ↓ ↩ ⇥ Esc) are commands of the page
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
gone. The host does the moving, frame by frame, and does not ask the page for the frames in between;
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
a large document is read in pieces), `line_start(id, line)` and `line_of(id, at)`. A page reads in
the `edit` event the text of the version the event is of.

**Timers** run when the host wakes the page (a reply's `wake`, Section 2.3), and are named as the
program runner's. `pencere.time.after(seconds, fn)` runs `fn` once, `seconds` (0 or more) from now;
`pencere.time.every(seconds, fn)` every `seconds` (more than 0) from then on, once and not once for
each for runs it fell behind on. Both return a timer: `:cancel()` stops it (`true` where it was still
waiting) and `:active()` says whether it waits. `pencere.time.now()` is the time the host last gave
the page, `render`'s `now` or a wake's (0 before any); it does not go on between them. The kit asks
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

| `type`             | Members                                                                                                                                                                                                                                                  |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `move`, `leave`    | `x`, `y` (`leave` has none)                                                                                                                                                                                                                              |
| `press`, `release` | `x`, `y`, `button` (`"left"`, `"right"` or `"middle"`), and for `press` `mods` and `count` (2 for the second of a double click, as the system counts them)                                                                                               |
| `scroll`           | `x`, `y`, `dx`, `dy`: a wheel no box the host scrolls took, the points the content would move left and up by (for a page that does something of its own with the wheel)                                                                                  |
| `scrolled`         | `boxes`: the boxes the host scrolled, each `{ id =, x =, y = }`, how far it is scrolled now                                                                                                                                                              |
| `key`              | `key` (`"up"`, `"down"`, `"left"`, `"right"`, `"page_up"`, `"page_down"`, `"home"`, `"end"`, `"enter"`, `"escape"`, `"tab"`, `"backspace"`, `"delete"`, `"f1"` to `"f20"`, or the character), `mods`                                                     |
| `text`             | `field` (its name) and `text`: what a field holds now, after the person changed it (not after the page's own `fill`)                                                                                                                                     |
| `focus`            | `field`: the field the person clicked into, or nil when the keyboard went back to the page of itself (not said when it went where the page sent it)                                                                                                      |
| `menu`             | `index` (from 1) of the entry picked from the menu the page asked for                                                                                                                                                                                    |
| `key_up`           | `key` and `mods`, as `key`: a key the page was told of went up                                                                                                                                                                                           |
| `mods`             | `mods`: the modifiers held changed, with no other key                                                                                                                                                                                                    |
| `command`          | `id`: the command asked for, from the host's list or by its keys (the key is not told as well)                                                                                                                                                           |
| `files`            | `x`, `y`, `paths`: files dropped on the page from outside it, their paths as the host's system writes them                                                                                                                                               |
| `edit`             | `editor`, `version` (what the document came to), `origin` (`"person"`, `"compose"`, `"page"`, `"undo"`, `"redo"`, `"load"`), `changes` (`{ { from =, to =, insert = }, ... }`, in the places of the version before, in order)                            |
| `selection`        | `editor`, `version`, `ranges` (`{ { anchor =, head = }, ... }`), `main` (from 1), `composing` (an input method is composing at the main caret; the host does none of the page's commands then)                                                           |
| `viewport`         | `editor`, `first`, `last`: the lines in view, for a page that colours only what is seen                                                                                                                                                                  |
| `editor_refused`   | `editor`, `reason` (`"reversed"`, `"out-of-document"`, `"overlapping"`, `"stale-version"`, `"future-version"`, `"too-large"`, `"too-many-decorations"`, `"outside-range"`, `"bad-pattern"`, `"no-editor"`): what the page asked was not done, none of it |
| `found`            | `editor`, `version`, `count` (at most 10 000), `current` (the match the main selection is exactly on, from 1; nil for none), `more` (there may be more than `count`): what its search finds (below)                                                      |
| `wake`             | `now`: the time the page asked to be woken at (a reply's `wake`, below) has come, seconds on `render`'s clock; told only to a page that asked, after the timers that were due ran (Section 2.2)                                                          |

`move`, `press` and `release` over an editor have `place`, `{ editor =, at = }`: the place of its
document under the pointer (a word to say what it is of while the pointer rests on it, a name to go to
at a ⌘-click). A press in an editor is the editor's as well: it puts the caret.

`mods` is `{ shift =, control =, alt =, command = }`. A reply is `nil` (nothing changed), `true` or
`false` (a frame is worth drawing or not), or a table: `redraw`, `close`, `copy` (text for the
pasteboard), `cursor` (`"arrow"`, `"pointer"`, `"text"`, `"resize_horizontal"`, `"resize_vertical"`,
`"not_allowed"`, `"grab"`, `"grabbing"` or `"crosshair"`), `keyboard` (`"page"` or `{ field = name }`),
`fill` (`{ [name] = text }`: what fields are to hold from now on; a name that is not a field of the
page is passed over), `menu = { at = { x =, y = }, entries = { { title =, key =, enabled = }, "-" } }`,
`link` (an address to open in the person's browser, below), `wake` (in how many seconds to wake the
page, below), and `editor = { [name] = ask, or { ask, ... } }`, what the page asks of its editors,
each ask one of:

| Ask                                                                    | What                                                                                                                                                                               |
| ---------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `{ load = text, more = true }`                                         | A piece of the document: the first in place of the whole text, each after it (`more` said before) on its end. Nothing of it is undone, and what could be undone before cannot be.  |
| `{ change = { { from =, to =, insert = }, ... }, base = version }`     | Changes against the version the page saw: carried over what was done since (a range's start after what came in at it, its end before), done, undone as one.                        |
| `{ select = { { anchor =, head = }, ... }, version =, main = }`        | The selections, in the places of `version`.                                                                                                                                        |
| `{ reveal = place, version = }`                                        | The place scrolled into view.                                                                                                                                                      |
| `{ command = name, ... }`                                              | What the person could do with a key: `"move"` (`movement`, `extend`), `"insert"` (`text`), `"undo"`, `"find"` and the commands of a search, … (`pencere_view::edit::EditCommand`). |
| `{ decorate = layer, version =, from =, to =, decorations = { ... } }` | The layer's decorations for `from`–`to`, in place of those it had there; the other layers stay.                                                                                    |

A layer is named by the page (`"syntax"`, `"diagnostics"`, `"folds"`): the colours of what is in view
are sent without taking the errors away. A decoration is `{ from =, to =, color =, background =,
weight =, italic =, strike =, underline = colour or { wavy = colour } }` (a look), `{ from =, to =,
line = colour }` (a ground for each line it touches), `{ from =, to =, gutter = colour }` (a mark by
its first line) or `{ from =, to =, fold = true }` (a region that folds: its first line stays; whether
it is folded is the person's). Decorations of an older version are carried over the changes made
since. What the editor cannot do is refused and the page told why (`editor_refused`): a range the wrong
way round, a place past the end, changes that overlap, a version older than the last 1 024 or yet to
come, a document larger than an editor may hold.

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
person cannot change (`read_only`) is not replaced in.

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
and decodes JSON (arrays and objects nested up to 127 levels, the outermost counting as the first: protocol, Section 4.5). A message the program cannot take yet waits its turn (`pencere.send`, the raw word,
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
opens on every host. The values of this repository's implementation are the defaults of
`ModuleLimits` in `kit/sandbox/src/sandbox.rs`.

Besides drawing what the engine lays out, a host does what follows. This repository's drive
(`pencere-drive`, `kit/drive`) does it for a host, all but what only the host can do: measure and
draw text in its fonts, draw in its renderer and theme, turn its keys into the kit's, and be its
system's input method, pasteboard, pointer, fields, menus, browser, screen reader, clock and program.
A host:

- puts one field of its own over each text field of the page (`Laid::fields`), of its kind, saying
  its hint while it is empty, and tells the page what the person types into it, where the person
  clicks into one, and takes the texts a reply fills them with;
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
- puts an editor of its own (`pencere-editor`, the same in every host) in each editor's room
  (`Laid::editors`), keeps its document as long as an editor of that name is on the page, hidden or
  not, and draws it in its place of the draw list (`Paint::Editor`), over the room's fill and under
  what covers it; draws its carets and blinks them on its own clock (the page is asked for no frames
  for it); gives it its system's input method (what is composed, and the questions of where the
  caret is and what is around it, answered at once, in the units its system counts in) and its
  pasteboard; sends a key to the page's command for it first, the editor's own next, the text last
  (none of the page's while an input method composes, and every key to the page where the editor
  gives it its keys), and tells its screen reader the editor as one;
- scrolls an editor's room as it scrolls a box, over the content the editor laid out, keeps the
  caret in view where the editor asks, and tells the page the lines in view; where a caret an
  overlay is anchored to moves, or an editor with one scrolls, it lays the page's last tree out again
  without asking the page;
- posts the page the text of each editor before it tells the page of the version it is (the copy
  costs nothing that grows with the document), and holds a reply to what one may hand the editors
  (1 MiB of text, 20 000 decorations) and the editors to the floors of the protocol (Section 9.8).

## 5. Pictures

[`README.md`](README.md) describes how a page names a picture and how the host draws it; the limits
and the formats are those of the protocol, Section 9.8.
