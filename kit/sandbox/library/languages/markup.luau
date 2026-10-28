-- @pencere/languages/markup: the languages of tags and of marked-up prose: HTML (its scripts and
-- styles read in their own languages), XML, and Markdown (the code in its fences read in the
-- fence's language). `@pencere/languages` requires it when one of them is first loaded.
--
-- A language held in another is found by its name through `@pencere/languages` (`context.reader`),
-- loaded the first time a page meets it; the state keeps its name and its own state, never the
-- reader itself, so that a state is data alone. One that is not known is plain text.

local common = require("./common")

local literal = common.literal

-- --- HTML and XML ------------------------------------------------------------------------------

-- The language of what is inside `<script>` and `<style>`, by the `lang` or `type` its tag said: a
-- language's name (`lang="ts"`, `lang="scss"`), or a media type (`type="text/css"`,
-- `type="application/json"`); nil for what is no code a language here reads
-- (`type="text/template"`).
local function embedded_name(tag, said)
	if said == nil then
		return tag == "style" and "CSS" or "JavaScript"
	end
	local kind = string.lower(said)
	if kind == "module" then
		return "JavaScript"
	elseif kind == "importmap" or kind == "speculationrules" then
		return "JSON"
	elseif not string.find(kind, "/", 1, true) then
		return said
	elseif tag == "style" then
		return string.find(kind, "css", 1, true) and "CSS" or nil
	elseif string.find(kind, "json", 1, true) then
		return "JSON"
	elseif string.find(kind, "typescript", 1, true) then
		return "TypeScript"
	elseif string.find(kind, "javascript", 1, true) or string.find(kind, "ecmascript", 1, true) then
		return "JavaScript"
	end
	return nil
end

-- The tag that closes what is embedded, in either case.
local CLOSE = { script = "</[sS][cC][rR][iI][pP][tT]", style = "</[sS][tT][yY][lL][eE]" }

-- A reader of tags. Where `embeds`, what is inside `<script>` and `<style>` is read in its own
-- language, to the tag that closes it (HTML).
local function tags(context, embeds)
	-- Where the tag that closes what is embedded is next on the line, and the line cut there (kept,
	-- as the tokens of a long line are read one by one from it).
	local find_close = common.finder()
	local cut_whole, cut_at, cut = nil, 0, nil

	-- The state: `mode` is nil in text, else "tag" (inside a tag: `tag` its name, `closing` whether
	-- it closes one, `attr` the attribute (`lang` or `type`) whose value is kept, `value` whether a
	-- value comes next, `said` the `lang` or `type` it said), "value" (a quoted value that goes on
	-- over lines: `quote`), "comment", "cdata", "embedded" (`inner` the name of the language,
	-- `inner_state` its state; `tag` the tag, "script" or "style", that closes it) and "raw" (text to
	-- the tag that closes it, read in no language).
	local function start()
		return {}
	end

	local token

	-- What is embedded, read by its language up to the tag that closes it, where that is on the line.
	local function embedded(stream, state)
		local close = find_close(stream, CLOSE[state.tag])
		if close == stream.pos then
			state.mode, state.tag, state.inner, state.inner_state = nil, nil, nil, nil
			return token(stream, state)
		end
		local reader = state.inner and context.reader(state.inner)
		if reader == nil then
			if close then
				stream.pos = close
			else
				stream:skip_to_end()
			end
			return nil
		end
		if close == nil then
			return reader.token(stream, state.inner_state)
		end
		-- The language reads no further than the tag that closes it: it is given the line to there
		-- (as CodeMirror's mixed HTML backs its stream up to the tag), and at least a character of it.
		local whole = stream.string
		if whole ~= cut_whole or close ~= cut_at then
			cut_whole, cut_at, cut = whole, close, string.sub(whole, 1, close - 1)
		end
		stream.string = cut
		local style = reader.token(stream, state.inner_state)
		stream.string = whole
		return style
	end

	function token(stream, state)
		local mode = state.mode
		if mode == "comment" then
			if stream:skip_to("-->") then
				stream:match("%-%->")
				state.mode = nil
			else
				stream:skip_to_end()
			end
			return "comment"
		elseif mode == "cdata" then
			if stream:skip_to("]]>") then
				stream:match("%]%]>")
				state.mode = nil
			else
				stream:skip_to_end()
			end
			return "string"
		elseif mode == "value" then
			if common.quoted(stream, state.quote, false) then
				state.mode, state.quote = "tag", nil
			end
			return "string"
		elseif mode == "embedded" or mode == "raw" then
			return embedded(stream, state)
		elseif mode == "tag" then
			if stream:eat_space() then
				return nil
			end
			if stream:match("/>") or stream:match("%?>") then
				state.mode, state.tag, state.closing, state.attr, state.value, state.said = nil, nil, nil, nil, nil, nil
				return "tag"
			elseif stream:match(">") then
				local name = state.tag and string.lower(state.tag)
				local closing, said = state.closing, state.said
				state.mode, state.tag, state.closing, state.attr, state.value, state.said = nil, nil, nil, nil, nil, nil
				if embeds and not closing and CLOSE[name] then
					state.mode, state.tag = "raw", name
					state.inner = embedded_name(name, said)
					local reader = state.inner and context.reader(state.inner)
					if reader then
						state.mode = "embedded"
						state.inner_state = reader.start()
					else
						state.inner = nil
					end
				end
				return "tag"
			end
			if state.value then
				-- The value after a `=`: quoted, or a word.
				state.value = nil
				local quote = stream:peek()
				local from, to = stream.pos, nil
				if quote == '"' or quote == "'" then
					stream:next()
					from = stream.pos
					if common.quoted(stream, quote, false) then
						to = stream.pos - 2
					else
						state.mode, state.quote = "value", quote
					end
				elseif not stream:match("[^%s\"'<>=`]+") then
					stream:next()
					return nil
				end
				if state.attr then
					state.said = string.sub(stream.string, from, to or stream.pos - 1)
					state.attr = nil
				end
				return "string"
			end
			if stream:match("=") then
				state.value = true
				return "operator"
			end
			local attr = stream:match("[^%s\"'<>=/`]+")
			if attr then
				local lower = string.lower(attr)
				state.attr = (lower == "lang" or lower == "type") and lower or nil
				return "attribute"
			end
			stream:next()
			return nil
		end
		if stream:match("<!%-%-") then
			state.mode = "comment"
			if stream:eol() then
				return "comment"
			end
			return token(stream, state)
		elseif stream:match("<!%[CDATA%[") then
			state.mode = "cdata"
			if stream:eol() then
				return "string"
			end
			return token(stream, state)
		elseif stream:match("<!%a+") or stream:match("<%?[%w%-]*") then
			state.mode = "tag"
			return "meta"
		end
		local name = stream:match("</([%a_][%w_:%.%-]*)")
		if name then
			state.mode, state.tag, state.closing = "tag", name, true
			return "tag"
		end
		name = stream:match("<([%a_][%w_:%.%-]*)")
		if name then
			state.mode, state.tag, state.closing = "tag", name, nil
			return "tag"
		end
		if stream:match("&[#%w]+;") then
			return "atom"
		end
		stream:next()
		while not stream:eol() do
			local c = stream:peek()
			if c == "<" or c == "&" then
				break
			end
			stream:next()
		end
		return nil
	end

	return { start = start, token = token }
end

-- --- Markdown ----------------------------------------------------------------------------------

-- What is inside a line of prose: code, strong, emphasis, struck words, links, escapes.
local function inline(stream)
	if stream:match("\\[%p]") then
		return "escape"
	end
	local ticks = stream:match("`+")
	if ticks then
		if stream:skip_to(ticks) then
			stream:match(literal(ticks))
		else
			stream:skip_to_end()
		end
		return "code"
	end
	-- An `_` inside a word (`snake_case`) is the word's.
	local in_word = stream.pos > 1 and string.find(string.sub(stream.string, stream.pos - 1, stream.pos - 1), "%w") ~= nil
	if stream:match("%*%*[^%*]+%*%*") or (not in_word and stream:match("__[^_]+__")) then
		return "strong"
	end
	if stream:match("%*[^%*%s][^%*]*%*") or (not in_word and stream:match("_[^_%s][^_]*_")) then
		return "emphasis"
	end
	if stream:match("~~[^~]+~~") then
		return "strike"
	end
	if stream:match("!?%[[^%]%[]*%]") then
		return "link"
	end
	if
		stream.pos > 1
		and string.byte(stream.string, stream.pos - 1) == 93 -- "]"
		and stream:match("%([^%)%s]*%)")
	then
		return "url"
	end
	if stream:match("<https?://[^>%s]+>") or stream:match("https?://[%w%-%._~:/%?#%[%]@!%$&'%*%+,;=%%]+") then
		return "url"
	end
	if stream:match("[%w%s]+") then
		return nil
	end
	stream:next()
	return nil
end

local function markdown(context)
	-- The state: `fence` the ticks or tildes of an open fence, `inner` the name of its language and
	-- `inner_state` its state (none for a fence of no language this module knows).
	local function start()
		return {}
	end
	local function token(stream, state)
		local fence = state.fence
		if fence then
			if stream:sol() then
				local close = stream:match("%s*([`~]+)%s*$", false)
				if close and string.sub(close, 1, 1) == string.sub(fence, 1, 1) and #close >= #fence then
					stream:skip_to_end()
					state.fence, state.inner, state.inner_state = nil, nil, nil
					return "meta"
				end
			end
			local reader = state.inner and context.reader(state.inner)
			if reader then
				return reader.token(stream, state.inner_state)
			end
			stream:skip_to_end()
			return "code"
		end
		if stream:sol() then
			local open, info = stream:match("%s*(```+)%s*([^`%s]*)")
			if not open then
				open, info = stream:match("%s*(~~~+)%s*([^%s]*)")
			end
			if open then
				state.fence = open
				-- The info string's first word names the language (`rust,ignore`, `{.python}`).
				local name = string.match(info, "^[{%.]*([%w_%+%-#]+)")
				local reader = name and context.reader(name)
				if reader then
					state.inner, state.inner_state = name, reader.start()
				end
				stream:skip_to_end()
				return "meta"
			end
			if stream:match("%s*#+%s") or stream:match("%s*#+$") then
				stream:skip_to_end()
				return "heading"
			end
			if stream:match("%s*[%-%*_]%s*[%-%*_]%s*[%-%*_][%s%-%*_]*$") then
				return "meta"
			end
			if stream:match("%s*>") then
				stream:skip_to_end()
				return "quote"
			end
			if stream:match("%s*[%-%*%+]%s") or stream:match("%s*%d+[%.%)]%s") then
				return "list"
			end
		end
		-- The box of a task, after a list's mark.
		if common.after(stream, "[ \t]*[%-%*%+][ \t]+") and stream:match("%[[ xX]%]") then
			return "atom"
		end
		if stream:match("|") then
			return "punctuation"
		end
		return inline(stream)
	end
	return { start = start, token = token }
end

-- --- The languages -----------------------------------------------------------------------------

local TAGS_INDENT = {
	HTML = { increase = "<([a-zA-Z][\\w.-]*)(\\s[^<>]*)?>[^<]*$", decrease = "^\\s*</" },
	XML = { increase = "<([a-zA-Z][\\w.:-]*)(\\s[^<>]*)?>[^<]*$", decrease = "^\\s*</" },
}

local markup = {}

-- The language named `name` of the family: its reader, and what an editor is told of it.
function markup.make(name, context)
	if name == "Markdown" then
		return {
			reader = markdown(context),
			config = { comment = { block = { "<!--", "-->" } }, pairs = "()[]{}``", folds = "indent" },
		}
	end
	return {
		reader = tags(context, name == "HTML"),
		config = {
			comment = { block = { "<!--", "-->" } },
			indent = TAGS_INDENT[name],
			pairs = "()[]{}\"\"''",
			folds = "indent",
		},
	}
end

return markup
