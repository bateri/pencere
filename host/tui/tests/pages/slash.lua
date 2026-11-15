-- A page for the test of Ctrl and the slash: it binds nothing, and when it is told a key that is the
-- slash it opens a menu of one line that names the modifiers it was pressed with; picking the line copies
-- its name, which is what the terminal's pasteboard is told.

local b = pencere

return {
	render = function()
		return b.div():size_full():p(8):child(b.text("a page that is told the slash"))
	end,

	handle = function(event)
		if event.type == "key" and event.key == "/" then
			local mods = event.mods
			local name = (mods.control and "ctrl-" or "")
				.. (mods.shift and "shift-" or "")
				.. (mods.command and "cmd-" or "")
				.. "slash"
			return { menu = { at = { x = 80, y = 54 }, entries = { { title = name, tag = name } } } }
		elseif event.type == "menu" then
			return { copy = event.entry.tag, redraw = true }
		end
	end,
}
