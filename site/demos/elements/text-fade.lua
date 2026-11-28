--@ size 460 120
--@ crop demo
local page = require("../_page")

-- A text that does not fit its box is cut with "…" (`truncate()`) or thins out where the box ends
-- (`truncate("fade")`, over two of its letters' widths).
local NAME = "A name much longer than the room it has in a list"

return page(function()
	return pencere.div():col():gap(10)
		:child(pencere.text(NAME):truncate():w(220))
		:child(pencere.text(NAME):truncate("fade"):w(220))
		:child(pencere.text(NAME):w(220):no_shrink())
end, { width = 380 })
