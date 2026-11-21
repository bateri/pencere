--@ size 560 250
--@ crop demo
local page = require("../_page")

-- The ground is the canvas's first path: what a path glows on or adds its light to is what the same
-- canvas drew under it, and not the page behind the canvas.
-- Colours of its own, not the theme's: lights on a dark ground look the same in either theme.
local night = "#12141a"
local blue, green, amber, red = "#5aa9ff", "#4ade80", "#ffc247", "#ff7a7a"
local lights = { blue, green, amber, red, blue, green }

return page(function()
	local paths = { pencere.path():rect(0, 0, 480, 180):fill(night) }
	-- Dots that glow: a shadow with no offset, and `add` so that where two glows meet they brighten.
	for index, light in lights do
		local x = 32 + (index - 1) * 34
		paths[#paths + 1] = pencere.path()
			:circle(x, 34, 6)
			:fill(light)
			:shadow(0, 0, 18, light)
			:blend("add")
	end
	-- A fill that runs from one colour to another, with a glow of its own.
	paths[#paths + 1] = pencere.path()
		:rect(20, 74, 190, 12)
		:fill(pencere.linear(20, 0, 210, 0, { { 0, blue }, { 1, green } }))
		:shadow(0, 2, 14, pencere.alpha(blue, 0.7))
	-- A ring painted with a gradient, and a dashed one round it with butt ends.
	paths[#paths + 1] = pencere.path()
		:circle(340, 62, 34)
		:stroke(8, pencere.linear(306, 62, 374, 62, { { 0, blue }, { 1, green } }))
	paths[#paths + 1] = pencere.path()
		:circle(340, 62, 48)
		:stroke(2, "#c9ced8")
		:dash({ 3, 6 })
		:cap("butt")
	paths[#paths + 1] = pencere.path()
		:circle(340, 62, 20)
		:fill(pencere.radial(340, 62, 20, { { 0, pencere.alpha(blue, 0.9) }, { 1, pencere.alpha(blue, 0) } }))
	-- Shapes turned about their own middles, the lines as wide and as sharp as they were.
	for index = 0, 5 do
		local x = 40 + index * 38
		paths[#paths + 1] = pencere.path()
			:rect(x - 12, 128, 24, 24)
			:stroke(2, amber)
			:join("miter")
			:transform({ rotate = index * 15, origin = { x, 140 } })
	end
	-- Two thick lines with square ends that screen into one another: brighter where they cross.
	paths[#paths + 1] = pencere.path()
		:move_to(300, 134)
		:line_to(390, 134)
		:stroke(16, blue)
		:cap("square")
		:blend("screen")
	paths[#paths + 1] = pencere.path()
		:move_to(300, 134)
		:line_to(390, 134)
		:stroke(16, red)
		:cap("square")
		:blend("screen")
		:transform({ translate = { 40, 0 } })
	return pencere.canvas(paths):w(480):h(180)
end)
