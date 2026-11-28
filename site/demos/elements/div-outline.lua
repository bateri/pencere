--@ size 460 150
--@ crop demo
local page = require("../_page")

-- A ring that takes no room, over the box: a point clear of it (its corners follow the box's), inside
-- it where the box around cuts what is outside, and square on a box that has square corners.
local function card(words, setting)
	local box = pencere.div():w(104):h(56):items_center():justify_center():bg("raised1"):border(1, "border")
		:rounded(10):child(pencere.text(words):text_size(11):color("dim"))
	return setting(box)
end

return page(function()
	return pencere.div():row():gap(24):p(8)
		:child(card("offset 1", function(box) return box:outline(2, "accent", 1) end))
		:child(card("offset -2", function(box) return box:outline(2, "accent", -2) end))
		:child(card("square", function(box) return box:rounded(0):outline(2, "accent", 1) end))
end, { width = 380 })
