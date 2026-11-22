# pencere-drive

The drive of a page of [pencere](https://github.com/bateri/pencere)'s native views. A page of the kit
is a Lua script in a sandbox (`pencere-sandbox`) that hands out a tree of
boxes and lines of text (`pencere-view`) and answers what the person does. Between the two stands
what the kit's specification (`kit/SPEC.md`, Section 4) asks of every host: the page laid out with
its editors (`pencere-editor`) in their rooms, its boxes scrolled, a key sent to the page's command
for it, to an editor or to the page, its replies done, its wakes given, what it says to its program
carried. This crate is that, written once, so that every terminal does it the same way.

It depends on the engine, the editor and the sandbox (and the messages of a program and its view,
for the program of a page under its host with no screen), and on no platform, no renderer, no font
library and no terminal (`make check`).

## A host's loop

```rust
use pencere_drive::{FrameAt, Host, Page, Setup};

// The page, its text measured by the host's own measure (the object that measures is the object
// that draws). `woken` is called from other threads; it only has the host's main thread look.
let mut page = Page::script(script, Setup { measure: Box::new(my_measure), ..Setup::default() }, woken);

// What the person does, in the host's points.
page.input(input);                 // the pointer, a key where no editor has the keyboard
let route = page.key(key, mods);   // a key: the page's command, an editor's, the text, the page's
page.wheel(at, dx, dy, phase);     // the box under the pointer scrolls, at once

// When woken, or after telling it something: what it said, done in the order every host does it.
page.drain().deliver(&mut my_host); // impl Host: pasteboard, pointer, fields, link, menu, program…

// A frame, when the host draws one: the draw list in pixels, with the editors drawn in.
let mut painted = page.frame(FrameAt { size, scale, now, reduce_motion });
// Your fields for painted.outline, then what replies asked of fields this frame brought (their
// texts and the keyboard), which the drain held back until you had them:
painted.deliver_held(&mut my_host);
// The next one: now if `page.animating()`, else at `page.wake_at()`.
```

The host never waits for the page: it tells it, and takes what came back when it is woken. A page at
rest costs nothing — no frame, no wake.

## What is the drive's and what is the host's

Every part of what bateri's pane did to drive a page is in one column of this table.

| What                                                                                                                                                           | Where                                                                    |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| The module started, its first page waited for with a line in its place, its end told once                                                                      | `Page::script`, `spawn`; `Drained::ended`                                |
| Taking what the module put out: pages, replies, what it said to its program, scrolls it asked, wakes                                                           | `Page::drain`, `Page::frame`                                             |
| Laying the page out at a size and scale, laid out again only when it or its images changed                                                                     | `Page::frame`                                                            |
| Asking the module for a page: a new size (waiting a little for it), the next step of a moving one, its time                                                    | `Page::frame`                                                            |
| Moving what says a `motion` between the module's frames                                                                                                        | `Page::frame` (`Motions`)                                                |
| Scrolling: the wheel on the box under the pointer, the bars before the page, the page's `scroll_to`, telling where boxes went                                  | `Page::wheel`, `Page::input`, `Scrolling`                                |
| A key that does a command told as the command; a command picked from the host's list                                                                           | `Page::input`, `Page::key`, `Page::command`                              |
| The editors: made for their rooms, the replies' asks and keyboard done, the layout again for a caret an overlay stands by, drawn in their places               | `Page::frame`                                                            |
| The pointer over an editor (caret, drag, the keyboard taken back), the place under it told the page                                                            | `Page::input`                                                            |
| A key to an editor (the page's command first, the editor's, the text), typing, pasting, an input method in UTF-16, a press that ends what it composes          | `Page::key`, `PageEditors`                                               |
| The texts of the editors posted before the events that tell of them                                                                                            | `Page` (every edit)                                                      |
| A caret's blink on the frames' clock, still with less motion, dark away from the keyboard                                                                      | `Page::frame`, `Page::set_active`                                        |
| Wakes: at the first frame at their time, once a frame; a second apart while not seen, none lost                                                                | `Page::frame`, `Page::wake_unseen`, `Drained::wake`                      |
| The page's images: decoded off the thread, their sizes in the layout, a frame when they come, a word for one refused                                           | `Page` (`Images`), `Drained::diagnostics`                                |
| The page placed in the host's view (a strip above it, drawn larger): every point in and out turned                                                             | `Placement`, `Page::set_zoom`                                            |
| The fields, what a screen reader says, the tooltips and the commands, told when they change                                                                    | `Painted::outline`, `Page::outline`                                      |
| What the page said, handed over in the order a host does it                                                                                                    | `Drained::deliver`, `Host`                                               |
| What a reply asked of a field its own frame brings (a find bar opened, filled, and given the keyboard in one reply), held and handed over with that frame      | `Page::drain` (holds), `Painted::held`, `Held::deliver`                  |
| One clock for the frames, the editors and the scrolling: the host's where its program has one, the frames' times on it too (a debug build checks)              | `Setup::program` (`Program::now`), `FrameAt::now`                        |
| **The host's:** measuring and drawing text in its fonts, the draw list in its renderer and theme (colours, the contrast floor, figures and images as textures) | the host (`Setup::measure`, `Page::images`)                              |
| **The host's:** its keyboard (its keys turned into `Key` and `Mods`), its input method, its own shortcuts                                                      | the host (`Page::key`, `PageEditors`)                                    |
| **The host's:** its pasteboard, its pointer's shapes, its text fields over the page, its menus, its browser and the question asked first, its screen reader    | the host (`Host`, `Painted::outline`)                                    |
| **The host's:** the clock that draws frames and the one that wakes a view not seen                                                                             | the host (`Page::animating`, `Page::wake_at`, `Page::wake_unseen`)       |
| **The host's:** the program and the session it speaks through, what it can take of what the page says                                                          | the host (`Setup::program`, `Page::message`, `Page::room`, `Host::send`) |

## A host with no screen

`headless` is a host of this crate's drive with no screen: a page run as a terminal runs it (the
kit's interpreter, the kit's editors, the drive), with a clock that moves only when the test moves
it, a person whose hands are methods, and eyes that read what the page shows as words. It is what an
app tests its view with.

```rust
use pencere_drive::headless::{Calls, Headless};
use serde_json::json;

let mut page = Headless::app("examples/containers-lua") // src/view/page.lua and its files
    .size(900.0, 500.0)
    .peer(Calls::new().on("list", |_| {
        Ok(json!({ "containers": [{ "id": "c1", "name": "api", "image": "api:1", "state": "running" }] }))
    }))
    .open()?;
assert!(page.says("api"));
page.click("act c1")?; // by the element's id: its middle, where a click reaches it
assert_eq!(page.methods(), ["list", "toggle"]); // no handler: it waits for the test's answer
page.press("cmd+enter")?; // keys as a page writes a command's
page.advance(1.5)?; // every frame and wake on the way, at its time
```

**Every method is done when it returns, and nothing is waited for by the clock.** The page is told,
then settled: the sandbox says when the module has done all it was told (`Module::flush`), the images
say when they are decoded (`Images::busy`), the replies are done as a host does them, the program
answers, and the frames that come of it are drawn — at the time the clock shows. Time moves only by
`advance` (every frame the page wants on the way: sixty a second while it moves, at its times, a
caret's blink), `step` (to the next time the page asked for), `idle` (until it asks for nothing) and
`hide` (the view out of sight: no frame, the page woken a second apart at the soonest). The kit's
timers, a search's `found`, a language read a piece at a time are as determined as the rest, and so
is what an editor undoes together: what is typed with no `advance` between is one group, and an
`advance` past the editor's pause (`UNDO_PAUSE`, a second) puts what is typed after in a group of
its own.

- **Opening:** `Headless::script(source)`, `bundle(files, entry)`, `app(dir)` (`Files::app`: a pencere
  app's `src/view/` with `src/assets/` under `assets/`); then `size`, `scale`,
  `measure` (`Fixed`, 7 × 16 pixels a character, unless said), `peer`, `limits`, `assets`,
  `at`, `patience`, `reduce_motion`, and `open`.
  What a page may take for one call is the floor's 250 ms (the least a host gives a page); `limits` says
  otherwise, and so does the environment for a gate on a busy machine (`PENCERE_CALL_MS=5000`, in
  milliseconds: the limit is the wall clock's, and a test of what a page does should not fail for
  how quick the machine is).
- **What a person does:** `key`, `press("cmd+shift+k")`, `type_text` (a key at a time), `insert_text`
  (what an input method commits), `paste`, `compose`, `unmark`; `click`, `double_click`,
  `right_click`, `click_in(id, x, y)`, `click_at`, `hover`, `move_to`, `leave`, `drag`,
  `drag_between`, `drag_out` (an element that carries, out of the view: `carries`, `finish_carry`), `wheel`, `wheel_at`, `trackpad_at`, `pinch`, `magnify_at`, `smart_magnify`, `rotate`, `swipe`, `force`, `click_extra`, `scroll_to`, `pointer` (any input); `focus_editor`, `select`,
  `caret_to`, `edit(EditCommand)`; `click_gutter(editor, column, line, button)`,
  `hover_gutter(editor, column, line)`, `dwell(id, x, y)` (the pointer rests over a word of an editor that
  hears of it, as long as it takes to be told), `hold(mods)`, `release()` and `click_holding(at, mods)` and, for where things are, `gutter_cell`, `zone`, `zoom_of` (how far a box that zooms is zoomed) and
  `anchor_rect` (an editor's gutter cells, the gaps it keeps open for the page and the ranges it names); `command`, `pick`, `pick_titled`, `fill` (a host's text field),
  `drop_files`, `resize`, `zoom`, `set_active`, `refresh`; the files beside the bundle: `give_files`
  (a drop or a paste, with the handles the host makes), `drag_files_over`, `drag_files_away`, the
  `panel` the page asked for, `choose_files` and `choose_place` (the person's answer in it), and
  `attach` and `detach` (and `Opening::attached`) for the files a program adds for the page to draw.
- **The program:** a `Peer` hears what the page says and answers. `Calls` is a table of calls
  (`on(method, handler)`), whose topics a test pushes to (`push`) and whose calls with no handler
  wait for the test to `answer` them; `tell` says anything to the page as the program. A program in
  another process (`pencere-fake-host`'s `program`: on a pty, as a terminal runs it) keeps its own
  time: a settling of a page at rest waits for the answers it owes (`Peer::busy`), and
  `wait_for(within, condition)` waits for what it says on its own; a program that ends its view
  ends the page's (`Error::Closed`). The program's own handlers in this process are `InProcess`
  (the feature `program`, below).
- **What a person sees:** `words` (drawn, in order), `says`, `words_in(id)`, `tree_words`, `bounds`,
  `editor` (the document: text, selections, version, decorations), `editor_text`, `focus`,
  `field`, `outline` (fields, screen reader, tooltips, commands), `cursor`, `pasteboard`, `links`,
  `menu`, `closed`, `diagnostics`, `ended`; and what went on: `sent`, `received`, `replies`,
  `told`, `list`,
  `laid`, `root`, `now`, `asks_at`.
- **What went wrong** is an `Error` that says it in words: the page ended (`Ended`, a script's
  error with its file and line), something did not come in time (`Slow`), the page never came to
  rest (`Unsettled`, `Restless`), no element of that id (`NoSuch`), one a click does not reach
  (`Unreachable`), no editor, no menu, no such entry, a menu open, no such command, no such key.

### What sqlpen's rig was, here

| sqlpen's `Rig`                           | the host with no screen                                                                                     |
| ---------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `open_with(size, db, options)`           | `Headless::app(dir)` or `bundle(files, entry)` / `script(source)`, `.size(w, h)`, `.peer(calls)`, `.open()` |
| `lay_out`                                | the drive's, in every frame `settle` draws                                                                  |
| `tell` (the editors' events to the page) | the drive's; what the page was told: `told()`                                                               |
| `answer` (the page's calls)              | the `Peer`: `Calls::on(method, handler)`; a call with no handler waits for `answer(method, result)`         |
| `absorb`                                 | `settle`'s                                                                                                  |
| `run`                                    | `settle()`, which every method does; no 400 ms of quiet is waited for                                       |
| `command(id)`                            | `command(id)`                                                                                               |
| `key(key, mods) -> Route`                | `key(key, mods) -> Result<Route, Error>`, `press("cmd+enter")`                                              |
| `type_text(text)` (in one piece)         | `insert_text(text)`                                                                                         |
| `type_letters(text)`                     | `type_text(text)` (a key at a time)                                                                         |
| `caret_to(at)`                           | `caret_to(editor, at)`                                                                                      |
| `select(from, to)`                       | `select(editor, from, to)`                                                                                  |
| `fold()`                                 | `edit(EditCommand::ToggleFold)`                                                                             |
| `scroll(id, y)`                          | `scroll_to(id, y)`                                                                                          |
| `shot(name)`                             | `shot(&fonts)` (the feature `shot`): the PNG's bytes                                                        |
| `text()`                                 | `editor_text(editor)`                                                                                       |
| `words()` (the page's tree)              | `tree_words()`; what is drawn: `words()`                                                                    |
| `says(words)`                            | `says(text)` (drawn words)                                                                                  |
| `methods()`                              | `methods()`; every call: `peer().calls()`                                                                   |
| `press_at_column(id, column, count)`     | `click_in(id, x, y)` (where in the element), `double_click(id)`                                             |
| `press(id, count)`                       | `click(id)`, `double_click(id)`                                                                             |
| the pointer's shape (`cursor`)           | `cursor()`                                                                                                  |
| `now`                                    | `now()`; moved by `advance`, `step`, `idle`, `hide`                                                         |

## A program in the same process (`program`)

With the feature `program`, `InProcess` is a page's program in this process: the handlers it gives
`pencere::serve` (its `pencere::Options`), served by the program library's own `Conversation` — no
terminal, no pty, the same code between. Its handlers answer on their own threads and in their own
time, as a program in another process does: a settling of a page at rest waits for the answers they
owe, and `wait_for` for what they push on their own.

```rust
use pencere_drive::headless::{Headless, InProcess};

let mut page = Headless::app("examples/containers-lua")
    .peer(InProcess::new(handlers())) // the program's `Options`, as its `main` gives them to `serve`
    .open()?;
page.wait_for(Duration::from_secs(5), |page| page.says("api"))?;
```

What the program does to its view ends the page's, as a terminal ends it (`Error::Closed`, with
why), and the test opens the page anew with the same program where a terminal would show it again
(`Headless::into_peer`, then `.peer(program)` of the next opening):

- it **closes** its view (`ViewControl::close`): the program shows none again, and a page opened
  with it is over at once;
- it **shows another** in its place (`ViewControl::reopen`): it speaks to the page opened next;
- it **lends the terminal** to a command (`ViewControl::hand_off`), which nothing runs here:
  `lent()` is the command, `give_back(status)` says how it ended (the program's hand-off is told),
  and the program speaks to the page opened next, as to the view a terminal shows again.

A page is opened with the program only for a view it shows — the first, or one it shows anew: one
opened on a view an earlier page had is over at once, since the program still holds what the
earlier page asked.

The program's back-pressure is the conversation's: the host takes what it said a terminal's read
(64 KiB) at a time, and a handler that pushes faster waits, as it would for a terminal. A page's own (`pencere.send` answered `false`, then `room`) is
not played: the headless host takes everything a page says at once.

## Tested from Lua

`pencere test` is this host with no screen driven from Lua: the page of an app opened with a
stand-in program, the app's own or a program on a pty, found by words and roles, and looked at with
`expect` (the command's README, "Testing an app"). It adds nothing to this crate but what a test
that waits in its own way needs: `Headless::tick` and `Headless::fill_field`.

## What an agent reads

An agent that reads a page and drives it needs what a person sees of it and a name for each thing
to point at. `Page::reading(input)` is that: the page's elements, its fields, the lines its editors
show and what is selected in them, the boxes that scroll, its commands, where the keyboard is and
whether the page is at rest (`reading::Reading`). It is plain data with two writers,
`Reading::text()` for a person or a model and `Reading::json()` for a program; a host with no page
of this crate (a web view whose script made the parts) builds a `Reading` and writes it the same way.
`Headless::reading()` is the same under the host with no screen, so a page's author tests what an
agent will see.

- **A name is six letters** (`reading::reference`): the element's `Id` mixed and written in base 32
  (`k3f9qa`). It is the same at every reading of a page that keeps the element, and the same for
  whoever reads: nothing is remembered between readings. If two elements of a reading come to the same
  name, both are marked and `Reading::find` refuses the name; the whole id (16 hex digits,
  `reading::full`) names either. Plain text has no name.
- **What the host holds, it gives** (`ReadingInput`): the text of its fields and where its keyboard is,
  a field of its own or a browser.
  The value of a secret field is never asked for and is in no reading.
- **A reading is a tree, and the page behind a modal is not in it**: an item that is in a group (a dialog,
  a row, a tab that holds controls) is two spaces in under it; where a modal layer is in front, the page
  behind it is not read and a `note: behind the modal in front, N items of the page, not read` says how much
  it was. The JSON has `format` (2 here; no `format` is the flat reading of before), each item's `parent`
  (the ref of its group) and `behind`. In the brackets of an item: what it is in (`checked`, `mixed`,
  `selected`, `expanded`, `disabled`, `value "…"`), where (`level N`, `N of M`, `rows R, columns C`, `row N`,
  `column N`, `sorted ascending`), and what else (`roledescription "…"`, `description "…"` where the tip says
  more than the name, `keys "…"` of the tip, and `action "…"` for each command the page defined on the
  element: `Headless::command_on` does one, `Headless::reveal` asks for a row to be shown).
- **A reading is cleaned**: control characters are `?`, long words are cut (a `…` says so), line breaks
  stay.
- **A reading is audited** (`Reading::audit`, `Headless::audit`; in `pencere test`, `page:audit()`): the flaws
  that a person who reads by ear and an agent meet first, each with the line of the reading's text it is
  on and the name it begins with (`audit::Finding`): a control with no name (`nameless`), the same words
  twice in a row (`twice`), a control with no id (`no id`), and an element that answers the pointer and
  says it only shows (`misfit`). The list is empty where the page is read well, and a test holds the
  page to that: `assert_eq!(page.audit(), [])` (`audit::report` writes the findings a line each).
- **A reading is pinned by a golden** (a pen's test sets `page:reading()` against a file it keeps; the
  kit's own are `src/goldens/`, one for each component and each state of a layer, and
  `PENCERE_BLESS=1` writes them again): what a change of the model or of a component changes in what is
  read is then the change of those files, which is where it is reviewed. A golden is the reading's
  text; a pen that pins one holds the refs fixed by its own ids.
- **At rest** (`Page::rest`) says whether the module has more to do of what it was told, how many
  calls to the program have no answer (a subscription and what the program pushes are not calls), whether
  an image is on its way and whether the page is laid out for what it last was. It never waits: a host
  asks at every drain and wake. Motion the host draws (a transition, a box that glides) is told and is
  not waited for, so a page that always moves can be at rest; a page whose own program makes a new page
  at every frame is not at rest while it does.
- **`Page::reach(id)`** is where a press reaches an element (its middle, if nothing covers it), the same
  place `Headless::click` presses.

Three things make a page easy to read and drive: a name (`id`) on every element that answers a press,
a title on every command, and a label or hint on every field.

## Pictures of a page (`shot`)

With the feature `shot`, `Headless::shot(&fonts)` draws the last frame into a PNG (and
`shot::png` any draw list): the boxes, the words, the colours of a dark or a light terminal's theme
(`Palette`), the canvases, the pictures and the editors where the kit put them — what an eye checks
(what is aligned, what overlaps, what is cut). `Headless::shot` draws the host's text fields over
the page too, as a system draws its own: what each holds (a secret as dots) or its hint, quiet, its
selection and the caret of the one with the keyboard; `shot::png` draws a list, and a host's fields
are not in one. It is not a terminal's drawing: no shaping, no shadows, no blur.

The fonts are the caller's. `shot::Fonts::installed()` draws in the machine's own (on a Mac SF Mono and
SF, as bateri does, in their bold and italic; a character a face lacks is drawn in the first face of
the machine that has it: a colour emoji, an ideograph, a symbol; one no face has is a hollow box),
`Fonts::carried()` in the faces the program carries (JetBrains Mono for the terminal's family and
Inter for the system's, both SIL OFL 1.1, packed with zlib: what a machine with none is drawn in),
and `Fonts::none().terminal(&bytes)?.system(&bytes)?` in faces of a file's bytes. They measure as
they draw (open the page with `.measure(fonts.clone())`, so that its words fill the boxes they were
measured for). A face with an optical size axis (SF, Inter) is set to the size of the text in
points: say how many pixels a point of the page is with `fonts.scaled(2.0)` (the page's own
`.scale`; the default is one). With no font each character is a box a character wide, and the picture is the
same bytes on every machine: a test can hold a picture to one checked in, and `make check` needs no
font of its system.

```rust
let fonts = Fonts::installed().scaled(2.0);
let page = Headless::app("examples/editor-lua").measure(fonts.clone()).scale(2.0).open()?;
std::fs::write("target/editor.png", page.shot(&fonts)?)?;
```

The pictures are drawn with tiny-skia and the faces read and drawn with swash (out of memory-mapped
files); the session crate that drives pages links none of them, and `pencere-tui` links them to
send a page to a terminal as a picture.

## License

Apache-2.0 — see [LICENSE](LICENSE).
