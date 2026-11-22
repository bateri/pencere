# Marks, zones and searches

## Marks, zones and widgets

A layer is named by the page (`"syntax"`, `"diagnostics"`, `"folds"`): the colours of what is in view
are sent without taking the errors away. A decoration is `{ from =, to =, color =, background =,
weight =, italic =, strike =, underline = colour or { wavy = colour } }` (a look), `{ from =, to =,
line = colour }` (a ground for each line it touches; a look may say an `id`, which the `hits` of a rest
over it tell), `{ from =, to =, gutter = colour, … }` (a
marker in a column of the gutter, below), `{ from =, to =, ruler = colour, lane = }` (a mark on the
overview ruler, below), `{ from =, to =, zone = name, height = points, side = "below" | "above" }` (a gap for the page, below),
`{ from =, to =, anchor = name }` (a name for the range: `pencere.range_of`, [The editor](editor.md#what-is-not-seen-the-document-and-its-places)),
`{ from =, to =, widget = words, side = … }` (text that is not the document's, below) or
`{ from =, to =, fold = true }` (a region that folds: its first line stays; whether it is folded is the
person's). Decorations of an older version are carried over the changes made
since. What the editor cannot do is refused and the page told why (`editor_refused`): a range the wrong
way round, a place past the end or inside a character (`"mid-character"`: [Counting](counting.md)), changes that
overlap, a version older than the last 1 024 or yet to come, a document larger than an editor may
hold.

**The gutter** is a row of named columns at the left of an editor's text. The page lists them in the
order it wants (`gutters = { "numbers", { name = "breakpoints", width = 14 }, "marks", "folds" }`);
the three the editor has (`"marks"`, the column of dots, `"numbers"` and `"folds"`) are there in that
order where it says none, and one the page leaves out is not there (no `"folds"`, no buttons that
fold). The numbers keep to their left the editor's padding where they are the first column, so that
they do not stand against the edge of the room (`pad = points` says another; none after another
column). A column of its own has a `width` (points, as wide as the dots' where none is said) and says
what of the pointer the page hears of it: `events = { "press", "context", "hover" }` (a column of the
page's hears the first two by default, `false` or `{}` nothing). A press of the main button in a
column that hears it is the page's alone, `gutter` with `kind = "press"`: the line is not selected,
nothing folds, the caret does not move (the editor has the keyboard); a press of another button, or
of the main one with ⌃, is `gutter` with `kind = "context"`, the press that opens a menu. That is so where the cell is empty as well (a page puts a
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

A zone with `side = "above"` is the gap before its line (a code lens: the row of what a function
has, over it), between the line before it that shows and the line, or before the first row where the
line is the first; the rows of the line before it do not take it, and it is the page's element as any
zone's is, made for the lines in view (`viewport`). A line folded away has none to show, and the first
line of a fold keeps its own. An editor does not hold an editor in a zone: peeking at another file's
definition is a `popover` that holds a read-only editor element.

**Links.** A look may say `held = "cmd"` (modifiers joined by `+`: `cmd`, `ctrl`, `alt`,
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

## Keys, commands and searches

**The editor's own keys** (what `keymap` changes; `pencere.layout.keymap(editor)` lists them with the
page's changes, each `{ keys =, does = }` with `does` as `keymap` takes it, nil where the page took the key
away): ←, →, ↑, ↓ move (with ⇧ they extend, with ⌥ ← → by words, with ⌘ ← → to the line's ends and ⌘↑ ⌘↓ to the
document's, `page_up`, `page_down`, `home`, `end` as written, `ctrl+a` and `ctrl+e` the line's ends);
`backspace` and `delete` (`alt` by words, `cmd+backspace` to the line's start, `ctrl+k` to its end);
`enter` is `newline`, `tab` is `tab` and `shift+tab` `outdent`, `escape` is `escape` (one selection);
`cmd+z`, `cmd+shift+z` are `undo`, `redo`; `cmd+a` `select_all`; `cmd+d` `select_next`; `cmd+/`
`toggle_comment`; `cmd+]`, `cmd+[` `indent`, `outdent`; `cmd+alt+]`, `cmd+alt+[` `unfold`, `fold`;
`cmd+g`, `cmd+shift+g` `find_next`, `find_previous`; `cmd+c`, `cmd+x`, `cmd+v` `copy`, `cut`, `paste` (the
person's own paste key: `keymap` has no word that pastes, so a page cannot bind another key to it);
`alt+up`, `alt+down` `move_lines_up`, `move_lines_down`; `alt+shift+up`, `alt+shift+down`
`duplicate_lines`; `cmd+alt+up`, `cmd+alt+down` `add_caret_above`, `add_caret_below`. A key that is a
command of the page (enabled) and the first key of a chord of the editor's `keymap` is a page that
fails where it wrote them (a key of the keymap that is one alone is the page's command's, as the
editor's own keys are).

The list above is a Mac's. Where the host's system has no ⌘ (`pencere.keyboard()` is `"windows"` or
`"linux"`) Ctrl has the keys ⌘ has there that a terminal can send: `ctrl+a` is `select_all` (not the
line's start: `ctrl+e` stays the line's end and `ctrl+k` deletes to it), `ctrl+z` and `ctrl+y` are `undo` and
`redo`, `ctrl+left` and `ctrl+right` go by words and `ctrl+home` and `ctrl+end` to the document's ends
(with ⇧ they extend), and `layout.keymap` lists them first. No `ctrl+c`, `ctrl+x` or `ctrl+v`: `ctrl+c`
is the interrupt of a terminal's program, a cut that the terminal may not let reach its pasteboard
would lose the text, and a paste is the person's own, the terminal's. No ⇧ with a letter (`ctrl+shift+z`
is not redo: a terminal that cannot tell it from `ctrl+z` sends the same key for both) and no ⌥ with ⌃
(a keyboard types with it, a desktop takes it). The keys of ⌘ stay for a host that reports the logo key
(`cmd+x` cuts for a terminal that sends Super), and `alt+left` and the rest are as they are.

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
= }` looks for `pattern` through the whole document (a regular expression, [Versions](versions.md), where `regex`, in either
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
copies and does not cut). **There is no command that pastes, and no key the page chooses that does**: a page
can write the pasteboard (`copy`) but never read it, so text reaches an editor from the pasteboard only on the
person's own paste key (`cmd+v`; a page that takes that key from the editor gets a key that pastes nothing) or the
person's pick of a menu entry the host performs itself (`paste = editor` in a menu's entry): the host reads its pasteboard and the
text goes in as the paste key's does, in a `read_only` editor not at all and at several carets a line each
where the text has as many lines; the page is told the pick (`menu`, the `index` and the `entry`) as any and never the text
(an `edit` that tells what went in is told, as for the person's own key). The line's words are the kit's
("Paste"), whatever the page wrote, and it pastes only into the editor that had the keyboard when the page
asked for the menu, and only a line that is on: a page cannot send what is on the pasteboard to an editor
the person is not in, nor give the pick another name. The line is on only in the menu a right press
asked for (below, **A menu is shown by the same rule**).
A page asks for a paste by a command, `{ command = "paste" }`, or binds a key to one,
`keymap = { enter = "paste" }`, and is failed where it writes it, as for any word that is no command; the person's own key, `["cmd+v"] = "paste"`, as `layout.keymap` lists it, is no error and changes nothing.
