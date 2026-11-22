# Motion

A **motion** says how a change of an element moves instead of jumping, as a style sheet's
`transition` and a motion library's layout animation do: `seconds` (0.2), `ease` (`"out"`, the
default, `"linear"`, `"in-out"`, `"spring"`), and which of `color`, `opacity`, `size` (a width or
height in points) and `place` (moved among the others, it slides there) follow a change. Five more
follow a change when they are said: `rounded` (the radii of its corners), `border` (the thickness
of its border and its colour), `shadow` (how far it falls, how blurred it is and its colour),
`padding` (all four sides) and `text_size` (the size of its words, and of the words inside it that
have no size of their own: they are laid out again at each step, so a line that wraps wraps where
it would at that size). A spring (`ease = "spring"`) keeps its speed where the page changes what
it goes to, so a change in flight bends instead of starting over (a slide among the others and
every number it follows keep the speed they had), and `bounce` (0 to 0.9, 0: none, a number outside
is the nearest) makes it swing past where it goes and back. Its stiffness is 10 over `seconds` and
its damping 1 less `bounce`: with none it is at rest in `seconds`, and the more it bounces the
longer it takes, up to `seconds` times 0.7 over the damping where that is more than `seconds`; then
it is put where it goes. No colour, opacity or size it passes through is outside what it can be. `enter` and `exit` (`{ opacity, dx, dy, seconds }`, and for a picture or a canvas
`rotate` and `scale`) say how it comes in when it appears and how it goes when it is gone: an
element that went is drawn as it was while it goes, cut by the box that held it as that box is
now.

`auto_size` makes the width and the height a box is laid out at follow a change, whatever its style
gives them (the content's, a share of the parent's, points): an answer that opens to its lines and
closes (`size` said with it does nothing more). Every box inside it is laid out for the size the
box goes to and keeps its place and its size in it for the whole move (a line of text does not wrap
again as a box narrows), and the box shows what fits and cuts the rest while it is on its way: at
rest it cuts what the page says it cuts. What is under the box follows its edge; what has its own
`place` slides there of its own. A box inside another that is on its way is on its way of its own,
cut by the outer one, and what comes after it follows it. To close, the page leaves what is inside
out of the page and gives it an `exit` (`exit = { opacity = 1, seconds = 0.3 }` keeps it as it was
for the move): the box cuts what went, as it closes and for as long as it is drawn. A page does not close a box by giving it no
height, which leaves what is inside it to be read. Under Reduce Motion, on the first frame of a
page or on the frame it changes size, the box is its size at once. In a long list built for what is
in view (`Rows`) the rows are as tall as the page says at once, and a box that is a row, or in one,
goes over its time: if it grows by more than the rows kept beyond the view, the end of the list
shows a gap until it is there.

`element:transform({ rotate = degrees, scale = times })` turns a picture or a canvas clockwise,
and draws it larger or smaller, about its middle: its box and the boxes around it do not change,
and what is drawn larger is not cut off at its box, only by what holds it; a change of it moves when the motion says
`transform = true`. No other element has one: the page is told once, and the element is drawn as it
was. `spin` (seconds, more than 0) turns it round, clockwise, a turn in that many seconds, for as
long as it is on the page and with no id too: its picture or its canvas turns about its middle, its
box and its place stay, and the host draws a frame for each step (a spinner while something loads,
`image("loader.svg"):motion({ spin = 1 })`). `loop` is the same kind of thing and more:
`{ seconds, opacity, dx, dy, rotate, scale, ease, alternate }` goes from where the element is to
that shift and back, a turn of it in `seconds` (required; `ease` is `"in-out"` unless it says
`"linear"` or `"out"`, and `alternate = false` is one way, and again), for as long as it is on the
page and with no id too. Two loops of the same seconds are in the same phase,
whatever the time they began; a turn of one that fades (an `opacity` under 1) takes at least half a second, so that nothing
that loops flashes (one that only turns, moves or grows has no such least). `spin = s` is `loop = { rotate = 360, ease = "linear", alternate = false, seconds = s }`.
A loop and a spin are not drawn under Reduce Motion, nor while the element is hidden. What an
element does at once goes together: the turns add (the turn the page gave it, as it is on its way
where a change of it moves, what it comes in or goes out with, a loop and a spin), the distances add,
and the sizes and the opacities multiply.

The host does the moving, frame by frame, and does not ask the page for the frames in between;
the element is followed by its id, so one with no id does not move (but for a loop and a spin).
Nothing moves under Reduce Motion, on the frame the page changes size, or for what scrolling brings
into view or takes out of it. A field a kit does not know of a motion, an ease it has not, a number
that means nothing (the seconds of a spin or a loop, that are 0 or less) is left out of it and said
once in the program's log, and the rest moves: a page written for a later kit runs on this one
without it (`pencere.has` says whether a kit has what came after the first: `"element.transform"`,
`"motion.bounce"`, `"motion.rounded"`, `"motion.border"`, `"motion.shadow"`, `"motion.padding"`,
`"motion.text_size"`, `"motion.transform"`, `"motion.loop"`, `"motion.auto_size"`, and `"motion.shift.rotate"` and
`"motion.shift.scale"` for the turn and the size of an `enter` and an `exit`). A value of the wrong kind
(a word for a number) is the page's mistake, and is refused. A row of a list is
`motion({ place = true, enter = { dy = 6 }, exit = { dy = -3, seconds = 0.14 } })`; a hover is
`bg(hovered and "raised2" or nil):motion({ seconds = 0.12, color = true })`; a pill that bounces
into place is `motion({ ease = "spring", bounce = 0.3, size = true, place = true })`; a card that
rises is `motion({ shadow = true, rounded = true })`; an arrow that turns is
`image("arrow.svg"):transform({ rotate = open and 90 or 0 }):motion({ transform = true })`; a dot
that is alive is `motion({ loop = { seconds = 1.6, opacity = 0.3 } })`; a body that opens to its
lines is `motion({ seconds = 0.3, auto_size = true })`, its lines `motion({ exit = { opacity = 1,
seconds = 0.3 } })`.
