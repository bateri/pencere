-- @pencere/ui, the looks: the tokens every component is drawn with. Each is named for what it is
-- on the page (a surface, the ground of what is chosen, the ring of what has the keyboard) and is
-- said as a role of the theme (`raised1`, `selection`, `accent`, …) or a mix of roles, never as a
-- value: a component follows the theme the pane wears, light or dark, and the host keeps its words
-- readable on the grounds it names.
--
-- A page changes any of them: `ui.new({ theme = { chosen = "raised3", size = { row = 28 } } })`.
-- The sizes are points of the page, which the host draws as large as the terminal's own text is to
-- the kit's (12.5 points): they grow and shrink with the terminal's font. Two densities set them.

local alpha = pencere.alpha

local theme = {}

-- The sizes of each density, points: a row of a list, a bar (tabs, a sidebar's head), the status
-- bar, a control (a button, a field), the room inside a control, the gap between what is in a row,
-- and an icon.
local DENSITIES = {
	regular = { row = 24, bar = 36, status = 24, control = 26, pad = 10, gap = 6, icon = 15 },
	compact = { row = 22, bar = 30, status = 22, control = 22, pad = 8, gap = 4, icon = 13 },
}

-- The tokens that are tables of their own, which a page changes a member at a time; every other
-- token is one colour (and a colour made by `pencere.alpha` or `pencere.mix` is a table too).
local GROUPS = { size = true, text = true, radius = true, tones = true, elevation = true }

local function defaults(density)
	return {
		density = density,
		size = table.clone(DENSITIES[density]),
		-- Text smaller than the page's own (which is the terminal's): a status bar, a badge.
		text = { small = 11.5, tiny = 10.5 },
		radius = { control = 6, card = 10, chip = 4 },
		-- How long a change of colour takes, seconds: the host moves it (`motion`).
		motion = 0.12,

		-- Grounds.
		surface = "raised1", -- a bar, a side panel
		popup = "raised2", -- a palette, a dialog, a menu's card
		chip = "raised3", -- a badge, a key cap
		line = "border", -- rules and the edges of cards
		control = alpha("foreground", 0.06), -- a button at rest
		control_hover = alpha("foreground", 0.1),
		control_press = alpha("foreground", 0.14),
		field = alpha("foreground", 0.05), -- the ground of a field of text
		hover = alpha("foreground", 0.05), -- a row the pointer is over
		press = alpha("foreground", 0.1),
		-- What is chosen, while its list has the keyboard and while it has not: the editor's own
		-- `colors.selection` and `colors.selection_blurred`.
		chosen = "selection",
		chosen_blurred = alpha("selection", 0.5),
		on = alpha("accent", 0.22), -- a toggle that is on
		match = alpha("warning", 0.3), -- what a search found: the editor's `colors.found`
		scrim = alpha("shadow", 0.25), -- the ground behind a dialog

		-- Words.
		ink = "foreground",
		ink_dim = "dim",
		ink_quiet = "quiet",

		-- The ring of what has the keyboard, drawn when the keyboard brought it there.
		ring = alpha("accent", 0.8),
		-- Nothing: the ring's place where there is no ring.
		clear = alpha("background", 0),
		-- The shadow a card casts.
		elevation = { dx = 0, dy = 18, blur = 48, color = alpha("shadow", 0.5) },

		-- What a tone of a badge, a banner, a toast is: a role of the theme.
		tones = { info = "accent", ok = "success", warn = "warning", bad = "error", quiet = "dim" },
	}
end

-- The tokens of `density` ("regular" or "compact"), with what `changes` says in place of them. An
-- error at the caller's `level` for a density or a token there is none of.
function theme.make(changes, density, level)
	density = density or "regular"
	if DENSITIES[density] == nil then
		error('a density is "regular" or "compact", not ' .. tostring(density), level + 1)
	end
	local tokens = defaults(density)
	if changes == nil then
		return tokens
	end
	if type(changes) ~= "table" then
		error("a theme is a table of tokens, not " .. type(changes), level + 1)
	end
	for name, value in changes do
		local mine = tokens[name]
		if mine == nil or name == "density" then
			error("a theme has no token " .. tostring(name), level + 1)
		end
		if GROUPS[name] then
			if type(value) ~= "table" then
				error("the theme's " .. name .. " is a table, not " .. type(value), level + 1)
			end
			for member, given in value do
				if name ~= "tones" and mine[member] == nil then
					error("the theme's " .. name .. " has no " .. tostring(member), level + 1)
				end
				mine[member] = given
			end
		else
			tokens[name] = value
		end
	end
	return tokens
end

-- The role a tone names (`"warn"` is `"warning"`), or the role itself where it is one.
function theme.tone(tokens, tone)
	return tokens.tones[tone] or tone
end

return theme
