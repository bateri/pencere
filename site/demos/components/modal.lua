--@ size 560 320
local page = require("../_page")

return page(function()
	return pencere.text("The page under the modal"):color("dim")
end, {
	finish = function(u, root)
		root:child(u:modal("about", { label = "About" },
			pencere.div():p(20):gap(8):w_full()
				:child(pencere.text("pencere"):text_size(16):weight(700))
				:child(pencere.text("A real interface in a terminal's pane."):wrap():color("dim"))))
	end,
})
