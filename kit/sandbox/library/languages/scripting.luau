-- @pencere/languages/scripting: the scripting languages, Python, Ruby, the shell and Lua, each a
-- reader of its own (their comments, strings and blocks have little in common), with the words, the
-- indent and the brackets each says. `@pencere/languages` requires it when one of them is first
-- loaded.

local common = require("./common")

local set, literal, first_on_line = common.set, common.literal, common.first_on_line

local IDENT = "^[%a_\128-\255][%w_\128-\255]*"

local function number(stream)
	return (
		stream:match("0[xX][%x_]+")
		or stream:match("0[bBoO][%x_]+")
		or stream:match("%d[%d_]*%.?[%d_]*[eE][%+%-]?%d+")
		or stream:match("%d[%d_]*%.%d[%d_]*")
		or stream:match("%d[%d_]*")
		or stream:match("%.%d[%d_]*")
	) ~= nil
end

-- A string to `close` (escapes with `\` where `escapes`); one that goes past the line is kept open
-- in `state.str` where `many`.
local function read_string(stream, state, close, escapes, many)
	local pattern = literal(close)
	while not stream:eol() do
		if escapes and stream:eat("\\") then
			stream:next()
		elseif stream:match(pattern) then
			state.str = nil
			return "string"
		else
			stream:next()
		end
	end
	state.str = many and { close = close, escapes = escapes } or nil
	return "string"
end

local function operator_or_punctuation(stream)
	if stream:match("[%(%)%[%]{};,%.]") then
		return "punctuation"
	elseif stream:match("[%+%-%*/%%=&|<>!%?:~%^@]+") then
		return "operator"
	end
	stream:next()
	return nil
end

-- The style of the word `word`, by the words of `o`; `def` the style the keyword before it gives
-- a name after it, `dot` whether a `.` is before it.
local function word_kind(stream, word, o, def, dot)
	if o.keywords[word] then
		return "keyword"
	elseif o.atoms and o.atoms[word] then
		return "atom"
	elseif o.special and o.special[word] then
		return "variable.special"
	elseif o.builtins and o.builtins[word] then
		return "function"
	elseif def then
		return def
	elseif stream:match("%s*%(", false) then
		return "function"
	elseif dot then
		return "property"
	end
	local first = string.byte(word, 1)
	if o.capitals and first >= 65 and first <= 90 then
		return (#word > 1 and not string.find(word, "%l")) and "constant" or "type"
	end
	return nil
end

-- What a token says of the next one alone (a keyword that names what comes after it, a `.`) does
-- not go on to the next line: a line that leaves nothing open leaves the empty state.
local function at_line_end(stream, state)
	if stream:eol() then
		state.def, state.dot = nil, nil
	end
end

-- --- Python ------------------------------------------------------------------------------------

local PYTHON = {
	keywords = set(
		"and as assert async await break class continue def del elif else except finally for from global "
			.. "if import in is lambda nonlocal not or pass raise return try while with yield match case type"
	),
	atoms = set("True False None Ellipsis NotImplemented"),
	special = set("self cls"),
	builtins = set(
		"print len range enumerate zip map filter sorted reversed open isinstance super int str float "
			.. "list dict set tuple bool bytes type repr abs min max sum any all iter next hasattr getattr setattr"
	),
	definers = { def = "function", class = "type" },
	capitals = true,
}

local function python_token(stream, state)
	if state.str then
		return read_string(stream, state, state.str.close, state.str.escapes, true)
	end
	if stream:eat_space() then
		return nil
	end
	local dot = state.dot
	state.dot = nil
	if stream:match("#") then
		stream:skip_to_end()
		return "comment"
	end
	if first_on_line(stream) and stream:match("@[%w_%.]+") then
		return "meta"
	end
	-- A string, with its prefix (r, b, f, u and their pairs): three quotes go over lines.
	local prefix = stream:match("[rRbBfFuU][rRbBfF]?", false)
	local at = stream.pos + (prefix and #prefix or 0)
	local q = string.sub(stream.string, at, at)
	if q == '"' or q == "'" then
		if prefix then
			stream:match("[rRbBfFuU][rRbBfF]?")
		end
		local raw = prefix and string.find(prefix, "[rR]") ~= nil
		if stream:match(q .. q .. q) then
			return read_string(stream, state, q .. q .. q, not raw, true)
		end
		stream:next()
		return read_string(stream, state, q, not raw, false)
	end
	if number(stream) then
		stream:match("[jJlL]")
		return "number"
	end
	local word = stream:match(IDENT)
	if word then
		local def = state.def
		state.def = PYTHON.keywords[word] and PYTHON.definers[word] or nil
		if PYTHON.keywords[word] then
			return "keyword"
		end
		return word_kind(stream, word, PYTHON, def, dot)
	end
	state.def = nil
	if stream:match("%.") then
		state.dot = true
		return "punctuation"
	end
	return operator_or_punctuation(stream)
end

-- --- Ruby --------------------------------------------------------------------------------------

local RUBY = {
	keywords = set(
		"BEGIN END alias and begin break case class def defined? do else elsif end ensure for if in module "
			.. "next not or redo rescue retry return super then undef unless until when while yield require "
			.. "require_relative include extend attr_reader attr_writer attr_accessor private protected public "
			.. "raise lambda proc"
	),
	atoms = set("true false nil"),
	special = set("self"),
	definers = { def = "function", class = "type", module = "type" },
	capitals = true,
}

local function ruby_token(stream, state)
	if state.doc then
		if stream:sol() and stream:match("=end") then
			state.doc = nil
		end
		stream:skip_to_end()
		return "comment"
	end
	if state.str then
		return read_string(stream, state, state.str.close, state.str.escapes, true)
	end
	if stream:sol() and stream:match("=begin") then
		state.doc = true
		stream:skip_to_end()
		return "comment"
	end
	if stream:eat_space() then
		return nil
	end
	if stream:match("#") then
		stream:skip_to_end()
		return "comment"
	end
	local q = stream:peek()
	if q == '"' or q == "'" or q == "`" then
		stream:next()
		return read_string(stream, state, q, q ~= "'", true)
	end
	if stream:match(":[%a_][%w_]*[%?!]?") then
		return "atom"
	end
	if stream:match("@@?[%a_][%w_]*") or stream:match("%$[%a_][%w_]*") then
		return "variable.special"
	end
	if number(stream) then
		return "number"
	end
	local word = stream:match("[%a_][%w_]*[%?!]?")
	if word then
		local def = state.def
		state.def = RUBY.keywords[word] and RUBY.definers[word] or nil
		if RUBY.keywords[word] then
			return "keyword"
		end
		return word_kind(stream, word, RUBY, def, false)
	end
	state.def = nil
	return operator_or_punctuation(stream)
end

-- --- The shell ---------------------------------------------------------------------------------

local SHELL = {
	keywords = set(
		"if then else elif fi case esac for select while until do done in function time coproc return "
			.. "break continue exit export local readonly declare typeset unset shift source alias"
	),
	builtins = set("echo printf cd pwd read test eval exec set trap wait kill command type which"),
}

local function shell_token(stream, state)
	-- The lines of a here-document, to the line that is its word.
	local here = state.here
	if here and stream:sol() then
		local word = string.match(stream.string, "^%s*(.-)%s*$")
		stream:skip_to_end()
		if word == here then
			state.here = nil
			return "meta"
		end
		return "string"
	end
	if state.str then
		return read_string(stream, state, state.str.close, state.str.escapes, true)
	end
	if stream:eat_space() then
		return nil
	end
	if stream:match("#") then
		stream:skip_to_end()
		return "comment"
	end
	local q = stream:peek()
	if q == '"' or q == "'" or q == "`" then
		stream:next()
		return read_string(stream, state, q, q ~= "'", true)
	end
	local word = stream:match("<<%-?%s*['\"]?([%a_][%w_]*)['\"]?")
	if word then
		state.here = word
		return "meta"
	end
	if
		stream:match("%${[^}]*}?")
		or stream:match("%$%(%(?")
		or stream:match("%$[%a_][%w_]*")
		or stream:match("%$[%w_@%*#%?%$!%-]")
	then
		return "variable.special"
	end
	if stream:match("%-%-?%a[%w%-]*") then
		return "attribute"
	end
	if stream:match("%d+") then
		return "number"
	end
	word = stream:match("[%a_][%w_%-]*")
	if word then
		if stream:match("=", false) then
			return "property"
		elseif SHELL.keywords[word] then
			return "keyword"
		elseif SHELL.builtins[word] then
			return "function"
		end
		return nil
	end
	if stream:match("[|&;<>]+") then
		return "operator"
	end
	stream:next()
	return nil
end

-- --- Lua ---------------------------------------------------------------------------------------

local LUA = {
	keywords = set(
		"and break do else elseif end for function goto if in local not or repeat return then until while "
			.. "continue export type"
	),
	atoms = set("true false nil"),
	special = set("self"),
	builtins = set(
		"print pairs ipairs next type tostring tonumber require error assert pcall xpcall select setmetatable "
			.. "getmetatable rawget rawset rawequal unpack"
	),
	definers = { ["function"] = "function" },
}

-- The rest of a long bracket (`[[`, `[==[`) of level `level` from where the stream is.
local function long_bracket(stream, state, level, style)
	local close = "]" .. level .. "]"
	if stream:skip_to(close) then
		stream:match(literal(close))
		state.long, state.comment = nil, nil
	else
		stream:skip_to_end()
		state.long, state.comment = level, style == "comment" or nil
	end
	return style
end

local function lua_token(stream, state)
	if state.long then
		return long_bracket(stream, state, state.long, state.comment and "comment" or "string")
	end
	if stream:eat_space() then
		return nil
	end
	local dot = state.dot
	state.dot = nil
	local level = stream:match("%-%-%[(=*)%[")
	if level then
		return long_bracket(stream, state, level, "comment")
	end
	if stream:match("%-%-") then
		stream:skip_to_end()
		return "comment"
	end
	level = stream:match("%[(=*)%[")
	if level then
		return long_bracket(stream, state, level, "string")
	end
	local q = stream:peek()
	if q == '"' or q == "'" or q == "`" then
		stream:next()
		return read_string(stream, {}, q, true, false)
	end
	if number(stream) then
		return "number"
	end
	local word = stream:match(IDENT)
	if word then
		local def = state.def
		state.def = LUA.keywords[word] and LUA.definers[word] or nil
		if LUA.keywords[word] then
			return "keyword"
		end
		return word_kind(stream, word, LUA, def, dot)
	end
	state.def = nil
	if stream:match("%.%.%.?") then
		return "operator"
	end
	if stream:match("[%.:]") then
		state.dot = true
		return "punctuation"
	end
	return operator_or_punctuation(stream)
end

-- --- The languages -----------------------------------------------------------------------------

local function reader(read)
	return {
		start = function()
			return {}
		end,
		token = function(stream, state)
			local style = read(stream, state)
			at_line_end(stream, state)
			return style
		end,
	}
end

local LANGUAGES = {
	Python = {
		read = python_token,
		config = {
			comment = "#",
			indent = { increase = ":\\s*(#.*)?$", decrease = "^\\s*(elif|else|except|finally|case)\\b.*:\\s*(#.*)?$" },
			pairs = "()[]{}''\"\"",
			-- The indent is the language's own (PEP 8: four spaces).
			hard_tabs = false,
			tab_width = 4,
		},
	},
	Ruby = {
		read = ruby_token,
		config = {
			comment = { line = "#", block = { "=begin", "=end" } },
			indent = {
				increase = "^\\s*(def|class|module|if|unless|while|until|for|begin|case|else|elsif|when|rescue|ensure)\\b|\\bdo(\\s*\\|[^|]*\\|)?\\s*$",
				decrease = "^\\s*(end|else|elsif|when|rescue|ensure)\\b",
			},
			pairs = "()[]{}''\"\"",
		},
	},
	Shell = {
		read = shell_token,
		config = {
			comment = "#",
			indent = {
				increase = "\\b(then|do|else|in)\\s*$|\\{\\s*$|\\(\\s*$",
				decrease = "^\\s*(fi|done|esac|else|elif)\\b|^\\s*[\\}\\)]",
			},
			pairs = "()[]{}''\"\"``",
		},
	},
	Lua = {
		read = lua_token,
		config = {
			comment = { line = "--", block = { "--[[", "]]" } },
			indent = {
				increase = "\\b(then|do|else|repeat)\\s*$|\\bfunction\\b[^)]*\\)\\s*$|\\{\\s*$",
				decrease = "^\\s*(end|else|elseif|until)\\b|^\\s*\\}",
			},
			pairs = "()[]{}''\"\"",
		},
	},
}

local scripting = {}

-- The language named `name` of the family: its reader, and what an editor is told of it.
function scripting.make(name)
	local language = LANGUAGES[name]
	local config = table.clone(language.config)
	config.folds = "indent"
	return { reader = reader(language.read), config = config }
end

return scripting
