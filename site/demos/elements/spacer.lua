--@ size 520 120
--@ crop demo
local page = require("../_page")

return page(function()
	return pencere.div():w(360):row():items_center():px(12):py(8):border(1, "border"):rounded(6)
		:child(pencere.text("main"))
		:child(pencere.spacer())
		:child(pencere.text("Ln 12, Col 4"):color("dim"))
end)
