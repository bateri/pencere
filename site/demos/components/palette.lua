--@ size 640 400
local ui = require("@pencere/ui")
local page = require("../_page")

local commands = ui.commands({
	{ id = "open", title = "Open file", group = "File", keys = "mod+p" },
	{ id = "save", title = "Save", group = "File", keys = "mod+s" },
	{ id = "find", title = "Find in files", group = "Search", keys = "mod+shift+f" },
	{ id = "replace", title = "Replace in files", group = "Search", keys = "mod+shift+h" },
	{ id = "close_tab", title = "Close tab", group = "View", keys = "mod+w" },
	{ id = "split", title = "Split the pane", group = "View" },
})
local items = commands:items()
local state = { query = "", cursor = nil }

return page(function()
	return pencere.text("The page under the palette"):color("dim")
end, {
	finish = function(u, root)
		root:child(u:palette("palette", { items = items, state = state, label = "Command palette", hint = "Type a command",
			footer = { { keys = "enter", title = "Run" }, { keys = "escape", title = "Close" } } }))
	end,
})
