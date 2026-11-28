# The editor

## An editor and its language

An **editor** (`pencere.editor(id, options)`) is the host's, as a text field is. These are the host's editor's, the same in every host:

- its document;
- its carets and selections (as many as the person makes, a column with ⌥);
- undoing;
- what an input method composes;
- finding;
- folding;
- the drawing of its lines.

The room scrolls as a box that scrolls does (`pencere.layout.scroll_to(id, …)` scrolls it). The box's fill and border are the page's. Two editors, or an editor and a box, that say the same `scroll_link` scroll together ([Layout and scrolling](layout.md#boxes-that-scroll-together)).

### The options of an editor

`options` are:

- `wrap`: long lines go on in the next row;
- `tab_width` (4);
- `hard_tabs`;
- `gutter`: the numbers of the lines, the page's marks and the buttons that fold; on by default;
- `read_only`: the person cannot change the text; the page can;
- `input`:
  - `"editor"`;
  - or `"page"`: every key the page's, and the editor types nothing and does none of its own commands for a key. It is for an editor of modes the page drives;
- `hint`: what it says while it is empty; or the first text inside it;
- `pairs`: the brackets and quotes that close themselves, `"()[]''"`.

### The language of an editor

What the page says of its **language**, as a language configuration of VS Code does, the editor does. An editor that says none of it is one for prose, as it was:

- a line break keeps the line's indent;
- what is typed moves no line;
- nothing is commented;
- and nothing folds but what the page folds.

### Comments

`comment` is how the language writes its comments: a line comment (`"--"`), or `{ line = "--", block = { "--[[", "]]" } }` with either part left out (the block a pair, or `{ open =, close = }`).

- `"toggle_comment"` comments the lines of each range with the line comment, or uncomments them where every line with text on it is commented. A language with block comments alone puts the lines of each range in one, or takes them out.
- `"toggle_block_comment"` puts each range in the block comment (a caret's range is its line's text, its indent left out), or takes it out where it is in one: the tokens just outside the range or at its ends, a space or a tab between them and the text taken with them. Where some ranges are in one and some are not, those that are not are put in one. No key does it: a page asks for it (`⌘/` toggles comments).

### Indent

`indent = { increase =, decrease = }` are regular expressions (Rust's `regex`, as [Versions](versions.md) pins it: no look-around, no back-references) matched against a line's text, either left out.

- A line break after a line whose text before the caret matches `increase` gives the new line a level of indent more, as one after an opener of `pairs` does. A line break between such a line and what matches `decrease` makes that go on a line of its own.
- A line that what is typed makes match `decrease` (it did not before) goes a level out from the indent the line above gives (the nearest line with text: its own indent, a level more where it matches `increase` or ends with an opener), where it is not already further out. An undo takes it back with what was typed.
- Lua's are `increase = "\\b(then|do|else)\\s*$"` and `decrease = "^\\s*(end|else|elseif|until)\\b"`.
- A pattern that is not one fails the page, the words naming the option, and so do more than 64 different patterns in the editors of one frame.
- A line longer than 10 000 characters is not read by them.

### Folds

`folds` is:

- `"page"`: the page's fold decorations alone ([Marks, zones and searches](editor-marks.md#marks-zones-and-widgets)); the default;
- or `"indent"`: the editor folds by the indent as well. A line with text folds the lines after it that are further in, and the lines with no text among them but not those after the last of them nor a run of more than 1 000 (a tab is as far in as to its next stop).

The page's folds come with them, and its own is the one on a line both fold from. They are worked out from the text as it is, for the lines in view, and so follow every change.

## How an editor looks

How an editor looks is the page's to say, and what it leaves out is the editor's default, in the theme's colours. Each option is said with what it is, and its default where it has one.

### The room and the gutter

- `line_height`: how tall a row is, times the font's size (1 to 3). The text is in its middle, with a point at least above and below. Default: 1.6. How tall the row comes to, in points, is `pencere.layout.scroll_extent(id).row`: nil until the editor has been laid out, and not told again when the row changes (the zoom, the text's size or the `line_height`), so a page that needs a height in rows says it in rows (a zone's `rows`, [Marks, zones and searches](editor-marks.md#zones)) and does not work one out.
- `padding`: the room kept around the text, points (0 to 64). Default: 4.
- `fade`: how far in from the edge of the text a line that does not fit thins out, points (0 to 64; 0 cuts it where the text ends). At the right of the text where a line of a room that does not wrap goes past it, and at its left where the room is scrolled across and a line starts before it; the gutter is not faded, nor are the carets, the line of the caret or a selection's ground, and a room that wraps has nothing to thin out. Default: 22. `pencere.has("editor.fade")`.
- `scroll_margin`: the rows kept between the caret and the top or the bottom of the room when the editor scrolls to the caret (at most half the rows in view). Default: 0.
- `numbers`: what the numbers of the lines count:
  - `"absolute"`;
  - `"relative"` (the caret's line its own number, every other line how many lines it is from it, a folded region one line);
  - or `"hidden"` (the gutter keeps its marks and fold buttons).

  Default: `"absolute"`.

- `gutters`: the gutter's columns, from the left, by name.
  - The editor's `"marks"` (the dots), `"numbers"` and `"folds"` (the buttons that fold) are there where the page says none, in that order.
  - A page lists the ones it wants, with its own among them (`{ name =, width =, events =, hover =, pad = }`, [Marks, zones and searches](editor-marks.md#marks-zones-and-widgets)).
  - One it leaves out is not there.
- `marks`: `{ bar =, hover =, ruler = }`. It says how wide a bar of the gutter is (3 points), how wide one the pointer is over is (6) and how wide the overview ruler is (4). Each is 1 to 16.

### Events, presses and keys

- `events`: what an editor tells the page of its own, as a list of words:
  - `"hover"` is the pointer's rest over a word and its going from it (`hover` event, [Events](events.md#what-a-page-is-told));
  - `"target"` is the pointer over a word with a press of the page's held, and that press (`target` event, [Events](events.md#what-a-page-is-told)).

  None is told unless asked, for the rare things are told, and what comes at the rate of the pointer's moving is not. Default: none.

- `hover`: `{ delay = }`, how long the pointer rests over a word before the page is told of it, seconds (0 to 10; VS Code's `editor.hover.delay`, Zed's `hover_popover_delay`). Default: `{ delay = 0.3 }`.
- `gestures`: how a press is taken, by its button, modifiers and count.
  - `{ ["press"] = "place", ["shift+press"] = "extend", ["mod+press"] = "target", ["alt+press"] = "column", ["press*2"] = "word", ["press*3"] = "line" }` are what an editor does where the page says nothing.
  - `"add_caret"` and `false` (nothing) are the others.
  - `"context"` is a press that opens the page's menu ([Events](events.md#what-a-reply-may-ask)).
  - `"target"` is the page's alone: it is told as a `target` event, no caret moves, and the editor has the keyboard. It needs `events = { "target" }`.
  - `"middle+press"` and the other buttons may be `"target"`, `"context"` or `false`.

  Default: as written.

- `keymap`: the editor's keys the page changes: `{ ["mod+d"] = "select_next", ["mod+/"] = false, ["mod+k mod+c"] = "toggle_comment" }`, the keys of a chord apart by a space (at most four).
  - A key is given one of:
    - the name of a command that needs none (`"undo"`, …), `"copy"`, `"cut"`, `"tab"`, `"escape"`, `"delete_to_line_start"`, `"delete_to_line_end"`;
    - the table of a command (`{ command = "move", movement = "down" }`);
    - or `false` (nothing of the editor's: the key is the page's).
  - A chord waits `chord_timeout` seconds for its next key, and the page is told (`keymap_pending`). A key that is none of its next ends it and does nothing. Escape ends it.
  - `pencere.layout.keymap(editor)` says what the keys do now, `cmd+v` as `"paste"`. That is the person's own key: it is read, and written back as it is read it is what the editor has anyway, so the table `layout.keymap` lists can be given back as the `keymap`. No other key may be a paste.

  Default: as written.

- `chord_timeout`: seconds a chord waits for its next key (0.1 to 10). Default: `1`.

### Widgets and links

- `widgets`: how the groups of widgets (text the page puts in lines, [Marks, zones and searches](editor-marks.md#marks-zones-and-widgets)) look and when they show, by name: `{ hints = { show = "unless_held", hold = "ctrl+alt", size = 0.9, color =, background =, radius = 3, pad = 3, gap = 16, margin = 3 } }`.
  - `show` is `"on"`, `"off"`, `"unless_held"` (VS Code's `onUnlessPressed`: hidden while `hold` is held) or `"when_held"` (`offUnlessPressed`).
  - `size` is how many times the text (0.5 to 2).
  - `radius`, `pad`, `gap` and `margin` are points (0 to 64): the corners, the room inside the box, the gap before a widget at a line's end, and the room kept outside the box of an inline one, between it and the characters beside it.

  Default: as written.

- `links`: `{ delay = }`, the least time between two words the pointer comes to being told of (`target`), seconds (0 to 1). Default: `{ delay = 0.05 }`.

### What is drawn on the text

- `current_line`: where the ground of the main caret's line is drawn: `"text"`, `"gutter"` or `"all"` (both). Its colour is `colors.current_line`. Default: `"text"`.
- `whitespace`: which spaces and tabs are shown, as VS Code's `renderWhitespace`:
  - `"none"`;
  - `"boundary"` (all but a space alone between two words);
  - `"selection"` (those in what is selected);
  - `"trailing"` (those after the last word of a line, every one of a line with no words);
  - or `"all"`.

  Default: `"none"`.

- `indent_guides`: a thin line down each level of indent, the block the main caret is in having its own colour while the editor has the keyboard. Default: false.
- `matches`: the other places of the main selection's text, marked where they are in view (CodeMirror's `highlightSelectionMatches`):
  - `"selection"` (or `true`): those of a main selection on one line, at most 200 characters and not all spaces, in the case written, and where it is a whole word only where they are whole words (no letter, digit or `_` just before or after);
  - `"word"`: those and the places of the word a caret is in.

  There are at most 100 in view, the selection itself not among them. They are only drawn: the selection is as it was. Default: `"none"`.

### The caret

- `caret.shape`:
  - `"bar"` (before the character);
  - `"block"` (over the character, which is drawn again on it; a space wide at the end of a line);
  - or `"underline"`.

  Default: `"bar"`.

- `caret.height`: `"row"` (as tall as the row, as the current line's ground is) or `"text"` (as tall as the text). Default: `"row"`.
- `caret.width`: how wide the bar is, or how thick the line under the character, points (0.5 to 8). Default: 2.
- `caret.color`, `caret.text`: the caret's colour, and the colour of the character under a block. Default: cursor, background.
- `caret.radius`: how round the caret's corners are, a fraction (0 to 0.5) of a row's height, held to half of the caret's width and height. 0, the default, is square.
- `caret.glow`: the soft light round the caret, in its colour: one number (0 to 3) for how far it reaches and how dark it is together, 1 a terminal's cursor's. 0, the default, is none. It is drawn as the caret's `shadow` (no offset), so a host that draws no soft shadow draws none.
- `caret.blink`: whether the caret blinks (`true`, the default). One that does not is lit, and no frame is asked for it.
- `caret.blink_interval`: how long the caret is lit, and then dark, seconds (0.05 to 5, 0.5 by default; VS Code's `ViewCursors.BLINK_INTERVAL`).
- `caret.blink_for`: how long after the caret last moved it goes on blinking, seconds (0 to 3600, 15 by default), lit and still after it so that a page at rest draws no frames. 0: it never stops.
- `caret.unfocused`: what the caret is while the window or the pane does not have the keyboard, where this editor has it in the page:
  - `"hidden"` (the default);
  - `"hollow"` (a block is an outline as thick as a bar, with its character as it is; a bar and a line are as they are with the keyboard);
  - or `"solid"` (as with the keyboard).

  It does not blink. An editor the keyboard is not in draws no caret, whatever this says.

- `caret.motion`: how the caret goes from a place to another, on the host's clock:
  - `"snap"` (at once, the default);
  - `"ease"` (0.18 seconds, a terminal's, whatever the way);
  - `"spring"` (a spring with no overshoot, stiffness 30, a terminal's, that keeps its speed where the caret is sent on, so that a key held down is one motion);
  - `{ ease = seconds }` (0.05 to 1);
  - or `{ spring = stiffness }` (5 to 100).

  With Reduce Motion on, `"snap"`.

  A caret glides where:

  - it went to another place of the document;
  - the editor has as many carets as it had;
  - and both places are in the room.

  It is at its place at once anywhere else (a room that scrolls to it, a layout that moved it, the pointer selecting, an input method composing). It is lit while it glides.

### The selection

- `selection.radius`: how round the corners of a selection are, a fraction (0 to 0.5) of a row's height. 0, the default, is square.
  - Where the lines of a selection meet, the corners that turn inward are rounded too, as a terminal's selection is (`pencere_view::bands` is the rule, shared with a terminal's own).
  - It is a fraction and not points, so that the corner grows with the text.

### Colour options

- `colors.mark_hover`: what is laid over a gutter marker while the pointer is over it or it is pressed, where the marker says no colour of its own (the foreground at 30%).
- `colors.current_line`: the ground of the main caret's line, while the editor has the keyboard. Default: foreground at 5%.
- `colors.selection`, `colors.selection_blurred`: the ground of what is selected, while the editor has the keyboard and while it has not. Default: selection, selection at half.
- `colors.found`: the ground of what a search found. Default: warning at 30%.
- `colors.matches`: the ground of the other places of the main selection's text. Default: selection at 40%.
- `colors.brackets`: the ring around the bracket by the caret and the one that matches it. Default: dim at 60%.
- `colors.numbers`, `colors.current_number`: the numbers of the lines, and the caret's line's. Default: quiet, dim.
- `colors.gutter`: the ground of the gutter. Default: what the editor is on, else the background.
- `colors.folds`, `colors.folded`: the buttons that fold and the `⋯` after a folded line, and the ground of that `⋯`. Default: dim, foreground at 8%.
- `colors.whitespace`: the dots, arrows and boxes of the spaces and tabs shown. Default: border.
- `colors.indent_guides`, `colors.active_indent_guide`: the guides of the indent, and the guide of the main caret's block (`false`: drawn as the others are). With the guides' colour `false` and the block's not, the block's guide alone is drawn. Default: border, quiet.
- `colors.hint`: what the editor says while it is empty. Default: quiet.

### What an option takes

A colour is any colour of the page (a role's name, a value, `{ colour, alpha }`). `false` draws none (`colors = { current_line = false }`, or `caret = { color = false }` for an editor with no caret).

A number out of its range is taken to its nearest end.

An option an editor has no such name for is a mistake, and so is a word it does not take: the page fails where it says it.

The text is the box's colour and font (`:color`, `:text_size`, `:family`), and what the page decorates ([Marks, zones and searches](editor-marks.md#marks-zones-and-widgets)) is drawn over all of it.

The host blinks the caret, as the options of the editor that has the keyboard say (`caret.blink`, `caret.blink_interval`, `caret.blink_for`), and not with Reduce Motion on.

## What is not seen, the document and its places

What is **not seen** is drawn where the page asks for it, and an editor that says nothing of it, one for prose, shows none of it.

### Spaces and tabs

- A space shown is a dot in the middle of its place.
- A tab is an arrow as wide as it is (to its stop).
- Any other space (a no-break space, the spaces of typesetting, an ideographic space) is a box as wide as it is.

A `\r` before a line break and the other controls show nothing, and count as spaces for where a line's text ends.

### Guides

A line has a guide for each level of `tab_width` columns its indent reaches into (a tab as far as its next stop, a level part filled counted). A line with no text has those of the block it is in, as VS Code has them: the block the line with text above it opens, or the one the line with text below it ends. It has none at the top or the bottom of the document, nor in a run of more than 1 000 lines with no text.

The main caret's block is the one its line opens or ends, else the one its line's last guide is of, and goes on over the lines that show that guide (VS Code's `highlightActiveIndentation`).

A line that wraps has its guides on its first row (the rows after it start at the left), and the lines a fold hides show nothing.

### Where they are drawn

The spaces and tabs shown and the guides are both drawn over the grounds (the caret's line, what is found and selected, the lines and looks the page decorates) and under the text and the carets.

### The document and its language

The document of an editor lives as long as an editor of its name is on the page, hidden or not: a page of tabs hides the editors of the files not in front and keeps their documents.

The language is the page's or its program's. The editor:

- colours, marks and folds what the page says ([Marks, zones and searches](editor-marks.md#marks-zones-and-widgets));
- comments and indents as its options say;
- and knows no language itself.

### Places and versions

A place in a document is counted as [Counting](counting.md) says:

- a byte of the text, from 1;
- a line from 1;
- a range from one place to another, the end not in it.

A version is a number that grows by one with every change of the document, whoever made it.

### The caret and named ranges

The caret of an editor has a name of its own, `pencere.caret_of(id)`. An overlay anchored to it stands by the caret (`:anchor(pencere.caret_of("sql"), "below")`, a popup of completions, which goes above it where there is no room below). `pencere.layout.bounds` says where it is.

A range of the text has a name too, where the page gives it one (`{ from =, to =, anchor = "name" }`, [Marks, zones and searches](editor-marks.md#marks-zones-and-widgets)).

- `pencere.range_of(id, name)` is what an overlay stands beside as it stands beside the caret.
- `pencere.layout.bounds` is its box: from where the range starts across, as wide as its first row's part of it is, from the top of that row to the bottom of its last.
- The overlay is not shown while no part of the range is in the room, and is placed again when the text, the range or the scroll moves it.

A name belongs to its editor, an editor holds at most 64, and of a name said twice the later is the one. A name the page took away is no box.

A popup's keys (↑ ↓ ↩ ⇥ Esc) are commands of the page written only while the popup is open. A command that is there and not `enabled` is a key for the page, and so not the editor's.
