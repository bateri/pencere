-- The page API, part 5: a language that colours an editor as it changes.
--
-- The editor knows no language: the page colours its text with decorations (a layer of them,
-- `decorate`). `pencere.Syntax` is how a page does that the way CodeMirror's `StreamLanguage`
-- does, with its legacy modes: a language is a function that reads a line a token at a time
-- from a stream (`token(stream, state) -> name`) and a state carried from line to line (a block
-- comment, a string that goes on). The kit keeps the state at the end of every line, reads again
-- only from the first line a change touched until the state comes to what it was, colours only
-- the lines in view (and a margin), and does a bounded amount of work in each call, the rest in
-- the frames after it, by itself (a timer, `pencere.time`): a large document is coloured to its
-- end with no event of the person's. It does nothing for a page that does not use it.
--
--   local sql = pencere.Syntax.define {
--       name = "sql",
--       start = function() return { comment = false } end,
--       token = function(stream, state)
--           if state.comment then
--               if stream:skip_to("*/") then stream:match("%*/"); state.comment = false
--               else stream:skip_to_end() end
--               return "comment"
--           end
--           if stream:eat_space() then return nil end
--           if stream:match("/%*") then state.comment = true; return "comment" end
--           if stream:match("%-%-") then stream:skip_to_end(); return "comment" end
--           if stream:match("%d+") then return "number" end
--           stream:next()
--       end,
--       styles = { comment = { color = "dim", italic = true }, number = { color = "warning" } },
--   }
--   local syntax = pencere.Syntax.new(sql, "editor")
--
--   handle = function(event) syntax:handle(event); return syntax:reply(respond(event)) end
--
-- A place the stream says (`pos`, `start`) is a byte of the line's text from 1, as `string.find`
-- counts; the kit makes characters of them for the editor.

local layout = pencere.layout
local merge = pencere._merge

-- A line is read to its first this many characters: a line of a minified file does not stall the
-- page, and the state after it is the state at that character (CodeMirror's `MaxLineLength`). A
-- line so has at most this many decorations, a character each.
local LINE_CHARACTERS = 10000
-- How much text is read from the editor at a time, characters (more than a line is read to).
local CHUNK_CHARACTERS = 32768
-- The most decorations a syntax holds for the page before they are handed out: half of what one
-- reply may hand the editors (20 000), so that two syntaxes fit in one reply. A line whose
-- decorations would take it past this waits for those it holds to be handed out; a line has at
-- most LINE_CHARACTERS of them, no more than this, so it always fits where none are held.
local PENDING_DECORATIONS = 10000
-- How much one call tokenises unless the page says (`step`), bytes of text. A language of a few
-- patterns a token reads 20 000 bytes (some 450 lines of code) in about 30 ms on a fast machine:
-- well within the time a call has (250 ms) on a slow one, with the page's own work beside it.
local STEP = 20000
-- The most names of tokens whose looks a language remembers.
local NAMES_KEPT = 1024

local LOOK_KEYS = { color = true, background = true, weight = true, italic = true, underline = true, strike = true }
local OPTION_KEYS = { layer = true, step = true, margin = true, tab_width = true }
local SPEC_KEYS = { name = true, start = true, token = true, copy = true, equal = true, blank = true, styles = true }

-- --- States ------------------------------------------------------------------------------------

-- A copy of `value`, tables all the way down (a table met twice is copied once, with its metatable).
local function deep_copy(value, seen)
	if type(value) ~= "table" then
		return value
	end
	seen = seen or {}
	local done = seen[value]
	if done then
		return done
	end
	local copy = {}
	seen[value] = copy
	for key, item in next, value do
		copy[deep_copy(key, seen)] = deep_copy(item, seen)
	end
	return setmetatable(copy, getmetatable(value))
end

-- Whether `a` and `b` are the same, tables by what they hold.
local function deep_equal(a, b, seen)
	if a == b then
		return true
	end
	if type(a) ~= "table" or type(b) ~= "table" then
		return false
	end
	seen = seen or {}
	local pairs_of = seen[a]
	if pairs_of and pairs_of[b] then
		return true
	end
	if not pairs_of then
		pairs_of = {}
		seen[a] = pairs_of
	end
	pairs_of[b] = true
	for key, item in next, a do
		if not deep_equal(item, rawget(b, key), seen) then
			return false
		end
	end
	for key in next, b do
		if rawget(a, key) == nil then
			return false
		end
	end
	return true
end

-- --- Sets of lines -----------------------------------------------------------------------------

-- A set of lines is a list of ranges { from, to } (the end not in it), in order, apart. The sets
-- the kit keeps have a few ranges each, so each change makes a new list.

local function set_add(set, from, to)
	if from >= to then
		return set
	end
	local out = {}
	local placed = false
	for _, range in ipairs(set) do
		if range[2] < from then
			out[#out + 1] = range
		elseif range[1] > to then
			if not placed then
				out[#out + 1] = { from, to }
				placed = true
			end
			out[#out + 1] = range
		else
			from, to = math.min(from, range[1]), math.max(to, range[2])
		end
	end
	if not placed then
		out[#out + 1] = { from, to }
	end
	return out
end

local function set_remove(set, from, to)
	if from >= to then
		return set
	end
	local out = {}
	for _, range in ipairs(set) do
		if range[2] <= from or range[1] >= to then
			out[#out + 1] = range
		else
			if range[1] < from then
				out[#out + 1] = { range[1], from }
			end
			if range[2] > to then
				out[#out + 1] = { to, range[2] }
			end
		end
	end
	return out
end

-- The set with lines `first` to `last_before` (of the text before a change) taken out, and the
-- lines after them moved by `delta`.
local function set_edit(set, first, last_before, delta)
	local out = {}
	for _, range in ipairs(set_remove(set, first, last_before + 1)) do
		if range[1] > last_before then
			out[#out + 1] = { range[1] + delta, range[2] + delta }
		else
			out[#out + 1] = range
		end
	end
	return out
end

-- Where the range of `set` that holds `line` ends; nil where none does.
local function set_end(set, line)
	local low, high = 1, #set
	while low <= high do
		local middle = (low + high) // 2
		local range = set[middle]
		if line < range[1] then
			high = middle - 1
		elseif line >= range[2] then
			low = middle + 1
		else
			return range[2]
		end
	end
	return nil
end

-- The first line from `from` to `to` (the end not in it) that `set` has not; nil where it has all.
local function set_first_missing(set, from, to)
	local line = from
	for _, range in ipairs(set) do
		if range[1] > line then
			break
		elseif range[2] > line then
			line = range[2]
		end
	end
	if line < to then
		return line
	end
	return nil
end

local function set_clip(set, from, to)
	local out = {}
	for _, range in ipairs(set) do
		local a, b = math.max(range[1], from), math.min(range[2], to)
		if a < b then
			out[#out + 1] = { a, b }
		end
	end
	return out
end

-- --- The stream --------------------------------------------------------------------------------

-- A line of text as a language reads it, CodeMirror's `StringStream`: `string` is the line (its
-- line break and a `\r` before it left out), `pos` where the next token starts reading and `start`
-- where the one being read started, bytes from 1. The token is `string:sub(start, pos - 1)`
-- (`current()`). A character is a whole UTF-8 sequence.
local Stream = {}
Stream.__index = Stream

-- One UTF-8 character at the start of what it is matched against.
local CHARACTER = "^" .. utf8.charpattern

local function stream_new(text, tab_width)
	return setmetatable(
		{ string = text, pos = 1, start = 1, tab_width = tab_width, seen_pos = 1, seen_column = 0 },
		Stream
	)
end

-- The columns from byte `from` to before byte `to` of `text`, a tab to its next stop, starting at
-- column `column`.
local function columns(text, from, to, column, tab_width)
	for at = from, to - 1 do
		local byte = string.byte(text, at)
		if byte == 9 then
			column += tab_width - column % tab_width
		elseif byte < 0x80 or byte >= 0xC0 then
			column += 1
		end
	end
	return column
end

-- Whether the stream is at the start of the line, and at its end.
function Stream:sol()
	return self.pos == 1
end

function Stream:eol()
	return self.pos > #self.string
end

-- The next character, not taken; nil at the end of the line.
function Stream:peek()
	local text, pos = self.string, self.pos
	local byte = string.byte(text, pos)
	if byte == nil then
		return nil
	elseif byte < 0x80 then
		return string.sub(text, pos, pos)
	end
	return string.match(text, CHARACTER, pos) or string.sub(text, pos, pos)
end

-- The next character, taken; nil at the end of the line.
function Stream:next()
	local character = self:peek()
	if character then
		self.pos += #character
	end
	return character
end

-- The patterns `eat` matches one character against, by what was given.
local whole = {}

-- Keeps `value` for `key` in `cache`, while it holds fewer than a language's names: patterns are
-- the language's, and one that makes them of the text it reads is not kept without end.
local function remember(cache, key, value)
	local count = cache[0] or 0
	if count < NAMES_KEPT then
		cache[key] = value
		cache[0] = count + 1
	end
	return value
end

-- Whether `character` is what `match` says: one character is itself (`"."` is a dot), a longer
-- string a pattern the whole character matches (`"%d"`, `"[%w_]"`), a function says yes or no.
local function is(character, match)
	local kind = type(match)
	if kind == "string" then
		if character == match then
			return true
		end
		if utf8.len(match) == 1 then
			return false
		end
		local pattern = whole[match]
		if pattern == nil then
			pattern = "^" .. match .. "$"
			remember(whole, match, pattern)
		end
		return string.find(character, pattern) ~= nil
	elseif kind == "function" then
		return match(character) and true or false
	end
	error("eat takes a character, a pattern of one character or a function, not a " .. kind, 3)
end

-- The next character, taken where it is what `match` says (see `is`); nil, and nothing taken,
-- where it is not.
function Stream:eat(match)
	local character = self:peek()
	if character and is(character, match) then
		self.pos += #character
		return character
	end
	return nil
end

-- Takes characters while they are what `match` says. Whether it took any.
function Stream:eat_while(match)
	local start = self.pos
	while true do
		local character = self:peek()
		if character == nil or not is(character, match) then
			break
		end
		self.pos += #character
	end
	return self.pos > start
end

-- Takes the spaces and tabs (and a no-break space) that come next. Whether it took any.
function Stream:eat_space()
	local text, start = self.string, self.pos
	while true do
		local _, last = string.find(text, "^[ \t\v\f\r]+", self.pos)
		if last then
			self.pos = last + 1
		elseif string.byte(text, self.pos) == 0xC2 and string.byte(text, self.pos + 1) == 0xA0 then
			self.pos += 2
		else
			break
		end
	end
	return self.pos > start
end

-- Takes the rest of the line.
function Stream:skip_to_end()
	self.pos = #self.string + 1
end

-- Moves to the next `text` on the line (before it), where there is one: whether there was.
function Stream:skip_to(text)
	local at = string.find(self.string, text, self.pos, true)
	if at then
		self.pos = at
		return true
	end
	return false
end

-- Gives back the last `count` characters taken.
function Stream:backup(count)
	if count <= 0 then
		return
	end
	local at = utf8.offset(self.string, -count, self.pos)
	if at == nil or at < 1 then
		error("backup goes back past the start of the line", 2)
	end
	self.pos = at
end

-- The column the token starts at, from 0, a tab to its next stop.
function Stream:column()
	local from, column = 1, 0
	if self.seen_pos <= self.start then
		from, column = self.seen_pos, self.seen_column
	end
	column = columns(self.string, from, self.start, column, self.tab_width)
	self.seen_pos, self.seen_column = self.start, column
	return column
end

-- How far in the line's text starts, in columns.
function Stream:indentation()
	local _, last = string.find(self.string, "^[ \t]*")
	return columns(self.string, 1, last + 1, 0, self.tab_width)
end

-- The patterns `match` reads at `pos`, by what was given, and the same in either case.
local anchored = {}
local caseless = {}

-- `pattern` matching a letter in either case: each letter outside a `%` escape is the set of both
-- (in a set of its own, both are put in it).
local function either_case(pattern)
	local out = {}
	local at, length = 1, #pattern
	local in_set, set_start = false, 0
	while at <= length do
		local c = string.sub(pattern, at, at)
		if c == "%" then
			out[#out + 1] = string.sub(pattern, at, at + 1)
			at += 2
		else
			if in_set then
				local to = string.sub(pattern, at + 2, at + 2)
				if c == "]" and at > set_start then
					in_set = false
					out[#out + 1] = c
				elseif
					string.match(c, "%a")
					and string.sub(pattern, at + 1, at + 1) == "-"
					and string.match(to, "%a")
				then
					-- A range of letters is the range in both cases.
					out[#out + 1] = string.lower(c)
						.. "-"
						.. string.lower(to)
						.. string.upper(c)
						.. "-"
						.. string.upper(to)
					at += 2
				elseif string.match(c, "%a") then
					out[#out + 1] = string.lower(c) .. string.upper(c)
				else
					out[#out + 1] = c
				end
			elseif c == "[" then
				in_set = true
				-- A `]` first in the set (after a `^`) is one of its characters.
				set_start = at + 1
				if string.sub(pattern, at + 1, at + 1) == "^" then
					set_start += 1
					out[#out + 1] = "[^"
					at += 1
				else
					out[#out + 1] = c
				end
			elseif string.match(c, "%a") then
				out[#out + 1] = "[" .. string.lower(c) .. string.upper(c) .. "]"
			else
				out[#out + 1] = c
			end
			at += 1
		end
	end
	return table.concat(out)
end

local function matched(stream, consume, first, last, ...)
	if first == nil then
		return nil
	end
	if consume ~= false then
		stream.pos = last + 1
	end
	if select("#", ...) > 0 then
		return ...
	end
	return string.sub(stream.string, first, last)
end

-- What `pattern` (a Lua pattern; a string with none of `^$()%.[]*+-?` in it is itself) matches
-- where the stream is: the text matched, or its captures; nil where it does not match there. It is
-- taken unless `consume` is false; `case_insensitive` matches the letters of the pattern in either
-- case (ASCII's).
function Stream:match(pattern, consume, case_insensitive)
	if type(pattern) ~= "string" then
		error("match takes a pattern (a string), not a " .. type(pattern), 2)
	end
	local key = pattern
	if case_insensitive then
		key = caseless[pattern]
		if key == nil then
			key = either_case(pattern)
			remember(caseless, pattern, key)
		end
	end
	local read = anchored[key]
	if read == nil then
		read = string.sub(key, 1, 1) == "^" and key or "^" .. key
		remember(anchored, key, read)
	end
	return matched(self, consume, string.find(self.string, read, self.pos))
end

-- The token being read: from `start` to before `pos`.
function Stream:current()
	return string.sub(self.string, self.start, self.pos - 1)
end

-- --- Languages ---------------------------------------------------------------------------------

local Language = {}
Language.__index = Language

local Syntax = {}
Syntax.__index = Syntax
pencere.Syntax = Syntax

-- The look of the token named `name` in `language` (false for none): the looks of its names (a name
-- is several, apart: "keyword strong"), each found by itself or by what is before its last dot
-- ("string.special" is "string" where the styles do not say it), the later over the earlier.
local function look_of(language, name)
	local looks = language.looks
	local look = looks[name]
	if look ~= nil then
		return look
	end
	local styles = language.styles
	local merged = nil
	for part in string.gmatch(name, "%S+") do
		local found = styles[part]
		while found == nil do
			part = string.match(part, "^(.*)%.[^%.]*$")
			if part == nil then
				break
			end
			found = styles[part]
		end
		if found then
			merged = merged or {}
			for key, value in pairs(found) do
				merged[key] = value
			end
		end
	end
	look = merged or false
	if language.kept < NAMES_KEPT then
		looks[name] = look
		language.kept += 1
	end
	return look
end

-- Defines a language. `spec` is:
--   name     what it is called (the words of its errors say it)
--   start()  the state before the first line: what the language carries from line to line
--   token(stream, state)  reads one token from the stream (at least a character) and says the
--            name of its style, or nil for none; it changes the state where the token does
--   copy(state)       (optional) a copy the token may change and the kept state not: by default a
--                     copy of the tables all the way down
--   equal(a, b)       (optional) whether two states are the same, which is where reading again
--                     after a change stops: by default the tables by what they hold
--   blank(state)      (optional) a line with nothing on it went by
--   styles   { [name] = look }: a look is what a decoration of the editor says (`color`,
--            `background`, `weight`, `italic`, `underline`, `strike`). A token's name may be
--            several ("keyword strong", the looks over one another) and a dotted name falls back
--            to what is before its last dot ("string.special" to "string"), so one table of styles
--            serves many languages; a name no style says is not coloured.
function Syntax.define(spec)
	if type(spec) ~= "table" then
		error("pencere.Syntax.define takes a table: { name =, start =, token =, styles = }", 2)
	end
	if type(spec.name) ~= "string" then
		error("a language has a name (a string)", 2)
	end
	local name = spec.name
	for _, key in ipairs({ "start", "token" }) do
		if type(spec[key]) ~= "function" then
			error("the language " .. name .. " has no " .. key .. " function", 2)
		end
	end
	for _, key in ipairs({ "copy", "equal", "blank" }) do
		if spec[key] ~= nil and type(spec[key]) ~= "function" then
			error("the " .. key .. " of the language " .. name .. " is a function, not a " .. type(spec[key]), 2)
		end
	end
	for key in pairs(spec) do
		if not SPEC_KEYS[key] then
			error("a language has no " .. tostring(key) .. " (name, start, token, copy, equal, blank, styles)", 2)
		end
	end
	local styles = {}
	if spec.styles ~= nil then
		if type(spec.styles) ~= "table" then
			error("the styles of the language " .. name .. " are a table of looks by name", 2)
		end
		for style, look in pairs(spec.styles) do
			if type(look) ~= "table" then
				error(
					"the style "
						.. tostring(style)
						.. " of the language "
						.. name
						.. " is a look (a table), not a "
						.. type(look),
					2
				)
			end
			for key in pairs(look) do
				if not LOOK_KEYS[key] then
					error(
						"the style "
							.. tostring(style)
							.. " of the language "
							.. name
							.. " says "
							.. tostring(key)
							.. ": a look is color, background, weight, italic, underline and strike",
						2
					)
				end
			end
			styles[style] = table.clone(look)
		end
	end
	return setmetatable({
		name = name,
		start = spec.start,
		token = spec.token,
		copy = spec.copy or deep_copy,
		equal = spec.equal or deep_equal,
		blank = spec.blank,
		styles = styles,
		looks = {},
		kept = 0,
	}, Language)
end

-- Reads one line: the stream holds its text, `state` is the state before it (a copy the token may
-- change, made here), `out` where the tokens with a name go (the byte each starts at, the byte after
-- it, its name), or nil. The state after the line. Run under `pcall`.
local function read_line(language, stream, before, out)
	local state = language.copy(before)
	local text = stream.string
	local length = #text
	if length == 0 then
		if language.blank then
			language.blank(state)
		end
		return state
	end
	local token = language.token
	while stream.pos <= length do
		local start = stream.pos
		stream.start = start
		local name = token(stream, state)
		if stream.pos <= start then
			stream.pos = start
			error(
				"token read nothing at column "
					.. ((utf8.len(text, 1, start - 1) or start - 1) + 1)
					.. " (each call reads at least one character)",
				0
			)
		end
		if name ~= nil then
			if type(name) ~= "string" then
				error("token says the name of a style, or nil, not a " .. type(name), 0)
			end
			if out then
				local n = #out
				out[n + 1], out[n + 2], out[n + 3] = start, stream.pos, name
			end
		end
	end
	return state
end

-- The characters from byte `from` to byte `to` of `text` (a token's end may cut a character a
-- pattern took a byte of: the bytes that start one are counted).
local function characters(text, from, to)
	if to < from then
		return 0
	end
	local count = utf8.len(text, from, to)
	if count then
		return count
	end
	count = 0
	for at = from, to do
		local byte = string.byte(text, at)
		if byte < 0x80 or byte >= 0xC0 then
			count += 1
		end
	end
	return count
end

-- The text of a line as the language reads it: its `\r` left out, and no longer than it reads.
local function as_read(text)
	local length = #text
	if length > 0 and string.byte(text, length) == 13 then
		text = string.sub(text, 1, length - 1)
		length -= 1
	end
	if length > LINE_CHARACTERS then
		local cut = utf8.offset(text, LINE_CHARACTERS + 1)
		if cut then
			text = string.sub(text, 1, cut - 1)
		end
	end
	return text
end

-- The tokens of one line of text, as a page colours a text that is not in an editor (a block of
-- code in a note): { { from =, to =, name = }, ... } in characters from 0 (the end not in it), the
-- tokens with no name left out, and the state after the line. `state` is the state before it (the
-- language's `start()` where nil), and is not changed; `tab_width` is what a tab is to the
-- stream's columns (4).
function Language:tokens(text, state, tab_width)
	if type(text) ~= "string" then
		error("tokens takes a line of text, not a " .. type(text), 2)
	end
	text = as_read(text)
	if state == nil then
		state = self.start()
	end
	local stream = stream_new(text, tab_width or 4)
	local out = {}
	local ok, after = pcall(read_line, self, stream, state, out)
	if not ok then
		error(self.name .. ": " .. tostring(after), 2)
	end
	local tokens = {}
	local at_byte, at_char = 1, 0
	for k = 1, #out, 3 do
		local from = at_char + characters(text, at_byte, out[k] - 1)
		local to = from + characters(text, out[k], out[k + 1] - 1)
		at_byte, at_char = out[k + 1], to
		tokens[#tokens + 1] = { from = from, to = to, name = out[k + 2] }
	end
	return tokens, after
end

-- --- A language on an editor ---------------------------------------------------------------------

-- Colours the editor named `editor` with `language`: `options` are `layer` (the layer of its
-- decorations, "syntax"), `step` (how much text one call reads, bytes: 20 000), `margin` (the lines
-- coloured above and below those in view: as many as are in view) and `tab_width` (what a tab is to
-- a stream's columns: 4).
--
-- It keeps the state at the end of each line, read from the start of the document, and the lines
-- whose colours the editor has. The page gives it every event (`handle`): an `edit` of the editor
-- makes it read again from the first line the change touched, until a line ends in the state it
-- ended in before (the lines after it are as they were); a `viewport` says what to colour. What it
-- asks of the editor (`decorate`, the lines in view and the margin, in place of what the layer had
-- there; and nothing for the lines out of it) goes out in the page's reply (`reply`, `asks()`).
--
-- One call reads at most `step` bytes, the lines to colour first. A document far longer than that
-- above the lines in view is coloured from a guess (the language's `start()` at the first line to
-- colour, as CodeMirror does) and read from where it knows in the calls after, which colour again
-- what the guess got wrong. While there is more to do (`busy()`) it goes on by itself, a call a
-- frame: a timer of its own (`pencere.time.after(0, ...)`) wakes the page, does the next call and
-- hands the editor what it found with the wake's reply. The page does nothing for it but give it
-- its events.
function Syntax.new(language, editor, options)
	if getmetatable(language) ~= Language then
		error("pencere.Syntax.new takes a language (pencere.Syntax.define), then the name of an editor", 2)
	end
	if editor == nil then
		error("pencere.Syntax.new takes a language, then the name of an editor", 2)
	end
	options = options or {}
	if type(options) ~= "table" then
		error("the options of pencere.Syntax.new are a table: { layer =, step =, margin =, tab_width = }", 2)
	end
	for key in pairs(options) do
		if not OPTION_KEYS[key] then
			error("pencere.Syntax.new has no option " .. tostring(key) .. " (layer, step, margin, tab_width)", 2)
		end
	end
	local layer = options.layer or "syntax"
	if type(layer) ~= "string" then
		error("the layer of a syntax is a name (a string), not a " .. type(layer), 2)
	end
	local step = options.step or STEP
	if type(step) ~= "number" or step < 1 then
		error("the step of a syntax is how much text a call reads, bytes (1 or more)", 2)
	end
	local margin = options.margin
	if margin ~= nil and (type(margin) ~= "number" or margin < 0) then
		error("the margin of a syntax is a number of lines (0 or more)", 2)
	end
	local tab_width = options.tab_width or 4
	if type(tab_width) ~= "number" or tab_width < 1 then
		error("the tab_width of a syntax is a number of columns (1 or more)", 2)
	end
	return setmetatable({
		language = language,
		editor = editor,
		layer = layer,
		step = step,
		margin = margin,
		stream = stream_new("", tab_width),
		-- The text it knows: its version, lines and length (nil before the editor told anything).
		version = nil,
		lines = 0,
		length = 0,
		-- The changes told past it, while the text read is of a version later still: the
		-- version and length they came to, and how much of the start and the end of the text
		-- none of them touched.
		told = nil,
		told_length = 0,
		batch = nil,
		-- `ends[k]` is the state before line k (after line k - 1), for k from 1. The lines before
		-- `valid` have theirs right. `ok` is the lines from `valid` on whose state after them was
		-- read from the one before them and their text as it is: where a line read again ends as
		-- it did, the lines after it that are `ok` are right as well.
		ends = {},
		valid = 0,
		ok = {},
		-- The lines in view ({ first, last }, as the editor said), the lines whose colours the
		-- editor has (`painted`) and the lines the layer may have any in (`held`, { from, to }).
		view = nil,
		painted = {},
		held = nil,
		-- What is asked and not yet handed out, and how many decorations it has.
		pending = {},
		pending_count = 0,
		-- The lines of the text read last, and the tokens of a line.
		chunk = nil,
		out = {},
		-- The lines being coloured: { first, last, decorations } for one ask; the decorations of the
		-- line being coloured; and whether a line waits for what is held to be handed out.
		segment = nil,
		made = {},
		full = false,
		-- The timer it goes on with while it is busy (nil while none waits).
		timer = nil,
	}, Syntax)
end

-- Starts afresh from the text as it is read now: nothing known, nothing coloured.
local function reset(self)
	local id = self.editor
	local version = layout.editor_version(id)
	self.version = version
	self.told = version
	self.lines = version and layout.editor_lines(id) or 0
	self.length = version and layout.editor_length(id) or 0
	self.told_length = self.length
	self.batch = nil
	self.ends, self.valid, self.ok = {}, 0, {}
	self.painted = {}
	self.chunk = nil
end

-- --- Reading the editor's text -------------------------------------------------------------------

-- Reads lines from `line` on, a chunk of the text at a time.
local function fill(self, line)
	local id = self.editor
	local from = layout.line_start(id, line) or 0
	local to = math.min(from + CHUNK_CHARACTERS, self.length)
	local text = layout.editor_text(id, from, to) or ""
	local starts, stops = {}, {}
	local at = 1
	while true do
		local newline = string.find(text, "\n", at, true)
		if newline then
			starts[#starts + 1], stops[#stops + 1] = at, newline - 1
			at = newline + 1
		else
			-- The end of the document ends a line; the end of a chunk does not, but where it is
			-- the whole chunk it is a line longer than the language reads.
			if to >= self.length or #starts == 0 then
				starts[#starts + 1], stops[#stops + 1] = at, #text
			end
			break
		end
	end
	self.chunk = { version = self.version, first = line, count = #starts, text = text, starts = starts, stops = stops }
end

local function line_text(self, line)
	local chunk = self.chunk
	if chunk == nil or chunk.version ~= self.version or line < chunk.first or line >= chunk.first + chunk.count then
		fill(self, line)
		chunk = self.chunk
	end
	local k = line - chunk.first + 1
	return string.sub(chunk.text, chunk.starts[k], chunk.stops[k])
end

-- Where line `line` ends: the place of its line break, or the end of the document.
local function line_end(self, line)
	if line + 1 < self.lines then
		return (layout.line_start(self.editor, line + 1) or self.length + 1) - 1
	end
	return self.length
end

-- --- Reading and colouring -----------------------------------------------------------------------

-- Reads line `line` from the state `before`: the state after it, and what reading it cost. Where
-- `paint`, its tokens are in `self.out` and its text is the second value.
local function read(self, line, before, paint)
	local text = as_read(line_text(self, line))
	local stream = self.stream
	stream.string, stream.pos, stream.start, stream.seen_pos, stream.seen_column = text, 1, 1, 1, 0
	local out = nil
	if paint then
		out = self.out
		table.clear(out)
	end
	local language = self.language
	local ok, after = pcall(read_line, language, stream, before, out)
	if not ok then
		error(language.name .. ": line " .. (line + 1) .. ": " .. tostring(after), 0)
	end
	return after, text, #text + 1
end

-- What is asked, handed out with the next reply: an ask in place of one it covers of the same
-- version.
local function ask(self, request)
	local kept = {}
	local count = 0
	for _, earlier in ipairs(self.pending) do
		if
			earlier.decorate == request.decorate
			and earlier.version == request.version
			and earlier.from >= request.from
			and earlier.to <= request.to
		then
			continue
		end
		kept[#kept + 1] = earlier
		count += #earlier.decorations
	end
	kept[#kept + 1] = request
	self.pending = kept
	self.pending_count = count + #request.decorations
end

-- The lines being coloured go to the editor as one ask.
local function flush(self)
	local segment = self.segment
	if segment == nil then
		return
	end
	self.segment = nil
	ask(self, {
		decorate = self.layer,
		version = self.version,
		from = layout.line_start(self.editor, segment[1]) or 0,
		to = line_end(self, segment[2]),
		decorations = segment[3],
	})
end

-- Line `line` coloured with the tokens in `self.out` (its text `text`). False, and nothing of it
-- coloured, where its decorations would take what is held past PENDING_DECORATIONS: it waits for
-- them to be handed out (`full`).
local function paint(self, line, text)
	local out = self.out
	local language = self.language
	local base = layout.line_start(self.editor, line) or 0
	local made = self.made
	table.clear(made)
	local ascii = string.find(text, "[\128-\255]") == nil
	local at_byte, at_char = 1, 0
	local last, last_look = nil, nil
	for k = 1, #out, 3 do
		local look = look_of(language, out[k + 2])
		if look then
			local from_byte, to_byte = out[k], out[k + 1]
			local from, to
			if ascii then
				from, to = from_byte - 1, to_byte - 1
			else
				from = at_char + characters(text, at_byte, from_byte - 1)
				to = from + characters(text, from_byte, to_byte - 1)
				at_byte, at_char = to_byte, to
			end
			from += base
			to += base
			if last and last_look == look and last.to == from then
				last.to = to
			else
				last = table.clone(look)
				last.from, last.to = from, to
				last_look = look
				made[#made + 1] = last
			end
		end
	end
	local segment = self.segment
	local holding = self.pending_count + (segment and #segment[3] or 0)
	if holding > 0 and holding + #made > PENDING_DECORATIONS then
		self.full = true
		return false
	end
	if segment == nil or segment[2] ~= line - 1 then
		flush(self)
		segment = { line, line, {} }
		self.segment = segment
	end
	segment[2] = line
	table.move(made, 1, #made, #segment[3] + 1, segment[3])
	self.painted = set_add(self.painted, line, line + 1)
	return true
end

-- The window of lines to colour: those in view and the margin, within the document.
local function window(self)
	local view = self.view
	if view == nil or self.lines == 0 then
		return nil
	end
	local first, last = view[1], view[2]
	if last < first then
		return nil
	end
	local margin = self.margin or (last - first + 1)
	return math.max(first - margin, 0), math.min(last + margin, self.lines - 1)
end

-- Whether line `line` is to be coloured: in the window and not coloured.
local function wanted(self, line, first, last)
	return line >= first and line <= last and set_end(self.painted, line) == nil
end

-- Reads the line at `valid` (whose state before it is right), colouring it where it is wanted.
-- What it cost.
local function advance(self, first, last)
	local line = self.valid
	local ends = self.ends
	local before = line > 0 and ends[line] or self.language.start()
	local colour = wanted(self, line, first, last)
	local after, text, cost = read(self, line, before, colour)
	if colour then
		paint(self, line, text)
	end
	local equal = self.language.equal
	local old = ends[line + 1]
	local same = old ~= nil and equal(after, old)
	local run = old ~= nil and set_end(self.ok, line + 1)
	if run and same then
		-- The lines after it read from the state they read from before: they are as they were.
		self.valid = run
	else
		if not same then
			-- The next line starts from another state: its colours are not right, nor what was
			-- read from it.
			if set_end(self.painted, line + 1) then
				self.painted = set_remove(self.painted, line + 1, line + 2)
			end
			if run then
				self.ok = set_remove(self.ok, line + 1, line + 2)
			end
			-- A state the same as the one before it is kept once.
			ends[line + 1] = equal(after, before) and before or after
		end
		self.valid = line + 1
	end
	local ok = self.ok
	if #ok > 0 and ok[1][1] < self.valid then
		self.ok = set_remove(ok, 0, self.valid)
	end
	return cost
end

-- Colours line `line`, after `valid`, from the state kept before it (read from a line that was
-- not right, or a guess): what the calls after find of it colours it again. What it cost.
local function paint_ahead(self, line)
	local ends = self.ends
	local before = ends[line]
	local after, text, cost = read(self, line, before, true)
	paint(self, line, text)
	local old = ends[line + 1]
	local same = old ~= nil and self.language.equal(after, old)
	if not same then
		ends[line + 1] = after
		self.ok = set_remove(self.ok, line + 1, line + 2)
		self.painted = set_remove(self.painted, line + 1, line + 2)
	end
	self.ok = set_add(self.ok, line, line + 1)
	return cost
end

-- Colours line `line` (after `valid`, and wanted) from what is right before it.
local function paint_known(self, line)
	local before = line == 0 and self.language.start() or self.ends[line]
	local _, text, cost = read(self, line, before, true)
	paint(self, line, text)
	return cost
end

-- The characters from the start of line `a` to the start of line `b`.
local function span(self, a, b)
	local id = self.editor
	local from = layout.line_start(id, a) or 0
	local to = b < self.lines and (layout.line_start(id, b) or self.length) or self.length
	return to - from
end

local function ready(self)
	return self.version ~= nil
		and self.batch == nil
		and layout.editor_version(self.editor) == self.version
		and window(self) ~= nil
end

-- The layer has decorations of lines out of the window: they are taken away.
local function forget_outside(self, first, last)
	local held = self.held
	if held == nil then
		self.held = { first, last }
		return
	end
	if held[1] < first then
		ask(self, {
			decorate = self.layer,
			version = self.version,
			from = layout.line_start(self.editor, held[1]) or 0,
			to = line_end(self, first - 1),
			decorations = {},
		})
	end
	if held[2] > last then
		ask(self, {
			decorate = self.layer,
			version = self.version,
			from = layout.line_start(self.editor, last + 1) or self.length,
			to = self.length,
			decorations = {},
		})
	end
	self.held = { first, last }
end

-- Does what one call may: colours the lines in view that want it (from what it knows, else from
-- a guess), then reads on from what it knows toward them, holding at most PENDING_DECORATIONS
-- decorations for the page. `handle` calls it for an edit or a view, and the timer it goes on with
-- while it is `busy()`: a page need not, and what it asks of the editor goes out with the page's
-- next reply (`reply`) or the timer's, whichever comes first.
function Syntax:work()
	if not ready(self) then
		return
	end
	local first, last = window(self)
	self.painted = set_clip(self.painted, first, last + 1)
	if self.held == nil or self.held[1] ~= first or self.held[2] ~= last then
		forget_outside(self, first, last)
	end
	local budget = self.step
	self.full = false
	while budget > 0 do
		local segment = self.segment
		if self.full or self.pending_count + (segment and #segment[3] or 0) >= PENDING_DECORATIONS then
			break
		end
		local line = set_first_missing(self.painted, first, last + 1)
		local valid = self.valid
		if line == nil then
			-- All of it is coloured; what was coloured from a guess is read to.
			if valid > last or valid >= self.lines then
				break
			end
			budget -= advance(self, first, last)
		elseif line < valid then
			budget -= paint_known(self, line)
		elseif line == valid then
			budget -= advance(self, first, last)
		elseif span(self, valid, line) + span(self, line, last + 1) <= budget then
			-- Near enough to read to in this call.
			while self.valid < line and budget > 0 do
				budget -= advance(self, first, last)
			end
		else
			if self.ends[line] == nil then
				-- Nothing is known before it: a guess, the state the language starts in.
				self.ends[line] = self.language.start()
				self.ok = set_remove(self.ok, line - 1, line + 1)
			end
			budget -= paint_ahead(self, line)
		end
	end
	flush(self)
end

-- Whether there is more to do that a call to `work()` would do. It goes on with it by itself,
-- a call a frame, while there is.
function Syntax:busy()
	if not ready(self) or self.pending_count >= PENDING_DECORATIONS then
		return false
	end
	local first, last = window(self)
	if set_first_missing(set_clip(self.painted, first, last + 1), first, last + 1) ~= nil then
		return true
	end
	return self.valid <= last and self.valid < self.lines
end

-- What it asks of the editor and has not handed out, handed out now: a list for the reply's
-- `editor[name]` (empty where there is nothing).
function Syntax:asks()
	local asks = self.pending
	self.pending = {}
	self.pending_count = 0
	return asks
end

-- The page's `reply` (nil, true, false or a table) with what it asks of the editor handed out in
-- it, before the page's own asks of the editor: what `handle`, `message` and `room` return. The
-- page's reply itself is not changed.
function Syntax:reply(reply)
	if #self.pending == 0 then
		return reply
	end
	return merge({ editor = { [self.editor] = self:asks() } }, reply)
end

-- Goes on with what is left by itself while there is any: a timer that wakes the page at the next
-- frame, does a call's work and hands what it found out with the wake's reply. Held up by as many
-- decorations as it may hold, it goes on to hand them out.
local function go_on(self)
	if self.timer ~= nil or not (self:busy() or self.pending_count >= PENDING_DECORATIONS) then
		return
	end
	self.timer = pencere.time.after(0, function()
		self.timer = nil
		self:work()
		go_on(self)
		return self:reply(nil)
	end)
end

-- The changes told since the text the kit knows came to the text read now: what is kept is moved
-- with them.
local function apply(self)
	local id = self.editor
	local batch = self.batch
	self.batch = nil
	local lines, length = layout.editor_lines(id), layout.editor_length(id)
	if lines == nil or length ~= self.told_length then
		reset(self)
		return
	end
	if batch.prefix == math.huge then
		self.version = self.told
		return
	end
	-- The first line a change touched, and the last, in the text as it is (the text before the
	-- first and after the last is as it was), and how many lines came or went.
	local prefix = math.min(batch.prefix, length)
	local first = layout.line_of(id, prefix)
	local last = layout.line_of(id, math.max(length - batch.suffix, prefix))
	if first == nil or last == nil then
		reset(self)
		return
	end
	local delta = lines - self.lines
	local last_before = last - delta
	-- The states: those before the change stay, those after it move with the lines, those of the
	-- lines it touched go (the state after its last line is the one after the last line it took
	-- the place of).
	local ends = self.ends
	local before_lines = self.lines
	if delta ~= 0 and last_before + 1 <= before_lines then
		table.move(ends, last_before + 1, before_lines, last_before + 1 + delta)
	end
	if delta < 0 then
		for k = before_lines + delta + 1, before_lines do
			ends[k] = nil
		end
	end
	for k = first + 1, last do
		ends[k] = nil
	end
	local valid = self.valid
	self.ok = set_edit(self.ok, first, last_before, delta)
	if valid > last_before + 1 then
		-- The lines after the change that were right were read from the line before them.
		self.ok = set_add(self.ok, last + 1, valid + delta)
	end
	self.valid = math.min(valid, first)
	self.painted = set_edit(self.painted, first, last_before, delta)
	local held = self.held
	if held then
		local function moved(line, towards)
			if line < first then
				return line
			elseif line > last_before then
				return line + delta
			end
			return towards
		end
		self.held = { moved(held[1], first), moved(held[2], last) }
	end
	self.version, self.lines, self.length = self.told, lines, length
	self.chunk = nil
end

-- An edit of the editor: what it changed is put with the changes told before it, and where the
-- text read is of its version, what is kept is moved with them all.
local function edited(self, event)
	local version = event.version
	if type(version) ~= "number" or version <= self.told then
		-- Its change is in the text the kit read.
		return
	end
	if version ~= self.told + 1 then
		-- An edit the kit was not told of: it starts afresh.
		reset(self)
		return
	end
	local batch = self.batch or { prefix = math.huge, suffix = math.huge }
	local length = self.told_length
	local changes = event.changes or {}
	local count = #changes
	if count > 0 then
		batch.prefix = math.min(batch.prefix, changes[1].from)
		batch.suffix = math.min(batch.suffix, length - changes[count].to)
		for _, change in ipairs(changes) do
			local insert = change.insert or ""
			length += (utf8.len(insert) or #insert) - (change.to - change.from)
		end
	end
	self.batch = batch
	self.told, self.told_length = version, length
	if layout.editor_version(self.editor) == version then
		apply(self)
	end
end

-- Takes the page's event (give it each of them): its editor's `edit` and `viewport` say what
-- changed and what is in view, and a call's work is done for them at once, so that the lines an
-- edit makes wrong are coloured again with the page's reply to it. Any other event (the pointer
-- moving, a key, a wheel's turn) it leaves alone: what is left is its timer's, a call a frame,
-- however many events come. What it asks of the editor is handed out in the page's reply (`reply`,
-- or `asks()`).
--
--   handle = function(event)
--       syntax:handle(event)
--       return syntax:reply(respond(event))
--   end
function Syntax:handle(event)
	if type(event) ~= "table" or event.editor ~= self.editor then
		return
	end
	local kind = event.type
	if kind ~= "edit" and kind ~= "viewport" then
		return
	end
	local now = layout.editor_version(self.editor)
	if self.version == nil or now == nil or now < self.told then
		-- Nothing read yet, or an editor of the name that is not the one read (it went and came
		-- back): it starts from the text as it is.
		reset(self)
	end
	if kind == "edit" then
		if self.version ~= nil then
			edited(self, event)
		end
	else
		self.view = { event.first, event.last }
	end
	self:work()
	go_on(self)
end
