--@ size 560 320
local page = require("../_page")

return page(function()
	return pencere.text("notes.md"):color("dim")
end, {
	finish = function(u, root)
		root:child(u:dialog("unsaved", {
			title = "Save changes to notes.md?",
			message = "Your changes will be lost if you do not save them.",
			actions = { { key = "drop", title = "Don't Save", danger = true }, { key = "save", title = "Save", primary = true } },
		}))
	end,
})
