# Layout and scrolling

`pencere.Ui`, `pencere.Fades`, `pencere.Rows` and `pencere.Built` keep what a page needs between frames:

- pointer and keyboard state;
- eased levels;
- the heights of a long list;
- and which of its rows are built.

### Fades

A fade takes a highlight's 0.12 s, or its own time (`fades:to(id, level, seconds)`). It is read along a curve where one is given (`fades:level(id, "out")`; `pencere.ease(t, ease)` is the curve).

## The box of an element

Every element is three boxes, each inside the one before it, as a style sheet's are with `box-sizing: border-box`:

- the **border box** is the element's room on the page. `w`, `h`, `min_w`, `max_w`, `min_h` and `max_h` give its size, and a box that has none of them is as large as what it holds with its border and padding added. Its fill is drawn in all of it, and `rounded` rounds it;
- the **padding box** is the border box less the border (`border(width, color)`: one width on every side), which takes room as the padding does;
- the **content box** is the padding box less the padding (`p`, `px`, …). What the box holds stands in it: its children, its words, its picture, a sparkline or a canvas.

A box of `w(100)` with `border(2)` and `p(4)` is 100 across and its children start 6 in from its edge and are 88 across. A box is never smaller than its border and padding.

An element is drawn in this order: its shadow, its fill, its border, then what is inside it. What is inside starts at the padding box, so it does not cover the border.

What a page and a host are told is these boxes. `bounds` is the border box. The boxes of a text field are the element's own three ([Host](host.md#text-fields)). A browser is a replaced element, as in a style sheet: its room is the content box, with the border and the padding of its element around it. An editor is a box that scrolls: it draws and cuts at the padding box, its text starts at the content box (and the editor's own `padding` after that, [The editor](editor.md#the-room-and-the-gutter)), and what the element has of padding goes with the text when it scrolls. `pencere.has("layout.replaced_content_box")` names this. A box that scrolls has the border box for its view, and its content reaches as far as the end of its padding: it scrolls to the end of the padding, inside its border.

A box that says `clip()` or `scroll()` cuts what is inside it at its padding box, as `overflow` does in a style sheet, and where it is `rounded` the cut has the corners of the padding box: the radius of a corner less the width of the border (never below nothing), so that what is inside never shows past the inner edge of the border. A box that scrolls moves its padding with its content, inside the border. What is cut is not drawn and does not answer the pointer: a press in a corner that is cut is not a press in what is there.

The bars of a box that scrolls stand in its padding box, inside the border, 2 points in from its edge (they are drawn as the system's are, `Bars`), over its content: a press on the border is not a press on a bar, and a bar is where the padding box ends, not the border box.

The cut is a rectangle and, of the innermost box that clips and has rounded corners, their shape (`Clip`): a box that clips and is square cuts the rectangle and leaves the corners of the rounded one around it, and where two rounded boxes clip, the outer cuts as a rectangle does and the inner has its corners. A box that is cut is cut by every box that clips around it, and an overlay is cut by none of them.

The cut may be **soft** at some of its edges (`Clip::fade`): a box that fades (an editor's text where a line goes past it, a text that is cut with `truncate("fade")`) says its rectangle and how far in from each of its sides the fade goes. What is drawn through the clip is there in full that far in from an edge that fades, less towards the edge, and not there at it, in a straight line between; where two edges fade, at a corner, the two are multiplied. As with the corners, only the innermost box that fades fades what is inside it, and a box that is cut further (by a box inside it) leaves the soft edge where it was. A soft edge does not take the pointer's place away: what thins out can still be pressed.

A box that is `absolute` stands against its parent's padding box, as in a style sheet (its `inset` is from the border in), an `overlay` against the page, and a box that is anchored beside the border box of its element.

### Standing on a baseline

A row that says `items_baseline()` stands its children on one baseline, as `align-items: baseline` does in a style sheet: the first line of words of each child (a text, a text of many looks, a box that has them in it, a host's text field) is on the same line, whatever size its words are, and the row is as tall as its children reach above that line and below it. A child that has no words (a picture, an empty box) has its bottom edge on the line. In a column `items_baseline()` puts the children at the start of the cross axis, as it does in a style sheet. `pencere.has("element.items_baseline")` names it.

The baseline of a line of words is `ascent` below the top of its line box (`Metrics::ascent`, [Host](host.md#the-baseline-of-words)), and the line stands in the middle of its box, or at the top of it where the lines do not all fit in a text that wraps: the layout is told the baseline from the place the words are drawn from, so that they stand on the line the row aligned. A text of many looks has the baseline of the piece it begins with. A box is laid out in whole pixels, so words that are not as tall as the pixels they stand in are at most half a pixel from the line of their row. The baseline of a host's text field is the first line of its text in the face the kit measures the field in ([Text fields](host.md#text-fields)): where the host draws the field in another face, a row of a label and a field may be a few points off.

### Where the kit leaves the style sheet

| A style sheet                                                                   | The kit                                                                                                                                                                                                               | On purpose                                                                                                                                                  |
| ------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `box-sizing` is `content-box` unless it is said                                 | Every element is `border-box`, as in a reset that sets it for all; there is no `content-box`.                                                                                                                         | Yes. An option may come, if a page needs one.                                                                                                               |
| A border has a style (solid, dashed, …), and a width and a colour for each side | One width and one colour on every side, solid. The corners are each their own (`rounded_corners`).                                                                                                                    | Yes.                                                                                                                                                        |
| The bars of every box that scroll are alike (whatever the system says of them)  | A host's field of lines has the host's own bar. In bateri it is macOS's, which is always laid over the text: the system's "always show scroll bars" is not applied in it, as it is to the bars of a box that scrolls. | Yes. A field of lines is the host's own, and its bar is the system's: laid over the text, so that a column at the edge does not narrow the text it scrolls. |
| A field has the text size of its element (`font-size`)                          | A host's field takes the terminal's face at the kit's text size (`ZOOM_TEXT`) times the host's zoom and ignores the element's `text_size`.                                                                            | No.                                                                                                                                                         |
| Every box that clips cuts with its own corners, however many are around it      | One rounded corner shape at a time: the innermost box that clips and is rounded. The outer ones cut as rectangles. A host draws one rounded cut for each thing it draws.                                              | Yes. No screen is known with two rounded cuts that meet at a corner.                                                                                        |
| A focus ring is an `outline`, which takes no room                               | There is no `outline`. A page rings an element with a border, which takes room: the library's controls keep a point of their padding for it, so that the ring does not change their size.                             | No: the ring will be drawn over the box, not in it.                                                                                                         |

## What is where

`pencere.layout` answers what is where on the page as it was last laid out:

- `hit`, `ids_at`;
- `bounds` (the [border box](#the-box-of-an-element); a field's too, by its name);
- `scroll_extent`:
  - `content` and `view` down, `content_w` and `view_w` across;
  - `max_offset` and `max_offset_x`;
  - how far it is scrolled, `offset` and `offset_x`;
  - for an editor, `row`: how tall one of its rows is, points (nil until it is laid out, and for any other box);
- `focus_order`, `path_of`;
- `drag_source` and `drop_target` (the topmost element at a point that can be dragged, and that what is dragged can be put down on).

They are defined by the prelude.

## Reading the text of an editor

The text of an editor is read at once, from what the host posted of it before it told the page of the version it is:

- `pencere.layout.editor_version(id)`: the version read, the one the last `edit` said;
- `editor_length(id)`: the bytes of the text;
- `editor_lines(id)`;
- `editor_text(id, from, to)`: at most 1 MiB a read. A large document is read in pieces;
- `line_start(id, line)`, `line_of(id, at)`;
- `word_at(id, at)`: the word at a place, or the run of marks or of spaces, as a double click takes it: `{ from =, to = }`. It is nil past the end of the text, so that a page that asks for what is at its caret asks of the word the pointer rests on by the same rule.

A place that is inside a character is an error at the line of the script, not nil and not the character's.

A page reads in the `edit` event the text of the version the event is of.

## How the host's terminal draws

How the host's terminal draws is told by the host and applied by the page.

### What the host tells

`pencere.terminal_look()` says how the host's terminal draws its cursor and its selection, as the person set them, in the words and units of an editor's options: `{ caret = { shape =, width = (points), radius =, glow =, blink =, blink_interval = (seconds), unfocused =, motion = }, selection = { radius = } }`.

Each field is there only where the host has such a setting (`nil` is the kit's own default for an editor, and a host that has no such settings says none: `{}`).

The colours are not in it (the theme's roles are the host's to resolve), nor Reduce Motion (`render` has it).

It is read as it is asked, so it is what the person has set now. The page is told `{ type = "terminal_look" }` ([Events](events.md#what-a-page-is-told)) when it changes.

### What the page does with it

**The host applies none of it to the page's editors**: the page decides what they take, and an editor is as its options say, whatever the host's terminal looks like.

### What the host draws

`pencere.draws()` says what the host draws that not every host does, as a table of names that are true:

- `shadow` where it draws a box's soft shadow (a caret's `glow` is one);
- `cells` where the page is drawn in the character cells of a terminal (what is small is best a character, and a size a cell would round to nothing is kept off);
- `coarse_images` where a picture is drawn in too few pixels to read a small one by (a terminal with no graphics protocol, which draws it in half blocks), so that an icon is best said in characters;
- `{}` where it draws only what every host does.

It is the same every time it is asked.[^terminal]

The library (`@pencere/ui`) follows `cells` itself (its glyphs are characters, a table's head and a palette's rows are rows, a status bar guesses a word a column wide). `coarse_images` is the page's to read, for the pictures it has of its own (an icon of its bundle).

[^terminal]: The terminal host of this repository draws pictures, with `shadow`, and says neither `cells` nor `coarse_images`. They are for a host that draws in characters.

### Editors that look as the terminal does

A page that wants its editors to look as the person's terminal does gives them all of it where it says nothing, with `@pencere/terminal` ([The kit's library](library.md#pencereterminal)).

## Timers

Timers run when the host wakes the page (a reply's `wake`, [Replies](replies.md)), and are named as the program runner's.

### Setting a timer

- `pencere.time.after(seconds, fn)` runs `fn` once, `seconds` (0 or more) from now.
- `pencere.time.every(seconds, fn)` runs it every `seconds` (more than 0) from then on, once, and not once for each run it fell behind on.

Both return a timer: `:cancel()` stops it (`true` where it was still waiting) and `:active()` says whether it waits. A timer has nothing else a script can read.

### The time

- `pencere.time.now()` is the time the host last gave the page, `render`'s `now` or a wake's (0 before any). It does not go on between them.
- `pencere.time.wall()` is the time of day, as the host's system says it: the seconds of Unix time (since 1970-01-01 00:00:00 UTC, leap seconds not counted) and how far the person's local time is ahead of UTC, seconds.

  It is read as it is asked and goes on within a call. It is what "4 minutes ago" and "14:32" are told from, and no clock to time a page's work by (the person may set it).

### When a timer runs

The kit asks for the wakes its timers want with the page's replies and frames. When the page is woken the kit runs the timers that are due, in the order of their times (and of their making). The kit does so before `handle` is told of the wake, which it is only where the page asked a `wake` itself.

What `fn` returns is a reply, put together with the other timers' and the page's:

- every ask of an editor, in order;
- a frame where any wants one;
- of what says one thing (the keyboard, the pasteboard, a link), the last;
- the earliest wake.

A timer set while the page is woken counts from the wake's time, and one it sets runs at a later wake, not at that one.

A timer set anywhere else (an event, a message, `render`, the script's start) counts from the next wake, which the kit asks for at once. The time the page last had may be that of a frame long gone, so a timer is late by at most a frame and never early.

One set in `settle`, which has no reply, has the page rendered again, and its frame asks.

An error in `fn` is the page's, with its line. A page that sets none and asks no `wake` is asked and told nothing new.

## What the kit puts together

What the kit puts together, it holds to what a reply may hand the editors ([What a host does for a kit view](host.md): 1 MiB of text, 20 000 decorations).

What the page's parts return (`syntax:reply`, the timers, the page's own reply) is one reply. The decorations that would take it past that wait for the page's next reply, which the kit asks a wake for at once.

They wait in their order, and with each of them every decoration after it for the same editor, so that what a layer is given last is what it has. They are made against a version, which the editor carries over what is done in between.

Nothing else waits. A load, a change, a selection or a command goes when the page asks it. An ask too large for a reply by itself goes as it is, for the host to refuse ([What a host does for a kit view](host.md): the page fails, as a module's would).

How replies are put together is `pencere.merge`'s ([Replies](replies.md)).

## A long list

A long list builds only the rows near what is in view, and keeps the rest of its length with a spacer above and one below.

### Making a list

`box:list(rows, make)` does all of it. In a box with a name that scrolls, a column with no padding above and no gap between its children, it puts a spacer, the rows near what is in view (`make(i)` makes row i) and a spacer, and returns the box.

A box that cannot hold a long list (no name, no scroll, a row or a grid, padding above, a gap) is refused by name when the page is made.

### How far ahead rows are made

The host moves what is built at once. The page builds what comes into view a frame or two later.

So two views are made ahead of what is in view, the way the list goes, and half a view behind (as much both ways before it has moved, all of it the other way at an end of the list, which can only go back).

The same rows are made again while they hold what is in view with a view to spare the way the list goes.

### The frame a list needs

The kit asks for the frame a list needs. A `scrolled` event that takes a list near the end of the rows made draws the page again, whatever `handle` answers. So does a layout that gives a list a taller window than its rows fill, or one less than three quarters of the window its rows were built for (a list is built before its box is laid out, for the page's height, and again for its own).

A list that goes less than a view a frame finds its rows built, and the page is built about once a view, not at every turn of the wheel.

A list the page asked past its end (a log that is followed) is built at the end of the rows it is given, though it grew since it was laid out.

Before the box is laid out its window is taken as the page's height. A list the page stops building is forgotten, and is built afresh when it comes back.

`pencere.list_window(id)` is the first and the last row the list made and how tall its window is (what a page asks its program for, a window at a time, and how far a page of rows goes).

### Where the rows stand

`pencere.Rows` is where a list's rows stand. It is made with:

- `Rows.new(heights)`;
- `Rows.from_tops(tops)`: where each row starts, from 0, the last entry where the last row ends. It is a table the page keeps as the list grows, taken as it is;
- `Rows.even(count, height)`: rows of one height, as long as any list.

It has:

- `:padded(lead, trail)` for room around them;
- `:window(offset, view, overscan)`: the rows that touch a window;
- `:at(y)`, `:top(index)`, `:total()`;
- `:reveal(index, offset, view)`: the offset that shows a row.

`pencere.Built` (`Built.new()`, `built:span(rows, offset, view)`, `built:stale(rows, offset, view)`) is what `:list` decides with, for a page that builds a list its own way.

## Scrolling

How far a box is scrolled is the **host's**, not the page's.

A box that says `scroll()` and has an id is moved by the wheel, the trackpad and its scroll bars as the host's system moves them (its speed, direction and momentum; where the system does, a box pulled past its end springs back, and the bars show as the system's do). It is moved at once and on the page as it was last laid out: the page is not asked.

The page is told where its boxes went (`scrolled`, [Events](events.md#what-a-page-is-told)) and builds what came into view.

### Reading and asking

- `pencere.layout.scroll_offset(id)` is how far a box is scrolled (down, then across), as the page was last told or asked.
- `pencere.layout.scroll_to(id, { y =, x =, animate = })` asks the host to scroll it. A way not given stays where it is, past the end is the end, and `animate` glides there unless the person asked for less motion. It is done before the next frame is laid out.

A box with no id does not scroll.

### Boxes that scroll together

`scroll_link(name, axis)` is an option of a box that scrolls (an editor is one). The boxes that say the same name go together, along `axis`: `"y"` (the default), `"x"` or `"both"`.

- Where one of them is scrolled, the others are scrolled to the same offset **in the same frame**, whatever moved it: the wheel, the trackpad, a bar, `scroll_to` or a caret brought into view. The page is not asked, and is told where all of them went (`scrolled`).
- Each stops at its own end. The offset of the box that was moved is kept for the group, so that a box that stopped goes on to it when it can (its content grew) and the boxes stand together again when the one that was moved comes back within the reach of the shorter.
- A turn that moves no box (the wheel over a box at its end, forward) moves none of the others.
- The pull past an end that the person holds is the held box's alone: the others are drawn at their ends.
- Where more than one box was moved in a frame, the one with the lowest name leads.
- A box with no id does not scroll and is linked to none. A page asks `pencere.has("element.scroll_link")` first.

A page that keeps two views level, such as the two sides of a diff, says one `scroll_link` on both and needs no `scrolled` handler and no scroll offsets of its own: that handler answered a frame late and had to tell its own `scroll_to`'s echo from the person's.

## Zoom

A box that scrolls can also say `zoom()` (`zoom({ min =, max =, smart =, wheel = })`), and the host zooms what it holds:

- a pinch makes it larger or smaller about the point between the fingers (that point of the content stays where it is under them);
- the wheel with the command key held does the same by the turn (`wheel` is how far one point of it goes: the scale is multiplied by `exp(-dy * wheel)`);
- a double tap with two fingers goes between the size the page gave (1) and `smart` (2).

The scale stays between `min` (1) and `max` (4). Where the system does so, a pinch past them is pulled as a box is past its end, and springs back.

### What is laid out

The box stays the size the page gave it and what it holds is **laid out** at the scale, as the page's own points are: a word in a larger face, a picture asked at its larger size (so that it is sharp), a click where it is drawn. A host does not stretch a drawing of the page.

### What the page is told and asks

The page is told the scale with where the box went (`scrolled`'s `zoom`, 1 for a box that does not zoom) and reads it with `pencere.layout.zoom_of(id)` (and `scroll_extent(id).zoom`).

`pencere.layout.zoom_to(id, scale, { at = { x =, y = }, animate = })` asks for one. `at`, a point of the page, stays where it is: the middle of the box where none is given. `animate` is as `scroll_to`'s. Past the range is its end. It is done before the next frame is laid out.

A pinch or a double tap no box that zooms took is the page's (`magnify`, `smart_magnify`).

### What a page cannot say

A page that says what goes against any of these is refused:

- a box that zooms scrolls;
- an editor or a browser is not inside a box that zooms nor one itself;
- and a long list (`:list`) is not on a box that zooms.

### Who says what

The constants that are the page's to say are its own (the range, the size a double tap goes to, how far a turn goes). How long a glide takes and how a pull past the range comes back are the host's, as they are for a scroll.

## What a Ui keeps

A page reads what its `Ui` keeps as fields:

- `ui.hover`: the id of the pressable element the pointer is over;
- `ui.pressed`: the one a button went down on and is held on (nil when none is);
- `ui.focus`: the element the keyboard is on (nil before any).

It changes them through the methods:

- `point(layout, at)`: the pointer moved, or left with `at` nil;
- `press` and `release`: a button went down and came up. `release` gives the element clicked;
- `focus_on(id)`;
- `focus_step(layout, back)`: to the next element that takes the keyboard, or the one before.

Each that takes the pointer says whether anything it keeps changed, so a page draws a frame only for what it sees.

`key_path(layout)` is where a key goes, the focused element and the named ones around it.

### A drag

`Ui` keeps a drag:

- `press` on an element that can be dragged begins one;
- `point` makes it a drag once the pointer has moved `pencere.DRAG_START` (4) points, and follows it;
- `dragging()` is `{ source =, at =, target = }` while it goes (`target`, the drop target under the pointer, is never the element dragged);
- `release` puts it down (it is no click), and `take_drop()` then gives `{ source =, target =, at = }` once;
- `cancel_drag()` gives it up (the escape key).

When the system's drag of a `carry` ends (the `carried` event, [Events](events.md#what-a-page-is-told)), every `Ui` of the page lets go of what the pointer held, whether the page passes the event on or not. The drag and the element pressed are gone, nothing is dropped and no click is made, and `carried()` says whether there was anything.
