--@ size 640 120
--@ crop demo
local page = require("../_page")

return page(function(u)
	return u:banner("conflict", {
		tone = "warn",
		icon = "warning",
		words = "notes.md changed on disk since it was opened.",
		actions = { { key = "reload", title = "Reload", primary = true }, { key = "keep", title = "Keep mine" } },
		dismiss = true,
	})
end, { width = 600 })
