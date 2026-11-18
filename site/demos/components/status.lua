--@ size 640 100
--@ crop demo
local page = require("../_page")

return page(function(u)
	return pencere.div():border(1, "border"):rounded(6):clip():child(u:status("status", {
		start = { { key = "branch", words = "main", icon = "dot", priority = 9 } },
		center = { { key = "note", words = "Saved notes.md", priority = 1 } },
		finish = {
			{ key = "pos", words = "Ln 12, Col 4", press = true, priority = 5, tip = "Go to line" },
			{ key = "lang", words = "Lua", priority = 3 },
		},
	}))
end, { width = 560 })
