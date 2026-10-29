-- @pencere/ui, the menu of an editor: what the person is offered where the text is pressed with the
-- right button, the same in every editor of every page.
--
--   -- the editor is written with the press that opens its menu:
--   pencere.editor("code", { gestures = { ["right+press"] = "context" } })
--   -- in handle, on that press (the caret is where a text view puts it already: event.place.inside):
--   if event.type == "press" and event.button == "right" and event.place then
--       return u:editor_menu("code-menu", { editor = event.place.editor, at = { x = event.x, y = event.y },
--           read_only = doc.read_only, before = { { key = "definition", title = "Go to Definition", keys = "f12" } },
--           after = { { key = "palette", title = "Command Palette", keys = "cmd+shift+p" } } })
--   end
--   -- and on what the context says of the pick:
--   act.from == "code-menu" and act.kind == "menu" → reply = merge(reply, u:editor_menu_pick(act))
--                                                    -- the page's own lines (act.entry) are the page's
--
-- The lines are Cut, Copy, Paste, Select All, Toggle Line Comment and Select All Occurrences, each
-- with the keys the editor has for it now (`pencere.layout.keymap`, as the page changed them: a key
-- it took away is not shown); the page's own lines come before them and after (`before`, `after`: the
-- entries of `u:menu`, `"-"` for a rule). Nothing is greyed for an empty selection, as the keys do
-- not: Copy and Cut take the caret's line. A `read_only` editor greys Cut, Paste and Toggle Line Comment.
-- Paste is a line the host performs itself, with its pasteboard, as the paste key would: the page
-- never reads the pasteboard and cannot ask for a paste, and is told the pick as any. The others are
-- asks the helper makes of the editor when `u:editor_menu_pick` is given the pick.
--
-- Without `at` the menu opens at the caret (`pencere.caret_of`), under it: for the keys, which
-- `ui.EDITOR_MENU_COMMAND` is the command of (Shift F10 as VS Code has it): the page opens the menu of
-- the editor it knows has the keyboard when the context says the command was asked for.

local core = require("./core")

local layout = pencere.layout

local editor_menu = {}

-- The lines: their key, their title, the word the editor's keys are looked up by, the ask the page
-- makes of the editor (none of Paste: the host's), and whether it changes the text.
local LINES = {
	{ key = "cut", title = "Cut", does = "cut", ask = { command = "cut" }, writes = true },
	{ key = "copy", title = "Copy", does = "copy", ask = { command = "copy" } },
	{ key = "paste", title = "Paste", does = "paste", writes = true, paste = true },
	"-",
	{ key = "select_all", title = "Select All", does = "select_all", ask = { command = "select_all" } },
	"-",
	{ key = "toggle_comment", title = "Toggle Line Comment", does = "toggle_comment", ask = { command = "toggle_comment" }, writes = true },
	{ key = "select_occurrences", title = "Select All Occurrences", does = "select_occurrences", ask = { command = "select_occurrences" } },
}

local ASKS, RESERVED = {}, {}
for _, line in LINES do
	if line ~= "-" then
		RESERVED[line.key] = true
		ASKS[line.key] = line.ask
	end
end

-- The command that opens the menu at the caret, to add to the page's `ui.commands`.
editor_menu.COMMAND = { id = "editor_menu", title = "Show Editor Menu", group = "Editor", keys = "shift+f10" }

-- The keys the editor has for each word, as the menu shows them (a line has the keys of one key,
-- never of a chord, which a menu cannot show); the first the editor lists, the page's own before its.
local function keys_by_word(editor)
	local found = {}
	for _, entry in layout.keymap(editor) do
		if type(entry.does) == "string" and found[entry.does] == nil and not string.find(entry.keys, " ", 1, true) then
			found[entry.does] = entry.keys
		end
	end
	return found
end

local function check(props)
	if type(props) ~= "table" or type(props.editor) ~= "string" then
		error("an editor's menu is of an editor: u:editor_menu(id, { editor = \"name\", at = { x =, y = } })", 3)
	end
	for _, group in { props.before or {}, props.after or {} } do
		for _, entry in group do
			if entry ~= "-" and RESERVED[entry.key] then
				error("the page's own line of an editor's menu cannot be named " .. entry.key .. ", which is the menu's own", 3)
			end
		end
	end
end

-- Where the menu opens by the keyboard: under the caret.
local function at_caret(editor)
	local box = layout.bounds(pencere.caret_of(editor))
	if box == nil then
		box = layout.bounds(editor)
		if box == nil then
			error("the editor " .. editor .. " is not on the page, and has no caret to open a menu at", 3)
		end
		return { x = box.x, y = box.y }
	end
	return { x = box.x, y = box.y + box.h }
end

-- The reply that asks the host for the menu of the editor `props.editor`, to be returned from `handle`.
function editor_menu.editor_menu(u, id, props)
	check(props)
	local entries = {}
	local function add(group)
		if #group == 0 then
			return
		end
		if #entries > 0 then
			table.insert(entries, "-")
		end
		for _, entry in group do
			table.insert(entries, entry)
		end
	end
	add(props.before or {})
	local own = {}
	local bound = keys_by_word(props.editor)
	for _, line in LINES do
		if line == "-" then
			table.insert(own, "-")
		else
			local entry = {
				key = line.key,
				title = line.title,
				keys = bound[line.does],
				enabled = not (line.writes and props.read_only == true),
			}
			if line.paste then
				entry.paste = props.editor
			end
			table.insert(own, entry)
		end
	end
	add(own)
	add(props.after or {})
	u.editor_menus = u.editor_menus or {}
	u.editor_menus[id] = props.editor
	return u:menu(id, entries, { at = props.at or at_caret(props.editor) })
end

-- The reply to the pick of one of the menu's own lines (`act`: `{ from = id, kind = "menu", entry = key }`):
-- the ask of the editor; nil for the page's lines, for Paste (the host did it) and for what is no pick
-- of an editor's menu.
function editor_menu.editor_menu_pick(u, act)
	local editor = u.editor_menus and u.editor_menus[act.from]
	if editor == nil or act.kind ~= "menu" then
		return nil
	end
	local ask = ASKS[act.entry]
	if ask == nil then
		return nil
	end
	return { redraw = true, editor = { [editor] = { ask } } }
end

return editor_menu
