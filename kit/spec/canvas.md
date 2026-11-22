# Canvas

A **canvas** (`pencere.canvas(paths)`) is a box with paths drawn in it, in its own points from its
top-left corner. A path (`pencere.path()`) is steps — `move_to(x, y)`, `line_to(x, y)`,
`quad_to(cx, cy, x, y)`, `cubic_to(ax, ay, bx, by, x, y)`, `close()`, and `rect(x, y, w, h)`,
`arc(cx, cy, r, from, to)` (degrees clockwise from the top: a ring that fills), `circle(cx, cy, r)`
made of them — with a colour inside it (`fill`: where a line out of a point crosses the path an odd
number of times) and a line along it (`stroke(width, color)`, round where it turns and ends). A path
is drawn smoothed at its edges; a **crisp** one (`crisp()`) is not: a pixel is the path's where its
middle is inside it (or under its line) and is otherwise untouched. A host draws a point of the page
as large as its own text is to the kit's ([What a host does for a kit view](host.md)), seldom a whole number of pixels, so the edges
of shapes laid on a grid of points fall between pixels: smoothed, each such edge is a pixel of part
colour and two shapes that touch leave a faint seam; crisp, the pixels are whole and the shapes meet,
some a pixel wider than others. It is for dots of a dithered fill, pixel art and bars that are to
meet; a line thinner than a pixel may not be drawn at all. The canvas has no size of its own; its
style gives it one.

A path can also glow, be painted with a gradient, mix with what is under it, be dashed and be moved,
and every host draws these alike, with the one rasteriser. A page that wants them asks first whether
they are there (`pencere.Path.shadow ~= nil`; [Versions](versions.md)).

- **A shadow** (`shadow(dx, dy, blur, color)`) is what the path leaves behind it: its fill and its
  line moved `dx` and `dy` points, blurred over `blur` points (a deviation of half of it, as a box's
  `shadow` has it, but `0` is not blurred at all where a box's is blurred a little, and `64` is the
  most), in `color`, and drawn under the path in the
  path's blend. With no offset and a bright colour it is a **glow**. The offset and the blur are in
  the points of the canvas **after** the path's transform, so that the glow of a turned shape stays
  round; a canvas that spins turns the offset with its picture. A shadow is made of the canvas's own
  pixels, so it is cut at the canvas's box: a page leaves it room (a shape half off the box still
  casts as much of its shadow on it as a whole one). The shadow's opacity is the colour's; the
  path's own fill does not change it. The shadows of one canvas are made in layers that come to at
  most 33 554 432 pixels together (a square of about 5800 on a side): a shadow that would go past
  that is not drawn, and its path is.
- **A gradient** is what a `fill` or a `stroke` is painted with in place of a colour:
  `pencere.linear(x0, y0, x1, y1, stops)` along the line from one point to the other,
  `pencere.radial(cx, cy, r, stops)` out from a middle to a radius from it. The stops are
  `{ { 0, "accent" }, { 1, "background" } }`: an offset from `0` at the start to `1` at the end and a
  colour in any form of [Colours](colours.md), at most 16 of them. The colour is mixed in sRGB between the stops,
  its opacity with it, and goes on as the first or the last stop's before and past the gradient. An
  offset outside `0` to `1` is taken at the nearest end, and one less than the stop before it at
  that one. The points are the path's own, so the gradient goes where the path's transform takes it.
  A gradient with one stop is that colour; one that cannot be drawn (a point of it is not a number, its two
  points are one, its radius is nothing) is its last stop's colour all over.
- **A blend** (`blend("normal" | "add" | "screen" | "multiply")`, `"normal"` where a path says none)
  is how the path is drawn over what the same canvas has drawn under it: `add` adds the colours
  (lights that overlap grow brighter), `screen` and `multiply` are the two blends of those names of
  a style sheet's `mix-blend-mode`. The page behind the canvas is no part of it, and a path with
  nothing drawn under it is its colour as it is, in any blend: a glow on a dark ground is the page's
  to put under the canvas, as the canvas's first path.
- **A line's own look**: `dash(pattern, offset)` breaks it into dashes and gaps of the lengths in
  `pattern`, `{ 4, 2 }` is four points of line and two of none, in turn, from `offset` points into
  it, at most 16 lengths; a list of an odd number is read twice over (as a style sheet's
  `stroke-dasharray` is), and one that is empty, comes to nothing (or to less than a tenth of a
  point), has a length that is less than nothing or not a number, or would break the line into
  more than a hundred thousand dashes is a line with no gaps.
  `cap("round" | "butt" | "square")` is how its ends are drawn and `join("round" | "miter" | "bevel")`
  how it is where it turns, `"round"` both where a path says neither; a `"miter"` turn that is so
  sharp that its point would be longer than four times half the line's width is a `"bevel"`. All
  three go before or after `stroke`, and a line is drawn only where there is a `stroke`.
- **A transform** (`transform({ scale =, rotate =, translate =, origin = })`) says where the path is
  drawn: scaled (`scale`, a number or `{ sx, sy }`), then turned (`rotate`, degrees clockwise, as
  `arc` goes), both about `origin` (`{ x, y }`; the canvas's top-left corner where there is none),
  then moved (`translate`, `{ x, y }`). The line's width, its dashes and a gradient go by it too, as
  they do in a style sheet's `transform`. Saying it again replaces it. The path's points are not
  changed: a canvas that spins turns the transform and not the points.
