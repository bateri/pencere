# Changelog

All notable changes to the `pencere-drive` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

### Changed

- A page's pictures are measured and drawn for the page of the file they show (`Images::size` and
  `Images::bitmap` take it): a page of a PDF is laid out at its own size and drawn at the size it is.

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
