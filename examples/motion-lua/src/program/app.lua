-- motion-lua: the program of the page of how things move (src/view/page.lua, which `serve` shows
-- because it is told no other). It has nothing to say to its page: every motion is the page's.
--
--   motion-lua          the view
--   motion-lua --text   says what it is, whatever the terminal is

local pencere = require("pencere")

local text = false
for _, argument in ipairs(pencere.args) do
	if argument == "--text" then
		text = true
	else
		print("motion-lua: no option " .. argument)
		pencere.exit(64)
	end
end

local function say_what_it_is()
	print("motion-lua: how a change of a page moves, in a terminal that hosts pencere views")
end

if text then
	say_what_it_is()
	pencere.exit(0)
end

local outcome = pencere.serve {
	title = "Motion",
	fallback = say_what_it_is,
}

pencere.exit(outcome.status)
