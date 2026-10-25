# Changelog

All notable changes to the `pencere-view` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- **Text that wraps.** `wrap()` on a text puts its words in new lines where its box is too narrow
  for them, and at its line breaks; a word longer than the box is broken where it must be. The box
  is as tall as the lines, and in a row it gives its room away down to the longest word. Each line
  is measured by the host, as a line always is, so the lines break where the host's font says.
  Without `wrap` a text is one line, as before.
- **What the host's text field says while it is empty.** A text inside the element that is the room
  of the host's field (`div().text_field().child(text("Filter images"))`) is its hint:
  `Laid::text_field_hint` says it, and the page does not draw it. A host that shows a hint shows
  the page's, not one of its own.

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
