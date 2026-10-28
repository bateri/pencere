-- The page API, part 2: elements and the chain that writes them,
-- `pencere.div():row():gap(8):child(pencere.text("hello"))`.
--
-- An element is a table: { kind = "box" | "text" | "rich" | "spark" | "image" | "canvas",
-- style = {...}, meta = {...}, kids = {...} }.
-- The host reads it, so the field names are the interface's (`pencere:view` `style`, `node`).

local Element = {}
Element.__index = Element
pencere.Element = Element

local function make(kind)
	return setmetatable({ kind = kind, style = {}, meta = {}, kids = {} }, Element)
end

-- A helper that finds its argument wrong raises the error at the line of the script that called
-- it, not at a line of this prelude: the person who reads it has the script. Three levels up from
-- here are `refuse`, the helper and then the helper's caller.
local function refuse(message)
	error(message, 3)
end

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
-- The argument is the path of the file as the bundle's manifest names it (PNG, JPEG or SVG). Its
-- size is the element's, as for any box (`:w(24)`, `:h(24)`); a side that is not said follows the
-- picture's proportions, and an element that says neither is the picture's own size in pixels, a
-- point to a pixel: a picture meant to be sharp on a screen with two pixels to the point is
-- sent at twice the size it is given. `:fit` is "contain" (all of the picture, centred; the
-- default) or "cover" (the box filled, what does not fit cut off). `:tint` is a colour (any way
-- of saying one, as for text): the picture is then a shape, drawn in that colour with its own
-- opacity at each pixel. How much of it shows is the element's `:opacity`, as for any element.
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
-- `method` (two levels up from here are the method and then its caller).
local function picture_of(element, method)
	if element.kind ~= "image" then
		error(method .. " is for an image, and this is a " .. element.kind, 3)
	end
	return element.picture
end

-- How the picture fills the box: "contain" or "cover".
function Element:fit(how)
	local picture = picture_of(self, "fit")
	if how ~= "contain" and how ~= "cover" then
		error('fit is "contain" or "cover", not ' .. tostring(how), 2)
	end
	picture.fit = how
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
		error("opacity is a number from 0 to 1, not " .. type(opacity), 2)
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
function Span:bold() self.look.weight = 700 return self end
function Span:italic() self.look.italic = true return self end
function Span:underline() self.look.underline = true return self end
function Span:strike() self.look.strike = true return self end
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
local Path = {}
Path.__index = Path
pencere.Path = Path

function pencere.path()
	-- How it is drawn lives in a table of its own: `fill` and `stroke` are the methods' names too.
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

-- The colour inside the path; and the line along it, `width` points, round where it turns and ends.
function Path:fill(color) self.paint.fill = color return self end

function Path:stroke(width, color)
	self.paint.stroke = { width = width, color = color }
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
		plain[index] = { segments = path.segments, fill = path.paint.fill, stroke = path.paint.stroke }
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
function Element:focusable()
	self.meta.focusable = true
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
		error("a text field is named: text_field(id)", 2)
	end
	kind = kind or "line"
	if not FIELD_KINDS[kind] then
		error('a text field is "line", "lines" or "secret", not ' .. tostring(kind), 2)
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
-- `read_only`, `keys` ("editor", or "page": every key the page's), `hint` (what it says while
-- empty), `pairs` (the brackets and quotes that close themselves: "()[]{}" or { "()", "[]" }); what
-- its language says, all of it left out for prose: `comment` (what the commands that comment put: a
-- line comment, "--", or { line = "--", block = { "--[[", "]]" } }, either of them, the block a pair
-- or { open =, close = }), `indent = { increase =, decrease = }` (regular expressions, Rust's: a
-- line break after a line that matches `increase` indents a level more, and a line that what is
-- typed makes match `decrease` goes a level out; one that is no pattern fails the page) and `folds`
-- ("page", the page's fold decorations alone, or "indent", the editor's from the indent as well);
-- how it looks: `line_height` (1.6
-- times the font's size), `padding` (4 points), `scroll_margin` (rows kept about the caret, 0),
-- `numbers` ("absolute", "relative", "hidden"), `current_line` (where the caret's line is drawn:
-- "text", "gutter", "all"), `whitespace` (which spaces and tabs it shows, as VS Code: "none",
-- "boundary" (all but a space alone between words), "selection", "trailing" or "all"; a space is a
-- dot, a tab an arrow, any other space a box), `indent_guides` (a thin line down each level of
-- indent, the caret's block's in its own colour), `matches` (the other places of the main selection's
-- text marked where they are in view: "none", "selection" (or true), or "word", the caret's word
-- too), `caret = { shape = "bar" | "block" | "underline", height = "row" | "text", width = 2, color =,
-- text = }` and `colors = { current_line, selection, selection_blurred, found, brackets, numbers,
-- current_number, gutter, folds, folded, hint, whitespace, indent_guides, active_indent_guide,
-- matches }`. A colour it does not say is the theme's;
-- `false` draws none. A plain editor shows no spaces and no guides.
local EDITOR_OPTIONS = {
	wrap = true,
	tab_width = true,
	hard_tabs = true,
	gutter = true,
	read_only = true,
	keys = true,
	hint = true,
	comment = { line = true, block = true },
	pairs = true,
	indent = { increase = true, decrease = true },
	folds = true,
	line_height = true,
	padding = true,
	scroll_margin = true,
	numbers = true,
	current_line = true,
	whitespace = true,
	indent_guides = true,
	matches = true,
	caret = { shape = true, height = true, width = true, color = true, text = true },
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
	},
}

-- An option an editor has no such name for is a mistake, said where it is made: a misspelt one
-- would be dropped without a word.
local function check_editor_options(given, known, where, level)
	for key, value in given do
		local takes = known[key]
		if takes == nil then
			error("an editor has no option " .. where .. tostring(key), level)
		end
		if type(takes) == "table" and type(value) == "table" then
			check_editor_options(value, takes, where .. key .. ".", level + 1)
		end
	end
end

function Element:editor(id, options)
	if id ~= nil then
		self.meta.id = id
	end
	if self.meta.id == nil then
		error("an editor is named: editor(id, options)", 2)
	end
	if options ~= nil and type(options) ~= "table" then
		error("an editor's options are a table, not " .. type(options), 2)
	end
	local given = options or {}
	check_editor_options(given, EDITOR_OPTIONS, "", 3)
	local said = {}
	for key, value in given do
		said[key] = value
	end
	if type(said.pairs) == "table" then
		said.pairs = table.concat(said.pairs)
	end
	self.meta.editor = said
	self.style.overflow = "scroll"
	return self
end

-- An editor named `id`, as `pencere.div():editor(id, options)`.
function pencere.editor(id, options)
	return make("box"):editor(id, options)
end

-- What the element is in words, for a person who does not see it (a screen reader says it): a
-- button that is an icon alone says what it does.
function Element:label(words)
	if type(words) ~= "string" then
		error("label takes the words, not " .. type(words), 2)
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
-- The element is followed by its id, so one with no id does not move. Nothing moves under Reduce
-- Motion, on the frame the page changes size, or for what scrolling brings in or takes out.
function Element:motion(motion)
	if type(motion) ~= "table" then
		error("motion takes a table, not " .. type(motion), 2)
	end
	local seconds = motion.seconds or 0.2
	if motion.ease ~= nil and not EASES[motion.ease] then
		error('an ease is "out", "linear", "in-out" or "spring", not ' .. tostring(motion.ease), 2)
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
	}
	return self
end

-- What the host says of this element while the pointer rests on it, and the keys that do the same
-- (`"1"`, `"⌘K"`): the tooltip of a button that is an icon alone, the key of a section.
function Element:tip(words, keys)
	if type(words) ~= "string" then
		error("tip takes the words, not " .. type(words), 2)
	end
	if keys ~= nil and type(keys) ~= "string" then
		error("the keys of a tip are words, not " .. type(keys), 2)
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
		error("a role is one of button, text, link, checkbox, switch, radio, tab, menu_item, "
			.. "heading, image, progress, slider; not " .. tostring(role), 2)
	end
	self.meta.role = role
	self.meta.level = level
	return self
end

-- On or off: a checkbox, a switch, a toggle button.
function Element:checked(on) self.meta.checked = on == true return self end
-- The one chosen of several: the tab in front, the row picked.
function Element:selected(chosen) self.meta.selected = chosen ~= false return self end
-- Open or shut: a group that opens, the button of a menu.
function Element:expanded(open) self.meta.expanded = open == true return self end
-- Of no use now: it does not answer the pointer, nor take the keyboard; a screen reader says so.
function Element:disabled(disabled) self.meta.disabled = disabled ~= false return self end

-- The value the element shows, in words, for a screen reader: "40%".
function Element:value(words)
	if type(words) ~= "string" and type(words) ~= "number" then
		error("value takes the words, not " .. type(words), 2)
	end
	self.meta.value = tostring(words)
	return self
end

local MODIFIERS = {
	cmd = "command", command = "command", super = "command", meta = "command",
	ctrl = "control", control = "control",
	alt = "alt", option = "alt", opt = "alt",
	shift = "shift",
}

-- The names a key goes by besides the one the page is told.
local KEY_NAMES = {
	space = " ",
	["return"] = "enter",
	esc = "escape",
	pageup = "page_up",
	pagedown = "page_down",
	del = "delete",
}

-- Keys as a person writes them, "cmd+shift+k", "ctrl+enter", "f5", as { key =, mods = }. The key
-- `+` is written as itself: "cmd++".
function pencere.keys(words)
	if type(words) ~= "string" then
		error("keys are written as words: \"cmd+k\", not " .. type(words), 2)
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
		if not held then
			error("\"" .. parts[index] .. "\" is not a modifier (cmd, ctrl, alt or shift)", 2)
		end
		mods[held] = true
	end
	local key = parts[#parts] or ""
	if utf8.len(key) ~= 1 then
		key = string.lower(key)
		key = KEY_NAMES[key] or key
	end
	return { key = key, mods = mods }
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
		return { pencere.keys(keys) }
	elseif type(keys) ~= "table" then
		error("the keys of a command are words, \"cmd+k\", or a list of them, not " .. type(keys), 3)
	end
	local list = {}
	for index, words in ipairs(keys) do
		if type(words) ~= "string" then
			error("the keys of a command are words, \"cmd+k\", or a list of them; key " .. index .. " is a " .. type(words), 3)
		end
		list[index] = pencere.keys(words)
	end
	return list
end

function Element:command(id, title, keys, enabled)
	if id == nil then
		error("a command is named: command(id, title, keys)", 2)
	end
	if type(title) ~= "string" then
		error("a command has a title, the words a person reads for it", 2)
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
function Element:focused(focused)
	self.meta.focused = focused ~= false
	return self
end

-- Names the element and makes it something that can be picked up and moved (`Ui:dragging`).
function Element:draggable(id)
	self.meta.id = id
	self.meta.draggable = true
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

function Element:wrap()
	self.style.wrap = true
	return self
end

-- A grid of `columns`: numbers are points, `pencere.fr(n)`, `pencere.range(least, n)`, "auto".
function Element:grid(columns)
	local style = self.style
	style.display = "grid"
	style.columns = columns
	return self
end

function Element:grid_rows(rows)
	self.style.rows = rows
	return self
end

function Element:span(columns)
	self.style.column_span = columns
	return self
end

function Element:hidden()
	self.style.display = "none"
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
function Element:no_shrink() self.style.shrink = 0 return self end
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
function Element:absolute()
	self.style.absolute = true
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
function Element:overlay()
	self.style.overlay = true
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
		error('an overlay stands "below", "above", "right" or "left" of its element, not ' .. tostring(side), 2)
	end
	options = options or {}
	local align = options.align or "start"
	if not ALIGNS[align] then
		error('an overlay lines up with the "start", "center" or "end" of its element, not ' .. tostring(align), 2)
	end
	local style = self.style
	style.overlay = true
	style.anchor = { to = to, side = side, align = align, gap = options.gap or 0 }
	return self
end

-- Nothing is drawn outside the box.
function Element:clip()
	self.style.overflow = "hidden"
	return self
end

-- A box whose content can be bigger than itself and is scrolled behind it, down and across. How
-- far is the host's, kept by the box's name (`:id`; one with none does not scroll): the wheel, the
-- trackpad and the scroll bars move it as the host's system does (`pencere.layout.scroll_offset`,
-- `scroll_to`, and the `scrolled` event).
function Element:scroll()
	self.style.overflow = "scroll"
	return self
end

-- --- Looks ------------------------------------------------------------------------------------

function Element:bg(color) self.style.fill = color return self end

-- A fill that runs from `from` to `to` along a line at `angle` degrees, as a style sheet's
-- linear-gradient: 180 from the top down, 90 from the left to the right.
function Element:bg_gradient(angle, from, to)
	if type(angle) ~= "number" then
		error("bg_gradient takes an angle in degrees, then two colours", 2)
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
function Element:italic() text_style(self).italic = true return self end
function Element:underline() text_style(self).underline = true return self end
function Element:strike() text_style(self).strike = true return self end

function Element:truncate()
	self.style.truncate = true
	return self
end

-- The largest value a sparkline's top stands for.
function Element:peak(value)
	if self.kind == "spark" then
		self.ceiling = value
	end
	return self
end
