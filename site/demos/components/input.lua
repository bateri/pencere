--@ size 480 160
--@ crop demo
local page = require("../_page")

local query = { value = "" }
local name = { value = "release-notes.md" }

return page(function(u)
	return pencere.div():gap(10)
		:child(u:input("query", { state = query, hint = "Search in files", label = "Search in files" }))
		:child(u:input("name", { state = name, label = "File name" }))
end, { width = 360 })
