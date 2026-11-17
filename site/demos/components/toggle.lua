--@ size 400 100
--@ crop demo
local page = require("../_page")

return page(function(u)
	return pencere.div():row():items_center():gap(4)
		:child(u:toggle("case", { on = false, glyph = "Aa", label = "Match case", keys = "alt+cmd+c" }))
		:child(u:toggle("word", { on = true, glyph = "W", label = "Whole word", keys = "alt+cmd+w" }))
		:child(u:toggle("regex", { on = false, glyph = ".*", label = "Regular expression", keys = "alt+cmd+r" }))
end)
