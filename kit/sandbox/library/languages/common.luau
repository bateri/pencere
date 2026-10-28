-- @pencere/languages/common: what the readers of the families of `@pencere/languages` share: sets
-- of words, text as a pattern, where on its line the stream is, strings and numbers. A family
-- requires it, and `@pencere/languages` requires the family only when a language of it is loaded.

local common = {}

-- The words of `words` (apart by spaces) as a set: `set("if else")` is { ["if"] = true, ... }.
function common.set(words)
	local out = {}
	for word in string.gmatch(words, "%S+") do
		out[word] = true
	end
	return out
end

-- `text` as a pattern that matches it as it is.
function common.literal(text)
	return (string.gsub(text, "%p", "%%%0"))
end

-- Whether nothing but spaces and tabs comes before where the stream is on its line. It looks at the
-- line's indent alone, so that a reader may ask it at every token of a long line.
function common.first_on_line(stream)
	local _, last = string.find(stream.string, "^[ \t]*")
	return stream.pos <= last + 1
end

-- Whether the line's text before where the stream is is all that `pattern` (anchored at the start
-- of the line, and read only as far as it goes) matches: `"[ \t]*%-[ \t]+"` is a list's dash.
function common.after(stream, pattern)
	local _, last = string.find(stream.string, "^" .. pattern)
	return last ~= nil and last + 1 == stream.pos
end

-- A finder of what comes later on the line: `find(stream, pattern)` is where `pattern` next
-- matches at or after where the stream is (nil for nowhere), the same as `string.find`, but what it
-- found is kept for the line, so that a reader that asks at every token of a long line does not
-- read the rest of the line each time.
function common.finder()
	local text, wanted, from, at = nil, nil, 0, nil
	return function(stream, pattern)
		local pos = stream.pos
		if text == stream.string and wanted == pattern and from <= pos and (at == nil or at >= pos) then
			return at
		end
		text, wanted, from = stream.string, pattern, pos
		at = string.find(stream.string, pattern, pos)
		return at
	end
end

-- Reads a string from where the stream is to `close` (a `\` taking the character after it where
-- `escapes`), or to the end of the line: whether it was closed.
function common.quoted(stream, close, escapes)
	local pattern = common.literal(close)
	while not stream:eol() do
		if escapes and stream:eat("\\") then
			stream:next()
		elseif stream:match(pattern) then
			return true
		else
			stream:next()
		end
	end
	return false
end

-- Reads a number where the stream is (hexadecimal, binary, octal, a fraction, an exponent, `_`
-- between digits, and the letters a language puts after one): whether there was one.
function common.number(stream)
	return (
		stream:match("^0[xX][%x_]+[%a_%d]*")
		or stream:match("^0[bB][01_]+[%a_%d]*")
		or stream:match("^0[oO][0-7_]+[%a_%d]*")
		or stream:match("^%d[%d_]*%.?[%d_]*[eE][%+%-]?%d+[%a_%d]*")
		or stream:match("^%d[%d_]*%.%d[%d_]*[%a_%d]*")
		or stream:match("^%d[%d_]*[%a_%d]*")
		or stream:match("^%.%d[%d_]*[%a_%d]*")
	) ~= nil
end

return common
