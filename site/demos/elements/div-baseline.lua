--@ size 460 240
--@ crop demo
local page = require("../_page")

-- Words of two sizes in a row: centred, their baselines are not on one line; with `items_baseline()`
-- the first line of each stands on the same one, as `align-items: baseline` does.
local function row(label, setting)
	local heading = pencere.div():row():gap(10):p(12):bg("raised1"):border(1, "border"):rounded(8)
		:child(pencere.text("Pull requests"):text_size(22):weight(700))
		:child(pencere.text("12 open"):text_size(12):color("dim"))
	return pencere.div():gap(6)
		:child(pencere.text(label):text_size(11):color("dim"))
		:child(setting(heading))
end

return page(function()
	return pencere.div():col():gap(14)
		:child(row("items_center()", function(heading) return heading:items_center() end))
		:child(row("items_baseline()", function(heading) return heading:items_baseline() end))
end, { width = 380 })
