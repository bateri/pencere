--@ size 600 190
--@ crop demo
local page = require("../_page")

-- Two boxes that scroll and say the same `scroll_link`, down and across: where one is scrolled the
-- other is scrolled to the same place, in the same frame. The page asks for one box to be scrolled
-- (the picture is made before anyone has a wheel) and the other goes with it.
local asked = false

local function pane(id, name)
	local lines = {}
	for line = 1, 12 do
		lines[#lines + 1] = pencere.text(string.format("%s %02d   the same place in both, down and across", name, line)):no_shrink()
	end
	return pencere.div():id(id):col():w(260):h(120):p(8):border(1, "border"):rounded(6)
		:scroll():scroll_link("sides", "both")
		:children(lines)
end

return page(function()
	if not asked then
		asked = true
		pencere.layout.scroll_to("old", { x = 70, y = 44 })
	end
	return pencere.div():row():gap(16)
		:child(pane("old", "old"))
		:child(pane("new", "new"))
end)
