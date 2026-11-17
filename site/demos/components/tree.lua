--@ size 400 280
--@ crop demo
local page = require("../_page")

local files = {
	src = { "src/main.lua", "src/lib", "src/notes.md" },
	["src/lib"] = { "src/lib/util.lua", "src/lib/ui.lua" },
	docs = { "docs/guide.md" },
}
local state = { expanded = { src = true, ["src/lib"] = true }, cursor = "src/main.lua" }

return page(function(u)
	return pencere.div():h(200):border(1, "border"):rounded(6):clip():child(
		u:tree("files", {
			roots = { "src", "docs", "README.md" },
			children = function(key) return files[key] end,
			item = function(key)
				return { title = string.match(key, "[^/]+$"), badge = key == "src/notes.md" and "M" or nil }
			end,
			state = state,
		}))
end, { width = 240 })
