--@ size 560 240
local ui = require("@pencere/ui")
local page = require("../_page")

local items = {}
ui.toast(items, "Saved notes.md", "ok", 0)
ui.toast(items, "3 files changed on disk", "info", 0)

return page(function()
	return pencere.text("The page under the toasts"):color("dim")
end, {
	finish = function(u, root)
		root:child(u:toasts("toasts", { items = items }))
	end,
})
