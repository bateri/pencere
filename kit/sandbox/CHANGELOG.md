# Changelog

All notable changes to the `pencere-sandbox` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Fixed

- A reply to what the program said (or to its room) is posted whenever it asks the host something:
  its `editor` asks and its `fill` too, which were dropped (a page could not load a file it was sent
  into its editor, nor mark a mistake its program told it of, unless the same reply also moved the
  keyboard).
- The answer to the pointer over an editor (`Event::Pointer`) is the pointer's (`Answer::pointer`),
  as the answer to a move beside it is: the shape a page asks for over its editor is taken, where
  the one it asked for just before (a divider's) stayed over the editor.

### Added

- **Editors.** The interface has `node.editor`, the events `editor-pointer`, `edit`, `selection`,
  `viewport` and `editor-refused`, a reply's `editor` asks, and `layout.editor-version`,
  `editor-length`, `editor-lines`, `editor-text`, `line-start` and `line-of`, which read the copy of
  each editor's text the application posts (`Module::texts`, before the events that tell of its
  version). A reply is held to the text (1 MiB) and the decorations (20 000) it may hand the editors
  (`ModuleLimits::editor_text`, `editor_decorations`, `editor_asks`), a read of a text to 1 MiB. The
  interpreter speaks them as the Lua page API: `pencere.editor`, `pencere.caret_of`, the text of an
  editor through `pencere.layout`, the events and the `editor` of a reply.
- **How an editor looks.** `editor-options` carries `line-height`, `padding`, `scroll-margin`,
  `numbers`, `caret` (`editor-caret`) and `colors` (`editor-colors`), each colour an `editor-tint`
  (the theme's, off, or a colour); a number the page does not say is the editor's default, and every
  number is held to what an editor takes. In Lua they are options of `pencere.editor` of the same
  names, `caret` and `colors` tables of their own, a colour `false` for none; an option an editor
  has no such name for fails the page where it is said.
- **What an editor's search finds.** The interface's `find` has `word`, the editor's commands
  `replace`, `replace-all`, `select-found` and `select-occurrences`, and the event `found` (the
  version searched, how many matches, which of them the main selection is on, from 1, whether there
  may be more). In Lua they are `{ command = "find", …, word = }`, `{ command = "replace", text = }`,
  `{ command = "replace_all", text = }`, `"select_found"`, `"select_occurrences"` and the event
  `found` (`current` nil for none). What a replacement puts in counts toward the text a reply may
  hand the editors.
- **What a page says of its editor's language.** The interface's `editor-options` has `comment` (a
  `comment-tokens`: a line comment and a `block-comment`), `indent` (`indent-rules`: `increase`,
  `decrease`) and `folds` (`editor-folds`: `page`, `indent`), and the command
  `toggle-block-comment`. In Lua they are `comment = "--"` or `{ line =, block = { open, close } }`,
  `indent = { increase =, decrease = }`, `folds = "page" | "indent"` and `"toggle_block_comment"`. A
  rule of the indent that is not a regular expression fails the page, the words naming the option
  (`indent.increase`) and what is wrong with it, and so do more than 64 different ones in the
  editors of one frame; each is compiled when a frame first says it, not again while the frames
  after say it too. For that the crate depends on `regex` (MIT or Apache-2.0), the editor's own, and
  compiles them as the editor does (a test holds the two to the same patterns).
- **What an editor shows of what is not seen.** The interface's `editor-options` has `whitespace`
  (`editor-whitespace`: `none`, `boundary`, `selection`, `trailing`, `all`) and `indent-guides`, and
  its `editor-colors` `whitespace`, `indent-guides` and `active-indent-guide`, each appended. In Lua
  they are `whitespace = "boundary"`, `indent_guides = true` and the colours of the same names.
- **Links.** The interface's `reply` has `link` (appended): an address the page asks the host to
  open in the person's browser. The sandbox hands on only one the host may open, by the rule of a
  web page's `link` (`pencere_wire::link::is_openable`: `http` or `https`, printable ASCII without
  the space, at most 8192 bytes), as `Reply::link`, and drops any other without closing the page; a
  reply to what the program said that asks to open a link is posted like one that fills a field. In
  Lua it is `link = "https://..."` in a reply; a `link` that is not a string is an error of the
  script.
- **Wakes.** The interface's `reply` and `frame` have `wake` (appended): in how many seconds the
  page asks to be woken; its `event` has `wake` (appended), the time it is woken at. A `wake` that
  is not a number of seconds (below 0, not a number, infinite) is not a reply, or not a page. The
  module keeps what a page asks, so that every host does the same: the seconds counted from the
  time it asked at (`Program::now`, else the last frame's and what passed since), the earliest of
  all its replies and frames asked standing, as `Update::wake`, told on every taking until the page
  is woken; `Module::wake(now)` wakes it at a frame at or after that time (`Event::Wake`), once a
  frame at most (one at a time no later than the last wake's, or before the time asked, is passed
  over; of wakes that waited while it was busy the newest is the one), and a page drawn again for
  what it did then is drawn at the wake's time. The `wake` of a reply or a frame the application
  takes is none: the module's to keep. `Frame` has a public `wake` field for it: code that writes a
  `Frame` whole names it (`wake: None`). In Lua a reply's `wake = seconds` asks, and `handle` is told
  `{ type = "wake", now = }`; a `wake` that is not a number of seconds, 0 or more, is an error of
  the script.
- `pencere.time`: timers, named as the program runner's. `pencere.time.after(seconds, fn)` and
  `pencere.time.every(seconds, fn)` return a timer (`:cancel()`, whether it was waiting;
  `:active()`), and `pencere.time.now()` is the time the host last gave the page (a frame's, a
  wake's). The kit asks for the wakes its timers want with the page's replies and frames, runs the
  ones due when the page is woken, in the order of their times and before the page's `handle` hears
  of the wake (only where it asked a `wake` itself), and puts what each returns into the reply. A
  timer set outside a wake counts from the next one, which is asked for at once (never early, for
  the time the page last had may be old); `every` runs once for runs it fell behind on; an error in
  a timer is the page's, with its line; a timer set in `settle`, which has no reply, has the page
  rendered again, and its frame asks. A page that sets none and asks no `wake` is asked and told
  nothing new. What the page's parts return together (its own reply, its timers', a syntax's) is
  held to what a reply may hand the editors: the decorations past that wait for the next reply, in
  their order, and a wake is asked for it at once.

- The interface is `pencere:view@2.0.0`: overlays and anchors, texts of many looks, canvases,
  commands, dragging, roles and states, fills from one colour to another, and the middle button, keys
  that go up, the modifiers alone, commands and dropped files as events (see `pencere-view`). What a
  module says of them is read with the care the rest is: parts of a text, paths of a canvas and
  commands of an element are held to `ModuleLimits::parts` (1024), steps of a path to `values`, and
  every part counts toward the bytes of a frame.
- `draw_figure`: a canvas's paths in pixels (premultiplied RGBA, the box's size), each colour as the
  host resolves it — the one rasteriser every host draws a figure with. The same figure in the same
  colours has the same `Bitmap::id`, and a figure's id is never an image's.
- The interpreter speaks the Lua page API `2` (`pencere.version`): `pencere.rich`, `pencere.span`,
  `pencere.canvas`, `pencere.path`, `pencere.keys`, the element methods `overlay`, `anchor`,
  `bg_gradient`, `max_h`, `role`, `checked`, `selected`, `expanded`, `disabled`, `value`, `command`,
  `focused`, `draggable` and `drop_target`, `layout.drag_source` and `layout.drop_target`, the drag
  of `pencere.Ui`, and the events `key_up`, `mods`, `command` and `files`.
- `box:list(rows, make)`: a long list in a box the host scrolls, only the rows near what is in view
  made, and the frames it needs asked for by the kit (a `scrolled` event near the end of the rows
  made, a layout that gives it a taller window), whatever the page answers; a box that cannot hold
  one is refused by name. `pencere.list_window(id)` says the rows a list made and how tall its
  window is. Under them, `pencere.Built`, which rows of a long list to build as the host scrolls it
  and whether a `scrolled` box is to be built again, and `pencere.Rows.even` and
  `pencere.Rows.from_tops`, as the engine's `Built` and `Rows` (see `pencere-view`).
- `pencere.Syntax`: a language that colours an editor, as CodeMirror's `StreamLanguage` does.
  `pencere.Syntax.define { name, start, token, copy, equal, blank, styles }` reads a line a token at
  a time from a stream (`sol`, `eol`, `peek`, `next`, `eat`, `eat_while`, `eat_space`,
  `skip_to_end`, `skip_to`, `backup`, `column`, `indentation`, `match` with Lua's patterns,
  `current`) with a state carried from line to line, and a token's name is looked up in its
  `styles` by each of its names and by what is before a dot.
  `pencere.Syntax.new(language, editor, options)` keeps the state after each line, reads again
  after an edit only until the state is what it was, colours the lines in view and a margin (in
  characters) and takes away the rest, and reads at most `step` bytes a call (20 000), lines in view
  far on coloured from a guess and read to in the frames after, by itself: a timer of its own wakes
  the page a frame at a time while there is more, and hands what it finds to the editor with the
  wake's reply, so that a large document is coloured to its end with no event of the person's
  (`handle`, `reply`, and `work`, `busy`, `asks` for a page that wants them). `handle` works at once
  for its editor's `edit` and `viewport` alone, and a syntax holds at most 10 000 decorations for
  the page, half of what a reply may hand the editors. `language:tokens(text, state, tab_width)`
  is a line's tokens, for a text that is not in an editor. A token that reads nothing fails the
  page with the language's name and the line. A page that does not use it has none of it.

### Changed

- A component written against `pencere:view@1.0.0` is refused, in words that name both versions.
- The application scrolls a page's boxes, not the page: a style says no offset (`scroll-x` and
  `scroll-y` are gone from the interface), `extent` says how far a box is scrolled, the module asks
  for a box to be scrolled by `layout.scroll-to` (what it asked is in `Update::asks`, for the
  application's `Scrolls`), and is told where its boxes went (`event.scrolled`, a run of which is
  each box where it stands last). The Lua page API has `:scroll()`,
  `pencere.layout.scroll_offset(id)`, `pencere.layout.scroll_to(id, { y =, x =, animate = })` and
  the `scrolled` event, in place of `scroll_y(offset)`, `scroll_x(offset)` and the offsets `Ui` kept.
- A run of wheel turns over one point that waits for the page is one turn by all of them, as a run
  of pointer moves is the last of them: a page slower to make than the wheel turns does not fall
  further behind with every turn.

## [0.3.1] - 2026-10-26

### Fixed

- 0.3.0 never reached crates.io: it was tagged and drafted, and the release workflow then skipped
  its publishing. This is the same code, released.

## [0.3.0] - 2026-10-26

### Added

- The interface says an element's `motion` and `tip` (both on a node), held to sane values (a move
  of at most two seconds; what is not a number is none). The Lua prelude writes them
  (`motion({...})`, `tip(words, keys)`), gives a fade its own time (`fades:to(id, level, seconds)`)
  and curve (`fades:level(id, ease)`, `pencere.ease(t, ease)`).
- `Module::wait_for_frame(size, within)`: waits, at most `within`, for a page made for `size`
  that the host has not taken. For a resized pane, so the page laid out for the old size is not
  drawn for a frame; a page made again for the old size meanwhile does not end the wait.

### Changed

- The interface (`pencere:view@1.0.0`, a draft until a host other than this repository's runs it)
  says what `pencere-view` added: a colour's `mix`, an element's `opacity` (an image's is its
  element's), `italic`, `underline` and `strike`, named fields of three kinds (`field` and `label`
  on a node, `text` and `focus` events with the field, `keyboard.field(id)`, `reply.fill`), a
  press's `count`, a scroll's `dx`, an extent across as well as down, the keys `backspace`,
  `delete` and `function`, and six more shapes of the pointer. `layout.text-field` is gone: a
  field's box is `bounds` of its name. The interpreter (`luau.wasm`) is built from it, and its page
  API has `pencere.mix`, `opacity` on any element, `italic`, `underline`, `strike`, `label`,
  `text_field(id, kind)`, `scroll_x`, and `Ui:scroll_by(laid, at, dx, dy)`.

## [0.2.0] - 2026-10-25

### Added

- The crate itself, and its first release (the pencere crates are released together at one version,
  which began at 0.2.0: there is no 0.1.0 of this one). It is taken from the terminal that wrote it
  first: the sandbox, its limits and watchdog, the thread a module runs on, and the embedded
  interpreter. The interface is `pencere:view@1.0.0` and the Lua global is `pencere`.
- `kit/SPEC.md`: the Lua page API, the colour strings, the recommended formulas of the derived tones and the sandbox floors.
- **A frame says what it says once.** The words of every line of text and the paths of pictures,
  the values of sparklines and the tracks of grids are three lists of the frame, and a menu's
  titles one string; an element (an entry) names the part it means (`slice`: a start and a
  length). A module cannot make the application copy one buffer for every element that
  points at it: the lists are taken in once, as large as the module's memory, and what the
  elements read of them is counted each time against `ModuleLimits::frame_bytes` (4 MiB). A part
  past its list, one that begins or ends inside a character, or a frame past the limit is not a page.
- **`ModuleLimits::compile`.** Compiling a component is held to a time (20 s); a component that takes
  longer is refused, in words, and its compiling ends on its own thread. A component written
  against another version of the interface than this one's is refused with a sentence that names both
  (`this view is written against `pencere:view@0.5.0`; this terminal opens `pencere:view@1.0.0``),
  and not with the runtime's words.
- **Images.** `ImageLimits::work_bytes` (the memory one image may be worked in: what a decoder
  holds, the layers an SVG is drawn on), `sizes` (an SVG is drawn at a few sizes, each its own
  raster, and a size past them is the nearest) and `idle` (how long pixels nothing asks for are
  kept when their room is needed). A drawing that unfolds into more than a hundred thousand parts is
  refused, and its parsed shapes are counted in the pixels of the view. A JPEG is held the way its
  EXIF orientation says, as a web view shows it.
- The package carries what it is built from and nothing of the repository around it: the interface
  it reads is its own copy (`wit/view.wit`, equal to the kit's `kit/wit/view.wit`, which `make check`
  keeps so), and the tests of `src/`, which read files of the repository, are left out.
- **`Program::now`.** An application that keeps a clock it can read from any thread says what it
  reads (the same seconds `render` is given as `now`), and the page that is made again because its
  program said something is made for that time. Without it (the default, `None`) the time is that
  of the last frame and what the system's clock says has passed since: right as long as
  the application's clock goes as the system's does, which a clock that stands still or is driven by
  hand does not. A time that is not a number, or is earlier than the last frame's, is not taken.
- **Images.** `Images` decodes the files of a view's bundle (`Assets`, a path to bytes) for the
  `image` element: PNG, JPEG and SVG, off the thread that draws, once for each path. It gives the
  host the size of an image for the layout (`Images::size`), the pixels to draw (`Images::bitmap`,
  premultiplied RGBA with an `id`; an SVG is drawn at the size it is asked and again for another),
  and one wake for what arrived (`Images::poll`: `relayout`, `redraw`, `diagnostics`). Held to
  `ImageLimits`: 4096 pixels a side, 64 MiB of decoded pixels, 8 MiB a file, 1024 paths. An SVG
  reads nothing it names, draws no text and loads no font. The interface and the Lua page API have
  the element (`pencere.image(src):fit("contain" | "cover"):tint(…):opacity(…)`); the
  interface is `pencere:view@1.0.0`.
- New dependencies, all of them MIT, Apache-2.0, BSD or Zlib: `png` 0.18, `zune-jpeg` 0.5.15 with
  `zune-core`, and `resvg` 0.48.1 with no default features (so no text, no raster images and no
  compressed files) and what it brings: `usvg`, `tiny-skia` and `tiny-skia-path` (BSD-3-Clause),
  `svgtypes`, `kurbo`, `roxmltree`, `simplecss`, `data-url`, `strict-num`, `imagesize`, `rgb`,
  `float-cmp`, `euclid`, `polycool`, `siphasher`, `pico-args`, `bytemuck`, `arrayref`, `flate2`,
  `zlib-rs`, `fdeflate`, `miniz_oxide`, `adler2`, `simd-adler32`, `num-traits`, `autocfg`.
- The interface and the Lua page API carry the new colours: the theme's ANSI palette and selection
  as roles (`"red"`, `"bright-red"`, `"selection"`, and `"border"` for the colour of rules and borders), and values (`"#rrggbb"`, `"#rrggbbaa"`,
  `{ light = "#…", dark = "#…" }`, with an opacity as before: `pencere.alpha(colour, 0.5)`). The
  interface is `pencere:view@1.0.0`.
- `pencere.json` of the Lua page API reads and writes arrays and objects nested up to 127 levels; at 128 it is neither (specification §4.5).
- **`require` in a script.** A script may load the other scripts of its bundle, and nothing else:
  Luau's own require-by-string (`require("./screens/logs")`, `require("../parts/table")`, directories
  with an `init` script, `@self`), relative to the path of the script that asks, over the files the
  bundle's manifest types `text/x-lua`. A path that leaves the bundle's root, an absolute path, an
  alias (there is no configuration file), a file that is not a script and a name the bundle has
  nothing at are errors that name the path, at the line of the `require`. Each script runs once per
  page, in a thread with globals of its own, and a cycle is an error that names the chain. An error
  in a required script (a syntax error too) says that script's own file and line; the entry's says
  its path too (`page.lua:12:`). The bytes of the scripts a page loads, the
  entry's included, are held together to `ModuleLimits::component` (4 MiB); a script that is
  alone over it is not started. For this the interface `pencere:view@1.0.0` has
  `script.path`, `script.has-module` and `script.module` beside `script.source`, `Module::from_script`
  takes a `Script` (the entry, its path in the bundle and the bundle) and `Assets` has
  `media_type`, which says what the manifest typed a file. The interpreter carries Luau's
  `Require` and `Config` libraries (about 190 KB more).
- A helper of the Lua page API that is given the wrong argument (`pencere.text` of a value that is
  not a string or a number, `pencere.spark` of a non-list, `children` of a non-list) raises the
  error at the line of the script that called it, not at a line of the prelude.
