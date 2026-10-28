-- @pencere/languages/tools: the languages of the tools around code: the Dockerfile, the Makefile and
-- a diff. `@pencere/languages` requires it when one of them is first loaded.

local common = require("./common")

local set, first_on_line = common.set, common.first_on_line

-- A string in `quote` from where the stream is (past the quote), `\` taking the character after it.
local function quoted_string(stream, quote)
	while not stream:eol() do
		local next_char = stream:next()
		if next_char == "\\" then
			stream:next()
		elseif next_char == quote then
			break
		end
	end
	return "string"
end

-- --- Dockerfile --------------------------------------------------------------------------------

local DOCKER = set(
	"FROM RUN CMD LABEL MAINTAINER EXPOSE ENV ADD COPY ENTRYPOINT VOLUME USER WORKDIR ARG ONBUILD "
		.. "STOPSIGNAL HEALTHCHECK SHELL"
)

local function dockerfile_token(stream)
	if stream:sol() and stream:match("%s*#") then
		stream:skip_to_end()
		return "comment"
	end
	if stream:eat_space() then
		return nil
	end
	local word = stream:match("%a+", false)
	if word and first_on_line(stream) and DOCKER[string.upper(word)] then
		stream:match("%a+")
		return "keyword"
	end
	if word and string.upper(word) == "AS" and stream:match("%a+%s", false) then
		stream:match("%a+")
		return "keyword"
	end
	if stream:match("%-%-[%w%-]+") then
		return "attribute"
	end
	if stream:match("%${[^}]*}") or stream:match("%$[%a_][%w_]*") then
		return "variable.special"
	end
	local c = stream:peek()
	if c == '"' or c == "'" then
		stream:next()
		return quoted_string(stream, c)
	end
	if stream:match("[%[%],=]") then
		return "punctuation"
	end
	stream:match("[^%s%$\"'=%[%],]+")
	if stream.pos == stream.start then
		stream:next()
	end
	return nil
end

-- --- Makefile ----------------------------------------------------------------------------------

local MAKE = set(
	"include -include sinclude ifeq ifneq ifdef ifndef else endif define endef export unexport override vpath"
)

local function makefile_token(stream)
	-- A recipe (a line that starts with a tab) is the shell's: its variables and comments are marked.
	local recipe = string.byte(stream.string, 1) == 9
	if stream:match("#") then
		stream:skip_to_end()
		return "comment"
	end
	if stream:eat_space() then
		return nil
	end
	if stream:match("%$%([^%)]*%)") or stream:match("%${[^}]*}") or stream:match("%$[%w@<%^%*%?%%+]") then
		return "variable.special"
	end
	if not recipe and first_on_line(stream) then
		local word = stream:match("%-?%a+", false)
		if word and MAKE[word] then
			stream:match("%-?%a+")
			return "keyword"
		end
		if stream:match("[%w_%.%-]+%s*[:%?%+!]?=", false) then
			stream:match("[%w_%.%-]+")
			return "property"
		end
		if stream:match("[^:#=%s][^:#=]-:", false) and not stream:match("[^:#=]-:=", false) then
			stream:match("[^:#=]-:")
			return "function"
		end
	end
	local c = stream:peek()
	if c == '"' or c == "'" then
		stream:next()
		stream:skip_to(c)
		stream:eat(c)
		return "string"
	end
	stream:match("[^%s%$#\"']+")
	if stream.pos == stream.start then
		stream:next()
	end
	return nil
end

-- --- Diff --------------------------------------------------------------------------------------

local function diff_token(stream)
	local style = nil
	if
		stream:match("%+%+%+")
		or stream:match("%-%-%-")
		or stream:match("diff ")
		or stream:match("index ")
		or stream:match("new file")
		or stream:match("deleted file")
		or stream:match("similarity")
		or stream:match("rename ")
	then
		style = "meta"
	elseif stream:match("@@") then
		style = "changed"
	elseif stream:match("%+") then
		style = "inserted"
	elseif stream:match("%-") then
		style = "deleted"
	end
	stream:skip_to_end()
	return style
end

-- --- The languages -----------------------------------------------------------------------------

local LANGUAGES = {
	Dockerfile = {
		token = dockerfile_token,
		config = { comment = "#", pairs = "()[]{}''\"\"", folds = "indent" },
	},
	Makefile = {
		token = makefile_token,
		-- A recipe starts with a tab.
		config = { comment = "#", pairs = "()[]{}''\"\"", folds = "indent", hard_tabs = true },
	},
	Diff = { token = diff_token, config = {} },
}

local tools = {}

-- The language named `name` of the family: its reader, and what an editor is told of it.
function tools.make(name)
	local language = LANGUAGES[name]
	return {
		reader = {
			start = function()
				return {}
			end,
			token = language.token,
		},
		config = table.clone(language.config),
	}
end

return tools
