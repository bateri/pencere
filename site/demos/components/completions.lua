--@ size 560 260
--@ focus code
--@ press cmd+down
local page = require("../_page")

local DOC = "local ui = require(\"@pencere/ui\")\nlocal u = ui.new()\n\nu:b"
local items = {
	{ key = "button", title = "button", detail = "(id, props)", icon = "dot" },
	{ key = "badge", title = "badge", detail = "(words, props)", icon = "dot" },
	{ key = "banner", title = "banner", detail = "(id, props)", icon = "dot" },
	{ key = "begin", title = "begin", detail = "(size, now, reduce_motion)", icon = "dot" },
}

return page(function()
	return pencere.editor("code", { gutter = true }):w(480):h(160)
end, {
	finish = function(u, root)
		root:child(u:completions("complete", { editor = "code", items = items }))
	end,
	on_event = function(event)
		if event.type == "editor_ready" and event.editor == "code" then
			return { editor = { code = { load = DOC } } }
		end
	end,
})
