--@ size 560 250
--@ crop demo
local page = require("../_page")

-- The ground is the canvas's first path: what a path glows on or adds its light to is what the same
-- canvas drew under it, and not the page behind the canvas.
-- Colours of its own, not the theme's: lights on a dark ground look the same in either theme.
local night = "#12141a"
local blue, green, amber, red = "#5aa9ff", "#4ade80", "#ffc247", "#ff7a7a"
local lights = { blue, green, amber, red, blue, green }

-- Everything stands inside a margin of 24 on every side of the 480 by 180 canvas. The left column is
-- 214 across: the dots, the bar and the squares all start at its left edge and end at its right, and
-- the ring and the lines of the right column share a middle, with the tops and the bottoms of the two
-- columns on the same lines.
local LEFT, WIDTH = 24, 214
local RIGHT_MIDDLE = 383

return page(function()
	local paths = { pencere.path():rect(0, 0, 480, 180):fill(night) }
	-- Dots that glow: a shadow with no offset, and `add` so that where two glows meet they brighten.
	for index, light in lights do
		local x = LEFT + 6 + (index - 1) * (WIDTH - 12) / (#lights - 1)
		paths[#paths + 1] = pencere.path()
			:circle(x, 30, 6)
			:fill(light)
			:shadow(0, 0, 18, light)
			:blend("add")
	end
	-- A fill that runs from one colour to another, with a glow of its own.
	paths[#paths + 1] = pencere.path()
		:rect(LEFT, 70, WIDTH, 12)
		:fill(pencere.linear(LEFT, 0, LEFT + WIDTH, 0, { { 0, blue }, { 1, green } }))
		:shadow(0, 2, 14, pencere.alpha(blue, 0.7))
	-- A ring painted with a gradient, and a dashed one round it with butt ends.
	paths[#paths + 1] = pencere.path()
		:circle(RIGHT_MIDDLE, 72, 34)
		:stroke(8, pencere.linear(RIGHT_MIDDLE - 34, 72, RIGHT_MIDDLE + 34, 72, { { 0, blue }, { 1, green } }))
	paths[#paths + 1] = pencere.path()
		:circle(RIGHT_MIDDLE, 72, 48)
		:stroke(2, "#c9ced8")
		:dash({ 3, 6 })
		:cap("butt")
	paths[#paths + 1] = pencere.path()
		:circle(RIGHT_MIDDLE, 72, 20)
		:fill(pencere.radial(RIGHT_MIDDLE, 72, 20, { { 0, pencere.alpha(blue, 0.9) }, { 1, pencere.alpha(blue, 0) } }))
	-- Shapes turned about their own middles, the lines as wide and as sharp as they were.
	for index = 0, 5 do
		local x = LEFT + 12 + index * (WIDTH - 24) / 5
		paths[#paths + 1] = pencere.path()
			:rect(x - 12, 127, 24, 24)
			:stroke(2, amber)
			:join("miter")
			:transform({ rotate = index * 15, origin = { x, 139 } })
	end
	-- Two thick lines with square ends that screen into one another: brighter where they cross.
	-- The two together are 146 across with their ends: their middle is the ring's.
	local from = RIGHT_MIDDLE - 73 + 8
	paths[#paths + 1] = pencere.path()
		:move_to(from, 148)
		:line_to(from + 90, 148)
		:stroke(16, blue)
		:cap("square")
		:blend("screen")
	paths[#paths + 1] = pencere.path()
		:move_to(from, 148)
		:line_to(from + 90, 148)
		:stroke(16, red)
		:cap("square")
		:blend("screen")
		:transform({ translate = { 40, 0 } })
	return pencere.canvas(paths):w(480):h(180)
end)
