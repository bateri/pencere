-- A page for the test of the terminal lent to a command: F1 asks the program to lend it (the
-- program writes the word with `printf`), F2 says it still takes keys. What it shows is made
-- anew when the view is shown again, so it says "ready" again. The editor is there to have the
-- keyboard, and with it a caret that the terminal draws.

local b = pencere

local note = "ready"

return {
	render = function()
		return b.div():size_full():p(8):gap(8)
			:child(b.text("Hand-off"):weight(600))
			:child(b.text("note: " .. note))
			:child(b.editor("doc", { hint = "type here" }):w_full():h(72))
	end,

	handle = function(event)
		if event.type == "key" then
			if event.key == "f1" then
				pencere.call("handoff", { word = "HANDED-OVER" })
				return true
			elseif event.key == "f2" then
				note = "a key after"
				return { redraw = true }
			end
		end
	end,
}
