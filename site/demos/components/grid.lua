--@ size 600 300
--@ crop demo
local page = require("../_page")

local columns = {
	{ key = "id", title = "id", type = "int", align = "end" },
	{ key = "name", title = "name", type = "text" },
	{ key = "qty", title = "qty", type = "int", align = "end" },
	{ key = "note", title = "note", type = "text" },
}
local state = { cursor = { row = 2, column = 2 }, widths = {}, sort = { column = "qty", descending = true } }

local function cell(row, column)
	if column.key == "id" then
		return tostring(row)
	elseif column.key == "name" then
		return "item " .. row
	elseif column.key == "qty" then
		return tostring(row * 7 % 100)
	end
	return row % 3 == 0 and "back ordered" or "in stock"
end

return page(function(u)
	return pencere.div():h(220):border(1, "border"):rounded(6):clip():child(
		u:grid("stock", { columns = columns, count = 5000, cell = cell, state = state }))
end, { width = 520 })
