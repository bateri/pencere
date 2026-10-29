-- @pencere/terminal: editors that look as the person's terminal draws its cursor and its selection.
--
-- The host says how its terminal draws them, as the person set them (`pencere.terminal_look()`: a
-- field only where the host has such a setting), and applies none of it to a page's editors: a page
-- decides what its editors take. This is for a page that wants them all, where it says nothing of its
-- own:
--
--   local terminal = require("@pencere/terminal")
--   pencere.editor("code", terminal.editor({ gutter = false, caret = { shape = "bar" } }))
--
-- `terminal.editor(options, look)` is a copy of `options` (the editor's options: none, `{}`) with each
-- field of the terminal's look that the page did not write put in it: a page that always wants a bar
-- says `caret = { shape = "bar" }` and gets the terminal's rounding, glow and blinking with it, one
-- field at a time (`caret` and `selection` are merged, not replaced). What the page wrote stays,
-- `false` and `0` included. `look` is `pencere.terminal_look()` where none is given: a page that
-- reads it itself to show it (a settings view) gives it so, and a host that says nothing gives the
-- options back as they were.
--
-- It is written on the page API alone.

local terminal = {}

-- The tables of an editor's options that the terminal's look has fields for.
local GROUPS = { "caret", "selection" }

-- `table` copied one level down (a motion is a table of one field: it is a value here).
local function copy(table_)
	local out = {}
	for key, value in table_ do
		out[key] = value
	end
	return out
end

function terminal.editor(options, look)
	if options == nil then
		options = {}
	elseif type(options) ~= "table" then
		error("terminal.editor takes the editor's options, a table, not a " .. type(options), 2)
	end
	if look == nil then
		look = pencere.terminal_look()
	elseif type(look) ~= "table" then
		error("terminal.editor takes the terminal's look, a table, not a " .. type(look), 2)
	end
	local out = copy(options)
	for _, name in GROUPS do
		local said = look[name]
		if type(said) == "table" then
			local own = options[name]
			if own ~= nil and type(own) ~= "table" then
				error("the " .. name .. " of an editor is a table", 2)
			end
			local merged = own ~= nil and copy(own) or {}
			for field, value in said do
				if merged[field] == nil then
					merged[field] = value
				end
			end
			out[name] = merged
		end
	end
	return out
end

return terminal
