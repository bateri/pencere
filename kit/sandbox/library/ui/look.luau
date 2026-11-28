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

-- Each glyph as a character, where the page is drawn in the cells of a terminal: a character is drawn
-- exactly as the font has it, where a line on a canvas is drawn in as many blocks as it has room for.
-- Only characters that the fonts of a terminal have (Menlo and SF Mono, which are the default ones of
-- macOS, among them): a `✕` or an `ⓘ` is not, and is drawn in another font or not at all.
local CHARACTERS = {
	chevron_right = "▸",
	chevron_down = "▾",
	chevron_left = "◂",
	chevron_up = "▴",
	close = "×",
	check = "✓",
	dash = "-",
	plus = "+",
	dot = "•",
	info = "i",
	warning = "!",
}

-- How far in from the left of its box of 16 points what each glyph draws starts (its strokes' width
-- counted), so that what puts a glyph first can pad to what is seen.
local BEARING = {
	chevron_right = 5.25,
	chevron_down = 3.25,
	chevron_left = 5.25,
	chevron_up = 3.25,
	close = 3.75,
	check = 2.75,
	dash = 3.25,
	plus = 2.75,
	dot = 4.5,
	info = 1.25,
	warning = 1.25,
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

-- Whether the page is drawn in the cells of a terminal (`pencere.draws().cells`): there what is small
-- is a character and what a cell would round to nothing is kept off. A page of a host that says
-- nothing is laid out as it always was.
function look.in_cells()
	return pencere.draws().cells == true
end

-- The glyph `name`, `size` points square, in `color`; in the cells of a terminal, the character that
-- is it, in `color`, a cell.
function look.glyph(name, size, color)
	if look.in_cells() then
		return pencere.text(CHARACTERS[name]):color(color):no_shrink()
	end
	local s = size / 16
	return pencere.canvas(GLYPHS[name](s, math.max(1.5 * s, 1.2), color)):size(size, size):no_shrink()
end

-- The ring of what has the keyboard, over `element`: drawn where `shown` (the keyboard brought it
-- there), and a clear one where it is not, so that it comes and goes with the element's `motion`. An
-- outline takes no room, so what it rings is as large with it as without. It stands off the element by
-- the theme's gap, or `inside` it where the box around the element cuts what is outside it (a strip
-- that scrolls, the track of a choice): its outer edge is then the element's edge.
function look.ring(element, t, shown, inside)
	local offset = inside and -t.ring_width or t.ring_gap
	return element:outline(t.ring_width, shown and t.ring or t.clear, offset)
end

-- How many points of the left of the box of `icon` (`size` points square) are empty: what a glyph does
-- not draw there, and none for a picture or an element, which are the page's, or on cells, where a glyph
-- is a character.
function look.bearing(icon, size)
	if type(icon) ~= "string" or look.in_cells() then
		return 0
	end
	return (BEARING[icon] or 0) * size / 16
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
