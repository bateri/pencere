-- @pencere/fuzzy: finding what a person types in a list, the way a picker, a palette or a filter
-- does.
--
-- A pattern matches a text where each of its characters is in the text, in order, with anything
-- between them ("fb" is in "foo_bar"). Of the ways it is there, the one that scores best is taken,
-- as fzf's second algorithm and VS Code's `fuzzyScore` take it: a character scores more at the
-- start of the text and at the start of a word (after a space or one of `- _ . / \ :`, a capital
-- after a small letter, a digit after a letter), a run of characters together more than the same
-- characters apart, and a gap between two costs, as does a first character late in the text. With
-- `path`, a text is a path, and a pattern found in its last part (the file's own name) scores above
-- one spread over its directories.
--
--   local fuzzy = require("@pencere/fuzzy")
--   local score, positions = fuzzy.match("fb", "foo_bar")       -- 55, { 1, 5 }
--   local found = fuzzy.filter("main", paths, { path = true, limit = 50 })
--   for _, row in ipairs(found) do
--       rows[#rows + 1] = pencere.rich(fuzzy.spans(row.item, row.positions, { color = "accent" }))
--   end
--
-- A list too long for one call (thousands of files) is searched a slice at a time, each slice held
-- to how much text it compares, the rest at the next frames, by itself:
--
--   search = fuzzy.search(pattern, files, { path = true, limit = 50 })
--   return search:run(function(search) shown = search:results(); return true end)
--
-- Places are characters (code points of UTF-8), counted from 1. A text that is not UTF-8 is read a
-- byte a character, and a text is matched in its first 64 KiB. Case: "smart" (the default) tells a
-- capital from a small letter only where the pattern has a capital; "ignore" never does, "respect"
-- always. A letter is lowered by Unicode's
-- simple mapping for ASCII, Latin-1, Latin Extended-A and the letters of Latin Extended-B and
-- Latin Extended Additional that come in pairs, Greek, Cyrillic, Armenian and the full-width Latin
-- letters; a letter of any other script is compared as it is. A space in the pattern is not looked
-- for: it parts the words a person types.
--
-- It is written on the page API alone: `pencere.time` to go on by itself and `pencere.span` for
-- the pieces of a text it marks.

local fuzzy = {}

-- --- Scores ------------------------------------------------------------------------------------

-- fzf's: a character matched, the cost of a gap (its first character and each after it), and the
-- bonuses of where a character is: at the start of a word after a space (the text's first
-- character too), after a delimiter (`/ \ : , ; |`), after any other character that is not a letter
-- or a digit; a capital after a small letter or a digit after what is not one; a character that is
-- not a letter or a digit matched itself; a run. The bonus of the pattern's first character counts
-- twice: where a match starts says most of what it is.
local SCORE_MATCH = 16
local GAP_START = -3
local GAP_EXTENSION = -1
local BONUS_BOUNDARY = 8
local BONUS_WHITE = 10
local BONUS_DELIMITER = 9
local BONUS_NON_WORD = 8
local BONUS_CAMEL = 7
local BONUS_CONSECUTIVE = 4
local FIRST_MULTIPLIER = 2
-- A match that starts late in the text loses a point for each character before it, at most this
-- many: a tie of two matches goes to the earlier, and no more (a word's start counts for more).
local LEADING_MOST = 5
-- The most one character of a pattern can score: its match and the largest bonus, twice for the
-- first. A match in a path's name scores this much for each character of the pattern, and one
-- more, above what it scores in the name alone: more than any match spread over the directories.
local BEST_FIRST = SCORE_MATCH + BONUS_WHITE * FIRST_MULTIPLIER
local BEST_NEXT = SCORE_MATCH + BONUS_WHITE

-- How many places of a table of scores (characters of the text from the first place the pattern
-- can start to the last it can end, times the pattern's characters) a match fills. A match that
-- would need more is scored along the first place the pattern is found (fzf's first algorithm):
-- a long text and a long pattern cost a call no more than this.
local MOST_CELLS = 32768

-- How much one step of a search compares unless the page says: bytes of text read and places of
-- the tables of scores filled. 20 000 paths of some 30 characters are gone through in some 10 to
-- 55 such steps, as many of them match; one takes about 15 ms on a fast machine, well within the
-- time a call has (250 ms) on a slow one.
local STEP = 60000

-- A text is matched in its first this many bytes (as long as a text of a page may be): what one
-- item costs a step has an end.
local MOST_BYTES = 65536

-- How many bytes of a text are made characters at a time (the interpreter hands out at most a few
-- thousand values from one call).
local CHUNK = 2048

-- --- Characters --------------------------------------------------------------------------------

-- The classes of characters, as fzf has them: what a word is made of is after DELIMITER.
local WHITE, NON_WORD, DELIMITER, LOWER, UPPER, LETTER, NUMBER = 0, 1, 2, 3, 4, 5, 6
local CLASSES = 7

-- The bonus of a character of class `class` after one of class `before`: BONUS[before * CLASSES +
-- class + 1].
local BONUS = {}
for before = 0, CLASSES - 1 do
	for class = 0, CLASSES - 1 do
		local bonus = 0
		if class > NON_WORD and before == WHITE then
			bonus = BONUS_WHITE
		elseif class > NON_WORD and before == DELIMITER then
			bonus = BONUS_DELIMITER
		elseif class > NON_WORD and before == NON_WORD then
			bonus = BONUS_BOUNDARY
		elseif (before == LOWER and class == UPPER) or (before ~= NUMBER and class == NUMBER) then
			bonus = BONUS_CAMEL
		elseif class == NON_WORD or class == DELIMITER then
			bonus = BONUS_NON_WORD
		elseif class == WHITE then
			bonus = BONUS_WHITE
		end
		BONUS[before * CLASSES + class + 1] = bonus
	end
end

-- A letter in the other case, by Unicode's simple mappings, for the scripts the header names:
-- LOWER_OF[capital] is its small letter, UPPER_OF[small] its capital (the first that lowers to it).
local LOWER_OF, UPPER_OF = {}, {}
local function pair(upper, lower)
	LOWER_OF[upper] = lower
	if UPPER_OF[lower] == nil then
		UPPER_OF[lower] = upper
	end
end
-- Letters that come in pairs, a capital and its small letter after it, from `from` (a capital) to
-- `to`.
local function pairs_from(from, to)
	for code = from, to - 1, 2 do
		pair(code, code + 1)
	end
end
local function shifted(from, to, by)
	for code = from, to do
		pair(code, code + by)
	end
end
shifted(0x41, 0x5A, 32)
shifted(0xC0, 0xD6, 32)
shifted(0xD8, 0xDE, 32)
pairs_from(0x100, 0x12F)
-- İ is i, and ı a small letter whose capital is I (which is i's).
pair(0x130, 0x69)
UPPER_OF[0x131] = 0x49
pairs_from(0x132, 0x137)
pairs_from(0x139, 0x148)
pairs_from(0x14A, 0x177)
pair(0x178, 0xFF)
pairs_from(0x179, 0x17E)
pairs_from(0x1CD, 0x1DC)
pairs_from(0x1DE, 0x1EF)
pairs_from(0x1F8, 0x21F)
pairs_from(0x222, 0x233)
pair(0x386, 0x3AC)
shifted(0x388, 0x38A, 37)
pair(0x38C, 0x3CC)
shifted(0x38E, 0x38F, 63)
shifted(0x391, 0x3A1, 32)
shifted(0x3A3, 0x3AB, 32)
shifted(0x400, 0x40F, 80)
shifted(0x410, 0x42F, 32)
pairs_from(0x460, 0x481)
pairs_from(0x48A, 0x4BF)
pair(0x4C0, 0x4CF)
pairs_from(0x4C1, 0x4CE)
pairs_from(0x4D0, 0x52F)
shifted(0x531, 0x556, 48)
pairs_from(0x1E00, 0x1E95)
pairs_from(0x1EA0, 0x1EFF)
shifted(0xFF21, 0xFF3A, 32)

-- The same in UTF-8, for lowering a whole text at once: a capital outside ASCII to its small
-- letter.
local LOWER_UTF8 = {}
for upper, lower in next, LOWER_OF do
	if upper >= 0x80 then
		LOWER_UTF8[utf8.char(upper)] = utf8.char(lower)
	end
end
-- A character of more than a byte, in a text that is UTF-8.
local WIDE = "[\xC2-\xF4][\x80-\xBF]+"

-- The class of each ASCII character.
local ASCII_CLASS = {}
for code = 0, 127 do
	local class = NON_WORD
	if code >= 0x61 and code <= 0x7A then
		class = LOWER
	elseif code >= 0x41 and code <= 0x5A then
		class = UPPER
	elseif code >= 0x30 and code <= 0x39 then
		class = NUMBER
	elseif code == 0x20 or (code >= 0x09 and code <= 0x0D) then
		class = WHITE
	elseif code == 0x2F or code == 0x5C or code == 0x3A or code == 0x2C or code == 0x3B or code == 0x7C then
		class = DELIMITER
	end
	ASCII_CLASS[code] = class
end

-- The spaces of other scripts than ASCII's.
local SPACES = {
	[0x85] = true,
	[0xA0] = true,
	[0x1680] = true,
	[0x2028] = true,
	[0x2029] = true,
	[0x202F] = true,
	[0x205F] = true,
	[0x3000] = true,
}
for code = 0x2000, 0x200A do
	SPACES[code] = true
end

-- The class of a character outside ASCII: a space, a mark of punctuation (Latin-1's, the general
-- punctuation, the ideographic, the full-width), a digit (the Arabic-Indic, the full-width), a
-- capital or a small letter of a script with cases, and any other letter.
local function wide_class(code)
	if SPACES[code] then
		return WHITE
	elseif LOWER_OF[code] then
		return UPPER
	elseif UPPER_OF[code] then
		return LOWER
	elseif
		(code >= 0xA1 and code <= 0xBF and code ~= 0xAA and code ~= 0xB5 and code ~= 0xBA)
		or code == 0xD7
		or code == 0xF7
		or (code >= 0x2010 and code <= 0x205E)
		or (code >= 0x3001 and code <= 0x303F)
		or (code >= 0xFF01 and code <= 0xFF0F)
		or (code >= 0xFF1A and code <= 0xFF20)
	then
		return NON_WORD
	elseif (code >= 0x660 and code <= 0x669) or (code >= 0xFF10 and code <= 0xFF19) then
		return NUMBER
	end
	return LETTER
end

local function class_of(code)
	if code < 128 then
		return ASCII_CLASS[code]
	end
	return wide_class(code)
end

-- The characters of `text` as a list of code points, and how many: a text that is not UTF-8 a
-- byte a character.
local function characters(text)
	local length = #text
	if length == 0 then
		return {}, 0
	end
	local ascii = string.find(text, "[\128-\255]") == nil
	local count = ascii and length or utf8.len(text)
	local read = string.byte
	if not ascii and count ~= nil then
		read = utf8.codepoint
	elseif not ascii then
		count = length
	end
	if length <= CHUNK then
		return { read(text, 1, length) }, count
	end
	local out = table.create(count)
	local from, made = 1, 0
	while from <= length do
		local to = math.min(from + CHUNK - 1, length)
		if read == utf8.codepoint then
			-- A piece ends where a character does: the byte after it starts one.
			while to < length and to > from do
				local next_byte = string.byte(text, to + 1)
				if next_byte < 0x80 or next_byte >= 0xC0 then
					break
				end
				to -= 1
			end
		end
		local piece = { read(text, from, to) }
		table.move(piece, 1, #piece, made + 1, out)
		made += #piece
		from = to + 1
	end
	return out, count
end

-- --- Patterns ----------------------------------------------------------------------------------

local CASES = { smart = true, ignore = true, respect = true }
local HIGHLIGHTS = { best = true, first = true }
local MATCH_OPTIONS = { case = true, path = true, highlight = true }
local LIST_OPTIONS = { case = true, path = true, highlight = true, text = true, limit = true }

-- The options a caller gave, checked: an error at the caller's line (`level`) for one that is not
-- one, in words that say what it is.
local function options_of(options, allowed, name, level)
	if options == nil then
		return {}
	end
	if type(options) ~= "table" then
		error(name .. " takes options as a table, not a " .. type(options), level + 1)
	end
	for key in next, options do
		if not allowed[key] then
			local names = allowed == LIST_OPTIONS and "case, path, highlight, text, limit" or "case, path, highlight"
			error(name .. " has no option " .. tostring(key) .. " (" .. names .. ")", level + 1)
		end
	end
	local case = options.case
	if case ~= nil and not CASES[case] then
		error('the case of a fuzzy match is "smart", "ignore" or "respect", not ' .. tostring(case), level + 1)
	end
	if options.path ~= nil and type(options.path) ~= "boolean" then
		error("the path option of a fuzzy match is true or false, not a " .. type(options.path), level + 1)
	end
	local highlight = options.highlight
	if highlight ~= nil and not HIGHLIGHTS[highlight] then
		error('the highlight of a fuzzy match is "best" or "first", not ' .. tostring(highlight), level + 1)
	end
	return options
end

-- A pattern made ready to match: its characters as they are compared (the spaces left out; lowered
-- where the case is not told apart; a `\` a `/` in a path), as UTF-8 too, how many, whether case
-- is told apart and whether texts are paths.
local function compile(pattern, case, path)
	local codes = characters(pattern)
	local respect = case == "respect"
	if case == nil or case == "smart" then
		for _, code in ipairs(codes) do
			if LOWER_OF[code] then
				respect = true
				break
			end
		end
	end
	local compared, pieces = {}, {}
	local slash = false
	for _, code in ipairs(codes) do
		if class_of(code) ~= WHITE then
			if not respect then
				code = LOWER_OF[code] or code
			end
			if path and code == 0x5C then
				code = 0x2F
			end
			if code == 0x2F then
				slash = true
			end
			compared[#compared + 1] = code
			pieces[#pieces + 1] = utf8.char(code)
		end
	end
	local count = #compared
	return {
		codes = compared,
		pieces = pieces,
		count = count,
		respect = respect,
		path = path == true,
		-- A pattern with a `/` in it is a path itself: it is matched against the whole of one.
		slash = slash,
		name_bonus = BEST_FIRST + BEST_NEXT * math.max(count - 1, 0) + 1,
	}
end

-- The pattern last made ready, for a page that matches one pattern against text after text.
local last_pattern, last_case, last_path, last_query = nil, nil, nil, nil

local function query_of(pattern, case, path)
	if pattern == last_pattern and case == last_case and path == last_path then
		return last_query
	end
	last_query = compile(pattern, case, path)
	last_pattern, last_case, last_path = pattern, case, path
	return last_query
end

-- --- Matching ----------------------------------------------------------------------------------

-- What a match costs, as a search counts it: bytes read and places of a table of scores filled.
local work = 0

-- Whether every character of the pattern is in `text`, in order: a quick look at the bytes, before
-- the characters are read and scored.
local function present(query, text)
	local subject = text
	if not query.respect then
		subject = string.lower(text)
		if string.find(subject, "[\128-\255]") then
			subject = (string.gsub(subject, WIDE, LOWER_UTF8))
		end
	end
	if query.path and string.find(subject, "\\", 1, true) then
		subject = (string.gsub(subject, "\\", "/"))
	end
	local at = 1
	local find = string.find
	for _, piece in ipairs(query.pieces) do
		local found = find(subject, piece, at, true)
		if found == nil then
			return false
		end
		at = found + #piece
	end
	return true
end

-- The table of scores and what goes with it, kept from match to match: H[(i - 1) * width + k] is
-- the best score of the pattern's first i characters with the i-th at the k-th place of the window
-- or before it, C the same place's run (how many characters of a run end there; 0 where the i-th is
-- not there), B the bonus of each place, T each place's character as compared, F where each
-- character of the pattern is first found.
local H, C, B, T, F = {}, {}, {}, {}, {}

-- What the last match found, for its places: the text's characters, the range it was looked for
-- in, the window of the table and where the best score ended in it; or the places themselves where
-- the text was too long for a table.
local found_codes, found_from, found_first, found_width, found_at, found_places = nil, 0, 0, 0, 0, nil

local function compared(code, respect, path)
	if not respect then
		code = LOWER_OF[code] or code
	end
	if path and code == 0x5C then
		return 0x2F
	end
	return code
end

-- The pattern's characters in `codes` from `from` to `to`, found where each first is after the one
-- before: that the pattern is there, and where each is in F.
local function first_places(query, codes, from, to)
	local wanted, respect, path = query.codes, query.respect, query.path
	local at = from
	for i = 1, query.count do
		local want = wanted[i]
		while at <= to do
			local code = codes[at]
			if not respect then
				code = LOWER_OF[code] or code
			end
			if path and code == 0x5C then
				code = 0x2F
			end
			if code == want then
				break
			end
			at += 1
		end
		if at > to then
			return false
		end
		F[i] = at
		at += 1
	end
	return true
end

-- The places of fzf's first algorithm: where the pattern is first found to its end, and of that
-- the shortest stretch that ends there (the latest start), each character the first after the one
-- before. Their score, scored along them as the table scores, and the places.
local function first_found(query, codes, from)
	local wanted, respect, path, count = query.codes, query.respect, query.path, query.count
	local finish = F[count]
	local start, i = finish, count
	for at = finish, from, -1 do
		if compared(codes[at], respect, path) == wanted[i] then
			if i == 1 then
				start = at
				break
			end
			i -= 1
		end
	end
	local places = table.create(count)
	local score, run, in_gap, first_bonus = 0, 0, false, 0
	local before = start > 1 and class_of(codes[start - 1]) or WHITE
	i = 1
	for at = start, finish do
		local code = codes[at]
		local class = class_of(code)
		if i <= count and compared(code, respect, path) == wanted[i] then
			places[i] = at
			local bonus = BONUS[before * CLASSES + class + 1]
			if run == 0 then
				first_bonus = bonus
			else
				if bonus >= BONUS_BOUNDARY and bonus > first_bonus then
					first_bonus = bonus
				end
				bonus = math.max(bonus, first_bonus, BONUS_CONSECUTIVE)
			end
			if i == 1 then
				score += SCORE_MATCH + bonus * FIRST_MULTIPLIER - math.min(at - from, LEADING_MOST)
			else
				score += SCORE_MATCH + bonus
			end
			in_gap, run = false, run + 1
			i += 1
		else
			score += in_gap and GAP_EXTENSION or GAP_START
			in_gap, run, first_bonus = true, 0, 0
		end
		before = class
	end
	work += finish - start + 1
	-- Scored with no floor, a long stretch comes to less than nothing; a match is more than that.
	return math.max(score, 1), places
end

-- The best score of the pattern in `codes` from `from` to `to`; nil where it is not there. What it
-- found is kept for its places (`places`).
local function best_in(query, codes, from, to)
	local count = query.count
	work += to - from + 1
	if not first_places(query, codes, from, to) then
		return nil
	end
	local wanted, respect, path = query.codes, query.respect, query.path
	-- The window: from where the first character is first found to where the last one last is.
	local first = F[1]
	local last = to
	local want_last = wanted[count]
	while last > F[count] and compared(codes[last], respect, path) ~= want_last do
		last -= 1
	end
	local width = last - first + 1
	found_codes, found_from = codes, from
	if width * count > MOST_CELLS then
		local score, places = first_found(query, codes, from)
		found_places = places
		return score
	end
	found_places = nil
	work += width * count
	local before = first > 1 and class_of(codes[first - 1]) or WHITE
	for k = 1, width do
		local code = codes[first + k - 1]
		local class = code < 128 and ASCII_CLASS[code] or wide_class(code)
		B[k] = BONUS[before * CLASSES + class + 1]
		before = class
		if not respect then
			code = LOWER_OF[code] or code
		end
		if path and code == 0x5C then
			code = 0x2F
		end
		T[k] = code
	end
	local best, best_at = 0, 0
	-- The first character of the pattern: its bonus twice, less the characters before it.
	local want = wanted[1]
	local previous, in_gap = 0, false
	for k = 1, width do
		local gap = previous + (in_gap and GAP_EXTENSION or GAP_START)
		local score, run = 0, 0
		if T[k] == want then
			local lead = first + k - 1 - from
			if lead > LEADING_MOST then
				lead = LEADING_MOST
			end
			score = SCORE_MATCH + B[k] * FIRST_MULTIPLIER - lead
			if score >= gap then
				run = 1
			end
		end
		if run == 0 then
			score = gap > 0 and gap or 0
		end
		in_gap = run == 0
		H[k], C[k] = score, run
		previous = score
		if count == 1 and run > 0 and score > best then
			best, best_at = score, k
		end
	end
	-- Each character after it: on from the one before it, at the place before (a run where that
	-- one was there), or a gap after a place before.
	for i = 2, count do
		want = wanted[i]
		local row = (i - 1) * width
		local up = row - width
		local is_last = i == count
		previous, in_gap = 0, false
		for k = F[i] - first + 1, width do
			local gap = previous + (in_gap and GAP_EXTENSION or GAP_START)
			local score, run = 0, 0
			if T[k] == want then
				run = C[up + k - 1] + 1
				local bonus = B[k]
				if run > 1 then
					local from_bonus = B[k - run + 1]
					if bonus >= BONUS_BOUNDARY and bonus > from_bonus then
						-- A word that starts in the middle of a run starts a run of its own.
						run = 1
					else
						if from_bonus > bonus then
							bonus = from_bonus
						end
						if BONUS_CONSECUTIVE > bonus then
							bonus = BONUS_CONSECUTIVE
						end
					end
				end
				score = H[up + k - 1] + SCORE_MATCH + bonus
				if score < gap then
					run = 0
				end
			end
			if run == 0 then
				score = gap > 0 and gap or 0
			end
			in_gap = run == 0
			H[row + k], C[row + k] = score, run
			previous = score
			if is_last and run > 0 and score > best then
				best, best_at = score, k
			end
		end
	end
	found_first, found_width, found_at = first, width, best_at
	return best
end

-- The places of what the last match found: the characters that gave its score ("best"), or where
-- the pattern is first found, from as late a first character as that allows ("first").
local function places(query, highlight)
	if found_places ~= nil then
		return table.clone(found_places)
	end
	local count = query.count
	if highlight == "first" then
		local _, out = first_found(query, found_codes, found_from)
		return out
	end
	local out = table.create(count)
	local width, first = found_width, found_first
	local i, k = count, found_at
	while i >= 1 and k >= 1 do
		if C[(i - 1) * width + k] > 0 then
			out[i] = first + k - 1
			i -= 1
		end
		k -= 1
	end
	return out
end

-- Whether `code` parts the directories of a path.
local function is_separator(code)
	return code == 0x2F or code == 0x5C
end

-- The score of `query` in `text`, nil where it is not there. What it found is kept for `places`.
local function score_of(query, text)
	local length = #text
	if length > MOST_BYTES then
		-- Cut where a character starts, so that a text in UTF-8 stays one.
		local cut = MOST_BYTES
		while cut > 1 do
			local byte = string.byte(text, cut + 1)
			if byte < 0x80 or byte >= 0xC0 then
				break
			end
			cut -= 1
		end
		text = string.sub(text, 1, cut)
	end
	work += #text
	if not present(query, text) then
		return nil
	end
	local codes, count = characters(text)
	work += count
	if query.path and not query.slash then
		-- The name: the last part of the path, the separators at its end left out.
		local name_end = count
		while name_end >= 1 and is_separator(codes[name_end]) do
			name_end -= 1
		end
		local name_from = name_end
		while name_from >= 1 and not is_separator(codes[name_from]) do
			name_from -= 1
		end
		name_from += 1
		if name_from <= name_end then
			local score = best_in(query, codes, name_from, name_end)
			if score ~= nil then
				return score + query.name_bonus
			end
		end
	end
	return best_in(query, codes, 1, count)
end

-- `pattern` in `text`: its score and the places (characters from 1, in order) of its characters in
-- the text, or nil where it is not there. An empty pattern is in every text, with a score of 0 and
-- no places. `options` are:
--   case       "smart" (the default: told apart where the pattern has a capital), "ignore" or
--              "respect"
--   path       the text is a path: a match in its name (its last part) scores above any other, a
--              `/` and a `\` are the same, and a pattern with a `/` is matched against the whole
--   highlight  which places: "best" (the default), those that gave the score; "first", where the
--              pattern is first all there, from as late a first character as that allows
function fuzzy.match(pattern, text, options)
	if type(pattern) ~= "string" then
		error("fuzzy.match takes a pattern (a string), then a text, not a " .. type(pattern), 2)
	end
	if type(text) ~= "string" then
		error("fuzzy.match takes a pattern, then a text (a string), not a " .. type(text), 2)
	end
	options = options_of(options, MATCH_OPTIONS, "fuzzy.match", 2)
	local query = query_of(pattern, options.case, options.path)
	if query.count == 0 then
		return 0, {}
	end
	local score = score_of(query, text)
	if score == nil then
		return nil
	end
	return score, places(query, options.highlight)
end

-- --- Lists -------------------------------------------------------------------------------------

local Search = {}
Search.__index = Search

-- Whether entry `a` goes after entry `b`: a lower score, or the same score and a later item.
local function after(a, b)
	if a.score ~= b.score then
		return a.score < b.score
	end
	return a.index > b.index
end

-- The heap of the best `limit` entries, the one that goes last at its top.
local function sift_down(heap, at)
	local size = #heap
	while true do
		local left = at * 2
		if left > size then
			return
		end
		local worst = left
		local right = left + 1
		if right <= size and after(heap[right], heap[left]) then
			worst = right
		end
		if not after(heap[worst], heap[at]) then
			return
		end
		heap[at], heap[worst] = heap[worst], heap[at]
		at = worst
	end
end

local function sift_up(heap, at)
	while at > 1 do
		local parent = at // 2
		if not after(heap[at], heap[parent]) then
			return
		end
		heap[at], heap[parent] = heap[parent], heap[at]
		at = parent
	end
end

-- Keeps what a match found where it is among the best (`limit`), its places made only then.
local function keep(search, item, index, score)
	local kept = search.kept
	local limit = search.limit
	if limit ~= nil and #kept >= limit then
		local top = kept[1]
		if score < top.score or (score == top.score and index > top.index) then
			return
		end
		kept[1] = { item = item, index = index, score = score, positions = places(search.query, search.highlight) }
		sift_down(kept, 1)
	else
		local positions = places(search.query, search.highlight)
		kept[#kept + 1] = { item = item, index = index, score = score, positions = positions }
		if limit ~= nil then
			sift_up(kept, #kept)
		end
	end
	search.sorted = nil
end

-- Makes a search: what `fuzzy.filter` and `fuzzy.search` were given, checked (an error at the
-- caller's line, `level` above this one).
local function make(pattern, items, options, name, level)
	if type(pattern) ~= "string" then
		error(name .. " takes a pattern (a string), then the items, not a " .. type(pattern), level + 1)
	end
	if type(items) ~= "table" then
		error(name .. " takes a pattern, then the items (a list), not a " .. type(items), level + 1)
	end
	options = options_of(options, LIST_OPTIONS, name, level + 1)
	local text = options.text
	local kind = type(text)
	if text ~= nil and kind ~= "function" and kind ~= "string" then
		error(
			"the text option of " .. name .. " is a function of an item or the name of its field, not a " .. kind,
			level + 1
		)
	end
	local limit = options.limit
	if limit ~= nil and (type(limit) ~= "number" or limit < 1 or limit ~= math.floor(limit)) then
		error("the limit of " .. name .. " is how many items it keeps at most: a whole number, 1 or more", level + 1)
	end
	return setmetatable({
		pattern = pattern,
		items = items,
		options = options,
		query = compile(pattern, options.case, options.path),
		highlight = options.highlight,
		text = text,
		limit = limit,
		-- The items it goes through (`within`, a list of their indexes, or nil for all), how many,
		-- and the next.
		within = nil,
		total = #items,
		next = 1,
		-- What it found: the best (a heap where there is a limit), every index that matched, and
		-- the best in order (nil until asked again).
		kept = {},
		matched = {},
		sorted = nil,
		-- It goes no further: cancelled.
		cancelled = false,
		-- What was wrong with an item, and the timer it goes on with (nil while none waits).
		bad = nil,
		timer = nil,
		on_progress = nil,
	}, Search)
end

-- The text of `item`: the item, or what the text option makes of it. Nil and why where it is none.
local function text_of(search, item, index)
	local text = item
	local how = search.text
	if how ~= nil then
		if type(how) == "function" then
			text = how(item)
		elseif type(item) == "table" then
			text = item[how]
		else
			return nil, "item " .. index .. " is a " .. type(item) .. ", which has no field " .. how
		end
	end
	local kind = type(text)
	if kind == "string" then
		return text
	elseif kind == "number" then
		return tostring(text)
	elseif how == nil then
		return nil, "item " .. index .. " is a " .. kind .. ": the text option says how to read its text"
	end
	return nil, "the text of item " .. index .. " is a " .. kind .. ", not a string"
end

-- Goes through the items, at least one, until what it compared comes to `budget`. Whether it is
-- done.
local function scan(search, budget)
	if search.cancelled or search.bad ~= nil then
		return true
	end
	local query = search.query
	local items, within, total = search.items, search.within, search.total
	local at = search.next
	local spent = 0
	if query.count == 0 then
		-- Every item is a match, in order: with a limit, the first are the best.
		local kept, limit = search.kept, search.limit
		while at <= total and (spent < budget or at == search.next) do
			if limit ~= nil and #kept >= limit then
				at = total + 1
				break
			end
			local index = within and within[at] or at
			kept[#kept + 1] = { item = items[index], index = index, score = 0, positions = {} }
			search.matched[#search.matched + 1] = index
			spent += 1
			at += 1
		end
		search.next = at
		search.sorted = nil
		return at > total
	end
	local matched = search.matched
	while at <= total and (spent < budget or at == search.next) do
		local index = within and within[at] or at
		local item = items[index]
		local text, why = text_of(search, item, index)
		if text == nil then
			search.bad = why
			search.next = at
			return true
		end
		work = 0
		local score = score_of(query, text)
		spent += work + 1
		if score ~= nil then
			matched[#matched + 1] = index
			keep(search, item, index, score)
		end
		at += 1
	end
	search.next = at
	return at > total
end

-- The best of what was found, in order: a higher score first, of two the same the earlier item.
local function results(search)
	local sorted = search.sorted
	if sorted == nil then
		sorted = table.clone(search.kept)
		table.sort(sorted, function(a, b)
			return after(b, a)
		end)
		search.sorted = sorted
	end
	return sorted
end

-- The items `pattern` is in, best first: { { item =, index =, score =, positions = }, ... }, a
-- higher score first and of two the same the earlier item; with an empty pattern every item, in
-- order. `options` are `fuzzy.match`'s, and:
--   text   how to read an item's text where the items are not strings: a function of the item, or
--          the name of a field of it
--   limit  how many to keep at most: the best, kept as they are found (no list of every match is
--          made and sorted)
-- It does it all at once: a list that may be too long for a call (some thousands of long texts) is
-- searched a slice at a time with `fuzzy.search`.
function fuzzy.filter(pattern, items, options)
	local search = make(pattern, items, options, "fuzzy.filter", 2)
	scan(search, math.huge)
	if search.bad ~= nil then
		error(search.bad, 2)
	end
	return results(search)
end

-- A search of `items` for `pattern` that goes a slice at a time (`options` as `fuzzy.filter`'s).
-- It finds nothing until it is told to go on: `step` does a slice at once, `run` goes on by itself.
-- The items are those the list holds when it is made.
function fuzzy.search(pattern, items, options)
	return make(pattern, items, options, "fuzzy.search", 2)
end

-- Goes through the next items, at least one, until what it compared comes to `budget` (bytes of
-- text read and places of the tables of scores filled: STEP, a few milliseconds, unless said).
-- Whether it is done.
function Search:step(budget)
	if getmetatable(self) ~= Search then
		error("step is a search's: search:step(budget)", 2)
	end
	if budget ~= nil and (type(budget) ~= "number" or budget < 1) then
		error("the budget of a step is how much it compares, 1 or more", 2)
	end
	local done = scan(self, budget or STEP)
	if self.bad ~= nil then
		error(self.bad, 2)
	end
	return done
end

-- The best of what it found so far, in order, as `fuzzy.filter` gives them (at most `limit`). The
-- list is the search's own, the same until it finds more: read it, and do not change it.
function Search:results()
	return results(self)
end

-- Whether it goes no further: it went through every item, or it was cancelled.
function Search:done()
	return self.cancelled or self.next > self.total
end

-- How many items it went through, and how many it goes through in all.
function Search:progress()
	return self.next - 1, self.total
end

-- Goes on by itself while there is more: a timer that wakes the page at the next frame does a step
-- and tells `on_progress`.
local function go_on(self)
	if self.timer ~= nil or self:done() or self.bad ~= nil then
		return
	end
	self.timer = pencere.time.after(0, function()
		self.timer = nil
		if self:done() then
			return nil
		end
		scan(self, STEP)
		if self.bad ~= nil then
			error("fuzzy.search: " .. self.bad, 0)
		end
		local reply = nil
		if self.on_progress ~= nil then
			reply = self.on_progress(self)
		end
		go_on(self)
		return reply
	end)
end

-- Goes through the items by itself, a step at once and a step a frame after it until it is done:
-- a timer of the kit's (`pencere.time.after(0, ...)`) wakes the page for each.
-- `on_progress(search)` is told after each step (`search:results()` is what it found so far), and
-- what it returns is a reply: the wake's, put together with the page's own and those of its other
-- timers. What it returns for the step done at once is what `run` returns, for the page to answer
-- with:
--
--   handle = function(event)
--       if event.type == "text" then
--           if search then search:cancel() end
--           search = fuzzy.search(event.text, files, { path = true, limit = 50 })
--           return search:run(function(search) shown = search:results(); return true end)
--       end
--   end
function Search:run(on_progress)
	if getmetatable(self) ~= Search then
		error("run is a search's: search:run(on_progress)", 2)
	end
	if on_progress ~= nil and type(on_progress) ~= "function" then
		error("run takes the function told after each step, not a " .. type(on_progress), 2)
	end
	self.on_progress = on_progress
	if not self:done() then
		scan(self, STEP)
		if self.bad ~= nil then
			error(self.bad, 2)
		end
	end
	local reply = nil
	if on_progress ~= nil then
		reply = on_progress(self)
	end
	go_on(self)
	return reply
end

-- Stops it: it goes through no more items, and its timer is stopped. What it found stays.
function Search:cancel()
	self.cancelled = true
	if self.timer ~= nil then
		self.timer:cancel()
		self.timer = nil
	end
end

-- Whether the characters of `small` are in `large`, in order.
local function within_codes(small, large)
	local at = 1
	for _, code in ipairs(small) do
		while large[at] ~= nil and large[at] ~= code do
			at += 1
		end
		if large[at] == nil then
			return false
		end
		at += 1
	end
	return true
end

-- A search for `pattern` that takes this one's place, with its items and options: where every text
-- `pattern` is in this one's pattern is in as well (the pattern goes on from what the person typed
-- before), it goes only through what this one found and the items it did not go through yet. This
-- one is cancelled.
--
--   search = search:narrow(event.text)
--   return search:run(show)
function Search:narrow(pattern)
	if getmetatable(self) ~= Search then
		error("narrow is a search's: search:narrow(pattern)", 2)
	end
	if type(pattern) ~= "string" then
		error("narrow takes a pattern (a string), not a " .. type(pattern), 2)
	end
	local options = self.options
	local search = make(pattern, self.items, options, "narrow", 2)
	local old, new = self.query, search.query
	-- The old pattern's characters in the new one, compared as the old one compared them: what the
	-- new one finds, the old one found (a pattern that tells case apart finds less than one that
	-- does not).
	local narrower = old.count > 0 and self.bad == nil and (new.respect or not old.respect)
	if narrower then
		local codes = new.codes
		if new.respect and not old.respect then
			codes = table.create(#codes)
			for k, code in ipairs(new.codes) do
				codes[k] = LOWER_OF[code] or code
			end
		end
		narrower = within_codes(old.codes, codes)
	end
	if narrower then
		local within = table.clone(self.matched)
		for at = self.next, self.total do
			within[#within + 1] = self.within and self.within[at] or at
		end
		search.within = within
		search.total = #within
	end
	self:cancel()
	return search
end

-- --- Drawing -----------------------------------------------------------------------------------

-- What a look of `fuzzy.spans` may say: the methods of a `pencere.span`.
local LOOKS = {
	color = true,
	weight = true,
	italic = true,
	underline = true,
	strike = true,
	family = true,
	bg = true,
	link = true,
}
-- The most parts a text of many looks may have (the kit's SPEC, Section 4).
local MOST_PARTS = 1024

local function part(words, look)
	local span = pencere.span(words)
	if look ~= nil then
		for key, value in next, look do
			if key == "italic" or key == "underline" or key == "strike" then
				if value then
					span[key](span)
				end
			else
				span[key](span, value)
			end
		end
	end
	return span
end

-- `text` as the parts of a text of many looks (`pencere.rich`), the characters at `positions`
-- (characters from 1, as `fuzzy.match` gives them) in `look`: what a `pencere.span` says
-- (`color`, `weight`, `italic`, `underline`, `strike`, `family`, `bg`, `link`), by default
-- `{ color = "accent", weight = 700 }`; the rest in the element's own look. `from` and `to`
-- (characters, the whole text by default) take a part of the text, the positions still counted in
-- the whole: a path's name and its directories drawn apart. Places outside it are passed over.
--
--   pencere.rich(fuzzy.spans(row.item, row.positions, { color = "accent" })):truncate()
function fuzzy.spans(text, positions, look, from, to)
	if type(text) ~= "string" then
		error("fuzzy.spans takes a text (a string), then the positions, not a " .. type(text), 2)
	end
	if positions ~= nil and type(positions) ~= "table" then
		error("fuzzy.spans takes the positions as a list of characters, not a " .. type(positions), 2)
	end
	if look == nil then
		look = { color = "accent", weight = 700 }
	elseif type(look) ~= "table" then
		error("the look of fuzzy.spans is a table of what a span says, not a " .. type(look), 2)
	end
	for key in next, look do
		if not LOOKS[key] then
			error(
				"a span says no "
					.. tostring(key)
					.. " (color, weight, italic, underline, strike, family, bg, link)",
				2
			)
		end
	end
	for _, name in ipairs({ "from", "to" }) do
		local value = name == "from" and from or to
		if value ~= nil and type(value) ~= "number" then
			error("the " .. name .. " of fuzzy.spans is a character (a number), not a " .. type(value), 2)
		end
	end
	local length = #text
	local ascii = string.find(text, "[\128-\255]") == nil
	local count = ascii and length or utf8.len(text)
	local valid = count ~= nil
	if not valid then
		count = length
	end
	from = math.max(math.ceil(from or 1), 1)
	to = math.min(math.floor(to or count), count)
	if from > to then
		return {}
	end
	-- The byte where character `at` starts, from a character and its byte known before it.
	local known_char, known_byte = 1, 1
	local function byte_of(at)
		if ascii or not valid then
			return at
		end
		if at == known_char then
			return known_byte
		end
		local byte = utf8.offset(text, at - known_char + 1, known_byte) or (length + 1)
		known_char, known_byte = at, byte
		return byte
	end
	local marked = {}
	for k, at in ipairs(positions or {}) do
		if type(at) ~= "number" then
			error("position " .. k .. " of fuzzy.spans is a character (a number), not a " .. type(at), 2)
		end
		if at >= from and at <= to and at == math.floor(at) then
			marked[#marked + 1] = at
		end
	end
	table.sort(marked)
	local parts = {}
	local at = from
	local k = 1
	while at <= to do
		-- The plain characters before the next mark, then the run of marks there.
		while k <= #marked and marked[k] < at do
			k += 1
		end
		local mark = marked[k]
		if #parts >= MOST_PARTS - 2 then
			-- Room for one more part (a turn of this loop makes two): the rest is one, plain.
			parts[#parts + 1] = part(string.sub(text, byte_of(at), byte_of(to + 1) - 1), nil)
			break
		end
		if mark == nil or mark > to then
			parts[#parts + 1] = part(string.sub(text, byte_of(at), byte_of(to + 1) - 1), nil)
			break
		end
		if mark > at then
			parts[#parts + 1] = part(string.sub(text, byte_of(at), byte_of(mark) - 1), nil)
		end
		local finish = mark
		while marked[k + 1] ~= nil and marked[k + 1] <= finish + 1 and marked[k + 1] <= to do
			k += 1
			finish = math.max(finish, marked[k])
		end
		parts[#parts + 1] = part(string.sub(text, byte_of(mark), byte_of(finish + 1) - 1), look)
		at = finish + 1
		k += 1
	end
	return parts
end

return fuzzy
