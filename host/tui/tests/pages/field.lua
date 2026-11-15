-- A page for the tests of the fields the screen draws over a page: one line to type in, with its hint,
-- at the top left. F1 asks the program to lend the terminal to a command (the program writes the word
-- with `printf`), which draws the whole view anew when it ends.

local b = pencere

return {
	render = function()
		return b.div():size_full():p(8):gap(8)
			:child(b.div():text_field("find"):w_full():h(28):child(b.text("Find")))
	end,

	handle = function(event)
		if event.type == "key" and event.key == "f1" then
			pencere.call("handoff", { word = "HANDED-OVER" })
			return true
		end
	end,
}
