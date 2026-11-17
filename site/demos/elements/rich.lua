--@ size 520 140
--@ crop demo
local page = require("../_page")

return page(function()
	return pencere.rich({
		"Build ",
		pencere.span("failed"):color("error"):bold(),
		" in ",
		pencere.span("src/main.rs"):link("file"),
		" at line 12: expected ",
		pencere.span("u32"):bg("raised2"),
		", found ",
		pencere.span("i64"):bg("raised2"),
		". The old build ",
		pencere.span("still runs"):italic(),
		".",
	}):wrap():w(360)
end)
