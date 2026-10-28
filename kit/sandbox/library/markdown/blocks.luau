-- How `@pencere/markdown` reads the blocks of a text: a line at a time, as CommonMark's reference
-- parser (commonmark.js) does, with GitHub's tables and task lists. The blocks still open (a quote,
-- a list and its item, a paragraph) are a path from the document down; each line is matched
-- against them from the top (`>` for a quote, the indent of an item), what it starts is opened
-- below the last one it matched, and its text goes to the block at the end of the path. A block
-- is closed when a line does not go on with it.
--
-- A reader is made with `blocks.new(line)` (the number of the first line it is given), told one
-- line at a time (`blocks.read`) and ended (`blocks.finish`). Each block of the document's own (a
-- block at the top) is handed out once it is closed, in `closed`: { line =, lines =, node =,
-- texts =, defs =, joins =, attached = }, where it began and how many lines it took, the block
-- (nil for a paragraph that was all definitions of links), the texts whose inlines are still to be
-- read, as { holder, key, source } (`holder[key]` is to be the inlines of `source`), the
-- definitions of links it holds, whether a line after the blank lines that follow it may still go
-- on with it (a list, code indented), and whether it began on a line read as the block before it
-- (a table whose header was the last line of a paragraph). The inlines are read apart (`inlines.parse`), once the definitions they may
-- name are known.

local inlines = require("./inlines")
local entities = require("./entities")

local byte, find, match, sub, gsub, rep, lower = string.byte, string.find, string.match, string.sub, string.gsub, string.rep, string.lower
local concat = table.concat
local trim = inlines.trim

local blocks = {}

local TAB, SPACE, STAR, PLUS, DASH, SLASH = 9, 32, 42, 43, 45, 47
local LT, EQUALS, GT, OPEN_BRACKET, BACKSLASH, UNDERSCORE, BACKTICK, PIPE, TILDE = 60, 61, 62, 91, 92, 95, 96, 124, 126
local CODE_INDENT = 4

-- The blocks whose lines are their text, and those that hold blocks.
local ACCEPTS = { paragraph = true, code = true, html = true, table = true }
local CONTAINER = { document = true, quote = true, list = true, item = true }

-- The first characters a line that starts a block may have: a line that starts with none of them
-- (and is not indented as code) is text, and is not tried against every start.
local MAYBE = {}
for _, code in ipairs({ string.byte("#`~*+_=<>-|:0123456789", 1, -1) }) do
	MAYBE[code] = true
end

-- The names of HTML whose tags start a block of HTML that ends at a blank line.
local BLOCK_TAGS = {}
for name in
	string.gmatch(
		"address article aside base basefont blockquote body caption center col colgroup dd details "
			.. "dialog dir div dl dt fieldset figcaption figure footer form frame frameset h1 h2 h3 h4 h5 h6 "
			.. "head header hr html iframe legend li link main menu menuitem nav noframes ol optgroup "
			.. "option p param search section summary table tbody td tfoot th thead title tr track ul",
		"%S+"
	)
do
	BLOCK_TAGS[name] = true
end

local function can_contain(parent, kind)
	if parent == "list" then
		return kind == "item"
	end
	return CONTAINER[parent] == true and kind ~= "item"
end

-- --- Where the reader is in its line -------------------------------------------------------------

-- `offset` is the byte read to (from 1) and `column` its column (a tab to the next multiple of 4);
-- `partial_tab` says the tab at `offset` is only partly read, as an indent may take part of one.

local function find_next_nonspace(p)
	local line = p.line
	local at, column = p.offset, p.column
	local code = byte(line, at)
	while code == SPACE or code == TAB do
		if code == SPACE then
			column += 1
		else
			column += 4 - column % 4
		end
		at += 1
		code = byte(line, at)
	end
	p.blank = code == nil
	p.next_nonspace = at
	p.next_nonspace_column = column
	p.indent = column - p.column
	p.indented = p.indent >= CODE_INDENT
end

-- Reads `count` characters on, or where `columns` `count` columns (part of a tab, where it takes).
local function advance_offset(p, count, columns)
	local line = p.line
	while count > 0 do
		local code = byte(line, p.offset)
		if code == nil then
			break
		end
		if code == TAB then
			local to_tab = 4 - p.column % 4
			if columns then
				p.partial_tab = to_tab > count
				local step = to_tab > count and count or to_tab
				p.column += step
				if not p.partial_tab then
					p.offset += 1
				end
				count -= step
			else
				p.partial_tab = false
				p.column += to_tab
				p.offset += 1
				count -= 1
			end
		else
			p.partial_tab = false
			p.offset += 1
			p.column += 1
			count -= 1
		end
	end
end

local function advance_next_nonspace(p)
	p.offset = p.next_nonspace
	p.column = p.next_nonspace_column
	p.partial_tab = false
end

-- --- Blocks ---------------------------------------------------------------------------------------

local finalize

-- The rest of the line is the next line of the block at the end of the path.
local function add_line(p)
	local line = p.line
	local text
	if p.partial_tab then
		p.offset += 1
		text = rep(" ", 4 - p.column % 4) .. sub(line, p.offset)
	else
		text = sub(line, p.offset)
	end
	local buffer = p.tip._lines
	buffer[#buffer + 1] = text
end

-- A new block of `kind` at the end of the path, the blocks that cannot hold it closed first.
local function add_child(p, kind)
	while not can_contain(p.tip.type, kind) do
		finalize(p, p.tip, p.number - 1)
	end
	local node = { type = kind, line = p.number, _open = true, _parent = p.tip }
	if CONTAINER[kind] then
		node.children = {}
	end
	if ACCEPTS[kind] then
		node._lines = {}
	end
	local kids = p.tip.children
	kids[#kids + 1] = node
	p.tip = node
	return node
end

-- Closes the blocks the line did not go on with.
local function close_unmatched(p)
	if not p.all_closed then
		while p.oldtip ~= p.last_matched do
			local parent = p.oldtip._parent
			finalize(p, p.oldtip, p.number - 1)
			p.oldtip = parent
		end
		p.all_closed = true
	end
end

-- A row of a table, split at its pipes (one written `\|` is a pipe in its cell), the pipes at its
-- ends and the spaces around each cell left out.
local function split_row(text)
	text = trim(text)
	local at, length = 1, #text
	if byte(text, 1) == PIPE then
		at = 2
	end
	if length >= at and byte(text, length) == PIPE and byte(text, length - 1) ~= BACKSLASH then
		length -= 1
	end
	local cells = {}
	local pieces = {}
	local from = at
	local here = at
	while here <= length do
		local code = byte(text, here)
		if code == BACKSLASH then
			if byte(text, here + 1) == PIPE then
				pieces[#pieces + 1] = sub(text, from, here - 1)
				from = here + 1
			end
			here += 2
		elseif code == PIPE then
			pieces[#pieces + 1] = sub(text, from, here - 1)
			cells[#cells + 1] = trim(concat(pieces))
			pieces = {}
			here += 1
			from = here
		else
			here += 1
		end
	end
	pieces[#pieces + 1] = sub(text, from, length)
	cells[#cells + 1] = trim(concat(pieces))
	return cells
end

-- How the columns of a table align, from its delimiter row (`| :-- | :-: | --: |`), or nil where
-- the line is not one.
local function delimiter_row(text)
	if not find(text, "-", 1, true) or find(text, "[^ \t|:%-]") then
		return nil
	end
	local aligns = {}
	for index, cell in ipairs(split_row(text)) do
		local left, right = match(cell, "^(:?)%-+(:?)$")
		if left == nil then
			return nil
		end
		if left ~= "" and right ~= "" then
			aligns[index] = "center"
		elseif right ~= "" then
			aligns[index] = "right"
		elseif left ~= "" then
			aligns[index] = "left"
		else
			aligns[index] = false
		end
	end
	return aligns
end

local CONTINUE = {}

function CONTINUE.document()
	return 0
end

function CONTINUE.list()
	return 0
end

function CONTINUE.quote(p)
	if not p.indented and byte(p.line, p.next_nonspace) == GT then
		advance_next_nonspace(p)
		advance_offset(p, 1, false)
		local code = byte(p.line, p.offset)
		if code == SPACE or code == TAB then
			advance_offset(p, 1, true)
		end
		return 0
	end
	return 1
end

function CONTINUE.item(p, item)
	if p.blank then
		if #item.children == 0 then
			-- A blank line after an item with nothing in it ends it.
			return 1
		end
		advance_next_nonspace(p)
	elseif p.indent >= item._list.marker_offset + item._list.padding then
		advance_offset(p, item._list.marker_offset + item._list.padding, true)
	else
		return 1
	end
	return 0
end

function CONTINUE.heading()
	return 1
end

function CONTINUE.rule()
	return 1
end

function CONTINUE.code(p, block)
	if block._fence then
		local line, at = p.line, p.next_nonspace
		if p.indent <= 3 and byte(line, at) == block._fence_char then
			local stop = at
			while byte(line, stop) == block._fence_char do
				stop += 1
			end
			if stop - at >= block._fence_length and find(line, "^[ \t]*$", stop) then
				-- The closing fence: the line is done with.
				finalize(p, block, p.number)
				return 2
			end
		end
		-- The indent of the opening fence is taken off each line.
		local left = block._fence_offset
		while left > 0 do
			local code = byte(line, p.offset)
			if code ~= SPACE and code ~= TAB then
				break
			end
			advance_offset(p, 1, true)
			left -= 1
		end
	elseif p.indent >= CODE_INDENT then
		advance_offset(p, CODE_INDENT, true)
	elseif p.blank then
		advance_next_nonspace(p)
	else
		return 1
	end
	return 0
end

function CONTINUE.html(p, block)
	return (p.blank and block._html >= 6) and 1 or 0
end

function CONTINUE.paragraph(p)
	return p.blank and 1 or 0
end

function CONTINUE.table(p)
	return p.blank and 1 or 0
end

local FINALIZE = {}

function FINALIZE.list(_, list)
	-- Loose where an item ends before a blank line and another follows, or two blocks of an item
	-- have one between them.
	local tight = true
	local items = list.children
	for index, item in ipairs(items) do
		local after = items[index + 1]
		if after and item._end ~= after.line - 1 then
			tight = false
			break
		end
		local kids = item.children
		for k = 1, #kids - 1 do
			if kids[k]._end ~= kids[k + 1].line - 1 then
				tight = false
				break
			end
		end
		if not tight then
			break
		end
	end
	list.tight = tight
	list._end = items[#items]._end
end

function FINALIZE.item(_, item)
	local kids = item.children
	item._end = #kids > 0 and kids[#kids]._end or item.line
end

function FINALIZE.code(_, block)
	local lines = block._lines
	if block._fence then
		local info = entities.unescape(trim(lines[1] or ""))
		block.info = info ~= "" and info or nil
		block.language = match(info, "^%S+")
		block.text = concat(lines, "\n", 2)
	else
		local last = #lines
		while last > 0 and find(lines[last], "^[ \t]*$") do
			last -= 1
		end
		block.text = concat(lines, "\n", 1, last)
		block._end = block.line + last - 1
	end
end

function FINALIZE.html(_, block)
	block.text = concat(block._lines, "\n")
end

function FINALIZE.table(_, block)
	local width = #block.align
	local rows = {}
	for _, raw in ipairs(block._lines) do
		if find(raw, "[^ \t]") then
			local cells = split_row(raw)
			local row = {}
			for k = 1, width do
				row[k] = cells[k] or ""
			end
			rows[#rows + 1] = row
		end
	end
	block._rows = rows
end

-- --- Starts of blocks ----------------------------------------------------------------------------

-- Each looks at the line where the reader is (after the blocks it matched) and starts its block
-- there: 0 where it does not, 1 where it started one that holds blocks (more may start inside),
-- 2 where it started one whose lines are its text.

local function start_quote(p)
	if p.indented or byte(p.line, p.next_nonspace) ~= GT then
		return 0
	end
	advance_next_nonspace(p)
	advance_offset(p, 1, false)
	local code = byte(p.line, p.offset)
	if code == SPACE or code == TAB then
		advance_offset(p, 1, true)
	end
	close_unmatched(p)
	add_child(p, "quote")
	return 1
end

local function start_heading(p)
	if p.indented then
		return 0
	end
	local line, at = p.line, p.next_nonspace
	local hashes = match(line, "^#+", at)
	if hashes == nil or #hashes > 6 then
		return 0
	end
	local after = byte(line, at + #hashes)
	if after ~= nil and after ~= SPACE and after ~= TAB then
		return 0
	end
	advance_next_nonspace(p)
	advance_offset(p, #hashes, false)
	close_unmatched(p)
	local node = add_child(p, "heading")
	node.level = #hashes
	-- The closing `#`s, where a space is before them, are not its words.
	local rest = gsub(sub(line, p.offset), "^[ \t]*#+[ \t]*$", "")
	node._text = gsub(rest, "[ \t]+#+[ \t]*$", "")
	advance_offset(p, #line - p.offset + 1, false)
	return 2
end

local function start_fence(p)
	if p.indented then
		return 0
	end
	local line, at = p.line, p.next_nonspace
	local mark = byte(line, at)
	if mark ~= BACKTICK and mark ~= TILDE then
		return 0
	end
	local stop = at
	while byte(line, stop) == mark do
		stop += 1
	end
	local length = stop - at
	if length < 3 or (mark == BACKTICK and find(line, "`", stop, true)) then
		return 0
	end
	close_unmatched(p)
	local node = add_child(p, "code")
	node._fence, node._fence_char, node._fence_length, node._fence_offset = true, mark, length, p.indent
	advance_next_nonspace(p)
	advance_offset(p, length, false)
	return 2
end

-- Which of CommonMark's seven kinds of block of HTML `text` (a line from its `<`) starts, or nil;
-- the seventh (any whole tag alone on its line) only where `seventh`.
local function html_kind(text, seventh)
	local tag = match(text, "^<(%a+)")
	if tag then
		local name = lower(tag)
		if name == "script" or name == "pre" or name == "textarea" or name == "style" then
			local after = byte(text, #tag + 2)
			if after == nil or after == SPACE or after == TAB or after == GT then
				return 1
			end
		end
	end
	if sub(text, 1, 4) == "<!--" then
		return 2
	elseif sub(text, 1, 2) == "<?" then
		return 3
	elseif match(text, "^<!%a") then
		return 4
	elseif sub(text, 1, 9) == "<![CDATA[" then
		return 5
	end
	local slash, name = match(text, "^<(/?)(%a%w*)")
	if name and BLOCK_TAGS[lower(name)] then
		local at = #slash + #name + 2
		local after = byte(text, at)
		if after == nil or after == SPACE or after == TAB or after == GT or (after == SLASH and byte(text, at + 1) == GT) then
			return 6
		end
	end
	if seventh then
		local stop = inlines.open_tag(text, 1) or inlines.close_tag(text, 1)
		if stop and find(text, "^[ \t]*$", stop) then
			return 7
		end
	end
	return nil
end

-- Whether `text` has what ends a block of HTML of the kinds 1 to 5.
local function html_ends(kind, text)
	if kind == 1 then
		local lowered = lower(text)
		return find(lowered, "</script>", 1, true) ~= nil
			or find(lowered, "</pre>", 1, true) ~= nil
			or find(lowered, "</textarea>", 1, true) ~= nil
			or find(lowered, "</style>", 1, true) ~= nil
	elseif kind == 2 then
		return find(text, "-->", 1, true) ~= nil
	elseif kind == 3 then
		return find(text, "?>", 1, true) ~= nil
	elseif kind == 4 then
		return find(text, ">", 1, true) ~= nil
	end
	return find(text, "]]>", 1, true) ~= nil
end

local function start_html(p, container)
	if p.indented or byte(p.line, p.next_nonspace) ~= LT then
		return 0
	end
	-- The seventh kind does not break a paragraph, nor go on with one lazily.
	local seventh = container.type ~= "paragraph"
		and not (not p.all_closed and not p.blank and p.tip.type == "paragraph")
	local kind = html_kind(sub(p.line, p.next_nonspace), seventh)
	if kind == nil then
		return 0
	end
	close_unmatched(p)
	-- The spaces before it are its text.
	local node = add_child(p, "html")
	node._html = kind
	return 2
end

-- The definitions of links at the start of a paragraph's `content` (which starts at line `first`)
-- taken into the reader's: the rest, and how many lines they took.
local function take_definitions(p, content, first)
	if byte(content, 1) ~= OPEN_BRACKET then
		return content, 0
	end
	local at, line = 1, first
	local defs = p.defs
	while byte(content, at) == OPEN_BRACKET do
		local stop, def = inlines.reference(content, at)
		if stop == nil then
			break
		end
		def.line = line
		defs[#defs + 1] = def
		local _, breaks = gsub(sub(content, at, stop - 1), "\n", "")
		line += breaks
		at = stop
	end
	if at == 1 then
		return content, 0
	end
	return sub(content, at), line - first
end

local function start_setext(p, container)
	if p.indented or container.type ~= "paragraph" then
		return 0
	end
	local line, at = p.line, p.next_nonspace
	local mark = byte(line, at)
	if mark ~= EQUALS and mark ~= DASH then
		return 0
	end
	local stop = at
	while byte(line, stop) == mark do
		stop += 1
	end
	if not find(line, "^[ \t]*$", stop) then
		return 0
	end
	close_unmatched(p)
	-- Definitions of links at its start are not the heading's words.
	local content = concat(container._lines, "\n")
	local rest, taken = take_definitions(p, content, container.line)
	if rest ~= content then
		container._lines = { rest }
		container.line += taken
	end
	if rest == "" then
		return 0
	end
	local heading = {
		type = "heading",
		level = mark == EQUALS and 1 or 2,
		line = container.line,
		_open = true,
		_parent = container._parent,
		_lines = container._lines,
	}
	local kids = container._parent.children
	kids[#kids] = heading
	p.tip = heading
	advance_offset(p, #line - p.offset + 1, false)
	return 2
end

-- GitHub's table: a paragraph's last line is its header where the line under it is a delimiter
-- row of as many cells. The lines of the paragraph before it stay a paragraph.
local function start_table(p, container)
	if p.indented or container.type ~= "paragraph" then
		return 0
	end
	local line = p.line
	local aligns = delimiter_row(sub(line, p.next_nonspace))
	if aligns == nil then
		return 0
	end
	local lines = container._lines
	local head = split_row(lines[#lines])
	if #head ~= #aligns then
		return 0
	end
	close_unmatched(p)
	local parent = container._parent
	local attached = #lines > 1
	if attached then
		lines[#lines] = nil
		finalize(p, container, p.number - 2)
	else
		local kids = parent.children
		kids[#kids] = nil
		p.tip = parent
	end
	local node = add_child(p, "table")
	node.line = p.number - 1
	node.align = aligns
	node._head = head
	-- Its header was read as a line of the paragraph before it: a reading cannot start at it.
	node._attached = attached
	advance_offset(p, #line - p.offset + 1, false)
	return 2
end

local function start_rule(p)
	if p.indented then
		return 0
	end
	local line, at = p.line, p.next_nonspace
	local mark = byte(line, at)
	if mark ~= STAR and mark ~= UNDERSCORE and mark ~= DASH then
		return 0
	end
	local count = 0
	for k = at, #line do
		local code = byte(line, k)
		if code == mark then
			count += 1
		elseif code ~= SPACE and code ~= TAB then
			return 0
		end
	end
	if count < 3 then
		return 0
	end
	close_unmatched(p)
	add_child(p, "rule")
	advance_offset(p, #line - p.offset + 1, false)
	return 2
end

-- The marker of a list item where the reader is (`-`, `*`, `+`, `1.`, `1)`): what it says and how
-- far its text is in (`padding`), or nil.
local function list_marker(p, container)
	if p.indent >= 4 then
		return nil
	end
	local line, at = p.line, p.next_nonspace
	local code = byte(line, at)
	local data, length
	if code == STAR or code == PLUS or code == DASH then
		data = { ordered = false, bullet = code, marker_offset = p.indent }
		length = 1
	else
		local digits, delimiter = match(line, "^(%d+)([%.%)])", at)
		if digits == nil or #digits > 9 then
			return nil
		end
		-- Only a list that starts at 1 breaks a paragraph.
		if container.type == "paragraph" and tonumber(digits) ~= 1 then
			return nil
		end
		data = { ordered = true, start = tonumber(digits), delimiter = delimiter, marker_offset = p.indent }
		length = #digits + 1
	end
	local after = byte(line, at + length)
	if after ~= nil and after ~= TAB and after ~= SPACE then
		return nil
	end
	-- An item with nothing on its first line does not break a paragraph.
	if container.type == "paragraph" and not find(line, "[^ \t]", at + length) then
		return nil
	end
	advance_next_nonspace(p)
	advance_offset(p, length, true)
	local spaces_column, spaces_offset = p.column, p.offset
	repeat
		advance_offset(p, 1, true)
		after = byte(line, p.offset)
	until not (p.column - spaces_column < 5 and (after == SPACE or after == TAB))
	local blank_item = byte(line, p.offset) == nil
	local spaces = p.column - spaces_column
	if spaces >= 5 or spaces < 1 or blank_item then
		-- Code after five spaces, or nothing: the text is one space in.
		data.padding = length + 1
		p.column, p.offset = spaces_column, spaces_offset
		local next_code = byte(line, p.offset)
		if next_code == SPACE or next_code == TAB then
			advance_offset(p, 1, true)
		end
	else
		data.padding = length + spaces
	end
	return data
end

local function start_item(p, container)
	if p.indented and container.type ~= "list" then
		return 0
	end
	local data = list_marker(p, container)
	if data == nil then
		return 0
	end
	close_unmatched(p)
	local tip = p.tip
	local same = tip.type == "list"
		and tip._list.ordered == data.ordered
		and tip._list.delimiter == data.delimiter
		and tip._list.bullet == data.bullet
	if not same then
		local list = add_child(p, "list")
		list._list = data
		list.ordered = data.ordered
		list.start = data.start
	end
	local item = add_child(p, "item")
	item._list = data
	return 1
end

local function start_indented(p)
	if not p.indented or p.tip.type == "paragraph" or p.blank then
		return 0
	end
	advance_offset(p, CODE_INDENT, true)
	close_unmatched(p)
	add_child(p, "code")
	return 2
end

local STARTS = {
	start_quote,
	start_heading,
	start_fence,
	start_html,
	start_setext,
	start_table,
	start_rule,
	start_item,
	start_indented,
}
local STARTS_COUNT = #STARTS

-- --- A block of the document's own, closed ------------------------------------------------------

local function push_text(entry, holder, key, source)
	local texts = entry.texts
	texts[#texts + 1] = { holder, key, source }
end

-- Takes the reader's own fields off a closed block.
local function clean(node)
	for key in pairs(node) do
		if byte(key, 1) == UNDERSCORE then
			node[key] = nil
		end
	end
end

-- Readies a closed block and what is in it for the page: definitions out of its paragraphs (a
-- paragraph of nothing else is gone: false), a task's box out of an item's first line, the texts
-- whose inlines are to be read, `lines`, and the reader's fields taken off.
local function settle(p, node, entry)
	local kind = node.type
	if kind == "paragraph" then
		local content = concat(node._lines, "\n")
		local rest, taken = take_definitions(p, content, node.line)
		if not find(rest, "[^ \t\n]") then
			return false
		end
		node.line += taken
		push_text(entry, node, "children", trim(rest))
	elseif kind == "heading" then
		push_text(entry, node, "children", trim(node._text or concat(node._lines, "\n")))
	elseif kind == "table" then
		local head = {}
		for index, source in ipairs(node._head) do
			head[index] = {}
			push_text(entry, head, index, source)
		end
		node.head = head
		local rows = {}
		for index, cells in ipairs(node._rows) do
			local row = {}
			for k, source in ipairs(cells) do
				row[k] = {}
				push_text(entry, row, k, source)
			end
			rows[index] = row
		end
		node.rows = rows
	elseif kind == "item" then
		-- GitHub's task: `[ ]`, `[x]` or `[X]` and a space at the start of its first paragraph.
		local first = node.children[1]
		if first and first.type == "paragraph" then
			local line = first._lines[1]
			local mark, after = match(line, "^%[([ \txX])%]()")
			local next_code = mark and byte(line, after)
			if mark and (next_code == nil or next_code == SPACE or next_code == TAB) then
				node.checked = mark ~= " " and mark ~= "\t"
				first._lines[1] = sub(line, match(line, "^[ \t]*()", after))
			end
		end
	end
	local kids = node.children
	if kids then
		local kept = 0
		for index = 1, #kids do
			local kid = kids[index]
			if settle(p, kid, entry) then
				kept += 1
				kids[kept] = kid
			end
		end
		for index = #kids, kept + 1, -1 do
			kids[index] = nil
		end
	end
	node.lines = node._end - node.line + 1
	clean(node)
	return true
end

-- A block of the document's own was closed: it is handed out.
local function close_top(p, block)
	local kids = p.doc.children
	kids[#kids] = nil
	local entry = { line = block.line, lines = block._end - block.line + 1, node = block, texts = {} }
	-- A list, and code indented, may take lines after the blank lines that follow them; a table
	-- whose header was a line of the paragraph before it began inside that paragraph.
	entry.joins = block.type == "list" or (block.type == "code" and not block._fence)
	entry.attached = block._attached == true
	if not settle(p, block, entry) then
		entry.node = nil
	end
	if #p.defs > 0 then
		local defs = p.defs
		table.sort(defs, function(a, b)
			return a.line < b.line
		end)
		entry.defs = defs
		p.defs = {}
	end
	local closed = p.closed
	closed[#closed + 1] = entry
end

-- Closes `block` at line `number`: what it is made of is worked out, and the reader goes on in the
-- block that holds it.
function finalize(p, block, number)
	local above = block._parent
	block._open = false
	block._end = number
	local done = FINALIZE[block.type]
	if done then
		done(p, block)
	end
	p.tip = above
	if above == p.doc then
		close_top(p, block)
	end
end

-- --- Reading ------------------------------------------------------------------------------------

-- A reader whose first line is line `first` (1 where nil).
function blocks.new(first)
	local doc = { type = "document", children = {}, _open = true }
	return {
		doc = doc,
		tip = doc,
		oldtip = doc,
		last_matched = doc,
		number = (first or 1) - 1,
		line = "",
		offset = 1,
		column = 0,
		next_nonspace = 1,
		next_nonspace_column = 0,
		indent = 0,
		indented = false,
		blank = false,
		partial_tab = false,
		all_closed = true,
		closed = {},
		defs = {},
	}
end

-- Reads the next line (its line break left out).
function blocks.read(p, line)
	if find(line, "\0", 1, true) then
		line = gsub(line, "%z", "\u{FFFD}")
	end
	p.number += 1
	p.line = line
	p.offset, p.column, p.blank, p.partial_tab = 1, 0, false, false
	p.oldtip = p.tip
	-- The blocks the line goes on with, from the document down.
	local container = p.doc
	while true do
		local kids = container.children
		local last = kids and kids[#kids]
		if last == nil or not last._open then
			break
		end
		container = last
		find_next_nonspace(p)
		local result = CONTINUE[last.type](p, last)
		if result == 2 then
			return
		elseif result == 1 then
			container = last._parent
			break
		end
	end
	p.all_closed = container == p.oldtip
	p.last_matched = container
	-- What it starts below them.
	local kind = container.type
	local leaf = kind ~= "paragraph" and kind ~= "table" and ACCEPTS[kind] == true
	while not leaf do
		find_next_nonspace(p)
		if not p.indented and not MAYBE[byte(line, p.next_nonspace) or 0] then
			advance_next_nonspace(p)
			break
		end
		local result = 0
		for index = 1, STARTS_COUNT do
			result = STARTS[index](p, container)
			if result ~= 0 then
				break
			end
		end
		if result == 0 then
			advance_next_nonspace(p)
			break
		end
		container = p.tip
		leaf = result == 2
	end
	-- And its text: a paragraph's lazy line, or the next line of the block at the end.
	if not p.all_closed and not p.blank and p.tip.type == "paragraph" then
		add_line(p)
		return
	end
	close_unmatched(p)
	kind = container.type
	if ACCEPTS[kind] then
		add_line(p)
		if kind == "html" and container._html <= 5 and html_ends(container._html, sub(line, p.offset)) then
			finalize(p, container, p.number)
		end
	elseif p.offset <= #line and not p.blank then
		add_child(p, "paragraph")
		advance_next_nonspace(p)
		add_line(p)
	end
end

-- Closes what is open: the text has no more lines.
function blocks.finish(p)
	while p.tip ~= p.doc do
		finalize(p, p.tip, p.number)
	end
end

-- The block of the document's own that is still open, or nil.
function blocks.open(p)
	return p.doc.children[1]
end

return blocks
