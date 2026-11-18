--@ size 560 120
--@ crop demo
local page = require("../_page")

return page(function(u)
	return pencere.div():row():items_center():gap(8)
		:child(u:button("save", { title = "Save", tone = "primary", keys = "cmd+s" }))
		:child(u:button("revert", { title = "Revert", icon = "chevron_left" }))
		:child(u:button("delete", { title = "Delete", tone = "danger" }))
		:child(u:button("busy", { title = "Disabled", disabled = true }))
end)
