--@ size 420 220
local page = require("../_page")

return page(function(u)
	return u:button("hint", { title = "Details" })
end, {
	finish = function(u, root)
		root:child(u:popover("hint-card", { anchor = "hint" },
			pencere.div():p(12):gap(4):w(270)
				:child(pencere.text("postgres"):weight(700))
				:child(pencere.text("pid 324, 128.4% CPU, 851 MiB"):color("dim"))))
	end,
})
