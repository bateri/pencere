--@ size 560 240
--@ crop demo
local page = require("../_page")

local state = { size = 170 }

return page(function(u)
	return pencere.div():h(180):border(1, "border"):rounded(6):clip():child(u:split("side", {
		state = state,
		min = 120,
		max = 320,
		label = "Sidebar width",
		first = pencere.div():size_full():bg("raised1"):p(12):child(pencere.text("Files"):color("dim")),
		second = pencere.div():size_full():p(12):child(pencere.text("main.lua")),
	}))
end, { width = 480 })
