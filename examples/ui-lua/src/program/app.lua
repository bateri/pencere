-- ui-lua: the program of the gallery of the kit's components (src/view/page.lua, which `serve` shows
-- because it is told no other). It has nothing to say to its page: every component is the page's.
--
--   ui-lua          the view
--   ui-lua --text   says what it is, whatever the terminal is

local pencere = require("pencere")

local text = false
for _, argument in ipairs(pencere.args) do
	if argument == "--text" then
		text = true
	else
		print("ui-lua: no option " .. argument)
		pencere.exit(64)
	end
end

local function say_what_it_is()
	print("ui-lua: the components of the kit's library, in a terminal that hosts pencere views")
end

if text then
	say_what_it_is()
	pencere.exit(0)
end

local outcome = pencere.serve {
	title = "Components",
	fallback = say_what_it_is,
}

pencere.exit(outcome.status)
