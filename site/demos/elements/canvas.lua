--@ size 520 200
--@ crop demo
local page = require("../_page")

local values = { 14, 22, 18, 30, 26, 38, 34, 46, 40, 52, 48, 60 }

return page(function()
	local line = pencere.path()
	local area = pencere.path():move_to(0, 120)
	for index, value in values do
		local x, y = (index - 1) * 28, 120 - value * 1.8
		if index == 1 then
			line:move_to(x, y)
		else
			line:line_to(x, y)
		end
		area:line_to(x, y)
	end
	area:line_to((#values - 1) * 28, 120):close()
	return pencere.canvas({
		area:fill(pencere.alpha("accent", 0.15)),
		line:stroke(2, "accent"),
		pencere.path():circle((#values - 1) * 28, 120 - 60 * 1.8, 4):fill("accent"),
	}):w(320):h(124)
end)
