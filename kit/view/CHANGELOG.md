# Changelog

All notable changes to the `pencere-view` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

The engine of version `2.0` of the kit (`pencere:view@2.0.0`, `lua` `2.0`).

### Added

- **Overlays.** `overlay()` takes an element out of the flow and out of every box that holds it: it
  is placed against the page by its `inset`, drawn after everything else, cut by no box around it
  and found by the pointer first, and is still as faint as what holds it, hidden with it, and on the
  path a key takes out of it. `anchor(id, Side)` places one beside another element (`Side::Below`,
  `Above`, `Right`, `Left`), with `anchor_gap` and `anchor_align`; it goes on the other side where
  the page has not the room, and is moved along the side to stay on the page.
- **Text of many looks.** `rich([span("…"), …])`: one paragraph of parts (`Span`), each with its own
  colour, weight, italic, underline, strike, face, a ground behind its words (`bg`) and a `link(id)`
  that answers the pointer by that name (`Laid::hit`) and is read as a link. It wraps across the parts
  and does not break a word made of two of them; one that does not wrap is cut with "…" inside the
  part that runs over.
- **Canvases.** `canvas([path()…])`: paths of `move_to`, `line_to`, `quad_to`, `cubic_to`, `close`,
  `rect`, `arc` (degrees clockwise from the top) and `circle`, with a `fill` and a `stroke`, drawn
  in the box's points. The draw list hands them to the host as a `Paint::Figure`.
- **Commands.** `command(Command::new(id, title).keys(Shortcut::parse("cmd+k")?))` on an element and
  `focused(true)` on the one the keyboard is on; `Laid::commands` is what can be done now (the
  focused element's and those of the boxes around it, the nearest first, a key taken by the nearest)
  and `Laid::command_for(key, mods)` the command a key does. `Event::Command(id)` tells a page the
  command asked for.
- **Dragging.** `draggable(id)` and `drop_target(id)`; `Ui` keeps the drag (`dragging`, `take_drop`,
  `cancel_drag`, after `DRAG_START` points), and `Laid::drag_source` and `Laid::drop_target` (in
  `Queries` too) say what is at a point.
- **Screen readers.** `role(AccessibleRole::…)` — `Link`, `Checkbox`, `Switch`, `Radio`, `Tab`,
  `MenuItem`, `Heading(level)`, `Image`, `Progress`, `Slider` besides `Button` and `Text` — and the
  state `checked`, `selected`, `expanded`, `disabled` (which also keeps an element from the pointer
  and the keyboard) and `value`, in `Accessible::state`.
- `bg_gradient(angle, from, to)`: a fill from one colour to another (`Quad::gradient`); a line of
  text on it is read against its middle.
- `Button::Middle`, `Input::KeyUp`, `Input::Mods` and `Event::Files`.
- `max_h` and `scroll_x` on an element, as the Lua page API has them.

### Changed

- `Kind`, `Meta`, `Style`, `Paint`, `Quad`, `Accessible`, `AccessibleRole`, `Input`, `Event`,
  `Button` and the `Queries` trait have the members above: a host or a page that matches them all
  or builds them whole is written again for them.

### Fixed

- **Negative margins in a row.** An element with margins below nothing (a line seven points wide to
  the pointer that gives three back on each side) takes the room it is left with from its row, as
  in a style sheet. The layout crate is taffy 0.15 (was 0.10), whose sizing of a box from its
  content counted such an element at its width times its margins, and pushed what came after it
  past the page.
- **The wheel and a click over an overlay.** `Laid::ids_at` is the topmost element under the point
  and what holds it, out to the page, as an event bubbles in a browser. It was every named element
  there, in whatever branch, so the wheel over a palette with nothing to scroll moved the list it
  covers, and a click there could give the keyboard to a field under it.

## [0.3.1] - 2026-10-26

### Fixed

- 0.3.0 never reached crates.io: it was tagged and drafted, and the release workflow then skipped
  its publishing. This is the same code, released.

## [0.3.0] - 2026-10-26

### Fixed

- A box inside another's padding stands in its middle on a page drawn at a scale that is not a
  whole number of pixels to a point (a terminal's zoom): each length a page gives in points is
  rounded to whole pixels on its own, so a padding is as many pixels on each side. The edges were
  rounded where the fractions added up to, a pixel more on one side than the other.

### Added

- **Motion.** `motion(Motion)` on an element says how a change of it moves instead of jumping: its
  colours, opacity, a size in points, its place among the others, and how it comes in and goes
  out (`Shift`), over a time along an `Ease` (`Out`, `Linear`, `InOut`, `Spring`). The host lays the
  page out with `Motions::layout`, which keeps by id where each element was, and again at each
  frame while `Motions::moving()`; an element that is gone is drawn while it goes, and cannot be
  pressed. Nothing moves under Reduce Motion, on a resize, or for what scrolling brings in or out.
- **Tips.** `tip(words, keys)` on an element is what the host says while the pointer rests on it;
  `Laid::tips()` lists them.
- **A fade's own time and curve.** `Fades::to_over(id, level, seconds)` and
  `Fades::level_eased(id, ease)`.

- **Text that wraps.** `wrap()` on a text puts its words in new lines where its box is too narrow
  for them, and at its line breaks; a word longer than the box is broken where it must be. The box
  is as tall as the lines, and in a row it gives its room away down to the longest word. Each line
  is measured by the host, as a line always is, so the lines break where the host's font says.
  Without `wrap` a text is one line, as before.
- **A colour taken toward another.** `Color::toward(other, amount)` mixes a colour part of the way
  toward another (`Color::mix`, a `Mix`), in sRGB, before its opacity: a highlight that comes in on
  its ground, a word that brightens. `Color::resolve` is how every host resolves a colour, mix and
  opacity included; `Color::faded` multiplies the opacity.
- **An element's opacity.** `opacity(0..1)` on any element fades it and everything inside it: each
  thing it draws (fills, borders, shadows, words, pictures, sparklines, and the grounds a line is
  read against) is that much fainter, and nothing of an element at nothing is drawn.
- **Italic, and lines under and through words.** `italic()`, `underline()` and `strike()` are
  inherited like the rest of the text's style: `Font::italic`, `Run::underline`, `Run::strike`.
- **As many text fields as a page names.** `field(id, FieldKind)` (or `text_field(id)`, one line)
  makes an element the room of a host's field named `id`, of a kind: `Line`, `Lines` or `Secret`.
  `Laid::fields` lists them with where each stands, its kind, its hint (the first text inside it,
  which the page does not draw) and its label. A host that shows a hint shows the page's.
- **Labels.** `label(words)` says what an element is for a person who does not see it; the host
  gives it to the system's screen reader (`Meta::label`).
- **Scrolling across.** `Extent` is the content's and the box's `Size` (`max_offset` down,
  `max_offset_x` across), `Input::Scroll` has a `dx`, and `Ui::scroll_by(laid, at, dx, dy)` moves
  the innermost box with room each way (`Ui::scroll_offset_x`, `Ui::set_scroll_x`).
- **More of what the person did.** A press says its `count` (2: the second of a double click); the
  keys `Backspace`, `Delete` and `Function(1..=20)`; `Event::Focus` when the person clicks into a
  field; `Cursor` has the resize, not-allowed, grab, grabbing and crosshair shapes.

### Changed

- A text field is named: `Event::Text { field, text }` says which field, `Keyboard::Field(id)` sends
  the keyboard to one, and `Reply::fill` puts texts in fields. `Laid::text_field` and
  `Laid::text_field_hint` are `Laid::fields`, and `Queries` no longer has `text_field`.
- An image's opacity is its element's (`Element::opacity` on any element); `Image::opacity` is gone.

## [0.2.0] - 2026-10-25

### Added

- The crate itself, and its first release (the pencere crates are released together at one version,
  which began at 0.2.0: there is no 0.1.0 of this one). It is the page engine of the terminal that
  wrote it first: elements and their style, layout, the draw list, what a page remembers between
  frames, and the flat list a sandboxed page hands out (`wire`). The container list that served as
  its worked page and gauge is the example `containers-page`, no longer part of the engine.
- **Colours.** A `Role` names a colour of the theme: its nine roles, the colour of rules and borders
  (`Role::Border`, specification §11.2), `Selection` and the sixteen ANSI colours (`Black` …
  `BrightWhite`, in the order of the palette: `Role::ANSI`, `Role::ansi_index`). A `Color` is a
  `Source` (a role, or an `Rgba` value with an optional form for a dark theme) and an opacity:
  `Color { source, alpha }` (`Color::role()` gives the role). `Rgba::parse` reads `#rrggbb` and
  `#rrggbbaa`. The engine resolves none of it: the host takes the role from its theme and the form
  of a value that goes with its brightness (`Source::value`).
- **`Run::grounds`.** Each line of text in the draw list names the fills under it, nearest first,
  up to the first opaque one (`Color::is_opaque`), unresolved: what a host needs to hold the text
  to its lowest contrast.
- **Images.** `image(src, fit)` is an element that draws a file of the view's bundle (`Kind::Image`;
  `.tint(colour)` and `.opacity(x)` on it; `Fit::Contain` or `Fit::Cover`). The host says how large
  an image is (`Measure::image`, in pixels of the image, `None` while it is not known), the engine
  makes the room (a side the style leaves out follows the proportions, both the image's own size)
  and the draw list holds `Paint::Image(Picture)`: where it is drawn, the share of the image it
  shows (`crop`, which is how `cover` cuts), the unresolved tint, the opacity and the corners.
  `Picture::full()` is the size the whole image comes to, which is what a vector image is drawn at.
  An image whose box takes its size from the image has the border and the padding beside it, and
  one held only by a `min_w`/`max_w`/`min_h`/`max_h` keeps its other side in proportion where it has
  no padding or border.
