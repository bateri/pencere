-- @pencere/languages/data: the languages of data and settings: JSON, YAML, TOML, and INI with its
-- kin (properties, .env). A key is coloured as a property, its value as what it is.
-- `@pencere/languages` requires it when one of them is first loaded.

local common = require("./common")

local first_on_line, quoted = common.first_on_line, common.quoted

local NUMBER = {
	"^%-?0[xX]%x+",
	"^[%-%+]?%d[%d_]*%.?[%d_]*[eE][%+%-]?%d+",
	"^[%-%+]?%d[%d_]*%.%d[%d_]*",
	"^[%-%+]?%d[%d_]*",
}

local function number(stream)
	for _, pattern in NUMBER do
		if stream:match(pattern) then
			return true
		end
	end
	return false
end

-- Whether what comes next ends a plain value: the end of the line, a space, a separator.
local function ends_value(stream)
	return stream:eol() or stream:match("[%s,%]}]", false) ~= nil
end

-- --- JSON --------------------------------------------------------------------------------------

local function json_token(stream, state)
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
	if stream:match("//") then
		stream:skip_to_end()
		return "comment"
	elseif stream:match("/%*") then
		state.comment = true
		if stream:skip_to("*/") then
			stream:match("%*/")
			state.comment = nil
		else
			stream:skip_to_end()
		end
		return "comment"
	end
	if stream:eat('"') then
		quoted(stream, '"', true)
		return stream:match("%s*:", false) and "property" or "string"
	end
	if number(stream) then
		return "number"
	end
	if stream:match("true") or stream:match("false") or stream:match("null") then
		return "atom"
	end
	if stream:match("[{}%[%],:]") then
		return "punctuation"
	end
	stream:next()
	return "invalid"
end

-- --- YAML --------------------------------------------------------------------------------------

local YAML_ATOMS = common.set("true false yes no on off null True False Yes No Null TRUE FALSE NULL ~")

local function yaml_token(stream, state)
	-- A block scalar (`|`, `>`): the lines further in than the key that opened it.
	if state.block and stream:sol() then
		if string.find(stream.string, "^%s*$") or stream:indentation() > state.block then
			stream:skip_to_end()
			return "string"
		end
		state.block = nil
	end
	if stream:sol() and (stream:match("%-%-%-") or stream:match("%.%.%.")) then
		return "meta"
	end
	if stream:eat_space() then
		return nil
	end
	if stream:match("#", false) and (stream:sol() or string.find(string.sub(stream.string, stream.pos - 1, stream.pos - 1), "%s")) then
		stream:skip_to_end()
		return "comment"
	end
	-- A key: first on its line (after a list's `-`), up to its colon.
	if first_on_line(stream) or common.after(stream, "[ \t]*%-[ \t]+") then
		if
			stream:match("[^#:%s%-%[{][^#:]-%s*:%s", false)
			or stream:match("[^#:%s%-%[{][^#:]-%s*:$", false)
			or stream:match('"[^"]*"%s*:', false)
			or stream:match("'[^']*'%s*:", false)
		then
			stream:match("[^:]+")
			return "property"
		end
		if stream:match("%-%s", false) or stream:match("%-$", false) then
			stream:next()
			return "punctuation"
		end
	end
	if stream:match("[|>][%-%+]?%d*%s*$") or stream:match("[|>][%-%+]?%d*%s+#", false) then
		local _, indent = string.find(stream.string, "^ *")
		state.block = indent
		if not stream:eol() then
			stream:match("[|>][%-%+]?%d*")
		end
		return "meta"
	end
	if stream:eat('"') then
		quoted(stream, '"', true)
		return "string"
	elseif stream:eat("'") then
		quoted(stream, "'", false)
		return "string"
	end
	if stream:match("[&%*][%w_%-]+") then
		return "variable.special"
	end
	if stream:match("!!?[%w_%-/]*") then
		return "meta"
	end
	if stream:match("[:,%[%]{}]") then
		return "punctuation"
	end
	local word = stream:match("[%w~]+", false)
	if word and YAML_ATOMS[word] then
		local after = string.sub(stream.string, stream.pos + #word, stream.pos + #word)
		if after == "" or string.find(after, "^[%s,%]}]") then
			stream:match("[%w~]+")
			return "atom"
		end
	end
	if number(stream) then
		if ends_value(stream) then
			return "number"
		end
		stream.pos = stream.start
	end
	-- A plain value runs to a comment, or to what ends it in a flow (`,`, `]`, `}`).
	stream:match("[^#,%[%]{}]+")
	if stream.pos == stream.start then
		stream:next()
	end
	local words = stream:current()
	local hash = string.find(words, "%s#")
	if hash then
		stream.pos = stream.start + hash
	end
	return "string"
end

-- --- TOML --------------------------------------------------------------------------------------

local function toml_token(stream, state)
	if state.str then
		if quoted(stream, state.str, state.str == '"""') then
			state.str = nil
		end
		return "string"
	end
	if stream:eat_space() then
		return nil
	end
	if stream:match("#") then
		stream:skip_to_end()
		return "comment"
	end
	local first = first_on_line(stream)
	if first and stream:match("%[%[?[^%]]*%]%]?") then
		return "type"
	end
	if first and stream:match("[%w_%-%.\"' ]-%s*=", false) then
		local key = stream:match("[^=]+")
		local trailing = #string.match(key, "%s*$")
		if trailing > 0 then
			stream:backup(trailing)
		end
		return "property"
	end
	for _, close in { '"""', "'''" } do
		if stream:match(close) then
			if not quoted(stream, close, close == '"""') then
				state.str = close
			end
			return "string"
		end
	end
	if stream:eat('"') then
		quoted(stream, '"', true)
		return "string"
	elseif stream:eat("'") then
		quoted(stream, "'", false)
		return "string"
	end
	if stream:match("%d%d%d%d%-%d%d%-%d%d[T %d:%.Z%+%-]*") or stream:match("%d%d:%d%d:%d%d[%.%d]*") then
		return "number"
	end
	if number(stream) then
		return "number"
	end
	if stream:match("true") or stream:match("false") or stream:match("[%+%-]?inf") or stream:match("[%+%-]?nan") then
		return "atom"
	end
	if stream:match("[=,%[%]{}%.]") then
		return "punctuation"
	end
	stream:next()
	return nil
end

-- --- INI, properties, .env ---------------------------------------------------------------------

local function ini_token(stream, state)
	if stream:sol() then
		if stream:match("%s*[;#!]") then
			stream:skip_to_end()
			return "comment"
		end
		if stream:match("%s*%[[^%]]*%]") then
			return "type"
		end
		if stream:match("%s*export%s+") then
			return "keyword"
		end
	end
	if stream:eat_space() then
		return nil
	end
	if
		(first_on_line(stream) or common.after(stream, "[ \t]*export[ \t]+"))
		and stream:match("[^=:%s]+%s*[=:]", false)
	then
		stream:match("[^=:%s]+")
		return "property"
	end
	if stream:match("[=:]") then
		return "operator"
	end
	if stream:eat('"') then
		quoted(stream, '"', true)
		return "string"
	elseif stream:eat("'") then
		quoted(stream, "'", false)
		return "string"
	end
	if stream:match("%${[^}]*}") or stream:match("%$[%a_][%w_]*") then
		return "variable.special"
	end
	if stream:match("true%s*$") or stream:match("false%s*$") then
		return "atom"
	end
	if number(stream) then
		if stream:eol() or stream:match("%s", false) then
			return "number"
		end
		stream.pos = stream.start
	end
	stream:skip_to_end()
	return "string"
end

-- --- The languages -----------------------------------------------------------------------------

local LANGUAGES = {
	JSON = {
		token = json_token,
		config = {
			comment = { line = "//", block = { "/*", "*/" } },
			indent = { decrease = "^\\s*[\\}\\]]" },
			pairs = '()[]{}""',
		},
	},
	YAML = {
		token = yaml_token,
		config = {
			comment = "#",
			indent = { increase = ":\\s*(#.*)?$|^\\s*-\\s*$|[|>][-+]?\\s*$" },
			pairs = "()[]{}''\"\"",
			-- A tab is no indent in YAML.
			hard_tabs = false,
			tab_width = 2,
		},
	},
	TOML = { token = toml_token, config = { comment = "#", pairs = "()[]{}''\"\"" } },
	INI = { token = ini_token, config = { comment = "#", pairs = "()[]{}\"\"" } },
}

local data = {}

-- The language named `name` of the family: its reader, and what an editor is told of it.
function data.make(name)
	local language = LANGUAGES[name]
	local config = table.clone(language.config)
	config.folds = "indent"
	return {
		reader = {
			start = function()
				return {}
			end,
			token = language.token,
		},
		config = config,
	}
end

return data
