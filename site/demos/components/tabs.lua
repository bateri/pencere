--@ size 560 100
--@ crop demo
local page = require("../_page")

return page(function(u)
	return u:tabs("tabs", {
		items = {
			{ key = "main.lua", title = "main.lua" },
			{ key = "notes.md", title = "notes.md", dirty = true },
			{ key = "todo.txt", title = "todo.txt" },
		},
		active = "main.lua",
		reorder = true,
	})
end, { width = 480 })
