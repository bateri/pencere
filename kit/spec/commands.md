# Commands and dragging

## Commands

A **command** (`command(id, title, keys, enabled)`) is something the page can be told to do. The host lists it by its `title` where a person looks for what can be done (its command palette, its menus), and tells the page `{ type = "command", id = id }` when it is asked for there or by its `keys`, in place of the key.

### The keys of a command

The keys are written as a person writes them: modifiers (`cmd`, `ctrl`, `alt`, `shift`) and a key (one character, or the name of a key the page reads) joined by `+`. They are `"cmd+shift+k"`, `"ctrl+enter"`, `"f5"`, and `"cmd++"` for the key `+`.

`pencere.keys(words)` reads them. A modifier or a key by another name (`option`, `esc`, `pageup`, `return`) is an error, here and in `gestures`, `keymap`, `held` and `hold`.

A letter is the same key in either case. A character that has no case (`?`, `/`, a digit) is its key whether shift typed it or not, since one keyboard types it with shift and another without (`/` is shift and 7 on a Turkish one). Keys that name `shift` still want it.

`mod` is the main modifier of the host's system (CodeMirror's `Mod`, Electron's `CmdOrCtrl`): `"mod+s"` is ⌘S on a Mac and Ctrl+S elsewhere.

`pencere.keyboard()` says how the host's system makes and writes its shortcuts:

- `"mac"`: ⌘ the main modifier, the keys written as signs, ⌘⇧K;
- `"windows"`: Ctrl the main one, the keys written as words, Ctrl+Shift+K;
- `"linux"`: as Windows, the logo key Super.

What a page shows of a shortcut follows it.

A command that more than one set of keys does has a list of them (`{ "ctrl+page_down", "ctrl+alt+right" }`), as an editor's keybindings do. Each of them does it, and a list of the commands shows the first.

### Where a command is done

A command can be done while the keyboard is on its element or inside it. The page says which element has the keyboard with `focused()` (the element its `Ui` says has the focus).

The commands of the page's outermost box can be done whatever has the keyboard.

Where two say the same keys, the one nearer the keyboard is done. One that is not `enabled` is listed and its keys do nothing (they are a key for the page).

## Dragging

- An element that can be **dragged** (`draggable(id)`) answers the pointer as a pressed one does.
- A **drop target** (`drop_target(id)`) is where what is dragged can be put down.

The page's `Ui` keeps the drag ([Layout and scrolling](layout.md)). What the page draws of it while it goes (an overlay at the pointer, the target marked) is the page's.

## Carrying

An element can be **carried** out of the view: `carry({ text = "words" })` or `carry({ path = "/a/file" })`, and `url = "https://..."` besides for the applications that take an address.

The element has a name already (`id`, or `draggable(id)` first). A person who presses on it and moves the pointer `pencere.DRAG_START` points (the host's own number) hands it to the host's system drag, to a terminal pane or another application.

An element that is also `draggable` is the page's own drag while the pointer is in the view, and the system's from the moment it leaves. The page does not hear the pointer move or let go while the system carries. It is told `carried` when that is over ([Events](events.md#what-a-page-is-told)).

### What may be carried

What a page says to carry is one line:

- at least 1 byte and at most 64 KiB;
- with no line break and no control character: the C0 characters, DEL, the C1 characters, U+2028, U+2029, U+FEFF and the controls of the direction of text, U+202A to U+202E and U+2066 to U+2069;
- and its `url` is a link a host opens ([What a host does for a kit view](host.md)).

Anything else is the script's mistake where `carry` is called (the protocol, Section 12.3, has why).
