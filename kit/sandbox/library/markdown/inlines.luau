-- How `@pencere/markdown` reads the text of a block (a paragraph, a heading, a cell of a table)
-- into its inlines: CommonMark's inline rules, as its reference parser (commonmark.js) reads them,
-- with GitHub's strikethrough and its links that need no brackets. The text is read once, from the
-- left: a delimiter of emphasis or a bracket is remembered on a stack, and what it opens is made
-- when what closes it is found (CommonMark's "delimiter run" algorithm), so that the work grows with
-- the text and not with how its marks nest.
--
-- It also reads a definition of a link (`[label]: address "title"`), which the blocks find at the
-- start of a paragraph, and the tags of HTML, which a block of HTML starts with.

local entities = require("./entities")

local byte, char, find, match, sub, format = string.byte, string.char, string.find, string.match, string.sub, string.format
local concat = table.concat
local ESCAPABLE = entities.ESCAPABLE

local inlines = {}

local NEWLINE, SPACE, TAB, BANG, DQUOTE, AMP, SQUOTE = 10, 32, 9, 33, 34, 38, 39
local OPEN_PAREN, CLOSE_PAREN, STAR, COLON, LT, GT, QUESTION = 40, 41, 42, 58, 60, 62, 63
local OPEN_BRACKET, BACKSLASH, CLOSE_BRACKET, UNDERSCORE, BACKTICK, TILDE, SLASH = 91, 92, 93, 95, 96, 126, 47

-- Where a run of plain text stops: at a character that may mean something.
local SPECIAL = "[\n\\`%*_~%[%]!<&]"

-- --- Characters ---------------------------------------------------------------------------------

-- The code point at byte `at` of `text` (a broken sequence is U+FFFD, a stray byte itself), or nil
-- past the end.
local function code_at(text, at)
	local lead = byte(text, at)
	if lead == nil or lead < 0xC0 then
		return lead
	end
	local more, code
	if lead < 0xE0 then
		more, code = 1, lead - 0xC0
	elseif lead < 0xF0 then
		more, code = 2, lead - 0xE0
	else
		more, code = 3, lead - 0xF0
	end
	for k = 1, more do
		local next_byte = byte(text, at + k)
		if next_byte == nil or next_byte < 0x80 or next_byte >= 0xC0 then
			return 0xFFFD
		end
		code = code * 64 + (next_byte - 0x80)
	end
	return code
end

-- The code point that ends just before byte `at`, or nil at the start.
local function code_before(text, at)
	local last = at - 1
	if last < 1 then
		return nil
	end
	local start = last
	while start > 1 and start > last - 3 do
		local b = byte(text, start)
		if b < 0x80 or b >= 0xC0 then
			break
		end
		start -= 1
	end
	return code_at(text, start)
end

-- Unicode's whitespace, as CommonMark reads it next to a delimiter (nil, an end of the text, is).
local function is_space(code)
	if code == nil then
		return true
	end
	if code < 0x80 then
		return code == SPACE or (code >= 9 and code <= 13)
	end
	return code == 0xA0
		or code == 0x1680
		or (code >= 0x2000 and code <= 0x200A)
		or code == 0x2028
		or code == 0x2029
		or code == 0x202F
		or code == 0x205F
		or code == 0x3000
		or code == 0xFEFF
end

-- Punctuation and symbols (Unicode's P and S), as CommonMark reads them next to a delimiter: all
-- of ASCII's, and the blocks of the others that are made of them.
local function is_punctuation(code)
	if code == nil then
		return false
	end
	if code < 0x80 then
		return (code >= 33 and code <= 47) or (code >= 58 and code <= 64) or (code >= 91 and code <= 96) or (code >= 123 and code <= 126)
	end
	if code < 0x100 then
		return (code >= 0xA1 and code <= 0xA9)
			or code == 0xAB
			or code == 0xAC
			or (code >= 0xAE and code <= 0xB1)
			or code == 0xB4
			or (code >= 0xB6 and code <= 0xB8)
			or code == 0xBB
			or code == 0xBF
			or code == 0xD7
			or code == 0xF7
	end
	return code == 0x37E
		or code == 0x387
		or (code >= 0x55A and code <= 0x55F)
		or code == 0x589
		or code == 0x5BE
		or code == 0x5C0
		or code == 0x5C3
		or code == 0x5C6
		or code == 0x5F3
		or code == 0x5F4
		or code == 0x60C
		or code == 0x60D
		or code == 0x61B
		or code == 0x61F
		or (code >= 0x66A and code <= 0x66D)
		or code == 0x6D4
		or (code >= 0x2010 and code <= 0x2027)
		or (code >= 0x2030 and code <= 0x205E)
		or (code >= 0x207A and code <= 0x207E)
		or (code >= 0x208A and code <= 0x208E)
		or (code >= 0x20A0 and code <= 0x20C0)
		or (code >= 0x2190 and code <= 0x245F)
		or (code >= 0x2500 and code <= 0x2775)
		or (code >= 0x2794 and code <= 0x2BFF)
		or (code >= 0x2E00 and code <= 0x2E7F)
		or (code >= 0x3001 and code <= 0x3003)
		or (code >= 0x3008 and code <= 0x3020)
		or code == 0x3030
		or code == 0x30FB
		or (code >= 0xFE10 and code <= 0xFE19)
		or (code >= 0xFE30 and code <= 0xFE6B)
		or (code >= 0xFF01 and code <= 0xFF0F)
		or (code >= 0xFF1A and code <= 0xFF20)
		or (code >= 0xFF3B and code <= 0xFF40)
		or (code >= 0xFF5B and code <= 0xFF65)
		or (code >= 0x1F000 and code <= 0x1FAFF)
end

-- `text` without the spaces, tabs and line breaks at its ends.
local function trim(text)
	local first = find(text, "[^ \t\n\r]")
	if first == nil then
		return ""
	end
	local last = #text
	while true do
		local b = byte(text, last)
		if b ~= SPACE and b ~= TAB and b ~= NEWLINE and b ~= 13 then
			break
		end
		last -= 1
	end
	if first == 1 and last == #text then
		return text
	end
	return sub(text, first, last)
end
inlines.trim = trim

-- --- Addresses ----------------------------------------------------------------------------------

-- The characters an address keeps as they are (the rest are written `%XX`, a byte at a time): what
-- markdown-it and commonmark.js keep, so that an address is printable ASCII, as a host opens one.
local KEPT = {}
for _, code in ipairs({ string.byte(";/?:@&=+$,-_.!~*'()#", 1, -1) }) do
	KEPT[code] = true
end
for code = 48, 57 do
	KEPT[code] = true
end
for code = 65, 90 do
	KEPT[code] = true
	KEPT[code + 32] = true
end

-- `address` with what an address may not hold written `%XX` (a `%XX` already there is kept).
local function encode(address)
	if not find(address, "[^%w;/%?:@&=%+%$,%-_%.!~%*'%(%)#]") then
		return address
	end
	local out = {}
	local at, length = 1, #address
	while at <= length do
		local code = byte(address, at)
		if code == 37 then
			if match(address, "^%%%x%x", at) then
				out[#out + 1] = sub(address, at, at + 2)
				at += 3
			else
				out[#out + 1] = "%25"
				at += 1
			end
		else
			out[#out + 1] = KEPT[code] and char(code) or format("%%%02X", code)
			at += 1
		end
	end
	return concat(out)
end
inlines.encode = encode

-- --- Labels -------------------------------------------------------------------------------------

-- What a lower case letter is for the capitals Unicode folds past ASCII's (Latin, Greek and
-- Cyrillic), by code point.
local function folded(code)
	if code < 0x80 then
		return (code >= 65 and code <= 90) and code + 32 or code
	elseif code >= 0xC0 and code <= 0xDE and code ~= 0xD7 then
		return code + 32
	elseif code >= 0x100 and code <= 0x17F then
		if code == 0x178 then
			return 0xFF
		elseif (code >= 0x139 and code <= 0x148) or (code >= 0x179 and code <= 0x17E) then
			return code % 2 == 1 and code + 1 or code
		elseif code ~= 0x130 and code ~= 0x131 and code ~= 0x138 and code ~= 0x149 then
			return code % 2 == 0 and code + 1 or code
		end
	elseif code >= 0x391 and code <= 0x3A9 and code ~= 0x3A2 then
		return code + 32
	elseif code == 0x3C2 then
		return 0x3C3
	elseif code == 0x386 then
		return 0x3AC
	elseif code >= 0x388 and code <= 0x38A then
		return code + 37
	elseif code == 0x38C then
		return 0x3CC
	elseif code == 0x38E or code == 0x38F then
		return code + 63
	elseif code >= 0x410 and code <= 0x42F then
		return code + 32
	elseif code >= 0x400 and code <= 0x40F then
		return code + 80
	end
	return code
end

-- A label as definitions and links are matched by: its brackets taken off, its ends trimmed, its
-- runs of whitespace one space, its letters in one case (`ß` and `ẞ` are `ss`).
local function normalize(label)
	local inner = trim(sub(label, 2, -2))
	inner = string.gsub(inner, "[ \t\r\n]+", " ")
	if not find(inner, "[\128-\255]") then
		return string.lower(inner)
	end
	local out = {}
	local at, length = 1, #inner
	while at <= length do
		local code = code_at(inner, at)
		local lead = byte(inner, at)
		local size = lead < 0xC0 and 1 or lead < 0xE0 and 2 or lead < 0xF0 and 3 or 4
		if code == 0xDF or code == 0x1E9E then
			out[#out + 1] = "ss"
		else
			local lower = folded(code)
			out[#out + 1] = lower == code and sub(inner, at, at + size - 1) or utf8.char(lower)
		end
		at += size
	end
	return concat(out)
end
inlines.normalize = normalize

-- --- HTML ---------------------------------------------------------------------------------------

-- The byte after the open tag of HTML at byte `at` of `text` (`<a href="x">`), or nil for none.
local function open_tag(text, at)
	local here = match(text, "^<%a[%w%-]*()", at)
	if here == nil then
		return nil
	end
	while true do
		local name = match(text, "^%s+()[%a_:]", here)
		if name == nil then
			break
		end
		here = match(text, "^[%a_:][%w:%._%-]*()", name)
		local value = match(text, "^%s*=%s*()", here)
		if value then
			local after = match(text, "^[^%s\"'=<>`%c]+()", value)
				or match(text, "^'[^']*'()", value)
				or match(text, '^"[^"]*"()', value)
			if after == nil then
				return nil
			end
			here = after
		end
	end
	return match(text, "^%s*/?>()", here)
end
inlines.open_tag = open_tag

-- The byte after the closing tag at byte `at` of `text` (`</a>`), or nil for none.
local function close_tag(text, at)
	return match(text, "^</%a[%w%-]*%s*>()", at)
end
inlines.close_tag = close_tag

-- The byte after the HTML at byte `at` of `text` (a `<`): a tag, a comment, a processing
-- instruction, a declaration or a CDATA section; nil for none.
local function html_at(text, at)
	local second = byte(text, at + 1)
	if second == SLASH then
		return close_tag(text, at)
	elseif second == BANG then
		if sub(text, at, at + 3) == "<!--" then
			if sub(text, at + 4, at + 4) == ">" then
				return at + 5
			elseif sub(text, at + 4, at + 5) == "->" then
				return at + 6
			end
			local stop = find(text, "-->", at + 4, true)
			return stop and stop + 3 or nil
		elseif sub(text, at, at + 8) == "<![CDATA[" then
			local stop = find(text, "]]>", at + 9, true)
			return stop and stop + 3 or nil
		elseif match(text, "^<!%a", at) then
			local stop = find(text, ">", at + 2, true)
			return stop and stop + 1 or nil
		end
		return nil
	elseif second == QUESTION then
		local stop = find(text, "?>", at + 2, true)
		return stop and stop + 2 or nil
	end
	return open_tag(text, at)
end

-- --- The tree being made ------------------------------------------------------------------------

-- While a text is read its inlines are a list of links (`_prev`, `_next`, `_parent`; a container's
-- `_first` and `_last`), so that what emphasis or a link wraps is moved at once. They are made
-- lists of `children` at the end.

local function append(parent, node)
	node._parent = parent
	local last = parent._last
	if last then
		last._next = node
		node._prev = last
	else
		parent._first = node
	end
	parent._last = node
end

local function unlink(node)
	local parent = node._parent
	local before, after = node._prev, node._next
	if before then
		before._next = after
	else
		parent._first = after
	end
	if after then
		after._prev = before
	else
		parent._last = before
	end
	node._prev, node._next, node._parent = nil, nil, nil
end

local function insert_after(node, new)
	local parent = node._parent
	local after = node._next
	new._parent, new._prev, new._next = parent, node, after
	if after then
		after._prev = new
	else
		parent._last = new
	end
	node._next = new
end

local function text_node(words)
	return { type = "text", text = words }
end

-- The nodes from after `from` up to `to` (not it) moved into a new node of `kind`, which takes
-- their place.
local function wrap(kind, from, to)
	local node = { type = kind }
	local at = from._next
	while at and at ~= to do
		local after = at._next
		unlink(at)
		append(node, at)
		at = after
	end
	insert_after(from, node)
	return node
end

-- --- Reading ------------------------------------------------------------------------------------

-- A reader is { subject, pos, length, root, delimiters, brackets, refs, lookups, runs, cursors }:
-- the text, where it is (bytes from 1), the container the inlines go in, the stacks of delimiters
-- and brackets, the definitions of links by label and the labels it looked up (what it found, or
-- false), and where the text's runs of backticks are.

local function append_text(r, words)
	append(r.root, text_node(words))
end

-- Spaces and tabs, and at most one line break with the spaces and tabs after it.
local function spnl(r)
	local s = r.subject
	local at = match(s, "^[ \t]*()", r.pos)
	if byte(s, at) == NEWLINE then
		at = match(s, "^[ \t]*()", at + 1)
	end
	r.pos = at
end

local function newline(r)
	r.pos += 1
	local last = r.root._last
	local hard = false
	if last and last.type == "text" then
		local words = last.text
		local stop = #words
		while byte(words, stop) == SPACE do
			stop -= 1
		end
		if stop < #words then
			hard = #words - stop >= 2
			last.text = sub(words, 1, stop)
		end
	end
	append(r.root, { type = hard and "hardbreak" or "softbreak" })
	while byte(r.subject, r.pos) == SPACE do
		r.pos += 1
	end
end

local function backslash(r)
	local s = r.subject
	r.pos += 1
	local after = byte(s, r.pos)
	if after == NEWLINE then
		r.pos += 1
		append(r.root, { type = "hardbreak" })
	elseif after and ESCAPABLE[after] then
		append_text(r, char(after))
		r.pos += 1
	else
		append_text(r, "\\")
	end
end

-- Where each run of backticks of the subject starts, by its length: found once, the first time a
-- code span is looked for, so that a text of many backticks that close nothing is read in a time
-- that grows with it, not with its square.
local function runs_of(text)
	local runs = {}
	local at = 1
	while true do
		local start = find(text, "`", at, true)
		if start == nil then
			break
		end
		local stop = start
		while byte(text, stop) == BACKTICK do
			stop += 1
		end
		local length = stop - start
		local list = runs[length]
		if list == nil then
			list = {}
			runs[length] = list
		end
		list[#list + 1] = start
		at = stop
	end
	return runs
end

local function backticks(r)
	local s = r.subject
	local start = r.pos
	local stop = start
	while byte(s, stop) == BACKTICK do
		stop += 1
	end
	local count = stop - start
	if r.runs == nil then
		r.runs = runs_of(s)
		r.cursors = {}
	end
	local list = r.runs[count]
	local close = nil
	if list then
		local cursor = r.cursors[count] or 1
		while list[cursor] and list[cursor] < stop do
			cursor += 1
		end
		r.cursors[count] = cursor
		close = list[cursor]
	end
	if close == nil then
		append_text(r, sub(s, start, stop - 1))
		r.pos = stop
		return
	end
	local content = string.gsub(sub(s, stop, close - 1), "\n", " ")
	-- One space at each end is taken off where both have one and it is not all spaces.
	if #content > 2 and byte(content, 1) == SPACE and byte(content, -1) == SPACE and find(content, "[^ ]") then
		content = sub(content, 2, -2)
	end
	append(r.root, { type = "code", text = content })
	r.pos = close + count
end

-- A run of `*`, `_` or `~`: its text, and on the stack of delimiters where it may open or close
-- (CommonMark's left- and right-flanking runs; `_` not inside a word; `~` in runs of one or two).
local function delimiters(r, mark)
	local s = r.subject
	local start = r.pos
	local stop = start
	while byte(s, stop) == mark do
		stop += 1
	end
	local count = stop - start
	local before, after = code_before(s, start), code_at(s, stop)
	local after_space, after_punctuation = is_space(after), is_punctuation(after)
	local before_space, before_punctuation = is_space(before), is_punctuation(before)
	local left = not after_space and (not after_punctuation or before_space or before_punctuation)
	local right = not before_space and (not before_punctuation or after_space or after_punctuation)
	local can_open, can_close = left, right
	if mark == UNDERSCORE then
		can_open = left and (not right or before_punctuation)
		can_close = right and (not left or after_punctuation)
	end
	local node = text_node(sub(s, start, stop - 1))
	append(r.root, node)
	r.pos = stop
	if (can_open or can_close) and (mark ~= TILDE or count <= 2) then
		local delimiter = {
			mark = mark,
			count = count,
			original = count,
			node = node,
			previous = r.delimiters,
			next = nil,
			can_open = can_open,
			can_close = can_close,
		}
		if r.delimiters then
			r.delimiters.next = delimiter
		end
		r.delimiters = delimiter
	end
end

local function remove_delimiter(r, delimiter)
	if delimiter.previous then
		delimiter.previous.next = delimiter.next
	end
	if delimiter.next == nil then
		r.delimiters = delimiter.previous
	else
		delimiter.next.previous = delimiter.previous
	end
end

-- Makes emphasis, strong emphasis and struck text of the delimiters above `bottom` on the stack
-- (commonmark.js's `processEmphasis`, with GitHub's `~`).
local function emphasis(r, bottom)
	local openers_bottom = {}
	local closer = r.delimiters
	while closer ~= nil and closer.previous ~= bottom do
		closer = closer.previous
	end
	while closer ~= nil do
		if not closer.can_close then
			closer = closer.next
		else
			local mark = closer.mark
			local slot
			if mark == TILDE then
				slot = 1
			else
				slot = (mark == STAR and 8 or 2) + (closer.can_open and 3 or 0) + closer.original % 3
			end
			local floor = openers_bottom[slot]
			local opener = closer.previous
			local found = false
			while opener ~= nil and opener ~= bottom and (floor == nil or opener ~= floor) do
				if opener.mark == mark and opener.can_open then
					if mark == TILDE then
						found = true
						break
					end
					local odd = (closer.can_open or opener.can_close)
						and closer.original % 3 ~= 0
						and (opener.original + closer.original) % 3 == 0
					if not odd then
						found = true
						break
					end
				end
				opener = opener.previous
			end
			local old_closer = closer
			if mark == TILDE then
				if found then
					if opener.count == closer.count then
						wrap("strike", opener.node, closer.node)
						-- What was between them is inside it now.
						opener.next = closer
						closer.previous = opener
						unlink(opener.node)
						remove_delimiter(r, opener)
						unlink(closer.node)
					end
					local after = closer.next
					remove_delimiter(r, closer)
					closer = after
				else
					closer = closer.next
				end
			elseif not found then
				closer = closer.next
			else
				local used = (closer.count >= 2 and opener.count >= 2) and 2 or 1
				local opener_node, closer_node = opener.node, closer.node
				opener.count -= used
				closer.count -= used
				opener_node.text = sub(opener_node.text, 1, #opener_node.text - used)
				closer_node.text = sub(closer_node.text, 1, #closer_node.text - used)
				wrap(used == 1 and "emphasis" or "strong", opener_node, closer_node)
				if opener.next ~= closer then
					opener.next = closer
					closer.previous = opener
				end
				if opener.count == 0 then
					unlink(opener_node)
					remove_delimiter(r, opener)
				end
				if closer.count == 0 then
					unlink(closer_node)
					local after = closer.next
					remove_delimiter(r, closer)
					closer = after
				end
			end
			if not found then
				openers_bottom[slot] = old_closer.previous
				if not old_closer.can_open then
					remove_delimiter(r, old_closer)
				end
			end
		end
	end
	while r.delimiters ~= nil and r.delimiters ~= bottom do
		remove_delimiter(r, r.delimiters)
	end
end

-- An address after a `(` or a definition's `:`: in `<…>`, or a run of no spaces whose parentheses
-- are balanced. The address, encoded; nil for none.
local function destination(r)
	local s, start = r.subject, r.pos
	if byte(s, start) == LT then
		local at = start + 1
		while true do
			local code = byte(s, at)
			if code == nil or code == NEWLINE or code == LT then
				return nil
			elseif code == GT then
				break
			elseif code == BACKSLASH and byte(s, at + 1) ~= nil and byte(s, at + 1) ~= NEWLINE then
				at += 2
			else
				at += 1
			end
		end
		r.pos = at + 1
		return encode(entities.unescape(sub(s, start + 1, at - 1)))
	end
	local open = 0
	local at = start
	local code
	while true do
		code = byte(s, at)
		if code == nil then
			break
		elseif code == BACKSLASH and ESCAPABLE[byte(s, at + 1) or 0] then
			at += 2
		elseif code == OPEN_PAREN then
			at += 1
			open += 1
		elseif code == CLOSE_PAREN then
			if open < 1 then
				break
			end
			at += 1
			open -= 1
		elseif code == SPACE or (code >= 9 and code <= 13) then
			break
		else
			at += 1
		end
	end
	if (at == start and code ~= CLOSE_PAREN) or open ~= 0 then
		return nil
	end
	r.pos = at
	return encode(entities.unescape(sub(s, start, at - 1)))
end

-- A title in `"…"`, `'…'` or `(…)`, unescaped; nil for none.
local function title(r)
	local s, start = r.subject, r.pos
	local open = byte(s, start)
	local close
	if open == DQUOTE or open == SQUOTE then
		close = open
	elseif open == OPEN_PAREN then
		close = CLOSE_PAREN
	else
		return nil
	end
	local at = start + 1
	while true do
		local code = byte(s, at)
		if code == nil then
			return nil
		elseif code == BACKSLASH then
			if byte(s, at + 1) == nil then
				return nil
			end
			at += 2
		elseif code == close then
			break
		elseif open == OPEN_PAREN and code == OPEN_PAREN then
			return nil
		else
			at += 1
		end
	end
	r.pos = at + 1
	return entities.unescape(sub(s, start + 1, at - 1))
end

-- The length of a label (`[…]`, at most 999 characters, no bracket inside unless escaped) at the
-- reader's place, taken; 0 for none.
local function label(r)
	local s, start = r.subject, r.pos
	if byte(s, start) ~= OPEN_BRACKET then
		return 0
	end
	local at = start + 1
	local characters = 0
	while true do
		local code = byte(s, at)
		if code == nil or code == OPEN_BRACKET then
			return 0
		elseif code == CLOSE_BRACKET then
			break
		elseif code == BACKSLASH then
			if byte(s, at + 1) == nil then
				return 0
			end
			at += 2
		else
			at += 1
		end
		if code < 0x80 or code >= 0xC0 then
			characters += 1
		end
		if characters > 999 then
			return 0
		end
	end
	r.pos = at + 1
	return at - start + 1
end

local function add_bracket(r, node, index, image)
	if r.brackets then
		r.brackets.bracket_after = true
	end
	r.brackets = {
		node = node,
		previous = r.brackets,
		previous_delimiter = r.delimiters,
		index = index,
		image = image,
		active = true,
	}
end

-- The definition of a label, remembered as looked up (what was found, or false): the definitions
-- of a document can change while a block's inlines stay, and those that looked one up are read again.
local function look_up(r, raw)
	local key = normalize(raw)
	local found = r.refs[key]
	if r.lookups then
		r.lookups[key] = found or false
	end
	return found
end

-- The plain text of inlines, as an image's description is said: what they read, without their marks.
local function plain(node, out)
	local at = node._first
	while at do
		local kind = at.type
		if kind == "text" or kind == "code" or kind == "html" then
			out[#out + 1] = at.text
		elseif kind == "softbreak" or kind == "hardbreak" then
			out[#out + 1] = kind == "softbreak" and " " or "\n"
		elseif kind == "image" then
			out[#out + 1] = at.alt
		else
			plain(at, out)
		end
		at = at._next
	end
	return out
end

local function close_bracket(r)
	local s = r.subject
	r.pos += 1
	local start = r.pos
	local opener = r.brackets
	if opener == nil then
		append_text(r, "]")
		return
	end
	if not opener.active then
		append_text(r, "]")
		r.brackets = opener.previous
		return
	end
	local image = opener.image
	local href, link_title = nil, nil
	local matched = false
	local saved = r.pos
	if byte(s, r.pos) == OPEN_PAREN then
		r.pos += 1
		spnl(r)
		href = destination(r)
		if href ~= nil then
			spnl(r)
			local before = byte(s, r.pos - 1)
			if before == SPACE or before == TAB or before == NEWLINE then
				link_title = title(r)
			end
			spnl(r)
			if byte(s, r.pos) == CLOSE_PAREN then
				r.pos += 1
				matched = true
			end
		end
		if not matched then
			r.pos = saved
			link_title = nil
		end
	end
	if not matched then
		local before_label = r.pos
		local length = label(r)
		local raw = nil
		if length > 2 then
			raw = sub(s, before_label, before_label + length - 1)
		elseif not opener.bracket_after then
			raw = sub(s, opener.index, start - 1)
		end
		if length == 0 then
			r.pos = saved
		end
		if raw then
			local found = look_up(r, raw)
			if found then
				href, link_title = found.href, found.title
				matched = true
			end
		end
	end
	if not matched then
		r.brackets = opener.previous
		r.pos = start
		append_text(r, "]")
		return
	end
	local node = { type = image and "image" or "link" }
	local at = opener.node._next
	while at do
		local after = at._next
		unlink(at)
		append(node, at)
		at = after
	end
	append(r.root, node)
	emphasis(r, opener.previous_delimiter)
	r.brackets = opener.previous
	unlink(opener.node)
	if image then
		node.src = href
		node.alt = concat(plain(node, {}))
		node.title = link_title
		node._first, node._last = nil, nil
	else
		node.href = href
		node.title = link_title
		-- No link in a link: the brackets before it open none.
		local before = r.brackets
		while before do
			if not before.image then
				before.active = false
			end
			before = before.previous
		end
	end
end

-- An autolink (`<https://…>`, `<a@b.c>`), at a `<`.
local function autolink(r)
	local s, start = r.subject, r.pos
	local address = match(s, "^<([%w%.!#%$%%&'%*%+/=%?%^_`{|}~%-]+@[%w%-%.]+)>", start)
	if address then
		local domain = match(address, "@(.*)$")
		local ok = true
		for part in string.gmatch(domain .. ".", "([^%.]*)%.") do
			if #part == 0 or #part > 63 or byte(part, 1) == 45 or byte(part, -1) == 45 then
				ok = false
			end
		end
		if ok then
			local node = { type = "link", href = encode("mailto:" .. address) }
			append(node, text_node(address))
			append(r.root, node)
			r.pos = start + #address + 2
			return true
		end
	end
	local scheme, rest = match(s, "^<(%a[%w%+%.%-]*):([^%c <>]*)>", start)
	if scheme and #scheme >= 2 and #scheme <= 32 then
		local uri = scheme .. ":" .. rest
		local node = { type = "link", href = encode(uri) }
		append(node, text_node(uri))
		append(r.root, node)
		r.pos = start + #uri + 2
		return true
	end
	return false
end

local function less_than(r)
	if autolink(r) then
		return
	end
	local stop = html_at(r.subject, r.pos)
	if stop then
		append(r.root, { type = "html", text = sub(r.subject, r.pos, stop - 1) })
		r.pos = stop
		return
	end
	append_text(r, "<")
	r.pos += 1
end

local function entity(r)
	local value, stop = entities.at(r.subject, r.pos)
	if value then
		append_text(r, value)
		r.pos = stop
	else
		append_text(r, "&")
		r.pos += 1
	end
end

-- --- GitHub's links with no brackets ------------------------------------------------------------

local TRAILING = { [QUESTION] = true, [BANG] = true, [46] = true, [44] = true, [COLON] = true, [STAR] = true, [UNDERSCORE] = true, [TILDE] = true }

-- Where the link whose domain starts at byte `from` of `text` ends (it starts at `at`), or nil where
-- no link is there (GitHub's rules: a domain of parts apart by dots, a dot at least after `www.`,
-- no `_` in its last two parts, and what follows to a space or a `<`, less the punctuation at its
-- end and the `)`s past those it opened).
local function link_end(text, at, from, www)
	local domain_end = match(text, "^[%w_%-\128-\255]+()", from)
	if domain_end == nil then
		return nil
	end
	local parts = { sub(text, from, domain_end - 1) }
	while byte(text, domain_end) == 46 do
		local after = match(text, "^[%w_%-\128-\255]+()", domain_end + 1)
		if after == nil then
			break
		end
		parts[#parts + 1] = sub(text, domain_end + 1, after - 1)
		domain_end = after
	end
	if www and #parts < 2 then
		return nil
	end
	for k = math.max(#parts - 1, 1), #parts do
		if find(parts[k], "_", 1, true) then
			return nil
		end
	end
	local stop = (find(text, "[%s<]", domain_end) or #text + 1) - 1
	while stop >= domain_end do
		local last = byte(text, stop)
		if TRAILING[last] then
			stop -= 1
		elseif last == CLOSE_PAREN then
			local _, opens = string.gsub(sub(text, at, stop), "%(", "")
			local _, closes = string.gsub(sub(text, at, stop), "%)", "")
			if closes > opens then
				stop -= 1
			else
				break
			end
		elseif last == 59 then
			local reference = match(sub(text, at, stop), "&%w+;$")
			if reference then
				stop -= #reference
			else
				break
			end
		else
			break
		end
	end
	if stop < domain_end - 1 then
		stop = domain_end - 1
	end
	return stop
end

-- Where the address of mail around the `@` at byte `at` of `text` starts and ends (not before byte
-- `from`), or nil: letters, digits, `.`, `-`, `_` and `+` before it, and after it parts of letters,
-- digits, `-` and `_` apart by dots, a dot at least, not ending in `-` or `_` (a dot at the end is
-- not the address's).
local function mail_around(text, at, from)
	local start = at
	while start > from and find(text, "^[%w%.%-_%+]", start - 1) do
		start -= 1
	end
	if start == at then
		return nil
	end
	local stop, dots, here = at, 0, at + 1
	while true do
		local after = match(text, "^[%w%-_]+()", here)
		if after == nil then
			break
		end
		stop = after - 1
		if byte(text, after) == 46 and find(text, "^[%w%-_]", after + 1) then
			dots += 1
			here = after + 1
		else
			break
		end
	end
	local last = byte(text, stop)
	if stop == at or dots == 0 or last == 45 or last == UNDERSCORE then
		return nil
	end
	return start, stop
end

-- The links of a text that need no brackets (`www.…`, `http://…`, `https://…`, `ftp://…`, an
-- address of mail), as a list of text and link nodes; nil where it has none.
local function bare_links(words)
	if not (find(words, "www.", 1, true) or find(words, "://", 1, true) or find(words, "@", 1, true)) then
		return nil
	end
	local out = nil
	local from, search = 1, 1
	-- Where each kind is next found from `search` (false: nowhere), looked for again only once passed.
	local found = {}
	local function next_of(key, pattern, plain)
		local at = found[key]
		if at == nil or (at and at < search) then
			at = find(words, pattern, search, plain) or false
			found[key] = at
		end
		return at or nil
	end
	while true do
		local start, kind = nil, nil
		for _, candidate in
			ipairs({
				{ "www", next_of("www", "www.", true) },
				{ "web", next_of("web", "https?://", false) },
				{ "ftp", next_of("ftp", "ftp://", true) },
				{ "mail", next_of("mail", "@", true) },
			})
		do
			if candidate[2] and (start == nil or candidate[2] < start) then
				start, kind = candidate[2], candidate[1]
			end
		end
		if start == nil then
			break
		end
		local first, stop, href = nil, nil, nil
		if kind == "mail" then
			first, stop = mail_around(words, start, from)
			if first then
				href = "mailto:" .. sub(words, first, stop)
			end
		else
			local before = start > 1 and byte(words, start - 1) or nil
			if
				before == nil
				or before == SPACE
				or before == TAB
				or before == NEWLINE
				or before == STAR
				or before == UNDERSCORE
				or before == TILDE
				or before == OPEN_PAREN
			then
				local domain = kind == "www" and start or match(words, "^%a+://()", start)
				stop = link_end(words, start, domain, kind == "www")
				if stop then
					first = start
					href = sub(words, start, stop)
					if kind == "www" then
						href = "http://" .. href
					end
				end
			end
		end
		if first then
			out = out or {}
			if first > from then
				out[#out + 1] = text_node(sub(words, from, first - 1))
			end
			local address = sub(words, first, stop)
			out[#out + 1] = { type = "link", href = encode(href), children = { text_node(address) } }
			from, search = stop + 1, stop + 1
		else
			search = start + 1
		end
	end
	if out and from <= #words then
		out[#out + 1] = text_node(sub(words, from))
	end
	return out
end

-- --- The lists made at the end ------------------------------------------------------------------

-- The children of `parent` as a list: the links taken off, the texts next to one another made one
-- (and those left empty dropped), and GitHub's links found in the texts not in a link.
local function listed(parent, in_link)
	local out = {}
	local pending, count = nil, 0
	local at = parent._first
	while true do
		local kind = at and at.type
		if kind == "text" then
			if pending == nil then
				pending, count = { at.text }, 1
			else
				count += 1
				pending[count] = at.text
			end
		elseif pending then
			-- The texts before this node, made one.
			local words = count == 1 and pending[1] or concat(pending)
			pending = nil
			if words ~= "" then
				local found = not in_link and bare_links(words)
				if found then
					for _, node in ipairs(found) do
						out[#out + 1] = node
					end
				else
					out[#out + 1] = text_node(words)
				end
			end
		end
		if at == nil then
			break
		end
		local after = at._next
		at._prev, at._next, at._parent = nil, nil, nil
		if kind ~= "text" then
			if at._first ~= nil or kind == "emphasis" or kind == "strong" or kind == "strike" or kind == "link" then
				at.children = listed(at, in_link or kind == "link")
				at._first, at._last = nil, nil
			end
			out[#out + 1] = at
		end
		at = after
	end
	parent._first, parent._last = nil, nil
	return out
end

-- The inlines of `source` (a block's text, its ends trimmed): `refs` are the definitions of links
-- by their normalized label ({ href =, title = }); `lookups`, where given, is told each label
-- looked up (its definition, or false).
function inlines.parse(source, refs, lookups)
	local root = {}
	local r = {
		subject = source,
		pos = 1,
		length = #source,
		root = root,
		delimiters = nil,
		brackets = nil,
		refs = refs or {},
		lookups = lookups,
	}
	local s, length = source, #source
	while r.pos <= length do
		local at = r.pos
		local code = byte(s, at)
		if code == NEWLINE then
			newline(r)
		elseif code == BACKSLASH then
			backslash(r)
		elseif code == BACKTICK then
			backticks(r)
		elseif code == STAR or code == UNDERSCORE or code == TILDE then
			delimiters(r, code)
		elseif code == OPEN_BRACKET then
			local node = text_node("[")
			append(root, node)
			add_bracket(r, node, at, false)
			r.pos += 1
		elseif code == BANG then
			if byte(s, at + 1) == OPEN_BRACKET then
				local node = text_node("![")
				append(root, node)
				add_bracket(r, node, at + 1, true)
				r.pos += 2
			else
				append_text(r, "!")
				r.pos += 1
			end
		elseif code == CLOSE_BRACKET then
			close_bracket(r)
		elseif code == LT then
			less_than(r)
		elseif code == AMP then
			entity(r)
		else
			local stop = find(s, SPECIAL, at + 1) or length + 1
			append_text(r, sub(s, at, stop - 1))
			r.pos = stop
		end
	end
	emphasis(r, nil)
	return listed(root, false)
end

-- --- Definitions --------------------------------------------------------------------------------

-- The definition of a link at byte `at` of `text` (`[label]: address "title"`, which ends its
-- line): the byte after it and { label =, href =, title = } (the label normalized), or nil.
function inlines.reference(text, at)
	local r = { subject = text, pos = at }
	local length = label(r)
	if length == 0 then
		return nil
	end
	local raw = sub(text, at, at + length - 1)
	if byte(text, r.pos) ~= COLON then
		return nil
	end
	r.pos += 1
	spnl(r)
	local href = destination(r)
	if href == nil then
		return nil
	end
	local before_title = r.pos
	spnl(r)
	local found_title = nil
	if r.pos ~= before_title then
		found_title = title(r)
	end
	if found_title == nil then
		r.pos = before_title
	end
	local line_end = match(text, "^[ \t]*()", r.pos)
	local at_end = byte(text, line_end) == nil or byte(text, line_end) == NEWLINE
	if not at_end then
		if found_title == nil then
			return nil
		end
		-- The title is not at the end of its line: a definition may still end with the address.
		found_title = nil
		r.pos = before_title
		line_end = match(text, "^[ \t]*()", r.pos)
		if not (byte(text, line_end) == nil or byte(text, line_end) == NEWLINE) then
			return nil
		end
	end
	local key = normalize(raw)
	if key == "" then
		return nil
	end
	if byte(text, line_end) == NEWLINE then
		line_end += 1
	end
	return line_end, { label = key, href = href, title = found_title }
end

return inlines
