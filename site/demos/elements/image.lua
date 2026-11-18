--@ size 520 160
--@ crop demo
local page = require("../_page")

return page(function()
	return pencere.div():row():items_center():gap(20)
		:child(pencere.image("elements/assets/window.svg"):w(64):h(64):tint("accent"))
		:child(pencere.image("elements/assets/window.svg"):w(40):h(40):tint("foreground"))
		:child(pencere.image("elements/assets/window.svg"):w(24):h(24):tint("dim"))
end)
