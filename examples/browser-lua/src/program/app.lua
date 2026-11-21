-- browser-lua: the program of a page that has a browser in it (src/view/page.lua, which `serve` shows
-- because it is told no other). It has nothing to say to its page: the browser is the host's, and
-- so are the pages it loads.
--
--   browser-lua          the view
--   browser-lua --text   says what it is, whatever the terminal is

local pencere = require("pencere")

local text = false
for _, argument in ipairs(pencere.args) do
	if argument == "--text" then
		text = true
	else
		print("browser-lua: no option " .. argument)
		pencere.exit(64)
	end
end

local function say_what_it_is()
	print("browser-lua: an address line and a browser, in a terminal that hosts pencere views and their browsers")
end

if text then
	say_what_it_is()
	pencere.exit(0)
end

local outcome = pencere.serve {
	title = "Browser",
	calls = {},
	fallback = say_what_it_is,
}

pencere.exit(outcome.status)
