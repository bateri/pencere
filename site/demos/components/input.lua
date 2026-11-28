--@ size 480 160
--@ crop demo
local page = require("../_page")

local query = { value = "" }
local name = { value = "release-notes.md" }
local larger = { value = "Larger words, 15 points" }

return page(function(u)
	return pencere.div():gap(10)
		:child(u:input("query", { state = query, hint = "Search in files", label = "Search in files" }))
		:child(u:input("name", { state = name, label = "File name" }))
		:child(u:input("larger", { state = larger, label = "Larger words", text_size = 15 }))
end, { width = 360 })
