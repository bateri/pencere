# Changelog

All notable changes to the `pencere-drive` crate are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the crate follows
[semantic versioning](https://semver.org/).

## [Unreleased]

The drive of a page of version `2.0` of the kit (`pencere:view@2.0.0`, `lua` `2.0`).

### Added

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
  (`FrameAt::now`) must be that one clock; a debug build stops on a reading more than a second behind
  the last frame, which would otherwise stand time still between frames.
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
- **What a host with no screen is held to.** `Files` gives a file the media type its program gives
  it (`pencere_wire::bundle::media_type_of`), and `Files::app` carries what the program's bundle
  carries: the scripts of `src/view/`, the pictures of `src/assets/`, nothing hidden.
  `Page::set_zoom` changes nothing for what is not a size.
