--@ size 480 160
--@ crop demo
local page = require("../_page")

return page(function(u)
	local bar = pencere.div():row():items_center():gap(10):h(24)
		:child(pencere.text("main"))
		:child(u:rule("y"))
		:child(pencere.text("3 changed"):color("dim"))
		:child(u:rule("y"))
		:child(pencere.text("Ln 12, Col 4"):color("dim"))
	return pencere.div():gap(12):w(300)
		:child(pencere.text("Changes"):weight(700))
		:child(u:rule("x"))
		:child(bar)
end)
