-- @pencere/languages/clike: the languages of braces and semicolons (JavaScript, JSX, TypeScript,
-- TSX, Rust, Go, C, C++, C#, Java, Kotlin, Swift, PHP), one reader made for each by what it says:
-- its words, how it writes comments and strings, whether a capital starts a type's name, and the few
-- things one of them has alone (a template's `${}`, a regular expression, a raw string, a lifetime,
-- a tag of JSX). `@pencere/languages` requires it when one of them is first loaded.
--
-- The state a line leaves open is small: a block comment (how deep), a string that goes on, the
-- `${}` of templates, the inside of a tag. What a line says of the next token alone (a word that
-- names a function, a value before a `/`) goes with the line's end, so that a line that leaves
-- nothing open leaves the empty state, which the lines after it share.

local common = require("./common")

local set, literal, first_on_line = common.set, common.literal, common.first_on_line

local IDENT = "^[%a_\128-\255][%w_\128-\255]*"
local IDENT_DOLLAR = "^[%a_%$\128-\255][%w_%$\128-\255]*"

-- The words after which a `/` starts a regular expression (and does not divide).
local REGEX_AFTER = set("return typeof instanceof in of new delete void throw case do else yield await")

-- A reader for a language: `o` is
--   keywords, types, atoms, special    words (`set`), coloured as such
--   definers   { [keyword] = style }: the name after the keyword is a function's or a type's
--   line       line comments ({ "//" }), block = { "/*", "*/" }, nested (a block in a block)
--   quotes     { ['"'] = true, ["'"] = true, ["`"] = "template" | "raw" }: one line, `${}`, many lines
--   triple     `"""` strings over lines; raw = "rust" | "csharp" | "cpp"
--   capitals   a word that starts with a capital is a type (ALL_CAPS a constant)
--   dollar     `$` in names; variables = `$name` coloured as one (PHP)
--   regex, jsx, lifetimes, preprocessor, annotations, macros, php_tags, dot_operators, generics_call
local function reader(o)
	local ident = o.dollar and IDENT_DOLLAR or IDENT
	local line_comments = {}
	for _, token in o.line or {} do
		line_comments[#line_comments + 1] = literal(token)
	end
	local block_open = o.block and literal(o.block[1])
	local block_close = o.block and literal(o.block[2])
	local keywords = o.keywords or {}
	local types = o.types or {}
	local atoms = o.atoms or {}
	local special = o.special or {}
	local definers = o.definers or {}

	-- Nothing open: no block comment, no string, no template, no tag.
	local function start()
		return {}
	end

	-- A block comment from where the stream is, to its end or the end of the line.
	local function block_comment(stream, state)
		if not o.nested then
			if stream:skip_to(o.block[2]) then
				stream:match(block_close)
				state.block = nil
			else
				stream:skip_to_end()
			end
			return "comment"
		end
		while not stream:eol() do
			if stream:match(block_open) then
				state.block += 1
			elseif stream:match(block_close) then
				state.block -= 1
				if state.block == 0 then
					state.block = nil
					return "comment"
				end
			else
				stream:next()
			end
		end
		return "comment"
	end

	-- A string from where the stream is: `close` ends it; `escapes` where `\` takes the next
	-- character. Where the line ends first, a string of many lines goes on in the state.
	local function string_body(stream, state, close, escapes, many, template)
		local pattern = literal(close)
		while not stream:eol() do
			if escapes and stream:eat("\\") then
				stream:next()
			elseif template and stream:match("%${", false) then
				if stream.pos == stream.start then
					stream:match("%${")
					state.str = nil
					state.tpl = state.tpl or {}
					state.tpl[#state.tpl + 1] = 0
					if state.jsx or state.jsxd then
						-- The template is in a tag of JSX: the tag waits while its `${…}` is read,
						-- so that the `{` and `}` in it are the template's and its words not the
						-- tag's.
						state.held = state.held or {}
						state.held[#state.tpl] = { jsx = state.jsx, jsxd = state.jsxd }
						state.jsx, state.jsxd = nil, nil
					end
					state.last = nil
					return "string.special"
				end
				state.str = { close = close, escapes = escapes, many = many, template = template }
				return "string"
			elseif stream:match(pattern) then
				state.str = nil
				state.last = "value"
				return "string"
			else
				stream:next()
			end
		end
		state.str = many and { close = close, escapes = escapes, many = many, template = template } or nil
		state.last = "value"
		return "string"
	end

	local function word_style(stream, state, word, after_dot)
		local def = state.def
		state.def = nil
		if keywords[word] then
			state.def = definers[word]
			state.last = REGEX_AFTER[word] and "keyword" or "value"
			return "keyword"
		end
		state.last = "value"
		if atoms[word] then
			return "atom"
		elseif special[word] then
			return "variable.special"
		elseif types[word] then
			return "type"
		elseif def then
			return def
		end
		if o.macros and stream:match("!", false) and not stream:match("!=", false) then
			return "function"
		end
		if stream:match("%s*%(", false) or (o.generics_call and stream:match("::<", false)) then
			return "function"
		end
		if after_dot then
			return "property"
		end
		if o.capitals then
			local first = string.byte(word, 1)
			if first >= 65 and first <= 90 then
				if #word > 1 and not string.find(word, "%l") then
					return "constant"
				end
				return "type"
			end
		end
		return nil
	end

	local function read(stream, state)
		if (state.block or 0) > 0 then
			return block_comment(stream, state)
		end
		local open = state.str
		if open then
			return string_body(stream, state, open.close, open.escapes, open.many, open.template)
		end
		if stream:eat_space() then
			return nil
		end
		local after_dot = state.dot
		state.dot = nil
		-- Inside a tag of JSX: its name's attributes, their strings, its end.
		if state.jsx and not state.jsxd then
			if stream:match("/>") or stream:match(">") then
				state.jsx = nil
				state.last = "value"
				return "tag"
			elseif stream:match("[%a_][%w_%-:]*") then
				return "attribute"
			elseif stream:eat('"') then
				return string_body(stream, state, '"', false, false, false)
			elseif stream:eat("'") then
				return string_body(stream, state, "'", false, false, false)
			elseif stream:eat("{") then
				state.jsxd = 0
				return "punctuation"
			end
			stream:next()
			return "operator"
		end
		for _, comment in line_comments do
			if stream:match(comment) then
				stream:skip_to_end()
				return "comment"
			end
		end
		if block_open and stream:match(block_open) then
			state.block = 1
			return block_comment(stream, state)
		end
		if o.preprocessor and first_on_line(stream) and stream:match("#%s*%a+") then
			state.last = nil
			return "meta"
		end
		if o.php_tags and (stream:match("<%?php") or stream:match("<%?=") or stream:match("%?>")) then
			return "meta"
		end
		if o.triple and stream:match('"""') then
			return string_body(stream, state, '"""', true, true, false)
		end
		if o.raw then
			if o.raw == "rust" then
				local hashes = stream:match('b?r(#*)"')
				if hashes then
					return string_body(stream, state, '"' .. hashes, false, true, false)
				end
			elseif o.raw == "csharp" and (stream:match('@"') or stream:match('%$@"') or stream:match('@%$"')) then
				return string_body(stream, state, '"', false, true, false)
			elseif o.raw == "cpp" then
				local delimiter = stream:match('u?8?R"([^%(%s]*)%(')
				if delimiter then
					return string_body(stream, state, ")" .. delimiter .. '"', false, true, false)
				end
			end
		end
		if
			o.lifetimes
			and stream:match("'[%a_][%w_]*", false)
			and not stream:match("'[%a_]'", false)
			and not stream:match("'\\", false)
		then
			stream:match("'[%a_][%w_]*")
			return "keyword.modifier"
		end
		local c = stream:peek()
		local quote = o.quotes and o.quotes[c]
		if quote then
			stream:next()
			if quote == "template" then
				return string_body(stream, state, c, true, true, true)
			elseif quote == "raw" then
				return string_body(stream, state, c, false, true, false)
			end
			return string_body(stream, state, c, true, false, false)
		end
		if o.variables and stream:match("%$[%a_][%w_]*") then
			state.last = "value"
			return "variable.special"
		end
		if o.annotations and stream:match("@[%a_][%w_%.]*") then
			return "meta"
		end
		if common.number(stream) then
			state.last = "value"
			return "number"
		end
		local word = stream:match(ident)
		if word then
			return word_style(stream, state, word, after_dot)
		end
		-- A tag of JSX opens where a value cannot be (a `<` after a value compares); `</` closes one
		-- wherever it is, after the words of an element as well.
		if o.jsx and c == "<" then
			local closing = stream:match("</", false) ~= nil
			if (closing or state.last ~= "value") and stream:match("</?[%a_][%w_%.%-:]*") then
				state.jsx = true
				state.jsxd = nil
				return "tag"
			end
			if (closing or state.last ~= "value") and stream:match("</?>") then
				state.last = nil
				return "tag"
			end
		end
		if o.regex and c == "/" and state.last ~= "value" and not stream:match("/[/%*]", false) then
			stream:next()
			local in_class = false
			while not stream:eol() do
				local next_char = stream:next()
				if next_char == "\\" then
					stream:next()
				elseif next_char == "[" then
					in_class = true
				elseif next_char == "]" then
					in_class = false
				elseif next_char == "/" and not in_class then
					stream:match("%a*")
					break
				end
			end
			state.last = "value"
			return "regexp"
		end
		if c == "{" then
			stream:next()
			if state.tpl then
				state.tpl[#state.tpl] += 1
			end
			if state.jsxd then
				state.jsxd += 1
			end
			state.last = nil
			return "punctuation"
		elseif c == "}" then
			stream:next()
			if state.jsxd then
				if state.jsxd == 0 then
					state.jsxd = nil
				else
					state.jsxd -= 1
				end
			elseif state.tpl then
				local depth = state.tpl[#state.tpl]
				if depth == 0 then
					local held = state.held and state.held[#state.tpl]
					if held then
						state.held[#state.tpl] = nil
						if next(state.held) == nil then
							state.held = nil
						end
						state.jsx, state.jsxd = held.jsx, held.jsxd
					end
					table.remove(state.tpl)
					if #state.tpl == 0 then
						state.tpl = nil
					end
					state.str = { close = "`", escapes = true, many = true, template = true }
					return "string.special"
				end
				state.tpl[#state.tpl] = depth - 1
			end
			state.last = "value"
			return "punctuation"
		elseif c == "." then
			stream:next()
			if stream:match("%.%.?") then
				state.last = nil
				return "operator"
			end
			state.dot = true
			return "punctuation"
		elseif c == ")" or c == "]" then
			stream:next()
			state.last = "value"
			return "punctuation"
		elseif c == "(" or c == "[" or c == ";" or c == "," then
			stream:next()
			state.last = nil
			return "punctuation"
		end
		local operator = stream:match("[%+%-%*/%%=&|<>!%?:~%^@#\\]+")
		if operator then
			-- A comment after an operator with no space between is a comment still.
			local comment = string.find(operator, "/[/%*]", 2)
			if comment then
				stream.pos = stream.start + comment - 1
			end
			if o.dot_operators and stream:current() == "::" then
				state.dot = true
			end
			state.last = nil
			return "operator"
		end
		stream:next()
		return nil
	end

	local function token(stream, state)
		local style = read(stream, state)
		if stream:eol() then
			-- What a token says of the next one alone does not go on to the next line.
			state.last, state.def, state.dot = nil, nil, nil
		end
		return style
	end

	return { start = start, token = token }
end

-- --- The languages -----------------------------------------------------------------------------

local JS_KEYWORDS = "break case catch class const continue debugger default delete do else export extends finally for "
	.. "from function if import in instanceof let new of return static super switch throw try typeof var void "
	.. "while with yield async await get set as"
local TS_KEYWORDS = JS_KEYWORDS
	.. " abstract declare enum implements interface keyof namespace private protected "
	.. "public readonly type infer is satisfies override module unique"
local JS_DEFINERS = { ["function"] = "function", class = "type", extends = "type", new = "type" }
local TS_DEFINERS = {
	["function"] = "function",
	class = "type",
	extends = "type",
	implements = "type",
	interface = "type",
	type = "type",
	enum = "type",
	new = "type",
	namespace = "type",
}
local JS_QUOTES = { ['"'] = true, ["'"] = true, ["`"] = "template" }

local function js(ts, jsx)
	return {
		keywords = set(ts and TS_KEYWORDS or JS_KEYWORDS),
		atoms = set("true false null undefined NaN Infinity"),
		special = set("this super arguments"),
		types = ts and set("string number boolean any unknown never void object symbol bigint") or {},
		definers = ts and TS_DEFINERS or JS_DEFINERS,
		line = { "//" },
		block = { "/*", "*/" },
		quotes = JS_QUOTES,
		regex = true,
		jsx = jsx,
		annotations = ts,
		capitals = ts,
		dollar = true,
	}
end

local C_KEYWORDS = "auto break case char const continue default do double else enum extern float for goto if inline "
	.. "int long register restrict return short signed sizeof static struct switch typedef union unsigned void "
	.. "volatile while _Bool _Complex bool"
local CPP_KEYWORDS = C_KEYWORDS
	.. " alignas alignof and asm catch class constexpr consteval constinit const_cast "
	.. "decltype delete dynamic_cast explicit export final friend mutable namespace new noexcept not operator or "
	.. "override private protected public reinterpret_cast static_assert static_cast template this thread_local "
	.. "throw try typeid typename using virtual co_await co_return co_yield concept requires"

-- What makes the reader of each language.
local READERS = {
	JavaScript = function()
		return js(false, false)
	end,
	JSX = function()
		return js(false, true)
	end,
	TypeScript = function()
		return js(true, false)
	end,
	TSX = function()
		return js(true, true)
	end,
	Rust = function()
		return {
			keywords = set(
				"as async await break const continue crate dyn else enum extern fn for if impl in let loop "
					.. "match mod move mut pub ref return static struct super trait type unsafe use where while "
					.. "macro_rules union"
			),
			atoms = set("true false None Some Ok Err"),
			special = set("self Self"),
			types = set(
				"i8 i16 i32 i64 i128 isize u8 u16 u32 u64 u128 usize f32 f64 bool char str String Vec Option "
					.. "Result Box"
			),
			definers = {
				fn = "function",
				struct = "type",
				enum = "type",
				trait = "type",
				type = "type",
				impl = "type",
				union = "type",
			},
			line = { "//" },
			block = { "/*", "*/" },
			nested = true,
			quotes = { ['"'] = true, ["'"] = true },
			raw = "rust",
			lifetimes = true,
			macros = true,
			capitals = true,
			dot_operators = true,
			generics_call = true,
			annotations = false,
			preprocessor = false,
		}
	end,
	Go = function()
		return {
			keywords = set(
				"break case chan const continue default defer else fallthrough for func go goto if import "
					.. "interface map package range return select struct switch type var"
			),
			atoms = set("true false nil iota"),
			types = set(
				"bool byte complex64 complex128 error float32 float64 int int8 int16 int32 int64 rune string "
					.. "uint uint8 uint16 uint32 uint64 uintptr any"
			),
			definers = { func = "function", type = "type" },
			line = { "//" },
			block = { "/*", "*/" },
			quotes = { ['"'] = true, ["'"] = true, ["`"] = "raw" },
		}
	end,
	C = function()
		return {
			keywords = set(C_KEYWORDS),
			atoms = set("true false NULL"),
			types = set("size_t ssize_t int8_t int16_t int32_t int64_t uint8_t uint16_t uint32_t uint64_t FILE"),
			definers = { struct = "type", enum = "type", union = "type" },
			line = { "//" },
			block = { "/*", "*/" },
			quotes = { ['"'] = true, ["'"] = true },
			preprocessor = true,
		}
	end,
	["C++"] = function()
		return {
			keywords = set(CPP_KEYWORDS),
			atoms = set("true false nullptr NULL"),
			special = set("this"),
			types = set(
				"size_t int8_t int16_t int32_t int64_t uint8_t uint16_t uint32_t uint64_t string vector map "
					.. "set unique_ptr shared_ptr optional"
			),
			definers = { class = "type", struct = "type", enum = "type", union = "type", namespace = "type" },
			line = { "//" },
			block = { "/*", "*/" },
			quotes = { ['"'] = true, ["'"] = true },
			raw = "cpp",
			preprocessor = true,
			dot_operators = true,
		}
	end,
	["C#"] = function()
		return {
			keywords = set(
				"abstract as base break case catch checked class const continue default delegate do else "
					.. "enum event explicit extern finally fixed for foreach goto if implicit in interface internal is "
					.. "lock namespace new operator out override params private protected public readonly record ref "
					.. "return sealed sizeof stackalloc static struct switch throw try typeof unchecked unsafe using "
					.. "virtual volatile while async await var get set init where yield nameof when"
			),
			atoms = set("true false null"),
			special = set("this base"),
			types = set(
				"bool byte char decimal double float int long object sbyte short string uint ulong ushort void dynamic"
			),
			definers = {
				class = "type",
				struct = "type",
				interface = "type",
				enum = "type",
				record = "type",
				namespace = "type",
			},
			line = { "//" },
			block = { "/*", "*/" },
			quotes = { ['"'] = true, ["'"] = true },
			triple = true,
			raw = "csharp",
			preprocessor = true,
			capitals = true,
		}
	end,
	Java = function()
		return {
			keywords = set(
				"abstract assert break case catch class const continue default do else enum extends final "
					.. "finally for goto if implements import instanceof interface native new package private "
					.. "protected public return static strictfp super switch synchronized throw throws transient try "
					.. "volatile while var record sealed permits yield non-sealed"
			),
			atoms = set("true false null"),
			special = set("this super"),
			types = set("boolean byte char double float int long short void String Object"),
			definers = {
				class = "type",
				interface = "type",
				enum = "type",
				extends = "type",
				implements = "type",
				record = "type",
				new = "type",
			},
			line = { "//" },
			block = { "/*", "*/" },
			quotes = { ['"'] = true, ["'"] = true },
			triple = true,
			annotations = true,
			capitals = true,
		}
	end,
	Kotlin = function()
		return {
			keywords = set(
				"as break class continue do else for fun if in interface is object package return super throw "
					.. "try typealias typeof val var when while by catch constructor delegate dynamic field file "
					.. "finally get import init param property receiver set setparam where actual abstract "
					.. "annotation companion const crossinline data enum expect external final infix inline inner "
					.. "internal lateinit noinline open operator out override private protected public reified "
					.. "sealed suspend tailrec vararg value"
			),
			atoms = set("true false null"),
			special = set("this super it"),
			definers = { fun = "function", class = "type", interface = "type", object = "type", typealias = "type" },
			line = { "//" },
			block = { "/*", "*/" },
			nested = true,
			quotes = { ['"'] = true, ["'"] = true },
			triple = true,
			annotations = true,
			capitals = true,
		}
	end,
	Swift = function()
		return {
			keywords = set(
				"associatedtype class deinit enum extension fileprivate func import init inout internal let "
					.. "open operator private precedencegroup protocol public rethrows static struct subscript "
					.. "typealias var break case catch continue default defer do else fallthrough for guard if in "
					.. "repeat return throw switch where while as is try throws async await actor some any final "
					.. "lazy mutating nonmutating optional override required weak unowned convenience dynamic indirect"
			),
			atoms = set("true false nil"),
			special = set("self Self super"),
			definers = {
				func = "function",
				class = "type",
				struct = "type",
				enum = "type",
				protocol = "type",
				extension = "type",
				actor = "type",
				typealias = "type",
			},
			line = { "//" },
			block = { "/*", "*/" },
			nested = true,
			quotes = { ['"'] = true },
			triple = true,
			annotations = true,
			capitals = true,
		}
	end,
	PHP = function()
		return {
			keywords = set(
				"abstract and array as break callable case catch class clone const continue declare default "
					.. "do echo else elseif empty enddeclare endfor endforeach endif endswitch endwhile enum extends "
					.. "final finally fn for foreach function global goto if implements include include_once "
					.. "instanceof insteadof interface isset list match namespace new or print private protected "
					.. "public readonly require require_once return static switch throw trait try unset use var "
					.. "while xor yield"
			),
			atoms = set("true false null TRUE FALSE NULL"),
			special = set("this self parent"),
			definers = {
				["function"] = "function",
				class = "type",
				interface = "type",
				trait = "type",
				enum = "type",
				new = "type",
			},
			line = { "//", "#" },
			block = { "/*", "*/" },
			quotes = { ['"'] = true, ["'"] = true },
			variables = true,
			php_tags = true,
			dot_operators = true,
		}
	end,
}

-- The comment, indent and brackets of the languages of braces: a line break after an opener of
-- `pairs` indents, and a closer typed first on a line goes back out.
local COMMENT = { line = "//", block = { "/*", "*/" } }
local INDENT = { decrease = "^\\s*[\\}\\]\\)]" }

-- How the languages whose one formatter fixes their indent indent (gofmt's tabs, rustfmt's four
-- spaces); the rest leave it to the page.
local TABS = {
	Go = { hard_tabs = true },
	Rust = { hard_tabs = false, tab_width = 4 },
}

local clike = {}

-- The language named `name` of the family: its reader, and what an editor is told of it.
function clike.make(name)
	local o = READERS[name]()
	local pairs_of = "()[]{}''\"\""
	if name == "Rust" then
		-- A lifetime is a lone quote.
		pairs_of = '()[]{}""'
	elseif o.quotes and o.quotes["`"] then
		pairs_of ..= "``"
	end
	local config = { comment = COMMENT, indent = INDENT, folds = "indent", pairs = pairs_of }
	for key, value in TABS[name] or {} do
		config[key] = value
	end
	return { reader = reader(o), config = config }
end

return clike
