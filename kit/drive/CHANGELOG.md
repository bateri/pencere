# Changelog

All notable changes to the `pencere-drive` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Added

- **`Page::reading` and `reading::Reading`**: what an agent reads of a page: its elements by a six-letter
  name made of their `Id`, its fields, the lines its editors show and what is selected, the boxes that
  scroll, its commands, where the keyboard is (a field, an editor or a browser) and whether it is at
  rest, written as text or as JSON.
  `Headless::reading()` gives the same, so a page's author tests what an agent sees. The value of a
  secret field is never in it.
- **`Page::rest`**: whether the module has more to do, how many calls to the program have no answer,
  whether an image is on its way and whether the page is laid out for what it last was, without waiting.
- **`Page::reach` and `Page::reaches`**: where a press reaches an element, which `Headless` took from
  its own clicks.
- **Browsers: `Page::set_browsers`, `Page::browser`, `Page::browsers`, `Host::browser`.** A host that
  shows browsers says so and tells the page what each is and does; one that does not has the drive
  draw a placeholder in each room and tell the page it is `unsupported`. An address a browser does not
  go to is refused to the page and never reaches the host. The headless host stands for one that shows
  browsers (`Opening::browsers(false)` for one that does not), keeps what the page asks of them
  (`Headless::browser_asks`) and takes the state a test gives (`Headless::browser_state`). A zoom is
  held to what a browser may be zoomed to, an ask the kit does not know is refused to the page, a key
  the browser has no use for is given back to the page (`Headless::browser_unused`), and
  `Page::browser_window` says whether the person asked for the window behind the page. A find of
  nothing ends the search, a pattern that is not a search's is refused to the page, and what a host
  says of a browser that is not on the page is not told it.
- **`shot::Scratch` and `shot::raster_rows_in`**: rows drawn in buffers the caller keeps from one
  frame to the next, with the page's edge repeated past them, so that a frame at the rate of keys asks
  the system for no picture's worth of memory.
- **`shot::raster_part_in` and `shot::across`**: the columns of rows that changed are drawn alone, and
  `across` says how far across an item draws (the lean of a letter, a box's edge), so that a key
  draws the columns of the line it is typed in and not the width of the window. A soft shadow that
  reaches them is blurred over the whole width and is the `Error::Shadow` of drawing them alone.
- **`shot::raster_rows` and `shot::reach`**: the rows of a picture that changed are drawn again and
  the rest are kept, and `reach` says how far an item draws beyond its own box (a shadow, a glow), so
  that a row is not left out that one of them touches.
- **`shot::Fonts::installed()` and `Fonts::carried()`**: a picture is drawn in the machine's own
  faces (SF Mono and SF on a Mac) in their bold and italic, a character a face lacks in the first
  face that has it (a colour emoji, an ideograph) and a hollow box where none does; where the machine
  has no face the crate's own are drawn: JetBrains Mono and Inter (SIL OFL 1.1), packed with zlib. A
  face with an optical size (SF, Inter) is set to the size of the text in points (`Fonts::scaled`).
  The faces measure as they draw.
- **Pictures of a directory of pages** (the example `shots`, with the feature `shot`): every page
  script of a directory is pictured in a dark terminal's colours and in a light one's, after what
  its own lines that start with `--@` say is done to it (its size, a click, a key, what is typed),
  and `shots.json` says the size of each. It is what the site's pictures of the kit are made with
  (`site/shots.sh`). `Headless::images()` is the page's pictures, decoded, which a picture of its
  draw list is drawn with, and `Headless::field_of(id)` is what a host's field holds, by its id.
- **A drag out of the view, and the view's text size**: a host that says it carries
  (`Page::set_carry(true)`) is given, in the `input` that moved the pointer, the system's drag of
  what an element says it carries: pressed on it with the main button and moved `DRAG_START` points
  (an element the page also drags: moved out of the view), `Drained::carry` is a `CarryOut` (the
  element's id, what it carries, where the pointer is and the element's room, in the host's points)
  that `Drained::deliver` hands to `Host::carry`, after the
  replies and the pasteboard. The page hears none of the pointer's moves and its release from then
  on; `Page::carried(done)` ends the system's drag and tells it `carried` (a press while one goes
  tells it as not done). What is carried is held to `pencere_wire::carry` again before it is handed
  on. `Host::text_size(points)` is how many points larger than the host's own text a reply asked the
  view to be drawn (after the keyboard, before the pasteboard). `Headless` is a host that carries:
  `drag_out(name, to)`, `carries()`, `finish_carry(done)` and `text_size()`. `carriable(&Carry)` is
  the rule as the drive holds a drag to it, public: a host that keeps what it was handed until the
  drag is let go holds it to the rule again where it takes it.
- **A box that says a `zoom` is zoomed by the host**: a pinch, the wheel with the command key held
  and a double tap with two fingers over it are the box's (no longer the page's: it is told the
  scale with `scrolled`), the page is laid out again at each step of a zoom and at the end of one
  that springs back, and the page's `zoom_to` is done before the next frame. `Headless::zoom_of(name)`
  says how far a box is zoomed.
- **`Page::magnify`, `smart_magnify`, `rotate`, `swipe` and `pressure`**, and `Input::Extra` through
  `Page::input`: what a trackpad's fingers and a mouse's side buttons do, told to the page with the
  modifiers the host last said. `Page::mods()` says them. **Breaking:** `Page::wheel` takes
  `precise` (the device says points as fingers do), and the page's `wheel` carries the phase and the
  modifiers; `Input::Move` and `Input::Release` carry `mods`.
- **`Headless`** drives them: `pinch`, `pinch_at`, `magnify_at`, `smart_magnify`, `rotate`, `swipe`,
  `force`, `click_extra` and `trackpad_at` (the fingers of a trackpad moving a box or the page).
- **Features `compiler` and `pdf`, both on by default,** which are those of `pencere-sandbox` (the
  code generator of the runtime that compiles the interpreter as the first page opens, and the PDF
  reader). The crate takes the sandbox with `default-features = false` and gives its own on, so a
  host that takes off what it has no use for takes the drive with `default-features = false` too.
- **Features `compiler` and `pdf`, both on by default, and `precompile`, off,** which are those of
  `pencere-sandbox` (the code generator of the runtime that compiles the interpreter as the first
  page opens, the PDF reader, and the interpreter compiled by the build instead of the code
  generator). The crate takes the sandbox with `default-features = false` and gives its own on, so a
  host that takes off what it has no use for takes the drive with `default-features = false` and
  `features = ["precompile"]` too.
- `Page::set_host_caret`: a host that draws the caret itself, as a terminal does with its cursor,
  says so, and the page draws none of its own (the selection stays) and asks for no frame to blink
  one. Off unless the host sets it. `PageEditors::caret`: where the main selection's caret is (the end
  that moves) in units of UTF-16, to ask `PageEditors::rect` for its box.
- `Page::binds` says whether a key with its modifiers is one the page binds to a command, so a
  host can tell a key that moves a view from one the page answers with a command.
- `Headless::spills`: the boxes of the last frame that stand past the inside of the box they are
  in (`Laid::spills` of `pencere-view`), in the host's points, for a test that looks at a page at
  every size of its window. `pencere test` has it as `page:spills()`: a list of tables, each with
  `box`, `inside`, `left`, `above`, `right`, `below`, `words` and `around`.

### Changed

- A press over a field that something is drawn over (a popover, a card) reaches what covers it, as in
  a host, which does not show such a field; it went to the field.
- A page the module makes again that is the page it was is not laid out again, and what an editor
  says of one act (a letter typed is a change and a move of the carets) is one page, not one for each.
- The crate's licence is `Apache-2.0 AND OFL-1.1`: the two faces it carries (JetBrains Mono, Inter)
  are under the SIL Open Font License 1.1, and their licence files are in the package (`fonts/`).

- A page's editors take their own keys from the host's keyboard (`Program::keyboard`), as `pencere.keyboard()`
  tells the page: where ⌃ is the main modifier ⌃A selects all and ⌃Z, ⌃Y undo and redo (`Editors::set_keyboard`).
  A host whose keyboard is a Mac's (what one built for macOS says unless it says another) has the keys it had.

- The headless host and the screenshot draw a picture with `Images::picture` and the crop of the
  bitmap (`Bitmap::crop_of`): a large SVG or page of a PDF is drawn where it is seen, so a page is
  sharp at every zoom and the pixels held come to the screen.

- The pictures the headless host asks the pixels of are those it shows: one scrolled out of its
  box is not in the draw list (`pencere-view`), and so holds no room of the view's images.

- A page's pictures are measured and drawn for the page of the file they show (`Images::size` and
  `Images::bitmap` take it): a page of a PDF is laid out at its own size and drawn at the size it is.

### Fixed

- **A picture shows the host's text fields.** `Headless::shot` drew a field's box empty, whatever
  was typed in it, and with no hint: it now draws what each field holds in the terminal's face at the
  kit's text size (a secret as dots), or its hint, quiet, where it holds nothing, what is selected in
  it and the caret of the one that has the keyboard, as a system draws its own over the page.
- **A reading gives the controls of a dialog.** A reading of a page with a dialog over a scrim that
  answers the pointer gave one button named by the dialog's title; the buttons and fields in it are in
  it now, each by its ref (`pencere_view::Laid::accessible`).

## [0.4.1] - 2026-11-05

Released with the other crates; no change of its own.

## [0.4.0] - 2026-11-03

The drive of a page of version `0.1` of the kit (`pencere:view@0.1.0`, `lua` `0.1`).

### Added

- The headless host's table of calls (`Calls`) answers the page's `print` and `warn`, which are calls
  of the reserved method `pencere.log`, itself, at once with `{}`, and keeps the lines:
  `Calls::logged()` and `Headless::logged()` (`Logged { level, text }`, the text as the page sent it).
  The lines are not the application's calls: they are not in `Calls::calls`, `methods` or `waiting`,
  and a handler given for the name is never called. `InProcess` and the peers that carry the page's
  words to a real program are not changed: the program's library answers there.
- **Files beside the bundle**: `Page::give` (files the person gave the page, with their handles),
  `Page::files_over`, `Page::files_left` and `Page::saving`; `Host::panel` for the Open or the Save
  panel a page asked for. The headless host plays them: `Headless::give_files`, `drag_files_over`,
  `drag_files_away`, the panel that is up (`panel`, `dropped_panels`), `choose_files` and
  `choose_place`, and `attach` and `detach` (and `Opening::attached`, before the page opens) for the
  files a program adds to the page, which its pictures draw by `sha256:` and `file:` names.
- `Host::paste` (the host's pasteboard, for a menu entry the host performs itself,
  `MenuEntry::Paste`) and `Drained::copied` (text a page had an editor copy or cut). **Breaking:**
  `Page::pick` takes the host (`pick(index, &mut host)`), which pastes first where the entry is one the
  host performs.
- `Held::Copy`: text a page had an editor copy or cut within a frame is put on the pasteboard with
  the frame (`Painted::deliver_held`), the rest with the drain (`Drained::copied`). A menu's paste line
  pastes only into the editor the person was in and only if it is on.
- A press that is the right button's (and, on a Mac, ⌃ with the main button, told the page as the right
  button's) reaches the editors as the page's `gestures` say; the `Place` of a pointer event says `inside` and the editor's `version`.

### Removed

- **The WebAssembly component page**: `Page::component` and `Headless::component`. A page is a Lua
  script (`Page::script`, `Headless::script`, `bundle`, `app`) or a module the test makes
  (`spawn`).

### Changed

- The command a key did (a page's command for a key, with no editor holding the keyboard or with one)
  is told the module with the key (`Module::event_by_key`), so that its answer has the person's word
  for the pasteboard only where the key is the system's copy or cut; `Page::command`, the host's own
  list, is the person's. `Headless::copies()` shows it.
- A Paste line of a menu is off, in its place, except in the menu a right press asked for
  (`pencere_view::Menu::for_event`): `Headless::pick` of it is `Error::NoEntry`.
- **Breaking:** `Host::copy` takes `by_person`, `fn copy(&mut self, text: String, by_person: bool)`:
  whether the person's own word asked for the text (a reply to a press or its release, a pick in a
  menu or a command, and of the keys only the system's copy and cut, `pencere_view::copy_key`; the
  copy or cut an editor makes of what the page asked in such a reply included). It is `false` for a
  reply to any other key, to what the program said, to a wake or to a change of the room, so that a
  host which keeps the program from its pasteboard
  (a terminal's `osc52 = "off"`) writes only where it is `true`. `Drained::copied` is a `Vec<Copied>`
  (`Copied { text, by_person }`), `Held::Copy` holds a `Copied`, and `Headless::copies()` lists what the
  page put on the headless host's pasteboard, each with `by_person`. The editor's own copy key is a
  `Route::Copy` of `Page::key` and is the host's, and so is a key the page chose for a copy
  (`keymap = { f6 = "copy" }`), which is the person's word only where `copy_key` says so.
- **Breaking:** the page's `keymap` has no `"paste"` (`pencere-sandbox`, `pencere-view`).

- **The kit is version `0.1`** (`pencere:view@0.1.0`, `lua` `0.1`). While the specification is a
  draft the versions of `lua` and `web` are `0.x`: a change an existing view can notice moves the
  minor number (`0.2` after `0.1`), an addition does not, and a host opens one version. A program
  whose version the host does not open is not opened, and says so in a sentence that names both
  sides.

- **The places a page says and is told are bytes of the editor's text** (see `pencere-view`); what a
  host says of the person (`Page::select`, `Headless::select`, `caret_to`) stays in characters, and
  the headless host's field takes a reply's `select` in bytes, an end inside a character being the
  start of it.

- **The headless host holds a page to the floor.** Where a test does not say (`Opening::limits`),
  a page is held to `ModuleLimits::floor()` (a call 250 ms, unless `PENCERE_CALL_MS` says), the
  least a host gives; `ModuleLimits::default()`, which a terminal gives, is now more
  (see `pencere-sandbox`). `Setup::default()` keeps the terminal's.

### Fixed

- `Field::covered` in the outline: a field under a popover or a dialog of the page is not to be
  shown, or its text stands over what covers it.

- **A pick is of the menu the host showed, not of the last the page asked for.** `Page::pick` takes the
  `serial` of the menu the host showed (`Menu::serial`), and the drive keeps what the last sixteen menus
  would paste, so a Paste line of a menu the host did not show is not done. The headless host shows
  one menu at a time, as a pane does: a menu asked for while another is up is not shown.
  **Breaking:** `pick(serial, index, &mut host)`.
- **A reply that opens a field can fill it and send the keyboard to it.** A host has a field only
  once the frame that lays it out is drawn, and a reply's `fill` and `keyboard` were told to the
  host before that frame: a find bar or a palette that opened with its text and the keyboard lost
  both. `Page::drain` now holds what a reply asks of a field the outline does not name yet (and what
  comes after it), and the frame that lays the page out gives it in `Painted::held`
  (`Painted::deliver_held`, `Held::deliver`), after the host has the frame's fields, in the order it
  was asked; a name that frame does not have either is passed over as before. The host with no
  screen has a field only once a frame has laid it out, so its tests fail where a real host would.
- **What a drain holds for the next frame now asks for that frame.** A reply that gave the keyboard
  to an editor (a name the host's outline never has) and asked for no page of its own, taken by a
  frame built before the drain, was held with nothing coming to tell it: on a page at rest the host
  was never told where the keyboard went. `Drained::redraw` is now set whenever the drain holds
  something new.
- **A large answer from `InProcess` no longer hangs the page.** The host counted the size of what the
  program said on the text it is taken in, escaped by the wire, and refused an answer that is within
  the limit before the escapes (specification §9.3): the call was owed for ever and the settling
  waited 60 seconds for "the program did not answer the page", hiding the real timeout. It is counted
  as the program counts it.

### Added

- **The terminal's look**: `Page::terminal_look_changed` tells the page the host's terminal draws its
  cursor or its selection otherwise (from the pane's thread); the host with no screen says what a
  test says (`Opening::terminal_look`, `Opening::draws`, a soft shadow unless said) and tells the page
  where it changed (`Headless::set_terminal_look`). The example `editor-lua` takes the look.

- **A caret glides** (`caret.motion`: `ease`, `spring`, or `snap`, the default): on the frames' clock, in
  the content's coordinates, so that scrolling does not move it; the page is not asked for a frame of
  its own for it (`Page::animating` is true while it goes), it is lit while it goes, and with Reduce Motion
  it is at its place at once.

- **A caret's blink is the page's to say**: `Setup::blink` and `Opening::blink` are gone; the caret
  blinks by the options of the editor that has the keyboard (`caret.blink`, `blink_interval`,
  `blink_for`), still with Reduce Motion, and away from the keyboard it is what `caret.unfocused`
  says. The pictures (`shot`) draw a box's soft shadow (three box blurs), so a caret's light and a
  popover's shadow show in them.

- The example `text-lua` (hints, a code lens, a card by the word the pointer rests on, a link while ⌘ is held)
  and the pictures of it at rest, with a card and with a link.

- **The pointer's shape when only the modifiers changed it** (`Drained::cursor`), the modifiers a key,
  a press or `Input::Mods` says (the drive keeps them), and the headless host's and `pencere test`'s
  `hold`, `release` and `click_holding`.

- **The pointer's rest is timed on the frames' clock** (`Page::wake_at` has `rest_due`; the drive
  keeps the modifiers it was told of), `Page::anchor_rect`, and the headless host and `pencere test`
  can `dwell(element, x, y)` and say where a range the page named is (`anchor_rect`).

- **The gutter and the pointer's shape.** Every pointer move is the editors' as well (the marker
  under it is lit, the page told of a cell it came to and went from where its column hears that,
  with no frame for a move that changed nothing), and a press of another button is told to a column
  that hears menus. The shape of the pointer over an editor's room is decided here, for every host:
  the I-beam over the text, the arrow in the gutter and the hand where something answers a press
  there, unless the page asks for a shape that means something it does itself.

- **How long a call of a page may take, from the environment.** `headless::CALL_MS`
  (`PENCERE_CALL_MS`, in milliseconds) is the time a call of the page may take where a test does not
  say (`Opening::limits`): the floor's 250 ms unless it is set. The limit is the wall clock's, and a
  test of what a page shows and does failed with `Ended(Timeout)` on a runner that was busy, for
  nothing the page did; a gate that runs on one sets it, and a run on a machine at rest holds the
  page to the floor.
- **The system the host with no screen says it is.** `Opening::wall_clock(seconds, utc_offset)` is
  the time of day when its clock reads 0 (`WALL_ORIGIN` in UTC unless said), going on with its
  clock; `Opening::keyboard` says how its keys are written (a Mac's unless said), and
  `Headless::press` reads `mod` by it. A picture of a page (`shot`) draws a picture turned where it
  spins.
- **An editor says it is ready.** A page is told `Event::EditorReady` for each editor the drive
  makes for its rooms, shown or hidden, its text posted first: a page of tabs loads each document as
  its editor comes, and asks nothing of an editor that is not there yet.
- **Fields as a system has them.** The host with no screen keeps a caret and a selection in each
  field: what a reply's `fill` selects is selected (`Headless::field_selection`), what is typed
  takes its place, the arrows move and select, ⌘A, ⌘C, ⌘X and ⌘V are the field's, and a key the
  field has no use for where its caret is goes to the page by the kit's rule (`FieldState::takes`):
  ⌫ in an empty field, ← at its start, a chord it does not know. `Host::fill` is told a `Fill`.
- **The page, driven.** `Page`: a component, a script or a module of the host's own making, started
  on its thread; a line in its place until its first page comes, and its end told once. What the
  person does is told it in the host's points (`input`, `key`, `key_up`, `wheel`, `text`, `focus`,
  `pick`, `command`, `files`), and what its program says (`message`, `room`); the host never waits
  for it.
- **Frames.** `Page::frame` lays the page out at the host's size and scale, asks the module for a
  page when the size changed (waiting a little for it), when it moves or when its time has come,
  moves what says a `motion`, and paints it with every editor in its place. `animating` and
  `wake_at` say when the next frame is wanted; a page at rest wants none.
- **Scrolling.** The wheel moves the box under the pointer at once, on the page as it was laid out;
  the bars answer the pointer before the page; the page's `scroll_to` is done before the next
  layout; the page is told where its boxes went; a turn no box took is the page's. `Scrolling` is
  how the host's system scrolls.
- **Editors.** The kit's editor in each editor's room: what the page's replies ask of them, the
  keyboard where a reply sends it, the layout again where a caret an overlay stands by moved; the
  pointer over an editor puts its caret, drags and gives the keyboard back to the page outside every
  room; a key goes to the page's command for it, the editor's own, then the text (`Page::key`,
  `Route`); typing, pasting and an input method in units of UTF-16 (`PageEditors`), a press ending
  what it composes; the texts posted before the events that tell of them; the caret blinked on the
  frames' clock (`Setup::blink`).
- **Wakes.** A page is woken at the first frame at the time it asked, once a frame however often it
  asks, and a second apart while the host's view is not seen (`Page::wake_unseen`), with no wake
  lost; `Drained::wake` says the time changed when no frame took it.
- **Images.** The page's pictures decoded off the thread from the host's files (`Setup::assets`),
  their sizes in the layout, a frame when they come, a word for the page's author for one refused.
- **Where the page stands.** `Placement`: a strip of the host's above it and the page drawn larger
  than its own points; every point told and told back is the host's. `Outline` (the fields, what a
  screen reader says, the tooltips, the commands) comes with the frame that changed it.
- **What the host does.** `Page::drain` hands over what the page said, `Drained::deliver` hands it
  to a `Host` in the order every host does it: the program's messages, each reply (its close, its
  redraw, the pointer's shape for an answer to the pointer only, the fields' texts, the keyboard,
  the pasteboard, the link, the menu last), the images' diagnostics, the end, then the frame or the
  clock it wants.
- **One clock.** The frames, the editors (what is undone together) and the scrolling between frames
  run on the host's clock where its program keeps one (`Program::now`), so that a host whose clock is
  driven by hand drives everything the page does. The program's readings and the frames' times
  (`FrameAt::now`) must be that one clock; a reading more than a second behind the last frame, which
  would stand time still between frames, stops a debug build and is taken for no reading in a
  release build.
- **Editors without the pointer.** `PageEditors::focus`, `select` and `command`: the keyboard put in
  an editor, a selection made and an editor's own command done the way a screen reader or a host's
  menu does them. `Page::key` sends a key to the editor with the keyboard first and to the page where
  none has it, and a press ends what an input method composes (what it composed stays): a host lets
  its own system forget it, and does nothing else for it.
- **For a host that drives a page by hand.** `Page::unsettled` (the last frame left a layout for the
  next, not motion), `Page::asks_at` (the times the page itself asked for, not the caret's blink),
  `Page::refresh` (the page asked for itself again), `Page::record` (every event told and every reply
  whole kept: `Page::told`, `Drained::replies`), `Page::module`. An animating page is asked for its
  next step at a later time only: a frame built again at one time asks for nothing more.
- **A host with no screen** (`headless`): a page run as a terminal runs it, with a clock that moves
  only when the test moves it (`advance`, `step`, `idle`, `hide`: every frame and wake on the way, at
  its time), a person whose hands are methods (keys, typing, an input method, the pasteboard, clicks
  by an element's id, drags, the wheel, a box scrolled to a place, menus, fields, files, commands)
  and eyes that read what the page shows (the words drawn, an element's words, an editor's document,
  the keyboard's place, the pointer's shape, the pasteboard, the links, the menu, what was said to
  the program and told the page). Every method settles the page before it returns, waiting for
  nothing by the clock: the sandbox says when the page has done what it was told, the images when
  they are decoded. What an editor undoes together goes by the same clock: typing with no `advance`
  between is one group, and an `advance` past `UNDO_PAUSE` starts another. The program is a `Peer`; `Calls` is a table of calls in this process. A page's
  files come from a directory (`Files`, `Files::app`). What cannot be done is an `Error` in words.
- **Pictures of a page** (the feature `shot`): `Headless::shot` and `shot::png` draw a frame's list
  into a PNG in the caller's fonts (`shot::Fonts`, which measure as they draw) and a terminal's
  theme (`shot::Palette`); with no font each character is a box, and the picture is the same bytes
  on every machine. A page written in Rust opens under the host with no screen too
  (`Headless::spawn`).
- **A program in another process.** `Peer::closed` (the program ended its view: `Error::Closed`),
  `Headless::wait_for` (what a program that keeps its own time says, waited for) and
  `Headless::received` (what the program said to the page). A settling waits for the answers a
  busy program owes only while the page is at rest: a page that moves goes on at the frames of the
  clock, and what the program waits for may be what it says at them.
- **A program in the same process** (the feature `program`): `headless::InProcess` serves a page
  the handlers a program gives `pencere::serve`, by the program library's own `Conversation` — no
  terminal, no pty. A settling waits for the answers its handlers owe, and `wait_for` for what they
  push. What the program does to its view ends the page's as a terminal ends it: a view closed, one
  shown in its place, a terminal lent to a command (`InProcess::lent`, `InProcess::give_back`); the
  test opens the page anew with the same program (`Headless::into_peer`), and `Peer::opened` tells
  a peer a page was opened to speak to it. The crate is published after the program library.
- **A test that waits in its own way.** `Headless::tick` is one round of `wait_for` (a moment for a
  program that keeps its own time to speak, and a frame of the clock), and `Headless::fill_field`
  types into one of the outline's fields as `fill` does into one a test names.
- **What a host with no screen is held to.** `Files` gives a file the media type its program gives
  it (`pencere_wire::bundle::media_type_of`), and `Files::app` carries what the program's bundle
  carries: the scripts of `src/view/`, the pictures of `src/assets/`, nothing hidden.
  `Page::set_zoom` changes nothing for what is not a size.
