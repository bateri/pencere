-- editor-lua: the program of an editor of SQL (src/view/page.lua, which `serve` shows because it is
-- told no other). It has no database: it runs a statement by saying what it would do.
--
--   editor-lua          the view
--   editor-lua --text   says what it is, whatever the terminal is

local pencere = require("pencere")

local text = false
for _, argument in ipairs(pencere.args) do
	if argument == "--text" then
		text = true
	else
		print("editor-lua: no option " .. argument)
		pencere.exit(64)
	end
end

local function say_what_it_is()
	print("editor-lua: an editor of SQL, in a terminal that hosts pencere views")
end

if text then
	say_what_it_is()
	pencere.exit(0)
end

local outcome = pencere.serve {
	title = "SQL",
	calls = {
		-- A statement, run: what it would do, in words.
		run = function(params)
			local sql = params and params.sql
			if type(sql) ~= "string" or sql == "" then
				error({ code = "bad-params", message = "no statement" })
			end
			local verb = string.lower(string.match(sql, "^%s*(%a+)") or "?")
			local words = select(2, string.gsub(sql, "%S+", ""))
			return { note = string.format("A %s of %d words: there is no database here to run it on.", verb, words) }
		end,
	},
	fallback = say_what_it_is,
}

pencere.exit(outcome.status)
