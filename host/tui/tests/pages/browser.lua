-- A page for the test of what a terminal that shows no browser draws in a browser's room: a line of
-- words above, and the room under it, which is the rest of the window.

local b = pencere

return {
	render = function()
		return b.div():size_full():p(8):gap(8)
			:child(b.text("A browser"))
			:child(b.browser("web"):grow(1))
	end,

	handle = function() end,
}
