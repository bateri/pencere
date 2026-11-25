# Events

## What a page is told

`handle` is given `{ type = ..., ... }` in the page's points. Each event has a `type` and the members said below, in groups.

### The pointer and the fingers

- `move`, `leave`: `x`, `y` and, for `move`, `mods` (`leave` has none).
- `press`, `release`: `x`, `y`, `button` (`"left"`, `"right"` or `"middle"`) and `mods`.
  - `press` has also `count`: 2 for the second of a double click, as the system counts them.
- `extra_press`, `extra_release`:
  - `x`, `y`;
  - `button`: a button of the pointer beyond the third, counted from 1 as the system does: 4 is the first of them, a mouse's "back", and 5 its "forward";
  - `mods`.

  They are told as their own events and never as a `press` or a `release`.[^extra]

- `wheel`: a wheel that no box the host scrolls took. It is for a page that does something of its own with the wheel.
  - `x`, `y`;
  - `dx`, `dy`: the points the content would move left and up by;
  - `phase`: `"wheel"` for a wheel that turns in steps, or a trackpad's `"begin"`, `"move"`, `"end"` and `"coast"` (the momentum after the fingers left). The end of the fingers and the end of the coasting are both `"end"`;
  - `precise`: the points are a trackpad's, the fingers', and not a wheel's steps;
  - `mods`.

  A turn that moves nothing is not told. That is so of a gesture's beginning and end that carry no points: the box under the pointer has the gesture.

- `magnify`: `x`, `y`, `by`, `phase` (`"begin"`, `"move"` or `"end"`) and `mods`. It is the fingers of a pinch that no box that zooms took ([Layout and scrolling](layout.md)).
  - The content is to be `1 + by` times as large as it was at the last event of the gesture, `by` negative as the fingers come together.
  - `x`, `y` is where they are, the point to keep in place.
- `smart_magnify`: `x`, `y`. It is a double tap with two fingers, the trackpad's request to show the content larger or at its size.
- `rotate`:
  - `x`, `y`;
  - `degrees`: since the last event of the gesture, positive counter-clockwise as a person sees the page;
  - `phase`: as `magnify`'s;
  - `mods`.
- `swipe`: `dx`, `dy`, each -1, 0 or 1. `dx` is 1 to the right and `dy` is 1 down. It is a swipe of three fingers, one step.
- `pressure`: the force of a press on a trackpad that feels it.
  - `x`, `y`;
  - `stage`: 0, 1 or 2 (no click, a click, a deep click);
  - `pressure`: 0 to 1.
- `scrolled`: `boxes`, the boxes the host scrolled or zoomed. Each is `{ id =, x =, y =, zoom = }`. It says how far the box is scrolled now, and how many times the size the page gave it the content is shown at. That is 1 for a box that does not zoom.

[^extra]: So a page that takes every button but the left for the one that opens a menu does not open one for these.

### Keys and fields

- `key`:
  - `key`: `"up"`, `"down"`, `"left"`, `"right"`, `"page_up"`, `"page_down"`, `"home"`, `"end"`, `"enter"`, `"escape"`, `"tab"`, `"backspace"`, `"delete"`, `"f1"` to `"f20"`, or the character;
  - `mods`.
- `text`: `field` (its name) and `text`, what a field holds now, after the person changed it (not after the page's own `fill`).
- `focus`: `field`, the field the person clicked into, or nil when the keyboard went back to the page of itself. It is not said when the keyboard went where the page sent it.
- `menu`:
  - `index` (from 1) of the entry picked from the menu shown;
  - `entry`: the table the page wrote for that line, as it wrote it.
- `key_up`: a key the page was told of went up. It has `key` and `mods`, as `key` has.
- `mods`: `mods`, the modifiers held, changed with no other key.
- `command`: `id`, the command asked for, from the host's list or by its keys. The key is not told as well.

### Files, links and drags

- `files`:
  - `x`, `y`;
  - `source`:
    - `"drop"` on the page at `x`, `y`;
    - `"paste"` at the caret;
    - `"pick"` in the Open panel the page asked for, with none when the person turned it away;
  - `files`: each `{ handle =, name =, size =, type =, path =, kind = }` ([Replies](replies.md));
  - `paths`: those the files have.
- `files_over`: `x`, `y`, `count`. Files dragged from outside the page are over it, for showing where they would go.
- `files_left` (no field): they went from it without being dropped.
- `link_dropped`: `x`, `y`, `url`. The person dropped an address on the page, from another application or from the terminal. It is an `http` or `https` link. The page opens it (in a browser of its own, or by asking the host to) or leaves it. A kit that has `pencere.has("event.link_dropped")` tells it.
- `carried`: the system's drag is over.
  - `id`: the element whose `carry` the system dragged out of the view;
  - `done`: it was let go where something took it.

  The pointer's moves and its release did not reach the page while it went, so neither a `release` nor a drop follows.

- `saving`: `handle` and `name`, the place the person chose in the Save panel the page asked for (neither: turned away).

### What an editor tells

- `edit`:
  - `editor`;
  - `version`: what the document came to;
  - `origin`: `"person"`, `"compose"`, `"page"`, `"undo"`, `"redo"`, `"load"`;
  - `tag`: the page's name for a change it asked for with one;
  - `changes`: `{ { from =, to =, insert = }, ... }`, in the places of the version before, in order.
- `selection`:
  - `editor`, `version`;
  - `ranges`: `{ { anchor =, head = }, ... }`;
  - `main` (from 1);
  - `composing`: an input method is composing at the main caret. The host does none of the page's commands then.
- `viewport`: `editor`, `first`, `last`, the lines in view (from 1). It is for a page that colours only what is seen.
- `editor_refused`: what the page asked was not done, none of it.
  - `editor`;
  - `reason`: `"reversed"`, `"out-of-document"`, `"overlapping"`, `"stale-version"`, `"future-version"`, `"too-large"`, `"too-many-decorations"`, `"outside-range"`, `"bad-pattern"`, `"no-editor"`, `"mid-character"`.
- `found`: what its search finds ([Marks, zones and searches](editor-marks.md#keys-commands-and-searches)).
  - `editor`, `version`;
  - `count`: at most 10 000;
  - `current`: the match the main selection is exactly on, from 1; nil for none;
  - `more`: there may be more than `count`.

### The clock and the terminal

- `wake`: `now`. The time the page asked to be woken at (a reply's `wake`, [Replies](replies.md)) has come. It is in seconds on `render`'s clock. It is told only to a page that asked, after the timers that were due ran ([Layout and scrolling](layout.md)).
- `terminal_look` (no field): what `pencere.terminal_look()` says is not what it was. The person changed a setting of the host's terminal that is its cursor's or its selection's.
  - It is told once for each change, and not for what was the same.
  - The page reads it again and answers as it answers any event. A page that takes it for its editors answers `true`: the editor's options are asked again.

### What an editor's pointer and keys tell

- `gutter`: the pointer in a gutter column that hears it ([Marks, zones and searches](editor-marks.md#marks-zones-and-widgets)).
  - `editor`, `column`;
  - `line` (from 1);
  - `kind`: `"press"`, `"context"`, `"enter"`, `"leave"`;
  - `button`, `count`;
  - `mods` (of a press);
  - `marker`: `{ id =, layer =, from =, to = }`, nil where the cell is empty;
  - `cell`: `{ x =, y =, w =, h = }`, the page's points.
- `hover`:
  - `editor`;
  - `kind`: `"dwell"` or `"leave"`;
  - `at`: the place under the pointer;
  - `from`, `to`: the word it is on, or the run of marks: a double click's;
  - `line` (from 1);
  - `hits`: `{ { layer =, id =, from =, to = }, … }`, the styles with an `id` that hold `at`;
  - `word`: its box, the page's points;
  - `mods`, `version`.

  In an editor with `events = { "hover" }`, the pointer rested `hover.delay` over a word, or went from one it was told of.

- `target`:
  - `editor`;
  - `kind`: `"enter"`, `"leave"`, `"press"`;
  - `source`: `"word"`, `"link"`, `"widget"`;
  - `at`, `from`, `to`;
  - `id` (of a link);
  - `word` (the box);
  - `mods`, `button`;
  - `count` (of a press);
  - `version`.

  In an editor with `events = { "target" }`, it is told:

  - when the pointer came to a word (or went from it) while exactly the modifiers of the press that is the page's (`gestures`: `"target"`) are held, at most once in `links.delay`;
  - for that press.

  It is `source = "link"` where the place under the pointer is on a look that is `held`, has an `id` and is drawn now.

- `keymap_pending`:
  - `editor`;
  - `keys`: the chord's keys so far, as written. It is none when it is not waited on any more.
- `editor_ready`: `editor`, `version`. The host made the editor, as the page was laid out with it, shown or hidden. Its text is posted before this is told. What the page asks of it is done from now on. A page loads a document into an editor it has just made (a tab's that is not in front too) when it hears this.

### Browsers

- `browser`: all of what the browser is, told each time any of it changes.
  - `browser`: its name;
  - `url`: where it is; empty before the first load;
  - `title`: empty where it has none;
  - `loading`;
  - `back` and `forward`: there is a page to go to;
  - `status`: `"ready"`, `"starting"`, `"crashed"`, `"no_route"`, `"unsupported"`, or a word a later host says;
  - `message`: a sentence for the person in every status but `"ready"`, else nil;
  - `error`: `{ url =, code =, message =, certificate = }` for the load that failed, until the next begins, else nil;
    - `certificate` is there only where the load failed because the browser did not trust the site's certificate. It says why in a few words: `"self-signed"`, `"expired"`, `"wrong host name"`, `"revoked"`, `"unknown issuer"`, `"not trusted"`, or a word a later host says. The page may then offer to go on, `"proceed"` ([Browser](browser.md));
  - `found`: `{ count =, current = }` while a search is on, `current` from 1 and nil where there are no matches, else nil;
  - `zoom`: 1 is the page's size.
- `browser_refused`: what the page asked was not done.
  - `browser`;
  - `reason`:
    - `"scheme"`: the address is not of a kind a browser goes to;
    - `"address"`: it is not one it is given;
    - `"ask"`: the ask is not one the kit knows;
    - `"pattern"`: what it was asked to find is not a search's.
- `browser_window`:
  - `browser`, `url`;
  - `background`: the person asked for it behind the page: a click with the command key, a click of the middle button.

  A link of a browser whose `windows` is `"page"` opens a new window, for the page to open or not (in a tab behind, where `background`).

### Modifiers

`mods` is `{ shift =, control =, alt =, command = }`.

### A place under the pointer

`move`, `press` and `release` over an editor have `place`, `{ editor =, at =, inside =, version = }`: the place of its document under the pointer.[^place] `inside` says the place was in a selection or at a caret before a press moved the caret.

A press in an editor is the editor's as well: it puts the caret.

[^place]: A page uses it to say what a word is of while the pointer rests on it, and to go to a name at a ⌘-click.

### The pointer's rest

The pointer's **rest** is told to a page that asks (`events = { "hover" }`).

When the pointer stays `hover.delay` (0.3 s) over a word, the page is told `hover` with `kind = "dwell"` and the word's range and box. The word is the run a double click takes. The pointer is not at rest over spaces, the gutter, a gap, or past the end of a line.

The page is told the `hits` too: the styles with an `id` that hold the place under the pointer (a diagnostic, a link). So the page does not search its own lists.

It is told once however the pointer moves within the word. When the pointer goes from a word it was told of, the page is told `kind = "leave"` at once. What a card does then, how long it waits and whether the pointer came onto it is the page's: `@pencere/ui`'s `popover` does it ([The kit's library](library.md#pencereui)).

Each of these ends a rest:

- a scroll, a key, a press, an edit;
- a window that no longer has the keyboard;
- a button held (a drag).

Each tells the `leave` where there was a `dwell`, and tells nothing more until the pointer moves again.

An answer that comes late is written with the `version` the event carries. The editor carries it over what was typed meanwhile as it does any decoration.

The page reads the word at its own caret by the same rule (`pencere.layout.word_at`), so that a command that does what the pointer's rest does (Show Hover) answers as the rest does.

## What a reply may ask

A reply is one of:

- `nil` (nothing changed);
- `true` or `false` (a frame is worth drawing or not);
- a table.

A table may have `redraw` and `close`, and the members below.

### Copy

`copy`: text for the pasteboard.

A host that keeps programs from its pasteboard (a terminal's `osc52 = "off"`) is told whether the person asked for it. It writes it only if they did.

The person asked for it in a reply to:

- a press or its release;
- a pick in a menu;
- a command the host's list asked for;
- of the keys, only the system's copy and cut, ⌘C and ⌘X. So also a command a key did: a page's command bound to ⌘C is the person's, one bound to any other key is not.

A reply to another key, to a message of the program, to a wake or to the room is not one, whatever the page made the key do.

### Cursor

`cursor`: `"arrow"`, `"pointer"`, `"text"`, `"resize_horizontal"`, `"resize_vertical"`, `"not_allowed"`, `"grab"`, `"grabbing"` or `"crosshair"`.

Every reply to an input of the pointer says it. One that says none, `nil` too, is the arrow. No shape is kept from an earlier reply. So a page that asks for one says it again at each move, press, release and turn of the wheel over what it is for.

### Keyboard

`keyboard`: `"page"` or `{ field = name }`.

### Fill

`fill`: `{ [name] = text }`, or `{ [name] = { text =, select = } }`. It says what fields are to hold from now on, and what is selected in them.

- `select` is `{ from =, to = }`, bytes from 1 ([Counting](counting.md)), in either order.
  - Past the end is the end.
  - An end inside a character is where the character starts.[^mistake]
  - `true` is all of it.
- A field given no `text` keeps what it holds.
- A field given no `select` has its caret where its system puts it after a text, at the end.
- A name that is not a field of the page is passed over ([Replies](replies.md)).

[^mistake]: A reply cannot be told of a mistake.

### Menu

`menu = { at = { x =, y = }, entries = { { title =, keys =, enabled = }, "-", { enabled =, paste = editor } } }`.

- An entry's `keys` are written as a command's, `"ctrl+w"` or `"f2"`, and shown at the end of its line.
- The line of a `paste` has no keys, and what is written there is not read: no key picks it.
- An entry has a `title` (a string, whatever else the page puts in its table: a `key` of its own is the page's and is not read as keys).
- The one string an entry may be is `"-"`, a line between entries.
- A menu without `entries` is an error.
- An entry with `paste` is one the host performs itself when it is picked ([Replies](replies.md)).
- The table of an entry is told back with its pick, so a function or a field the page put in it is there when it is picked (`menu`, [Replies](replies.md)).

### Link, wake and text size

- `link`: an address to open in the person's browser ([Replies](replies.md)).
- `wake`: in how many seconds to wake the page ([Replies](replies.md)).
- `text_size`: how many points larger than the host's own text the view is drawn ([Replies](replies.md)).

### Haptic

`haptic`: `"tick"`, `{ name =, intensity = }`, `{ events = { … } }` or `false`: the trackpad's vibration, or the end of the one playing ([Haptics](haptics.md)).

### Sound

`sound`: `"success"`, `{ name =, volume =, pitch = }`, `{ file =, volume =, pitch = }`, a list of these or `false`: the sounds to play, or the end of every sound the view plays ([Sounds](sounds.md)).

### What the page asks of its browsers

`browser = { [name] = ask, or { ask, ... } }` is what the page asks of its browsers.

- An ask is `{ go = address }`, `{ zoom = size }`, or `{ command = "back" | "forward" | "reload" | "stop" | "find" | "find_next" | "find_previous" | "stop_finding" | "devtools" | "clear_cache" | "clear_site_data" | "proceed", ... }` ([Browser](browser.md)).
- There are at most 64 asks in a reply, and a page that asks more fails.
- The address is held to its rule, a zoom to its range, a pattern to a search's.
- An ask the kit does not know is refused, and a browser the host has not made is passed over.

### What the page asks of its editors

`editor = { [name] = ask, or { ask, ... } }` is what the page asks of its editors. Each ask is one of:

- `{ load = text, more = true }`: a piece of the document.
  - The first is in place of the whole text. Each after it (`more` said before) is on its end.
  - Nothing of it is undone, and what could be undone before cannot be.
- `{ change = { { from =, to =, insert = }, ... }, base = version, history =, tag = }`: changes against the version the page saw.
  - They are carried over what was done since (a range's start after what came in at it, its end before), done, undone as one.
  - Where `history` is `false`, they are never undone. What can be undone or done again is carried over them as over any change after it (CodeMirror's `addToHistory: false`).
  - `tag` is told back in their `edit`.
- `{ select = { { anchor =, head = }, ... }, version =, main = }`: the selections, in the places of `version`.
- `{ reveal = place, version =, align = }`: the place scrolled into view:
  - as far as it takes, and not where it shows (`"nearest"`, the default);
  - or its row at the top of the room (`"start"`), in its middle (`"center"`) or at its bottom (`"end"`).
- `{ command = name, ... }`: what the person could do with a key.
  - `"move"` (`movement`, `extend`);
  - `"insert"` (`text`);
  - `"undo"`, `"find"` and the commands of a search, those of whole lines ([Marks, zones and searches](editor-marks.md#keys-commands-and-searches)), … (`pencere_view::edit::EditCommand`).
- `{ decorate = layer, version =, from =, to =, decorations = { ... } }`: the layer's decorations for `from`–`to`, in place of those it had there. The other layers stay.

### A pick is of the menu that was shown

A host shows one menu at a time. A menu a reply asks for while another is up is not shown, and the page is not told.

So that a pick is never read against a menu the person did not see:

- the kit gives each menu a reply asks for a number (`serial`, not the page's to choose), and keeps the tables of its entries for the last sixteen;
- the host hands the number of the menu it showed back with the pick (`pencere:view` `menu-pick`: the `serial` and the `index`);
- the page is told the pick with that menu's `entry`, the table the page wrote for the line, with its fields and functions.

`index` stays the line's place in that menu.

A pick of a number that is not kept (a menu asked for long ago, a menu picked from already, a number the page never had), or of a line that is no entry (a rule, a place past the end), is not told the page. The entry of a menu the person was not shown is not reachable by any pick.

A page does not map `index` to its entries itself: `entry` is the entry.

### A press that opens a menu

`gestures` with `"context"` (`["right+press"] = "context"`) puts the caret where a text view puts it for a press that opens a menu (VS Code's rule), and has the editor take the keyboard.

- Outside every selection it puts one caret at the place.
- Inside one (at its ends too, and at a caret) the selections stay.

The press is then told the page as any (`press` with its `button`), with the `place` it has: `{ editor, at, inside, version }`. `inside` says the place was in a selection or at a caret before the press moved the caret, and `version` is the editor's after.

The page opens its menu from that `press` (`menu` in its reply, at the press's `x` and `y`). It asks `copy`, `cut` and its other commands of the editor when the entry is picked. Those carry no version. An ask that does takes the editor's current one (`pencere.layout.editor_version`), not the press's.

A second press of the quick run opens the menu as the first does. No `events` word is needed: the gesture is the page's asking.

A host whose system makes ⌃ with the main button a right press (a Mac's) may send it as the main button with ⌃. The drive takes it for the right press where the page's `gestures` say a right press does what opens a menu and say nothing of ⌃ with the main button. It tells the page the press as the right button's, without the ⌃ (so does its release).
