# Changelog

All notable changes to the `pencere-view` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

The engine of version `2.0` of the kit (`pencere:view@2.0.0`, `lua` `2.0`).

### Added

- **A press that opens a menu** (`Pressing::Context`, `gestures = { ["right+press"] = "context" }`):
  outside every selection one caret is put there, inside one the selections stay; the press is told
  the page as any, and `Place` has `inside` (the place was in a selection or at a caret before the
  press moved the caret) and `version`.
- `EditCommand::Copy` and `EditCommand::Cut`, as the editor's keys do them. There is no command that
  pastes: a page cannot read the pasteboard. `MenuEntry::Paste { title, keys, enabled, editor }` is a
  line the host performs itself when it is picked.

### Fixed

- An inline widget keeps room outside its box, between it and the characters beside it
  (`WidgetGroup::margin`, 3 points; `Widget::margin_left`/`margin_right` say another): the box of a
  hint stood against the character after it.

- A gap kept open for the page (`Mark::Zone`) is as tall as its `height`, not as tall as the whole
  rows that hold it: the text after it follows at once, with no band under what stands in it.

- A box's bars answer the pointer only where nothing is drawn over them: under a dialog or a popup
  (an overlay, or anything after the box in the paint order) a bar was lit by the pointer and took
  its press, which was the dialog's.

### Added

- **What a host says of its terminal**: `TerminalLook` (`TerminalCaret`, `TerminalSelection`: each field
  an `Option`, in the words and units of the editor's options), `TerminalLook::fill` (the look put in
  an editor's options where the page said nothing, a field at a time), `Drawing` (`Shadow`) and
  `Event::TerminalLook`.

- **A caret that glides**: `Caret::motion` (`CaretMotion`: `Snap`, `Ease(seconds)`, `Spring(stiffness)`;
  `limits::CARET_EASE`, `CARET_SPRING`). The default is `Snap`, as a caret was.

- **How a caret looks and blinks**: `Caret::radius`, `glow` (`limits::CARET_GLOW`), `blink`,
  `blink_interval` (`limits::BLINK_INTERVAL`), `blink_for` (`limits::BLINK_FOR`) and `unfocused`
  (`CaretUnfocused`: `Hidden`, `Hollow`, `Solid`). The defaults are what a caret was: a square bar
  that blinks every half second for 15 seconds after it moved and is not drawn away from the keyboard.

- **Round selections**: `EditorOptions::selection` (`SelectionLook::radius`, a fraction of a row's
  height, `limits::ROUNDING`) and `pencere_view::bands` (`Band`, `Part`, `rounded`): the shape of a
  selection over lines, one rule for an editor and a terminal's grid (the corners that are open are
  rounded, the turns inward are filled by a square cut by a quarter of a circle, edges less than
  `LEVEL` apart are one).

- `Zone::side` (`ZoneSide::Above`): a gap before its line, for a code lens.

- **Widgets**: `Mark::Widget(Widget)` (parts, `WidgetSide`, group, `id`, `label`, `cursor`, `anchor`,
  `pad_left`/`pad_right`), `EditorOptions::widgets` (`WidgetGroup`, `WidgetShow`), `limits::WIDGET_*`,
  `HoverEvent::over` and `id` (`HoverOver`), `TargetSource::Widget`.

- **Looks drawn by the modifiers held and the pointer's place, presses that are the page's, the editor's
  keys as the page changed them.** `Look::held`, `over`, `cursor` (`Look::shows`), `EditorEvents::target`,
  `LinkOptions` (`links.delay`), `EditorOptions::gestures` (`Gesture`, `Pressing`), `keymap`
  (`Keybinding`, `KeyAction`) and `chord_timeout`, `Event::Target(TargetEvent)` and
  `Event::KeymapPending`.

- **The pointer's rest, the names of ranges and of styles.** `EditorOptions::events` (`hover`) and
  `hover` (`delay`, 0.3 s, `limits::HOVER_DELAY`), `Event::Hover(HoverEvent)` (`Dwell`, `Leave`, the
  word's range and box, the `hits` of the styles with a name under it), `Look::id`, `Mark::Anchor` (a
  name for a range, at most `limits::ANCHORS` to an editor), `Id::anchor(name)` (what an overlay is
  anchored to, as to `Id::caret`), `Measure::anchors` (their boxes in the content),
  `Queries::word_at` and `TextSnapshot::word_range`, and `Column::pad` (the room the numbers keep
  to their left). An overlay anchored to a range that is out of the room is not shown.

- **The gutter is a row of named columns, its markers have shapes, and an editor has an overview
  ruler and zones.** `EditorOptions::gutters` (`Column`: `name`, `width`, `events`, `hover`) lists the
  columns from the left, the editor's `marks`, `numbers` and `folds` where it lists none; `marks`
  (`MarkLooks`: `bar`, `hover`, `ruler`) and `colors.mark_hover` are the measures and the colour.
  `Mark::Gutter` is a `GutterMark` (`column`, `shape` (`Dot`, `Bar`, `Above`, `Below`, `Image`),
  `color`, `id`, `label`, `cursor`, `hover`, `pressed`), and there are `Mark::Ruler` (`RulerMark`,
  `RulerLane`) and `Mark::Zone` (`Zone`). `Event::Gutter` (`GutterEvent`, `GutterKind`, `GutterHit`)
  tells the page of the pointer in a column that hears it. `Element::zone` makes an element the
  content of an editor's zone, `Meta::zone` carries it, and `Measure::zones` says where the editor
  keeps each gap, so that the layout puts the element there and it scrolls with the text.

- **The time of day, the host's keyboard, and what spins.** `WallTime` (Unix seconds and the
  person's offset from UTC) and `KeyConvention` (`Mac`, `Windows`, `Linux`, with `primary`, the main
  modifier) are what a host says of its system; `Shortcut::parse_for` reads `mod` as that modifier
  (`"mod+s"`). `Motion::spin(seconds)` turns an element's picture or canvas round, clockwise, a turn
  in so many seconds, while it is on the page and not under Reduce Motion (`Motions` asks for every
  frame meanwhile): a canvas is turned in its paths, a picture says how far (`Picture::turn`) for
  the host to draw it turned.
- **An editor's new commands and asks.** `EditCommand::{FoldAll, DeleteLines, SelectLine,
DuplicateLinesUp, JoinLines, InsertBlankLine, SplitLine, TransposeChars, DeleteToLineStart,
DeleteToLineEnd}`; `EditorAsk::Change` has `history` and `tag` (told back in `Edited::tag`);
  `EditorAsk::Reveal` has an `align` (`RevealAlign`); `EditorOptions::matches` (`Matches`) and
  `EditorColors::matches` mark the other places of the main selection's text
  (`limits::MATCH_LENGTH`, `limits::MATCHES_IN_VIEW`); `Event::EditorReady` tells a page an editor
  it named was made.
- **A command has as many keys as do it.** `Command::keys` is a list (`Command::keys(shortcut)` adds
  one; `done_by` asks whether a key is one of them), as an editor's keybindings are: each does it, a
  list shows the first, and keys a nearer command has are taken from a farther one's and its others
  kept. A menu's entry has keys as a command does (`MenuEntry::Item::keys`, a `Shortcut`) where it
  had a character. `Shortcut::parse` reads the key `+` (`cmd++`).
- **What a field does with a key, and what a page puts in one.** `FieldState::takes` is the kit's
  rule of the keys a host's field keeps, asked with the field's kind, its length and what is
  selected: escape, tab and the function keys are the page's; in one line enter and the keys that go
  up and down; a chord of command or control unless it is the field's own (select all, paste, undo,
  redo, and copy and cut with something selected); and backspace, left and home at the start,
  delete, right and end at the end, where nothing is selected (an empty field gives backspace to the
  page). A reply's `fill` is a `Fill` for each field: a text, what is selected in it (`select`,
  characters, `select_all`), or both.
- **Editors.** `editor(id, EditorOptions)` makes a box the room of an editor of text of the host's
  (the `pencere-editor` crate): it scrolls as a box does, over the content the host's editor says
  (`Measure::editor`), and draws in its place of the list (`Paint::Editor`, an `EditorSlot`). A text
  inside the room is its hint, and is not drawn. `Laid::editors` finds every editor on the page, the
  hidden ones too (`EditorRoom`); the caret has a name of its own (`Id::caret`), which an overlay is
  anchored to and `bounds` answers, and an editor scrolled with an overlay on its caret says the
  page wants laying out again (`Laid::wants_layout`). `Measure::offsets` says where across a line
  each place is, by default from the start of the line up to it measured. What a page and an
  editor say to each other is plain data, `pencere_view::edit`: changes, ranges, versions, origins,
  decorations by layer (`Decorate`, `Mark`, `Look`), the editor's options, its commands, the asks of
  a reply (`EditorAsk`), refusals, the copy of the text a page reads (`TextSnapshot`) and the floors
  both sides hold a page to (`edit::limits`). `Event` has `Pointer` (with the `Place` under the
  pointer), `Edit`, `Selection`, `Viewport` and `EditorRefused`; `Reply::editor` asks; `Queries`
  reads an editor's text (each with a default of none).
- **How an editor looks is the page's.** `EditorOptions` says how tall a row is (`line_height`,
  times the font's size), the room around the text (`padding`), the rows kept about the caret when
  it scrolls (`scroll_margin`), what the line numbers count (`Numbers`: absolute, relative, hidden),
  where the caret's line is drawn (`LineHighlight`: across the text, the gutter or both), the caret
  (`Caret`: a bar, a block or a line under the character, as tall as the row or the text, how wide,
  its colour and the colour of the character under a block) and the colour of everything the editor
  draws besides the text (`EditorColors`). Each colour is a `Tint`: the theme's, none, or the
  page's. What a page leaves out is the default, today's look; `EditorOptions::held` takes each
  number into what an editor takes (`edit::limits`).
- **What an editor's search finds.** `EditCommand::Find` has `word` (only a whole word: no letter
  or digit, of any script, and no `_` beside it); `EditCommand::Replace` and `ReplaceAll` put a
  text in place of the main selection's match, or of every match (a regular expression's groups put
  in by `$1`, `${1}`, `$name`, `${name}`, `$$`), `SelectFound` selects every match and
  `SelectOccurrences` every place of the main selection's text. `Event::Found` tells a page what
  its editor's search finds (`edit::Found`: the version searched, how many matches, which of them
  the main selection is on, from 1, and whether there may be more), at most
  `edit::limits::FIND_MATCHES`.
- **What a page says of its language.** `EditorOptions::comment` is a `CommentTokens`: a line
  comment, a block comment (`open`, `close`) or both (`CommentTokens::line`,
  `CommentTokens::block`); `EditCommand::ToggleComment` comments with the line comment, or with the
  block comment around the lines of each range where the language has none, and
  `EditCommand::ToggleBlockComment` puts each range in the block comment or takes it out.
  `EditorOptions::indent` (`IndentRules`: `increase`, `decrease`, regular expressions) says how the
  language indents, and `EditorOptions::folds` (`Folds::Page`, `Folds::Indent`) whether the editor
  folds by the indent besides the page's folds. An editor that says none of them is as it was.
  `edit::limits::PATTERN_SIZE` is how large a pattern of an editor's may compile to.
- **What is not seen.** `EditorOptions::whitespace` (`Whitespace`: `None`, `Boundary`, `Selection`,
  `Trailing`, `All`, VS Code's `renderWhitespace`) says which spaces and tabs an editor shows, and
  `EditorOptions::indent_guides` whether it draws the guides of the indent; `EditorColors` has
  `whitespace`, `indent_guides` and `active_indent_guide` (the guide of the block the main caret is
  in). An editor that says neither is as it was.
- **Links.** `Reply::link` asks the host to open an address in the person's browser, by the rule of
  a web page's `link` (`http` or `https`, printable ASCII, at most 8192 bytes; any other is dropped
  and the page is not told); `Reply::with_link` says it in a reply.
- **Wakes.** `Reply::wake` asks the host to wake the page that many seconds from now, and
  `Event::Wake { now }` is the wake, at the time of the frame it comes at: the work a page does
  over time (a long document read a piece at a time, a save once the person stops typing) goes on
  with no event of the person's. Of the times a page asks the earliest stands; `Reply::with_wake`
  says it in a reply, the earlier of two where it is said twice.
- **Long lists that keep up with the wheel.** `Built` is which rows of a long list a page builds as
  the host scrolls it: two views ahead of what is in view, the way the list goes, half a view behind
  (all of it the other way at an end of the list, which can only go back), and the same rows again
  until what is in view comes within a view of their end (`Built::span`); `Built::stale` says
  whether a box the host scrolled is to be built again. `Rows::even(count, height)` is a list of
  rows of one height with no table of where each starts, and `Rows::from_tops` one from the sums a
  page keeps. `List` is all of it for one list a page keeps: `List::build` fills the list's box
  with a spacer, the rows near what is in view and a spacer (it takes `&self`, so a page builds it
  as it builds the rest), `List::settle` and `List::scrolled` say whether the page is to be built
  again for it, and `List::reveal`, `List::scroll_to` and `List::offset` are how far it is scrolled.

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
- `max_h` on an element, as the Lua page API has it.

- **The host scrolls.** `Scrolls` is how far each box of a page is scrolled, kept by the host and
  not the page: the wheel and the trackpad move a box at once on the page already laid out
  (`Scrolls::wheel`, with the system's `Phase` of the gesture: a gesture stays with the box it
  started on, and with `set_elastic` a box pulled past its end springs back; what of the turn no
  box took is the page's, `Wheeled::left`), its bars are drawn
  as the system's are (`Bars`) and are dragged and clicked (`press`, `point`, `release`,
  `set_track_jumps`), and a page asks for a box to go somewhere (`ScrollTo`, `Scrolls::ask`, over a
  glide or at once). A box moves without the page being laid out again (`Scrolls::place` moves what
  is inside it on the `Laid` it has), and the page is told where its boxes went
  (`Scrolls::take_moved`, `Event::Scrolled`). `Extent::offset` is how far a box is scrolled.

### Changed

- How far a box is scrolled is no longer the page's: `Element::scroll()` makes a box that scrolls,
  in place of `scroll_y(offset)` and `scroll_x(offset)`, and `Ui` keeps no offsets (`scroll_offset`,
  `scroll_to`, `set_scroll`, `scroll_by` and `settle` are gone; the host's `Scrolls` does it). A
  wheel no box took is still told to the page (`Input::Scroll`).
- `Kind`, `Meta`, `Style`, `Paint`, `Quad`, `Accessible`, `AccessibleRole`, `Input`, `Event`,
  `Button` and the `Queries` trait have the members above: a host or a page that matches them all
  or builds them whole is written again for them.
- **Words fade with their box.** A line of text in the draw list keeps its own colour (`Run::color`,
  its own opacity included) and says apart from it how much of it shows (`Run::opacity`: the
  `opacity` of its element and the boxes that hold it, and what a motion fades), as an editor's place
  already did (`EditorSlot::opacity`). A host that holds words to a lowest contrast lays the opacity
  over what it holds: before, it held the faded colour, so words that came or went with their row
  stopped fading at the floor and appeared or vanished at once, and `opacity` could not make words
  fainter than the floor. A host that builds a `Run` gives it an `opacity`.

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
