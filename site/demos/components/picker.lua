--@ size 420 320
local page = require("../_page")

local function dot(color)
	return pencere.div():w(9):h(9):rounded(5):bg(color)
end

local items = {
	{ key = "bug", title = "Bug", icon = dot("#e5484d"), on = true },
	{ key = "feature", title = "Feature", icon = dot("#8e4ec6") },
	{ key = "api", title = "API", icon = dot("#3e63dd"), on = true },
	{ key = "design", title = "Design", icon = dot("#f76b15") },
	{ key = "docs", title = "Documentation", icon = dot("#30a46c") },
}
local state = { query = "", cursor = "api" }

return page(function(u)
	return u:button("labels", { title = "Labels" })
end, {
	finish = function(u, root)
		root:child(u:picker("labels-picker", { anchor = "labels", items = items, multi = true, state = state,
			label = "Labels", hint = "Change labels" }))
	end,
	on_action = function(action)
		if action.kind == "toggle" then
			action.item.on = action.on
		end
	end,
})
