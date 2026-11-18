--@ size 480 260
--@ crop demo
local page = require("../_page")

local rows = {}
for index = 1, 400 do
	table.insert(rows, { key = "r" .. index, title = "Result " .. index, hint = "line " .. index * 3 })
end
local state = { cursor = "r3", marks = {} }

return page(function(u)
	return pencere.div():h(200):border(1, "border"):rounded(6):clip():child(
		u:list("results", { items = rows, state = state, height = 24, inset = 2 }))
end, { width = 400 })
