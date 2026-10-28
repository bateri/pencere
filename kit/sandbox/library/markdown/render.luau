-- How `@pencere/markdown` draws a document's blocks as elements of the page: a paragraph is a text
-- of many looks that wraps, a heading one larger and bold, a list a column of rows (a marker and
-- the item's blocks), a quote its blocks beside a bar, a table a grid, a block of code a box of the
-- terminal's font (coloured by `@pencere/languages` where it is there), a rule a line. The colours
-- are roles of the theme; what the page does not say is inherited from the box it puts them in.
--
-- A draw is held to a number of elements (`limit`) and of bytes, so that a long document cannot take
-- a frame past what a frame may hold (20 000 elements): what is past it is left out, and an "…"
-- says so. The links it draws are named (`<id>/link/<n>`) and remembered by that name, so that a
-- press on one can be answered (`render.answer`).

local byte, find, sub = string.byte, string.find, string.sub
local concat = table.concat

local render = {}

local HEADINGS = { 22, 18, 16, 14.5, 13.5, 13 }
local COLORS = {
	text = false,
	quiet = "dim",
	link = "accent",
	code = "raised2",
	head = "raised1",
	quote = "border",
	rule = "border",
}
local OPTIONS = {
	id = true,
	headings = true,
	colors = true,
	gap = true,
	size = true,
	link = true,
	code = true,
	image = true,
	languages = true,
	limit = true,
}
-- The most elements one draw makes unless the page says (`limit`): half of what a frame may hold,
-- the rest the page's own.
local LIMIT = 10000
-- The most bytes of words one draw hands out: a frame may hold 4 MiB.
local BYTES = 1048576
-- How much code one draw colours, bytes: a language of a few patterns a token reads some 20 000
-- bytes in 30 ms on a fast machine. Code past it is drawn plain, and coloured in a draw after.
local COLOURED = 20000
-- The most bytes a part of a text holds (a part may hold 64 KiB), and the most parts a text holds.
local PART_BYTES = 32768
local PARTS = 1000
-- The parts a text may hold at all (the kit's SPEC, Section 4): the words past `PARTS` go in parts
-- of PART_BYTES up to it.
local MOST_PARTS = 1024
-- How many lines of code one text of many looks holds.
local CODE_LINES = 200
-- How deep blocks are drawn inside one another: a tree of a page is at most 128 deep.
local DEPTH = 24

local BULLETS = { "•", "◦", "▪" }
local ALIGN = { left = "start", center = "center", right = "end" }

-- --- Options ------------------------------------------------------------------------------------

-- The options of a draw, checked and with what they leave out filled in. `level` is where an error
-- is said (the caller of the module's function).
function render.options(given, level)
	level += 1
	if given ~= nil and type(given) ~= "table" then
		error("the options of markdown are a table, not a " .. type(given), level)
	end
	given = given or {}
	for key in pairs(given) do
		if not OPTIONS[key] then
			error(
				"markdown has no option "
					.. tostring(key)
					.. " (id, headings, colors, gap, size, link, code, image, languages, limit)",
				level
			)
		end
	end
	for _, key in ipairs({ "link", "code", "image" }) do
		if given[key] ~= nil and type(given[key]) ~= "function" then
			error("the option " .. key .. " of markdown is a function, not a " .. type(given[key]), level)
		end
	end
	local colors = table.clone(COLORS)
	if given.colors ~= nil then
		if type(given.colors) ~= "table" then
			error("the colors of markdown are a table of colours by what they colour", level)
		end
		for key, value in pairs(given.colors) do
			if COLORS[key] == nil then
				error("markdown colours no " .. tostring(key) .. " (text, quiet, link, code, head, quote, rule)", level)
			end
			colors[key] = value
		end
	end
	local headings = HEADINGS
	if given.headings ~= nil then
		if type(given.headings) ~= "table" then
			error("the headings of markdown are the sizes of its six levels, a list of numbers", level)
		end
		headings = table.clone(HEADINGS)
		for index, size in pairs(given.headings) do
			if type(index) ~= "number" or index < 1 or index > 6 or type(size) ~= "number" then
				error("the headings of markdown are the sizes of its six levels, a list of numbers", level)
			end
			headings[index] = size
		end
	end
	for _, key in ipairs({ "gap", "size", "limit" }) do
		if given[key] ~= nil and type(given[key]) ~= "number" then
			error("the option " .. key .. " of markdown is a number, not a " .. type(given[key]), level)
		end
	end
	local id = given.id
	if id ~= nil and type(id) ~= "string" then
		error("the id of markdown is a string, the start of the names of its elements", level)
	end
	return {
		id = id,
		headings = headings,
		colors = colors,
		gap = given.gap or 12,
		size = given.size,
		link = given.link,
		code = given.code,
		image = given.image,
		languages = given.languages,
		limit = given.limit or LIMIT,
	}
end

-- Whether two options draw the same elements (what a press answers is not drawn).
function render.same(a, b)
	if a == nil or b == nil then
		return false
	end
	for _, key in ipairs({ "id", "gap", "size", "code", "image", "languages", "limit" }) do
		if not rawequal(a[key], b[key]) then
			return false
		end
	end
	for index = 1, 6 do
		if a.headings[index] ~= b.headings[index] then
			return false
		end
	end
	for key in pairs(COLORS) do
		if not rawequal(a.colors[key], b.colors[key]) then
			return false
		end
	end
	return true
end

-- --- Links --------------------------------------------------------------------------------------

-- The links drawn, by the start of their names: { hrefs = { [n] = href }, next =, answer = }.
local owners = {}

-- The owner of the links named `<id>/link/<n>`: made anew (its links forgotten) where `fresh`.
function render.owner(id, fresh)
	local owner = owners[id]
	if owner == nil or fresh then
		owner = { hrefs = {}, next = 1, answer = nil }
		owners[id] = owner
	end
	return owner
end

-- What a press on a link of this module answers by default: the address opened in the person's
-- browser, where it is one a host opens (the web's).
local function open(href)
	if find(href, "^[Hh][Tt][Tt][Pp][Ss]?://") then
		return { link = href }
	end
	return nil
end

-- The address of the link named `id`, and the owner that drew it; nil for a name that is no link.
function render.href(id)
	if type(id) ~= "string" then
		return nil
	end
	local prefix, number = string.match(id, "^(.*)/link/(%d+)$")
	local owner = prefix and owners[prefix]
	local href = owner and owner.hrefs[tonumber(number)]
	if href == nil then
		return nil
	end
	return href, owner
end

-- What a press on the link named `id` answers: the page's `link` function's reply, or the default.
function render.answer(id)
	local href, owner = render.href(id)
	if href == nil then
		return nil
	end
	return (owner.answer or open)(href)
end

-- Forgets the links numbered `numbers` of `owner`.
function render.forget(owner, numbers)
	if numbers then
		for _, number in ipairs(numbers) do
			owner.hrefs[number] = nil
		end
	end
end

-- --- A draw -------------------------------------------------------------------------------------

-- A draw is { o = options, owner =, count =, bytes =, limit =, coloured =, provisional =, links = }:
-- the elements and bytes made so far, what may yet be coloured, whether some code was left plain for
-- want of it, and the numbers of the links made (for a page's kept document to forget them).
function render.draw(o, owner)
	return { o = o, owner = owner, count = 0, bytes = 0, limit = o.limit, coloured = COLOURED, provisional = false, links = nil }
end

local function full(draw)
	return draw.count >= draw.limit or draw.bytes >= BYTES
end
render.full = full

local function made(draw, count)
	draw.count += count or 1
end

-- What says that the rest is left out.
local function more(draw)
	made(draw)
	return pencere.text("…"):color(draw.o.colors.quiet)
end
render.more = more

-- --- Inlines ------------------------------------------------------------------------------------

-- A look is { color, bold, italic, strike, code, link } (a link's name), a table shared by the parts
-- it is given to, so that parts of one look next to one another are made one.
local function with(look, key, value)
	local new = table.clone(look)
	new[key] = value
	return new
end

-- The parts of a text being made: { looks, texts } (a text is a list of pieces), and the elements
-- made in it where images are elements of their own (`segments`).
local function parts_new()
	return { looks = {}, texts = {} }
end

local function put(parts, words, look)
	if words == "" then
		return
	end
	local count = #parts.looks
	if count > 0 and parts.looks[count] == look then
		local pieces = parts.texts[count]
		pieces[#pieces + 1] = words
	else
		parts.looks[count + 1] = look
		parts.texts[count + 1] = { words }
	end
end

-- The spans of the parts: a part too long for one is cut (at a character) into several, and the
-- parts past the most a text holds are put in the last one as plain text.
local function spans_of(parts, draw)
	local spans = {}
	local colors = draw.o.colors
	local function span(words, look)
		local made_span = pencere.span(words)
		draw.bytes += #words
		if look.color then
			made_span:color(look.color)
		end
		if look.bold then
			made_span:bold()
		end
		if look.italic then
			made_span:italic()
		end
		if look.strike then
			made_span:strike()
		end
		if look.code then
			made_span:family("terminal"):bg(colors.code)
		end
		if look.link then
			made_span:link(look.link):color(colors.link)
		end
		if look.weight then
			made_span:weight(look.weight)
		end
		if look.underline then
			made_span:underline()
		end
		if look.background then
			made_span:bg(look.background)
		end
		return made_span
	end
	for index, look in ipairs(parts.looks) do
		local words = concat(parts.texts[index])
		if #spans >= PARTS then
			local rest = {}
			for k = index, #parts.looks do
				rest[#rest + 1] = concat(parts.texts[k])
			end
			words = concat(rest)
			look = {}
		end
		local last = false
		while #words > PART_BYTES do
			local cut = PART_BYTES
			while cut > 1 do
				local b = byte(words, cut + 1)
				if b == nil or b < 0x80 or b >= 0xC0 then
					break
				end
				cut -= 1
			end
			if #spans >= MOST_PARTS - 1 then
				-- The last part a text may hold: what is past it is left out, and says so.
				words = sub(words, 1, cut) .. "…"
				last = true
				break
			end
			spans[#spans + 1] = span(sub(words, 1, cut), look)
			words = sub(words, cut + 1)
		end
		spans[#spans + 1] = span(words, look)
		if last or #spans > PARTS then
			break
		end
	end
	if #spans == 0 then
		spans[1] = pencere.span("")
	end
	return spans
end

-- The number of a new link of `href`, remembered by the draw's owner.
local function link_id(draw, href)
	local owner = draw.owner
	local number = owner.next
	owner.next += 1
	owner.hrefs[number] = href
	if draw.links then
		draw.links[#draw.links + 1] = number
	end
	return (draw.o.id or "markdown") .. "/link/" .. number
end

local paragraph_of

-- Puts the inlines `nodes` in the parts, in `look`. An image is its description (or, where the page
-- draws images, an element of its own: the parts so far are a text, and the image after it).
local function put_inlines(nodes, look, parts, draw, segments)
	if nodes == nil then
		return
	end
	local o = draw.o
	for _, node in ipairs(nodes) do
		local kind = node.type
		if kind == "text" then
			put(parts, node.text, look)
		elseif kind == "softbreak" then
			put(parts, " ", look)
		elseif kind == "hardbreak" then
			put(parts, "\n", look)
		elseif kind == "emphasis" then
			put_inlines(node.children, with(look, "italic", true), parts, draw, segments)
		elseif kind == "strong" then
			put_inlines(node.children, with(look, "bold", true), parts, draw, segments)
		elseif kind == "strike" then
			put_inlines(node.children, with(look, "strike", true), parts, draw, segments)
		elseif kind == "code" then
			put(parts, node.text, with(look, "code", true))
		elseif kind == "link" then
			put_inlines(node.children, with(look, "link", link_id(draw, node.href or "")), parts, draw, segments)
		elseif kind == "image" then
			if o.image then
				segments[#segments + 1] = { looks = parts.looks, texts = parts.texts }
				made(draw)
				segments[#segments + 1] = o.image(node.src, node.alt, node.title)
				-- What follows the image is a text of its own.
				parts.looks, parts.texts = {}, {}
			else
				local words = node.alt ~= "" and node.alt or node.src or ""
				put(parts, words, with(look, "color", o.colors.quiet))
			end
		elseif kind == "html" then
			put(parts, node.text, with(look, "color", o.colors.quiet))
		end
	end
end

-- A text of many looks of the inlines `nodes`, in `look`; or, where it holds images the page draws,
-- a row that wraps of its texts and images.
function paragraph_of(nodes, look, draw)
	local parts = parts_new()
	local segments = {}
	put_inlines(nodes, look, parts, draw, segments)
	if #segments == 0 then
		made(draw)
		return pencere.rich(spans_of(parts, draw)):wrap()
	end
	segments[#segments + 1] = parts
	local row = pencere.div():row():wrap():items_center():gap(4)
	made(draw)
	for _, segment in ipairs(segments) do
		if segment.looks then
			if #segment.looks > 0 then
				made(draw)
				row:child(pencere.rich(spans_of(segment, draw)):wrap())
			end
		else
			row:child(segment)
		end
	end
	return row
end

-- --- Code ---------------------------------------------------------------------------------------

-- `@pencere/languages`, looked for the first time code is coloured (false where the kit has none).
local languages_module = nil

local function languages_of(draw)
	local given = draw.o.languages
	if given ~= nil then
		return given
	end
	if languages_module == nil then
		local ok, found = pcall(require, "@pencere/languages")
		languages_module = ok and type(found) == "table" and found or false
	end
	return languages_module or nil
end

-- The language of the word a fence says (`lua`, `sql`) loaded, or nil where none is known.
local function language_of(word, draw)
	local languages = languages_of(draw)
	if languages == nil or type(word) ~= "string" then
		return nil
	end
	local ok, language = pcall(function()
		local description = languages.find(word)
		return description and languages.load(description)
	end)
	if ok and type(language) == "table" and language.syntax then
		return language
	end
	return nil
end

-- The lines of `text` (a last empty one left out).
local function lines_of(text)
	local lines = {}
	local at, length = 1, #text
	while at <= length do
		local stop = find(text, "\n", at, true)
		lines[#lines + 1] = sub(text, at, (stop or length + 1) - 1)
		if stop == nil then
			break
		end
		at = stop + 1
	end
	if #lines == 0 then
		lines[1] = ""
	end
	return lines
end

-- The byte of `line` where character `count` (from 0) starts.
local function byte_of(line, count, ascii)
	if ascii then
		return count + 1
	end
	return utf8.offset(line, count + 1) or #line + 1
end

-- Puts a line of code in the parts, its tokens in the looks of `syntax`; the state after it, or nil
-- where it could not be read (it is then put plain).
local function put_tokens(parts, line, syntax, state, looks, plain)
	local ok, tokens, after = pcall(syntax.tokens, syntax, line, state)
	if not ok or type(tokens) ~= "table" then
		put(parts, line, plain)
		return nil, false
	end
	local ascii = not find(line, "[\128-\255]")
	local at = 1
	for _, token in ipairs(tokens) do
		local from, to = byte_of(line, token.from, ascii), byte_of(line, token.to, ascii)
		if from > at then
			put(parts, sub(line, at, from - 1), plain)
		end
		local look = looks[token.name]
		if look == nil then
			local found = syntax.look and syntax:look(token.name)
			look = plain
			if found then
				look = {
					color = found.color,
					background = found.background,
					italic = found.italic,
					underline = found.underline and true or nil,
					strike = found.strike,
				}
				if found.weight == "bold" then
					look.bold = true
				elseif type(found.weight) == "number" then
					look.weight = found.weight
				end
			end
			looks[token.name] = look
		end
		put(parts, sub(line, from, to - 1), look)
		at = to
	end
	if at <= #line then
		put(parts, sub(line, at), plain)
	end
	return after, true
end

-- A block of code as the module draws it: a box of the terminal's font, its lines coloured where
-- the language is known (and the draw may yet colour), in texts of at most CODE_LINES lines.
local function code_box(language, text, draw)
	local colors = draw.o.colors
	local box = pencere.div():col():p(10):rounded(6):bg(colors.code):family("terminal"):clip()
	made(draw)
	local lines = lines_of(text)
	local found = language and language_of(language, draw)
	local syntax = found and found.syntax
	local state = nil
	local looks = {}
	local plain = {}
	local index = 1
	while index <= #lines do
		if full(draw) then
			box:child(more(draw))
			break
		end
		local parts = parts_new()
		local last = math.min(index + CODE_LINES - 1, #lines)
		for k = index, last do
			local line = lines[k]
			if syntax and draw.coloured > 0 then
				draw.coloured -= #line + 1
				local ok
				state, ok = put_tokens(parts, line, syntax, state, looks, plain)
				if not ok then
					syntax = nil
				end
			else
				if syntax then
					draw.provisional = true
				end
				put(parts, line, plain)
			end
			if k < #lines then
				put(parts, "\n", plain)
			end
		end
		if #parts.looks == 0 then
			put(parts, " ", plain)
		end
		made(draw)
		box:child(pencere.rich(spans_of(parts, draw)))
		index = last + 1
	end
	return box
end
render.code = code_box

-- --- Blocks -------------------------------------------------------------------------------------

local block_of

-- A column of `nodes`, `gap` apart.
local function column_of(nodes, draw, gap, depth, tight)
	local column = pencere.div():col():gap(gap)
	made(draw)
	for _, node in ipairs(nodes) do
		if full(draw) then
			column:child(more(draw))
			break
		end
		column:child(block_of(node, draw, depth, tight))
	end
	return column
end

local function checkbox(checked, draw)
	local colors = draw.o.colors
	local box = pencere.div()
		:size(14, 14)
		:mt(2)
		:rounded(3)
		:border(1, checked and colors.link or colors.quiet)
		:role("checkbox")
		:checked(checked)
	made(draw)
	if checked then
		box:bg(colors.link)
		box:child(pencere.canvas({
			pencere.path():move_to(2.5, 6):line_to(5, 8.5):line_to(9.5, 3.5):stroke(1.6, "background"),
		}):size(12, 12))
		made(draw)
	end
	return box
end

local function list_of(node, draw, depth)
	local o = draw.o
	local gap = node.tight and 4 or o.gap
	local list = pencere.div():col():gap(gap)
	made(draw)
	local size = o.size or 13
	local width = 14
	if node.ordered then
		width = math.ceil(size * 0.6 * (#tostring((node.start or 1) + #node.children - 1) + 1)) + 2
	end
	for index, item in ipairs(node.children) do
		if full(draw) then
			list:child(more(draw))
			break
		end
		local marker
		if item.checked ~= nil then
			marker = checkbox(item.checked, draw)
		elseif node.ordered then
			marker = pencere.text(tostring((node.start or 1) + index - 1) .. "."):color(o.colors.quiet)
			made(draw)
		else
			marker = pencere.text(BULLETS[depth % #BULLETS + 1]):color(o.colors.quiet)
			made(draw)
		end
		local row = pencere.div()
			:row()
			:gap(6)
			:items_start()
			:child(pencere.div():w(math.max(width, item.checked ~= nil and 16 or 0)):row():justify_end():no_shrink():child(marker))
			:child(column_of(item.children, draw, gap, depth + 1, node.tight):grow(1):min_w(0))
		made(draw, 2)
		list:child(row)
	end
	return list
end

local function table_of(node, draw)
	local o = draw.o
	local columns = math.min(#node.align, 64)
	local tracks = {}
	for k = 1, columns do
		tracks[k] = pencere.fr(1)
	end
	local grid = pencere.div():grid(tracks):border(1, o.colors.rule):rounded(4):clip()
	made(draw)
	local function cell(nodes, k, head)
		local box = pencere.div():px(8):py(5):border(1, pencere.alpha(o.colors.rule, 0.6))
		made(draw)
		local words = paragraph_of(nodes, {}, draw):align(ALIGN[node.align[k]] or "start")
		if head then
			box:bg(o.colors.head)
			words:weight(700)
		end
		return box:child(words)
	end
	for k = 1, columns do
		grid:child(cell(node.head[k], k, true))
	end
	for _, row in ipairs(node.rows) do
		if full(draw) then
			break
		end
		for k = 1, columns do
			grid:child(cell(row[k], k, false))
		end
	end
	return grid
end

-- The element of a block. `depth` is how many blocks hold it; `tight`, whether it is in an item of
-- a tight list (its paragraphs are then as close as its lines).
function block_of(node, draw, depth, tight)
	local o = draw.o
	local colors = o.colors
	local kind = node.type
	if depth > DEPTH then
		return more(draw)
	end
	if kind == "paragraph" then
		local element = paragraph_of(node.children, {}, draw)
		if colors.text then
			element:color(colors.text)
		end
		return element
	elseif kind == "heading" then
		local level = node.level or 1
		local element = paragraph_of(node.children, {}, draw)
			:text_size(o.headings[level] or 13)
			:weight(700)
			:role("heading", level)
		if colors.text then
			element:color(colors.text)
		end
		return element
	elseif kind == "code" then
		if o.code then
			made(draw)
			return o.code(node.language, node.text or "")
		end
		return code_box(node.language, node.text or "", draw)
	elseif kind == "html" then
		-- Shown as the text it is, never run.
		made(draw, 2)
		local parts = parts_new()
		put(parts, node.text or "", {})
		return pencere.div()
			:p(10)
			:rounded(6)
			:bg(colors.code)
			:family("terminal")
			:color(colors.quiet)
			:clip()
			:child(pencere.rich(spans_of(parts, draw)))
	elseif kind == "rule" then
		made(draw)
		return pencere.div():h(1):bg(colors.rule)
	elseif kind == "quote" then
		made(draw, 2)
		return pencere.div()
			:row()
			:gap(10)
			:child(pencere.div():w(3):rounded(2):bg(colors.quote):no_shrink())
			:child(column_of(node.children or {}, draw, o.gap, depth + 1, false):grow(1):min_w(0):color(colors.quiet))
	elseif kind == "list" then
		return list_of(node, draw, depth)
	elseif kind == "table" then
		return table_of(node, draw)
	end
	made(draw)
	return pencere.div()
end

render.block = function(node, draw)
	return block_of(node, draw, 0, false)
end

-- The column of a document's blocks.
function render.document(document, draw)
	return column_of(document.children or {}, draw, draw.o.gap, 0, false)
end

-- --- Sizes before the layout says them --------------------------------------------------------------

-- How many characters the inlines `nodes` read.
local function characters(nodes)
	local count = 0
	if nodes == nil then
		return 0
	end
	for _, node in ipairs(nodes) do
		local words = node.text or node.alt
		if words then
			count += #words
		elseif node.children then
			count += characters(node.children)
		else
			count += 1
		end
	end
	return count
end

-- How tall a block is likely to be in a width of `width` points, its text `size` points: what a long
-- list of blocks stands on until the layout says how tall each is.
local function estimate(node, width, size, o, depth)
	local line = size * 1.45
	local kind = node.type
	width = math.max(width, 40)
	if kind == "paragraph" or kind == "heading" then
		local text_size = kind == "heading" and (o.headings[node.level or 1] or size) or size
		local per_line = math.max(math.floor(width / (text_size * 0.55)), 1)
		local rows = math.max(math.ceil(characters(node.children) / per_line), 1)
		return rows * text_size * 1.45
	elseif kind == "code" or kind == "html" then
		local _, breaks = string.gsub(node.text or "", "\n", "")
		return (breaks + 1) * size * 1.4 + 20
	elseif kind == "rule" then
		return 1
	elseif kind == "quote" or kind == "item" then
		local inner = kind == "quote" and width - 13 or width
		local total = 0
		for _, child in ipairs(node.children or {}) do
			total += estimate(child, inner, size, o, depth + 1) + o.gap
		end
		return math.max(total - o.gap, line)
	elseif kind == "list" then
		local total = 0
		local gap = node.tight and 4 or o.gap
		for _, item in ipairs(node.children) do
			total += estimate(item, width - 26, size, o, depth + 1) + gap
		end
		return math.max(total - gap, line)
	elseif kind == "table" then
		return (#node.rows + 1) * (line + 10) + 2
	end
	return line
end
render.estimate = function(node, width, o)
	return estimate(node, width, o.size or 13, o, 0)
end

return render
