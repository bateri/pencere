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
  driven by hand drives everything the page does.
