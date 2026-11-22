# Accessibility

A **label** is what an element is in words, for a person who does not see it: a host that offers its
system's screen reader says the label of an element that has one, and the words inside one that has
none. A button that is an icon alone needs one: where it has none and no words inside it, a control
(a button, a link, a checkbox, a switch, a radio, a tab, a menu item) is said by the words of its
tip, so that an icon with a tip is not nameless, but a page that writes the label says what it means.
An element that holds a control (one that answers the pointer, a pressed heading or picture too, or has a
`role` a person works on: `button`, `link`, `checkbox`, `switch`, `radio`, `tab`, `menu_item` or `slider`; a
text field, an editor, a browser, a link; not a `heading`, an `image` or a `progress` that is not pressed,
which only show) is not one name for all it holds: its controls are said each apart. Where it has a label
that is its name alone, and the words inside it that are in none of its controls are said apart, in their
place, as words, except those its label says already (a tab labelled "Files" with a close button in it does
not say "Files" twice: a text that is the label's words, or one part of it set off by a comma, a colon or a
semicolon, whole words only and not between digits, "1,024" and "10:30" being whole). What stands in an
overlay inside it is the page's own, and is said apart whatever the label says. Where it has no label it is
said by those words, or else its tip, and not at all where it has none, unless it has a `role` or is in a
state (a row, chosen), which are said whatever its words. So a dialog laid over a scrim that answers the
pointer is read as its buttons and fields (give the scrim a label that says what a press on it does, "Close
dialog", and the dialog's own words are read as words and not as the scrim's name), and a row with a checkbox
in it as the row and the checkbox.

An element says what it is to a **screen reader** where its look says it and its answers do not:
`role("checkbox" | "switch" | "radio" | "tab" | "menu_item" | "link" | "heading" | "image" |
"progress" | "slider" | "button" | "text", level)` (a heading's level is 1 to 6), and where it stands:
`checked(on)` (a checkbox, a switch, a radio), `selected(chosen)` (the tab in front, the row picked),
`expanded(open)`, `disabled(on)` (it does not answer the pointer, nor take the keyboard) and
`value(words)` (a progress's `"40%"`). A text field is one by `text_field`, an editor by `editor` and a browser by `browser`,
not by a role. The role `"none"` says the element is no control to a person who reads by ear (a scrim that
answers the pointer, a card that takes the click): it is not read, and what is inside it is. A role the
kit does not know is left out with a warning (`warn`), so that a page written for a later kit still runs.

The roles are those of the web's accessibility vocabulary, by its names: the twelve above (`menu_item`
is `menuitem` in a reading), `none`, and `dialog`, `alertdialog`, `group`, `list`, `listitem`, `listbox`,
`option`, `table`, `row`, `cell`, `columnheader`, `tree`, `treeitem`, `tablist`, `status` and `alert`. More
of where an element stands: `mixed()` (a checkbox that is neither on nor off; `checked` is none then),
`level(n)` (depth in a tree, from 1), `position(index, size)` (which of a set it is: a tab, an option, a
row of a list that is built only where it shows), `table_size(rows, columns)` (of the whole table, which is
more than the cells the page builds) with `cell_at(row, column)` on a row (a row, nil) and on a cell or a
header (a row holds its cells: each is read apart, in its row, as a control is, and its words are in the
row's name too), `sorted("ascending" | "descending")` on a header, and `role_description(words)`, what the page calls
the kind of element it is ("process", "issue card"), which is read in place of the role's. What an element's
tip says more than its name does is what it is for (its `description`), and the keys in the tip are the ones
that do it. A command defined on an element with an id other than the page's outermost box is an **action**
of that element (`Element:command`): the reading lists it on the element, and the page is told `{ type =
"command", id =, target = }` when it is done on it. A host asks a page to **show** something (a screen reader
moved to a row that is not built, an agent's `reveal`) with `{ type = "reveal", target =, index = }`: the
element of that id, or the row `index` (from 1) of the list, table or tree of that id. The page scrolls it
into view, loads it, and moves its cursor there.

**Asking whether the kit has what a page needs.** `pencere.has(name)` says whether the kit has a thing, by a
name that never changes: `"role.<role>"`, `"element.<method>"` for each method that came after the first
kit (`layer`, `fallback_name`, `mixed`, `level`, `position`, `table_size`, `cell_at`, `sorted`,
`role_description`), `"event.reveal"`, `"event.command"` (a command done on an element), `"command.target"`, and
`"ui.<component>.<option>"` for the options and abilities of the library's components that came after the
first kit (`"ui.list.reveal"`, `"ui.grid.reveal"`); `pencere.roles` is the set of the roles. A kit that is older than this
has no `pencere.has` at all, which is as good as `false`; a page that needs what it names asks before it
uses it and, where the host is too old, shows a screen that says so and does nothing else:

```lua
if pencere.has == nil or not pencere.has("role.dialog") then
	return { render = function()
		return pencere.div():size_full():items_center():justify_center():child(
			pencere.text("This needs a newer version of the application: please update it."))
	end }
end
```

A method or an option that has no name in `pencere.has` is a mistake of the kit: a test lists the methods
of an element and fails for one that is not in the first kit's list and not named.

An element that has a role and holds controls is read as a **group**: its name, and what is inside it
under it (a dialog, a row, a tab). So is a `dialog`, `alertdialog`, `group`, `list`, `listbox`, `table`,
`tree` or `tablist` that has a label whether or not a control is in it, as on the web: its name is the label and
what is written in it is read under it, apart from what the label says already (a heading of a role of its own
that repeats it, a dialog's title, is read, and is no repeat to the audit). A row, a cell, a column
header, a tab, an option, a list or tree item, a status and an alert are named by their label, or else by what is
in them, and have no children where no control is in them.
An element that is the root of a **layer** drawn over the page says so
with `layer(modal)`: with a modal layer in front, what is outside it is not read, nor pressable to a screen
reader (as behind a sheet or an `aria-modal`), and what is inside it, a menu or a popover it opens from
inside it included, is; a layer that is not modal (`layer(false)`: a popover, a toast) leaves the page.
`layer` stays a boolean for good: what more a layer comes to say (how it is given up, what it is for) comes as
methods of their own, each with its own name in `pencere.has`, and no value of `layer` is ever added. A layer a page
draws by hand says three things: `layer(true)` on its root, `role("dialog")` (or `"alertdialog"`) and a label on
its card, and `role("none")` on the scrim that only answers the pointer (`none` yields to a label the same
element has: it is read by it, as with no role at all, with the tip and the commands it has; without one it is
not read, and the commands it lists are not listed: they are put on what is read). How a layer is given up is told by a command listed on its card (`command(id, "Close")`: a
screen reader's action, `bateri view command`), which the library's modals, dialogs and palettes list. A layer
that is drawn and not said to be one leaves the page behind it read in full under what covers it, and
`Headless::audit` finds that (`covered`: what is read and a press on it does not reach, where what covers it is
no layer that is not modal).
`fallback_name(words)` is the name an element is read by where it has none of its own (a row of controls:
"Row 3"); it never replaces a name.
