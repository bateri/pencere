# Layout and scrolling

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
said), `editor_length(id)` (the bytes of the text) and `editor_lines(id)`, `editor_text(id, from, to)` (at most 1 MiB a read;
a large document is read in pieces), `line_start(id, line)`, `line_of(id, at)` and `word_at(id, at)` (the word at a place, or the run of
marks or of spaces, as a double click takes it: `{ from =, to = }`; nil past the end of the text, so that a
page that asks for what is at its caret asks of the word the pointer rests on by the same rule). A place
that is inside a character is an error at the line of the script, not nil and not the character's. A page reads in
the `edit` event the text of the version the event is of.

**How the host's terminal draws** is told by the host and applied by the page. `pencere.terminal_look()`
says how the host's terminal draws its cursor and its selection, as the person set them, in the words
and units of an editor's options: `{ caret = { shape =, width = (points), radius =, glow =, blink =,
blink_interval = (seconds), unfocused =, motion = }, selection = { radius = } }`, each field only where
the host has such a setting (`nil` is the kit's own default for an editor, and a host that has no such
settings says none: `{}`). The colours are not in it (the theme's roles are the host's to resolve) nor
Reduce Motion (`render` has it). It is read as it is asked, so it is what the person has set now; the
page is told `{ type = "terminal_look" }` ([Events](events.md#what-a-page-is-told)) when it changes. **The host applies none of it to the
page's editors**: the page decides what they take, and an editor is as its options say, whatever the
host's terminal looks like. `pencere.draws()` says what the host draws that not every host does, as a
table of names that are true: `shadow` where it draws a box's soft shadow (a caret's `glow` is one);
`cells` where the page is drawn in the character cells of a terminal (what is small is best a
character, and a size a cell would round to nothing is kept off); `coarse_images` where a picture is drawn
in too few pixels to read a small one by (a terminal with no graphics protocol, which draws it in half
blocks), so that an icon is best said in characters; `{}` where it draws only what every host does; the
same every time it is asked. (The terminal host of this repository draws pictures, with `shadow`, and
says neither `cells` nor `coarse_images`; they are for a host that draws in characters.) The library (`@pencere/ui`) follows `cells` itself (its glyphs are
characters, a table's head and a palette's rows are rows, a status bar guesses a word a column wide);
`coarse_images` is the page's to read, for the pictures it has of its own (an icon of its bundle). A page that
wants its editors to look as the person's terminal does gives them all of it where it says nothing,
with `@pencere/terminal` ([The kit's library](library.md#pencereterminal)).

**Timers** run when the host wakes the page (a reply's `wake`, [Replies](replies.md)), and are named as the
program runner's. `pencere.time.after(seconds, fn)` runs `fn` once, `seconds` (0 or more) from now;
`pencere.time.every(seconds, fn)` every `seconds` (more than 0) from then on, once and not once for
each for runs it fell behind on. Both return a timer: `:cancel()` stops it (`true` where it was still
waiting) and `:active()` says whether it waits; a timer has nothing else a script can read. `pencere.time.now()` is the time the host last gave
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

**What the kit puts together, it holds to what a reply may hand the editors** ([What a host does for a kit view](host.md): 1 MiB of
text, 20 000 decorations): what the page's parts return (`syntax:reply`, the timers, the page's own
reply) is one reply, and the decorations that would take it past that wait for the page's next
reply, which the kit asks a wake for at once. They wait in their order, and with each of them every
decoration after it for the same editor, so that what a layer is given last is what it has; they
are made against a version, which the editor carries over what is done in between. Nothing else
waits: a load, a change, a selection or a command goes when the page asks it, and an ask too large
for a reply by itself goes as it is, for the host to refuse ([What a host does for a kit view](host.md): the page fails, as a
module's would). How replies are put together is `pencere.merge`'s ([Replies](replies.md)).

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
window than its rows fill, or one less than three quarters of the window its rows were built for
(a list is built before its box is laid out, for the page's height, and again for its own). A list that goes less than a view a frame finds its rows built, and the
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
The page is told where its boxes went (`scrolled`, [Events](events.md#what-a-page-is-told)) and builds what came into view.
`pencere.layout.scroll_offset(id)` is how far a box is scrolled (down, then across), as the page was
last told or asked; `pencere.layout.scroll_to(id, { y =, x =, animate = })` asks the host to scroll
it (a way not given stays where it is, past the end is the end, `animate` glides there unless the
person asked for less motion), done before the next frame is laid out. A box with no id does not
scroll.

A box that scrolls can also say `zoom()` (`zoom({ min =, max =, smart =, wheel = })`), and the host
zooms what it holds: a pinch makes it larger or smaller about the point between the fingers (that
point of the content stays where it is under them), the wheel with the command key held does the
same by the turn (`wheel` is how far one point of it goes: the scale is multiplied by `exp(-dy *
wheel)`), and a double tap with two fingers goes between the size the page gave (1) and `smart`
(2). The scale stays between `min` (1) and `max` (4); where the system does so, a pinch past them
is pulled as a box is past its end, and springs back. The box stays the size the page gave it and
what it holds is **laid out** at the scale, as the page's own points are: a word in a larger face,
a picture asked at its larger size (so that it is sharp), a click where it is drawn; a host does not
stretch a drawing of the page. The page is told the scale with where the box went (`scrolled`'s
`zoom`, 1 for a box that does not zoom) and reads it with `pencere.layout.zoom_of(id)` (and
`scroll_extent(id).zoom`); `pencere.layout.zoom_to(id, scale, { at = { x =, y = }, animate = })`
asks for one (`at`, a point of the page, stays where it is: the middle of the box where none is
given; `animate` as `scroll_to`'s; past the range is its end), done before the next frame is laid
out. A pinch or a double tap no box that zooms took is the page's (`magnify`, `smart_magnify`). A
box that zooms scrolls, and an editor is not inside one nor one itself, and a long list (`:list`) is
not on one: a page that says any of these is refused. The constants that are the page's to say are
its own (the range, the size a double tap goes to, how far a turn goes); how long a glide takes and
how a pull past the range comes back are the host's, as they are for a scroll.

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
`{ source =, target =, at = }` once, and `cancel_drag()` gives it up (the escape key). When the
system's drag of a `carry` ends (the `carried` event, [Events](events.md#what-a-page-is-told)) every `Ui` of the page lets go of
what the pointer held, whether the page passes the event on or not: the drag and the element pressed
are gone, nothing is dropped and no click is made, and `carried()` says whether there was
anything.
