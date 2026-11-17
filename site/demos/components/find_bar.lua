--@ size 600 220
--@ crop demo
local page = require("../_page")

local DOC = "A haystack of words\nwith a needle in it,\nand another needle in the last line.\n"
local state = { query = "needle", replacement = "", replacing = true }

return page(function(u)
	return pencere.div():border(1, "border"):rounded(6):clip()
		:child(u:find_bar("find", { editor = "doc", state = state }))
		:child(pencere.editor("doc", { gutter = false, read_only = true }):w_full():h(70))
end, {
	width = 520,
	on_event = function(event)
		if event.type == "editor_ready" and event.editor == "doc" then
			return { editor = { doc = { load = DOC } } }
		end
	end,
})
