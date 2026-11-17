--@ size 480 100
--@ crop demo
local page = require("../_page")

return page(function(u)
	return pencere.div():row():items_center():gap(8)
		:child(u:badge(12, { tone = "info", round = true }))
		:child(u:badge("draft"))
		:child(u:badge("saved", { tone = "ok" }))
		:child(u:badge("behind", { tone = "warn" }))
		:child(u:badge("conflict", { tone = "bad" }))
end)
