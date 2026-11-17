-- A page for the test of what the screen draws on the bottom row of the window: a strip along the
-- foot that is taller than a row of cells, as the status line of the kit is, with a key cap in it that
-- stands higher than the row. F2 asks for a link, the bar of which has to cover the whole strip.

local b = pencere

return {
	render = function()
		return b.div():size_full():gap(8)
			:child(b.text("Footer"):p(8))
			:child(b.spacer())
			:child(
				b.div():row():items_center():h(24):w_full():px(6):bg("raised1"):child(
					b.div():w(40):h(20):rounded(4):bg("raised2")
				)
			)
	end,

	handle = function(event)
		if event.type == "key" and event.key == "f2" then
			return { link = "https://example.org/pencere?x=1" }
		end
	end,
}
