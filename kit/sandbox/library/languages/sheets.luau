-- @pencere/languages/sheets: the style sheets: CSS, and SCSS and Less with their variables, nesting
-- and line comments. Outside a rule's braces a word is a selector (a tag, `.class`, `#id`,
-- `:state`); inside them a word before a colon is a property and what follows it, to the semicolon,
-- its value. `@pencere/languages` requires it when one of them is first loaded.

local common = require("./common")

local function reader(dialect)
	local line_comments = dialect ~= "CSS"
	-- Where the next `{` is on the line: a word before a colon on a line that opens no rule is a
	-- property (`a:hover {` is a selector).
	local find_brace = common.finder()

	-- The state: `comment` in a block comment, `depth` how many braces are open (nil for none),
	-- `value` in a property's value.
	local function start()
		return {}
	end

	local function token(stream, state)
		if state.comment then
			if stream:skip_to("*/") then
				stream:match("%*/")
				state.comment = nil
			else
				stream:skip_to_end()
			end
			return "comment"
		end
		if stream:eat_space() then
			return nil
		end
		if stream:match("/%*") then
			state.comment = true
			if stream:skip_to("*/") then
				stream:match("%*/")
				state.comment = nil
			else
				stream:skip_to_end()
			end
			return "comment"
		end
		if line_comments and stream:match("//") then
			stream:skip_to_end()
			return "comment"
		end
		local c = stream:peek()
		if c == '"' or c == "'" then
			stream:next()
			while not stream:eol() do
				local next_char = stream:next()
				if next_char == "\\" then
					stream:next()
				elseif next_char == c then
					break
				end
			end
			return "string"
		end
		if c == "{" then
			stream:next()
			state.depth = (state.depth or 0) + 1
			state.value = nil
			return "punctuation"
		elseif c == "}" then
			stream:next()
			local depth = (state.depth or 0) - 1
			state.depth = depth > 0 and depth or nil
			state.value = nil
			return "punctuation"
		elseif c == ";" then
			stream:next()
			state.value = nil
			return "punctuation"
		end
		if stream:match("@[%w_%-]+") then
			-- A variable of Less is said where it is set (`@gap: 4px`) and used (in a value).
			if dialect == "Less" and (state.value or stream:match("%s*:", false)) then
				return "variable.special"
			end
			return "keyword"
		end
		if dialect == "SCSS" and stream:match("%$[%w_%-]+") then
			return "variable.special"
		end
		if stream:match("!important") or stream:match("!default") then
			return "keyword"
		end
		if state.value then
			if stream:match("#%x+") then
				return "number"
			end
			if stream:match("[%-%+]?%d*%.?%d+[%a%%]*") then
				return "number"
			end
			local word = stream:match("[%a_%-][%w_%-]*")
			if word then
				if stream:match("%(", false) then
					return "function"
				end
				return "atom"
			end
			if stream:match("[:,%(%)/]") then
				return "punctuation"
			end
			stream:next()
			return nil
		end
		-- A property: a word inside braces before a colon, on a line that opens no rule.
		if
			state.depth
			and stream:match("[%a_%-][%w_%-]*%s*:[^:]", false)
			and find_brace(stream, "{") == nil
		then
			stream:match("[%a_%-][%w_%-]*")
			state.value = true
			return "property"
		end
		if stream:match("%.[%a_%-][%w_%-]*") then
			return "type"
		elseif stream:match("#[%a_%-][%w_%-]*") then
			return "constant"
		elseif stream:match("::?[%a%-]+") then
			return "keyword"
		elseif stream:match("%[[^%]%[]*%]") then
			return "attribute"
		elseif stream:match("&") then
			return "keyword"
		elseif stream:match("%a[%w%-]*") then
			return "tag"
		elseif stream:match("[>+~,*]") then
			return "operator"
		end
		stream:next()
		return nil
	end

	return { start = start, token = token }
end

local sheets = {}

-- The language named `name` of the family: its reader, and what an editor is told of it.
function sheets.make(name)
	return {
		reader = reader(name),
		config = {
			comment = name == "CSS" and { block = { "/*", "*/" } } or { line = "//", block = { "/*", "*/" } },
			indent = { decrease = "^\\s*\\}" },
			pairs = "()[]{}''\"\"",
			folds = "indent",
		},
	}
end

return sheets
