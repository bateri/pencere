--@ size 400 100
--@ crop demo
local page = require("../_page")

return page(function(u)
	return u:segments("view", { value = "list", items = {
		{ key = "list", title = "List" },
		{ key = "grid", title = "Grid" },
		{ key = "tree", title = "Tree" },
	} })
end)
