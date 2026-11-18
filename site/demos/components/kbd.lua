--@ size 480 100
--@ crop demo
local page = require("../_page")

return page(function(u)
	return pencere.div():row():items_center():gap(16)
		:child(u:kbd("cmd+shift+p"))
		:child(u:kbd("cmd+k"))
		:child(u:kbd("ctrl+enter", { quiet = true }))
end)
