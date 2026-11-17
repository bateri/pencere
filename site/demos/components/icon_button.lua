--@ size 400 100
--@ crop demo
local page = require("../_page")

return page(function(u)
	return pencere.div():row():items_center():gap(6)
		:child(u:icon_button("add", { icon = "plus", label = "New file", keys = "cmd+n" }))
		:child(u:icon_button("close", { icon = "close", label = "Close" }))
		:child(u:icon_button("preview", { icon = "dot", label = "Preview", on = true }))
		:child(u:icon_button("info", { icon = "info", label = "About" }))
end)
