-- The view of ui-lua: the components of the kit's library (`@pencere/ui`), each drawn as an app draws
-- it and answering as it answers one. What the person does comes back as an action, which the page
-- writes at its foot and acts on: a toggle the page keeps, a banner it dismisses.

local ui = require("@pencere/ui")

local u = ui.new()

-- What the page keeps of what the person did.
local S = {
	case = false,
	word = true,
	regex = false,
	preview = false,
	banner = true,
	last = "nothing yet",
	presses = 0,
	query = { value = "" },
	view = "list",
	tabs = {
		{ key = "main.lua", title = "main.lua" },
		{ key = "notes.md", title = "notes.md", dirty = true },
		{ key = "todo.txt", title = "todo.txt" },
	},
	active = "main.lua",
	asking = false,
	popover = false,
	toasts = {},
}

local cmds = ui.commands({
	{ id = "ask", title = "Ask a question", group = "Gallery", keys = "mod+shift+d" },
	{ id = "toast", title = "Show a toast", group = "Gallery" },
})

local function heading(words)
	return pencere.text(words):text_size(11):weight(700):color("quiet")
end

local function section(title, body)
	return pencere.div():gap(8):child(heading(title)):child(body)
end

local function row()
	return pencere.div():row():items_center():gap(8):wrap()
end

local function buttons()
	return row()
		:child(u:button("save", { title = "Save", tone = "primary", keys = "cmd+s" }))
		:child(u:button("revert", { title = "Revert", icon = "chevron_left" }))
		:child(u:button("delete", { title = "Delete", tone = "danger" }))
		:child(u:button("busy", { title = "Disabled", disabled = true }))
end

local function icons()
	return row()
		:child(u:icon_button("add", { icon = "plus", label = "New file", keys = "cmd+n" }))
		:child(u:icon_button("close", { icon = "close", label = "Close" }))
		:child(u:icon_button("preview", { icon = "dot", label = "Preview", on = S.preview }))
end

local function toggles()
	return row()
		:child(u:toggle("case", { on = S.case, glyph = "Aa", label = "Match case", keys = "alt+cmd+c" }))
		:child(u:toggle("word", { on = S.word, glyph = "W", label = "Whole word", keys = "alt+cmd+w" }))
		:child(u:toggle("regex", { on = S.regex, glyph = ".*", label = "Regular expression", keys = "alt+cmd+r" }))
end

local function badges()
	return row()
		:child(u:badge(S.presses, { tone = "info", round = true }))
		:child(u:badge("draft"))
		:child(u:badge("saved", { tone = "ok" }))
		:child(u:badge("conflict", { tone = "bad" }))
		:child(u:rule("y"))
		:child(u:kbd("cmd+shift+p"))
		:child(u:kbd("ctrl+enter", { quiet = true }))
end

local function banner()
	if not S.banner then
		return pencere.text("The banner was dismissed."):color("dim")
	end
	return u:banner("conflict", {
		tone = "warn",
		icon = "warning",
		words = "notes.md changed on disk since it was opened.",
		actions = { { key = "reload", title = "Reload", primary = true }, { key = "keep", title = "Keep mine" } },
		dismiss = true,
	})
end

local function fields()
	return pencere.div():gap(8)
		:child(u:input("query", {
			state = S.query,
			hint = "Search in files",
			label = "Search in files",
			trail = { u:toggle("regex2", { on = S.regex, glyph = ".*", label = "Regular expression" }) },
		}))
		:child(row():child(u:segments("view", { value = S.view, items = {
			{ key = "list", title = "List" },
			{ key = "grid", title = "Grid" },
			{ key = "tree", title = "Tree" },
		} })))
end

local function layers()
	local line = row()
		:child(u:button("ask", { title = "Ask", keys = "mod+shift+d" }))
		:child(u:button("toast", { title = "Toast" }))
		:child(u:button("more", { title = "Menu" }))
		:child(u:button("hint", { title = "Popover" }))
	return line
end

-- What an action does to the page.
local function act(action)
	S.last = action.from .. " " .. action.kind .. (action.key and (" " .. action.key) or "")
	S.presses += 1
	if action.kind == "toggle" then
		S[action.from] = action.on
	elseif action.from == "preview" then
		S.preview = not S.preview
	elseif action.from == "conflict" and action.kind == "dismiss" then
		S.banner = false
	elseif action.from == "tabs" and action.kind == "select" then
		S.active = action.key
	elseif action.from == "tabs" and action.kind == "close" then
		for index, tab in S.tabs do
			if tab.key == action.key then
				table.remove(S.tabs, index)
				break
			end
		end
	elseif action.from == "tabs" and action.kind == "move" then
		local from
		for index, tab in S.tabs do
			if tab.key == action.key then
				from = index
			end
		end
		local tab = table.remove(S.tabs, from)
		local to = #S.tabs + 1
		for index, other in S.tabs do
			if other.key == action.before then
				to = index
			end
		end
		table.insert(S.tabs, to, tab)
	elseif action.from == "view" then
		S.view = action.key
	elseif action.from == "ask" or (action.from == "commands" and action.id == "ask") then
		S.asking = true
	elseif action.from == "unsaved" then
		S.asking = false
	elseif action.from == "toast" or (action.from == "commands" and action.id == "toast") then
		ui.toast(S.toasts, "Saved notes.md", "ok", 3)
	elseif action.from == "toasts" and action.kind == "close" then
		for index, item in S.toasts do
			if item.key == action.key then
				table.remove(S.toasts, index)
				break
			end
		end
	elseif action.from == "more" then
		return u:menu("gallery-menu", { { key = "copy", title = "Copy", keys = "mod+c" }, "-", { key = "gone", title = "Delete" } }, { near = "more" })
	elseif action.from == "hint" or (action.from == "hint-card" and action.kind == "dismiss") then
		S.popover = not S.popover
	end
	return true
end

return {
	render = function(size, now, reduce_motion)
		u:begin(size, now, reduce_motion)
		local root = pencere.div()
			:id("gallery")
			:size_full()
			:scroll()
			:p(16)
			:gap(18)
			:bg("background")
			:color("foreground")
			:child(pencere.text("Components"):text_size(16):weight(700))
			:child(u:tabs("tabs", { items = S.tabs, active = S.active, reorder = true }))
			:child(section("BUTTONS", buttons()))
			:child(section("ICON BUTTONS", icons()))
			:child(section("TOGGLES", toggles()))
			:child(section("BADGES AND KEYS", badges()))
			:child(section("BANNER", banner()))
			:child(section("FIELDS AND CHOICES", fields()))
			:child(section("LAYERS", layers()))
			:child(u:rule("x"))
			:child(pencere.text("Last: " .. S.last):id("last"):color("dim"))
		if S.asking then
			root:child(u:dialog("unsaved", {
				title = "Save changes to notes.md?",
				message = "Your changes will be lost if you do not save them.",
				actions = { { key = "drop", title = "Don't Save", danger = true }, { key = "save", title = "Save", primary = true } },
			}))
		end
		if S.popover then
			root:child(u:popover("hint-card", { anchor = "hint" }, pencere.div():p(10):child(pencere.text("A card by its button"))))
		end
		root:child(u:toasts("toasts", { items = S.toasts }))
		cmds:write(u, root)
		return u:finish(root)
	end,
	settle = function()
		return u:settle()
	end,
	handle = function(event)
		local reply, actions = u:handle(event)
		for _, action in actions do
			reply = pencere.merge(reply, act(action))
		end
		return reply
	end,
}
