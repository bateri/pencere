-- @pencere/ui, the small pictures the components draw: a chevron, a cross, a check, drawn in lines
-- on a canvas (`pencere.canvas`), in a colour of the theme and at any size; and what a component
-- makes of the icon a page gives it.
--
-- The library has no files of its own: an icon a page gives is a path of its own bundle (an SVG or
-- a PNG, tinted in the colour the component draws it in), an element it made, or the name of one of
-- these glyphs.

local look = {}

-- Each glyph in a box of 16 points, scaled by `s`, its lines `w` wide in `color`.
local GLYPHS = {
	chevron_right = function(s, w, color)
		return { pencere.path():move_to(6 * s, 4 * s):line_to(10 * s, 8 * s):line_to(6 * s, 12 * s):stroke(w, color) }
	end,
	chevron_down = function(s, w, color)
		return { pencere.path():move_to(4 * s, 6 * s):line_to(8 * s, 10 * s):line_to(12 * s, 6 * s):stroke(w, color) }
	end,
	chevron_left = function(s, w, color)
		return { pencere.path():move_to(10 * s, 4 * s):line_to(6 * s, 8 * s):line_to(10 * s, 12 * s):stroke(w, color) }
	end,
	chevron_up = function(s, w, color)
		return { pencere.path():move_to(4 * s, 10 * s):line_to(8 * s, 6 * s):line_to(12 * s, 10 * s):stroke(w, color) }
	end,
	close = function(s, w, color)
		return {
			pencere.path():move_to(4.5 * s, 4.5 * s):line_to(11.5 * s, 11.5 * s):stroke(w, color),
			pencere.path():move_to(11.5 * s, 4.5 * s):line_to(4.5 * s, 11.5 * s):stroke(w, color),
		}
	end,
	check = function(s, w, color)
		return {
			pencere.path():move_to(3.5 * s, 8.5 * s):line_to(6.5 * s, 11.5 * s):line_to(12.5 * s, 4.5 * s):stroke(w, color),
		}
	end,
	dash = function(s, w, color)
		return { pencere.path():move_to(4 * s, 8 * s):line_to(12 * s, 8 * s):stroke(w, color) }
	end,
	plus = function(s, w, color)
		return {
			pencere.path():move_to(8 * s, 3.5 * s):line_to(8 * s, 12.5 * s):stroke(w, color),
			pencere.path():move_to(3.5 * s, 8 * s):line_to(12.5 * s, 8 * s):stroke(w, color),
		}
	end,
	dot = function(s, _, color)
		return { pencere.path():circle(8 * s, 8 * s, 3.5 * s):fill(color) }
	end,
	info = function(s, w, color)
		return {
			pencere.path():circle(8 * s, 8 * s, 6 * s):stroke(w, color),
			pencere.path():move_to(8 * s, 7.5 * s):line_to(8 * s, 11 * s):stroke(w, color),
			pencere.path():circle(8 * s, 5 * s, 0.9 * s):fill(color),
		}
	end,
	warning = function(s, w, color)
		return {
			pencere.path():move_to(8 * s, 2.5 * s):line_to(14 * s, 13 * s):line_to(2 * s, 13 * s):close():stroke(w, color),
			pencere.path():move_to(8 * s, 6.5 * s):line_to(8 * s, 9.5 * s):stroke(w, color),
			pencere.path():circle(8 * s, 11.2 * s, 0.8 * s):fill(color),
		}
	end,
}

-- The names of the glyphs, sorted.
look.GLYPHS = {}
for name in GLYPHS do
	table.insert(look.GLYPHS, name)
end
table.sort(look.GLYPHS)

-- Whether `name` is a glyph's.
function look.is_glyph(name)
	return GLYPHS[name] ~= nil
end

-- The glyph `name`, `size` points square, in `color`.
function look.glyph(name, size, color)
	local s = size / 16
	return pencere.canvas(GLYPHS[name](s, math.max(1.5 * s, 1.2), color)):size(size, size):no_shrink()
end

-- Why `icon` is no icon, or nil: an icon is an element, the name of a glyph, or the path of a file
-- of the page's bundle (it has a `.` or a `/` in it).
function look.check(icon)
	local kind = type(icon)
	if kind == "table" and getmetatable(icon) == pencere.Element then
		return nil
	elseif kind == "string" then
		if string.find(icon, "[./]") or GLYPHS[icon] then
			return nil
		end
		return "there is no glyph " .. icon .. " (a picture of the bundle is its path: \"icons/" .. icon .. ".svg\")"
	end
	return "an icon is an element, a glyph's name or a path of the bundle, not " .. kind
end

-- The icon `icon` (checked by `look.check`), `size` points square, in `color`: a glyph drawn in
-- it, a picture of the bundle tinted in it, or the page's own element as it made it.
function look.icon(icon, size, color)
	if type(icon) ~= "string" then
		return icon
	end
	if GLYPHS[icon] then
		return look.glyph(icon, size, color)
	end
	return pencere.image(icon):size(size, size):tint(color):no_shrink()
end

return look
