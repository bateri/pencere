-- A page for the tests of pictures the terminal draws: a list that scrolls, with one picture in
-- it among the lines.

local b = pencere

return {
	render = function()
		local list = b.div():col():id("list"):grow(1):min_h(0):scroll()
		for i = 1, 8 do
			list:child(b.text("line " .. i))
		end
		-- Twenty cells across and three down.
		list:child(b.image("dot.svg"):w(160):h(54))
		for i = 9, 60 do
			list:child(b.text("line " .. i))
		end
		return b.div():size_full():col():child(b.text("Pictures")):child(list)
	end,

	handle = function(event) end,
}
