# Motion

A **motion** says how a change of an element moves instead of jumping.[^transition]

[^transition]: A style sheet's `transition` and a motion library's layout animation do the same.

## What a motion says

- `seconds`: how long a change takes (0.2).
- `ease`: how the change goes. It is `"out"`, the default, `"linear"`, `"in-out"` or `"spring"`.

## What follows a change

A motion says which of an element's things follow a change: `color` and `opacity`, and size and place in the next section. Five more follow a change when they are said.

### Size and place

- `size`: a width or height in points.
- `place`: moved among the others, it slides there.

### Corners, border and shadow

Three of the five come first.

- `rounded`: the radii of its corners.
- `border`: the thickness of its border and its colour.
- `shadow`: how far it falls, how blurred it is and its colour.

### Padding and text size

The other two.

- `padding`: all four sides.
- `text_size`: the size of its words, and of the words inside it that have no size of their own. They are laid out again at each step, so a line that wraps wraps where it would at that size.

## A spring

With `ease = "spring"`, a change in flight bends instead of starting over: the spring keeps its speed where the page changes what it goes to. A slide among the others and every number it follows keep the speed they had.

`bounce` (0 to 0.9) makes it swing past where it goes and back. 0 is none, and a number outside is the nearest.

- Its stiffness is 10 over `seconds`, and its damping 1 less `bounce`.
- With none of that bounce, it is at rest in `seconds`.
- The more it bounces, the longer it takes: up to `seconds` times 0.7 over the damping, where that is more than `seconds`. Then it is put where it goes.
- No colour, opacity or size it passes through is outside what it can be.

## Coming in and going out

`enter` and `exit` (`{ opacity, dx, dy, seconds }`) say how an element comes in when it appears and how it goes when it is gone. A picture or a canvas also takes `rotate` and `scale`. An element that went is drawn as it was while it goes, cut by the box that held it as that box is now.

## A box that opens and closes

`auto_size` makes the width and the height a box is laid out at follow a change, whatever its style gives them (the content's, a share of the parent's, points). An answer that opens to its lines and closes is one use (`size` said with it does nothing more).

- Every box inside it is laid out for the size the box goes to. It keeps its place and its size in it for the whole move. A line of text does not wrap again as a box narrows.
- The box shows what fits and cuts the rest while it is on its way. At rest it cuts what the page says it cuts.
- What is under the box follows its edge. What has its own `place` slides there of its own.
- A box inside another that is on its way is on its way of its own, cut by the outer one, and what comes after it follows it.

### Closing a box

- To close, the page leaves what is inside out of the page and gives it an `exit`. `exit = { opacity = 1, seconds = 0.3 }` keeps it as it was for the move.
- The box cuts what went, as it closes and for as long as it is drawn.
- A page does not close a box by giving it no height, which leaves what is inside it to be read.

### At once, and in a long list

- Under Reduce Motion, on the first frame of a page or on the frame it changes size, the box is its size at once.
- In a long list built for what is in view (`Rows`), the rows are as tall as the page says at once. A box that is a row, or in one, goes over its time.
- If such a box grows by more than the rows kept beyond the view, the end of the list shows a gap until it is there.

## A picture that turns

`element:transform({ rotate = degrees, scale = times })` turns a picture or a canvas clockwise, and draws it larger or smaller, about its middle.

- Its box and the boxes around it do not change.
- What is drawn larger is not cut off at its box, only by what holds it.
- A change of it moves when the motion says `transform = true`.

No other element has one: the page is told once, and the element is drawn as it was.

## What goes round and round

Two motions go on for as long as the element is on the page: a spin and a loop.

### Spin

`spin` (seconds, more than 0) turns the element round, clockwise, a turn in that many seconds. It goes on for as long as it is on the page, and with no id too. Its picture or its canvas turns about its middle; its box and its place stay. The host draws a frame for each step.[^spinner]

[^spinner]: A spinner while something loads is `image("loader.svg"):motion({ spin = 1 })`.

### Loop

`loop` is the same kind of thing, and more. `{ seconds, opacity, dx, dy, rotate, scale, ease, alternate }` goes from where the element is to that shift and back, a turn of it in `seconds`. The seconds are required. It goes on for as long as it is on the page, and with no id too.

- `ease` is `"in-out"` unless it says `"linear"` or `"out"`.
- `alternate = false` is one way, and again.
- Two loops of the same seconds are in the same phase, whatever the time they began.
- A turn of one that fades (an `opacity` under 1) takes at least half a second, so that nothing that loops flashes. One that only turns, moves or grows has no such least.

`spin = s` is `loop = { rotate = 360, ease = "linear", alternate = false, seconds = s }`. A loop and a spin are not drawn under Reduce Motion, nor while the element is hidden.

### What goes together

What an element does at once goes together.

- The turns add: the turn the page gave it, as it is on its way where a change of it moves, what it comes in or goes out with, a loop and a spin.
- The distances add.
- The sizes and the opacities multiply.

## Who does the moving

- The host does the moving, frame by frame. It does not ask the page for the frames in between.
- The element is followed by its id, so one with no id does not move (but for a loop and a spin).
- Nothing moves under Reduce Motion, on the frame the page changes size, or for what scrolling brings into view or takes out of it.

## What a kit leaves out

Three things are left out of a motion, and said once in the program's log:

- a field a kit does not know of a motion;
- an ease it has not;
- a number that means nothing (the seconds of a spin or a loop, that are 0 or less).

The rest moves: a page written for a later kit runs on this one without it. `pencere.has` says whether a kit has what came after the first: `"element.transform"`, `"motion.bounce"`, `"motion.rounded"`, `"motion.border"`, `"motion.shadow"`, `"motion.padding"`, `"motion.text_size"`, `"motion.transform"`, `"motion.loop"`, `"motion.auto_size"`, and `"motion.shift.rotate"` and `"motion.shift.scale"` for the turn and the size of an `enter` and an `exit`.

A value of the wrong kind is the page's mistake, and is refused.[^kind]

[^kind]: A word for a number is one.

## Examples

- A row of a list is `motion({ place = true, enter = { dy = 6 }, exit = { dy = -3, seconds = 0.14 } })`.
- A hover is `bg(hovered and "raised2" or nil):motion({ seconds = 0.12, color = true })`.
- A pill that bounces into place is `motion({ ease = "spring", bounce = 0.3, size = true, place = true })`.
- A card that rises is `motion({ shadow = true, rounded = true })`.
- An arrow that turns is `image("arrow.svg"):transform({ rotate = open and 90 or 0 }):motion({ transform = true })`.
- A dot that is alive is `motion({ loop = { seconds = 1.6, opacity = 0.3 } })`.
- A body that opens to its lines is `motion({ seconds = 0.3, auto_size = true })`, its lines `motion({ exit = { opacity = 1, seconds = 0.3 } })`.
