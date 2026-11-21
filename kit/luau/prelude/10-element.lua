-- The page API, part 2: elements and the chain that writes them,
-- `pencere.div():row():gap(8):child(pencere.text("hello"))`.
--
-- An element is a table: { kind = "box" | "text" | "rich" | "spark" | "image" | "canvas",
-- style = {...}, meta = {...}, kids = {...} }.
-- The host reads it, so the field names are the interface's (`pencere:view` `style`, `node`).

local internal = ...

local Element = {}
Element.__index = Element
pencere.Element = Element

local function make(kind)
	return setmetatable({ kind = kind, style = {}, meta = {}, kids = {} }, Element)
end

-- What a flag is told: on when it is called with nothing (`:hidden()`), otherwise what it is given,
-- so that a field with nothing in it (`:selected(row.chosen)`) is off.
local function told(...)
	if select("#", ...) == 0 then
		return true
	end
	return (...) and true or false
end

-- A helper that finds its argument wrong raises the error at the line of the script that called
-- the page API, not at a line of this prelude: the person who reads it has the script.
local refuse = internal.refuse

-- A box: a column of its children until told otherwise.
function pencere.div()
	return make("box")
end

-- One line of text, in the font, size and colour of the boxes it is in.
function pencere.text(content)
	local kind = type(content)
	if kind ~= "string" and kind ~= "number" then
		refuse("pencere.text takes a string or a number, not " .. kind)
	end
	local element = make("text")
	element.text = tostring(content)
	return element
end

-- A sparkline of `values` (newest last), `color` the line.
function pencere.spark(values, color)
	if type(values) ~= "table" then
		refuse("pencere.spark takes a list of numbers, not " .. type(values))
	end
	local element = make("spark")
	element.values = values
	element.line = color
	return element
end

-- A picture: a file of the bundle the view came in, in the box of the element.
--
--   pencere.image("icons/stop.svg"):w(24):fit("cover"):tint("error"):opacity(0.8)
--
-- The argument is the path of the file as the bundle's manifest names it (PNG, JPEG, SVG or PDF). Its
-- size is the element's, as for any box (`:w(24)`, `:h(24)`); a side that is not said follows the
-- picture's proportions, and an element that says neither is the picture's own size in pixels, a
-- point to a pixel: a picture meant to be sharp on a screen with two pixels to the point is
-- sent at twice the size it is given. `:fit` is "contain" (all of the picture, centred; the
-- default) or "cover" (the box filled, what does not fit cut off). `:tint` is a colour (any way
-- of saying one, as for text): the picture is then a shape, drawn in that colour with its own
-- opacity at each pixel. How much of it shows is the element's `:opacity`, as for any element.
-- `:page` is the page of a PDF that is shown (from 1, the first where it is not said): the
-- picture is that page, at the page's own size in points.
-- Besides a file of the bundle, `src` is a file beside it: "sha256:" and the digest of one the
-- program attached to the view, "file:" and the handle of one the person gave it (a `files` event).
function pencere.image(src)
	if type(src) ~= "string" then
		refuse("pencere.image takes the path of a file of the bundle, not " .. type(src))
	end
	local element = make("image")
	-- What the picture is lives in a table of its own: the host reads these by their names, and a
	-- name that is also a method of the chain (`fit`, `tint`) would hide the method.
	element.picture = { src = src, fit = "contain" }
	return element
end

-- The picture of an element that is one; the error of any other, at the line that called
-- `method`.
local function picture_of(element, method)
	if element.kind ~= "image" then
		refuse(method .. " is for an image, and this is a " .. element.kind)
	end
	return element.picture
end

-- How the picture fills the box: "contain" or "cover".
function Element:fit(how)
	local picture = picture_of(self, "fit")
	if how ~= "contain" and how ~= "cover" then
		refuse('fit is "contain" or "cover", not ' .. tostring(how))
	end
	picture.fit = how
	return self
end

-- Which page of the file the picture is, from 1: a PDF has as many as it says, and a file that has
-- no pages (a PNG, a JPEG, an SVG) is its only page. A page the file does not have is a picture
-- that is not drawn, and the page's diagnostics say so.
function Element:page(number)
	local picture = picture_of(self, "page")
	if type(number) ~= "number" or number < 1 or number % 1 ~= 0 or number > 4294967295 then
		refuse("page is a whole number from 1, not " .. tostring(number))
	end
	picture.page = number
	return self
end

-- Draws the picture as a shape of this colour (nil for as it is).
function Element:tint(color)
	picture_of(self, "tint").tint = color
	return self
end

-- How much of the element shows with everything inside it, 0 to 1: each thing it draws is that
-- much fainter (where the things inside overlap, they show through each other).
function Element:opacity(opacity)
	if type(opacity) ~= "number" then
		refuse("opacity is a number from 0 to 1, not " .. type(opacity))
	end
	self.style.opacity = opacity
	return self
end

-- A box that takes the room left over: what pushes the elements after it to the end.
function pencere.spacer()
	return make("box"):grow(1)
end

-- --- Text of many looks -----------------------------------------------------------------------

-- A part of a text of many looks: its words, and what it changes of the element's look (what it
-- leaves out is the element's, as for the words of `pencere.text`). The size is the element's.
--
--   pencere.rich({ pencere.span("run "), pencere.span("make"):family("terminal"):bg("raised2") })
local Span = {}
Span.__index = Span
pencere.Span = Span

function pencere.span(words)
	local kind = type(words)
	if kind ~= "string" and kind ~= "number" then
		refuse("pencere.span takes a string or a number, not " .. kind)
	end
	-- What the part changes lives in a table of its own: its names are the methods' too.
	return setmetatable({ text = tostring(words), look = {} }, Span)
end

function Span:color(color) self.look.color = color return self end
function Span:weight(weight) self.look.weight = weight return self end
function Span:bold(...)
	if told(...) then
		self.look.weight = 700
	elseif self.look.weight == 700 then
		self.look.weight = nil
	end
	return self
end
function Span:italic(...) self.look.italic = told(...) return self end
function Span:underline(...) self.look.underline = told(...) return self end
function Span:strike(...) self.look.strike = told(...) return self end
-- "system" or "terminal".
function Span:family(family) self.look.family = family return self end
-- A ground behind the words of this part alone: a highlight, the box of a piece of code.
function Span:bg(color) self.look.background = color return self end

-- The part is a link named `id`: it answers the pointer as a pressed element does (`layout.hit`
-- says its name), a screen reader says it is a link, and it is underlined unless it says not.
function Span:link(id)
	self.look.link = id
	if self.look.underline == nil then
		self.look.underline = true
	end
	return self
end

-- A text of many looks: its parts one after the other, as one paragraph that runs from line to
-- line where it `wrap`s (a word made of two parts is not broken between them).
function pencere.rich(spans)
	if type(spans) ~= "table" then
		refuse("pencere.rich takes a list of parts (pencere.span), not " .. type(spans))
	end
	local element = make("rich")
	local parts = {}
	for index, part in ipairs(spans) do
		if type(part) == "string" then
			part = pencere.span(part)
		elseif getmetatable(part) ~= Span then
			refuse("part " .. index .. " of pencere.rich is not a pencere.span or a string")
		end
		-- The host reads a part by the interface's names, in a plain table.
		local look = part.look
		parts[index] = {
			text = part.text,
			color = look.color,
			weight = look.weight,
			italic = look.italic,
			underline = look.underline,
			strike = look.strike,
			family = look.family,
			background = look.background,
			link = look.link,
		}
	end
	element.spans = parts
	return element
end

-- --- Drawing in lines and shapes --------------------------------------------------------------

-- A path of a canvas, in the points of the canvas's box from its top-left corner:
--
--   pencere.path():move_to(0, 10):line_to(20, 4):stroke(1.5, "accent")
--   pencere.path():arc(12, 12, 9, 0, 270):stroke(3, "accent")       -- a ring three quarters full
--   pencere.path():rect(0, 0, 8, 20):fill("raised3")
--   pencere.path():rect(0, 0, 2, 2):fill("accent"):crisp()           -- a dot of whole pixels
--   pencere.path():circle(8, 8, 3):fill("accent"):shadow(0, 0, 12, "accent"):blend("add")  -- a glow
--   pencere.path():rect(0, 0, 40, 6):fill(pencere.linear(0, 0, 40, 0, { { 0, "accent" }, { 1, "success" } }))
--   pencere.path():circle(20, 20, 9):stroke(2, "dim"):dash({ 3, 3 }):cap("butt")  -- a dashed ring
--   pencere.path():rect(0, 0, 8, 8):fill("accent"):transform({ rotate = 45, origin = { 4, 4 } })
local Path = {}
Path.__index = Path
pencere.Path = Path

function pencere.path()
	-- How it is drawn lives in a table of its own: `fill`, `stroke`, `crisp`, `blend`, `shadow`,
	-- `dash`, `cap`, `join` and `transform` are the methods' names too.
	return setmetatable({ segments = {}, paint = {} }, Path)
end

local function step(path, ...)
	local segments = path.segments
	segments[#segments + 1] = { ... }
	return path
end

function Path:move_to(x, y) return step(self, "move", x, y) end
function Path:line_to(x, y) return step(self, "line", x, y) end
function Path:quad_to(cx, cy, x, y) return step(self, "quad", cx, cy, x, y) end
function Path:cubic_to(ax, ay, bx, by, x, y) return step(self, "cubic", ax, ay, bx, by, x, y) end
function Path:close() return step(self, "close") end

function Path:rect(x, y, w, h)
	return self:move_to(x, y):line_to(x + w, y):line_to(x + w, y + h):line_to(x, y + h):close()
end

-- An arc of the circle about (cx, cy) of radius r, from the angle `start` to `finish`, in degrees
-- clockwise from the top (0 to 360 is the whole circle, from twelve o'clock round). It starts a
-- new part of the path where it starts.
function Path:arc(cx, cy, r, start, finish)
	local sweep = math.clamp(finish - start, -360, 360)
	if sweep == 0 or sweep ~= sweep then
		return self
	end
	local function at(degrees)
		local radians = math.rad(degrees - 90)
		return cx + r * math.cos(radians), cy + r * math.sin(radians)
	end
	local function tangent(degrees)
		local radians = math.rad(degrees - 90)
		return -math.sin(radians), math.cos(radians)
	end
	self:move_to(at(start))
	-- Each piece is at most a quarter turn, as the cubic curve that follows a circle best.
	local pieces = math.clamp(math.ceil(math.abs(sweep) / 90), 1, 4)
	local each = sweep / pieces
	local handle = 4 / 3 * math.tan(math.rad(each) / 4) * r
	for piece = 0, pieces - 1 do
		local from = start + each * piece
		local to = from + each
		local x0, y0 = at(from)
		local x3, y3 = at(to)
		local tx0, ty0 = tangent(from)
		local tx3, ty3 = tangent(to)
		self:cubic_to(x0 + tx0 * handle, y0 + ty0 * handle, x3 - tx3 * handle, y3 - ty3 * handle, x3, y3)
	end
	return self
end

function Path:circle(cx, cy, r)
	return self:arc(cx, cy, r, 0, 360):close()
end

-- What a fill or a line is painted with when it is not one colour: a colour that changes along a
-- line or out from a point, from its stops, `{ { 0, "accent" }, { 1, "background" } }` (a number from
-- 0 at its start to 1 at its end, then a colour; at most 16). Its points are the path's own, so they
-- go where the path's transform takes it. One stop is that colour all over; a gradient whose two
-- points are one, or whose radius is nothing, is its last stop's colour.
--
--   pencere.linear(0, 0, 100, 0, { { 0, "accent" }, { 1, "success" } })
--   pencere.radial(50, 50, 40, { { 0, "accent" }, { 1, { "accent", 0 } } })
local Brush = {}
table.insert(internal.classes, Brush)

local function numbers(name, what, ...)
	for index = 1, select("#", ...) do
		local value = select(index, ...)
		if type(value) ~= "number" then
			refuse(name .. " takes " .. what .. " as numbers, not " .. type(value))
		end
	end
end

local function stops_of(name, stops)
	if type(stops) ~= "table" or #stops == 0 then
		refuse(name .. " takes its stops as a list, { { 0, \"accent\" }, { 1, \"background\" } }")
	end
	if #stops > 16 then
		refuse(name .. " takes at most 16 stops, not " .. #stops)
	end
	local made = {}
	for index = 1, #stops do
		local stop = stops[index]
		if type(stop) ~= "table" or type(stop[1]) ~= "number" or stop[2] == nil then
			refuse("stop " .. index .. " of " .. name .. " is an offset and a colour, { 0, \"accent\" }")
		end
		made[index] = { stop[1], stop[2] }
	end
	return made
end

function pencere.linear(x0, y0, x1, y1, stops)
	numbers("pencere.linear", "the two points of its line", x0, y0, x1, y1)
	return setmetatable({
		kind = "linear",
		x0 = x0, y0 = y0, x1 = x1, y1 = y1,
		stops = stops_of("pencere.linear", stops),
	}, Brush)
end

function pencere.radial(cx, cy, radius, stops)
	numbers("pencere.radial", "its middle and its radius", cx, cy, radius)
	return setmetatable({
		kind = "radial",
		cx = cx, cy = cy, radius = radius,
		stops = stops_of("pencere.radial", stops),
	}, Brush)
end

-- The colour (or gradient) inside the path.
function Path:fill(color) self.paint.fill = color return self end

-- The line along the path, `width` points, in a colour or a gradient; round where it turns and
-- ends, and with no gaps, unless `dash`, `cap` and `join` say otherwise (before or after this).
function Path:stroke(width, color)
	self.paint.stroke = { width = width, color = color }
	return self
end

-- Drawn with no smoothing, its pixels whole: the dots of a dithered fill, pixel art.
function Path:crisp(...)
	self.paint.crisp = told(...)
	return self
end

local blends = { normal = true, add = true, screen = true, multiply = true }

-- How the path is drawn over what the canvas has drawn under it (not the page behind the canvas):
-- "normal", "add" (lights that overlap grow brighter), "screen" or "multiply".
function Path:blend(name)
	if not blends[name] then
		refuse("a blend is \"normal\", \"add\", \"screen\" or \"multiply\", not " .. tostring(name))
	end
	self.paint.blend = name
	return self
end

-- What the path leaves behind it, drawn under it in its blend: its fill and its line moved `dx` and
-- `dy` points and blurred over `blur` points (0 to 64; 0 is not blurred at all), in `color`. With no
-- offset and a bright colour, a glow. The canvas's own pixels are all there is: past its box a
-- shadow is cut.
function Path:shadow(dx, dy, blur, color)
	numbers("path:shadow", "its offset and its blur", dx, dy, blur)
	if type(color) ~= "string" and type(color) ~= "table" then
		refuse("path:shadow takes its colour as a colour, \"accent\" or \"#rrggbb\", not " .. type(color))
	end
	self.paint.shadow = { dx = dx, dy = dy, blur = blur, color = color }
	return self
end

-- The line broken into dashes and gaps of the lengths in `pattern` ({ 4, 2 }: four points of line,
-- two of none, in turn; at most 16; an odd number is read twice), starting `offset` points into it.
function Path:dash(pattern, offset)
	if type(pattern) ~= "table" then
		refuse("path:dash takes a list of lengths, { 4, 2 }, not " .. type(pattern))
	end
	if #pattern > 16 then
		refuse("path:dash takes at most 16 lengths, not " .. #pattern)
	end
	local lengths = {}
	for index = 1, #pattern do
		local length = pattern[index]
		if type(length) ~= "number" then
			refuse("length " .. index .. " of path:dash is a number, not " .. type(length))
		end
		lengths[index] = length
	end
	if offset ~= nil and type(offset) ~= "number" then
		refuse("path:dash takes its offset as a number, not " .. type(offset))
	end
	self.paint.dash = lengths
	self.paint.dash_offset = offset or 0
	return self
end

local caps = { round = true, butt = true, square = true }

-- How the ends of the line are drawn: "round" (the default), "butt" or "square".
function Path:cap(name)
	if not caps[name] then
		refuse("a cap is \"round\", \"butt\" or \"square\", not " .. tostring(name))
	end
	self.paint.cap = name
	return self
end

local joins = { round = true, miter = true, bevel = true }

-- How the line is drawn where it turns: "round" (the default), "miter" (a point; one too sharp for it,
-- longer than four times half the line's width, is bevelled) or "bevel".
function Path:join(name)
	if not joins[name] then
		refuse("a join is \"round\", \"miter\" or \"bevel\", not " .. tostring(name))
	end
	self.paint.join = name
	return self
end

local transforms = { scale = true, rotate = true, translate = true, origin = true }

local function pair(name, value)
	if type(value) ~= "table" or type(value[1]) ~= "number" or type(value[2]) ~= "number" then
		refuse("transform's " .. name .. " is two numbers, { x, y }")
	end
	return value[1], value[2]
end

-- Where the path is drawn, `{ scale =, rotate =, translate =, origin = }` (each as it is wanted):
-- scaled (`scale`, a number or `{ sx, sy }`), then turned (`rotate`, degrees clockwise, as `arc`
-- goes), both about `origin` (`{ x, y }`, the canvas's top-left corner when it is not given), then
-- moved (`translate`, `{ x, y }`). Its line's width, its dashes and its gradients go by it too.
-- Called again, it replaces the one before.
function Path:transform(parts)
	if type(parts) ~= "table" then
		refuse("path:transform takes { scale =, rotate =, translate =, origin = }, not " .. type(parts))
	end
	for key in parts do
		if not transforms[key] then
			refuse("a transform has scale, rotate, translate and origin, not " .. tostring(key))
		end
	end
	local made = { sx = 1, sy = 1, rotate = 0, tx = 0, ty = 0, ox = 0, oy = 0 }
	local scale = parts.scale
	if type(scale) == "number" then
		made.sx, made.sy = scale, scale
	elseif scale ~= nil then
		made.sx, made.sy = pair("scale", scale)
	end
	if parts.rotate ~= nil then
		if type(parts.rotate) ~= "number" then
			refuse("transform's rotate is a number of degrees, not " .. type(parts.rotate))
		end
		made.rotate = parts.rotate
	end
	if parts.translate ~= nil then
		made.tx, made.ty = pair("translate", parts.translate)
	end
	if parts.origin ~= nil then
		made.ox, made.oy = pair("origin", parts.origin)
	end
	self.paint.transform = made
	return self
end

-- A box with paths drawn in it. It has no size of its own: `:size(w, h)` gives it one.
function pencere.canvas(paths)
	if type(paths) ~= "table" then
		refuse("pencere.canvas takes a list of paths (pencere.path), not " .. type(paths))
	end
	local plain = {}
	for index, path in ipairs(paths) do
		if getmetatable(path) ~= Path then
			refuse("path " .. index .. " of pencere.canvas is not a pencere.path")
		end
		local paint = path.paint
		local stroke = paint.stroke
		plain[index] = {
			segments = path.segments,
			fill = paint.fill,
			stroke = stroke and {
				width = stroke.width,
				color = stroke.color,
				dash = paint.dash,
				dash_offset = paint.dash_offset,
				cap = paint.cap,
				join = paint.join,
			},
			crisp = paint.crisp,
			blend = paint.blend,
			shadow = paint.shadow,
			transform = paint.transform,
		}
	end
	local element = make("canvas")
	element.paths = plain
	return element
end

-- --- Children ---------------------------------------------------------------------------------

-- Adds `child` after the ones already here (nothing, for nothing).
function Element:child(child)
	if child ~= nil then
		self.kids[#self.kids + 1] = child
	end
	return self
end

-- Adds every element of the list.
function Element:children(list)
	if type(list) ~= "table" then
		refuse("children takes a list of elements, not " .. type(list))
	end
	for _, child in ipairs(list) do
		self.kids[#self.kids + 1] = child
	end
	return self
end

-- --- Names and answers ------------------------------------------------------------------------

-- Names the element (a string, or a whole number), so that what is kept about it between
-- frames and what the host is asked of it can find it.
function Element:id(id)
	self.meta.id = id
	return self
end

-- Names the element and makes it something the pointer answers.
function Element:press(id)
	self.meta.id = id
	self.meta.pressable = true
	return self
end

-- The keyboard can be on this element (it is named too).
function Element:focusable(...)
	self.meta.focusable = told(...)
	return self
end

local FIELD_KINDS = { line = true, lines = true, secret = true }

-- The box is the room of a text field the host puts over the page, named `id` (the element's own
-- name when `id` is nil): a page has as many as it names. `kind` is "line" (the default), "lines"
-- (enter breaks the line) or "secret" (a password). The first text inside is what the field says
-- while it is empty.
function Element:text_field(id, kind)
	if id ~= nil then
		self.meta.id = id
	end
	if self.meta.id == nil then
		refuse("a text field is named: text_field(id)")
	end
	kind = kind or "line"
	if not FIELD_KINDS[kind] then
		refuse('a text field is "line", "lines" or "secret", not ' .. tostring(kind))
	end
	self.meta.field = kind
	return self
end

-- The box is the room of an editor of text the host puts there, named `id` (the element's own name
-- when `id` is nil). The host keeps its document, edits it, lays out and draws its lines and
-- scrolls them (as a box that scrolls: `pencere.layout.scroll_to(id, …)`); the page is told what
-- changed (`editor_ready` once it has made it, `edit`, `selection`, `viewport`, `editor_refused`,
-- and `found` while it searches) and asks for what it wants in a reply (`editor = { [id] = … }`). Its document lives as long as an editor of its name is on the page,
-- hidden or not. `options` (all optional): `wrap`, `tab_width` (4), `hard_tabs`, `gutter` (true),
-- `read_only`, `input` ("editor", or "page": every key the page's), `hint` (what it says while
-- empty), `pairs` (the brackets and quotes that close themselves: "()[]{}" or { "()", "[]" }); what
-- its language says, all of it left out for prose: `comment` (what the commands that comment put: a
-- line comment, "--", or { line = "--", block = { "--[[", "]]" } }, either of them, the block a pair
-- or { open =, close = }), `indent = { increase =, decrease = }` (regular expressions, Rust's: a
-- line break after a line that matches `increase` indents a level more, and a line that what is
-- typed makes match `decrease` goes a level out; one that is no pattern fails the page) and `folds`
-- ("page", the page's fold decorations alone, or "indent", the editor's from the indent as well);
-- how it looks: `line_height` (1.6
-- times the font's size), `padding` (4 points), `scroll_margin` (rows kept about the caret, 0),
-- `numbers` ("absolute", "relative", "hidden"), `gutters` (the gutter's columns, from the left: the
-- editor's `"marks"` (dots), `"numbers"` and `"folds"` — the three there are where it says none —
-- and the page's own, `{ name = "breakpoints", width = 14, events = { "press", "context", "hover" },
-- hover = { shape = "dot", color = … } }`: `events` says what of the pointer the page hears of the
-- column (a page's column: presses of both buttons; `false`: nothing; a press there selects nothing
-- and folds nothing), `hover` what shows, faint, in the cell the pointer is over where nothing
-- stands; `pad` the points the `"numbers"` keep to their left: the padding where they are the first
-- column, none after another), `marks = { bar = 3, hover = 6, ruler = 4 }` (how wide a bar, a bar the pointer is over
-- and the overview ruler are), `events = { "hover" }` (what of its own the editor tells the page:
-- the pointer's rest over a word and its going from it, once `hover = { delay = 0.3 }` seconds have
-- passed; and "target": the pointer over a word with the modifiers of a press that is the page's held,
-- and that press), `gestures` (how presses are taken, by their modifiers, button and count:
-- `{ ["press"] = "place", ["shift+press"] = "extend", ["mod+press"] = "target", ["alt+press"] =
-- "column", ["press*2"] = "word", ["press*3"] = "line" }` are what an editor does where the page says
-- nothing; "add_caret" and false (nothing) are the others; "target" is the page's and needs
-- `events = { "target" }`; "context" is a press that opens the page's menu: `["right+press"] =
-- "context"` puts one caret where the press is, unless it is in a selection (then it stays) and the
-- editor has the keyboard; the page is told the press as any, with `place.inside`; a Mac's ⌃ with the
-- main button is that right press where the page says nothing of ⌃; `mod` is the host's main modifier), `keymap` (the editor's keys the page
-- changes: `{ ["mod+d"] = "select_next", ["mod+/"] = false, ["mod+k mod+c"] = "toggle_comment" }`, the keys
-- of a chord apart by a space; what they do is the name of a command that needs none, "copy", "cut",
-- "tab", "escape", "delete_to_line_start", "delete_to_line_end", a command's table, or false: nothing of
-- the editor's, and no word pastes; `pencere.layout.keymap(editor)` says what the keys do now), `chord_timeout` (seconds a
-- chord waits for its next key, 1), `links = { delay = 0.05 }` (the least time between two words told
-- of), `widgets` (how the groups of text the page puts in lines, `{ from =, to =, widget = "words", side =,
-- group = }`, look and when they show: `{ hints = { show = "unless_held", hold = "ctrl+alt", size = 0.9,
-- color =, background =, radius = 3, pad = 3, gap = 16, margin = 3 } }`; "on", "off", "unless_held", "when_held"),
-- `current_line` (where the caret's line is drawn:
-- "text", "gutter", "all"), `whitespace` (which spaces and tabs it shows, as VS Code: "none",
-- "boundary" (all but a space alone between words), "selection", "trailing" or "all"; a space is a
-- dot, a tab an arrow, any other space a box), `indent_guides` (a thin line down each level of
-- indent, the caret's block's in its own colour), `matches` (the other places of the main selection's
-- text marked where they are in view: "none", "selection" (or true), or "word", the caret's word
-- too), `caret = { shape = "bar" | "block" | "underline", height = "row" | "text", width = 2, color =,
-- text =, radius = 0, glow = 0, blink = true, blink_interval = 0.5, blink_for = 15, unfocused = "hidden",
-- motion = "snap" }`
-- (radius: a fraction of a row's height, 0 to 0.5; glow: the soft light round it, 0 to 3; `blink_interval`:
-- seconds lit and then dark; `blink_for`: seconds it goes on blinking after it moved, 0: for ever;
-- `unfocused`: what it is while the window or the pane has not the keyboard, "hidden", "hollow" (a block
-- is an outline) or "solid"; `motion`: how it goes from a place to another, "snap" (at once), "ease" (0.18
-- seconds, a terminal's), "spring" (a spring of 30, a terminal's, that keeps its speed where it is sent on),
-- `{ ease = seconds }` or `{ spring = stiffness }`, and at once with Reduce Motion), `selection = { radius = 0 }` (how round the corners of a selection are, a fraction of a row's
-- height, 0 to 0.5: 0 square; where lines meet the turns inward are rounded too) and `colors = { current_line, selection, selection_blurred, found, brackets, numbers,
-- current_number, gutter, folds, folded, hint, whitespace, indent_guides, active_indent_guide,
-- matches, mark_hover }`. A colour it does not say is the theme's;
-- `false` draws none. A plain editor shows no spaces and no guides.
local EDITOR_OPTIONS = {
	wrap = true,
	tab_width = true,
	hard_tabs = true,
	gutter = true,
	read_only = true,
	input = true,
	hint = true,
	comment = { line = true, block = true },
	pairs = true,
	indent = { increase = true, decrease = true },
	folds = true,
	line_height = true,
	padding = true,
	scroll_margin = true,
	numbers = true,
	gutters = true,
	marks = { bar = true, hover = true, ruler = true },
	events = true,
	hover = { delay = true },
	gestures = true,
	keymap = true,
	widgets = true,
	chord_timeout = true,
	links = { delay = true },
	current_line = true,
	whitespace = true,
	indent_guides = true,
	matches = true,
	caret = {
		shape = true,
		height = true,
		width = true,
		color = true,
		text = true,
		radius = true,
		glow = true,
		blink = true,
		blink_interval = true,
		blink_for = true,
		unfocused = true,
		motion = true,
	},
	selection = { radius = true },
	colors = {
		current_line = true,
		selection = true,
		selection_blurred = true,
		found = true,
		brackets = true,
		numbers = true,
		current_number = true,
		gutter = true,
		folds = true,
		folded = true,
		hint = true,
		whitespace = true,
		indent_guides = true,
		active_indent_guide = true,
		matches = true,
		mark_hover = true,
	},
}

-- An option an editor has no such name for is a mistake, said where it is made: a misspelt one
-- would be dropped without a word.
local function check_editor_options(given, known, where)
	for key, value in given do
		local takes = known[key]
		if takes == nil then
			refuse("an editor has no option " .. where .. tostring(key))
		end
		if type(takes) == "table" and type(value) == "table" then
			check_editor_options(value, takes, where .. key .. ".")
		end
	end
end

function Element:editor(id, options)
	if id ~= nil then
		self.meta.id = id
	end
	if self.meta.id == nil then
		refuse("an editor is named: editor(id, options)")
	end
	if options ~= nil and type(options) ~= "table" then
		refuse("an editor's options are a table, not " .. type(options))
	end
	local given = options or {}
	check_editor_options(given, EDITOR_OPTIONS, "")
	local said = {}
	for key, value in given do
		said[key] = value
	end
	if type(said.pairs) == "table" then
		said.pairs = table.concat(said.pairs)
	end
	self.meta.editor = said
	internal.keymaps[self.meta.id] = said.keymap
	self.style.overflow = "scroll"
	return self
end

-- An editor named `id`, as `pencere.div():editor(id, options)`.
function pencere.editor(id, options)
	return make("box"):editor(id, options)
end

-- The box is the room of a browser the host puts there, named `id` (the element's own name when
-- `id` is nil): the host's engine loads the pages and draws them in the room. It lives as long as
-- an element of that name is on the page, hidden or not, and its history goes with it; the same
-- `id` is the same browser. Where it goes is not an option but an ask, in the `browser` of a reply:
-- `{ go = "https://..." }`, `{ zoom = 1.25 }` (from 0.25 to 5), or `{ command = name }`: "back", "forward",
-- "reload" (`cache = false` loads the page again from where it is), "stop", "find" (`pattern`, `case`),
-- "find_next", "find_previous", "stop_finding" and "devtools". An ask the kit does not know is refused
-- (`browser_refused`, "ask"). `options` are
-- `profile` (the name its cookies and storage are kept under, apart from any other's, 1 to 32 of
-- a-z, 0-9 and -; none, and they are gone with the browser; read when the browser is first made),
-- `windows` ("here": a link that opens a new window opens in the browser itself, the default; "page":
-- the host opens none and tells the page, `browser_window`) and `keys` (the keys that go to the page
-- as `key` events while the browser has the keyboard, written as a command's are: { "mod+l" }; the
-- rest go to the browser first, and to the page as `key` events only where it has no use for them).
-- The page is told where the browser is and what it is doing by `browser`
-- events and reads nothing of the page the browser shows. What the page writes inside the room is
-- drawn over the browser.
function Element:browser(id, options)
	if id ~= nil then
		self.meta.id = id
	end
	if self.meta.id == nil then
		refuse("a browser is named: browser(id, options)")
	end
	if options ~= nil and type(options) ~= "table" then
		refuse("a browser's options are a table, not " .. type(options))
	end
	local said = {}
	for key, value in options or {} do
		if key == "profile" then
			if
				type(value) ~= "string"
				or #value < 1
				or #value > 32
				or string.find(value, "[^a-z0-9%-]") ~= nil
			then
				refuse("a browser's profile is 1 to 32 of a to z, 0 to 9 and -, not " .. tostring(value))
			end
		elseif key == "windows" then
			if value ~= "here" and value ~= "page" then
				refuse('a browser\'s windows are "here" or "page", not ' .. tostring(value))
			end
		elseif key == "keys" then
			if type(value) ~= "table" then
				refuse("a browser's keys are a list of keys written as words: { \"mod+l\" }")
			end
			if #value > 32 then
				refuse("a browser keeps at most 32 keys for the page")
			end
			for _, words in value do
				-- Said where it is written, not where the page is read: a key that is not one is the
				-- page's mistake.
				pencere.keys(words)
			end
		else
			refuse("a browser has no option " .. tostring(key) .. " (it has profile, windows and keys)")
		end
		said[key] = value
	end
	self.meta.browser = said
	self.style.overflow = "hidden"
	return self
end

-- A browser named `id`, as `pencere.div():browser(id, options)`.
function pencere.browser(id, options)
	return make("box"):browser(id, options)
end

-- The element is the content of the zone `id` of the editor it is a child of: a gap the editor
-- keeps open after a line (a decoration `{ from =, to =, zone = id, height = points }`), in whole
-- rows. It is laid out as a box as wide as the editor, as tall as the page says (`:height(...)`, the
-- same as the zone's), and put in the gap, where it scrolls with the text; it is on the page only
-- while the editor has such a gap. `pencere.editor("code"):child(pencere.div():id("peek"):zone("peek"):height(120))`.
function Element:zone(id)
	if id == nil then
		refuse("zone takes the name of the zone")
	end
	self.meta.zone = id
	self.style.absolute = true
	self.style.inset = { top = 0, right = 0, left = 0 }
	return self
end

-- What the element is in words, for a person who does not see it (a screen reader says it): a
-- button that is an icon alone says what it does.
function Element:label(words)
	if type(words) ~= "string" then
		refuse("label takes the words, not " .. type(words))
	end
	self.meta.label = words
	return self
end

local EASES = { linear = true, out = true, ["in-out"] = true, spring = true }

-- How a change of this element moves instead of jumping; the host does the moving, frame by
-- frame, without asking the page again. `motion` is a table:
--   seconds   how long a change takes (0.2)
--   ease      "out" (the default: fast, then settling), "linear", "in-out" or "spring"
--   color     the colours of its fill, border, shadow and words follow a change
--   opacity   its opacity follows a change
--   size      a width or height given in points follows a change
--   place     moved among the others, it slides from where it was to where it is
--   enter     how it comes in when it appears: { opacity =, dx =, dy =, seconds = }
--   exit      how it goes when it is no longer on the page, the same
--   spin      it turns round, clockwise, a turn in this many seconds, while it is on the page: its
--             picture or its canvas about its middle (a spinner: `image("loader.svg"):motion({
--             spin = 1 })`); with no id too, and not under Reduce Motion
-- The element is followed by its id, so one with no id does not move. Nothing moves under Reduce
-- Motion, on the frame the page changes size, or for what scrolling brings in or takes out.
function Element:motion(motion)
	if type(motion) ~= "table" then
		refuse("motion takes a table, not " .. type(motion))
	end
	local seconds = motion.seconds or 0.2
	if motion.ease ~= nil and not EASES[motion.ease] then
		refuse('an ease is "out", "linear", "in-out" or "spring", not ' .. tostring(motion.ease))
	end
	if motion.spin ~= nil and not (type(motion.spin) == "number" and motion.spin > 0 and motion.spin < math.huge) then
		refuse("spin is the seconds of a turn, more than 0, not " .. tostring(motion.spin))
	end
	local function shift(given)
		if given == nil then
			return nil
		end
		return {
			opacity = given.opacity or 0,
			dx = given.dx or 0,
			dy = given.dy or 0,
			seconds = given.seconds or seconds,
		}
	end
	self.meta.motion = {
		seconds = seconds,
		ease = motion.ease,
		color = motion.color == true,
		opacity = motion.opacity == true,
		size = motion.size == true,
		place = motion.place == true,
		enter = shift(motion.enter),
		exit = shift(motion.exit),
		spin = motion.spin,
	}
	return self
end

-- What the host says of this element while the pointer rests on it, and the keys that do the same
-- (`"1"`, `"⌘K"`): the tooltip of a button that is an icon alone, the key of a section.
function Element:tip(words, keys)
	if type(words) ~= "string" then
		refuse("tip takes the words, not " .. type(words))
	end
	if keys ~= nil and type(keys) ~= "string" then
		refuse("the keys of a tip are words, not " .. type(keys))
	end
	self.meta.tip = { words = words, keys = keys }
	return self
end

local ROLES = {
	button = true, text = true, link = true, checkbox = true, switch = true, radio = true,
	tab = true, menu_item = true, heading = true, image = true, progress = true, slider = true,
}

-- What the element is to a screen reader, where its look says it and its answers do not:
-- "checkbox", "switch", "radio", "tab", "menu_item", "link", "heading" (with its level, 1 to 6),
-- "image", "progress", "slider", "button", "text".
function Element:role(role, level)
	if not ROLES[role] then
		refuse("a role is one of button, text, link, checkbox, switch, radio, tab, menu_item, "
			.. "heading, image, progress, slider; not " .. tostring(role))
	end
	self.meta.role = role
	self.meta.level = level
	return self
end

-- On or off: a checkbox, a switch, a toggle button.
function Element:checked(...) self.meta.checked = told(...) return self end
-- The one chosen of several: the tab in front, the row picked.
function Element:selected(...) self.meta.selected = told(...) return self end
-- Open or shut: a group that opens, the button of a menu.
function Element:expanded(...) self.meta.expanded = told(...) return self end
-- Of no use now: it does not answer the pointer, nor take the keyboard; a screen reader says so.
function Element:disabled(...) self.meta.disabled = told(...) return self end

-- The value the element shows, in words, for a screen reader: "40%".
function Element:value(words)
	if type(words) ~= "string" and type(words) ~= "number" then
		refuse("value takes the words, not " .. type(words))
	end
	self.meta.value = tostring(words)
	return self
end

local MODIFIERS = {
	cmd = "command",
	ctrl = "control",
	alt = "alt",
	shift = "shift",
}

-- The keys that are not a character, by the names a key event gives them. A key is also one
-- character, "f1" to "f20", or "space" (the character " ").
local KEY_NAMES = {
	up = true, down = true, left = true, right = true, page_up = true, page_down = true, home = true,
	["end"] = true, enter = true, escape = true, tab = true, backspace = true, delete = true,
}

-- Keys as a person writes them, "cmd+shift+k", "ctrl+enter", "f5", as { key =, mods = }. The key
-- `+` is written as itself: "cmd++". `mod` is the main modifier of the host's system
-- (`pencere.keyboard()`): "mod+s" is ⌘S on a Mac and Ctrl+S elsewhere.
function pencere.keys(words)
	if type(words) ~= "string" then
		refuse("keys are written as words: \"cmd+k\", not " .. type(words))
	end
	local parts = string.split(words, "+")
	-- A `+` at the end is the key, not the mark before it: "cmd++" splits to "cmd", "", "".
	if #parts >= 2 and parts[#parts] == "" and parts[#parts - 1] == "" then
		parts[#parts] = nil
		parts[#parts] = "+"
	end
	local mods = { shift = false, control = false, alt = false, command = false }
	for index = 1, #parts - 1 do
		local held = MODIFIERS[string.lower(parts[index])]
		if string.lower(parts[index]) == "mod" then
			held = pencere.keyboard() == "mac" and "command" or "control"
		end
		if not held then
			refuse("\"" .. parts[index] .. "\" is not a modifier (cmd, ctrl, alt, shift or mod)")
		end
		mods[held] = true
	end
	local key = parts[#parts] or ""
	if utf8.len(key) ~= 1 then
		key = string.lower(key)
		if key == "space" then
			key = " "
		elseif key ~= "" and not KEY_NAMES[key] then
			local number = tonumber(string.match(key, "^f(%d+)$"))
			if number == nil or number < 1 or number > 20 or string.match(key, "^f0") then
				refuse(
					'"' .. key .. '" is not a key: one character, or up, down, left, right, page_up, page_down, '
						.. "home, end, enter, escape, tab, backspace, delete, space or f1 to f20"
				)
			end
		end
	end
	return { key = key, mods = mods }
end

-- The keys that were read, by the words they were written as. A page lists its commands again at
-- every frame, in the same words, and `mod` means the same all the while (`pencere.keyboard`), so the
-- words are read once. `pencere.keys` hands its caller a table of its own; the commands share these,
-- so they are frozen. A page that writes words without end does not fill the module: past
-- `READ_KEYS_AT` they are forgotten.
local READ_KEYS_AT = 512
local read_keys = {}
local read_keys_count = 0
local function keys_read(words)
	local keys = read_keys[words]
	if keys == nil then
		keys = pencere.keys(words)
		table.freeze(keys.mods)
		table.freeze(keys)
		if read_keys_count >= READ_KEYS_AT then
			read_keys = {}
			read_keys_count = 0
		end
		read_keys[words] = keys
		read_keys_count += 1
	end
	return keys
end

-- Something the page can be told to do while the keyboard is on this element or inside it (on the
-- page's outermost box: always), which the host lists and does by its keys; the page is told
-- { type = "command", id = id }. `keys` is written as a person writes them ("cmd+k"), a list of
-- such (each does it; the first is the one a list shows: { "ctrl+page_down", "ctrl+alt+right" }),
-- or nil; `enabled` is false for one that cannot be done now.
-- The keys of a command: words, or a list of words, as a list of { key =, mods = }.
local function keys_of(keys)
	if keys == nil then
		return nil
	elseif type(keys) == "string" then
		return { keys_read(keys) }
	elseif type(keys) ~= "table" then
		refuse("the keys of a command are words, \"cmd+k\", or a list of them, not " .. type(keys))
	end
	local list = {}
	for index, words in ipairs(keys) do
		if type(words) ~= "string" then
			refuse("the keys of a command are words, \"cmd+k\", or a list of them; key " .. index .. " is a " .. type(words))
		end
		list[index] = keys_read(words)
	end
	return list
end

function Element:command(id, title, keys, enabled)
	if id == nil then
		refuse("a command is named: command(id, title, keys)")
	end
	if type(title) ~= "string" then
		refuse("a command has a title, the words a person reads for it")
	end
	local commands = self.meta.commands
	if not commands then
		commands = {}
		self.meta.commands = commands
	end
	commands[#commands + 1] = {
		id = id,
		title = title,
		keys = keys_of(keys),
		enabled = enabled ~= false,
	}
	return self
end

-- The keyboard is on this element: its commands and those of the boxes around it can be done.
function Element:focused(...)
	self.meta.focused = told(...)
	return self
end

-- Names the element and makes it something that can be picked up and moved (`Ui:dragging`).
function Element:draggable(id)
	self.meta.id = id
	self.meta.draggable = true
	return self
end

-- What a drag out of the view may take (the rule of the kit's specification, Section 2.1): one line,
-- never empty, at most 64 KiB, no control character (the C0 ones, the delete character, the C1
-- ones), no separator of lines or paragraphs, no control of the direction of text (the embeddings,
-- overrides and isolates that make words look other than they are); an address only of the kind a
-- `link` may be. The
-- host holds a page to the same rule when it takes the frame.
local function carriable(words)
	return type(words) == "string"
		and #words >= 1
		and #words <= 65536
		and not string.find(words, "%c")
		and not string.find(words, "\xC2[\x80-\x9F]")
		and not string.find(words, "\xE2\x80[\xA8\xA9]")
		and not string.find(words, "\xE2\x80[\xAA-\xAE]")
		and not string.find(words, "\xE2\x81[\xA6-\xA9]")
		and not string.find(words, "\xEF\xBB\xBF")
end

local function carriable_url(url)
	return type(url) == "string"
		and #url <= 8192
		and string.find(url, "^[Hh][Tt][Tt][Pp][Ss]?://") ~= nil
		and not string.find(url, "[^\x21-\x7E]")
end

-- Makes the element something a person drags out of the view, to a terminal pane or another
-- application: pressed on and moved past `pencere.DRAG_START` points, the host's system drag
-- begins with what it carries, `{ text = "words" }` or `{ path = "/a/file" }` (a terminal quotes a
-- path for its shell), and `url = "https://..."` for the applications that take an address. The
-- element has a name already (`:id`, or `:draggable(id)` first); one that is also `draggable` is the
-- page's own drag while the pointer is in the view and the system's from the moment it leaves. The
-- page does not hear the pointer move or let go while the system carries: it is told `carried`
-- (`{ type = "carried", id =, done = }`) when that is over, and its `Ui` lets go of what it held.
function Element:carry(spec)
	if type(spec) ~= "table" then
		refuse("carry takes { text = | path =, url = }, not a " .. type(spec))
	end
	for key in pairs(spec) do
		if key ~= "text" and key ~= "path" and key ~= "url" then
			refuse("carry has no " .. tostring(key) .. " (text, path, url)")
		end
	end
	if (spec.text ~= nil) == (spec.path ~= nil) then
		refuse("carry says its text or its path, one of them")
	end
	if self.meta.id == nil then
		refuse("an element that carries is a box with a name: give it one (:id) before :carry")
	end
	if not carriable(spec.text or spec.path) then
		refuse("a carry is one line (a string of at least 1 byte and at most 65536, with no line break and no control character)")
	end
	if spec.url ~= nil and not carriable_url(spec.url) then
		refuse("the url of a carry is an http or https address of printable ASCII without spaces, at most 8192 bytes")
	end
	self.meta.carry = { text = spec.text, path = spec.path, url = spec.url }
	return self
end

-- Names the element and makes it a place what is dragged can be put down on.
function Element:drop_target(id)
	self.meta.id = id
	self.meta.drop_target = true
	return self
end

-- --- Layout: the box --------------------------------------------------------------------------

function Element:row()
	local style = self.style
	style.display, style.direction = "flex", "row"
	return self
end

function Element:col()
	local style = self.style
	style.display, style.direction = "flex", "column"
	return self
end

function Element:wrap(...)
	self.style.wrap = told(...)
	return self
end

-- A grid of `columns`: numbers are points, `pencere.fr(n)`, `pencere.range(least, n)`, "auto".
function Element:grid(columns)
	if columns ~= nil and type(columns) ~= "table" then
		refuse("grid takes a list of tracks (numbers, pencere.fr(n), pencere.range(least, n) or \"auto\"), not a " .. type(columns))
	end
	local style = self.style
	style.display = "grid"
	style.columns = columns
	return self
end

function Element:grid_rows(rows)
	if rows ~= nil and type(rows) ~= "table" then
		refuse("grid_rows takes a list of tracks (numbers, pencere.fr(n), pencere.range(least, n) or \"auto\"), not a " .. type(rows))
	end
	self.style.rows = rows
	return self
end

function Element:span(columns)
	self.style.column_span = columns
	return self
end

-- A flag of its own and not a `display`, so that what `row`, `col` and `grid` said stands, whichever
-- was said first.
function Element:hidden(...)
	self.style.hidden = told(...)
	return self
end

function Element:items_start() self.style.items = "start" return self end
function Element:items_center() self.style.items = "center" return self end
function Element:items_end() self.style.items = "end" return self end
function Element:items_stretch() self.style.items = "stretch" return self end
function Element:justify_start() self.style.justify = "start" return self end
function Element:justify_center() self.style.justify = "center" return self end
function Element:justify_end() self.style.justify = "end" return self end
function Element:justify_between() self.style.justify = "between" return self end

function Element:gap(points)
	local style = self.style
	style.gap_x, style.gap_y = points, points
	return self
end

function Element:gap_x(points) self.style.gap_x = points return self end
function Element:gap_y(points) self.style.gap_y = points return self end

-- --- Layout: the element in its parent --------------------------------------------------------

function Element:grow(share) self.style.grow = share return self end
function Element:no_shrink(...)
	local style = self.style
	if told(...) then
		style.shrink = 0
	elseif style.shrink == 0 then
		style.shrink = nil
	end
	return self
end
function Element:w_full() self.style.width = { percent = 100 } return self end
function Element:h_full() self.style.height = { percent = 100 } return self end

function Element:size_full()
	return self:w_full():h_full()
end

function Element:w(points) self.style.width = points return self end
function Element:w_pct(percent) self.style.width = { percent = percent } return self end
function Element:h(points) self.style.height = points return self end

function Element:size(width, height)
	return self:w(width):h(height)
end

function Element:min_w(points) self.style.min_width = points return self end
function Element:min_h(points) self.style.min_height = points return self end
function Element:max_w(points) self.style.max_width = points return self end
function Element:max_h(points) self.style.max_height = points return self end

-- Padding and margin are four numbers in the order top, right, bottom, left.
local function edges(style, name)
	local value = style[name]
	if not value then
		value = { 0, 0, 0, 0 }
		style[name] = value
	end
	return value
end

function Element:p(points)
	self.style.padding = { points, points, points, points }
	return self
end

function Element:px(points)
	local padding = edges(self.style, "padding")
	padding[2], padding[4] = points, points
	return self
end

function Element:py(points)
	local padding = edges(self.style, "padding")
	padding[1], padding[3] = points, points
	return self
end

function Element:pt(points) edges(self.style, "padding")[1] = points return self end
function Element:pr(points) edges(self.style, "padding")[2] = points return self end
function Element:pb(points) edges(self.style, "padding")[3] = points return self end
function Element:pl(points) edges(self.style, "padding")[4] = points return self end
function Element:mt(points) edges(self.style, "margin")[1] = points return self end
function Element:mr(points) edges(self.style, "margin")[2] = points return self end
function Element:mb(points) edges(self.style, "margin")[3] = points return self end
function Element:ml(points) edges(self.style, "margin")[4] = points return self end

-- Takes the box out of the flow; `inset` places it against the parent's box.
function Element:absolute(...)
	self.style.absolute = told(...)
	return self
end

-- The distances of an absolute box's edges from its parent's (nil leaves an edge free).
function Element:inset(top, right, bottom, left)
	self.style.inset = { top = top, right = right, bottom = bottom, left = left }
	return self
end

-- Takes the box out of the flow and out of every box around it: placed against the page by
-- `inset` (`:inset(0, 0, 0, 0)` is the whole page, the ground of a dialog) or beside an element
-- (`:anchor`), drawn over everything else, cut by nothing that holds it, and the first the pointer
-- finds. It is as faint as what holds it and hidden with it, and its keys go out through it.
function Element:overlay(...)
	self.style.overlay = told(...)
	return self
end

local SIDES = { below = true, above = true, right = true, left = true }
local ALIGNS = { start = true, center = true, ["end"] = true }

-- An overlay beside the element named `to`, on `side` of it ("below", "above", "right" or
-- "left"; the other where the page has not the room), `options.gap` points away, lined up with
-- its "start", "center" or "end" (`options.align`), and kept on the page.
function Element:anchor(to, side, options)
	side = side or "below"
	if not SIDES[side] then
		refuse('an overlay stands "below", "above", "right" or "left" of its element, not ' .. tostring(side))
	end
	options = options or {}
	local align = options.align or "start"
	if not ALIGNS[align] then
		refuse('an overlay lines up with the "start", "center" or "end" of its element, not ' .. tostring(align))
	end
	local style = self.style
	style.overlay = true
	style.anchor = { to = to, side = side, align = align, gap = options.gap or 0 }
	return self
end

-- Nothing is drawn outside the box.
function Element:clip(...)
	local style = self.style
	if told(...) then
		style.overflow = "hidden"
	elseif style.overflow == "hidden" then
		style.overflow = nil
	end
	return self
end

-- A box whose content can be bigger than itself and is scrolled behind it, down and across. How
-- far is the host's, kept by the box's name (`:id`; one with none does not scroll): the wheel, the
-- trackpad and the scroll bars move it as the host's system does (`pencere.layout.scroll_offset`,
-- `scroll_to`, and the `scrolled` event).
function Element:scroll(...)
	local style = self.style
	if told(...) then
		style.overflow = "scroll"
	elseif style.overflow == "scroll" then
		style.overflow = nil
	end
	return self
end

-- A box that scrolls (`:scroll()`) that the host lets the person zoom: pinched, by the command key
-- with the wheel, by a double tap with two fingers. What is inside is laid out larger as it is
-- zoomed (a text in a larger face, a picture in a larger size), the box stays the size it was
-- given, and the page is told (`scrolled`, with `zoom`; `pencere.layout.zoom_of`, `zoom_to`).
-- `{ min =, max =, smart =, wheel = }`: the smallest and the largest scale (1 and 4), the scale a
-- double tap goes to from 1 (2) and how far one point of the wheel zooms (0.008). Without a
-- table it is the same as `{}`; `false` takes the zoom off. An editor does not go inside one.
function Element:zoom(rule)
	if rule == false then
		self.style.zoom = nil
		return self
	end
	if rule == nil or rule == true then
		rule = {}
	end
	if type(rule) ~= "table" then
		refuse("zoom takes { min =, max =, smart =, wheel = }, not a " .. type(rule))
	end
	if self.listed then
		-- The rows of a long list stand where the page's own points put them (see `Element:list`).
		refuse("a long list does not zoom: the box that zooms holds it (or builds its pages itself)")
	end
	for key, value in pairs(rule) do
		if key ~= "min" and key ~= "max" and key ~= "smart" and key ~= "wheel" then
			refuse("zoom has no " .. tostring(key) .. " (min, max, smart, wheel)")
		end
		if type(value) ~= "number" then
			refuse("zoom's " .. key .. " is a number, not a " .. type(value))
		end
		-- A scale is more than nothing and not unbounded; the wheel's is nothing or more.
		if value ~= value or value == math.huge or value == -math.huge or value < 0 or (value == 0 and key ~= "wheel") then
			refuse("zoom's " .. key .. " is a finite number of times the size" .. (key == "wheel" and " (0 or more)" or " (more than 0)"))
		end
	end
	if (rule.min or 1) > (rule.max or 4) then
		refuse("zoom's min is more than its max (a range of scales, 1 to 4 where it is not said)")
	end
	self.style.zoom = { min = rule.min, max = rule.max, smart = rule.smart, wheel = rule.wheel }
	return self
end

-- --- Looks ------------------------------------------------------------------------------------

function Element:bg(color) self.style.fill = color return self end

-- A fill that runs from `from` to `to` along a line at `angle` degrees, as a style sheet's
-- linear-gradient: 180 from the top down, 90 from the left to the right.
function Element:bg_gradient(angle, from, to)
	if type(angle) ~= "number" then
		refuse("bg_gradient takes an angle in degrees, then two colours")
	end
	self.style.gradient = { angle = angle, from = from, to = to }
	return self
end

function Element:border(width, color)
	self.style.border = { width = width, color = color }
	return self
end

function Element:rounded(points)
	self.style.radii = { points, points, points, points }
	return self
end

-- A radius per corner: top-left, top-right, bottom-right, bottom-left.
function Element:rounded_corners(radii)
	self.style.radii = radii
	return self
end

function Element:shadow(dx, dy, blur, color)
	self.style.shadow = { dx = dx, dy = dy, blur = blur, color = color }
	return self
end

-- --- Text (set here, inherited by the elements inside) ----------------------------------------

local function text_style(element)
	local style = element.style
	local text = style.text
	if not text then
		text = {}
		style.text = text
	end
	return text
end

function Element:text_size(points) text_style(self).size = points return self end
function Element:weight(weight) text_style(self).weight = weight return self end
function Element:color(color) text_style(self).color = color return self end
-- "system" or "terminal".
function Element:family(family) text_style(self).family = family return self end
-- "start", "center" or "end".
function Element:align(align) text_style(self).align = align return self end
-- The face's italic form; a line under the words; a line through them.
function Element:italic(...) text_style(self).italic = told(...) return self end
function Element:underline(...) text_style(self).underline = told(...) return self end
function Element:strike(...) text_style(self).strike = told(...) return self end

function Element:truncate(...)
	self.style.truncate = told(...)
	return self
end

-- The largest value a sparkline's top stands for.
function Element:peak(value)
	if self.kind == "spark" then
		self.ceiling = value
	end
	return self
end
