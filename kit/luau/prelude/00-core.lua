-- The page API a script is written against, part 1: what the host gives (kept out of the
-- script's reach once it is wrapped) and the small things the rest is made of.
--
-- A script is Luau. What it can use besides the language is the base functions, `string`,
-- `table`, `math` and `utf8`, `require` (the other scripts of its bundle, and the kit's library:
-- `require("@pencere/syntax")`), and `pencere`, the table this prelude fills:
--
--   elements   pencere.div(), pencere.text(), pencere.rich({ pencere.span(...), ... }),
--              pencere.spark(), pencere.image(), pencere.canvas({ pencere.path()..., ... }),
--              pencere.spacer()
--   colours    a role of the theme ("accent", "red", "bright-red", "selection", "raised2", ...), a
--              value ("#rrggbb", "#rrggbbaa", or { light = "#...", dark = "#..." } for a dark
--              theme), any of them taken part of the way toward another:
--              pencere.mix("dim", "foreground", 0.5), and any of those with an opacity:
--              pencere.alpha("accent", 0.5)
--   pages      the script returns { render = ..., handle = ..., settle = ..., message = ..., room = ... }
--   memory     pencere.Ui, pencere.Fades, pencere.Rows, pencere.Built (what a page remembers
--              between frames)
--   lists      box:list(rows, make), pencere.list_window(id) (a long list in a box the host
--              scrolls: only the rows near what is in view are made)
--   time       pencere.time.after, pencere.time.every, pencere.time.now (timers, run when the host
--              wakes the page: a reply's `wake`), pencere.merge (two replies as one)
--   layout     pencere.layout (what is where on the page as it was last laid out)
--   program    pencere.send, pencere.call, pencere.subscribe, pencere.cancel, pencere.json
--
-- Lengths are points. An element is a plain table; the builders are methods that return it.

-- The major number of the page API (2.1 adds the files beside the bundle to 2.0: the `files`,
-- `files_over`, `files_left` and `saving` events, the `pick` and `save` of a reply, images named
-- `sha256:` and `file:`).
pencere.version = 2

-- A number as the engine holds it, in single precision. The engine measures and draws in
-- single precision, and a page that adds up highlights or sizes in doubles can come to a
-- value that is not the engine's (a level of 1e-17 where the engine has 0).
function pencere.f32(value)
	return (string.unpack("<f", string.pack("<f", value)))
end

-- The host's functions, taken here and given a shape of Lua's below (`pencere.layout`,
-- `pencere.send`); the table of the raw ones stays at `pencere.host` for a page that wants them.
local host = {
	send = pencere.send,
	hit = pencere.hit,
	ids_at = pencere.ids_at,
	bounds = pencere.bounds,
	scroll_extent = pencere.scroll_extent,
	scroll_to = pencere.scroll_to,
	focus_order = pencere.focus_order,
	path_of = pencere.path_of,
	drag_source = pencere.drag_source,
	drop_target = pencere.drop_target,
	editor_version = pencere.editor_version,
	editor_length = pencere.editor_length,
	editor_lines = pencere.editor_lines,
	editor_text = pencere.editor_text,
	line_start = pencere.line_start,
	line_of = pencere.line_of,
	word_at = pencere.word_at,
	wall_clock = pencere.wall_clock,
	keyboard = pencere.keyboard,
	terminal_look = pencere.terminal_look,
	draws = pencere.draws,
}
pencere.send, pencere.hit, pencere.ids_at, pencere.bounds = nil, nil, nil, nil
pencere.scroll_extent, pencere.scroll_to, pencere.focus_order, pencere.path_of = nil, nil, nil, nil
pencere.drag_source, pencere.drop_target = nil, nil
pencere.editor_version, pencere.editor_length, pencere.editor_lines = nil, nil, nil
pencere.editor_text, pencere.line_start, pencere.line_of, pencere.word_at = nil, nil, nil, nil
pencere.wall_clock, pencere.keyboard, pencere.terminal_look, pencere.draws = nil, nil, nil, nil
pencere.host = host

-- How the host's system makes and writes its keyboard shortcuts: "mac" (⌘ the main modifier, the
-- keys written as signs, ⌘⇧K), "windows" (Ctrl the main one, the keys written as words,
-- Ctrl+Shift+K) or "linux" (as Windows, the logo key Super). What a page shows of a shortcut, and
-- which modifier it means by `mod` (`pencere.keys`), follow it.
local convention = nil
function pencere.keyboard()
	-- The same every time it is asked: asked once.
	if convention == nil then
		convention = host.keyboard()
	end
	return convention
end

-- How the host's terminal draws its cursor and its selection, as the person has set them, in the words
-- and units of an editor's options: `{ caret = { shape =, width =, radius =, glow =, blink =,
-- blink_interval =, unfocused =, motion = }, selection = { radius = } }`, with only the fields the host
-- says (it says a field only where it has such a setting, and a field that is nil is the kit's own
-- default for an editor). A host with no such settings says none: `{}`. Read
-- as it is asked, so it is what the person has set now; the page's `handle` is told
-- `{ type = "terminal_look" }` when it changes. The host applies none of it to the page's editors: the
-- page decides what its editors take (`@pencere/terminal`'s `terminal.editor` takes it all, where the
-- page says nothing).
function pencere.terminal_look()
	return host.terminal_look()
end

-- What the host draws that not every host does, as a table of names that are true: `{ shadow = true }`
-- where it draws a box's soft shadow (a caret's `glow`), `{}` where it draws only what every host does.
-- The same every time it is asked.
local draws = nil
function pencere.draws()
	if draws == nil then
		draws = host.draws()
	end
	return draws
end

-- A colour with an opacity. Everywhere a colour is taken, a role's name, a "#rrggbb" value and a
-- { light =, dark = } table do too; `colour` is any of the three, or one `pencere.mix` made (whose
-- opacity this multiplies).
function pencere.alpha(colour, alpha)
	if type(colour) == "table" and colour.toward ~= nil then
		return { colour[1], (colour[2] or 1) * alpha, toward = colour.toward, amount = colour.amount }
	end
	return { colour, alpha }
end

-- `colour` taken `amount` of the way toward `toward` (0: as it is, 1: `toward`), channel by
-- channel in sRGB: a ground a highlight comes in on, `dim` words that brighten to `foreground`.
-- Both are a role's name, a value or a { light =, dark = } table; the result takes an opacity
-- (`pencere.alpha`) like any colour.
function pencere.mix(colour, toward, amount)
	if type(amount) ~= "number" then
		error("pencere.mix takes a colour, the colour it goes toward and how far, 0 to 1", 2)
	end
	return { colour, 1, toward = toward, amount = amount }
end

-- The ground raised by `steps` levels ("raised1" is a card on the pane).
function pencere.raised(steps)
	return "raised" .. steps
end

-- A grid track: points are a number, the rest are these.
function pencere.fr(share)
	return { fr = share }
end

function pencere.range(least, share)
	return { min = least, fr = share }
end

-- What the host answers about the page as it laid it out last (which may be a frame behind
-- the tree handed out since). Points are `{ x =, y = }`.
local layout = {}
pencere.layout = layout

function layout.hit(at)
	return host.hit(at.x, at.y)
end

function layout.ids_at(at)
	return host.ids_at(at.x, at.y)
end

function layout.bounds(id)
	return host.bounds(id)
end

-- How far the scrolling box named `id` reaches: { content =, view = } down and { content_w =,
-- view_w = } across, with how far each way it can be scrolled, `max_offset` and `max_offset_x`,
-- and how far it is scrolled, `offset` and `offset_x`, as it was laid out.
function layout.scroll_extent(id)
	local extent = host.scroll_extent(id)
	if extent then
		extent.max_offset = math.max(extent.content - extent.view, 0)
		extent.max_offset_x = math.max(extent.content_w - extent.view_w, 0)
	end
	return extent
end

-- How far each box is scrolled, the host's word: what the page was last told of it (a `scrolled`
-- event) or asked for it since. By the box's name: { x =, y = }.
local offsets = {}
-- How far down the page asked a box to go and the host has not yet said it went, as asked: the
-- end of a box that grows in the same frame is further than the layout the ask was held to.
local asked = {}

-- How far the box named `id` is scrolled: down, then across, points. The host scrolls a box (an
-- element with a name and `:scroll()`) with the wheel, the trackpad and its bars, as its system
-- does; the page is told (`scrolled`) and builds what came into view.
function layout.scroll_offset(id)
	local known = offsets[id]
	if known then
		return known.y, known.x
	end
	local extent = host.scroll_extent(id)
	if extent then
		return extent.offset, extent.offset_x
	end
	return 0, 0
end

-- Asks the host to scroll the box named `id`: `to` is { y =, x =, animate = } (a way not given
-- stays where it is; past the end is the end; `animate` glides there, unless the person asked for
-- less motion). Done before the next frame is laid out; a box not on the page yet is there when
-- it comes.
function layout.scroll_to(id, to)
	local x, y = to.x, to.y
	local known = offsets[id] or { x = 0, y = 0 }
	local extent = host.scroll_extent(id)
	local clamp = function(value, reach)
		value = math.max(value, 0)
		return reach and math.min(value, reach) or value
	end
	offsets[id] = {
		x = x and clamp(x, extent and math.max(extent.content_w - extent.view_w, 0)) or known.x,
		y = y and clamp(y, extent and math.max(extent.content - extent.view, 0)) or known.y,
	}
	if y then
		asked[id] = math.max(y, 0)
	end
	host.scroll_to(id, x, y, to.animate and true or false)
end

-- The prelude's: the host scrolled these boxes ({ { id =, x =, y = }, ... }).
function layout._heard(boxes)
	for _, scrolled in ipairs(boxes) do
		offsets[scrolled.id] = { x = scrolled.x, y = scrolled.y }
		asked[scrolled.id] = nil
	end
end

-- The prelude's: how far down the page asked the box named `id` to go, as it asked, while the host
-- has not said where it went since (nil when it has).
function layout._asked(id)
	return asked[id]
end

function layout.focus_order()
	return host.focus_order()
end

function layout.path_of(id)
	return host.path_of(id)
end

-- The element under `at` that can be picked up and moved, and the one what is dragged can be put
-- down on: the topmost of each.
function layout.drag_source(at)
	return host.drag_source(at.x, at.y)
end

function layout.drop_target(at)
	return host.drop_target(at.x, at.y)
end

-- --- The text of the page's editors -----------------------------------------------------------
--
-- What the host posted of each editor's document (`pencere.editor`), read at once: the text is of
-- the version the last `edit` event said (`layout.editor_version`). Places are characters (code
-- points) and lines are counted from 0, as the host counts them; a range is from one place to
-- another, the end not in it. A large document is read in pieces: one read answers with at most
-- 1 MiB.

-- The version of the editor's text that is read; nil for an editor the page has not.
function layout.editor_version(editor)
	return host.editor_version(editor)
end

-- How many characters it has, and how many lines.
function layout.editor_length(editor)
	return host.editor_length(editor)
end

function layout.editor_lines(editor)
	return host.editor_lines(editor)
end

-- The characters `from` to `to` (the end not in it); nil for a range it has not.
function layout.editor_text(editor, from, to)
	return host.editor_text(editor, from, to)
end

-- Where line `line` starts, and the line the place `at` is on.
function layout.line_start(editor, line)
	return host.line_start(editor, line)
end

function layout.line_of(editor, at)
	return host.line_of(editor, at)
end

-- The word at the place `at` (or the run of marks, or of spaces) as a double click takes it: its
-- `from` and `to` (the end not in it); nil for an editor or a place it has not. The rule is the
-- editor's own, so that a page that asks of a word at the caret (Show Hover, Go to Definition) asks
-- of the same one the pointer rests on.
function layout.word_at(editor, at)
	return host.word_at(editor, at)
end

-- What the keys of an editor do: the editor's own keys, `{ keys = "cmd+d", does = "select_next" }`
-- (what the `keymap` option takes: a command's word, "copy", "cut", "paste", "tab", "escape",
-- "to_line_start", "to_line_end", or a command's table), as the
-- page's `keymap` changed them: a key it says is what it says, one it says is false is gone, and a
-- chord it adds is added. For a page that shows a key where it says what a command does.
pencere._keymaps = {}
local function go(movement, extend)
	return { command = "move", movement = movement, extend = extend or false }
end
local DEFAULT_KEYS = {
	{ "left", go("left") }, { "shift+left", go("left", true) }, { "alt+left", go("word_left") },
	{ "alt+shift+left", go("word_left", true) }, { "cmd+left", go("line_start") },
	{ "cmd+shift+left", go("line_start", true) }, { "right", go("right") },
	{ "shift+right", go("right", true) }, { "alt+right", go("word_right") },
	{ "alt+shift+right", go("word_right", true) }, { "cmd+right", go("line_end") },
	{ "cmd+shift+right", go("line_end", true) }, { "up", go("up") }, { "shift+up", go("up", true) },
	{ "cmd+up", go("document_start") }, { "down", go("down") }, { "shift+down", go("down", true) },
	{ "cmd+down", go("document_end") }, { "alt+up", "move_lines_up" }, { "alt+down", "move_lines_down" },
	{ "alt+shift+up", "duplicate_lines" }, { "alt+shift+down", "duplicate_lines" },
	{ "cmd+alt+up", "add_caret_above" }, { "cmd+alt+down", "add_caret_below" },
	{ "page_up", go("page_up") }, { "page_down", go("page_down") }, { "home", go("line_start") },
	{ "end", go("line_end") }, { "cmd+home", go("document_start") }, { "cmd+end", go("document_end") },
	{ "backspace", "delete_backward" }, { "alt+backspace", "delete_word_backward" },
	{ "cmd+backspace", "to_line_start" }, { "delete", "delete_forward" },
	{ "alt+delete", "delete_word_forward" }, { "enter", "newline" }, { "tab", "tab" },
	{ "shift+tab", "outdent" }, { "escape", "escape" }, { "cmd+z", "undo" }, { "cmd+shift+z", "redo" },
	{ "cmd+a", "select_all" }, { "cmd+d", "select_next" }, { "cmd+/", "toggle_comment" },
	{ "cmd+]", "indent" }, { "cmd+[", "outdent" }, { "cmd+alt+]", "unfold" }, { "cmd+alt+[", "fold" },
	{ "cmd+g", "find_next" }, { "cmd+shift+g", "find_previous" }, { "cmd+c", "copy" },
	{ "cmd+x", "cut" }, { "cmd+v", "paste" }, { "ctrl+a", go("line_start") },
	{ "ctrl+e", go("line_end") }, { "ctrl+k", "to_line_end" },
}

-- The keys of an editor and what they do, as `{ { keys =, does = }, ... }`; `does` is nil where the
-- page took the key from the editor.
function layout.keymap(editor)
	local changed = pencere._keymaps[editor] or {}
	local out, seen = {}, {}
	local function resolved(words)
		return (string.gsub(string.lower(words), "mod", pencere.host.keyboard() == "mac" and "cmd" or "ctrl"))
	end
	-- In the order of the keys, so that what is read back is the same every time.
	local names = {}
	for words in changed do
		table.insert(names, words)
	end
	table.sort(names)
	for _, words in names do
		local written = resolved(words)
		seen[written] = true
		table.insert(out, { keys = written, does = changed[words] or nil })
	end
	for _, entry in DEFAULT_KEYS do
		if not seen[entry[1]] then
			table.insert(out, { keys = entry[1], does = entry[2] })
		end
	end
	return out
end

-- The caret of the editor named `editor`: what an overlay stands beside
-- (`:anchor(pencere.caret_of("sql"), "below")`, a popup of completions) and what
-- `layout.bounds` says the box of. A name of the editor's own, which no name of the page's is.
function pencere.caret_of(editor)
	return { caret = editor }
end

-- The range the page named `name` in the editor named `editor` (a decoration
-- `{ from =, to =, anchor = name }`): what an overlay stands beside as it does by the caret
-- (`:anchor(pencere.range_of("sql", "hover"), "below")`) and what `layout.bounds` says the box of,
-- from the start of the range across its first row and down its last. An overlay whose range is
-- not in the room, or is gone, is not shown. A name is an editor's alone.
function pencere.range_of(editor, name)
	if type(name) ~= "string" then
		error("range_of takes the editor's name and the range's: range_of(editor, name)", 2)
	end
	return { anchor = editor, name = name }
end
